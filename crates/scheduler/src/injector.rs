//! Thread-safe injection queue.
//!
//! An [`Injector`] is the only queue in this crate that multiple threads may
//! touch. Runner units pull whole [`crate::Tasks`] packages from it, or foreign
//! threads/producers push completions into it. It is intentionally simpler than
//! the per-RU work-stealing deques: a single mutex-protected FIFO, adequate for
//! the M5 skeleton and easy to replace with a lock-free MPMC queue later.
//!
//! Lock granularity is the whole queue: push/pop hold the mutex only long
//! enough to move one item, then release it. The mutex supplies the
//! happens-before edge between producer and consumer; the `pushed`/`popped`
//! counters are observation-only and therefore use `Relaxed` ordering.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// A thread-safe FIFO injection queue.
#[derive(Debug)]
pub struct Injector<T> {
    queue: Mutex<VecDeque<T>>,
    pushed: AtomicU64,
    popped: AtomicU64,
}

impl<T> Injector<T> {
    /// Creates an empty injector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            pushed: AtomicU64::new(0),
            popped: AtomicU64::new(0),
        }
    }

    /// Pushes an item at the tail.
    pub fn push(&self, item: T) {
        self.lock().push_back(item);
        // 计数只用于观测，Relaxed 即可；跨线程可见性由 Mutex 提供。
        self.pushed.fetch_add(1, Ordering::Relaxed);
    }

    /// Pops the oldest item, if any.
    pub fn try_pop(&self) -> Option<T> {
        let item = self.lock().pop_front();
        if item.is_some() {
            self.popped.fetch_add(1, Ordering::Relaxed);
        }
        item
    }

    /// Number of currently queued items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether the queue is currently empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Total number of successful pushes.
    #[must_use]
    pub fn pushed(&self) -> u64 {
        self.pushed.load(Ordering::Relaxed)
    }

    /// Total number of successful pops.
    #[must_use]
    pub fn popped(&self) -> u64 {
        self.popped.load(Ordering::Relaxed)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<T>> {
        // producer 侧 panic 后恢复锁内数据，避免一次 panic 永久瘫痪调度。
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<T> Default for Injector<T> {
    fn default() -> Self {
        Self::new()
    }
}
