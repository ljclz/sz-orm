//! 自动诊断 Hook（v6.8.0 AI-DIAG-01）
//!
//! 查询错误/慢查询时自动触发诊断，生成索引建议和修复 DDL。

use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

/// 诊断严重级别
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnoseSeverity {
    Info,
    Warning,
    Critical,
}

/// 索引建议
#[derive(Debug, Clone)]
pub struct IndexSuggestion {
    /// 表名
    pub table: String,
    /// 列名列表
    pub columns: Vec<String>,
    /// 建议的 DDL
    pub ddl: String,
    /// 建议原因
    pub reason: String,
}

/// 诊断结果
#[derive(Debug, Clone)]
pub struct DiagnoseResult {
    /// 查询 SQL
    pub query: String,
    /// 诊断严重级别
    pub severity: DiagnoseSeverity,
    /// 根因分析
    pub root_cause: String,
    /// 索引建议列表
    pub index_suggestions: Vec<IndexSuggestion>,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
    /// 是否使用 LLM 诊断
    pub llm_used: bool,
    /// 是否回退到规则引擎
    pub fallback_to_rules: bool,
}

/// 自动诊断 Hook
pub struct AutoDiagnoseHook {
    slow_threshold_ms: u64,
    diagnose_count: AtomicU64,
    recent_reports: RwLock<Vec<DiagnoseResult>>,
    max_recent: usize,
}

impl AutoDiagnoseHook {
    /// 创建诊断 Hook
    pub fn new(slow_threshold_ms: u64) -> Self {
        Self {
            slow_threshold_ms,
            diagnose_count: AtomicU64::new(0),
            recent_reports: RwLock::new(Vec::new()),
            max_recent: 100,
        }
    }

    /// 慢查询诊断
    pub fn on_slow_query(&self, query: &str, elapsed_ms: u64, plan: &str) -> DiagnoseResult {
        let severity = if elapsed_ms > self.slow_threshold_ms * 3 {
            DiagnoseSeverity::Critical
        } else if elapsed_ms > self.slow_threshold_ms * 2 {
            DiagnoseSeverity::Warning
        } else {
            DiagnoseSeverity::Info
        };

        let root_cause = analyze_slow_query_root_cause(query, plan);
        let index_suggestions = suggest_indexes(query, plan);

        let result = DiagnoseResult {
            query: query.to_string(),
            severity,
            root_cause,
            index_suggestions,
            elapsed_ms,
            llm_used: false,
            fallback_to_rules: true,
        };

        self.record(result.clone());
        result
    }

    /// 查询错误诊断
    pub fn on_query_error(&self, query: &str, error: &str) -> DiagnoseResult {
        let root_cause = analyze_error_root_cause(error);
        let index_suggestions = if error.contains("index") || error.contains("索引") {
            suggest_indexes(query, "")
        } else {
            vec![]
        };

        let result = DiagnoseResult {
            query: query.to_string(),
            severity: DiagnoseSeverity::Critical,
            root_cause,
            index_suggestions,
            elapsed_ms: 0,
            llm_used: false,
            fallback_to_rules: true,
        };

        self.record(result.clone());
        result
    }

    /// 返回诊断次数
    pub fn diagnose_count(&self) -> u64 {
        self.diagnose_count.load(Ordering::Relaxed)
    }

    /// 返回慢查询阈值
    pub fn slow_threshold_ms(&self) -> u64 {
        self.slow_threshold_ms
    }

    /// 返回最近的诊断报告
    pub fn recent_reports(&self) -> Vec<DiagnoseResult> {
        self.recent_reports.read().clone()
    }

    fn record(&self, result: DiagnoseResult) {
        self.diagnose_count.fetch_add(1, Ordering::Relaxed);
        let mut reports = self.recent_reports.write();
        if reports.len() >= self.max_recent {
            reports.remove(0);
        }
        reports.push(result);
    }
}

fn analyze_slow_query_root_cause(query: &str, plan: &str) -> String {
    let plan_lower = plan.to_lowercase();
    if plan_lower.contains("seq scan")
        || plan_lower.contains("table scan")
        || plan_lower.contains("full scan")
    {
        "全表扫描：查询未使用索引，导致扫描全部行".to_string()
    } else if plan_lower.contains("nested loop") && plan_lower.contains("seq scan") {
        "嵌套循环 + 全表扫描：N+1 查询模式".to_string()
    } else if query.to_lowercase().contains("join") && plan_lower.contains("hash join") {
        "Hash Join 开销高：连接字段可能缺失索引".to_string()
    } else if plan_lower.contains("sort") {
        "排序开销高：ORDER BY 字段可能缺失索引".to_string()
    } else {
        "查询耗时超过阈值，建议检查执行计划".to_string()
    }
}

