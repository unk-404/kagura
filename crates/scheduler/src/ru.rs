//! Runner unit and elastic pool skeleton. No OS thread, VM, or stack is created
//! here; state is CAS-updated (`AcqRel`/`Acquire`) and counters are `Relaxed`.

use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crate::deque::{Stealer, WorkerDeque};
use crate::id::RuId;
use crate::injector::Injector;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuState {
    Idle,
    Running,
    Parked,
    Retiring,
}

impl RuState {
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

    #[must_use]
    pub const fn ru(self) -> RuId {
        self.ru
    }

    #[must_use]
    pub const fn from(self) -> RuState {
        self.from
    }

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuStats {
    pub items_processed: u64,
    pub local_pushes: u64,
    pub local_pops: u64,
    pub steals_attempted: u64,
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

#[derive(Debug)]
pub struct Ru<T: Send> {
    id: RuId,
    state: AtomicU8,
    local: WorkerDeque<T>,
    counters: RuCounters,
}

impl<T: Send> Ru<T> {
    #[must_use]
    pub fn new(id: RuId) -> Self {
        Self {
            id,
            state: AtomicU8::new(RuState::Idle.as_u8()),
            local: WorkerDeque::new(),
            counters: RuCounters::default(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> RuId {
        self.id
    }

    #[must_use]
    pub fn state(&self) -> RuState {
        RuState::from_u8(self.state.load(Ordering::Acquire)).unwrap_or(RuState::Idle)
    }

    pub fn begin_running(&self) -> Result<(), RuStateError> {
        self.try_transition(RuState::Idle, RuState::Running)
    }

    pub fn finish_running(&self) -> Result<(), RuStateError> {
        self.try_transition(RuState::Running, RuState::Idle)
    }

    pub fn park(&self) -> Result<(), RuStateError> {
        self.try_transition(RuState::Idle, RuState::Parked)
    }

    pub fn unpark(&self) -> Result<(), RuStateError> {
        self.try_transition(RuState::Parked, RuState::Idle)
    }

    pub fn retire(&self) -> Result<(), RuStateError> {
        let current = self.state();
        if matches!(current, RuState::Idle | RuState::Parked) {
            self.try_transition(current, RuState::Retiring)
        } else {
            Err(RuStateError::new(self.id, current, RuState::Retiring))
        }
    }

    pub fn push_local(&self, item: T) {
        self.local.push(item);
        self.counters.local_pushes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn pop_local(&self) -> Option<T> {
        let item = self.local.pop();
        if item.is_some() {
            self.counters.local_pops.fetch_add(1, Ordering::Relaxed);
        }
        item
    }

    #[must_use]
    pub fn stealer(&self) -> Stealer<T> {
        self.local.stealer()
    }

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

    pub fn record_processed(&self) {
        self.counters
            .items_processed
            .fetch_add(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn local_len(&self) -> usize {
        self.local.len()
    }

    #[must_use]
    pub fn is_local_empty(&self) -> bool {
        self.local.is_empty()
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuSpawnError {
    limit: usize,
}

impl RuSpawnError {
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

#[derive(Debug)]
pub struct RuPool<T: Send> {
    injector: Arc<Injector<T>>,
    rus: Vec<Arc<Ru<T>>>,
    max_rus: usize,
}

impl<T: Send> RuPool<T> {
    #[must_use]
    pub fn new(max_rus: usize) -> Self {
        Self {
            injector: Arc::new(Injector::new()),
            rus: Vec::new(),
            max_rus,
        }
    }

    #[must_use]
    pub fn injector(&self) -> &Arc<Injector<T>> {
        &self.injector
    }

    #[must_use]
    pub fn rus(&self) -> &[Arc<Ru<T>>] {
        &self.rus
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rus.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rus.is_empty()
    }

    #[must_use]
    pub const fn max_rus(&self) -> usize {
        self.max_rus
    }

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
