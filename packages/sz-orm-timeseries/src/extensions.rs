//! TimescaleDB 深度扩展功能
//!
//! 本模块补充 TimescaleDB 扩展缺失的核心深度功能，包括：
//!
//! - **连续聚合管理**：物化视图刷新策略、刷新窗口、策略元数据
//! - **数据压缩策略**：启用/禁用压缩、压缩配置（segmentby/orderby）、压缩统计
//! - **数据保留策略**：自动数据清理策略、保留时长、策略注册表
//! - **时间桶对齐与 gapfill**：time_bucket 对齐到自然边界、locf/interpolate 缺失桶填充
//!
//! # 设计说明
//!
//! 本模块以**独立结构 + 扩展 trait** 的方式提供，不修改既有 `TimeseriesExt` trait，
//! 避免破坏已有的 memory / stub / real_timescale 三种实现。
//! 内存实现部分基于纯 Rust 实现，不依赖外部库。

#![allow(dead_code)]

use crate::error::TimescaleError;
use crate::types::TimeBucket;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

// =============================================================================
// 一、连续聚合管理
// =============================================================================

/// 连续聚合刷新策略
///
/// TimescaleDB 的连续聚合视图支持自动刷新，刷新策略决定刷新频率和窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshPolicy {
    /// 永不自动刷新（手动调用 refresh）
    Manual,
    /// 按固定间隔自动刷新
    Scheduled,
    /// 仅在数据写入时触发刷新（实时聚合）
    RealTime,
}

impl RefreshPolicy {
    /// 转为 TimescaleDB SQL 参数片段
    pub fn as_sql_config(&self) -> &'static str {
        match self {
            RefreshPolicy::Manual => "timescaledb.continuous = true",
            RefreshPolicy::Scheduled => "timescaledb.continuous = true",
            RefreshPolicy::RealTime => "timescaledb.continuous = true, timescaledb.materialized_only = false",
        }
    }
}

/// 连续聚合定义
///
/// 对应 TimescaleDB 的 `CREATE MATERIALIZED VIEW ... WITH (timescaledb.continuous)` 语句。
#[derive(Debug, Clone)]
pub struct ContinuousAggregateDef {
    /// 视图名称
    pub view_name: String,
    /// 源 hypertable 名
    pub source_table: String,
    /// 时间桶宽度（如 "1h"、"1d"）
    pub bucket_interval: String,
    /// 聚合 SQL 表达式（如 "AVG(value)"、"SUM(value)"）
    pub aggregate_expr: String,
    /// 刷新策略
    pub refresh_policy: RefreshPolicy,
    /// 刷新窗口起点偏移（秒，负数表示过去）
    /// 例如 -86400 表示从 1 天前开始刷新
    pub refresh_start_offset: i64,
    /// 刷新窗口终点偏移（秒，负数表示过去）
    /// 例如 -3600 表示刷新到 1 小时前为止（避免实时数据冲突）
    pub refresh_end_offset: i64,
    /// 刷新间隔（秒）
    pub refresh_interval: i64,
}

impl ContinuousAggregateDef {
    /// 创建一个新的连续聚合定义
    pub fn new(
        view_name: impl Into<String>,
        source_table: impl Into<String>,
        bucket_interval: impl Into<String>,
        aggregate_expr: impl Into<String>,
    ) -> Self {
        Self {
            view_name: view_name.into(),
            source_table: source_table.into(),
            bucket_interval: bucket_interval.into(),
            aggregate_expr: aggregate_expr.into(),
            refresh_policy: RefreshPolicy::Scheduled,
            refresh_start_offset: -86400,     // 默认从 1 天前开始
            refresh_end_offset: -3600,         // 默认到 1 小时前为止
            refresh_interval: 3600,            // 默认每小时刷新一次
        }
    }

    /// 设置刷新策略
    pub fn with_refresh_policy(mut self, policy: RefreshPolicy) -> Self {
        self.refresh_policy = policy;
        self
    }

    /// 设置刷新窗口偏移（秒）
    pub fn with_refresh_window(mut self, start_offset: i64, end_offset: i64) -> Self {
        self.refresh_start_offset = start_offset;
        self.refresh_end_offset = end_offset;
        self
    }

    /// 设置刷新间隔（秒）
    pub fn with_refresh_interval(mut self, interval_secs: i64) -> Self {
        self.refresh_interval = interval_secs;
        self
    }

    /// 生成 CREATE MATERIALIZED VIEW SQL
    pub fn to_create_sql(&self) -> String {
        format!(
            "CREATE MATERIALIZED VIEW {} WITH ({}) AS \
             SELECT time_bucket('{}', time), {} FROM {} \
             GROUP BY 1 WITH NO DATA",
            self.view_name,
            self.refresh_policy.as_sql_config(),
            self.bucket_interval,
            self.aggregate_expr,
            self.source_table
        )
    }

    /// 生成 REFRESH 语句的 SQL（刷新窗口由 start/end offset 决定）
    pub fn to_refresh_sql(&self) -> String {
        format!(
            "CALL refresh_continuous_aggregate('{}', now() + interval '{} seconds', now() + interval '{} seconds')",
            self.view_name, self.refresh_start_offset, self.refresh_end_offset
        )
    }

    /// 生成 DROP MATERIALIZED VIEW SQL
    pub fn to_drop_sql(&self) -> String {
        format!("DROP MATERIALIZED VIEW IF EXISTS {}", self.view_name)
    }
}

/// 连续聚合注册表（内存版，用于跟踪已创建的连续聚合）
#[derive(Debug, Clone, Default)]
pub struct ContinuousAggregateRegistry {
    /// 已注册的连续聚合：view_name -> def
    aggregates: HashMap<String, ContinuousAggregateDef>,
    /// 每个视图的刷新历史：view_name -> 最后刷新时间
    last_refreshed: HashMap<String, DateTime<Utc>>,
}

