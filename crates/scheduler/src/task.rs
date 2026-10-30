//! The [`Task`] structure: the smallest interruptible/resumable unit of work.
//!
//! A task owns a kind, a VM entry point, an optional continuation, an effect
//! set, a fuel budget, and a lifecycle state. The scheduler never executes a
//! task by itself; it only tracks and validates the state machine around the
//! embedder (VM, JIT, or host callback) that actually runs the task.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use crate::id::{ContinuationId, EntryPoint, TaskId, TasksId};

/// Execution class of a task.
///
/// The class decides what is legal when the scheduler has to interrupt,
/// migrate, or re-run a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskKind {
    /// No externally visible effects and inputs are exclusive: may restart and
    /// may be stolen at task granularity.
    Pure,
    /// Performs I/O, FFI, shared writes, or other observable effects. It may
    /// suspend at an effect boundary but must never be re-run from its entry.
    Effect,
    /// Buffers effects and can be rolled back: may restart on conflict or at a
    /// commit point.
    Transactional,
}

impl TaskKind {
    /// Returns the stable name used in logs and timelines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Effect => "effect",
            Self::Transactional => "transactional",
        }
    }

    /// Whether the task may be discarded and restarted from its entry point.
    #[must_use]
    pub const fn is_restartable(self) -> bool {
        matches!(self, Self::Pure)
    }
}

/// Lifecycle state of one task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskState {
    /// Constructed but not yet submitted to a scheduling package.
    Created,
    /// Submitted and waiting for a runner unit to start it.
    Ready,
    /// Currently executing on a runner unit.
    Running,
    /// Suspended at a safepoint; a continuation describes how to resume it.
    Suspended,
    /// Ran to completion successfully.
    Completed,
    /// Cancelled before completion; no further execution is allowed.
    Cancelled,
    /// Terminated with an exception or host error.
    Failed,
}

impl TaskState {
    /// Returns the stable name used in logs and timelines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Suspended => "suspended",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    /// Whether this is a terminal state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

/// An effect domain identifier.
///
/// Domains are opaque to the scheduler: the packager decides how to map object
/// graphs, modules, sockets, files, and agents onto domain ids. Tasks that
/// share a domain must not run concurrently unless the domain is proven
/// conflict-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectDomain(u32);

impl EffectDomain {
    /// Creates a domain identifier from its raw representation.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for EffectDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EffectDomain#{}", self.0)
    }
}

/// The set of effect domains touched by a task.
///
/// Uses a sorted set so iteration and debug output are deterministic across
/// runs. The set is metadata, not a lock: the scheduler uses it to construct
/// dependency or serialization barriers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectSet {
    domains: BTreeSet<EffectDomain>,
}

impl EffectSet {
    /// Creates an empty effect set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a set from an iterator of domains.
    #[must_use]
    pub fn from_domains(domains: impl IntoIterator<Item = EffectDomain>) -> Self {
        Self {
            domains: domains.into_iter().collect(),
        }
    }

    /// Inserts a domain; returns whether it was newly inserted.
    pub fn insert(&mut self, domain: EffectDomain) -> bool {
        self.domains.insert(domain)
    }

    /// Returns whether the set contains `domain`.
    #[must_use]
    pub fn contains(&self, domain: EffectDomain) -> bool {
        self.domains.contains(&domain)
    }

    /// Number of domains in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.domains.len()
    }

    /// Whether the set is empty (that is, the task has no recorded effects).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }

    /// Iterates domains in ascending order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = EffectDomain> + '_ {
        self.domains.iter().copied()
    }

    /// Adds every domain from `other` into this set.
    pub fn union_with(&mut self, other: &Self) {
        self.domains.extend(other.domains.iter().copied());
    }
}

/// Per-task fuel budget.
///
/// Fuel is a deterministic instruction/step allowance, not wall time. Running
/// out of fuel is a normal scheduler signal used to reach a safepoint; it is
/// reported as [`BudgetExhausted`] so the caller can preempt or fail the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskBudget {
    limit: u64,
    used: u64,
}

