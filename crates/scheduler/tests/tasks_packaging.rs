//! Integration tests for [`Tasks`] packaging, state, and split policy.

use kagura_scheduler::{
    ContextId, EntryPoint, SplitPolicy, Task, TaskId, TaskKind, TaskState, Tasks, TasksAction,
    TasksError, TasksId, TasksState,
};

fn tasks(policy: SplitPolicy) -> Tasks {
    Tasks::new(TasksId::new(1), ContextId::new(2), policy)
}

fn task(id: u64) -> Task {
    Task::new(TaskId::new(id), TaskKind::Pure, EntryPoint::new(id))
}

fn ready_task(id: u64) -> Task {
    let mut task = task(id);
    task.make_ready().unwrap();
    task
}

#[test]
fn package_runs_to_done() {
    let mut tasks = tasks(SplitPolicy::Splittable)
        .with_affinity(3)
        .with_deadline(1_000);

    assert_eq!(tasks.id(), TasksId::new(1));
    assert_eq!(tasks.context(), ContextId::new(2));
    assert_eq!(tasks.split_policy(), SplitPolicy::Splittable);
    assert_eq!(tasks.affinity(), Some(3));
    assert_eq!(tasks.deadline(), Some(1_000));
    assert_eq!(tasks.state(), TasksState::Pending);
    assert!(tasks.is_empty());

    let task_id = tasks.push(ready_task(10)).unwrap();
    assert_eq!(task_id, TaskId::new(10));
    assert_eq!(
        tasks.task(task_id).unwrap().parent_tasks(),
        Some(tasks.id())
    );
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks.ready_tasks(), 1);
    assert_eq!(tasks.remaining_tasks(), 1);

    tasks.make_runnable().unwrap();
    assert_eq!(tasks.state(), TasksState::Runnable);
    tasks.begin_run().unwrap();
    assert_eq!(tasks.state(), TasksState::Running);

    tasks.task_mut(task_id).unwrap().start().unwrap();
    tasks.task_mut(task_id).unwrap().complete().unwrap();
    tasks.ledger_mut().record_fuel(12);
    tasks.ledger_mut().record_heap(64);
    tasks.ledger_mut().record_stack(128);
    assert_eq!(tasks.ledger().fuel_used(), 12);
    assert_eq!(tasks.ledger().heap_bytes(), 64);
    assert_eq!(tasks.ledger().stack_bytes(), 128);

    tasks.finish().unwrap();
    assert_eq!(tasks.state(), TasksState::Done);
    assert!(tasks.state().is_terminal());
}

#[test]
fn empty_package_cannot_run() {
    let mut tasks = tasks(SplitPolicy::Atomic);
    let error = tasks.make_runnable().unwrap_err();
    assert_eq!(
        error,
        TasksError::Empty {
            tasks: TasksId::new(1)
        }
    );
    assert_eq!(tasks.state(), TasksState::Pending);
}

#[test]
fn pushes_are_rejected_after_leaving_pending() {
    let mut tasks = tasks(SplitPolicy::Splittable);
    tasks.push(ready_task(1)).unwrap();
    tasks.make_runnable().unwrap();

    let error = tasks.push(ready_task(2)).unwrap_err();
    assert_eq!(
        error,
        TasksError::InvalidTransition {
            tasks: TasksId::new(1),
            from: TasksState::Runnable,
            action: TasksAction::Push,
        }
    );
}

#[test]
fn duplicate_and_terminal_tasks_are_rejected() {
    let mut tasks = tasks(SplitPolicy::Splittable);
    tasks.push(ready_task(1)).unwrap();

    let error = tasks.push(ready_task(1)).unwrap_err();
    assert_eq!(
        error,
        TasksError::DuplicateTask {
            tasks: TasksId::new(1),
            task: TaskId::new(1),
        }
    );

    let mut finished = task(2);
    finished.make_ready().unwrap();
    finished.start().unwrap();
    finished.complete().unwrap();
    let error = tasks.push(finished).unwrap_err();
    assert_eq!(
        error,
        TasksError::TaskTerminal {
            tasks: TasksId::new(1),
            task: TaskId::new(2),
        }
    );
}

#[test]
fn finish_requires_all_children_terminal() {
    let mut tasks = tasks(SplitPolicy::Splittable);
    tasks.push(ready_task(1)).unwrap();
    tasks.push(ready_task(2)).unwrap();
    tasks.make_runnable().unwrap();
    tasks.begin_run().unwrap();
    tasks.task_mut(TaskId::new(1)).unwrap().start().unwrap();
    tasks.task_mut(TaskId::new(1)).unwrap().complete().unwrap();

    let error = tasks.finish().unwrap_err();
    assert_eq!(
        error,
        TasksError::UnfinishedTasks {
            tasks: TasksId::new(1),
            remaining: 1,
        }
    );
    assert_eq!(tasks.state(), TasksState::Running);
    assert_eq!(tasks.remaining_tasks(), 1);
}

