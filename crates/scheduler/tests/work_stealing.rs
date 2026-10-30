//! Integration tests for the injection queue, work-stealing deque, RU, and the
//! two-thread smoke path.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use kagura_scheduler::{Injector, Ru, RuId, RuPool, RuState, WorkerDeque};

#[test]
fn injector_is_fifo() {
    let injector = Injector::new();
    for item in 0..4 {
        injector.push(item);
    }
    assert_eq!(injector.len(), 4);
    assert!(!injector.is_empty());
    assert_eq!(injector.try_pop(), Some(0));
    assert_eq!(injector.try_pop(), Some(1));
    injector.push(4);
    assert_eq!(injector.try_pop(), Some(2));
    assert_eq!(injector.try_pop(), Some(3));
    assert_eq!(injector.try_pop(), Some(4));
    assert_eq!(injector.try_pop(), None);
    assert!(injector.is_empty());
    assert_eq!(injector.pushed(), 5);
    assert_eq!(injector.popped(), 5);
}

#[test]
fn injector_is_thread_safe_and_loses_nothing() {
    const ITEMS: usize = 512;
    let injector = Arc::new(Injector::new());
    let producer = {
        let injector = Arc::clone(&injector);
        std::thread::spawn(move || {
            for item in 0..ITEMS {
                injector.push(item);
            }
        })
    };

    let mut collected = Vec::with_capacity(ITEMS);
    while collected.len() < ITEMS {
        if let Some(item) = injector.try_pop() {
            collected.push(item);
        } else {
            std::thread::yield_now();
        }
    }
    producer.join().unwrap();

    assert_eq!(collected, (0..ITEMS).collect::<Vec<_>>());
    assert_eq!(injector.pushed(), u64::try_from(ITEMS).unwrap());
    assert_eq!(injector.popped(), u64::try_from(ITEMS).unwrap());
    assert!(injector.is_empty());
}

#[test]
fn owner_pops_newest_and_thief_steals_oldest() {
    let deque: WorkerDeque<u32> = WorkerDeque::new();
    deque.push(1);
    deque.push(2);
    deque.push(3);

    let stealer = deque.stealer();
    let cloned = stealer.clone();
    assert_eq!(stealer.steal(), Some(1));
    assert_eq!(deque.pop(), Some(3));
    assert_eq!(cloned.steal(), Some(2));
    assert_eq!(stealer.steal(), None);
    assert!(deque.is_empty());
}

#[test]
fn steal_batch_preserves_fifo_order() {
    let deque: WorkerDeque<u32> = WorkerDeque::new();
    for item in 1..=4 {
        deque.push(item);
    }
    let stealer = deque.stealer();
    assert_eq!(stealer.steal_batch(2), vec![1, 2]);
    assert_eq!(stealer.steal_batch(0), Vec::<u32>::new());
    assert_eq!(deque.pop(), Some(4));
    assert_eq!(stealer.steal_batch(10), vec![3]);
    assert!(stealer.is_empty());
    assert_eq!(stealer.len(), 0);
}

#[test]
fn ru_state_machine_rejects_illegal_transitions() {
    let ru = Ru::<u32>::new(RuId::new(0));
    assert_eq!(ru.state(), RuState::Idle);

    ru.park().unwrap();
    assert_eq!(ru.state(), RuState::Parked);
    let error = ru.begin_running().unwrap_err();
    assert_eq!(error.ru(), RuId::new(0));
    assert_eq!(error.from(), RuState::Parked);
    assert_eq!(error.to(), RuState::Running);

    ru.unpark().unwrap();
    ru.begin_running().unwrap();
    assert_eq!(ru.state(), RuState::Running);
    let error = ru.begin_running().unwrap_err();
    assert_eq!(error.from(), RuState::Running);

    ru.finish_running().unwrap();
    assert_eq!(ru.state(), RuState::Idle);
    ru.retire().unwrap();
    assert_eq!(ru.state(), RuState::Retiring);
    assert!(ru.retire().is_err());
}

#[test]
fn ru_tracks_local_and_steal_counters() {
    let owner = Ru::<u32>::new(RuId::new(0));
    let thief = Ru::<u32>::new(RuId::new(1));

    owner.push_local(1);
    owner.push_local(2);
    thief.push_local(3);

    assert_eq!(thief.try_steal_from(&owner), Some(1));
    assert_eq!(owner.pop_local(), Some(2));
    assert_eq!(owner.pop_local(), None);
    assert_eq!(thief.pop_local(), Some(3));
    assert_eq!(thief.try_steal_from(&owner), None);
    thief.record_processed();

    let owner_stats = owner.stats();
    assert_eq!(owner_stats.local_pushes, 2);
    assert_eq!(owner_stats.local_pops, 1);
    assert_eq!(owner_stats.steals_attempted, 0);
    assert_eq!(owner_stats.items_processed, 0);

    let thief_stats = thief.stats();
    assert_eq!(thief_stats.local_pushes, 1);
    assert_eq!(thief_stats.local_pops, 1);
    assert_eq!(thief_stats.steals_attempted, 2);
    assert_eq!(thief_stats.steals_succeeded, 1);
    assert_eq!(thief_stats.items_processed, 1);
}

