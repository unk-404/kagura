//! Context-bound task package: the scheduling, stealing, and accounting unit.

use std::error::Error;
use std::fmt;

use crate::id::{ContextId, TaskId, TasksId};
use crate::task::{CancelReason, Task, TaskState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SplitPolicy {
    /// Whole package only; used for effect chains.
    Atomic,
    /// Individual tasks may be stolen.
    Splittable,
    /// Every task is restartable; tasks may be stolen or duplicated for
    /// speculative parallel execution.
    Pure,
}

impl SplitPolicy {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Atomic => "atomic",
            Self::Splittable => "splittable",
            Self::Pure => "pure",
        }
    }

    #[must_use]
    pub const fn allows_task_steal(self) -> bool {
        matches!(self, Self::Splittable | Self::Pure)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TasksState {
    Pending,
    Runnable,
    Running,
    Done,
    Cancelled,
}

impl TasksState {
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

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TasksAction {
    Push,
    MakeRunnable,
    BeginRun,
    Finish,
    Cancel,
    SplitTask,
}

impl TasksAction {
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TasksLedger {
    fuel_used: u64,
    heap_bytes: u64,
    stack_bytes: u64,
}

impl TasksLedger {
    #[must_use]
    pub const fn fuel_used(self) -> u64 {
        self.fuel_used
    }

    #[must_use]
    pub const fn heap_bytes(self) -> u64 {
        self.heap_bytes
    }

    #[must_use]
    pub const fn stack_bytes(self) -> u64 {
        self.stack_bytes
    }

    pub fn record_fuel(&mut self, units: u64) {
        self.fuel_used = self.fuel_used.saturating_add(units);
    }

    pub fn record_heap(&mut self, bytes: u64) {
        self.heap_bytes = self.heap_bytes.saturating_add(bytes);
    }

    pub fn record_stack(&mut self, bytes: u64) {
        self.stack_bytes = self.stack_bytes.saturating_add(bytes);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TasksError {
    DuplicateTask {
        tasks: TasksId,
        task: TaskId,
    },
    TaskTerminal {
        tasks: TasksId,
        task: TaskId,
    },
    Empty {
        tasks: TasksId,
    },
    InvalidTransition {
        tasks: TasksId,
        from: TasksState,
        action: TasksAction,
    },
    UnfinishedTasks {
        tasks: TasksId,
        remaining: usize,
    },
    TaskStealNotAllowed {
        tasks: TasksId,
        policy: SplitPolicy,
    },
    NothingToSplit {
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

    #[must_use]
    pub fn with_affinity(mut self, affinity: u16) -> Self {
        self.affinity = Some(affinity);
        self
    }

    #[must_use]
    pub fn with_deadline(mut self, deadline: u64) -> Self {
        self.deadline = Some(deadline);
        self
    }

    #[must_use]
    pub const fn id(&self) -> TasksId {
        self.id
    }

    #[must_use]
    pub const fn state(&self) -> TasksState {
        self.state
    }

    #[must_use]
    pub const fn context(&self) -> ContextId {
        self.context
    }

    #[must_use]
    pub const fn split_policy(&self) -> SplitPolicy {
        self.split_policy
    }

    #[must_use]
    pub const fn affinity(&self) -> Option<u16> {
        self.affinity
    }

    #[must_use]
    pub const fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    #[must_use]
    pub const fn ledger(&self) -> TasksLedger {
        self.ledger
    }

    pub fn ledger_mut(&mut self) -> &mut TasksLedger {
        &mut self.ledger
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

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

    #[must_use]
    pub fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id() == id)
    }

    pub fn task_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id() == id)
    }

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

    #[must_use]
    pub fn remaining_tasks(&self) -> usize {
        self.tasks.iter().filter(|task| !task.is_terminal()).count()
    }

    #[must_use]
    pub fn ready_tasks(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.state() == TaskState::Ready)
            .count()
    }

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

    pub fn begin_run(&mut self) -> Result<(), TasksError> {
        self.expect_state(
            TasksState::Runnable,
            TasksState::Running,
            TasksAction::BeginRun,
        )
    }

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