fn analyze_error_root_cause(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "查询超时：可能因全表扫描或锁等待".to_string()
    } else if lower.contains("deadlock") {
        "死锁：多个事务以不同顺序获取锁".to_string()
    } else if lower.contains("out of memory") || lower.contains("oom") {
        "内存不足：结果集过大或聚合中间结果溢出".to_string()
    } else if lower.contains("connection") || lower.contains("pool") {
        "连接池耗尽：并发请求超过连接池容量".to_string()
    } else if lower.contains("syntax") {
        "SQL 语法错误".to_string()
    } else {
        format!("查询执行失败：{}", error)
    }
}

fn suggest_indexes(query: &str, plan: &str) -> Vec<IndexSuggestion> {
    let mut suggestions = vec![];
    let query_lower = query.to_lowercase();
    let plan_lower = plan.to_lowercase();

    if let Some(table) = extract_table_from_query(&query_lower) {
        if plan_lower.contains("seq scan")
            || plan_lower.contains("table scan")
            || plan_lower.contains("full scan")
        {
            if let Some(where_cols) = extract_where_columns(&query_lower) {
                let cols_str = where_cols.join(", ");
                suggestions.push(IndexSuggestion {
                    table: table.clone(),
                    columns: where_cols.clone(),
                    ddl: format!(
                        "CREATE INDEX idx_{}_{} ON {} ({})",
                        table,
                        where_cols.join("_"),
                        table,
                        cols_str
                    ),
                    reason: "WHERE 条件字段缺失索引，导致全表扫描".to_string(),
                });
            }
        }

        if query_lower.contains("order by") {
            if let Some(order_cols) = extract_order_by_columns(&query_lower) {
                let cols_str = order_cols.join(", ");
                suggestions.push(IndexSuggestion {
                    table: table.clone(),
                    columns: order_cols.clone(),
                    ddl: format!(
                        "CREATE INDEX idx_{}_{}_order ON {} ({})",
                        table,
                        order_cols.join("_"),
                        table,
                        cols_str
                    ),
                    reason: "ORDER BY 字段缺失索引，导致额外排序".to_string(),
                });
            }
        }

        if query_lower.contains("join") {
            if let Some(join_cols) = extract_join_columns(&query_lower) {
                let cols_str = join_cols.join(", ");
                suggestions.push(IndexSuggestion {
                    table: table.clone(),
                    columns: join_cols.clone(),
                    ddl: format!("CREATE INDEX idx_join ON {} ({})", table, cols_str),
                    reason: "JOIN 连接字段缺失索引".to_string(),
                });
            }
        }
    }
    suggestions
}

fn extract_table_from_query(query: &str) -> Option<String> {
    let from_idx = query.find("from ")?;
    let after_from = &query[from_idx + 5..];
    let table: String = after_from
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    if table.is_empty() {
        None
    } else {
        Some(table.trim_start_matches('.').to_string())
    }
}

fn extract_where_columns(query: &str) -> Option<Vec<String>> {
    let where_idx = query.find("where ")?;
    let after_where = &query[where_idx + 6..];
    let before_group = after_where
        .split("group by")
        .next()
        .unwrap_or(after_where)
        .split("order by")
        .next()
        .unwrap_or(after_where)
        .split("limit")
        .next()
        .unwrap_or(after_where);

    let mut cols = vec![];
    for part in before_group.split("and").chain(before_group.split("or")) {
        let part = part.trim();
        if let Some(col) = part.split([' ', '=', '<', '>', '!']).next() {
            let col = col.trim();
            if !col.is_empty()
                && col.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !cols.contains(&col.to_string())
            {
                cols.push(col.to_string());
            }
        }
    }
    if cols.is_empty() {
        None
    } else {
        Some(cols)
    }
}

fn extract_order_by_columns(query: &str) -> Option<Vec<String>> {
    let order_idx = query.find("order by ")?;
    let after_order = &query[order_idx + 9..];
    let before_limit = after_order.split("limit").next().unwrap_or(after_order);
    let cols: Vec<String> = before_limit
        .split(',')
        .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if cols.is_empty() {
        None
    } else {
        Some(cols)
    }
}