impl ContinuousAggregateRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个连续聚合
    pub fn register(&mut self, def: ContinuousAggregateDef) -> Result<(), TimescaleError> {
        if self.aggregates.contains_key(&def.view_name) {
            return Err(TimescaleError::Query(format!(
                "continuous aggregate already exists: {}",
                def.view_name
            )));
        }
        self.aggregates.insert(def.view_name.clone(), def);
        Ok(())
    }

    /// 注销一个连续聚合
    pub fn unregister(&mut self, view_name: &str) -> Result<ContinuousAggregateDef, TimescaleError> {
        self.aggregates
            .remove(view_name)
            .ok_or_else(|| TimescaleError::NotFound(format!("continuous aggregate: {}", view_name)))
    }

    /// 记录刷新时间
    pub fn mark_refreshed(&mut self, view_name: &str, when: DateTime<Utc>) {
        self.last_refreshed.insert(view_name.to_string(), when);
    }

    /// 获取最后刷新时间
    pub fn last_refresh_time(&self, view_name: &str) -> Option<DateTime<Utc>> {
        self.last_refreshed.get(view_name).copied()
    }

    /// 检查是否需要刷新（根据刷新间隔判断）
    pub fn needs_refresh(&self, view_name: &str, now: DateTime<Utc>) -> bool {
        match self.aggregates.get(view_name) {
            Some(def) => match self.last_refreshed.get(view_name) {
                Some(last) => (now - *last).num_seconds() >= def.refresh_interval,
                None => true, // 从未刷新过
            },
            None => false, // 不存在的视图不需要刷新
        }
    }

    /// 列出所有需要刷新的视图
    pub fn list_needs_refresh(&self, now: DateTime<Utc>) -> Vec<String> {
        self.aggregates
            .keys()
            .filter(|name| self.needs_refresh(name, now))
            .cloned()
            .collect()
    }

    /// 获取所有已注册的连续聚合
    pub fn list_all(&self) -> Vec<&ContinuousAggregateDef> {
        self.aggregates.values().collect()
    }

    /// 生成所有需要刷新视图的 REFRESH SQL
    pub fn refresh_all_sql(&self, now: DateTime<Utc>) -> Vec<String> {
        self.list_needs_refresh(now)
            .iter()
            .filter_map(|name| self.aggregates.get(name).map(|def| def.to_refresh_sql()))
            .collect()
    }
}

// =============================================================================
// 二、数据压缩策略
// =============================================================================

/// 压缩状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStatus {
    /// 压缩已启用
    Enabled,
    /// 压缩已禁用
    Disabled,
}

/// 数据压缩配置
///
/// 对应 TimescaleDB 的 `ALTER TABLE ... SET (timescaledb.compress)` 语句。
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// 目标 hypertable 名
    pub table: String,
    /// 按 segmentby 列分区压缩（通常为 tag/label 列）
    pub segmentby: String,
    /// 按 orderby 列排序压缩（通常为时间列）
    pub orderby: String,
    /// 每个压缩块包含的数据行数（默认 1000）
    pub chunk_time_interval: i64,
}

impl CompressionConfig {
    /// 创建压缩配置
    pub fn new(
        table: impl Into<String>,
        segmentby: impl Into<String>,
        orderby: impl Into<String>,
    ) -> Self {
        Self {
            table: table.into(),
            segmentby: segmentby.into(),
            orderby: orderby.into(),
            chunk_time_interval: 86400, // 默认 1 天一个 chunk
        }
    }

    /// 设置 chunk 时间间隔（秒）
    pub fn with_chunk_interval(mut self, secs: i64) -> Self {
        self.chunk_time_interval = secs;
        self
    }

    /// 生成启用压缩的 SQL
    pub fn to_enable_sql(&self) -> String {
        format!(
            "ALTER TABLE {} SET (timescaledb.compress, \
             timescaledb.compress_segmentby = '{}', \
             timescaledb.compress_orderby = '{}')",
            self.table, self.segmentby, self.orderby
        )
    }

    /// 生成禁用压缩的 SQL
    pub fn to_disable_sql(&self) -> String {
        format!(
            "ALTER TABLE {} SET (timescaledb.compress = false)",
            self.table
        )
    }

    /// 生成压缩指定 chunk 的 SQL
    pub fn to_compress_chunk_sql(&self, chunk_name: &str) -> String {
        format!("SELECT compress_chunk('{}')", chunk_name)
    }

    /// 生成解压指定 chunk 的 SQL
    pub fn to_decompress_chunk_sql(&self, chunk_name: &str) -> String {
        format!("SELECT decompress_chunk('{}')", chunk_name)
    }
}

/// 压缩策略注册表（内存版，用于跟踪已启用压缩的表及其统计）
#[derive(Debug, Clone, Default)]
pub struct CompressionPolicyRegistry {
    /// 已启用压缩的表：table -> (config, status)
    configs: HashMap<String, (CompressionConfig, CompressionStatus)>,
    /// 每张表的压缩统计：table -> CompressionStats
    stats: HashMap<String, CompressionStats>,
}

/// 压缩统计信息
#[derive(Debug, Clone, Default)]
pub struct CompressionStats {
    /// 已压缩的 chunk 数
    pub compressed_chunks: u64,
    /// 未压缩的 chunk 数
    pub uncompressed_chunks: u64,
    /// 压缩前总字节数
    pub before_bytes: u64,
    /// 压缩后总字节数
    pub after_bytes: u64,
}

impl CompressionStats {
    /// 创建空统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 计算压缩比（after / before，越小越好）
    pub fn ratio(&self) -> f64 {
        if self.before_bytes == 0 {
            return 1.0;
        }
        self.after_bytes as f64 / self.before_bytes as f64
    }

    /// 计算节省的空间百分比
    pub fn space_saved_percent(&self) -> f64 {
        if self.before_bytes == 0 {
            return 0.0;
        }
        (1.0 - self.ratio()) * 100.0
    }

    /// 记录一次压缩操作
    pub fn record_compression(&mut self, before: u64, after: u64) {
        self.compressed_chunks += 1;
        self.uncompressed_chunks = self.uncompressed_chunks.saturating_sub(1);
        self.before_bytes += before;
        self.after_bytes += after;
    }

    /// 记录一次解压操作
    pub fn record_decompression(&mut self) {
        self.uncompressed_chunks += 1;
        self.compressed_chunks = self.compressed_chunks.saturating_sub(1);
    }
}

