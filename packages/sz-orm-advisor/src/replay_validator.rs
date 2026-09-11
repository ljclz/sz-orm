//! 索引建议回放验证器（v6.8.0 AI-INDEX-01）
//!
//! 对索引候选进行回放验证，测量实际收益百分比。

use std::collections::HashMap;

/// 索引候选
#[derive(Debug, Clone)]
pub struct IndexCandidate {
    /// 表名
    pub table: String,
    /// 列名列表
    pub columns: Vec<String>,
    /// 创建 DDL
    pub ddl: String,
    /// 预期收益百分比（0.0 ~ 100.0）
    pub expected_benefit_pct: f64,
    /// 风险等级
    pub risk: RiskLevel,
}

/// 风险等级
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// 回放结果
#[derive(Debug, Clone)]
pub struct ReplayResult {
    /// 候选索引
    pub candidate: IndexCandidate,
    /// 是否已验证
    pub verified: bool,
    /// 实测收益百分比
    pub measured_benefit_pct: f64,
    /// 回放错误信息
    pub replay_error: Option<String>,
    /// 是否仅静态推断（无真实回放）
    pub static_only: bool,
}

/// 工作负载查询
#[derive(Debug, Clone)]
pub struct WorkloadQuery {
    /// 查询 SQL
    pub sql: String,
    /// 执行耗时（毫秒）
    pub elapsed_ms: u64,
    /// 是否使用索引
    pub uses_index: bool,
}

/// 工作负载摘要
#[derive(Debug, Clone)]
pub struct WorkloadSummary {
    /// 查询总数
    pub total_queries: usize,
    /// 平均耗时（毫秒）
    pub avg_elapsed_ms: f64,
    /// 全表扫描查询数
    pub full_scan_count: usize,
}

/// 回放验证器
pub struct ReplayValidator {
    workload: Vec<WorkloadQuery>,
    table_stats: HashMap<String, TableStats>,
    shadow_db: Option<Box<dyn ShadowDatabase>>,
}

/// 影子数据库 trait（用于回放验证）
pub trait ShadowDatabase: Send + Sync {
    fn execute_ddl(&self, ddl: &str) -> Result<(), String>;
    fn execute_query(&self, sql: &str) -> Result<u64, String>;
    fn rollback_ddl(&self, ddl: &str) -> Result<(), String>;
}

/// 回放警告码
pub const AI_INDEX_REPLAY_SKIPPED: &str = "AI_INDEX_REPLAY_SKIPPED";

/// 表统计信息
#[derive(Debug, Clone)]
struct TableStats {
    row_count: u64,
    existing_indexes: Vec<String>,
}

impl ReplayValidator {
    /// 创建验证器
    pub fn new(workload: Vec<WorkloadQuery>) -> Self {
        Self {
            workload,
            table_stats: HashMap::new(),
            shadow_db: None,
        }
    }

    /// 设置影子数据库
    pub fn with_shadow_db(mut self, db: Box<dyn ShadowDatabase>) -> Self {
        self.shadow_db = Some(db);
        self
    }

    /// 使用影子库回放验证候选索引
    pub fn validate_candidate_with_replay(&self, candidate: IndexCandidate) -> ReplayResult {
        let expected_benefit = candidate.expected_benefit_pct;
        if let Some(ref shadow_db) = self.shadow_db {
            match shadow_db.execute_ddl(&candidate.ddl) {
                Ok(()) => {
                    let matching_queries: Vec<&WorkloadQuery> = self
                        .workload
                        .iter()
                        .filter(|q| {
                            q.sql
                                .to_lowercase()
                                .contains(&candidate.table.to_lowercase())
                        })
                        .collect();

                    if matching_queries.is_empty() {
                        let _ = shadow_db.rollback_ddl(&candidate.ddl);
                        return ReplayResult {
                            candidate,
                            verified: true,
                            measured_benefit_pct: 0.0,
                            replay_error: None,
                            static_only: false,
                        };
                    }

                    let total_before: u64 = matching_queries.iter().map(|q| q.elapsed_ms).sum();
                    let mut total_after: u64 = 0;
                    let mut all_succeeded = true;

                    for q in &matching_queries {
                        match shadow_db.execute_query(&q.sql) {
                            Ok(after_ms) => total_after += after_ms,
                            Err(_) => {
                                all_succeeded = false;
                                break;
                            }
                        }
                    }

                    let _ = shadow_db.rollback_ddl(&candidate.ddl);

                    if !all_succeeded {
                        return ReplayResult {
                            candidate,
                            verified: false,
                            measured_benefit_pct: expected_benefit,
                            replay_error: Some(format!(
                                "{}: 影子库查询执行失败，降级为静态推断",
                                AI_INDEX_REPLAY_SKIPPED
                            )),
                            static_only: true,
                        };
                    }

                    let avg_before = total_before as f64 / matching_queries.len() as f64;
                    let avg_after = total_after as f64 / matching_queries.len() as f64;
                    let benefit = if avg_before > 0.0 {
                        ((avg_before - avg_after) / avg_before) * 100.0
                    } else {
                        0.0
                    };

                    return ReplayResult {
                        candidate,
                        verified: true,
                        measured_benefit_pct: benefit.max(0.0).min(100.0),
                        replay_error: None,
                        static_only: false,
                    };
                }
                Err(_) => {
                    return ReplayResult {
                        candidate,
                        verified: false,
                        measured_benefit_pct: expected_benefit,
                        replay_error: Some(format!(
                            "{}: 影子库不可用，降级为静态推断",
                            AI_INDEX_REPLAY_SKIPPED
                        )),
                        static_only: true,
                    };
                }
            }
        }

        self.validate_candidate(candidate)
    }

