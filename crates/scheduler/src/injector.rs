//! Thread-safe FIFO injection queue. Completions from other threads enter the
//! agent event loop here, not by touching the VM directly.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct Injector<T> {
    queue: Mutex<VecDeque<T>>,
    pushed: AtomicU64,
    popped: AtomicU64,
}

impl<T> Injector<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            pushed: AtomicU64::new(0),
            popped: AtomicU64::new(0),
        }
    }

    pub fn push(&self, item: T) {
        self.lock().push_back(item);
        // 计数只用于观测，Relaxed 即可；跨线程可见性由 Mutex 提供。
        self.pushed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn try_pop(&self) -> Option<T> {
        let item = self.lock().pop_front();
        if item.is_some() {
            self.popped.fetch_add(1, Ordering::Relaxed);
        }
        item
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    #[must_use]
    pub fn pushed(&self) -> u64 {
        self.pushed.load(Ordering::Relaxed)
    }

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
