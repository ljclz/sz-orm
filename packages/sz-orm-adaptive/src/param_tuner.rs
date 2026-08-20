//! 自适应参数调优器
//!
//! 根据运行时统计自动调优配置参数（阈值、批大小、缓存 TTL 等），
//! 使优化器随负载变化自我进化，无需人工介入。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// 调优参数标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TunableParam {
    /// 分页阈值（平均行数超过此值则切换分页）
    PaginationThreshold,
    /// 缓存触发最小执行次数
    CacheMinExecutions,
    /// 缓存触发慢查询阈值（毫秒）
    CacheSlowQueryMs,
    /// 批大小下限
    BatchSizeMin,
    /// 批大小上限
    BatchSizeMax,
    /// 缓存 TTL（秒）
    CacheTtlSecs,
    /// 慢查询标记阈值（毫秒）
    SlowQueryThresholdMs,
}

impl TunableParam {
    /// 返回参数的默认值
    pub fn default_value(self) -> u64 {
        match self {
            TunableParam::PaginationThreshold => 1000,
            TunableParam::CacheMinExecutions => 5,
            TunableParam::CacheSlowQueryMs => 100,
            TunableParam::BatchSizeMin => 10,
            TunableParam::BatchSizeMax => 1000,
            TunableParam::CacheTtlSecs => 300,
            TunableParam::SlowQueryThresholdMs => 200,
        }
    }

    /// 返回参数的最小允许值
    pub fn min_value(self) -> u64 {
        match self {
            TunableParam::PaginationThreshold => 100,
            TunableParam::CacheMinExecutions => 1,
            TunableParam::CacheSlowQueryMs => 10,
            TunableParam::BatchSizeMin => 1,
            TunableParam::BatchSizeMax => 10,
            TunableParam::CacheTtlSecs => 10,
            TunableParam::SlowQueryThresholdMs => 10,
        }
    }

    /// 返回参数的最大允许值
    pub fn max_value(self) -> u64 {
        match self {
            TunableParam::PaginationThreshold => 100_000,
            TunableParam::CacheMinExecutions => 100,
            TunableParam::CacheSlowQueryMs => 10_000,
            TunableParam::BatchSizeMin => 100,
            TunableParam::BatchSizeMax => 100_000,
            TunableParam::CacheTtlSecs => 3600,
            TunableParam::SlowQueryThresholdMs => 60_000,
        }
    }

    /// 参数的简短名称（用于日志/序列化）
    pub fn name(self) -> &'static str {
        match self {
            TunableParam::PaginationThreshold => "pagination_threshold",
            TunableParam::CacheMinExecutions => "cache_min_executions",
            TunableParam::CacheSlowQueryMs => "cache_slow_query_ms",
            TunableParam::BatchSizeMin => "batch_size_min",
            TunableParam::BatchSizeMax => "batch_size_max",
            TunableParam::CacheTtlSecs => "cache_ttl_secs",
            TunableParam::SlowQueryThresholdMs => "slow_query_threshold_ms",
        }
    }

    /// 返回所有可调参数
    pub fn all() -> &'static [TunableParam] {
        &[
            TunableParam::PaginationThreshold,
            TunableParam::CacheMinExecutions,
            TunableParam::CacheSlowQueryMs,
            TunableParam::BatchSizeMin,
            TunableParam::BatchSizeMax,
            TunableParam::CacheTtlSecs,
            TunableParam::SlowQueryThresholdMs,
        ]
    }
}

/// 调优策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum TuningStrategy {
    /// 保守策略：每次只调整 5%
    Conservative,
    /// 平衡策略：每次调整 15%
    #[default]
    Balanced,
    /// 激进策略：每次调整 30%
    Aggressive,
}


impl TuningStrategy {
    /// 返回调整步长比例（0.0 ~ 1.0）
    pub fn step_ratio(self) -> f64 {
        match self {
            TuningStrategy::Conservative => 0.05,
            TuningStrategy::Balanced => 0.15,
            TuningStrategy::Aggressive => 0.30,
        }
    }

    /// 返回策略名称
    pub fn name(self) -> &'static str {
        match self {
            TuningStrategy::Conservative => "conservative",
            TuningStrategy::Balanced => "balanced",
            TuningStrategy::Aggressive => "aggressive",
        }
    }
}

/// 调优信号：指示参数应朝哪个方向调整
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningSignal {
    /// 增大参数
    Increase,
    /// 减小参数
    Decrease,
    /// 保持不变
    Hold,
}

impl TuningSignal {
    /// 信号取反
    pub fn invert(self) -> Self {
        match self {
            TuningSignal::Increase => TuningSignal::Decrease,
            TuningSignal::Decrease => TuningSignal::Increase,
            TuningSignal::Hold => TuningSignal::Hold,
        }
    }

    /// 信号强度权重（用于聚合多信号）
    pub fn weight(self) -> f64 {
        match self {
            TuningSignal::Increase => 1.0,
            TuningSignal::Decrease => -1.0,
            TuningSignal::Hold => 0.0,
        }
    }
}

/// 调优事件记录
#[derive(Debug, Clone)]
pub struct TuningEvent {
    /// 被调优的参数
    pub param: TunableParam,
    /// 调优前的值
    pub old_value: u64,
    /// 调优后的值
    pub new_value: u64,
    /// 触发信号
    pub signal: TuningSignal,
    /// 调优原因
    pub reason: String,
    /// 事件时间戳（自 epoch 的毫秒数）
    pub timestamp_ms: u64,
}

impl TuningEvent {
    /// 创建新的调优事件
    pub fn new(
        param: TunableParam,
        old_value: u64,
        new_value: u64,
        signal: TuningSignal,
        reason: impl Into<String>,
    ) -> Self {
        let timestamp_ms = current_ms();
        Self {
            param,
            old_value,
            new_value,
            signal,
            reason: reason.into(),
            timestamp_ms,
        }
    }