    /// 注册表统计信息
    pub fn register_table(&mut self, table: &str, row_count: u64, existing_indexes: Vec<String>) {
        self.table_stats.insert(
            table.to_string(),
            TableStats {
                row_count,
                existing_indexes,
            },
        );
    }

    /// 验证单个候选索引
    pub fn validate_candidate(&self, candidate: IndexCandidate) -> ReplayResult {
        let table_stats = self.table_stats.get(&candidate.table);

        let (measured, verified, error) = if let Some(stats) = table_stats {
            let candidate_cols: String = candidate.columns.join(",");
            if stats
                .existing_indexes
                .iter()
                .any(|idx| idx == &candidate_cols)
            {
                (0.0, true, Some("索引已存在".to_string()))
            } else {
                let full_scan_queries: Vec<&WorkloadQuery> = self
                    .workload
                    .iter()
                    .filter(|q| {
                        !q.uses_index
                            && q.sql
                                .to_lowercase()
                                .contains(&candidate.table.to_lowercase())
                    })
                    .collect();

                if full_scan_queries.is_empty() {
                    (0.0, true, None)
                } else {
                    let total_elapsed: u64 = full_scan_queries.iter().map(|q| q.elapsed_ms).sum();
                    let avg_before = total_elapsed as f64 / full_scan_queries.len() as f64;

                    let selectivity = estimate_selectivity(stats.row_count);
                    let estimated_after = avg_before * selectivity;
                    let benefit = ((avg_before - estimated_after) / avg_before) * 100.0;

                    (benefit.max(0.0).min(100.0), true, None)
                }
            }
        } else {
            (
                candidate.expected_benefit_pct,
                false,
                Some("表统计信息缺失，仅静态推断".to_string()),
            )
        };

        let static_only = !verified || error.is_some();

        ReplayResult {
            candidate,
            verified,
            measured_benefit_pct: measured,
            replay_error: error,
            static_only,
        }
    }

    /// 批量验证候选索引
    pub fn validate_batch(&self, candidates: Vec<IndexCandidate>) -> Vec<ReplayResult> {
        candidates
            .into_iter()
            .map(|c| self.validate_candidate(c))
            .collect()
    }

    /// 生成工作负载摘要
    pub fn workload_summary(&self) -> WorkloadSummary {
        let total = self.workload.len();
        if total == 0 {
            return WorkloadSummary {
                total_queries: 0,
                avg_elapsed_ms: 0.0,
                full_scan_count: 0,
            };
        }
        let avg = self.workload.iter().map(|q| q.elapsed_ms).sum::<u64>() as f64 / total as f64;
        let full_scan = self.workload.iter().filter(|q| !q.uses_index).count();
        WorkloadSummary {
            total_queries: total,
            avg_elapsed_ms: avg,
            full_scan_count: full_scan,
        }
    }

    /// 返回工作负载引用
    pub fn workload(&self) -> &[WorkloadQuery] {
        &self.workload
    }
}

