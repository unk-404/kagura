//! Integration tests for deterministic job and microtask ordering.

use std::cell::RefCell;
use std::rc::Rc;

use kagura_scheduler::{JobClass, JobError, JobQueue};

fn snapshot(log: &Rc<RefCell<Vec<&'static str>>>) -> Vec<&'static str> {
    log.borrow().clone()
}

#[test]
fn jobs_run_in_fifo_order() {
    let queue = JobQueue::new();
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let mut ids = Vec::new();

    for label in ["a", "b", "c"] {
        let log = Rc::clone(&log);
        ids.push(queue.enqueue_job(move || {
            log.borrow_mut().push(label);
            Ok(())
        }));
    }

    let report = queue.drain();
    assert_eq!(snapshot(&log), ["a", "b", "c"]);
    assert_eq!(report.executed_jobs(), 3);
    assert_eq!(report.executed_microtasks(), 0);
    assert_eq!(report.executed_total(), 3);
    assert!(report.is_ok());
    assert!(queue.is_empty());
    assert_eq!(ids[0].get() + 1, ids[1].get());
    assert_eq!(ids[1].get() + 1, ids[2].get());
}

#[test]
fn microtask_checkpoint_runs_after_each_job() {
    let queue = JobQueue::new();
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));

    let handle = queue.handle();
    let log_in_job = Rc::clone(&log);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("job-1");
        let micro_log = Rc::clone(&log_in_job);
        handle.enqueue_microtask(move || {
            micro_log.borrow_mut().push("micro-1a");
            Ok(())
        });
        let micro_log = Rc::clone(&log_in_job);
        handle.enqueue_microtask(move || {
            micro_log.borrow_mut().push("micro-1b");
            Ok(())
        });
        Ok(())
    });

    let log_in_job = Rc::clone(&log);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("job-2");
        Ok(())
    });

    let report = queue.drain();
    assert_eq!(snapshot(&log), ["job-1", "micro-1a", "micro-1b", "job-2"]);
    assert_eq!(report.executed_jobs(), 2);
    assert_eq!(report.executed_microtasks(), 2);
    assert!(report.is_ok());
}

#[test]
fn nested_microtasks_append_to_the_tail() {
    let queue = JobQueue::new();
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let handle = queue.handle();

    let log_in_job = Rc::clone(&log);
    let handle_in_job = handle.clone();
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("job");
        let micro_log = Rc::clone(&log_in_job);
        handle_in_job.enqueue_microtask(move || {
            micro_log.borrow_mut().push("micro-1");
            let nested_log = Rc::clone(&micro_log);
            // micro-1 内部再入队的任务排到当前 checkpoint 的尾部。
            handle.enqueue_microtask(move || {
                nested_log.borrow_mut().push("micro-3");
                Ok(())
            });
            Ok(())
        });
        let micro_log = Rc::clone(&log_in_job);
        handle_in_job.enqueue_microtask(move || {
            micro_log.borrow_mut().push("micro-2");
            Ok(())
        });
        Ok(())
    });

    let report = queue.drain();
    assert_eq!(snapshot(&log), ["job", "micro-1", "micro-2", "micro-3"]);
    assert_eq!(report.executed_microtasks(), 3);
    assert!(report.is_ok());
}

#[test]
fn pause_takes_effect_after_the_microtask_checkpoint() {
    let queue = JobQueue::new();
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let handle = queue.handle();

    let log_in_job = Rc::clone(&log);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("job-1");
        let micro_log = Rc::clone(&log_in_job);
        handle.enqueue_microtask(move || {
            micro_log.borrow_mut().push("micro-1");
            Ok(())
        });
        let micro_log = Rc::clone(&log_in_job);
        handle.enqueue_microtask(move || {
            micro_log.borrow_mut().push("micro-2");
            Ok(())
        });
        handle.pause();
        Ok(())
    });

    let log_in_job = Rc::clone(&log);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("job-2");
        Ok(())
    });

    let report = queue.drain();
    assert_eq!(snapshot(&log), ["job-1", "micro-1", "micro-2"]);
    assert!(report.is_paused());
    assert_eq!(report.executed_jobs(), 1);
    assert_eq!(report.executed_microtasks(), 2);
    assert_eq!(queue.jobs_len(), 1);

    queue.resume();
    let report = queue.drain();
    assert_eq!(snapshot(&log), ["job-1", "micro-1", "micro-2", "job-2"]);
    assert!(!report.is_paused());
    assert!(queue.is_empty());
}