impl TaskBudget {
    /// Creates a budget with the given fuel limit.
    #[must_use]
    pub const fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    /// The configured fuel limit.
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }

    /// Fuel consumed so far.
    #[must_use]
    pub const fn used(self) -> u64 {
        self.used
    }

    /// Fuel left before the budget is exhausted.
    #[must_use]
    pub const fn remaining(self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    /// Whether all fuel has been consumed.
    #[must_use]
    pub const fn is_exhausted(self) -> bool {
        self.used >= self.limit
    }

    /// Charges `amount` fuel.
    ///
    /// # Errors
    ///
    /// Returns [`BudgetExhausted`] if the charge would exceed the limit or
    /// overflow `u64`. The budget is left unchanged in that case.
    pub fn charge(&mut self, amount: u64) -> Result<(), BudgetExhausted> {
        let requested = amount;
        let Some(used) = self.used.checked_add(amount) else {
            return Err(BudgetExhausted {
                requested,
                remaining: self.remaining(),
            });
        };
        if used > self.limit {
            return Err(BudgetExhausted {
                requested,
                remaining: self.remaining(),
            });
        }
        self.used = used;
        Ok(())
    }
}

/// Error returned when a fuel charge would exhaust the task budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetExhausted {
    requested: u64,
    remaining: u64,
}

impl BudgetExhausted {
    /// Fuel the caller tried to charge.
    #[must_use]
    pub const fn requested(self) -> u64 {
        self.requested
    }

    /// Fuel remaining before the failed charge.
    #[must_use]
    pub const fn remaining(self) -> u64 {
        self.remaining
    }
}

impl fmt::Display for BudgetExhausted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "fuel budget exhausted: requested {}, only {} remaining",
            self.requested, self.remaining
        )
    }
}

impl Error for BudgetExhausted {}

/// Host-visible failure attached to a failed task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFailure {
    message: String,
}

impl TaskFailure {
    /// Creates a failure from a human-readable message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The failure message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Why a task was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CancelReason {
    /// The embedder requested cancellation (for example a host shutdown).
    HostRequested,
    /// An `AbortSignal` aborted the operation.
    AbortSignal,
    /// The owning agent is terminating.
    AgentTerminated,
}

impl CancelReason {
    /// Returns the stable name used in logs and timelines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HostRequested => "host-requested",
            Self::AbortSignal => "abort-signal",
            Self::AgentTerminated => "agent-terminated",
        }
    }
}

/// Terminal payload of a task.
///
/// `Completed` intentionally carries no value yet: the VM embedding will add
/// a value/`Ref` payload, while this crate stays independent of the object
/// model. Failed and cancelled outcomes are already fully described.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    /// The task finished successfully.
    Completed,
    /// The task threw or returned a host error.
    Failed(TaskFailure),
    /// The task was cancelled.
    Cancelled(CancelReason),
}

/// The state-machine action that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskAction {
    /// `Created -> Ready`.
    MakeReady,
    /// `Ready -> Running`.
    Start,
    /// `Running -> Suspended`.
    Suspend,
    /// `Suspended -> Ready`.
    Resume,
    /// `Running -> Completed`.
    Complete,
    /// `Running -> Failed`.
    Fail,
    /// Any non-terminal state -> Cancelled.
    Cancel,
    /// `Suspended | Failed -> Ready` for restartable tasks.
    Restart,
}

impl TaskAction {
    /// Returns the stable action name used in errors.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MakeReady => "make-ready",
            Self::Start => "start",
            Self::Suspend => "suspend",
            Self::Resume => "resume",
            Self::Complete => "complete",
            Self::Fail => "fail",
            Self::Cancel => "cancel",
            Self::Restart => "restart",
        }
    }
}

/// Error returned for an illegal task state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTransitionError {
    task: TaskId,
    from: TaskState,
    action: TaskAction,
    reason: &'static str,
}

impl TaskTransitionError {
    fn new(task: TaskId, from: TaskState, action: TaskAction, reason: &'static str) -> Self {
        Self {
            task,
            from,
            action,
            reason,
        }
    }