    /// 返回值变化量（有符号）
    pub fn delta(&self) -> i64 {
        self.new_value as i64 - self.old_value as i64
    }

    /// 返回值变化百分比
    pub fn delta_pct(&self) -> f64 {
        if self.old_value == 0 {
            0.0
        } else {
            (self.new_value as f64 - self.old_value as f64) / self.old_value as f64 * 100.0
        }
    }

    /// 是否为有效调优（值确实发生了变化）
    pub fn is_effective(&self) -> bool {
        self.old_value != self.new_value
    }
}

/// 调优统计
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct TuningStats {
    /// 总调优次数
    pub total_tunings: u64,
    /// 有效调优次数（值实际变化）
    pub effective_tunings: u64,
    /// 各参数调优次数
    pub per_param_counts: HashMap<TunableParam, u64>,
    /// 最近调优时间戳
    pub last_tuning_ms: u64,
}


impl TuningStats {
    /// 有效调优比率
    pub fn effective_ratio(&self) -> f64 {
        if self.total_tunings == 0 {
            0.0
        } else {
            self.effective_tunings as f64 / self.total_tunings as f64
        }
    }

    /// 返回指定参数的调优次数
    pub fn count_for(&self, param: TunableParam) -> u64 {
        self.per_param_counts.get(&param).copied().unwrap_or(0)
    }

    /// 记录一次调优
    pub fn record(&mut self, event: &TuningEvent) {
        self.total_tunings += 1;
        if event.is_effective() {
            self.effective_tunings += 1;
        }
        *self.per_param_counts.entry(event.param).or_insert(0) += 1;
        self.last_tuning_ms = event.timestamp_ms;
    }
}

/// 自适应参数调优器
pub struct AdaptiveParameterTuner {
    /// 当前参数值
    values: HashMap<TunableParam, u64>,
    /// 调优策略
    strategy: TuningStrategy,
    /// 调优统计
    stats: Mutex<TuningStats>,
    /// 调优事件历史（环形缓冲，最多保留 max_history 条）
    history: Mutex<Vec<TuningEvent>>,
    /// 最大历史记录数
    max_history: usize,
    /// 调优次数计数器
    tuning_counter: AtomicU64,
}

impl std::fmt::Debug for AdaptiveParameterTuner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdaptiveParameterTuner")
            .field("values", &self.values)
            .field("strategy", &self.strategy)
            .field("max_history", &self.max_history)
            .finish()
    }
}

impl Default for AdaptiveParameterTuner {
    fn default() -> Self {
        Self::new(TuningStrategy::default(), 100)
    }
}

impl AdaptiveParameterTuner {
    /// 创建新的调优器
    pub fn new(strategy: TuningStrategy, max_history: usize) -> Self {
        let mut values = HashMap::new();
        for &param in TunableParam::all() {
            values.insert(param, param.default_value());
        }
        Self {
            values,
            strategy,
            stats: Mutex::new(TuningStats::default()),
            history: Mutex::new(Vec::with_capacity(max_history)),
            max_history,
            tuning_counter: AtomicU64::new(0),
        }
    }

    /// 获取参数当前值
    pub fn get(&self, param: TunableParam) -> u64 {
        self.values
            .get(&param)
            .copied()
            .unwrap_or_else(|| param.default_value())
    }

    /// 手动设置参数值（会被 clamp 到合法范围）
    pub fn set(&self, param: TunableParam, value: u64) -> u64 {
        
        clamp_value(param, value)
    }

    /// 返回当前策略
    pub fn strategy(&self) -> TuningStrategy {
        self.strategy
    }

    /// 根据信号调优指定参数
    pub fn tune(
        &self,
        param: TunableParam,
        signal: TuningSignal,
        reason: impl Into<String>,
    ) -> TuningEvent {
        let old_value = self.get(param);
        let new_value = apply_signal(param, old_value, signal, self.strategy);
        let event = TuningEvent::new(param, old_value, new_value, signal, reason);

        if new_value != old_value {
            // 值发生变化，记录事件
            self.record_event(&event);
        } else {
            // 值未变化（被 clamp 或 Hold），仍记录统计
            self.record_stats_only(&event);
        }

        self.tuning_counter.fetch_add(1, Ordering::Relaxed);
        event
    }

    /// 批量调优：根据多个信号聚合后调优多个参数
    pub fn tune_batch(
        &self,
        signals: &[(TunableParam, TuningSignal)],
        reason: impl Into<String>,
    ) -> Vec<TuningEvent> {
        let reason_str: String = reason.into();
        signals
            .iter()
            .map(|(param, signal)| self.tune(*param, *signal, reason_str.clone()))
            .collect()
    }

    /// 获取调优统计快照
    pub fn stats(&self) -> TuningStats {
        let guard = self.stats.lock().expect("stats mutex poisoned");
        guard.clone()
    }

    /// 获取调优历史快照
    pub fn history(&self) -> Vec<TuningEvent> {
        let guard = self.history.lock().expect("history mutex poisoned");
        guard.clone()
    }

    /// 返回总调优次数
    pub fn total_tunings(&self) -> u64 {
        self.tuning_counter.load(Ordering::Relaxed)
    }

    /// 重置指定参数到默认值
    pub fn reset_param(&self, param: TunableParam) -> TuningEvent {
        let old_value = self.get(param);
        let new_value = param.default_value();
        let event = TuningEvent::new(
            param,
            old_value,
            new_value,
            TuningSignal::Hold,
            "reset to default",
        );
        self.record_event(&event);
        event
    }

