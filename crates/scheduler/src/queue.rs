//! Deterministic single-threaded job and microtask queues.
//!
//! This is the semantic core of the runtime: one FIFO **job** queue, one FIFO
//! **microtask** queue, and an ECMAScript-shaped microtask checkpoint after
//! each job. It is deliberately not `Send`: an agent event loop owns its queue
//! on exactly one thread. Cross-thread work enters through
//! [`crate::Injector`] instead.
//!
//! Ordering contract:
//!
//! 1. Jobs run in FIFO order, one at a time, to completion.
//! 2. After each job the loop drains every queued microtask, including
//!    microtasks enqueued while draining (always appended to the tail).
//! 3. A microtask checkpoint is atomic with respect to pause: `pause()` is only
//!    observed at the next job boundary, never in the middle of a checkpoint.
//! 4. A failing job or microtask is recorded in the [`DrainReport`] and does
//!    not stop later work; the embedder decides how to surface the failure.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::id::JobId;

/// Whether a queued callback is a job (macrotask) or a microtask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JobClass {
    /// A regular job: runs before the next checkpoint, one at a time.
    Job,
    /// A microtask: runs inside the current checkpoint.
    Microtask,
}

impl JobClass {
    /// Returns the stable name used in logs and failures.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Job => "job",
            Self::Microtask => "microtask",
        }
    }
}

/// A host error raised by a queued callback.
///
/// The message is kept host-readable; the VM embedding maps this to the
/// appropriate JS exception or rejected Promise at the boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    message: String,
}

impl JobError {
    /// Creates an error from a message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for JobError {}

/// One failed queue entry, in execution order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobFailure {
    job: JobId,
    class: JobClass,
    error: JobError,
}

impl JobFailure {
    fn new(job: JobId, class: JobClass, error: JobError) -> Self {
        Self { job, class, error }
    }

    /// The id assigned when the callback was enqueued.
    #[must_use]
    pub const fn job(&self) -> JobId {
        self.job
    }

    /// Whether the failing callback was a job or a microtask.
    #[must_use]
    pub const fn class(&self) -> JobClass {
        self.class
    }

    /// The host error raised by the callback.
    #[must_use]
    pub const fn error(&self) -> &JobError {
        &self.error
    }

    /// Consumes the failure and returns its error.
    #[must_use]
    pub fn into_error(self) -> JobError {
        self.error
    }
}

impl fmt::Display for JobFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} failed: {}",
            self.class.name(),
            self.job,
            self.error
        )
    }
}

/// Summary of a [`JobQueue::drain`] or [`JobQueue::drain_microtasks`] call.
#[derive(Debug, Default)]
pub struct DrainReport {
    executed_jobs: usize,
    executed_microtasks: usize,
    failures: Vec<JobFailure>,
    paused: bool,
    reentrant: bool,
}

impl DrainReport {
    /// Number of jobs that were executed.
    #[must_use]
    pub const fn executed_jobs(&self) -> usize {
        self.executed_jobs
    }

    /// Number of microtasks that were executed.
    #[must_use]
    pub const fn executed_microtasks(&self) -> usize {
        self.executed_microtasks
    }

    /// Total callbacks executed.
    #[must_use]
    pub const fn executed_total(&self) -> usize {
        self.executed_jobs + self.executed_microtasks
    }

    /// Failures in execution order.
    #[must_use]
    pub fn failures(&self) -> &[JobFailure] {
        &self.failures
    }

    /// Whether no callback failed.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    /// The first failure, if any.
    #[must_use]
    pub fn first_failure(&self) -> Option<&JobFailure> {
        self.failures.first()
    }

    /// Whether the queue was still paused when the drain returned.
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Whether this drain was rejected because another drain was in progress.
    ///
    /// A reentrant drain performs no work and returns an otherwise empty
    /// report. This can only happen when an embedder wraps the queue in
    /// `Rc`/`RefCell` and calls `drain` from inside a queued callback.
    #[must_use]
    pub const fn is_reentrant(&self) -> bool {
        self.reentrant
    }

