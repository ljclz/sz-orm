//! 查询阶段耗时采集
//!
//! [`QueryTracer::trace_execute`] 包裹一次查询执行，通过 [`PhaseRecorder`]
//! 分阶段计时（`Instant::now()` 高精度），产出 [`QueryPhaseTiming`] 列表。

use std::cell::RefCell;
use std::time::Instant;

/// 查询执行阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 查询构造（SQL 生成）
    Build,
    /// 参数绑定
    Bind,
    /// 连接池获取连接
    PoolAcquire,
    /// SQL 执行
    SqlExecute,
    /// 结果映射
    ResultMap,
}

impl Phase {
    /// 阶段名（火焰图标注用）
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Build => "query.build",
            Phase::Bind => "query.bind",
            Phase::PoolAcquire => "pool.acquire",
            Phase::SqlExecute => "db.execute",
            Phase::ResultMap => "result.map",
        }
    }
}

/// 单阶段耗时记录
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPhaseTiming {
    /// 阶段
    pub phase: Phase,
    /// 相对起始毫秒（从 recorder 创建起）
    pub start_ms: u64,
    /// 阶段耗时毫秒
    pub duration_ms: u64,
}

/// 查询计时器（无锁，单线程使用）
pub struct QueryTracer;

impl QueryTracer {
    /// 包裹一次查询执行，返回 (结果, 各阶段耗时)
    ///
    /// ```rust
    /// use sz_orm_flamegraph::{Phase, QueryTracer};
    /// let (result, timings) = QueryTracer::trace_execute(|rec| {
    ///     rec.record(Phase::Build, || 1i32)
    /// });
    /// assert_eq!(result, 1);
    /// ```
    pub fn trace_execute<F, T>(f: F) -> (T, Vec<QueryPhaseTiming>)
    where
        F: FnOnce(&PhaseRecorder) -> T,
    {
        let recorder = PhaseRecorder::new();
        let out = f(&recorder);
        (out, recorder.finish())
    }

    /// 将阶段耗时写入既有 `sz-orm-tracing` 的 `Tracer` span（需 `query-flamegraph` feature）
    ///
    /// 每个阶段生成一个独立 span（`{span_name}.{phase}`），span 的生命周期
    /// 对应阶段执行区间。阶段耗时（`duration_ms`）通过返回的
    /// [`QueryPhaseTiming`] 列表关联，调用方可按需用 `inject` 提取 span 上下文。
    ///
    /// 注意：`Tracer` trait 无字段设置 API，本方法仅记录 span 边界
    /// （start/end），不写入 `duration_ms` 字段。如需关联耗时，
    /// 调用方应在 `end_span` 后自行用 timings 列表关联。
    #[cfg(feature = "query-flamegraph")]
    pub fn with_tracer(
        tracer: &dyn sz_orm_tracing::Tracer,
        span_name: &str,
        timings: &[QueryPhaseTiming],
    ) {
        for t in timings {
            let span = tracer.start_span(&format!("{span_name}.{}", t.phase.as_str()));
            tracer.end_span(span);
        }
    }
}

/// 阶段记录器（向闭包内传入，调用 `record` 包裹各阶段执行）
pub struct PhaseRecorder {
    started: Instant,
    timings: RefCell<Vec<QueryPhaseTiming>>,
}

impl Default for PhaseRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl PhaseRecorder {
    /// 创建记录器
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            timings: RefCell::new(Vec::new()),
        }
    }

    /// 记录一个阶段的执行并返回其结果
    pub fn record<R>(&self, phase: Phase, f: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let out = f();
        let duration_ms = start.elapsed().as_millis() as u64;
        let start_ms = start.duration_since(self.started).as_millis() as u64;
        self.timings.borrow_mut().push(QueryPhaseTiming {
            phase,
            start_ms,
            duration_ms,
        });
        out
    }

    /// 结束记录，返回全部阶段耗时
    pub fn finish(self) -> Vec<QueryPhaseTiming> {
        self.timings.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn records_all_phases() {
        let (sum, timings) = QueryTracer::trace_execute(|rec| {
            rec.record(Phase::Build, || 1u32) + rec.record(Phase::SqlExecute, || 2u32)
        });
        assert_eq!(sum, 3);
        assert_eq!(timings.len(), 2);
        assert_eq!(timings[0].phase, Phase::Build);
        assert_eq!(timings[1].phase, Phase::SqlExecute);
    }

    #[test]
    fn durations_are_monotonic() {
        let (_, timings) = QueryTracer::trace_execute(|rec| {
            rec.record(Phase::Build, || {
                thread::sleep(Duration::from_millis(2));
            });
            rec.record(Phase::Bind, || {
                thread::sleep(Duration::from_millis(3));
            });
        });
        assert_eq!(timings.len(), 2);
        // 第二阶段 start 不早于第一阶段
        assert!(timings[1].start_ms >= timings[0].start_ms);
        // 阶段耗时非零（有真实 sleep）
        assert!(timings[0].duration_ms >= 2);
        assert!(timings[1].duration_ms >= 3);
    }

    #[test]
    fn phase_names_are_unique_and_readable() {
        let names: Vec<&str> = [
            Phase::Build,
            Phase::Bind,
            Phase::PoolAcquire,
            Phase::SqlExecute,
            Phase::ResultMap,
        ]
        .iter()
        .map(|p| p.as_str())
        .collect();
        assert_eq!(names.len(), 5);
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 5, "phase names must be unique");
    }

    #[cfg(feature = "query-flamegraph")]
    #[test]
    fn with_tracer_accepts_timings() {
        use sz_orm_tracing::SzTracer;
        let tracer = SzTracer::new("test");
        let (_, timings) = QueryTracer::trace_execute(|rec| {
            rec.record(Phase::SqlExecute, || ());
        });
        // 不应 panic
        QueryTracer::with_tracer(&tracer, "q", &timings);
    }
}