    /// 重置所有参数到默认值
    pub fn reset_all(&self) -> Vec<TuningEvent> {
        TunableParam::all()
            .iter()
            .map(|&p| self.reset_param(p))
            .collect()
    }

    /// 返回所有参数当前值的快照
    pub fn snapshot(&self) -> HashMap<TunableParam, u64> {
        self.values.clone()
    }

    /// 根据平均行数和执行时间自动产生调优信号
    pub fn auto_signal_from_metrics(
        &self,
        avg_rows: u64,
        avg_time_ms: u64,
    ) -> HashMap<TunableParam, TuningSignal> {
        let mut signals = HashMap::new();
        let pag_threshold = self.get(TunableParam::PaginationThreshold);

        // 平均行数远超阈值 → 增大阈值（减少不必要的分页切换）
        if avg_rows > pag_threshold * 3 {
            signals.insert(TunableParam::PaginationThreshold, TuningSignal::Increase);
        } else if avg_rows < pag_threshold / 3 && avg_rows > 0 {
            signals.insert(TunableParam::PaginationThreshold, TuningSignal::Decrease);
        }

        // 慢查询阈值调优
        let slow_threshold = self.get(TunableParam::SlowQueryThresholdMs);
        if avg_time_ms > slow_threshold * 2 {
            signals.insert(TunableParam::SlowQueryThresholdMs, TuningSignal::Increase);
        } else if avg_time_ms > 0 && avg_time_ms < slow_threshold / 4 {
            signals.insert(TunableParam::SlowQueryThresholdMs, TuningSignal::Decrease);
        }

        // 缓存 TTL 调优：高频慢查询 → 增大 TTL
        if avg_time_ms > self.get(TunableParam::CacheSlowQueryMs) {
            signals.insert(TunableParam::CacheTtlSecs, TuningSignal::Increase);
        }

        signals
    }

    /// 根据自动信号执行调优
    pub fn auto_tune(&self, avg_rows: u64, avg_time_ms: u64) -> Vec<TuningEvent> {
        let signals = self.auto_signal_from_metrics(avg_rows, avg_time_ms);
        let signal_list: Vec<(TunableParam, TuningSignal)> = signals.into_iter().collect();
        self.tune_batch(&signal_list, "auto_tune from metrics")
    }

    /// 返回参数是否在合法范围内
    pub fn is_in_range(&self, param: TunableParam, value: u64) -> bool {
        (param.min_value()..=param.max_value()).contains(&value)
    }

    fn record_event(&self, event: &TuningEvent) {
        {
            let mut stats = self.stats.lock().expect("stats mutex poisoned");
            stats.record(event);
        }
        {
            let mut history = self.history.lock().expect("history mutex poisoned");
            if history.len() >= self.max_history {
                history.remove(0);
            }
            history.push(event.clone());
        }
    }

    fn record_stats_only(&self, event: &TuningEvent) {
        let mut stats = self.stats.lock().expect("stats mutex poisoned");
        stats.record(event);
    }
}