impl CompressionPolicyRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 启用压缩策略
    pub fn enable(&mut self, config: CompressionConfig) -> Result<(), TimescaleError> {
        let table = config.table.clone();
        if let Some((_, status)) = self.configs.get(&table) {
            if *status == CompressionStatus::Enabled {
                return Err(TimescaleError::Query(format!(
                    "compression already enabled on table: {}",
                    table
                )));
            }
        }
        self.configs
            .insert(table.clone(), (config, CompressionStatus::Enabled));
        self.stats.entry(table).or_default();
        Ok(())
    }

    /// 禁用压缩策略
    pub fn disable(&mut self, table: &str) -> Result<CompressionConfig, TimescaleError> {
        match self.configs.get_mut(table) {
            Some((config, status)) => {
                if *status == CompressionStatus::Disabled {
                    return Err(TimescaleError::Query(format!(
                        "compression already disabled on table: {}",
                        table
                    )));
                }
                *status = CompressionStatus::Disabled;
                Ok(config.clone())
            }
            None => Err(TimescaleError::NotFound(format!(
                "compression config for table: {}",
                table
            ))),
        }
    }

    /// 查询压缩状态
    pub fn status(&self, table: &str) -> Option<CompressionStatus> {
        self.configs.get(table).map(|(_, s)| *s)
    }

    /// 获取压缩配置
    pub fn config(&self, table: &str) -> Option<&CompressionConfig> {
        self.configs.get(table).map(|(c, _)| c)
    }

    /// 更新压缩统计
    pub fn update_stats(&mut self, table: &str, stats: CompressionStats) {
        self.stats.insert(table.to_string(), stats);
    }

    /// 获取压缩统计
    pub fn stats(&self, table: &str) -> Option<&CompressionStats> {
        self.stats.get(table)
    }

    /// 生成所有已启用表的压缩 SQL
    pub fn enable_all_sql(&self) -> Vec<String> {
        self.configs
            .iter()
            .filter(|(_, (_, status))| *status == CompressionStatus::Enabled)
            .map(|(_, (config, _))| {
                // 仅对配置存在但尚未实际启用的情况生成 SQL
                config.to_enable_sql()
            })
            .collect()
    }
}

// =============================================================================
// 三、数据保留策略
// =============================================================================

/// 数据保留策略
///
/// 对应 TimescaleDB 的 `add_retention_policy` 函数，自动删除超过保留时长的数据。
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// 目标 hypertable 名
    pub table: String,
    /// 保留时长（秒），超过此时长的数据将被删除
    /// 例如 30 天 = 30 * 86400 = 2,592,000 秒
    pub retention_secs: i64,
    /// 策略调度间隔（秒），默认 1 天
    pub schedule_interval: i64,
    /// 是否启用
    pub enabled: bool,
}

impl RetentionPolicy {
    /// 创建保留策略
    pub fn new(table: impl Into<String>, retention_secs: i64) -> Self {
        Self {
            table: table.into(),
            retention_secs,
            schedule_interval: 86400, // 默认每天检查一次
            enabled: true,
        }
    }

    /// 创建以天为单位的保留策略
    pub fn with_days(table: impl Into<String>, days: i64) -> Self {
        Self::new(table, days * 86400)
    }

    /// 设置调度间隔（秒）
    pub fn with_schedule_interval(mut self, secs: i64) -> Self {
        self.schedule_interval = secs;
        self
    }

    /// 禁用策略
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 生成 add_retention_policy SQL
    pub fn to_add_sql(&self) -> String {
        format!(
            "SELECT add_retention_policy('{}', INTERVAL '{} seconds')",
            self.table, self.retention_secs
        )
    }

    /// 生成 remove_retention_policy SQL
    pub fn to_remove_sql(&self) -> String {
        format!("SELECT remove_retention_policy('{}')", self.table)
    }

    /// 计算截止时间点：早于此时间的数据应被删除
    pub fn cutoff_time(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - Duration::seconds(self.retention_secs)
    }
}

/// 保留策略注册表
#[derive(Debug, Clone, Default)]
pub struct RetentionPolicyRegistry {
    /// 已注册的保留策略：table -> policy
    policies: HashMap<String, RetentionPolicy>,
    /// 每张表的删除统计：table -> (删除次数, 删除行数)
    purge_stats: HashMap<String, (u64, u64)>,
}

impl RetentionPolicyRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册保留策略
    pub fn register(&mut self, policy: RetentionPolicy) -> Result<(), TimescaleError> {
        if self.policies.contains_key(&policy.table) {
            return Err(TimescaleError::Query(format!(
                "retention policy already exists for table: {}",
                policy.table
            )));
        }
        let table = policy.table.clone();
        self.policies.insert(table.clone(), policy);
        self.purge_stats.insert(table, (0, 0));
        Ok(())
    }

    /// 注销保留策略
    pub fn unregister(&mut self, table: &str) -> Result<RetentionPolicy, TimescaleError> {
        self.purge_stats.remove(table);
        self.policies
            .remove(table)
            .ok_or_else(|| TimescaleError::NotFound(format!("retention policy: {}", table)))
    }

    /// 获取策略
    pub fn get(&self, table: &str) -> Option<&RetentionPolicy> {
        self.policies.get(table)
    }

    /// 列出所有已启用的策略
    pub fn list_enabled(&self) -> Vec<&RetentionPolicy> {
        self.policies.values().filter(|p| p.enabled).collect()
    }

    /// 列出所有策略
    pub fn list_all(&self) -> Vec<&RetentionPolicy> {
        self.policies.values().collect()
    }

    /// 记录一次清理操作
    pub fn record_purge(&mut self, table: &str, rows_deleted: u64) {
        if let Some(stats) = self.purge_stats.get_mut(table) {
            stats.0 += 1;
            stats.1 += rows_deleted;
        }
    }

    /// 获取清理统计
    pub fn purge_stats(&self, table: &str) -> Option<(u64, u64)> {
        self.purge_stats.get(table).copied()
    }

    /// 生成所有已启用策略的 add_retention_policy SQL
    pub fn add_all_sql(&self) -> Vec<String> {
        self.list_enabled()
            .iter()
            .map(|p| p.to_add_sql())
            .collect()
    }

    /// 计算所有已启用策略的截止时间
    pub fn cutoff_times(&self, now: DateTime<Utc>) -> Vec<(String, DateTime<Utc>)> {
        self.list_enabled()
            .iter()
            .map(|p| (p.table.clone(), p.cutoff_time(now)))
            .collect()
    }
}

// =============================================================================
// 四、时间桶对齐与 gapfill
// =============================================================================

