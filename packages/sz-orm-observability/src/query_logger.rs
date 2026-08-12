//! 结构化查询日志器（`query-logging` feature）
//!
//! [`QueryLogger`] 输出 [`QueryLogEntry`]（JSON 格式含查询 SQL/参数/耗时/阶段/慢标记），
//! 复用既有 `sz-orm-masking`（`MaskingRule`）参数脱敏，
//! 复用既有 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39` 阶段耗时。

use serde::{Deserialize, Serialize};

use sz_orm_flamegraph::QueryPhaseTiming;
use sz_orm_masking::{DataMasker, MaskingRule};

/// 可序列化的阶段耗时记录（镜像 `QueryPhaseTiming`）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SerializablePhaseTiming {
    /// 阶段名
    pub phase: String,
    /// 相对起始毫秒
    pub start_ms: u64,
    /// 阶段耗时毫秒
    pub duration_ms: u64,
}

impl From<&QueryPhaseTiming> for SerializablePhaseTiming {
    fn from(t: &QueryPhaseTiming) -> Self {
        Self {
            phase: t.phase.as_str().to_string(),
            start_ms: t.start_ms,
            duration_ms: t.duration_ms,
        }
    }
}

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    /// 含 SQL/参数（已脱敏）
    Debug,
    /// 仅含统计（不含 SQL/params）
    Info,
    /// 仅含慢查询
    Warn,
}

/// 结构化查询日志条目
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryLogEntry {
    /// 查询标识
    pub query_key: String,
    /// SQL 文本（Debug 级别含，Info/Warn 级别不含）
    pub sql: String,
    /// 参数列表（已脱敏，Debug 级别含，Info/Warn 级别不含）
    pub params: Vec<String>,
    /// 总耗时（毫秒）
    pub total_elapsed_ms: u64,
    /// 阶段分解（可序列化，由 `QueryPhaseTiming` 转换）
    pub phase_breakdown: Vec<SerializablePhaseTiming>,
    /// 是否慢查询
    pub slow: bool,
    /// 是否命中缓存
    pub from_cache: bool,
    /// 时间戳（ISO 8601）
    pub timestamp: String,
}

impl QueryLogEntry {
    /// JSON 格式输出
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }
}

/// 结构化查询日志器
pub struct QueryLogger {
    /// 采样率（0.0~1.0，非慢查询按此概率采样）
    sample_rate: f64,
    /// 日志级别
    level: LogLevel,
    /// 随机数生成器种子（用于采样判定）
    counter: std::sync::atomic::AtomicU64,
    /// 脱敏规则
    masking_rules: Vec<MaskingRule>,
}

impl Default for QueryLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryLogger {
    /// 创建日志器（默认 sample_rate = 0.01, level = Info）
    pub fn new() -> Self {
        Self {
            sample_rate: 0.01,
            level: LogLevel::Info,
            counter: std::sync::atomic::AtomicU64::new(0),
            masking_rules: vec![
                MaskingRule::Phone,
                MaskingRule::Email,
                MaskingRule::IdCard,
                MaskingRule::BankCard,
                MaskingRule::Password,
                MaskingRule::ApiKey,
            ],
        }
    }

    /// 设置采样率
    pub fn with_sample_rate(mut self, rate: f64) -> Self {
        self.sample_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// 设置日志级别
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.level = level;
        self
    }

    /// 设置脱敏规则
    pub fn with_masking_rules(mut self, rules: Vec<MaskingRule>) -> Self {
        self.masking_rules = rules;
        self
    }

    /// 日志输出入口
    ///
    /// 返回 `Some(json)` 表示输出日志，`None` 表示被采样过滤或级别过滤。
    /// - 慢查询 100% 采样
    /// - 非慢查询按 `sample_rate` 概率采样
    /// - `Warn` 级别仅含慢查询
    /// - `Info` 级别不含 SQL/params
    /// - `Debug` 级别含 SQL/params（已脱敏）
    pub fn log(&self, mut entry: QueryLogEntry) -> Option<String> {
        // 级别过滤：Warn 且非慢查询 → 不输出
        if self.level == LogLevel::Warn && !entry.slow {
            return None;
        }

        // 采样判定
        if !entry.slow {
            let count = self
                .counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let threshold = (self.sample_rate * 1000.0) as u64;
            let rand_val = count % 1000;
            if rand_val >= threshold {
                return None;
            }
        }

        // 参数脱敏
        entry.params = mask_params(&entry.params, &self.masking_rules);

        // 级别过滤：Info → 不含 SQL/params
        if self.level == LogLevel::Info {
            entry.sql.clear();
            entry.params.clear();
        }

        Some(entry.to_json())
    }
}

