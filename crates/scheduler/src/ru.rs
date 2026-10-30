//! Runner units (RUs) and the elastic RU-pool skeleton.
//!
//! An RU is the execution vehicle for [`Tasks`](crate::Tasks) packages. In M5
//! it normally maps to one OS thread with a private stack, TLAB, and local
//! work-stealing deque; the object heap is shared according to agent semantics.
//! This module provides the queue/state/stats skeleton only. It does not spawn
//! threads, execute JS, or manage stacks yet.
//!
//! The lifecycle state is a single atomic byte updated with compare-and-swap
//! (`AcqRel` on success, `Acquire` on failure) so state changes are visible
//! across RU threads. Scheduling counters are observation-only snapshots and
//! use `Relaxed` loads/stores; they must not be used for correctness decisions.

use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crate::deque::{Stealer, WorkerDeque};
use crate::id::RuId;
use crate::injector::Injector;

/// Lifecycle state of a runner unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuState {
    /// Available to pick up work.
    Idle,
    /// Executing a scheduling package.
    Running,
    /// Parked by the scheduler, for example until an injected job arrives.
    Parked,
    /// Shutting down; no new work is accepted.
    Retiring,
}

impl RuState {
    /// Returns the stable name used in logs and timelines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Parked => "parked",
            Self::Retiring => "retiring",
        }
    }

    const fn as_u8(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Running => 1,
            Self::Parked => 2,
            Self::Retiring => 3,
        }
    }

    const fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Idle),
            1 => Some(Self::Running),
            2 => Some(Self::Parked),
            3 => Some(Self::Retiring),
            _ => None,
        }
    }
}

/// Error returned for an illegal RU state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuStateError {
    ru: RuId,
    from: RuState,
    to: RuState,
}

impl RuStateError {
    const fn new(ru: RuId, from: RuState, to: RuState) -> Self {
        Self { ru, from, to }
    }

    /// The runner unit that rejected the transition.
    #[must_use]
    pub const fn ru(self) -> RuId {
        self.ru
    }

    /// The state the runner unit was in.
    #[must_use]
    pub const fn from(self) -> RuState {
        self.from
    }

    /// The state the caller tried to enter.
    #[must_use]
    pub const fn to(self) -> RuState {
        self.to
    }
}

impl fmt::Display for RuStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "runner unit {} cannot transition from {} to {}",
            self.ru,
            self.from.name(),
            self.to.name()
        )
    }
}

impl Error for RuStateError {}

/// Snapshot of a runner unit's scheduling counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuStats {
    /// Items this RU finished executing.
    pub items_processed: u64,
    /// Items pushed onto this RU's local deque.
    pub local_pushes: u64,
    /// Items popped from this RU's local deque.
    pub local_pops: u64,
    /// Steal attempts made by this RU.
    pub steals_attempted: u64,
    /// Successful steals made by this RU.
    pub steals_succeeded: u64,
}

#[derive(Debug, Default)]
struct RuCounters {
    items_processed: AtomicU64,
    local_pushes: AtomicU64,
    local_pops: AtomicU64,
    steals_attempted: AtomicU64,
    steals_succeeded: AtomicU64,
}

impl RuCounters {
    fn snapshot(&self) -> RuStats {
        // 计数快照允许在并发更新中短暂不一致；它只用于观测，不参与调度正确性。
        RuStats {
            items_processed: self.items_processed.load(Ordering::Relaxed),
            local_pushes: self.local_pushes.load(Ordering::Relaxed),
            local_pops: self.local_pops.load(Ordering::Relaxed),
            steals_attempted: self.steals_attempted.load(Ordering::Relaxed),
            steals_succeeded: self.steals_succeeded.load(Ordering::Relaxed),
        }
    }
}

/// A runner unit: private local deque, lifecycle state, and counters.
///
/// `T` is the scheduling unit. The M5 runtime instantiates it with
/// [`Tasks`](crate::Tasks); tests use lightweight integers.
#[derive(Debug)]
pub struct Ru<T: Send> {
    id: RuId,
    state: AtomicU8,
    local: WorkerDeque<T>,
    counters: RuCounters,
}

impl<T: Send> Ru<T> {
    /// Creates an idle runner unit.
    #[must_use]
    pub fn new(id: RuId) -> Self {
        Self {
            id,
            state: AtomicU8::new(RuState::Idle.as_u8()),
            local: WorkerDeque::new(),
            counters: RuCounters::default(),
        }
    }

    /// The runner-unit identifier.
    #[must_use]
    pub const fn id(&self) -> RuId {
        self.id
    }

    /// The current lifecycle state.
    #[must_use]
    pub fn state(&self) -> RuState {
        RuState::from_u8(self.state.load(Ordering::Acquire)).unwrap_or(RuState::Idle)
    }

    /// `Idle -> Running`.
    ///
    /// # Errors
    ///
    /// Returns [`RuStateError`] unless the RU is `Idle`.
    pub fn begin_running(&self) -> Result<(), RuStateError> {
        self.try_transition(RuState::Idle, RuState::Running)
    }

    /// `Running -> Idle`.
    ///
    /// # Errors
    ///
    /// Returns [`RuStateError`] unless the RU is `Running`.
    pub fn finish_running(&self) -> Result<(), RuStateError> {
        self.try_transition(RuState::Running, RuState::Idle)
    }

