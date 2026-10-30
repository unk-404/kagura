//! The [`Tasks`] package: the scheduling, stealing, and accounting unit.
//!
//! A `Tasks` bundles one or more [`Task`]s that share a context (object graph,
//! module, effect domain). The scheduler and work-stealing deques move whole
//! `Tasks`; a package may additionally expose individual tasks for stealing
//! when its [`SplitPolicy`] allows it.

use std::error::Error;
use std::fmt;

use crate::id::{ContextId, TaskId, TasksId};
use crate::task::{CancelReason, Task, TaskState};

/// How a [`Tasks`] package may be split for work stealing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SplitPolicy {
    /// The package is a transaction sequence and can only be stolen or migrated
    /// as a whole. Used for effect chains.
    Atomic,
    /// Individual tasks inside the package may be stolen.
    Splittable,
    /// Every task is restartable; tasks may be stolen and even duplicated for
    /// speculative parallel execution.
    Pure,
}

impl SplitPolicy {
    /// Returns the stable name used in logs, timelines, and errors.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Atomic => "atomic",
            Self::Splittable => "splittable",
            Self::Pure => "pure",
        }
    }

    /// Whether task-granularity stealing is allowed.
    #[must_use]
    pub const fn allows_task_steal(self) -> bool {
        matches!(self, Self::Splittable | Self::Pure)
    }
}

/// Lifecycle state of one [`Tasks`] package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TasksState {
    /// Under construction; tasks may still be pushed.
    Pending,
    /// Ready to be placed on a queue.
    Runnable,
    /// Claimed by a runner unit.
    Running,
    /// All child tasks reached a terminal state.
    Done,
    /// The package was cancelled before completion.
    Cancelled,
}

impl TasksState {
    /// Returns the stable name used in logs and timelines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Runnable => "runnable",
            Self::Running => "running",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    /// Whether this is a terminal state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
}

/// The state-machine action that failed on a [`Tasks`] package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TasksAction {
    /// Push a task into the package.
    Push,
    /// `Pending -> Runnable`.
    MakeRunnable,
    /// `Runnable -> Running`.
    BeginRun,
    /// `Running -> Done`.
    Finish,
    /// Any non-terminal state -> Cancelled.
    Cancel,
    /// Split a single task out for task-granularity stealing.
    SplitTask,
}

impl TasksAction {
    /// Returns the stable action name used in errors.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::MakeRunnable => "make-runnable",
            Self::BeginRun => "begin-run",
            Self::Finish => "finish",
            Self::Cancel => "cancel",
            Self::SplitTask => "split-task",
        }
    }
}

/// Resource sub-ledger owned by one [`Tasks`] package.
///
/// The scheduler only accumulates counters here. Enforcement (emergency mode,
/// GC pressure, deadlines) belongs to the resource accounting layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TasksLedger {
    fuel_used: u64,
    heap_bytes: u64,
    stack_bytes: u64,
}

impl TasksLedger {
    /// Fuel charged to this package so far.
    #[must_use]
    pub const fn fuel_used(self) -> u64 {
        self.fuel_used
    }

    /// Heap bytes attributed to this package.
    #[must_use]
    pub const fn heap_bytes(self) -> u64 {
        self.heap_bytes
    }

    /// Stack bytes attributed to this package.
    #[must_use]
    pub const fn stack_bytes(self) -> u64 {
        self.stack_bytes
    }

    /// Records `units` of consumed fuel, saturating at `u64::MAX`.
    pub fn record_fuel(&mut self, units: u64) {
        self.fuel_used = self.fuel_used.saturating_add(units);
    }

    /// Records `bytes` of heap attributed to the package, saturating.
    pub fn record_heap(&mut self, bytes: u64) {
        self.heap_bytes = self.heap_bytes.saturating_add(bytes);
    }

    /// Records `bytes` of stack attributed to the package, saturating.
    pub fn record_stack(&mut self, bytes: u64) {
        self.stack_bytes = self.stack_bytes.saturating_add(bytes);
    }
}

/// Error returned for an illegal [`Tasks`] operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TasksError {
    /// A task with the same id is already in the package.
    DuplicateTask {
        /// The package that rejected the push.
        tasks: TasksId,
        /// The duplicated task id.
        task: TaskId,
    },
    /// The task is already terminal and cannot join a runnable package.
    TaskTerminal {
        /// The package that rejected the push.
        tasks: TasksId,
        /// The terminal task id.
        task: TaskId,
    },
    /// A package without tasks cannot become runnable.
    Empty {
        /// The empty package.
        tasks: TasksId,
    },
    /// The package is in the wrong state for the requested transition.
    InvalidTransition {
        /// The package that rejected the transition.
        tasks: TasksId,
        /// The state the package was in.
        from: TasksState,
        /// The attempted action.
        action: TasksAction,
    },
    /// `finish` was called while some child tasks were still running or pending.
    UnfinishedTasks {
        /// The package with unfinished children.
        tasks: TasksId,
        /// Number of child tasks that are not terminal yet.
        remaining: usize,
    },
    /// The split policy does not allow task-granularity stealing.
    TaskStealNotAllowed {
        /// The package that would have been split.
        tasks: TasksId,
        /// The package's split policy.
        policy: SplitPolicy,
    },
    /// No `Ready` child task was available for task-granularity stealing.
    NothingToSplit {
        /// The package that was asked to give up a task.
        tasks: TasksId,
    },
}

