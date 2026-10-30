//! Task lifecycle data. The scheduler validates state; the embedder executes.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use crate::id::{ContinuationId, EntryPoint, TaskId, TasksId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskKind {
    Pure,
    /// Performs I/O, FFI, shared writes, or other observable effects. It may
    /// suspend at an effect boundary but must never be re-run from its entry.
    Effect,
    Transactional,
}

impl TaskKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Effect => "effect",
            Self::Transactional => "transactional",
        }
    }

    #[must_use]
    pub const fn is_restartable(self) -> bool {
        matches!(self, Self::Pure)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskState {
    Created,
    Ready,
    Running,
    /// Suspended at a safepoint; a continuation describes how to resume it.
    Suspended,
    Completed,
    Cancelled,
    Failed,
}

impl TaskState {
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

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

/// Opaque effect domain. Sharing a domain is a serialization barrier unless the
/// packager proves conflict freedom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectDomain(u32);

impl EffectDomain {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

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

/// Effect domains touched by a task. Sorted for deterministic iteration; this
/// is dependency metadata, not a lock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectSet {
    domains: BTreeSet<EffectDomain>,
}

impl EffectSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_domains(domains: impl IntoIterator<Item = EffectDomain>) -> Self {
        Self {
            domains: domains.into_iter().collect(),
        }
    }

    pub fn insert(&mut self, domain: EffectDomain) -> bool {
        self.domains.insert(domain)
    }

    #[must_use]
    pub fn contains(&self, domain: EffectDomain) -> bool {
        self.domains.contains(&domain)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.domains.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }

    /// Iterates domains in ascending order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = EffectDomain> + '_ {
        self.domains.iter().copied()
    }

    pub fn union_with(&mut self, other: &Self) {
        self.domains.extend(other.domains.iter().copied());
    }
}

/// Deterministic instruction/step allowance, not wall time. Exhaustion is a
/// preemption signal, reported without changing the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskBudget {
    limit: u64,
    used: u64,
}

impl TaskBudget {
    #[must_use]
    pub const fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }

    #[must_use]
    pub const fn used(self) -> u64 {
        self.used
    }

    #[must_use]
    pub const fn remaining(self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetExhausted {
    requested: u64,
    remaining: u64,
}

impl BudgetExhausted {
    #[must_use]
    pub const fn requested(self) -> u64 {
        self.requested
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFailure {
    message: String,
}

impl TaskFailure {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CancelReason {
    HostRequested,
    AbortSignal,
    AgentTerminated,
}

impl CancelReason {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HostRequested => "host-requested",
            Self::AbortSignal => "abort-signal",
            Self::AgentTerminated => "agent-terminated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    Completed,
    Failed(TaskFailure),
    Cancelled(CancelReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskAction {
    MakeReady,
    Start,
    Suspend,
    Resume,
    Complete,
    Fail,
    Cancel,
    Restart,
}

impl TaskAction {
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

    #[must_use]
    pub const fn task(&self) -> TaskId {
        self.task
    }

    #[must_use]
    pub const fn from(&self) -> TaskState {
        self.from
    }

    #[must_use]
    pub const fn action(&self) -> TaskAction {
        self.action
    }

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

    #[must_use]
    pub fn with_budget(mut self, budget: TaskBudget) -> Self {
        self.budget = budget;
        self
    }

    #[must_use]
    pub fn with_effect(mut self, domain: EffectDomain) -> Self {
        self.effects.insert(domain);
        self
    }

    #[must_use]
    pub fn with_effects(mut self, effects: EffectSet) -> Self {
        self.effects = effects;
        self
    }

    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    #[must_use]
    pub const fn kind(&self) -> TaskKind {
        self.kind
    }

    #[must_use]
    pub const fn state(&self) -> TaskState {
        self.state
    }

    #[must_use]
    pub const fn entry(&self) -> EntryPoint {
        self.entry
    }

    #[must_use]
    pub const fn continuation(&self) -> Option<ContinuationId> {
        self.continuation
    }

    pub fn take_continuation(&mut self) -> Option<ContinuationId> {
        self.continuation.take()
    }

    #[must_use]
    pub const fn effects(&self) -> &EffectSet {
        &self.effects
    }

    #[must_use]
    pub const fn budget(&self) -> TaskBudget {
        self.budget
    }

    pub fn charge_fuel(&mut self, amount: u64) -> Result<(), BudgetExhausted> {
        self.budget.charge(amount)
    }

    #[must_use]
    pub const fn parent_tasks(&self) -> Option<TasksId> {
        self.parent
    }

    #[must_use]
    pub const fn outcome(&self) -> Option<&TaskOutcome> {
        self.outcome.as_ref()
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    pub fn make_ready(&mut self) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Created,
            TaskState::Ready,
            TaskAction::MakeReady,
            "make-ready requires a newly created task",
        )
    }

    pub fn start(&mut self) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Ready,
            TaskState::Running,
            TaskAction::Start,
            "start requires a ready task",
        )
    }

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

    pub fn resume(&mut self) -> Result<(), TaskTransitionError> {
        self.expect_state(
            TaskState::Suspended,
            TaskState::Ready,
            TaskAction::Resume,
            "resume requires a suspended task",
        )
    }

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

    /// Cooperative cancellation: marks the task; a running host callback must
    /// reach a safepoint before it actually stops.
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

    /// Restart only pure tasks; effectful work must resume from its
    /// continuation. Clears continuation/outcome and returns to `Ready`.
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