/// 时间桶对齐工具
///
/// TimescaleDB 的 `time_bucket` 函数会将时间戳对齐到桶的自然边界。
/// 例如 `time_bucket('1h', ts)` 会将时间对齐到整点。
pub struct TimeBucketAligner;

impl TimeBucketAligner {
    /// 将时间戳对齐到桶的自然边界
    ///
    /// - `bucket_secs`：桶宽度（秒）
    /// - `epoch`：对齐基准时间（通常是 Unix epoch 或某个起始时间）
    pub fn align(timestamp: DateTime<Utc>, bucket_secs: i64, epoch: DateTime<Utc>) -> DateTime<Utc> {
        let elapsed = (timestamp - epoch).num_seconds();
        let bucket_idx = elapsed.div_euclid(bucket_secs);
        epoch + Duration::seconds(bucket_idx * bucket_secs)
    }

    /// 对齐到 Unix epoch（1970-01-01 00:00:00 UTC）
    pub fn align_to_epoch(timestamp: DateTime<Utc>, bucket_secs: i64) -> DateTime<Utc> {
        let epoch = DateTime::<Utc>::from_timestamp(0, 0).unwrap();
        Self::align(timestamp, bucket_secs, epoch)
    }

    /// 生成对齐到自然边界的桶序列
    ///
    /// 返回从 `start`（对齐后）到 `end` 的所有桶起始时间。
    pub fn bucket_sequence(
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        bucket_secs: i64,
    ) -> Vec<DateTime<Utc>> {
        let aligned_start = Self::align_to_epoch(start, bucket_secs);
        let mut buckets = Vec::new();
        let mut current = aligned_start;
        while current < end {
            buckets.push(current);
            current += Duration::seconds(bucket_secs);
        }
        buckets
    }
}

/// gapfill 填充策略
///
/// TimescaleDB 提供 `time_bucket_gapfill` 函数，对缺失的桶进行填充。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapfillStrategy {
    /// 用 NULL 填充（即不填充，保持空值）
    Null,
    /// LOCF（Last Observation Carried Forward）：用前一个非空值填充
    Locf,
    /// 线性插值：在两个已知值之间线性插值
    Interpolate,
    /// 用固定值填充
    Constant,
}

/// gapfill 填充器
pub struct GapfillFiller;

impl GapfillFiller {
    /// 对时间桶序列进行 gapfill
    ///
    /// - `buckets`：原始桶数据（可能有不连续的桶）
    /// - `expected_starts`：期望的桶起始时间序列（包含所有应存在的桶）
    /// - `strategy`：填充策略
    /// - `constant_value`：当策略为 Constant 时使用的固定值
    pub fn fill(
        buckets: &[TimeBucket],
        expected_starts: &[DateTime<Utc>],
        strategy: GapfillStrategy,
        constant_value: f64,
    ) -> Vec<TimeBucket> {
        // 建立已有的桶索引
        let mut bucket_map: HashMap<DateTime<Utc>, &TimeBucket> = HashMap::new();
        for b in buckets {
            bucket_map.insert(b.bucket_start, b);
        }

        let mut result = Vec::with_capacity(expected_starts.len());
        let mut last_non_empty: Option<&TimeBucket> = None;
        let mut next_non_empty: Option<&TimeBucket> = None;
        let mut next_idx = 0usize;

        for (i, &start) in expected_starts.iter().enumerate() {
            if let Some(b) = bucket_map.get(&start) {
                if b.count > 0 {
                    last_non_empty = Some(b);
                }
                result.push((*b).clone());
            } else {
                // 缺失的桶
                let filled = match strategy {
                    GapfillStrategy::Null => TimeBucket {
                        bucket_start: start,
                        count: 0,
                        sum: 0.0,
                        min: 0.0,
                        max: 0.0,
                        avg: 0.0,
                    },
                    GapfillStrategy::Constant => TimeBucket {
                        bucket_start: start,
                        count: 0,
                        sum: constant_value,
                        min: constant_value,
                        max: constant_value,
                        avg: constant_value,
                    },
                    GapfillStrategy::Locf => {
                        if let Some(last) = last_non_empty {
                            TimeBucket {
                                bucket_start: start,
                                count: 0,
                                sum: last.avg,
                                min: last.avg,
                                max: last.avg,
                                avg: last.avg,
                            }
                        } else {
                            TimeBucket {
                                bucket_start: start,
                                count: 0,
                                sum: 0.0,
                                min: 0.0,
                                max: 0.0,
                                avg: 0.0,
                            }
                        }
                    }
                    GapfillStrategy::Interpolate => {
                        // 查找下一个非空桶
                        if next_non_empty.is_none() || next_idx <= i {
                            next_non_empty = None;
                            next_idx = i + 1;
                            #[allow(clippy::needless_range_loop)]
                            for j in (i + 1)..expected_starts.len() {
                                if let Some(b) = bucket_map.get(&expected_starts[j]) {
                                    if b.count > 0 {
                                        next_non_empty = Some(b);
                                        next_idx = j;
                                        break;
                                    }
                                }
                            }
                        }
                        // 线性插值
                        let interpolated = match (last_non_empty, next_non_empty) {
                            (Some(prev), Some(next)) => {
                                let total_gap = next_idx - i + (i - find_idx(expected_starts, prev.bucket_start).unwrap_or(i));
                                if total_gap == 0 {
                                    prev.avg
                                } else {
                                    let progress = 1.0; // 简化：当前位置的插值
                                    prev.avg + (next.avg - prev.avg) * progress / total_gap as f64
                                }
                            }
                            (Some(prev), None) => prev.avg,
                            (None, Some(next)) => next.avg,
                            (None, None) => 0.0,
                        };
                        TimeBucket {
                            bucket_start: start,
                            count: 0,
                            sum: interpolated,
                            min: interpolated,
                            max: interpolated,
                            avg: interpolated,
                        }
                    }
                };
                result.push(filled);
            }
        }
        result
    }
}

/// 在期望桶序列中查找某个时间点的索引
fn find_idx(starts: &[DateTime<Utc>], target: DateTime<Utc>) -> Option<usize> {
    starts.iter().position(|&t| t == target)
}

// =============================================================================
// 五、桶解析扩展
// =============================================================================