impl fmt::Display for TasksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTask { tasks, task } => {
                write!(f, "tasks {} already contains task {}", tasks, task)
            }
            Self::TaskTerminal { tasks, task } => {
                write!(f, "tasks {} cannot adopt terminal task {}", tasks, task)
            }
            Self::Empty { tasks } => {
                write!(f, "tasks {} has no tasks and cannot run", tasks)
            }
            Self::InvalidTransition {
                tasks,
                from,
                action,
            } => write!(
                f,
                "tasks {} cannot {} from {}",
                tasks,
                action.name(),
                from.name()
            ),
            Self::UnfinishedTasks { tasks, remaining } => write!(
                f,
                "tasks {} still has {} unfinished task(s)",
                tasks, remaining
            ),
            Self::TaskStealNotAllowed { tasks, policy } => write!(
                f,
                "tasks {} cannot be split: split policy is {}",
                tasks,
                policy.name()
            ),
            Self::NothingToSplit { tasks } => write!(
                f,
                "tasks {} has no ready task available for stealing",
                tasks
            ),
        }
    }
}

impl Error for TasksError {}

/// A scheduling package: a context-bound group of tasks.
#[derive(Debug)]
pub struct Tasks {
    id: TasksId,
    state: TasksState,
    tasks: Vec<Task>,
    context: ContextId,
    split_policy: SplitPolicy,
    affinity: Option<u16>,
    deadline: Option<u64>,
    ledger: TasksLedger,
}

impl Tasks {
    /// Creates an empty package in [`TasksState::Pending`].
    #[must_use]
    pub fn new(id: TasksId, context: ContextId, split_policy: SplitPolicy) -> Self {
        Self {
            id,
            state: TasksState::Pending,
            tasks: Vec::new(),
            context,
            split_policy,
            affinity: None,
            deadline: None,
            ledger: TasksLedger::default(),
        }
    }

    /// Sets a preferred run-unit affinity (for example a NUMA node or core id).
    #[must_use]
    pub fn with_affinity(mut self, affinity: u16) -> Self {
        self.affinity = Some(affinity);
        self
    }

    /// Sets a scheduling deadline in embedder-defined monotonic time units.
    #[must_use]
    pub fn with_deadline(mut self, deadline: u64) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// The package identifier.
    #[must_use]
    pub const fn id(&self) -> TasksId {
        self.id
    }

