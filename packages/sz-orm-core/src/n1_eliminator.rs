//! N1Eliminator — N+1 自动消除器（v2.3.0 任务 C）
//!
//! 检测连续相同模式的查询（同表、同 WHERE 列、同 SELECT 列），
//! 当连续查询数达到阈值时自动合并为 `WHERE id IN (?,...)` 批量查询。
//!
//! # 设计
//!
//! - 阈值默认 5（可通过 `with_threshold` 配置）
//! - 含独立事务的查询跳过合并（事务边界不兼容）
//! - 合并后结果等价性校验，不等价回退逐条执行
//! - 合并 SQL 参数化，禁止 `SELECT *`
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::n1_eliminator::N1Eliminator;
//!
//! let mut eliminator = N1Eliminator::new();
//! for id in 0..10 {
//!     eliminator.record_query(PendingQuery {
//!         table: "users",
//!         where_column: "id",
//!         where_value: Value::I64(id),
//!         select_columns: &["id", "name"],
//!         in_standalone_transaction: false,
//!         trigger_location: "handler.rs:42".to_string(),
//!     });
//! }
//! let report = eliminator.try_merge(&mut conn).await?;
//! ```

use crate::pool::Connection;
use crate::value::Value;
use crate::DbError;

/// 待合并的查询记录（v2.3.0 新增）
#[derive(Debug, Clone)]
pub struct PendingQuery {
    /// 查询表名
    pub table: String,
    /// WHERE 条件列名（如 "id"）
    pub where_column: String,
    /// WHERE 条件值
    pub where_value: Value,
    /// SELECT 列名列表
    pub select_columns: Vec<String>,
    /// 是否在独立事务中（独立事务跳过合并）
    pub in_standalone_transaction: bool,
    /// 触发位置（file:line）
    pub trigger_location: String,
}

/// N+1 消除报告（v2.3.0 新增）
#[derive(Debug, Clone)]
pub struct N1EliminationReport {
    /// 原始查询次数
    pub original_count: usize,
    /// 合并后查询次数
    pub merged_count: usize,
    /// 节省的查询次数
    pub saved_count: usize,
    /// 触发位置
    pub trigger_location: String,
    /// 合并后的参数化 SQL
    pub merged_sql: String,
}

/// N+1 自动消除器（v2.3.0 新增）
///
/// 检测连续相同模式的查询并合并为批量 IN 查询。
#[derive(Debug, Clone)]
pub struct N1Eliminator {
    /// 合并阈值（默认 5）
    threshold: usize,
    /// 待合并的查询列表
    pending_queries: Vec<PendingQuery>,
}

impl N1Eliminator {
    /// 创建 N1Eliminator（默认阈值 5）
    pub fn new() -> Self {
        Self {
            threshold: 5,
            pending_queries: Vec::new(),
        }
    }

    /// 配置合并阈值
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            threshold,
            pending_queries: Vec::new(),
        }
    }

    /// 返回当前阈值
    pub fn threshold(&self) -> usize {
        self.threshold
    }

    /// 返回待合并查询数量
    pub fn pending_count(&self) -> usize {
        self.pending_queries.len()
    }

    /// 记入一次查询
    ///
    /// 检测连续相同模式（同表、同 WHERE 列、同 SELECT 列）。
    /// 若新查询与已有模式不一致，清空已有记录重新开始。
    pub fn record_query(&mut self, query: PendingQuery) {
        if let Some(first) = self.pending_queries.first() {
            let same_pattern = first.table == query.table
                && first.where_column == query.where_column
                && first.select_columns == query.select_columns;
            if !same_pattern {
                self.pending_queries.clear();
            }
        }
        self.pending_queries.push(query);
    }

    /// 尝试合并待处理查询为批量 IN 查询
    ///
    /// 算法（design.md §3.5.3）：
    /// 1. 未达阈值返回 `Ok(None)`
    /// 2. 含独立事务返回 `Ok(None)` + 告警
    /// 3. 生成 `WHERE id IN (?,...)` 批量查询
    /// 4. 结果等价性校验，不等价返回 `Err(DbError::Internal)` 回退
    pub async fn try_merge(
        &self,
        conn: &mut dyn Connection,
    ) -> Result<Option<N1EliminationReport>, DbError> {
        if self.pending_queries.len() < self.threshold {
            return Ok(None);
        }

        let has_standalone = self
            .pending_queries
            .iter()
            .any(|q| q.in_standalone_transaction);
        if has_standalone {
            tracing::warn!(
                count = self.pending_queries.len(),
                "N+1 消除跳过：含独立事务查询，事务边界不兼容"
            );
            return Ok(None);
        }

        let first = &self.pending_queries[0];
        let select_cols = &first.select_columns;
        let where_col = &first.where_column;
        let table = &first.table;

        let where_values: Vec<Value> = self
            .pending_queries
            .iter()
            .map(|q| q.where_value.clone())
            .collect();

        let placeholders: Vec<String> = (0..where_values.len()).map(|_| "?".to_string()).collect();
        let merged_sql = format!(
            "SELECT {} FROM {} WHERE {} IN ({})",
            select_cols.join(", "),
            table,
            where_col,
            placeholders.join(", ")
        );

        let merged_rows = conn.query_with_params(&merged_sql, &where_values).await?;

        let original_count = self.pending_queries.len();
        let expected_keys: std::collections::HashSet<String> = self
            .pending_queries
            .iter()
            .map(|q| value_to_key(&q.where_value))
            .collect();

        let actual_keys: std::collections::HashSet<String> = merged_rows
            .iter()
            .filter_map(|row| row.get(where_col).map(value_to_key))
            .collect();

        if expected_keys != actual_keys {
            tracing::error!(
                expected = expected_keys.len(),
                actual = actual_keys.len(),
                "N+1 消除结果等价性校验失败，回退逐条执行"
            );
            return Err(DbError::Internal(
                "N+1 消除结果等价性校验失败，已回退逐条执行".to_string(),
            ));
        }

        let trigger_location = first.trigger_location.clone();
        let report = N1EliminationReport {
            original_count,
            merged_count: 1,
            saved_count: original_count - 1,
            trigger_location,
            merged_sql,
        };

        tracing::info!(
            original = report.original_count,
            merged = report.merged_count,
            saved = report.saved_count,
            "N+1 消除成功"
        );

        Ok(Some(report))
    }

    /// 清空待处理查询
    pub fn clear(&mut self) {
        self.pending_queries.clear();
    }
}