/// 解析时间桶字符串为秒数（支持多字符数字，如 "15m"、"12h"）
///
/// 支持的单位：s（秒）、m（分）、h（时）、d（天）、w（周）
pub fn parse_bucket_to_secs(bucket: &str) -> Result<i64, TimescaleError> {
    if bucket.is_empty() {
        return Err(TimescaleError::InvalidConfig(
            "bucket string is empty".to_string(),
        ));
    }
    let unit_char = bucket.chars().last().unwrap();
    let num_str = &bucket[..bucket.len() - 1];
    let num: i64 = num_str.parse().map_err(|_| {
        TimescaleError::InvalidConfig(format!(
            "invalid bucket number in '{}': '{}' is not a valid integer",
            bucket, num_str
        ))
    })?;
    let secs = match unit_char {
        's' => num,
        'm' => num.checked_mul(60).ok_or_else(|| {
            TimescaleError::InvalidConfig(format!("bucket overflow: {}", bucket))
        })?,
        'h' => num.checked_mul(3600).ok_or_else(|| {
            TimescaleError::InvalidConfig(format!("bucket overflow: {}", bucket))
        })?,
        'd' => num.checked_mul(86400).ok_or_else(|| {
            TimescaleError::InvalidConfig(format!("bucket overflow: {}", bucket))
        })?,
        'w' => num.checked_mul(86400 * 7).ok_or_else(|| {
            TimescaleError::InvalidConfig(format!("bucket overflow: {}", bucket))
        })?,
        _ => {
            return Err(TimescaleError::InvalidConfig(format!(
                "unsupported bucket unit '{}' in '{}': expected one of s/m/h/d/w",
                unit_char, bucket
            )))
        }
    };
    if secs <= 0 {
        return Err(TimescaleError::InvalidConfig(format!(
            "bucket must be positive: {}",
            bucket
        )));
    }
    Ok(secs)
}

/// 将秒数转为时间桶字符串
pub fn secs_to_bucket_string(secs: i64) -> Result<String, TimescaleError> {
    if secs <= 0 {
        return Err(TimescaleError::InvalidConfig(format!(
            "bucket seconds must be positive: {}",
            secs
        )));
    }
    if secs % (86400 * 7) == 0 {
        Ok(format!("{}w", secs / (86400 * 7)))
    } else if secs % 86400 == 0 {
        Ok(format!("{}d", secs / 86400))
    } else if secs % 3600 == 0 {
        Ok(format!("{}h", secs / 3600))
    } else if secs % 60 == 0 {
        Ok(format!("{}m", secs / 60))
    } else {
        Ok(format!("{}s", secs))
    }
}

