//! Integration tests for the [`Task`] lifecycle state machine.

use kagura_scheduler::{
    BudgetExhausted, CancelReason, ContinuationId, EffectDomain, EntryPoint, Task, TaskAction,
    TaskBudget, TaskId, TaskKind, TaskOutcome, TaskState,
};

fn task(kind: TaskKind) -> Task {
    Task::new(TaskId::new(1), kind, EntryPoint::new(7))
}

#[test]
fn full_lifecycle_with_suspend_and_resume() {
    let mut task = task(TaskKind::Effect);
    assert_eq!(task.state(), TaskState::Created);
    assert_eq!(task.parent_tasks(), None);
    assert_eq!(task.budget(), TaskBudget::new(0));
    assert!(task.effects().is_empty());

    task.make_ready().unwrap();
    assert_eq!(task.state(), TaskState::Ready);

    task.start().unwrap();
    assert_eq!(task.state(), TaskState::Running);

    task.suspend(ContinuationId::new(9)).unwrap();
    assert_eq!(task.state(), TaskState::Suspended);
    assert_eq!(task.continuation(), Some(ContinuationId::new(9)));

    task.resume().unwrap();
    assert_eq!(task.state(), TaskState::Ready);
    // continuation 在 resume 后仍保留，由 dispatcher 真正恢复时再取走。
    assert_eq!(task.continuation(), Some(ContinuationId::new(9)));

    task.start().unwrap();
    task.complete().unwrap();
    assert_eq!(task.state(), TaskState::Completed);
    assert_eq!(task.outcome(), Some(&TaskOutcome::Completed));
    assert!(task.is_terminal());
}

#[test]
fn invalid_transitions_report_state_and_action() {
    let mut task = task(TaskKind::Pure);

    let error = task.start().unwrap_err();
    assert_eq!(error.task(), TaskId::new(1));
    assert_eq!(error.from(), TaskState::Created);
    assert_eq!(error.action(), TaskAction::Start);
    assert!(error.to_string().contains("start"));
    assert!(error.to_string().contains("created"));

    task.make_ready().unwrap();
    let error = task.make_ready().unwrap_err();
    assert_eq!(error.from(), TaskState::Ready);
    assert_eq!(error.action(), TaskAction::MakeReady);

    let error = task.suspend(ContinuationId::new(1)).unwrap_err();
    assert_eq!(error.action(), TaskAction::Suspend);

    task.start().unwrap();
    let error = task.start().unwrap_err();
    assert_eq!(error.from(), TaskState::Running);
    assert_eq!(error.action(), TaskAction::Start);
}

#[test]
fn pure_task_restart_clears_continuation_and_outcome() {
    let mut task = task(TaskKind::Pure);
    task.make_ready().unwrap();
    task.start().unwrap();
    task.suspend(ContinuationId::new(4)).unwrap();

    task.restart().unwrap();
    assert_eq!(task.state(), TaskState::Ready);
    assert_eq!(task.continuation(), None);
    assert_eq!(task.outcome(), None);

    task.start().unwrap();
    task.fail("boom").unwrap();
    assert_eq!(task.state(), TaskState::Failed);
    assert_eq!(
        task.outcome(),
        Some(&TaskOutcome::Failed(kagura_scheduler::TaskFailure::new(
            "boom"
        )))
    );

    task.restart().unwrap();
    assert_eq!(task.state(), TaskState::Ready);
    assert_eq!(task.outcome(), None);
}

#[test]
fn effect_and_transactional_tasks_cannot_restart() {
    for kind in [TaskKind::Effect, TaskKind::Transactional] {
        let mut task = task(kind);
        task.make_ready().unwrap();
        task.start().unwrap();
        task.suspend(ContinuationId::new(2)).unwrap();

        let error = task.restart().unwrap_err();
        assert_eq!(error.action(), TaskAction::Restart);
        assert!(error.reason().contains("pure"));
        assert_eq!(task.state(), TaskState::Suspended);
    }
}

#[test]
fn restart_requires_suspended_or_failed() {
    let mut task = task(TaskKind::Pure);
    let error = task.restart().unwrap_err();
    assert_eq!(error.from(), TaskState::Created);
    assert_eq!(error.action(), TaskAction::Restart);
}

#[test]
fn cancellation_is_terminal() {
    let mut task = task(TaskKind::Effect);
    task.cancel(CancelReason::HostRequested).unwrap();
    assert_eq!(task.state(), TaskState::Cancelled);
    assert_eq!(
        task.outcome(),
        Some(&TaskOutcome::Cancelled(CancelReason::HostRequested))
    );

    let error = task.cancel(CancelReason::AbortSignal).unwrap_err();
    assert_eq!(error.action(), TaskAction::Cancel);
    assert_eq!(error.from(), TaskState::Cancelled);
}

#[test]
fn fuel_budget_charges_and_reports_exhaustion() {
    let mut budget = TaskBudget::new(10);
    assert_eq!(budget.limit(), 10);
    assert_eq!(budget.remaining(), 10);
    assert!(!budget.is_exhausted());

    budget.charge(4).unwrap();
    assert_eq!(budget.used(), 4);
    assert_eq!(budget.remaining(), 6);

    let error: BudgetExhausted = budget.charge(7).unwrap_err();
    assert_eq!(error.requested(), 7);
    assert_eq!(error.remaining(), 6);
}

#[test]
fn task_charge_fuel_delegates_to_budget() {
    let mut task = task(TaskKind::Pure).with_budget(TaskBudget::new(5));
    task.charge_fuel(3).unwrap();
    assert_eq!(task.budget().used(), 3);
    assert!(task.charge_fuel(3).is_err());
    assert_eq!(task.budget().used(), 3);
}

#[test]
fn effect_sets_are_sorted_and_unioned() {
    let mut effects = kagura_scheduler::EffectSet::new();
    assert!(effects.insert(EffectDomain::new(3)));
    assert!(effects.insert(EffectDomain::new(1)));
    assert!(!effects.insert(EffectDomain::new(3)));
    assert!(effects.contains(EffectDomain::new(1)));
    assert_eq!(effects.len(), 2);
    assert_eq!(
        effects.iter().collect::<Vec<_>>(),
        vec![EffectDomain::new(1), EffectDomain::new(3)]
    );

    let mut other =
        kagura_scheduler::EffectSet::from_domains([EffectDomain::new(2), EffectDomain::new(3)]);
    other.union_with(&effects);
    assert_eq!(other.len(), 3);
    assert!(!other.is_empty());
}

#[test]
fn ids_display_with_their_kind() {
    assert_eq!(TaskId::new(3).to_string(), "TaskId#3");
    assert_eq!(kagura_scheduler::TasksId::new(4).to_string(), "TasksId#4");
    assert_eq!(kagura_scheduler::RuId::new(2).to_string(), "RuId#2");
}

#[test]
fn task_and_package_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Task>();
    assert_send_sync::<kagura_scheduler::Tasks>();
}