fn extract_join_columns(query: &str) -> Option<Vec<String>> {
    let on_idx = query.find("on ")?;
    let after_on = &query[on_idx + 3..];
    let condition = after_on.split_whitespace().next().unwrap_or("");
    if let Some(col) = condition.split('=').next() {
        let col = col.trim();
        if !col.is_empty() {
            return Some(vec![col.to_string()]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_slow_query_full_table_scan_suggests_index() {
        let hook = AutoDiagnoseHook::new(100);
        let result = hook.on_slow_query(
            "SELECT * FROM users WHERE age = 25",
            500,
            "Seq Scan on users",
        );
        assert_eq!(result.severity, DiagnoseSeverity::Critical);
        assert!(!result.index_suggestions.is_empty());
        assert!(result.index_suggestions[0].ddl.contains("CREATE INDEX"));
        assert!(result.index_suggestions[0].ddl.contains("users"));
        assert!(result.index_suggestions[0].ddl.contains("age"));
    }

    #[test]
    fn on_slow_query_with_order_by_suggests_sort_index() {
        let hook = AutoDiagnoseHook::new(100);
        let result = hook.on_slow_query("SELECT * FROM users ORDER BY created_at", 300, "Sort");
        assert!(!result.index_suggestions.is_empty());
        let has_sort_advice = result
            .index_suggestions
            .iter()
            .any(|s| s.reason.contains("排序"));
        assert!(has_sort_advice);
    }

    #[test]
    fn on_query_error_timeout_diagnoses() {
        let hook = AutoDiagnoseHook::new(100);
        let result = hook.on_query_error("SELECT * FROM big_table", "query timeout after 30s");
        assert_eq!(result.severity, DiagnoseSeverity::Critical);
        assert!(result.root_cause.contains("超时"));
    }

    #[test]
    fn on_query_error_deadlock_diagnoses() {
        let hook = AutoDiagnoseHook::new(100);
        let result = hook.on_query_error("UPDATE accounts SET balance = 100", "deadlock detected");
        assert_eq!(result.severity, DiagnoseSeverity::Critical);
        assert!(result.root_cause.contains("死锁"));
    }

    #[test]
    fn on_query_error_pool_exhaustion() {
        let hook = AutoDiagnoseHook::new(100);
        let result = hook.on_query_error("SELECT 1", "connection pool exhausted");
        assert!(result.root_cause.contains("连接池"));
    }

    #[test]
    fn severity_levels_correct() {
        let hook = AutoDiagnoseHook::new(100);
        let info = hook.on_slow_query("SELECT 1", 150, "");
        assert_eq!(info.severity, DiagnoseSeverity::Info);

        let warning = hook.on_slow_query("SELECT 1", 250, "");
        assert_eq!(warning.severity, DiagnoseSeverity::Warning);

        let critical = hook.on_slow_query("SELECT 1", 400, "");
        assert_eq!(critical.severity, DiagnoseSeverity::Critical);
    }

    #[test]
    fn diagnose_count_increments() {
        let hook = AutoDiagnoseHook::new(100);
        assert_eq!(hook.diagnose_count(), 0);
        hook.on_slow_query("SELECT 1", 200, "");
        assert_eq!(hook.diagnose_count(), 1);
        hook.on_query_error("SELECT 1", "error");
        assert_eq!(hook.diagnose_count(), 2);
    }

    #[test]
    fn recent_reports_stored() {
        let hook = AutoDiagnoseHook::new(100);
        hook.on_slow_query("SELECT * FROM users WHERE id = 1", 200, "Seq Scan");
        hook.on_query_error("SELECT 1", "timeout");
        let reports = hook.recent_reports();
        assert_eq!(reports.len(), 2);
    }

    #[test]
    fn fallback_to_rules_flag_set() {
        let hook = AutoDiagnoseHook::new(100);
        let result = hook.on_slow_query("SELECT 1", 200, "");
        assert!(result.fallback_to_rules);
        assert!(!result.llm_used);
    }

    #[test]
    fn join_query_suggests_join_index() {
        let hook = AutoDiagnoseHook::new(100);
        let result = hook.on_slow_query(
            "SELECT * FROM orders JOIN users ON orders.user_id = users.id",
            500,
            "Hash Join",
        );
        assert!(!result.index_suggestions.is_empty());
    }

    #[test]
    fn slow_threshold_configurable() {
        let hook = AutoDiagnoseHook::new(500);
        assert_eq!(hook.slow_threshold_ms(), 500);
        let result = hook.on_slow_query("SELECT 1", 600, "");
        assert_eq!(result.severity, DiagnoseSeverity::Info);
    }
}
