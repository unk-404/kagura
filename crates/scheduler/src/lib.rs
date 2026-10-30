//! Task/Tasks/RU scheduling primitives and a deterministic single-thread job /
//! microtask queue. The queue is `!Send`; cross-thread work enters through
//! [`Injector`]. This crate does not execute JS or spawn OS threads.

#![forbid(unsafe_code)]

mod deque;
mod injector;
mod queue;
mod ru;
mod task;
mod tasks;

pub mod id;

pub use deque::{Stealer, WorkerDeque};
pub use id::{ContextId, ContinuationId, EntryPoint, JobId, RuId, TaskId, TasksId};
pub use injector::Injector;
pub use queue::{DrainReport, JobClass, JobError, JobFailure, JobQueue, QueueHandle};
pub use ru::{Ru, RuPool, RuSpawnError, RuState, RuStateError, RuStats};
pub use task::{
    BudgetExhausted, CancelReason, EffectDomain, EffectSet, Task, TaskAction, TaskBudget,
    TaskFailure, TaskKind, TaskOutcome, TaskState, TaskTransitionError,
};
pub use tasks::{SplitPolicy, Tasks, TasksAction, TasksError, TasksLedger, TasksState};