#[test]
fn cancel_propagates_to_non_terminal_children() {
    let mut tasks = tasks(SplitPolicy::Atomic);
    tasks.push(ready_task(1)).unwrap();
    tasks.push(ready_task(2)).unwrap();
    tasks.make_runnable().unwrap();
    tasks.begin_run().unwrap();
    tasks.task_mut(TaskId::new(1)).unwrap().start().unwrap();
    tasks.task_mut(TaskId::new(1)).unwrap().complete().unwrap();

    tasks
        .cancel(kagura_scheduler::CancelReason::AgentTerminated)
        .unwrap();
    assert_eq!(tasks.state(), TasksState::Cancelled);
    assert_eq!(
        tasks.task(TaskId::new(1)).unwrap().state(),
        TaskState::Completed
    );
    assert_eq!(
        tasks.task(TaskId::new(2)).unwrap().state(),
        TaskState::Cancelled
    );

    let error = tasks
        .cancel(kagura_scheduler::CancelReason::HostRequested)
        .unwrap_err();
    assert_eq!(
        error,
        TasksError::InvalidTransition {
            tasks: TasksId::new(1),
            from: TasksState::Cancelled,
            action: TasksAction::Cancel,
        }
    );
}

#[test]
fn atomic_package_cannot_split_a_task() {
    let mut tasks = tasks(SplitPolicy::Atomic);
    tasks.push(ready_task(1)).unwrap();
    tasks.push(ready_task(2)).unwrap();
    tasks.make_runnable().unwrap();

    let error = tasks.split_task().unwrap_err();
    assert_eq!(
        error,
        TasksError::TaskStealNotAllowed {
            tasks: TasksId::new(1),
            policy: SplitPolicy::Atomic,
        }
    );
    assert_eq!(tasks.len(), 2);
}

#[test]
fn splittable_package_hands_out_ready_tasks_in_order() {
    let mut tasks = tasks(SplitPolicy::Splittable);
    tasks.push(ready_task(1)).unwrap();
    tasks.push(ready_task(2)).unwrap();
    tasks.push(task(3)).unwrap();
    tasks.make_runnable().unwrap();

    let stolen = tasks.split_task().unwrap();
    assert_eq!(stolen.id(), TaskId::new(1));
    assert_eq!(stolen.parent_tasks(), Some(tasks.id()));
    assert_eq!(stolen.state(), TaskState::Ready);

    let stolen = tasks.split_task().unwrap();
    assert_eq!(stolen.id(), TaskId::new(2));

    // 只剩一个任务，不再拆分。
    let error = tasks.split_task().unwrap_err();
    assert_eq!(
        error,
        TasksError::NothingToSplit {
            tasks: TasksId::new(1)
        }
    );
}

#[test]
fn split_requires_runnable_package_and_ready_task() {
    let mut tasks = tasks(SplitPolicy::Pure);
    tasks.push(task(1)).unwrap();
    tasks.push(task(2)).unwrap();

    let error = tasks.split_task().unwrap_err();
    assert_eq!(
        error,
        TasksError::InvalidTransition {
            tasks: TasksId::new(1),
            from: TasksState::Pending,
            action: TasksAction::SplitTask,
        }
    );

    tasks.make_runnable().unwrap();
    let error = tasks.split_task().unwrap_err();
    assert_eq!(
        error,
        TasksError::NothingToSplit {
            tasks: TasksId::new(1)
        }
    );
}

#[test]
fn split_task_can_be_repackaged_and_gets_a_new_parent() {
    let mut first = tasks(SplitPolicy::Splittable);
    first.push(ready_task(1)).unwrap();
    first.push(ready_task(2)).unwrap();
    first.make_runnable().unwrap();
    let stolen = first.split_task().unwrap();

    let mut second = Tasks::new(TasksId::new(9), ContextId::new(2), SplitPolicy::Atomic);
    second.push(stolen).unwrap();
    assert_eq!(
        second.task(TaskId::new(1)).unwrap().parent_tasks(),
        Some(TasksId::new(9))
    );
    assert!(first.task(TaskId::new(1)).is_none());
}

#[test]
fn split_policy_names_and_steal_permission() {
    assert!(SplitPolicy::Splittable.allows_task_steal());
    assert!(SplitPolicy::Pure.allows_task_steal());
    assert!(!SplitPolicy::Atomic.allows_task_steal());
    assert_eq!(SplitPolicy::Atomic.name(), "atomic");
}
