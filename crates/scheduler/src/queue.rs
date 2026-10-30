//! Deterministic single-threaded job/microtask queue. `!Send` by design:
//! cross-thread completions enter through [`crate::Injector`].
//!
//! Ordering: FIFO jobs, full FIFO microtask checkpoint after each job, pause only
//! at job boundaries, failures recorded without stopping later work.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::id::JobId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JobClass {
    Job,
    Microtask,
}

impl JobClass {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Job => "job",
            Self::Microtask => "microtask",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    message: String,
}

impl JobError {
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

    #[must_use]
    pub const fn job(&self) -> JobId {
        self.job
    }

    #[must_use]
    pub const fn class(&self) -> JobClass {
        self.class
    }

    #[must_use]
    pub const fn error(&self) -> &JobError {
        &self.error
    }

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

#[derive(Debug, Default)]
pub struct DrainReport {
    executed_jobs: usize,
    executed_microtasks: usize,
    failures: Vec<JobFailure>,
    paused: bool,
    reentrant: bool,
}

impl DrainReport {
    #[must_use]
    pub const fn executed_jobs(&self) -> usize {
        self.executed_jobs
    }

    #[must_use]
    pub const fn executed_microtasks(&self) -> usize {
        self.executed_microtasks
    }

    #[must_use]
    pub const fn executed_total(&self) -> usize {
        self.executed_jobs + self.executed_microtasks
    }

    /// Failures in execution order.
    #[must_use]
    pub fn failures(&self) -> &[JobFailure] {
        &self.failures
    }

    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    #[must_use]
    pub fn first_failure(&self) -> Option<&JobFailure> {
        self.failures.first()
    }

    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub const fn is_reentrant(&self) -> bool {
        self.reentrant
    }

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

#[derive(Clone)]
pub struct QueueHandle {
    state: Rc<RefCell<QueueState>>,
}

impl QueueHandle {
    pub fn enqueue_job<F>(&self, job: F) -> JobId
    where
        F: FnOnce() -> Result<(), JobError> + 'static,
    {
        self.enqueue(JobClass::Job, job)
    }

    pub fn enqueue_microtask<F>(&self, task: F) -> JobId
    where
        F: FnOnce() -> Result<(), JobError> + 'static,
    {
        self.enqueue(JobClass::Microtask, task)
    }

    pub fn pause(&self) {
        self.state.borrow_mut().paused = true;
    }

    pub fn resume(&self) {
        self.state.borrow_mut().paused = false;
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.state.borrow().paused
    }

    #[must_use]
    pub fn jobs_len(&self) -> usize {
        self.state.borrow().jobs.len()
    }

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

    #[must_use]
    pub fn handle(&self) -> QueueHandle {
        QueueHandle {
            state: Rc::clone(&self.state),
        }
    }

    pub fn enqueue_job<F>(&self, job: F) -> JobId
    where
        F: FnOnce() -> Result<(), JobError> + 'static,
    {
        self.handle().enqueue_job(job)
    }

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

    pub fn pause(&self) {
        self.handle().pause();
    }

    pub fn resume(&self) {
        self.handle().resume();
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.state.borrow().paused
    }

    #[must_use]
    pub fn jobs_len(&self) -> usize {
        self.state.borrow().jobs.len()
    }

    #[must_use]
    pub fn microtasks_len(&self) -> usize {
        self.state.borrow().microtasks.len()
    }

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