    /// Converts the report into a result, preserving all failures.
    ///
    /// # Errors
    ///
    /// Returns every recorded [`JobFailure`] when at least one callback failed.
    pub fn into_result(self) -> Result<Self, Vec<JobFailure>> {
        if self.failures.is_empty() {
            Ok(self)
        } else {
            Err(self.failures)
        }
    }

    fn record(&mut self, class: JobClass) {
        match class {
            JobClass::Job => self.executed_jobs += 1,
            JobClass::Microtask => self.executed_microtasks += 1,
        }
    }
}

/// Type-erased queued callback.
type JobFn = Box<dyn FnOnce() -> Result<(), JobError>>;

struct QueuedJob {
    id: JobId,
    class: JobClass,
    action: JobFn,
}

struct QueueState {
    jobs: VecDeque<QueuedJob>,
    microtasks: VecDeque<QueuedJob>,
    paused: bool,
    dispatching: bool,
    next_job_id: u64,
}

/// RAII 守卫：即使回调 panic，也把 `dispatching` 复位，避免事件循环永久拒绝 drain。
struct DispatchGuard {
    state: Rc<RefCell<QueueState>>,
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        self.state.borrow_mut().dispatching = false;
    }
}

/// A cloneable, queue-only handle.
///
/// Handles deliberately cannot drain the queue. Jobs receive a handle to
/// enqueue follow-up work, but they cannot recursively drive the event loop;
/// that keeps run-to-completion intact. The handle is single-threaded (`Rc`),
/// so it is not `Send`.
#[derive(Clone)]
pub struct QueueHandle {
    state: Rc<RefCell<QueueState>>,
}

impl QueueHandle {
    /// Enqueues a job at the tail of the job queue.
    pub fn enqueue_job<F>(&self, job: F) -> JobId
    where
        F: FnOnce() -> Result<(), JobError> + 'static,
    {
        self.enqueue(JobClass::Job, job)
    }

    /// Enqueues a microtask at the tail of the microtask queue.
    pub fn enqueue_microtask<F>(&self, task: F) -> JobId
    where
        F: FnOnce() -> Result<(), JobError> + 'static,
    {
        self.enqueue(JobClass::Microtask, task)
    }

    /// Pauses the event loop at the next job boundary.
    pub fn pause(&self) {
        self.state.borrow_mut().paused = true;
    }

    /// Clears the pause flag.
    pub fn resume(&self) {
        self.state.borrow_mut().paused = false;
    }

    /// Whether pause has been requested.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.state.borrow().paused
    }

    /// Number of queued jobs.
    #[must_use]
    pub fn jobs_len(&self) -> usize {
        self.state.borrow().jobs.len()
    }

    /// Number of queued microtasks.
    #[must_use]
    pub fn microtasks_len(&self) -> usize {
        self.state.borrow().microtasks.len()
    }

    fn enqueue<F>(&self, class: JobClass, job: F) -> JobId
    where
        F: FnOnce() -> Result<(), JobError> + 'static,
    {
        let mut state = self.state.borrow_mut();
        let id = JobId::new(state.next_job_id);
        state.next_job_id += 1;
        let queued = QueuedJob {
            id,
            class,
            action: Box::new(job),
        };
        match class {
            JobClass::Job => state.jobs.push_back(queued),
            JobClass::Microtask => state.microtasks.push_back(queued),
        }
        id
    }
}

/// Deterministic single-threaded job and microtask queue.
///
/// The queue is not `Clone`: a job that captured the queue itself would be able
/// to re-enter [`JobQueue::drain`]. Jobs only get [`QueueHandle`], which cannot
/// drain.
pub struct JobQueue {
    // Rc<RefCell> 让 JobQueue 显式 !Send：一个 agent 的事件循环只能由单线程驱动。
    // 借用只覆盖一次 push/pop，执行回调前必定释放，因此回调通过 QueueHandle 再入队不会触发 RefCell 冲突。
    state: Rc<RefCell<QueueState>>,
}