    /// The task whose state machine rejected the action.
    #[must_use]
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// The state the task was in when the action was attempted.
    #[must_use]
    pub const fn from(&self) -> TaskState {
        self.from
    }

    /// The attempted action.
    #[must_use]
    pub const fn action(&self) -> TaskAction {
        self.action
    }

    /// Human-readable reason.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for TaskTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "task {} cannot {} from {}: {}",
            self.task,
            self.action.name(),
            self.from.name(),
            self.reason
        )
    }
}

impl Error for TaskTransitionError {}

/// The smallest interruptible/resumable unit of work.
///
/// A task is data only. Execution is performed by the embedder; this type
/// validates and records the lifecycle so that the scheduler, timeline, and
/// tests share one definition of legal transitions.
#[derive(Debug)]
pub struct Task {
    id: TaskId,
    kind: TaskKind,
    state: TaskState,
    entry: EntryPoint,
    continuation: Option<ContinuationId>,
    effects: EffectSet,
    budget: TaskBudget,
    parent: Option<TasksId>,
    outcome: Option<TaskOutcome>,
}

impl Task {
    /// Creates a task in [`TaskState::Created`] with an empty effect set and a
    /// zero-fuel budget.
    #[must_use]
    pub fn new(id: TaskId, kind: TaskKind, entry: EntryPoint) -> Self {
        Self {
            id,
            kind,
            state: TaskState::Created,
            entry,
            continuation: None,
            effects: EffectSet::new(),
            budget: TaskBudget::new(0),
            parent: None,
            outcome: None,
        }
    }

    /// Replaces the task budget.
    #[must_use]
    pub fn with_budget(mut self, budget: TaskBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Adds one effect domain.
    #[must_use]
    pub fn with_effect(mut self, domain: EffectDomain) -> Self {
        self.effects.insert(domain);
        self
    }

    /// Replaces the effect set.
    #[must_use]
    pub fn with_effects(mut self, effects: EffectSet) -> Self {
        self.effects = effects;
        self
    }

    /// The task identifier.
    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    /// The task class.
    #[must_use]
    pub const fn kind(&self) -> TaskKind {
        self.kind
    }

    /// The current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> TaskState {
        self.state
    }

    /// The VM entry point used when the task starts or restarts.
    #[must_use]
    pub const fn entry(&self) -> EntryPoint {
        self.entry
    }

    /// The saved continuation, if the task is suspended.
    #[must_use]
    pub const fn continuation(&self) -> Option<ContinuationId> {
        self.continuation
    }

    /// Takes the saved continuation out of the task.
    ///
    /// The dispatcher calls this when it resumes a suspended task. Taking the
    /// continuation is not a state transition; call [`Self::resume`] first so
    /// the task is `Ready` again.
    pub fn take_continuation(&mut self) -> Option<ContinuationId> {
        self.continuation.take()
    }

    /// The task's recorded effect set.
    #[must_use]
    pub const fn effects(&self) -> &EffectSet {
        &self.effects
    }

    /// The task's fuel budget.
    #[must_use]
    pub const fn budget(&self) -> TaskBudget {
        self.budget
    }

    /// Charges fuel against the task budget.
    ///
    /// # Errors
    ///
    /// Returns [`BudgetExhausted`] when the budget cannot cover the charge.
    pub fn charge_fuel(&mut self, amount: u64) -> Result<(), BudgetExhausted> {
        self.budget.charge(amount)
    }

    /// The scheduling package that owns this task, if it has been submitted.
    #[must_use]
    pub const fn parent_tasks(&self) -> Option<TasksId> {
        self.parent
    }

    /// The terminal payload, if the task is terminal.
    #[must_use]
    pub const fn outcome(&self) -> Option<&TaskOutcome> {
        self.outcome.as_ref()
    }

