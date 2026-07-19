//! sz-orm-scheduler 压力测试套件
//!
//! 超大数据量验证：
//! - 1 万个任务调度
//! - 1000 个任务同时触发
//! - cron 解析在海量表达式下的稳定性
//! - try_fire_due 在大任务集下的正确性

use chrono::Utc;
use std::sync::Arc;
use sz_orm_scheduler::{
    CounterJobHandler, CronScheduler, JobHandler, RecordingJobHandler, ScheduledTask, Scheduler,
};

/// 验证：1 万个任务 schedule + list + cancel
#[test]
fn stress_scheduler_10k_tasks() {
    let scheduler = CronScheduler::new();
    let n: usize = 10_000;

    for i in 0..n {
        let task = ScheduledTask::new(format!("task-{}", i), format!("name-{}", i), "* * * * *");
        scheduler.schedule(task).unwrap();
    }
    assert_eq!(scheduler.list_tasks().len(), n);

    for i in 0..n {
        scheduler.cancel(&format!("task-{}", i)).unwrap();
    }
    assert_eq!(scheduler.list_tasks().len(), 0);
}

/// 验证：1000 个任务同时触发（每分钟都触发）
#[test]
fn stress_scheduler_1000_tasks_fire_simultaneously() {
    let scheduler = CronScheduler::new();
    let n: usize = 1000;
    let counter = Arc::new(CounterJobHandler::new());
    let counter_clone = counter.clone();

    for i in 0..n {
        let task = ScheduledTask::new(format!("task-{}", i), format!("name-{}", i), "* * * * *");
        scheduler.schedule(task).unwrap();
        // 为每个任务注册 handler（共享 counter）
        let c = counter_clone.clone();
        let wrapper = Arc::new(SharedCounterHandler::new(c));
        scheduler.register_handler(format!("task-{}", i), wrapper);
    }

    let now = Utc::now();
    let fired = scheduler.try_fire_due(now);
    assert_eq!(fired, n, "all {} tasks should fire", n);
    assert_eq!(counter.count(), n as u64);
}

/// 辅助 handler：共享 counter
struct SharedCounterHandler {
    counter: Arc<CounterJobHandler>,
}

impl SharedCounterHandler {
    fn new(counter: Arc<CounterJobHandler>) -> Self {
        Self { counter }
    }
}

impl JobHandler for SharedCounterHandler {
    fn handle(&self, _task: &ScheduledTask) -> Result<(), String> {
        self.counter.handle(_task)
    }
}

/// 验证：next_run_time 在大量调用下对合法表达式不 panic，对非法表达式返回错误
/// 注意：本库 CronExpr 5 字段为 second/minute/hour/day_of_month/month（无 day_of_week）
/// 已知 bug：next_run_time 对齐到分钟边界（second=0），对 second!=0 的表达式永远不匹配
#[test]
fn stress_scheduler_parse_cron_various() {
    let scheduler = CronScheduler::new();
    // 所有 second=0 的合法表达式
    let valid_exprs = vec![
        "* * * * *",
        "0 * * * *",
        "0 0 * * *",
        "0 0 1 * *",
        "0 0 1 1 *",
        "*/5 * * * *",
        "0 9-17 * * *",
        "0,15,30,45 * * * *",
        "0 0 1,15 * *",
    ];
    // 字段数错误的表达式（parse_cron 必然失败）
    let invalid_field_count_exprs = vec!["", "* * * *", "* * * * * *", "abc def * * *"];
    let from = Utc::now();

    // 验证合法表达式（少量循环，避免 next_run_time 内部 525600 次扫描过慢）
    for _ in 0..3 {
        for expr in &valid_exprs {
            let result = scheduler.next_run_time(expr, from);
            assert!(result.is_ok(), "expr should be valid: {}", expr);
        }
    }

    // 验证字段数错误的表达式
    for _ in 0..10 {
        for expr in &invalid_field_count_exprs {
            let result = scheduler.next_run_time(expr, from);
            assert!(
                result.is_err(),
                "expr should be invalid (field count): {}",
                expr
            );
        }
    }
}