/// 将值 clamp 到参数的合法范围
fn clamp_value(param: TunableParam, value: u64) -> u64 {
    let min = param.min_value();
    let max = param.max_value();
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// 根据信号和策略计算新值
fn apply_signal(
    param: TunableParam,
    current: u64,
    signal: TuningSignal,
    strategy: TuningStrategy,
) -> u64 {
    if signal == TuningSignal::Hold {
        return current;
    }

    let step_ratio = strategy.step_ratio();
    let delta = (current as f64 * step_ratio).round() as u64;
    let delta = delta.max(1); // 至少变化 1

    let raw_new = match signal {
        TuningSignal::Increase => current.saturating_add(delta),
        TuningSignal::Decrease => current.saturating_sub(delta),
        TuningSignal::Hold => current,
    };

    clamp_value(param, raw_new)
}

/// 获取当前时间戳（毫秒）
fn current_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 调优建议生成器：根据统计产生人类可读建议
pub struct TuningAdvisor {
    /// 调优器引用快照
    snapshot: HashMap<TunableParam, u64>,
    /// 统计快照
    stats: TuningStats,
}

impl TuningAdvisor {
    /// 从调优器创建建议生成器
    pub fn from_tuner(tuner: &AdaptiveParameterTuner) -> Self {
        Self {
            snapshot: tuner.snapshot(),
            stats: tuner.stats(),
        }
    }

    /// 生成建议列表
    pub fn suggestions(&self) -> Vec<TuningSuggestion> {
        let mut list = Vec::new();

        for &param in TunableParam::all() {
            let current = self
                .snapshot
                .get(&param)
                .copied()
                .unwrap_or_else(|| param.default_value());
            let default = param.default_value();
            let count = self.stats.count_for(param);

            if count == 0 {
                continue;
            }

            let deviation = if default == 0 {
                0.0
            } else {
                (current as f64 - default as f64) / default as f64 * 100.0
            };

            let severity = if deviation.abs() < 10.0 {
                SuggestionSeverity::Info
            } else if deviation.abs() < 30.0 {
                SuggestionSeverity::Warning
            } else {
                SuggestionSeverity::Critical
            };

            let msg = format!(
                "{}: current={}, default={}, deviation={:.1}%, tunings={}",
                param.name(),
                current,
                default,
                deviation,
                count
            );

            list.push(TuningSuggestion {
                param,
                current_value: current,
                default_value: default,
                deviation_pct: deviation,
                tuning_count: count,
                severity,
                message: msg,
            });
        }

        list.sort_by_key(|s| std::cmp::Reverse(s.tuning_count));
        list
    }

    /// 返回偏离默认值最大的参数
    pub fn most_deviated(&self) -> Option<TuningSuggestion> {
        self.suggestions().into_iter().max_by(|a, b| {
            a.deviation_pct
                .abs()
                .partial_cmp(&b.deviation_pct.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// 返回调优最频繁的参数
    pub fn most_tuned(&self) -> Option<TuningSuggestion> {
        self.suggestions()
            .into_iter()
            .max_by_key(|s| s.tuning_count)
    }

    /// 返回需要关注的参数（severity >= Warning）
    pub fn critical_suggestions(&self) -> Vec<TuningSuggestion> {
        self.suggestions()
            .into_iter()
            .filter(|s| s.severity == SuggestionSeverity::Critical)
            .collect()
    }
}

/// 建议严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SuggestionSeverity {
    Info,
    Warning,
    Critical,
}

impl SuggestionSeverity {
    /// 返回级别名称
    pub fn name(self) -> &'static str {
        match self {
            SuggestionSeverity::Info => "info",
            SuggestionSeverity::Warning => "warning",
            SuggestionSeverity::Critical => "critical",
        }
    }
}

/// 调优建议
#[derive(Debug, Clone)]
pub struct TuningSuggestion {
    /// 参数
    pub param: TunableParam,
    /// 当前值
    pub current_value: u64,
    /// 默认值
    pub default_value: u64,
    /// 偏离百分比
    pub deviation_pct: f64,
    /// 调优次数
    pub tuning_count: u64,
    /// 严重级别
    pub severity: SuggestionSeverity,
    /// 人类可读消息
    pub message: String,
}

impl TuningSuggestion {
    /// 是否偏离默认值
    pub fn is_deviated(&self) -> bool {
        self.current_value != self.default_value
    }

    /// 返回是否需要重置（偏离超过 50%）
    pub fn needs_reset(&self) -> bool {
        self.deviation_pct.abs() > 50.0
    }
}

/// 参数调优计划：一系列预定义的调优步骤
#[derive(Debug, Clone)]
pub struct TuningPlan {
    /// 计划名称
    pub name: String,
    /// 计划步骤
    pub steps: Vec<TuningPlanStep>,
    /// 计划创建时间
    pub created_ms: u64,
}

/// 调优计划步骤
#[derive(Debug, Clone)]
pub struct TuningPlanStep {
    /// 目标参数
    pub param: TunableParam,
    /// 目标信号
    pub signal: TuningSignal,
    /// 步骤描述
    pub description: String,
}

impl TuningPlan {
    /// 创建新的空计划
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
            created_ms: current_ms(),
        }
    }

    /// 添加步骤
    pub fn add_step(
        &mut self,
        param: TunableParam,
        signal: TuningSignal,
        description: impl Into<String>,
    ) -> &mut Self {
        self.steps.push(TuningPlanStep {
            param,
            signal,
            description: description.into(),
        });
        self
    }

    /// 返回步骤数
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// 是否为空计划
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// 执行计划
    pub fn execute(&self, tuner: &AdaptiveParameterTuner) -> Vec<TuningEvent> {
        self.steps
            .iter()
            .map(|step| tuner.tune(step.param, step.signal, step.description.clone()))
            .collect()
    }

    /// 预设计划：高负载场景
    pub fn high_load_plan() -> Self {
        let mut plan = Self::new("high_load");
        plan.add_step(
            TunableParam::PaginationThreshold,
            TuningSignal::Increase,
            "high avg rows: raise pagination threshold",
        );
        plan.add_step(
            TunableParam::BatchSizeMax,
            TuningSignal::Increase,
            "high load: allow larger batches",
        );
        plan.add_step(
            TunableParam::CacheTtlSecs,
            TuningSignal::Increase,
            "high load: extend cache TTL",
        );
        plan
    }

    /// 预设计划：低负载场景
    pub fn low_load_plan() -> Self {
        let mut plan = Self::new("low_load");
        plan.add_step(
            TunableParam::PaginationThreshold,
            TuningSignal::Decrease,
            "low avg rows: lower pagination threshold",
        );
        plan.add_step(
            TunableParam::BatchSizeMax,
            TuningSignal::Decrease,
            "low load: reduce max batch size",
        );
        plan.add_step(
            TunableParam::CacheTtlSecs,
            TuningSignal::Decrease,
            "low load: shorten cache TTL",
        );
        plan
    }

    /// 预设计划：慢查询场景
    pub fn slow_query_plan() -> Self {
        let mut plan = Self::new("slow_query");
        plan.add_step(
            TunableParam::SlowQueryThresholdMs,
            TuningSignal::Increase,
            "queries slower than threshold: raise threshold",
        );
        plan.add_step(
            TunableParam::CacheMinExecutions,
            TuningSignal::Decrease,
            "slow queries: lower cache trigger threshold",
        );
        plan.add_step(
            TunableParam::CacheTtlSecs,
            TuningSignal::Increase,
            "slow queries: extend cache TTL",
        );
        plan
    }
}

/// 调优效果评估器：对比调优前后的性能指标
#[derive(Debug, Clone)]
pub struct TuningImpactEvaluator {
    /// 调优前指标
    pub before: PerformanceMetrics,
    /// 调优后指标
    pub after: PerformanceMetrics,
}

/// 性能指标快照
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// 平均查询时间（毫秒）
    pub avg_query_ms: f64,
    /// P95 查询时间（毫秒）
    pub p95_query_ms: f64,
    /// 每秒查询数
    pub qps: f64,
    /// 缓存命中率
    pub cache_hit_rate: f64,
    /// 分页查询比例
    pub pagination_ratio: f64,
}

impl PerformanceMetrics {
    /// 创建新的性能指标
    pub fn new(
        avg_query_ms: f64,
        p95_query_ms: f64,
        qps: f64,
        cache_hit_rate: f64,
        pagination_ratio: f64,
    ) -> Self {
        Self {
            avg_query_ms,
            p95_query_ms,
            qps,
            cache_hit_rate,
            pagination_ratio,
        }
    }