#[test]
fn ru_pool_spawn_is_fallible_and_shares_one_injector() {
    let mut pool: RuPool<u32> = RuPool::new(2);
    assert!(pool.is_empty());
    assert_eq!(pool.len(), 0);
    assert_eq!(pool.max_rus(), 2);

    let first = pool.try_spawn_ru().unwrap();
    let second = pool.try_spawn_ru().unwrap();
    assert_eq!(first.id(), RuId::new(0));
    assert_eq!(second.id(), RuId::new(1));
    assert_eq!(pool.len(), 2);
    assert_eq!(pool.rus()[0].id(), RuId::new(0));

    let error = pool.try_spawn_ru().unwrap_err();
    assert_eq!(error.limit(), 2);
    assert_eq!(pool.len(), 2);

    pool.injector().push(7);
    assert_eq!(pool.injector().try_pop(), Some(7));

    let mut zero_limit: RuPool<u32> = RuPool::new(0);
    let error = zero_limit.try_spawn_ru().unwrap_err();
    assert_eq!(error.limit(), 0);
    assert!(zero_limit.is_empty());
}

struct SmokeResult {
    items: Vec<usize>,
    processed: u64,
    injected_pushed: u64,
    injected_popped: u64,
}

const SMOKE_ITEMS: usize = 2_000;

fn worker(
    me: Arc<Ru<usize>>,
    peer: Arc<Ru<usize>>,
    injector: Arc<Injector<usize>>,
    remaining: Arc<AtomicUsize>,
    collected: Arc<Mutex<Vec<usize>>>,
) {
    while remaining.load(Ordering::Acquire) > 0 {
        let next = me
            .pop_local()
            .or_else(|| injector.try_pop())
            .or_else(|| me.try_steal_from(peer.as_ref()));
        if let Some(item) = next {
            me.record_processed();
            collected
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(item);
            remaining.fetch_sub(1, Ordering::AcqRel);
        } else {
            // 队列暂时为空；让出时间片而不是 sleep，避免测试依赖墙钟时间。
            std::thread::yield_now();
        }
    }
}

fn run_two_ru_smoke() -> SmokeResult {
    let injector = Arc::new(Injector::new());
    let ru_a = Arc::new(Ru::<usize>::new(RuId::new(0)));
    let ru_b = Arc::new(Ru::<usize>::new(RuId::new(1)));

    // 初始分布固定：一半注入全局队列，其余按奇偶分给两个 RU 的本地 deque。
    for item in 0..SMOKE_ITEMS {
        match item % 4 {
            0 | 2 => injector.push(item),
            1 => ru_a.push_local(item),
            _ => ru_b.push_local(item),
        }
    }

    let remaining = Arc::new(AtomicUsize::new(SMOKE_ITEMS));
    let collected = Arc::new(Mutex::new(Vec::with_capacity(SMOKE_ITEMS)));

    let worker_a = {
        let me = Arc::clone(&ru_a);
        let peer = Arc::clone(&ru_b);
        let injector = Arc::clone(&injector);
        let remaining = Arc::clone(&remaining);
        let collected = Arc::clone(&collected);
        std::thread::spawn(move || worker(me, peer, injector, remaining, collected))
    };
    let worker_b = {
        let me = Arc::clone(&ru_b);
        let peer = Arc::clone(&ru_a);
        let injector = Arc::clone(&injector);
        let remaining = Arc::clone(&remaining);
        let collected = Arc::clone(&collected);
        std::thread::spawn(move || worker(me, peer, injector, remaining, collected))
    };

    worker_a.join().unwrap();
    worker_b.join().unwrap();

    let mut items = collected
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    items.sort_unstable();

    SmokeResult {
        items,
        processed: ru_a.stats().items_processed + ru_b.stats().items_processed,
        injected_pushed: injector.pushed(),
        injected_popped: injector.popped(),
    }
}

#[test]
fn two_threads_process_every_item_exactly_once_and_repeatably() {
    let expected: Vec<usize> = (0..SMOKE_ITEMS).collect();
    let expected_u64 = u64::try_from(SMOKE_ITEMS).unwrap();

    let first = run_two_ru_smoke();
    assert_eq!(first.items, expected);
    assert_eq!(first.items.len(), SMOKE_ITEMS);
    assert_eq!(first.processed, expected_u64);
    assert_eq!(first.injected_pushed, first.injected_popped);

    // 再跑一次：分布可能不同，但“总数/集合”必须可复现。
    let second = run_two_ru_smoke();
    assert_eq!(second.items, expected);
    assert_eq!(second.processed, first.processed);
    assert_eq!(second.injected_pushed, first.injected_pushed);
    assert_eq!(second.injected_popped, first.injected_popped);
}