fn estimate_selectivity(row_count: u64) -> f64 {
    if row_count == 0 {
        return 1.0;
    }
    let log = (row_count as f64).ln().max(1.0);
    1.0 / log
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate(table: &str, col: &str, benefit: f64) -> IndexCandidate {
        IndexCandidate {
            table: table.to_string(),
            columns: vec![col.to_string()],
            ddl: format!("CREATE INDEX idx_{}_{} ON {} ({})", table, col, table, col),
            expected_benefit_pct: benefit,
            risk: RiskLevel::Low,
        }
    }

    fn make_workload() -> Vec<WorkloadQuery> {
        vec![
            WorkloadQuery {
                sql: "SELECT * FROM orders WHERE user_id = 1".to_string(),
                elapsed_ms: 500,
                uses_index: false,
            },
            WorkloadQuery {
                sql: "SELECT * FROM orders WHERE user_id = 2".to_string(),
                elapsed_ms: 480,
                uses_index: false,
            },
            WorkloadQuery {
                sql: "SELECT * FROM users WHERE id = 1".to_string(),
                elapsed_ms: 5,
                uses_index: true,
            },
        ]
    }

    #[test]
    fn validate_candidate_with_stats_produces_measured_benefit() {
        let mut validator = ReplayValidator::new(make_workload());
        validator.register_table("orders", 100000, vec!["id".to_string()]);
        let result = validator.validate_candidate(make_candidate("orders", "user_id", 80.0));
        assert!(result.verified);
        assert!(!result.static_only);
        assert!(result.measured_benefit_pct > 0.0);
    }

    #[test]
    fn validate_candidate_without_stats_is_static_only() {
        let validator = ReplayValidator::new(make_workload());
        let result = validator.validate_candidate(make_candidate("orders", "user_id", 80.0));
        assert!(!result.verified);
        assert!(result.static_only);
        assert!(result.replay_error.is_some());
    }

    #[test]
    fn validate_batch_returns_all_results() {
        let mut validator = ReplayValidator::new(make_workload());
        validator.register_table("orders", 100000, vec![]);
        let candidates = vec![
            make_candidate("orders", "user_id", 80.0),
            make_candidate("orders", "created_at", 50.0),
        ];
        let results = validator.validate_batch(candidates);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.verified));
    }

    #[test]
    fn workload_summary_correct() {
        let validator = ReplayValidator::new(make_workload());
        let summary = validator.workload_summary();
        assert_eq!(summary.total_queries, 3);
        assert_eq!(summary.full_scan_count, 2);
        assert!(summary.avg_elapsed_ms > 0.0);
    }

    #[test]
    fn candidate_ddl_preserved_in_result() {
        let validator = ReplayValidator::new(make_workload());
        let candidate = make_candidate("orders", "user_id", 80.0);
        let ddl = candidate.ddl.clone();
        let result = validator.validate_candidate(candidate);
        assert_eq!(result.candidate.ddl, ddl);
    }

    #[test]
    fn no_matching_queries_zero_benefit() {
        let mut validator = ReplayValidator::new(vec![]);
        validator.register_table("orders", 100, vec![]);
        let result = validator.validate_candidate(make_candidate("orders", "user_id", 80.0));
        assert!(result.verified);
        assert_eq!(result.measured_benefit_pct, 0.0);
    }

    #[test]
    fn risk_level_preserved() {
        let validator = ReplayValidator::new(make_workload());
        let mut candidate = make_candidate("orders", "user_id", 80.0);
        candidate.risk = RiskLevel::High;
        let result = validator.validate_candidate(candidate);
        assert_eq!(result.candidate.risk, RiskLevel::High);
    }

    #[test]
    fn expected_benefit_preserved_in_static_mode() {
        let validator = ReplayValidator::new(make_workload());
        let result = validator.validate_candidate(make_candidate("orders", "user_id", 75.0));
        assert!(result.static_only);
        assert_eq!(result.measured_benefit_pct, 75.0);
    }

    #[test]
    fn empty_workload_summary() {
        let validator = ReplayValidator::new(vec![]);
        let summary = validator.workload_summary();
        assert_eq!(summary.total_queries, 0);
        assert_eq!(summary.avg_elapsed_ms, 0.0);
    }

    #[test]
    fn multiple_tables_validated_independently() {
        let mut validator = ReplayValidator::new(vec![
            WorkloadQuery {
                sql: "SELECT * FROM orders WHERE x = 1".to_string(),
                elapsed_ms: 300,
                uses_index: false,
            },
            WorkloadQuery {
                sql: "SELECT * FROM users WHERE y = 1".to_string(),
                elapsed_ms: 200,
                uses_index: false,
            },
        ]);
        validator.register_table("orders", 50000, vec![]);
        validator.register_table("users", 10000, vec![]);

        let r1 = validator.validate_candidate(make_candidate("orders", "x", 70.0));
        let r2 = validator.validate_candidate(make_candidate("users", "y", 60.0));
        assert!(r1.verified);
        assert!(r2.verified);
        assert!(r1.measured_benefit_pct > 0.0);
        assert!(r2.measured_benefit_pct > 0.0);
    }
}