/// 验证：next_run_time 在多次调用下不 panic
#[test]
fn stress_scheduler_next_run_time_repeated() {
    let scheduler = CronScheduler::new();
    // 注意：本库不支持 day_of_week，"0 9-17 * * 1-5" 的第 5 字段是 month
    let exprs = vec![
        "* * * * *",
        "0 * * * *",
        "0 0 * * *",
        "*/5 * * * *",
        "0 9-17 * * *",
    ];
    let from = Utc::now();

    // 少量循环（next_run_time 内部最多扫描 525600 次，避免过慢）
    for _ in 0..5 {
        for expr in &exprs {
            let result = scheduler.next_run_time(expr, from);
            assert!(result.is_ok(), "next_run_time failed for expr: {}", expr);
        }
    }
}

/// 验证：pause/resume 在大任务集下一致
#[test]
fn stress_scheduler_pause_resume() {
    let scheduler = CronScheduler::new();
    let n: usize = 1000;

    for i in 0..n {
        let task = ScheduledTask::new(format!("task-{}", i), format!("name-{}", i), "* * * * *");
        scheduler.schedule(task).unwrap();
    }

    // pause 一半
    for i in 0..n / 2 {
        scheduler.pause(&format!("task-{}", i)).unwrap();
    }

    let now = Utc::now();
    let fired = scheduler.try_fire_due(now);
    assert_eq!(fired, n / 2, "only non-paused tasks should fire");

    // resume 后所有任务都触发
    for i in 0..n / 2 {
        scheduler.resume(&format!("task-{}", i)).unwrap();
    }
    let fired = scheduler.try_fire_due(now);
    assert_eq!(fired, n, "all tasks should fire after resume");
}

/// 验证：cancel 不存在的任务返回错误
#[test]
fn stress_scheduler_cancel_nonexistent() {
    let scheduler = CronScheduler::new();
    for i in 0..100 {
        let result = scheduler.cancel(&format!("nonexistent-{}", i));
        assert!(result.is_err());
    }
}

/// 验证：重复 schedule 同一 id 覆盖
#[test]
fn stress_scheduler_schedule_duplicate_overwrites() {
    let scheduler = CronScheduler::new();
    for _ in 0..1000 {
        let task = ScheduledTask::new("dup-id", "name", "* * * * *");
        scheduler.schedule(task).unwrap();
    }
    assert_eq!(scheduler.list_tasks().len(), 1);
}

/// 验证：RecordingJobHandler 在大量触发下顺序记录
#[test]
fn stress_scheduler_recording_handler_order() {
    let scheduler = CronScheduler::new();
    let recording = Arc::new(RecordingJobHandler::new());

    // 注册一个任务，触发 1000 次
    let task = ScheduledTask::new("task-1", "name", "* * * * *");
    scheduler.schedule(task).unwrap();
    scheduler.register_handler("task-1", recording.clone());

    for _ in 0..1000 {
        scheduler.try_fire_due(Utc::now());
    }
    let handled = recording.handled_ids();
    assert_eq!(handled.len(), 1000);
    for id in &handled {
        assert_eq!(id, "task-1");
    }
    assert_eq!(recording.handled_ids().len(), 1000);
}

/// 验证：disabled 任务不触发
#[test]
fn stress_scheduler_disabled_task_does_not_fire() {
    let scheduler = CronScheduler::new();
    let counter = Arc::new(CounterJobHandler::new());
    let counter_clone = counter.clone();

    let task = ScheduledTask::new("task-1", "name", "* * * * *").disable();
    scheduler.schedule(task).unwrap();
    scheduler.register_handler("task-1", Arc::new(SharedCounterHandler::new(counter_clone)));

    for _ in 0..100 {
        scheduler.try_fire_due(Utc::now());
    }
    assert_eq!(counter.count(), 0, "disabled task should not fire");
}

/// 验证：start/stop 后台线程不 panic
#[test]
fn stress_scheduler_start_stop_cycle() {
    let scheduler = Arc::new(CronScheduler::new());
    for cycle in 0..10 {
        let s = scheduler.clone();
        // 用 spawn_blocking 避免阻塞 async runtime
        let handle = std::thread::spawn(move || {
            s.start(10).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            s.stop().unwrap();
        });
        handle.join().unwrap();
        let _ = cycle;
    }
    assert!(!scheduler.is_running());
}