impl JobQueue {
    /// Creates an empty, running (not paused) queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(QueueState {
                jobs: VecDeque::new(),
                microtasks: VecDeque::new(),
                paused: false,
                dispatching: false,
                next_job_id: 0,
            })),
        }
    }

    /// Creates a queue-only handle to enqueue work.
    #[must_use]
    pub fn handle(&self) -> QueueHandle {
        QueueHandle {
            state: Rc::clone(&self.state),
        }
    }

    /// Enqueues a job at the tail of the job queue.
    pub fn enqueue_job<F>(&self, job: F) -> JobId
    where
        F: FnOnce() -> Result<(), JobError> + 'static,
    {
        self.handle().enqueue_job(job)
    }

    /// Enqueues a microtask at the tail of the microtask queue.
    pub fn enqueue_microtask<F>(&self, task: F) -> JobId
    where
        F: FnOnce() -> Result<(), JobError> + 'static,
    {
        self.handle().enqueue_microtask(task)
    }

    /// Runs jobs and microtasks until both queues are empty or the queue is
    /// paused at a job boundary.
    ///
    /// Pause requested from inside a job takes effect after that job's
    /// microtask checkpoint, never before. Errors are recorded and execution
    /// continues.
    ///
    /// A drain requested while another drain is already running (possible only
    /// when the embedder wraps the queue in `Rc`/`RefCell`) is rejected: it
    /// performs no work and sets [`DrainReport::is_reentrant`].
    pub fn drain(&self) -> DrainReport {
        let mut report = DrainReport::default();
        let Some(_guard) = self.begin_dispatch() else {
            report.reentrant = true;
            report.paused = self.is_paused();
            return report;
        };
        loop {
            if self.is_paused() {
                break;
            }
            let next_job = self.state.borrow_mut().jobs.pop_front();
            if let Some(job) = next_job {
                Self::execute(job, &mut report);
                self.run_microtask_checkpoint(&mut report);
                continue;
            }
            if !self.state.borrow().microtasks.is_empty() {
                self.run_microtask_checkpoint(&mut report);
                continue;
            }
            break;
        }
        report.paused = self.is_paused();
        report
    }

    /// Runs a microtask checkpoint even when the queue is paused.
    ///
    /// A checkpoint is atomic: it drains microtasks until the queue is empty,
    /// including microtasks enqueued during the checkpoint. `pause()` does not
    /// split it. A reentrant checkpoint is rejected like a reentrant
    /// [`Self::drain`] and sets [`DrainReport::is_reentrant`].
    pub fn drain_microtasks(&self) -> DrainReport {
        let mut report = DrainReport::default();
        let Some(_guard) = self.begin_dispatch() else {
            report.reentrant = true;
            report.paused = self.is_paused();
            return report;
        };
        self.run_microtask_checkpoint(&mut report);
        report.paused = self.is_paused();
        report
    }

    /// Pauses the event loop at the next job boundary.
    pub fn pause(&self) {
        self.handle().pause();
    }

    /// Clears the pause flag.
    pub fn resume(&self) {
        self.handle().resume();
    }

    /// Whether pause has been requested.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.state.borrow().paused
    }

    /// Number of queued jobs.
    #[must_use]
    pub fn jobs_len(&self) -> usize {
        self.state.borrow().jobs.len()
    }

    /// Number of queued microtasks.
    #[must_use]
    pub fn microtasks_len(&self) -> usize {
        self.state.borrow().microtasks.len()
    }

    /// Whether both queues are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let state = self.state.borrow();
        state.jobs.is_empty() && state.microtasks.is_empty()
    }

    fn begin_dispatch(&self) -> Option<DispatchGuard> {
        let state_rc = Rc::clone(&self.state);
        let mut state = state_rc.borrow_mut();
        // 用状态位拒绝重入 drain：合法性由运行时检查，避免依赖“调用者不会用 Rc 包一层”。
        if state.dispatching {
            return None;
        }
        state.dispatching = true;
        drop(state);
        Some(DispatchGuard { state: state_rc })
    }

    fn run_microtask_checkpoint(&self, report: &mut DrainReport) {
        loop {
            let next = self.state.borrow_mut().microtasks.pop_front();
            let Some(microtask) = next else {
                return;
            };
            Self::execute(microtask, report);
        }
    }

    fn execute(job: QueuedJob, report: &mut DrainReport) {
        report.record(job.class);
        if let Err(error) = (job.action)() {
            report
                .failures
                .push(JobFailure::new(job.id, job.class, error));
        }
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}