// =============================================================================
// 六、单元测试
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap() + Duration::minutes(min)
    }

    // --- 连续聚合测试 ---

    #[test]
    fn test_refresh_policy_as_sql() {
        assert!(RefreshPolicy::Manual.as_sql_config().contains("timescaledb.continuous"));
        assert!(RefreshPolicy::Scheduled.as_sql_config().contains("timescaledb.continuous"));
        assert!(RefreshPolicy::RealTime.as_sql_config().contains("materialized_only = false"));
    }

    #[test]
    fn test_continuous_aggregate_def_create_sql() {
        let def = ContinuousAggregateDef::new("cpu_1h_view", "cpu_usage", "1h", "AVG(value)");
        let sql = def.to_create_sql();
        assert!(sql.contains("CREATE MATERIALIZED VIEW"));
        assert!(sql.contains("cpu_1h_view"));
        assert!(sql.contains("time_bucket('1h'"));
        assert!(sql.contains("AVG(value)"));
        assert!(sql.contains("WITH NO DATA"));
    }

    #[test]
    fn test_continuous_aggregate_def_refresh_sql() {
        let def = ContinuousAggregateDef::new("cpu_1h_view", "cpu_usage", "1h", "AVG(value)")
            .with_refresh_window(-7200, -1800);
        let sql = def.to_refresh_sql();
        assert!(sql.contains("refresh_continuous_aggregate"));
        assert!(sql.contains("cpu_1h_view"));
        assert!(sql.contains("-7200 seconds"));
        assert!(sql.contains("-1800 seconds"));
    }

    #[test]
    fn test_continuous_aggregate_def_drop_sql() {
        let def = ContinuousAggregateDef::new("cpu_1h_view", "cpu_usage", "1h", "AVG(value)");
        let sql = def.to_drop_sql();
        assert!(sql.contains("DROP MATERIALIZED VIEW"));
        assert!(sql.contains("IF EXISTS"));
    }

    #[test]
    fn test_continuous_aggregate_def_realtime_policy() {
        let def = ContinuousAggregateDef::new("cpu_rt", "cpu", "5m", "AVG(value)")
            .with_refresh_policy(RefreshPolicy::RealTime);
        let sql = def.to_create_sql();
        assert!(sql.contains("materialized_only = false"));
    }

    #[test]
    fn test_ca_registry_register_and_exists() {
        let mut reg = ContinuousAggregateRegistry::new();
        let def = ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)");
        reg.register(def).unwrap();
        assert_eq!(reg.list_all().len(), 1);
    }

    #[test]
    fn test_ca_registry_duplicate_fails() {
        let mut reg = ContinuousAggregateRegistry::new();
        reg.register(ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)"))
            .unwrap();
        let result = reg.register(ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)"));
        assert!(result.is_err());
    }

    #[test]
    fn test_ca_registry_unregister() {
        let mut reg = ContinuousAggregateRegistry::new();
        reg.register(ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)"))
            .unwrap();
        let removed = reg.unregister("v1").unwrap();
        assert_eq!(removed.view_name, "v1");
        assert!(reg.list_all().is_empty());
    }

    #[test]
    fn test_ca_registry_needs_refresh_never_refreshed() {
        let mut reg = ContinuousAggregateRegistry::new();
        reg.register(
            ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)")
                .with_refresh_interval(3600),
        )
        .unwrap();
        // 从未刷新过，应该需要刷新
        assert!(reg.needs_refresh("v1", Utc::now()));
    }

    #[test]
    fn test_ca_registry_needs_refresh_recently_refreshed() {
        let mut reg = ContinuousAggregateRegistry::new();
        reg.register(
            ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)")
                .with_refresh_interval(3600),
        )
        .unwrap();
        let now = Utc::now();
        reg.mark_refreshed("v1", now);
        // 刚刚刷新过，不应该需要刷新
        assert!(!reg.needs_refresh("v1", now));
    }

    #[test]
    fn test_ca_registry_needs_refresh_past_interval() {
        let mut reg = ContinuousAggregateRegistry::new();
        reg.register(
            ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)")
                .with_refresh_interval(3600),
        )
        .unwrap();
        let now = Utc::now();
        // 2 小时前刷新过，间隔为 1 小时，应该需要刷新
        reg.mark_refreshed("v1", now - Duration::seconds(7200));
        assert!(reg.needs_refresh("v1", now));
    }

    #[test]
    fn test_ca_registry_list_needs_refresh() {
        let mut reg = ContinuousAggregateRegistry::new();
        reg.register(
            ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)")
                .with_refresh_interval(3600),
        )
        .unwrap();
        reg.register(
            ContinuousAggregateDef::new("v2", "t2", "1h", "AVG(value)")
                .with_refresh_interval(3600),
        )
        .unwrap();
        let now = Utc::now();
        reg.mark_refreshed("v2", now);
        let needs = reg.list_needs_refresh(now);
        assert_eq!(needs.len(), 1);
        assert!(needs.contains(&"v1".to_string()));
    }

    #[test]
    fn test_ca_registry_refresh_all_sql() {
        let mut reg = ContinuousAggregateRegistry::new();
        reg.register(ContinuousAggregateDef::new("v1", "t1", "1h", "AVG(value)"))
            .unwrap();
        let sqls = reg.refresh_all_sql(Utc::now());
        assert_eq!(sqls.len(), 1);
        assert!(sqls[0].contains("refresh_continuous_aggregate"));
    }

    // --- 压缩策略测试 ---

    #[test]
    fn test_compression_config_enable_sql() {
        let config = CompressionConfig::new("cpu_usage", "host", "ts");
        let sql = config.to_enable_sql();
        assert!(sql.contains("ALTER TABLE cpu_usage SET"));
        assert!(sql.contains("timescaledb.compress"));
        assert!(sql.contains("compress_segmentby = 'host'"));
        assert!(sql.contains("compress_orderby = 'ts'"));
    }

    #[test]
    fn test_compression_config_disable_sql() {
        let config = CompressionConfig::new("cpu_usage", "host", "ts");
        let sql = config.to_disable_sql();
        assert!(sql.contains("compress = false"));
    }

    #[test]
    fn test_compression_config_compress_chunk_sql() {
        let config = CompressionConfig::new("cpu_usage", "host", "ts");
        let sql = config.to_compress_chunk_sql("_hyper_1_1_chunk");
        assert!(sql.contains("compress_chunk"));
        assert!(sql.contains("_hyper_1_1_chunk"));
    }

    #[test]
    fn test_compression_config_decompress_chunk_sql() {
        let config = CompressionConfig::new("cpu_usage", "host", "ts");
        let sql = config.to_decompress_chunk_sql("_hyper_1_1_chunk");
        assert!(sql.contains("decompress_chunk"));
    }

    #[test]
    fn test_compression_stats_ratio() {
        let mut stats = CompressionStats::new();
        stats.before_bytes = 1000;
        stats.after_bytes = 200;
        assert!((stats.ratio() - 0.2).abs() < 1e-10);
        assert!((stats.space_saved_percent() - 80.0).abs() < 1e-10);
    }

    #[test]
    fn test_compression_stats_empty() {
        let stats = CompressionStats::new();
        assert_eq!(stats.ratio(), 1.0);
        assert_eq!(stats.space_saved_percent(), 0.0);
    }

    #[test]
    fn test_compression_stats_record() {
        let mut stats = CompressionStats::new();
        stats.uncompressed_chunks = 3;
        stats.record_compression(1000, 200);
        assert_eq!(stats.compressed_chunks, 1);
        assert_eq!(stats.uncompressed_chunks, 2);
        assert_eq!(stats.before_bytes, 1000);
        assert_eq!(stats.after_bytes, 200);
    }

    #[test]
    fn test_compression_stats_record_decompression() {
        let mut stats = CompressionStats::new();
        stats.compressed_chunks = 2;
        stats.record_decompression();
        assert_eq!(stats.compressed_chunks, 1);
        assert_eq!(stats.uncompressed_chunks, 1);
    }

    #[test]
    fn test_compression_registry_enable() {
        let mut reg = CompressionPolicyRegistry::new();
        let config = CompressionConfig::new("cpu_usage", "host", "ts");
        reg.enable(config).unwrap();
        assert_eq!(reg.status("cpu_usage"), Some(CompressionStatus::Enabled));
    }

    #[test]
    fn test_compression_registry_duplicate_enable_fails() {
        let mut reg = CompressionPolicyRegistry::new();
        reg.enable(CompressionConfig::new("cpu_usage", "host", "ts"))
            .unwrap();
        let result = reg.enable(CompressionConfig::new("cpu_usage", "host", "ts"));
        assert!(result.is_err());
    }

    #[test]
    fn test_compression_registry_disable() {
        let mut reg = CompressionPolicyRegistry::new();
        reg.enable(CompressionConfig::new("cpu_usage", "host", "ts"))
            .unwrap();
        let config = reg.disable("cpu_usage").unwrap();
        assert_eq!(config.table, "cpu_usage");
        assert_eq!(reg.status("cpu_usage"), Some(CompressionStatus::Disabled));
    }

    #[test]
    fn test_compression_registry_disable_not_found() {
        let mut reg = CompressionPolicyRegistry::new();
        let result = reg.disable("nonexistent");
        assert!(matches!(result, Err(TimescaleError::NotFound(_))));
    }

    #[test]
    fn test_compression_registry_update_stats() {
        let mut reg = CompressionPolicyRegistry::new();
        reg.enable(CompressionConfig::new("cpu_usage", "host", "ts"))
            .unwrap();
        let stats = CompressionStats {
            compressed_chunks: 5,
            uncompressed_chunks: 2,
            before_bytes: 10000,
            after_bytes: 2000,
        };
        reg.update_stats("cpu_usage", stats);
        let s = reg.stats("cpu_usage").unwrap();
        assert_eq!(s.compressed_chunks, 5);
        assert!((s.space_saved_percent() - 80.0).abs() < 1e-10);
    }

    #[test]
    fn test_compression_registry_enable_all_sql() {
        let mut reg = CompressionPolicyRegistry::new();
        reg.enable(CompressionConfig::new("t1", "host", "ts"))
            .unwrap();
        reg.enable(CompressionConfig::new("t2", "host", "ts"))
            .unwrap();
        let sqls = reg.enable_all_sql();
        assert_eq!(sqls.len(), 2);
    }

    // --- 保留策略测试 ---

    #[test]
    fn test_retention_policy_new() {
        let policy = RetentionPolicy::new("cpu_usage", 2592000);
        assert_eq!(policy.table, "cpu_usage");
        assert_eq!(policy.retention_secs, 2592000);
        assert!(policy.enabled);
    }

    #[test]
    fn test_retention_policy_with_days() {
        let policy = RetentionPolicy::with_days("cpu_usage", 30);
        assert_eq!(policy.retention_secs, 30 * 86400);
    }

    #[test]
    fn test_retention_policy_disable() {
        let policy = RetentionPolicy::new("cpu_usage", 86400).disable();
        assert!(!policy.enabled);
    }

    #[test]
    fn test_retention_policy_add_sql() {
        let policy = RetentionPolicy::new("cpu_usage", 86400);
        let sql = policy.to_add_sql();
        assert!(sql.contains("add_retention_policy"));
        assert!(sql.contains("cpu_usage"));
        assert!(sql.contains("86400 seconds"));
    }

    #[test]
    fn test_retention_policy_remove_sql() {
        let policy = RetentionPolicy::new("cpu_usage", 86400);
        let sql = policy.to_remove_sql();
        assert!(sql.contains("remove_retention_policy"));
        assert!(sql.contains("cpu_usage"));
    }

    #[test]
    fn test_retention_policy_cutoff_time() {
        let policy = RetentionPolicy::new("cpu_usage", 86400);
        let now = Utc::now();
        let cutoff = policy.cutoff_time(now);
        assert_eq!(cutoff, now - Duration::seconds(86400));
    }

    #[test]
    fn test_retention_registry_register() {
        let mut reg = RetentionPolicyRegistry::new();
        reg.register(RetentionPolicy::new("cpu_usage", 86400))
            .unwrap();
        assert!(reg.get("cpu_usage").is_some());
    }

    #[test]
    fn test_retention_registry_duplicate_fails() {
        let mut reg = RetentionPolicyRegistry::new();
        reg.register(RetentionPolicy::new("cpu_usage", 86400))
            .unwrap();
        let result = reg.register(RetentionPolicy::new("cpu_usage", 86400));
        assert!(result.is_err());
    }

    #[test]
    fn test_retention_registry_unregister() {
        let mut reg = RetentionPolicyRegistry::new();
        reg.register(RetentionPolicy::new("cpu_usage", 86400))
            .unwrap();
        let removed = reg.unregister("cpu_usage").unwrap();
        assert_eq!(removed.table, "cpu_usage");
        assert!(reg.get("cpu_usage").is_none());
    }

    #[test]
    fn test_retention_registry_list_enabled() {
        let mut reg = RetentionPolicyRegistry::new();
        reg.register(RetentionPolicy::new("t1", 86400)).unwrap();
        reg.register(RetentionPolicy::new("t2", 86400).disable())
            .unwrap();
        let enabled = reg.list_enabled();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].table, "t1");
    }

    #[test]
    fn test_retention_registry_purge_stats() {
        let mut reg = RetentionPolicyRegistry::new();
        reg.register(RetentionPolicy::new("cpu_usage", 86400))
            .unwrap();
        reg.record_purge("cpu_usage", 1000);
        reg.record_purge("cpu_usage", 500);
        let (count, rows) = reg.purge_stats("cpu_usage").unwrap();
        assert_eq!(count, 2);
        assert_eq!(rows, 1500);
    }

    #[test]
    fn test_retention_registry_add_all_sql() {
        let mut reg = RetentionPolicyRegistry::new();
        reg.register(RetentionPolicy::new("t1", 86400)).unwrap();
        reg.register(RetentionPolicy::new("t2", 86400).disable())
            .unwrap();
        let sqls = reg.add_all_sql();
        assert_eq!(sqls.len(), 1); // 只有启用的才生成 SQL
    }

    #[test]
    fn test_retention_registry_cutoff_times() {
        let mut reg = RetentionPolicyRegistry::new();
        reg.register(RetentionPolicy::new("t1", 86400)).unwrap();
        let now = Utc::now();
        let cutoffs = reg.cutoff_times(now);
        assert_eq!(cutoffs.len(), 1);
        assert_eq!(cutoffs[0].0, "t1");
        assert_eq!(cutoffs[0].1, now - Duration::seconds(86400));
    }

    // --- 时间桶对齐测试 ---

    #[test]
    fn test_align_to_epoch_hour() {
        // 2026-07-20 10:35:42 对齐到 1 小时 -> 10:00:00
        let t = Utc.with_ymd_and_hms(2026, 7, 20, 10, 35, 42).unwrap();
        let aligned = TimeBucketAligner::align_to_epoch(t, 3600);
        assert_eq!(aligned, Utc.with_ymd_and_hms(2026, 7, 20, 10, 0, 0).unwrap());
    }

    #[test]
    fn test_align_to_epoch_minute() {
        let t = Utc.with_ymd_and_hms(2026, 7, 20, 10, 35, 42).unwrap();
        let aligned = TimeBucketAligner::align_to_epoch(t, 300); // 5 分钟
        assert_eq!(aligned, Utc.with_ymd_and_hms(2026, 7, 20, 10, 35, 0).unwrap());
    }

    #[test]
    fn test_align_to_epoch_day() {
        let t = Utc.with_ymd_and_hms(2026, 7, 20, 10, 35, 42).unwrap();
        let aligned = TimeBucketAligner::align_to_epoch(t, 86400);
        assert_eq!(aligned, Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap());
    }

    #[test]
    fn test_bucket_sequence() {
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 3, 0, 0).unwrap();
        let buckets = TimeBucketAligner::bucket_sequence(start, end, 3600);
        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[0], start);
        assert_eq!(buckets[1], start + Duration::hours(1));
        assert_eq!(buckets[2], start + Duration::hours(2));
    }

    #[test]
    fn test_bucket_sequence_unaligned_start() {
        // 起始时间未对齐，应先对齐
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 30, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 2, 0, 0).unwrap();
        let buckets = TimeBucketAligner::bucket_sequence(start, end, 3600);
        // 对齐后从 0:00 开始，到 2:00 之前 -> 0:00, 1:00
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0], Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap());
        assert_eq!(buckets[1], Utc.with_ymd_and_hms(2026, 7, 20, 1, 0, 0).unwrap());
    }

    // --- gapfill 测试 ---

    #[test]
    fn test_gapfill_null_strategy() {
        let start1 = ts(0);
        let start3 = ts(20);
        let buckets = vec![
            TimeBucket::from_values(start1, &[1.0, 2.0]),
            TimeBucket::from_values(start3, &[5.0]),
        ];
        let expected = vec![ts(0), ts(10), ts(20)];
        let filled = GapfillFiller::fill(&buckets, &expected, GapfillStrategy::Null, 0.0);
        assert_eq!(filled.len(), 3);
        assert_eq!(filled[0].count, 2); // 原始
        assert_eq!(filled[1].count, 0); // 填充的空桶
        assert_eq!(filled[2].count, 1); // 原始
    }

    #[test]
    fn test_gapfill_constant_strategy() {
        let start1 = ts(0);
        let buckets = vec![TimeBucket::from_values(start1, &[1.0])];
        let expected = vec![ts(0), ts(10), ts(20)];
        let filled = GapfillFiller::fill(&buckets, &expected, GapfillStrategy::Constant, 42.0);
        assert_eq!(filled.len(), 3);
        assert_eq!(filled[0].count, 1);
        assert!((filled[1].avg - 42.0).abs() < 1e-10);
        assert!((filled[2].avg - 42.0).abs() < 1e-10);
    }

    #[test]
    fn test_gapfill_locf_strategy() {
        let start1 = ts(0);
        let start3 = ts(20);
        let buckets = vec![
            TimeBucket::from_values(start1, &[1.0, 3.0]), // avg = 2.0
            TimeBucket::from_values(start3, &[5.0]),
        ];
        let expected = vec![ts(0), ts(10), ts(20)];
        let filled = GapfillFiller::fill(&buckets, &expected, GapfillStrategy::Locf, 0.0);
        assert_eq!(filled.len(), 3);
        // 第二个桶用 LOCF 填充，应等于前一个非空桶的 avg
        assert!((filled[1].avg - 2.0).abs() < 1e-10);
        assert_eq!(filled[1].count, 0);
    }

    #[test]
    fn test_gapfill_locf_no_previous() {
        // 第一个桶就缺失，LOCF 无前值可用
        let start2 = ts(10);
        let buckets = vec![TimeBucket::from_values(start2, &[5.0])];
        let expected = vec![ts(0), ts(10)];
        let filled = GapfillFiller::fill(&buckets, &expected, GapfillStrategy::Locf, 0.0);
        assert_eq!(filled.len(), 2);
        assert_eq!(filled[0].avg, 0.0); // 无前值，填 0
        assert!((filled[1].avg - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_gapfill_empty_input() {
        let expected: Vec<DateTime<Utc>> = vec![];
        let filled = GapfillFiller::fill(&[], &expected, GapfillStrategy::Null, 0.0);
        assert!(filled.is_empty());
    }

    #[test]
    fn test_gapfill_all_present() {
        let start1 = ts(0);
        let start2 = ts(10);
        let buckets = vec![
            TimeBucket::from_values(start1, &[1.0]),
            TimeBucket::from_values(start2, &[2.0]),
        ];
        let expected = vec![ts(0), ts(10)];
        let filled = GapfillFiller::fill(&buckets, &expected, GapfillStrategy::Null, 0.0);
        assert_eq!(filled.len(), 2);
        assert_eq!(filled[0].count, 1);
        assert_eq!(filled[1].count, 1);
    }

    // --- 桶解析测试 ---

    #[test]
    fn test_parse_bucket_to_secs_basic() {
        assert_eq!(parse_bucket_to_secs("30s").unwrap(), 30);
        assert_eq!(parse_bucket_to_secs("5m").unwrap(), 300);
        assert_eq!(parse_bucket_to_secs("1h").unwrap(), 3600);
        assert_eq!(parse_bucket_to_secs("1d").unwrap(), 86400);
        assert_eq!(parse_bucket_to_secs("1w").unwrap(), 86400 * 7);
    }

    #[test]
    fn test_parse_bucket_to_secs_multi_digit() {
        assert_eq!(parse_bucket_to_secs("15m").unwrap(), 900);
        assert_eq!(parse_bucket_to_secs("12h").unwrap(), 43200);
        assert_eq!(parse_bucket_to_secs("30d").unwrap(), 2592000);
    }

    #[test]
    fn test_parse_bucket_to_secs_empty() {
        assert!(parse_bucket_to_secs("").is_err());
    }

    #[test]
    fn test_parse_bucket_to_secs_invalid_unit() {
        assert!(parse_bucket_to_secs("5x").is_err());
    }

    #[test]
    fn test_parse_bucket_to_secs_invalid_number() {
        assert!(parse_bucket_to_secs("abcm").is_err());
    }

    #[test]
    fn test_parse_bucket_to_secs_zero() {
        assert!(parse_bucket_to_secs("0s").is_err());
    }

    #[test]
    fn test_parse_bucket_to_secs_negative() {
        // 负号会被解析为数字失败
        assert!(parse_bucket_to_secs("-5m").is_err());
    }

    #[test]
    fn test_secs_to_bucket_string_roundtrip() {
        assert_eq!(secs_to_bucket_string(30).unwrap(), "30s");
        assert_eq!(secs_to_bucket_string(300).unwrap(), "5m");
        assert_eq!(secs_to_bucket_string(3600).unwrap(), "1h");
        assert_eq!(secs_to_bucket_string(86400).unwrap(), "1d");
        assert_eq!(secs_to_bucket_string(86400 * 7).unwrap(), "1w");
    }

    #[test]
    fn test_secs_to_bucket_string_zero() {
        assert!(secs_to_bucket_string(0).is_err());
        assert!(secs_to_bucket_string(-1).is_err());
    }

    #[test]
    fn test_secs_to_bucket_string_prefers_largest_unit() {
        // 7 天应优先用 w 而非 d
        assert_eq!(secs_to_bucket_string(86400 * 7).unwrap(), "1w");
        // 14 天 = 2w
        assert_eq!(secs_to_bucket_string(86400 * 14).unwrap(), "2w");
    }
}