    /// 综合评分（越高越好）
    pub fn score(&self) -> f64 {
        let latency_score = if self.avg_query_ms > 0.0 {
            1000.0 / self.avg_query_ms
        } else {
            0.0
        };
        let p95_score = if self.p95_query_ms > 0.0 {
            1000.0 / self.p95_query_ms
        } else {
            0.0
        };
        latency_score * 0.3 + p95_score * 0.2 + self.qps * 0.3 + self.cache_hit_rate * 100.0 * 0.2
    }
}

impl TuningImpactEvaluator {
    /// 创建效果评估器
    pub fn new(before: PerformanceMetrics, after: PerformanceMetrics) -> Self {
        Self { before, after }
    }

    /// 平均查询时间改善百分比（正数表示改善）
    pub fn avg_latency_improvement(&self) -> f64 {
        if self.before.avg_query_ms <= 0.0 {
            return 0.0;
        }
        (self.before.avg_query_ms - self.after.avg_query_ms) / self.before.avg_query_ms * 100.0
    }

    /// P95 延迟改善百分比
    pub fn p95_latency_improvement(&self) -> f64 {
        if self.before.p95_query_ms <= 0.0 {
            return 0.0;
        }
        (self.before.p95_query_ms - self.after.p95_query_ms) / self.before.p95_query_ms * 100.0
    }

    /// QPS 提升百分比
    pub fn qps_improvement(&self) -> f64 {
        if self.before.qps <= 0.0 {
            return 0.0;
        }
        (self.after.qps - self.before.qps) / self.before.qps * 100.0
    }

    /// 缓存命中率变化（百分点）
    pub fn cache_hit_rate_delta(&self) -> f64 {
        self.after.cache_hit_rate - self.before.cache_hit_rate
    }

    /// 综合评分变化
    pub fn score_delta(&self) -> f64 {
        self.after.score() - self.before.score()
    }

    /// 是否为正向调优（综合评分提升）
    pub fn is_positive(&self) -> bool {
        self.score_delta() > 0.0
    }

    /// 生成评估报告
    pub fn report(&self) -> String {
        format!(
            "Tuning Impact Report:\n  avg latency: {:.1}%\n  p95 latency: {:.1}%\n  QPS: {:.1}%\n  cache hit rate: {:+.1}pp\n  score delta: {:+.2}\n  verdict: {}",
            self.avg_latency_improvement(),
            self.p95_latency_improvement(),
            self.qps_improvement(),
            self.cache_hit_rate_delta(),
            self.score_delta(),
            if self.is_positive() { "POSITIVE" } else { "NEGATIVE" }
        )
    }

    /// 返回调优效果持续时间估算
    pub fn estimated_duration(&self) -> Duration {
        if self.is_positive() {
            Duration::from_secs(300)
        } else {
            Duration::from_secs(60)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunable_param_default_values() {
        assert_eq!(TunableParam::PaginationThreshold.default_value(), 1000);
        assert_eq!(TunableParam::CacheMinExecutions.default_value(), 5);
        assert_eq!(TunableParam::CacheSlowQueryMs.default_value(), 100);
        assert_eq!(TunableParam::BatchSizeMin.default_value(), 10);
        assert_eq!(TunableParam::BatchSizeMax.default_value(), 1000);
        assert_eq!(TunableParam::CacheTtlSecs.default_value(), 300);
        assert_eq!(TunableParam::SlowQueryThresholdMs.default_value(), 200);
    }

    #[test]
    fn tunable_param_min_max_ordering() {
        for &param in TunableParam::all() {
            assert!(param.min_value() <= param.default_value());
            assert!(param.default_value() <= param.max_value());
        }
    }

    #[test]
    fn tunable_param_names_distinct() {
        let names: Vec<&str> = TunableParam::all().iter().map(|p| p.name()).collect();
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                assert_ne!(names[i], names[j]);
            }
        }
    }

    #[test]
    fn tunable_param_all_count() {
        assert_eq!(TunableParam::all().len(), 7);
    }

    #[test]
    fn tuning_strategy_step_ratios() {
        assert!(TuningStrategy::Conservative.step_ratio() < TuningStrategy::Balanced.step_ratio());
        assert!(TuningStrategy::Balanced.step_ratio() < TuningStrategy::Aggressive.step_ratio());
    }

    #[test]
    fn tuning_strategy_default_is_balanced() {
        assert_eq!(TuningStrategy::default(), TuningStrategy::Balanced);
    }

    #[test]
    fn tuning_strategy_names() {
        assert_eq!(TuningStrategy::Conservative.name(), "conservative");
        assert_eq!(TuningStrategy::Balanced.name(), "balanced");
        assert_eq!(TuningStrategy::Aggressive.name(), "aggressive");
    }

    #[test]
    fn tuning_signal_invert() {
        assert_eq!(TuningSignal::Increase.invert(), TuningSignal::Decrease);
        assert_eq!(TuningSignal::Decrease.invert(), TuningSignal::Increase);
        assert_eq!(TuningSignal::Hold.invert(), TuningSignal::Hold);
    }

    #[test]
    fn tuning_signal_weights() {
        assert!(TuningSignal::Increase.weight() > 0.0);
        assert!(TuningSignal::Decrease.weight() < 0.0);
        assert_eq!(TuningSignal::Hold.weight(), 0.0);
    }

    #[test]
    fn tuning_event_delta() {
        let event = TuningEvent::new(
            TunableParam::BatchSizeMin,
            10,
            15,
            TuningSignal::Increase,
            "test",
        );
        assert_eq!(event.delta(), 5);
        assert!(event.delta_pct() > 0.0);
        assert!(event.is_effective());
    }