    /// `Idle -> Parked`.
    ///
    /// # Errors
    ///
    /// Returns [`RuStateError`] unless the RU is `Idle`.
    pub fn park(&self) -> Result<(), RuStateError> {
        self.try_transition(RuState::Idle, RuState::Parked)
    }

    /// `Parked -> Idle`.
    ///
    /// # Errors
    ///
    /// Returns [`RuStateError`] unless the RU is `Parked`.
    pub fn unpark(&self) -> Result<(), RuStateError> {
        self.try_transition(RuState::Parked, RuState::Idle)
    }

    /// Enters [`RuState::Retiring`] from `Idle` or `Parked`.
    ///
    /// # Errors
    ///
    /// Returns [`RuStateError`] if the RU is running or already retiring.
    pub fn retire(&self) -> Result<(), RuStateError> {
        let current = self.state();
        if matches!(current, RuState::Idle | RuState::Parked) {
            self.try_transition(current, RuState::Retiring)
        } else {
            Err(RuStateError::new(self.id, current, RuState::Retiring))
        }
    }

    /// Pushes an item onto the local deque.
    pub fn push_local(&self, item: T) {
        self.local.push(item);
        self.counters.local_pushes.fetch_add(1, Ordering::Relaxed);
    }

    /// Pops the newest local item.
    pub fn pop_local(&self) -> Option<T> {
        let item = self.local.pop();
        if item.is_some() {
            self.counters.local_pops.fetch_add(1, Ordering::Relaxed);
        }
        item
    }

    /// Creates a stealer handle for this RU's local deque.
    #[must_use]
    pub fn stealer(&self) -> Stealer<T> {
        self.local.stealer()
    }

    /// Tries to steal the oldest item from `victim`.
    pub fn try_steal_from(&self, victim: &Self) -> Option<T> {
        self.counters
            .steals_attempted
            .fetch_add(1, Ordering::Relaxed);
        let item = victim.local.stealer().steal();
        if item.is_some() {
            self.counters
                .steals_succeeded
                .fetch_add(1, Ordering::Relaxed);
        }
        item
    }

    /// Records that this RU finished executing one item.
    pub fn record_processed(&self) {
        self.counters
            .items_processed
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Number of items in the local deque.
    #[must_use]
    pub fn local_len(&self) -> usize {
        self.local.len()
    }

    /// Whether the local deque is empty.
    #[must_use]
    pub fn is_local_empty(&self) -> bool {
        self.local.is_empty()
    }

    /// Copies the current counter snapshot.
    #[must_use]
    pub fn stats(&self) -> RuStats {
        self.counters.snapshot()
    }

    fn try_transition(&self, from: RuState, to: RuState) -> Result<(), RuStateError> {
        // 状态转换必须 CAS：成功用 AcqRel 发布新状态，失败用 Acquire 读回实际值供诊断。
        match self.state.compare_exchange(
            from.as_u8(),
            to.as_u8(),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(actual) => Err(RuStateError::new(
                self.id,
                RuState::from_u8(actual).unwrap_or(RuState::Idle),
                to,
            )),
        }
    }
}

/// Error returned when the RU pool cannot create another runner unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuSpawnError {
    limit: usize,
}

impl RuSpawnError {
    /// The configured pool limit that blocked the spawn.
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl fmt::Display for RuSpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot spawn runner unit: pool limit {} reached",
            self.limit
        )
    }
}

impl Error for RuSpawnError {}

/// An elastic pool of runner units backed by one shared injection queue.
///
/// Spawning is fallible on purpose: the embedder passes a hard limit, and a
/// failed spawn degrades to using the existing RUs instead of aborting.
#[derive(Debug)]
pub struct RuPool<T: Send> {
    injector: Arc<Injector<T>>,
    rus: Vec<Arc<Ru<T>>>,
    max_rus: usize,
}

impl<T: Send> RuPool<T> {
    /// Creates a pool with at most `max_rus` runner units and an empty shared
    /// injector.
    #[must_use]
    pub fn new(max_rus: usize) -> Self {
        Self {
            injector: Arc::new(Injector::new()),
            rus: Vec::new(),
            max_rus,
        }
    }

    /// The shared thread-safe injection queue.
    #[must_use]
    pub fn injector(&self) -> &Arc<Injector<T>> {
        &self.injector
    }

    /// The runner units created so far.
    #[must_use]
    pub fn rus(&self) -> &[Arc<Ru<T>>] {
        &self.rus
    }

    /// Number of runner units in the pool.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rus.len()
    }

    /// Whether the pool contains no runner units.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rus.is_empty()
    }

    /// The configured maximum number of runner units.
    #[must_use]
    pub const fn max_rus(&self) -> usize {
        self.max_rus
    }

    /// Creates a new idle runner unit, or fails once the pool limit is reached.
    ///
    /// # Errors
    ///
    /// Returns [`RuSpawnError`] when the pool already holds `max_rus` units.
    pub fn try_spawn_ru(&mut self) -> Result<Arc<Ru<T>>, RuSpawnError> {
        if self.rus.len() >= self.max_rus {
            return Err(RuSpawnError {
                limit: self.max_rus,
            });
        }
        let id = RuId::new(self.rus.len() as u32);
        let ru = Arc::new(Ru::new(id));
        self.rus.push(Arc::clone(&ru));
        Ok(ru)
    }
}