/// 参数脱敏（复用既有 `MaskingRule` `packages/sz-orm-masking/src/lib.rs:21`）
pub fn mask_params(params: &[String], rules: &[MaskingRule]) -> Vec<String> {
    params
        .iter()
        .map(|p| {
            for rule in rules {
                let masked = DataMasker::apply(rule, p);
                if masked != *p && masked != "***" {
                    return masked;
                }
            }
            p.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_flamegraph::Phase;

    fn timing(phase: Phase, ms: u64) -> SerializablePhaseTiming {
        SerializablePhaseTiming {
            phase: phase.as_str().to_string(),
            start_ms: 0,
            duration_ms: ms,
        }
    }

    fn entry(slow: bool) -> QueryLogEntry {
        QueryLogEntry {
            query_key: "q1".into(),
            sql: "SELECT * FROM users WHERE phone = ?".into(),
            params: vec!["13800138000".into()],
            total_elapsed_ms: 150,
            phase_breakdown: vec![timing(Phase::SqlExecute, 150)],
            slow,
            from_cache: false,
            timestamp: "2026-08-12T10:00:00Z".into(),
        }
    }

    #[test]
    fn log_entry_json_contains_all_fields() {
        let e = entry(false);
        let json = e.to_json();
        assert!(json.contains("query_key"));
        assert!(json.contains("sql"));
        assert!(json.contains("params"));
        assert!(json.contains("total_elapsed_ms"));
        assert!(json.contains("phase_breakdown"));
        assert!(json.contains("slow"));
        assert!(json.contains("from_cache"));
        assert!(json.contains("timestamp"));
    }

    #[test]
    fn log_entry_json_roundtrip() {
        let e = entry(true);
        let json = e.to_json();
        let back: QueryLogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn slow_query_always_sampled() {
        let logger = QueryLogger::new().with_sample_rate(0.0);
        let e = entry(true);
        assert!(logger.log(e).is_some());
    }

    #[test]
    fn sample_rate_zero_no_non_slow() {
        let logger = QueryLogger::new().with_sample_rate(0.0);
        for _ in 0..100 {
            assert!(logger.log(entry(false)).is_none());
        }
    }

    #[test]
    fn sample_rate_one_all_sampled() {
        let logger = QueryLogger::new().with_sample_rate(1.0);
        for _ in 0..100 {
            assert!(logger.log(entry(false)).is_some());
        }
    }

    #[test]
    fn warn_level_only_slow_queries() {
        let logger = QueryLogger::new().with_level(LogLevel::Warn);
        assert!(logger.log(entry(false)).is_none());
        assert!(logger.log(entry(true)).is_some());
    }

    #[test]
    fn info_level_strips_sql_and_params() {
        let logger = QueryLogger::new()
            .with_level(LogLevel::Info)
            .with_sample_rate(1.0);
        let json = logger.log(entry(false)).unwrap();
        let parsed: QueryLogEntry = serde_json::from_str(&json).unwrap();
        assert!(parsed.sql.is_empty());
        assert!(parsed.params.is_empty());
    }

    #[test]
    fn debug_level_keeps_sql_and_params() {
        let logger = QueryLogger::new()
            .with_level(LogLevel::Debug)
            .with_sample_rate(1.0);
        let json = logger.log(entry(false)).unwrap();
        let parsed: QueryLogEntry = serde_json::from_str(&json).unwrap();
        assert!(!parsed.sql.is_empty());
    }

    #[test]
    fn phone_param_masked() {
        let params = vec!["13800138000".to_string()];
        let rules = vec![MaskingRule::Phone];
        let masked = mask_params(&params, &rules);
        assert!(masked[0].contains('*'));
        assert!(!masked[0].contains("13800138000"));
    }

    #[test]
    fn email_param_masked() {
        let params = vec!["user@example.com".to_string()];
        let rules = vec![MaskingRule::Email];
        let masked = mask_params(&params, &rules);
        assert!(masked[0].contains('*'));
    }

    #[test]
    fn non_sensitive_param_not_masked() {
        let params = vec!["42".to_string()];
        let rules = vec![MaskingRule::Phone, MaskingRule::Email];
        let masked = mask_params(&params, &rules);
        assert_eq!(masked[0], "42");
    }

    #[test]
    fn empty_phase_breakdown_still_logs() {
        let mut e = entry(true);
        e.phase_breakdown.clear();
        let logger = QueryLogger::new();
        assert!(logger.log(e).is_some());
    }
}