#[test]
fn nested_drain_is_rejected_without_reordering() {
    let queue = Rc::new(JobQueue::new());
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let reentrant = Rc::new(RefCell::new(false));

    let queue_in_job = Rc::clone(&queue);
    let log_in_job = Rc::clone(&log);
    let flag = Rc::clone(&reentrant);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("job-1");
        let report = queue_in_job.drain();
        *flag.borrow_mut() = report.is_reentrant();
        assert_eq!(report.executed_total(), 0);
        Ok(())
    });

    let log_in_job = Rc::clone(&log);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("job-2");
        Ok(())
    });

    let report = queue.drain();
    assert!(!report.is_reentrant());
    assert_eq!(snapshot(&log), ["job-1", "job-2"]);
    assert!(*reentrant.borrow());
}

#[test]
fn paused_queue_does_not_run_until_resumed() {
    let queue = JobQueue::new();
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    queue.pause();

    let log_in_job = Rc::clone(&log);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("job");
        Ok(())
    });
    let log_in_microtask = Rc::clone(&log);
    queue.enqueue_microtask(move || {
        log_in_microtask.borrow_mut().push("microtask");
        Ok(())
    });

    let report = queue.drain();
    assert!(snapshot(&log).is_empty());
    assert!(report.is_paused());
    assert_eq!(report.executed_total(), 0);
    assert_eq!(queue.jobs_len(), 1);
    assert_eq!(queue.microtasks_len(), 1);

    queue.resume();
    let report = queue.drain();
    assert_eq!(snapshot(&log), ["job", "microtask"]);
    assert!(!report.is_paused());
    assert_eq!(report.executed_jobs(), 1);
    assert_eq!(report.executed_microtasks(), 1);
}

#[test]
fn failures_are_recorded_and_do_not_stop_later_work() {
    let queue = JobQueue::new();
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let handle = queue.handle();

    let failed_job = queue.enqueue_job(|| Err(JobError::new("job boom")));

    let log_in_job = Rc::clone(&log);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("enqueue-failing-microtask");
        handle.enqueue_microtask(|| Err(JobError::new("microtask boom")));
        Ok(())
    });

    let log_in_job = Rc::clone(&log);
    queue.enqueue_job(move || {
        log_in_job.borrow_mut().push("after-errors");
        Ok(())
    });

    let report = queue.drain();
    assert_eq!(
        snapshot(&log),
        ["enqueue-failing-microtask", "after-errors"]
    );
    assert_eq!(report.executed_jobs(), 3);
    assert_eq!(report.executed_microtasks(), 1);
    assert!(!report.is_ok());
    assert_eq!(report.failures().len(), 2);

    let first = report.first_failure().unwrap();
    assert_eq!(first.job(), failed_job);
    assert_eq!(first.class(), JobClass::Job);
    assert_eq!(first.error().message(), "job boom");

    let second = &report.failures()[1];
    assert_eq!(second.class(), JobClass::Microtask);
    assert_eq!(second.error().message(), "microtask boom");
    assert!(second.to_string().contains("microtask"));

    let failures = report.into_result().unwrap_err();
    assert_eq!(failures.len(), 2);
    assert!(queue.is_empty());
}

#[test]
fn explicit_microtask_checkpoint_runs_even_while_paused() {
    let queue = JobQueue::new();
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));

    for label in ["m1", "m2"] {
        let log = Rc::clone(&log);
        queue.enqueue_microtask(move || {
            log.borrow_mut().push(label);
            Ok(())
        });
    }

    let report = queue.drain_microtasks();
    assert_eq!(snapshot(&log), ["m1", "m2"]);
    assert_eq!(report.executed_microtasks(), 2);
    assert!(!report.is_paused());
    assert!(queue.is_empty());

    queue.pause();
    let log_in_microtask = Rc::clone(&log);
    queue.enqueue_microtask(move || {
        log_in_microtask.borrow_mut().push("m3");
        Ok(())
    });
    let report = queue.drain_microtasks();
    assert_eq!(snapshot(&log), ["m1", "m2", "m3"]);
    assert!(report.is_paused());
    assert!(queue.is_empty());
}

#[test]
fn bulk_microtasks_keep_fifo_order() {
    let queue = JobQueue::new();
    let log = Rc::new(RefCell::new(Vec::<usize>::new()));

    for index in 0..100 {
        let log = Rc::clone(&log);
        queue.enqueue_microtask(move || {
            log.borrow_mut().push(index);
            Ok(())
        });
    }

    let report = queue.drain();
    assert_eq!(*log.borrow(), (0..100).collect::<Vec<_>>());
    assert_eq!(report.executed_microtasks(), 100);
    assert!(report.is_ok());
}

#[test]
fn empty_queue_drain_is_a_no_op() {
    let queue = JobQueue::new();
    let report = queue.drain();
    assert!(report.is_ok());
    assert_eq!(report.executed_total(), 0);
    assert!(!report.is_paused());
    assert!(queue.is_empty());
}
