//! Kagura's task-runtime foundation: Task / Tasks / RU data model, a
//! deterministic single-threaded job + microtask queue, and the thread-safe
//! scheduling skeleton used by runner units.
//!
//! This crate deliberately has **no tokio, no futures, and no JS engine**
//! dependency. ECMAScript requires a single agent event loop with exact
//! run-to-completion and microtask ordering; a multi-threaded general-purpose
//! executor cannot provide that. Multi-RU parallelism is expressed only through
//! whole-[`Tasks`] packages (or explicitly stealable tasks inside a package),
//! and host reactors are expected to inject completions through [`Injector`]
//! rather than spawn JS anywhere they like.
//!
//! # Layers
//!
//! * [`Task`] — the smallest suspend/resume unit, with a validated lifecycle
//!   state machine and a fuel budget.
//! * [`Tasks`] — a context-bound package that the scheduler and work-stealing
//!   deques move as a unit, plus its split policy.
//! * [`JobQueue`] — the deterministic, single-threaded job and microtask queue
//!   that provides ECMAScript ordering. Not `Send` by construction.
//! * [`Injector`] — the thread-safe injection queue through which other threads
//!   (reactor completions, foreign callbacks) hand work to an agent loop.
//! * [`WorkerDeque`] / [`Stealer`] / [`Ru`] — the per-RU work-stealing skeleton.
//!
//! # Example: one job and its microtask checkpoint
//!
//! ```
//! use kagura_scheduler::{JobError, JobQueue};
//!
//! let queue = JobQueue::new();
//! let handle = queue.handle();
//!
//! queue.enqueue_job(move || {
//!     handle.enqueue_microtask(|| Ok(()));
//!     Ok(())
//! });
//!
//! let report = queue.drain();
//! assert!(report.is_ok());
//! assert_eq!(report.executed_jobs(), 1);
//! assert_eq!(report.executed_microtasks(), 1);
//! assert!(queue.is_empty());
//! ```
//!
//! The sequence is deterministic because both queues are FIFO and the
//! checkpoint runs immediately after the job, before the next job.

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
