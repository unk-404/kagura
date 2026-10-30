//! Per-RU work-stealing deque: owner pops newest (bottom), thieves steal oldest
//! (top). Mutex-backed until the Chase-Lev replacement lands.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct WorkerDeque<T> {
    queue: Arc<Mutex<VecDeque<T>>>,
}

impl<T> WorkerDeque<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn push(&self, item: T) {
        self.lock().push_back(item);
    }

    pub fn pop(&self) -> Option<T> {
        self.lock().pop_back()
    }

    #[must_use]
    pub fn stealer(&self) -> Stealer<T> {
        Stealer {
            queue: Arc::clone(&self.queue),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<T>> {
        // 骨架阶段优先正确性：Mutex 已提供跨线程 happens-before，因此暂不需要原子内存序。
        // 换成 Chase-Lev 时，owner push/pop 与 thief steal 的 Acquire/Release 必须重新推导。
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<T> Default for WorkerDeque<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct Stealer<T> {
    queue: Arc<Mutex<VecDeque<T>>>,
}

impl<T> Clone for Stealer<T> {
    fn clone(&self) -> Self {
        Self {
            queue: Arc::clone(&self.queue),
        }
    }
}

impl<T> Stealer<T> {
    pub fn steal(&self) -> Option<T> {
        self.lock().pop_front()
    }

    /// Steals up to `max` oldest items from the top, preserving FIFO order.
    pub fn steal_batch(&self, max: usize) -> Vec<T> {
        if max == 0 {
            return Vec::new();
        }
        let mut queue = self.lock();
        let count = queue.len().min(max);
        queue.drain(..count).collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<T>> {
        // 与 owner 侧共用同一把锁；steal 与 owner pop 串行化后才允许读队列内容。
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