    /// The current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> TasksState {
        self.state
    }

    /// The packager-assigned context id.
    #[must_use]
    pub const fn context(&self) -> ContextId {
        self.context
    }

    /// The package's split policy.
    #[must_use]
    pub const fn split_policy(&self) -> SplitPolicy {
        self.split_policy
    }

    /// Preferred run-unit affinity, if any.
    #[must_use]
    pub const fn affinity(&self) -> Option<u16> {
        self.affinity
    }

    /// Deadline in embedder-defined monotonic time units, if any.
    #[must_use]
    pub const fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    /// The package's cumulative resource ledger.
    #[must_use]
    pub const fn ledger(&self) -> TasksLedger {
        self.ledger
    }

    /// Mutable access to the resource ledger.
    pub fn ledger_mut(&mut self) -> &mut TasksLedger {
        &mut self.ledger
    }

    /// Number of tasks in the package.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether the package contains no tasks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// All tasks in insertion order.
    #[must_use]
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// All tasks in insertion order, mutably.
    pub fn tasks_mut(&mut self) -> &mut [Task] {
        &mut self.tasks
    }

    /// Looks up a task by id.
    #[must_use]
    pub fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id() == id)
    }

    /// Looks up a task by id, mutably.
    pub fn task_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id() == id)
    }

    /// Adds a task while the package is still [`TasksState::Pending`].
    ///
    /// The task's `parent_tasks` link is updated to this package, which also
    /// covers tasks that were split out of a package and re-packaged.
    ///
    /// # Errors
    ///
    /// Returns [`TasksError::DuplicateTask`] for a repeated id,
    /// [`TasksError::TaskTerminal`] for an already finished task, and
    /// [`TasksError::InvalidTransition`] once the package left `Pending`.
    pub fn push(&mut self, mut task: Task) -> Result<TaskId, TasksError> {
        if self.state != TasksState::Pending {
            return Err(TasksError::InvalidTransition {
                tasks: self.id,
                from: self.state,
                action: TasksAction::Push,
            });
        }
        let task_id = task.id();
        if task.is_terminal() {
            return Err(TasksError::TaskTerminal {
                tasks: self.id,
                task: task_id,
            });
        }
        if self.task(task_id).is_some() {
            return Err(TasksError::DuplicateTask {
                tasks: self.id,
                task: task_id,
            });
        }
        task.set_parent(self.id);
        self.tasks.push(task);
        Ok(task_id)
    }

    /// Number of child tasks that are not terminal yet.
    #[must_use]
    pub fn remaining_tasks(&self) -> usize {
        self.tasks.iter().filter(|task| !task.is_terminal()).count()
    }

    /// Number of child tasks in [`TaskState::Ready`].
    #[must_use]
    pub fn ready_tasks(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.state() == TaskState::Ready)
            .count()
    }

    /// `Pending -> Runnable`.
    ///
    /// # Errors
    ///
    /// Returns [`TasksError::Empty`] for a package without tasks and
    /// [`TasksError::InvalidTransition`] if the package is not `Pending`.
    pub fn make_runnable(&mut self) -> Result<(), TasksError> {
        if self.tasks.is_empty() {
            return Err(TasksError::Empty { tasks: self.id });
        }
        self.expect_state(
            TasksState::Pending,
            TasksState::Runnable,
            TasksAction::MakeRunnable,
        )
    }

    /// `Runnable -> Running`.
    ///
    /// # Errors
    ///
    /// Returns [`TasksError::InvalidTransition`] if the package is not
    /// `Runnable`.
    pub fn begin_run(&mut self) -> Result<(), TasksError> {
        self.expect_state(
            TasksState::Runnable,
            TasksState::Running,
            TasksAction::BeginRun,
        )
    }

    /// `Running -> Done`, but only once every child task is terminal.
    ///
    /// # Errors
    ///
    /// Returns [`TasksError::UnfinishedTasks`] if any child is not terminal,
    /// and [`TasksError::InvalidTransition`] if the package is not `Running`.
    pub fn finish(&mut self) -> Result<(), TasksError> {
        if self.state != TasksState::Running {
            return Err(TasksError::InvalidTransition {
                tasks: self.id,
                from: self.state,
                action: TasksAction::Finish,
            });
        }
        let remaining = self.remaining_tasks();
        if remaining > 0 {
            return Err(TasksError::UnfinishedTasks {
                tasks: self.id,
                remaining,
            });
        }
        self.state = TasksState::Done;
        Ok(())
    }

    /// Cancels the package and every non-terminal child task.
    ///
    /// Cancellation is cooperative for running tasks: they observe the
    /// cancelled state at their next safepoint.
    ///
    /// # Errors
    ///
    /// Returns [`TasksError::InvalidTransition`] if the package is already
    /// terminal.
    pub fn cancel(&mut self, reason: CancelReason) -> Result<(), TasksError> {
        if self.state.is_terminal() {
            return Err(TasksError::InvalidTransition {
                tasks: self.id,
                from: self.state,
                action: TasksAction::Cancel,
            });
        }
        for task in &mut self.tasks {
            if !task.is_terminal() {
                let _ = task.cancel(reason);
            }
        }
        self.state = TasksState::Cancelled;
        Ok(())
    }

    /// Splits one `Ready` child task out of the package for task-granularity
    /// stealing.
    ///
    /// The returned task keeps its `parent_tasks` link so timelines can still
    /// attribute it to the original package. Whole-package stealing does not
    /// need this method.
    ///
    /// # Errors
    ///
    /// * [`TasksError::InvalidTransition`] unless the package is `Runnable`.
    /// * [`TasksError::TaskStealNotAllowed`] for [`SplitPolicy::Atomic`].
    /// * [`TasksError::NothingToSplit`] when the package has fewer than two
    ///   tasks or no `Ready` child.
    pub fn split_task(&mut self) -> Result<Task, TasksError> {
        if self.state != TasksState::Runnable {
            return Err(TasksError::InvalidTransition {
                tasks: self.id,
                from: self.state,
                action: TasksAction::SplitTask,
            });
        }
        if !self.split_policy.allows_task_steal() {
            return Err(TasksError::TaskStealNotAllowed {
                tasks: self.id,
                policy: self.split_policy,
            });
        }
        if self.tasks.len() < 2 {
            return Err(TasksError::NothingToSplit { tasks: self.id });
        }
        let Some(index) = self
            .tasks
            .iter()
            .position(|task| task.state() == TaskState::Ready)
        else {
            return Err(TasksError::NothingToSplit { tasks: self.id });
        };
        Ok(self.tasks.remove(index))
    }

    fn expect_state(
        &mut self,
        expected: TasksState,
        next: TasksState,
        action: TasksAction,
    ) -> Result<(), TasksError> {
        if self.state != expected {
            return Err(TasksError::InvalidTransition {
                tasks: self.id,
                from: self.state,
                action,
            });
        }
        self.state = next;
        Ok(())
    }
}