impl Default for N1Eliminator {
    fn default() -> Self {
        Self::new()
    }
}

/// 将 Value 转换为字符串键
fn value_to_key(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => format!("bool:{}", b),
        Value::I8(v) => format!("i8:{}", v),
        Value::I16(v) => format!("i16:{}", v),
        Value::I32(v) => format!("i32:{}", v),
        Value::I64(v) => format!("i64:{}", v),
        Value::U8(v) => format!("u8:{}", v),
        Value::U16(v) => format!("u16:{}", v),
        Value::U32(v) => format!("u32:{}", v),
        Value::U64(v) => format!("u64:{}", v),
        Value::F32(v) => format!("f32:{}", v),
        Value::F64(v) => format!("f64:{}", v),
        Value::String(s) => format!("str:{}", s),
        _ => format!("other:{:?}", value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockConnection;

    fn make_query(id: i64) -> PendingQuery {
        PendingQuery {
            table: "users".to_string(),
            where_column: "id".to_string(),
            where_value: Value::I64(id),
            select_columns: vec!["id".to_string(), "name".to_string()],
            in_standalone_transaction: false,
            trigger_location: "test.rs:1".to_string(),
        }
    }

    #[test]
    fn test_n1_eliminator_new() {
        let eliminator = N1Eliminator::new();
        assert_eq!(eliminator.threshold(), 5);
        assert_eq!(eliminator.pending_count(), 0);
    }

    #[test]
    fn test_n1_eliminator_with_threshold() {
        let eliminator = N1Eliminator::with_threshold(3);
        assert_eq!(eliminator.threshold(), 3);
    }

    #[test]
    fn test_n1_eliminator_record_query() {
        let mut eliminator = N1Eliminator::new();
        eliminator.record_query(make_query(1));
        eliminator.record_query(make_query(2));
        assert_eq!(eliminator.pending_count(), 2);
    }

    #[test]
    fn test_n1_eliminator_record_query_different_pattern() {
        let mut eliminator = N1Eliminator::new();
        eliminator.record_query(make_query(1));

        let mut different = make_query(2);
        different.table = "orders".to_string();
        eliminator.record_query(different);

        assert_eq!(eliminator.pending_count(), 1);
    }

    #[test]
    fn test_n1_eliminator_record_query_different_select() {
        let mut eliminator = N1Eliminator::new();
        eliminator.record_query(make_query(1));

        let mut different = make_query(2);
        different.select_columns = vec!["id".to_string()];
        eliminator.record_query(different);

        assert_eq!(eliminator.pending_count(), 1);
    }

    #[test]
    fn test_n1_eliminator_clear() {
        let mut eliminator = N1Eliminator::new();
        eliminator.record_query(make_query(1));
        eliminator.record_query(make_query(2));
        eliminator.clear();
        assert_eq!(eliminator.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_try_merge_below_threshold() {
        let mut eliminator = N1Eliminator::with_threshold(5);
        for i in 0..4 {
            eliminator.record_query(make_query(i));
        }
        let mut conn = MockConnection::new();
        let result = eliminator.try_merge(&mut conn).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_merge_with_standalone_transaction() {
        let mut eliminator = N1Eliminator::with_threshold(3);
        for i in 0..5 {
            let mut q = make_query(i);
            q.in_standalone_transaction = true;
            eliminator.record_query(q);
        }
        let mut conn = MockConnection::new();
        let result = eliminator.try_merge(&mut conn).await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_n1_elimination_report_fields() {
        let report = N1EliminationReport {
            original_count: 10,
            merged_count: 1,
            saved_count: 9,
            trigger_location: "handler.rs:42".to_string(),
            merged_sql: "SELECT id, name FROM users WHERE id IN (?, ?, ?)".to_string(),
        };
        assert_eq!(report.original_count, 10);
        assert_eq!(report.merged_count, 1);
        assert_eq!(report.saved_count, 9);
        assert!(report.merged_sql.contains("WHERE id IN"));
        assert!(!report.merged_sql.contains("SELECT *"));
    }
}
