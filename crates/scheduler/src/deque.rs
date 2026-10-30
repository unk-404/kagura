//! Per-runner-unit work-stealing deque (skeleton).
//!
//! The owner pushes and pops at the **bottom** (LIFO, cache friendly); thieves
//! steal from the **top** (FIFO, oldest work first). This is the access pattern
//! of a Chase-Lev deque, implemented here with a mutex as a correctness-first
//! skeleton. The public shape is meant to survive the replacement with a
//! lock-free deque.
//!
//! The owner/thief distinction is documented, not type-enforced: both
//! [`WorkerDeque`] and [`Stealer`] share the same inner queue. A future version
//! may encode ownership in the type system or with a debug-only owner check.
//!
//! Synchronization currently comes from the inner mutex: a successful `lock()`
//! provides the happens-before edge between owner pushes and thief steals. No
//! atomics are used here; replacing this skeleton with a lock-free Chase-Lev
//! deque requires reworking owner/thief memory ordering (Acquire/Release and a
//! retry path for failed steals).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// The owner side of a work-stealing deque.
#[derive(Debug)]
pub struct WorkerDeque<T> {
    queue: Arc<Mutex<VecDeque<T>>>,
}

impl<T> WorkerDeque<T> {
    /// Creates an empty deque.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Pushes an item at the bottom (owner side).
    pub fn push(&self, item: T) {
        self.lock().push_back(item);
    }

    /// Pops the newest item from the bottom (owner side).
    pub fn pop(&self) -> Option<T> {
        self.lock().pop_back()
    }

    /// Creates a stealer handle for this deque.
    #[must_use]
    pub fn stealer(&self) -> Stealer<T> {
        Stealer {
            queue: Arc::clone(&self.queue),
        }
    }

    /// Number of queued items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether the deque is empty.
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

/// The thief side of a work-stealing deque.
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
    /// Steals the oldest item from the top, if any.
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

    /// Number of queued items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether the deque is empty.
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