    #[test]
    fn tuning_event_no_delta() {
        let event = TuningEvent::new(
            TunableParam::BatchSizeMin,
            10,
            10,
            TuningSignal::Hold,
            "no change",
        );
        assert_eq!(event.delta(), 0);
        assert_eq!(event.delta_pct(), 0.0);
        assert!(!event.is_effective());
    }

    #[test]
    fn tuning_event_delta_pct_zero_old() {
        let event = TuningEvent::new(
            TunableParam::BatchSizeMin,
            0,
            10,
            TuningSignal::Increase,
            "from zero",
        );
        assert_eq!(event.delta_pct(), 0.0);
    }

    #[test]
    fn tuning_stats_default() {
        let stats = TuningStats::default();
        assert_eq!(stats.total_tunings, 0);
        assert_eq!(stats.effective_tunings, 0);
        assert_eq!(stats.effective_ratio(), 0.0);
    }

    #[test]
    fn tuning_stats_record() {
        let mut stats = TuningStats::default();
        let event1 = TuningEvent::new(
            TunableParam::BatchSizeMin,
            10,
            15,
            TuningSignal::Increase,
            "up",
        );
        let event2 = TuningEvent::new(
            TunableParam::BatchSizeMin,
            15,
            15,
            TuningSignal::Hold,
            "hold",
        );
        stats.record(&event1);
        stats.record(&event2);
        assert_eq!(stats.total_tunings, 2);
        assert_eq!(stats.effective_tunings, 1);
        assert_eq!(stats.count_for(TunableParam::BatchSizeMin), 2);
        assert_eq!(stats.count_for(TunableParam::CacheTtlSecs), 0);
        assert!((stats.effective_ratio() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn tuner_default() {
        let tuner = AdaptiveParameterTuner::default();
        for &param in TunableParam::all() {
            assert_eq!(tuner.get(param), param.default_value());
        }
    }

    #[test]
    fn tuner_get_returns_default_for_unset() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 10);
        assert_eq!(tuner.get(TunableParam::PaginationThreshold), 1000);
    }

    #[test]
    fn tuner_set_clamps_to_range() {
        let tuner = AdaptiveParameterTuner::default();
        let clamped = tuner.set(TunableParam::PaginationThreshold, 1);
        assert_eq!(clamped, TunableParam::PaginationThreshold.min_value());

        let clamped_high = tuner.set(TunableParam::PaginationThreshold, u64::MAX);
        assert_eq!(clamped_high, TunableParam::PaginationThreshold.max_value());
    }

    #[test]
    fn tuner_tune_increase() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 10);
        let old = tuner.get(TunableParam::BatchSizeMin);
        let event = tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Increase, "test");
        assert!(event.new_value > old);
        assert!(event.is_effective());
        assert_eq!(tuner.total_tunings(), 1);
    }

    #[test]
    fn tuner_tune_decrease() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 10);
        let old = tuner.get(TunableParam::BatchSizeMax);
        let event = tuner.tune(TunableParam::BatchSizeMax, TuningSignal::Decrease, "test");
        assert!(event.new_value < old);
        assert!(event.is_effective());
    }

    #[test]
    fn tuner_tune_hold_no_change() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 10);
        let event = tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Hold, "hold");
        assert!(!event.is_effective());
        assert_eq!(event.old_value, event.new_value);
    }

    #[test]
    fn tuner_tune_clamped_at_max() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Aggressive, 10);
        let max = TunableParam::BatchSizeMax.max_value();
        // 多次增大直到触顶
        for _ in 0..100 {
            tuner.tune(
                TunableParam::BatchSizeMax,
                TuningSignal::Increase,
                "push up",
            );
        }
        let last = tuner.tune(
            TunableParam::BatchSizeMax,
            TuningSignal::Increase,
            "push up",
        );
        assert!(last.new_value <= max);
    }

    #[test]
    fn tuner_tune_clamped_at_min() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Aggressive, 10);
        let min = TunableParam::BatchSizeMin.min_value();
        for _ in 0..100 {
            tuner.tune(
                TunableParam::BatchSizeMin,
                TuningSignal::Decrease,
                "push down",
            );
        }
        let last = tuner.tune(
            TunableParam::BatchSizeMin,
            TuningSignal::Decrease,
            "push down",
        );
        assert!(last.new_value >= min);
    }

    #[test]
    fn tuner_batch() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 10);
        let signals = vec![
            (TunableParam::BatchSizeMin, TuningSignal::Increase),
            (TunableParam::CacheTtlSecs, TuningSignal::Decrease),
        ];
        let events = tuner.tune_batch(&signals, "batch test");
        assert_eq!(events.len(), 2);
        assert_eq!(tuner.total_tunings(), 2);
    }

    #[test]
    fn tuner_stats_tracking() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 10);
        tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Increase, "1");
        tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Increase, "2");
        tuner.tune(TunableParam::CacheTtlSecs, TuningSignal::Decrease, "3");
        let stats = tuner.stats();
        assert_eq!(stats.total_tunings, 3);
        assert_eq!(stats.count_for(TunableParam::BatchSizeMin), 2);
        assert_eq!(stats.count_for(TunableParam::CacheTtlSecs), 1);
    }

    #[test]
    fn tuner_history_capped() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 3);
        for i in 0..10 {
            tuner.tune(
                TunableParam::BatchSizeMin,
                TuningSignal::Increase,
                format!("tune {}", i),
            );
        }
        let history = tuner.history();
        assert!(history.len() <= 3);
    }

    #[test]
    fn tuner_reset_param() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Aggressive, 10);
        for _ in 0..5 {
            tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Increase, "up");
        }
        let event = tuner.reset_param(TunableParam::BatchSizeMin);
        assert_eq!(event.new_value, TunableParam::BatchSizeMin.default_value());
    }

    #[test]
    fn tuner_reset_all() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Aggressive, 10);
        tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Increase, "up");
        tuner.tune(TunableParam::CacheTtlSecs, TuningSignal::Decrease, "down");
        let events = tuner.reset_all();
        assert_eq!(events.len(), TunableParam::all().len());
    }

    #[test]
    fn tuner_snapshot() {
        let tuner = AdaptiveParameterTuner::default();
        let snap = tuner.snapshot();
        assert_eq!(snap.len(), TunableParam::all().len());
    }

    #[test]
    fn tuner_is_in_range() {
        let tuner = AdaptiveParameterTuner::default();
        let param = TunableParam::BatchSizeMin;
        assert!(tuner.is_in_range(param, param.default_value()));
        assert!(tuner.is_in_range(param, param.min_value()));
        assert!(tuner.is_in_range(param, param.max_value()));
        assert!(!tuner.is_in_range(param, 0));
        assert!(!tuner.is_in_range(param, u64::MAX));
    }

    #[test]
    fn tuner_auto_signal_high_rows() {
        let tuner = AdaptiveParameterTuner::default();
        let threshold = tuner.get(TunableParam::PaginationThreshold);
        let signals = tuner.auto_signal_from_metrics(threshold * 5, 50);
        assert_eq!(
            signals.get(&TunableParam::PaginationThreshold),
            Some(&TuningSignal::Increase)
        );
    }

    #[test]
    fn tuner_auto_signal_low_rows() {
        let tuner = AdaptiveParameterTuner::default();
        let threshold = tuner.get(TunableParam::PaginationThreshold);
        let signals = tuner.auto_signal_from_metrics(threshold / 10, 50);
        assert_eq!(
            signals.get(&TunableParam::PaginationThreshold),
            Some(&TuningSignal::Decrease)
        );
    }

    #[test]
    fn tuner_auto_signal_slow_query() {
        let tuner = AdaptiveParameterTuner::default();
        let slow = tuner.get(TunableParam::SlowQueryThresholdMs);
        let signals = tuner.auto_signal_from_metrics(100, slow * 3);
        assert_eq!(
            signals.get(&TunableParam::SlowQueryThresholdMs),
            Some(&TuningSignal::Increase)
        );
    }

    #[test]
    fn tuner_auto_tune() {
        let tuner = AdaptiveParameterTuner::default();
        let events = tuner.auto_tune(5000, 500);
        // 高行数和高时间应触发调优
        assert!(!events.is_empty());
    }

    #[test]
    fn clamp_value_basic() {
        let param = TunableParam::BatchSizeMin;
        assert_eq!(clamp_value(param, 0), param.min_value());
        assert_eq!(clamp_value(param, 50), 50);
        assert_eq!(clamp_value(param, u64::MAX), param.max_value());
    }

    #[test]
    fn apply_signal_hold() {
        let param = TunableParam::BatchSizeMin;
        assert_eq!(
            apply_signal(param, 50, TuningSignal::Hold, TuningStrategy::Balanced),
            50
        );
    }

    #[test]
    fn apply_signal_increase_grows() {
        let param = TunableParam::BatchSizeMax;
        let result = apply_signal(param, 500, TuningSignal::Increase, TuningStrategy::Balanced);
        assert!(result > 500);
    }

    #[test]
    fn apply_signal_decrease_shrinks() {
        let param = TunableParam::BatchSizeMax;
        let result = apply_signal(param, 500, TuningSignal::Decrease, TuningStrategy::Balanced);
        assert!(result < 500);
    }

    #[test]
    fn tuning_advisor_from_tuner() {
        let tuner = AdaptiveParameterTuner::default();
        let advisor = TuningAdvisor::from_tuner(&tuner);
        let suggestions = advisor.suggestions();
        // 未调优过，suggestions 应为空
        assert!(suggestions.is_empty());
    }

    #[test]
    fn tuning_advisor_after_tuning() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Aggressive, 10);
        for _ in 0..5 {
            tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Increase, "up");
        }
        let advisor = TuningAdvisor::from_tuner(&tuner);
        let suggestions = advisor.suggestions();
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn tuning_advisor_most_deviated() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Aggressive, 10);
        for _ in 0..10 {
            tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Increase, "up");
        }
        let advisor = TuningAdvisor::from_tuner(&tuner);
        let most = advisor.most_deviated();
        assert!(most.is_some());
    }

    #[test]
    fn tuning_advisor_most_tuned() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 10);
        for _ in 0..5 {
            tuner.tune(TunableParam::CacheTtlSecs, TuningSignal::Increase, "up");
        }
        tuner.tune(TunableParam::BatchSizeMin, TuningSignal::Increase, "up");
        let advisor = TuningAdvisor::from_tuner(&tuner);
        let most = advisor.most_tuned();
        assert!(most.is_some());
        assert_eq!(most.unwrap().param, TunableParam::CacheTtlSecs);
    }

    #[test]
    fn suggestion_severity_ordering() {
        assert!(SuggestionSeverity::Info < SuggestionSeverity::Warning);
        assert!(SuggestionSeverity::Warning < SuggestionSeverity::Critical);
    }

    #[test]
    fn suggestion_severity_names() {
        assert_eq!(SuggestionSeverity::Info.name(), "info");
        assert_eq!(SuggestionSeverity::Warning.name(), "warning");
        assert_eq!(SuggestionSeverity::Critical.name(), "critical");
    }

    #[test]
    fn tuning_suggestion_needs_reset() {
        let s = TuningSuggestion {
            param: TunableParam::BatchSizeMin,
            current_value: 100,
            default_value: 10,
            deviation_pct: 90.0,
            tuning_count: 5,
            severity: SuggestionSeverity::Critical,
            message: "test".to_string(),
        };
        assert!(s.needs_reset());
        assert!(s.is_deviated());
    }

    #[test]
    fn tuning_plan_new_empty() {
        let plan = TuningPlan::new("test");
        assert_eq!(plan.name, "test");
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
    }

    #[test]
    fn tuning_plan_add_step() {
        let mut plan = TuningPlan::new("test");
        plan.add_step(TunableParam::BatchSizeMin, TuningSignal::Increase, "step1");
        assert_eq!(plan.len(), 1);
        assert!(!plan.is_empty());
    }

    #[test]
    fn tuning_plan_execute() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Balanced, 10);
        let mut plan = TuningPlan::new("test");
        plan.add_step(TunableParam::BatchSizeMin, TuningSignal::Increase, "step1");
        plan.add_step(TunableParam::CacheTtlSecs, TuningSignal::Decrease, "step2");
        let events = plan.execute(&tuner);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn tuning_plan_high_load() {
        let plan = TuningPlan::high_load_plan();
        assert!(!plan.is_empty());
        assert_eq!(plan.name, "high_load");
    }

    #[test]
    fn tuning_plan_low_load() {
        let plan = TuningPlan::low_load_plan();
        assert!(!plan.is_empty());
        assert_eq!(plan.name, "low_load");
    }

    #[test]
    fn tuning_plan_slow_query() {
        let plan = TuningPlan::slow_query_plan();
        assert!(!plan.is_empty());
        assert_eq!(plan.name, "slow_query");
    }

    #[test]
    fn performance_metrics_score() {
        let m = PerformanceMetrics::new(100.0, 200.0, 50.0, 0.5, 0.1);
        let score = m.score();
        assert!(score > 0.0);
    }

    #[test]
    fn performance_metrics_score_zero_latency() {
        let m = PerformanceMetrics::new(0.0, 0.0, 10.0, 0.0, 0.0);
        let score = m.score();
        assert!(score > 0.0);
    }

    #[test]
    fn impact_evaluator_positive() {
        let before = PerformanceMetrics::new(100.0, 200.0, 50.0, 0.3, 0.1);
        let after = PerformanceMetrics::new(50.0, 100.0, 80.0, 0.6, 0.2);
        let evaluator = TuningImpactEvaluator::new(before, after);
        assert!(evaluator.avg_latency_improvement() > 0.0);
        assert!(evaluator.p95_latency_improvement() > 0.0);
        assert!(evaluator.qps_improvement() > 0.0);
        assert!(evaluator.cache_hit_rate_delta() > 0.0);
        assert!(evaluator.is_positive());
    }

    #[test]
    fn impact_evaluator_negative() {
        let before = PerformanceMetrics::new(50.0, 100.0, 80.0, 0.6, 0.2);
        let after = PerformanceMetrics::new(100.0, 200.0, 50.0, 0.3, 0.1);
        let evaluator = TuningImpactEvaluator::new(before, after);
        assert!(evaluator.avg_latency_improvement() < 0.0);
        assert!(!evaluator.is_positive());
    }

    #[test]
    fn impact_evaluator_report() {
        let before = PerformanceMetrics::new(100.0, 200.0, 50.0, 0.3, 0.1);
        let after = PerformanceMetrics::new(50.0, 100.0, 80.0, 0.6, 0.2);
        let evaluator = TuningImpactEvaluator::new(before, after);
        let report = evaluator.report();
        assert!(report.contains("Tuning Impact Report"));
        assert!(report.contains("POSITIVE"));
    }

    #[test]
    fn impact_evaluator_zero_before() {
        let before = PerformanceMetrics::default();
        let after = PerformanceMetrics::new(50.0, 100.0, 80.0, 0.6, 0.2);
        let evaluator = TuningImpactEvaluator::new(before, after);
        assert_eq!(evaluator.avg_latency_improvement(), 0.0);
        assert_eq!(evaluator.p95_latency_improvement(), 0.0);
    }

    #[test]
    fn impact_evaluator_estimated_duration() {
        let before = PerformanceMetrics::new(100.0, 200.0, 50.0, 0.3, 0.1);
        let after = PerformanceMetrics::new(50.0, 100.0, 80.0, 0.6, 0.2);
        let evaluator = TuningImpactEvaluator::new(before, after);
        assert!(evaluator.is_positive());
        assert_eq!(evaluator.estimated_duration(), Duration::from_secs(300));
    }

    #[test]
    fn impact_evaluator_negative_duration() {
        let before = PerformanceMetrics::new(50.0, 100.0, 80.0, 0.6, 0.2);
        let after = PerformanceMetrics::new(100.0, 200.0, 50.0, 0.3, 0.1);
        let evaluator = TuningImpactEvaluator::new(before, after);
        assert!(!evaluator.is_positive());
        assert_eq!(evaluator.estimated_duration(), Duration::from_secs(60));
    }

    #[test]
    fn tuner_strategy_getter() {
        let tuner = AdaptiveParameterTuner::new(TuningStrategy::Aggressive, 10);
        assert_eq!(tuner.strategy(), TuningStrategy::Aggressive);
    }

    #[test]
    fn conservative_strategy_small_steps() {
        let tuner_cons = AdaptiveParameterTuner::new(TuningStrategy::Conservative, 10);
        let tuner_aggr = AdaptiveParameterTuner::new(TuningStrategy::Aggressive, 10);

        let event_cons =
            tuner_cons.tune(TunableParam::BatchSizeMax, TuningSignal::Increase, "cons");
        let event_aggr =
            tuner_aggr.tune(TunableParam::BatchSizeMax, TuningSignal::Increase, "aggr");

        assert!(event_cons.delta() < event_aggr.delta());
    }
}