    /// Whether the task reached a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// `Created -> Ready`.
    ///
    /// # Errors
    ///
    /// Returns [`TaskTransitionError`] if the task is not `Created`.
    pub fn make_ready(&mut self) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Created,
            TaskState::Ready,
            TaskAction::MakeReady,
            "make-ready requires a newly created task",
        )
    }

    /// `Ready -> Running`.
    ///
    /// # Errors
    ///
    /// Returns [`TaskTransitionError`] if the task is not `Ready`.
    pub fn start(&mut self) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Ready,
            TaskState::Running,
            TaskAction::Start,
            "start requires a ready task",
        )
    }

    /// `Running -> Suspended`, recording the continuation.
    ///
    /// # Errors
    ///
    /// Returns [`TaskTransitionError`] if the task is not `Running`.
    pub fn suspend(&mut self, continuation: ContinuationId) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Running,
            TaskState::Suspended,
            TaskAction::Suspend,
            "suspend requires a running task",
        )?;
        self.continuation = Some(continuation);
        Ok(())
    }

    /// `Suspended -> Ready`.
    ///
    /// # Errors
    ///
    /// Returns [`TaskTransitionError`] if the task is not `Suspended`.
    pub fn resume(&mut self) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Suspended,
            TaskState::Ready,
            TaskAction::Resume,
            "resume requires a suspended task",
        )
    }

    /// `Running -> Completed`.
    ///
    /// # Errors
    ///
    /// Returns [`TaskTransitionError`] if the task is not `Running`.
    pub fn complete(&mut self) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Running,
            TaskState::Completed,
            TaskAction::Complete,
            "complete requires a running task",
        )?;
        self.outcome = Some(TaskOutcome::Completed);
        Ok(())
    }

    /// `Running -> Failed`.
    ///
    /// # Errors
    ///
    /// Returns [`TaskTransitionError`] if the task is not `Running`.
    pub fn fail(&mut self, message: impl Into<String>) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Running,
            TaskState::Failed,
            TaskAction::Fail,
            "fail requires a running task",
        )?;
        self.outcome = Some(TaskOutcome::Failed(TaskFailure::new(message)));
        Ok(())
    }

    /// Cancels a non-terminal task.
    ///
    /// Cancellation is cooperative: the task is marked cancelled here, but a
    /// running host callback must reach a safepoint before it actually stops.
    ///
    /// # Errors
    ///
    /// Returns [`TaskTransitionError`] if the task is already terminal.
    pub fn cancel(&mut self, reason: CancelReason) -> Result<(), TaskTransitionError> {
        if self.state.is_terminal() {
            return Err(TaskTransitionError::new(
                self.id,
                self.state,
                TaskAction::Cancel,
                "task is already terminal",
            ));
        }
        self.state = TaskState::Cancelled;
        self.outcome = Some(TaskOutcome::Cancelled(reason));
        Ok(())
    }

    /// Restarts a suspendable task from its entry point.
    ///
    /// Only [`TaskKind::Pure`] tasks may restart; effectful work must resume
    /// from its continuation instead. A restart clears the old continuation and
    /// outcome and puts the task back into [`TaskState::Ready`].
    ///
    /// # Errors
    ///
    /// Returns [`TaskTransitionError`] if the task is not pure, or is not
    /// `Suspended`/`Failed`.
    pub fn restart(&mut self) -> Result<(), TaskTransitionError> {
        if !self.kind.is_restartable() {
            return Err(TaskTransitionError::new(
                self.id,
                self.state,
                TaskAction::Restart,
                "only pure tasks can restart",
            ));
        }
        if !matches!(self.state, TaskState::Suspended | TaskState::Failed) {
            return Err(TaskTransitionError::new(
                self.id,
                self.state,
                TaskAction::Restart,
                "restart requires a suspended or failed task",
            ));
        }
        self.state = TaskState::Ready;
        self.continuation = None;
        self.outcome = None;
        Ok(())
    }

    pub(crate) fn set_parent(&mut self, parent: TasksId) {
        self.parent = Some(parent);
    }

    fn expect_state(
        &mut self,
        expected: TaskState,
        next: TaskState,
        action: TaskAction,
        reason: &'static str,
    ) -> Result<(), TaskTransitionError> {
        if self.state != expected {
            return Err(TaskTransitionError::new(
                self.id, self.state, action, reason,
            ));
        }
        self.state = next;
        Ok(())
    }
}
