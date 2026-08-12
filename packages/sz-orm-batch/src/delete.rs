//! batch_delete — 批量删除（IN 子句 + 分片 + 参数化绑定）
//!
//! 复用既有 DefaultBatchOps 分片逻辑（chunk_indices）+ quote 转义防注入。

use serde_json::Value;

use crate::{BatchResult, DefaultBatchOps};

/// 批量删除错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchDeleteError {
    /// ids 为空
    EmptyIds,
    /// 主键为空
    MissingPrimaryKey,
}

impl std::fmt::Display for BatchDeleteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchDeleteError::EmptyIds => write!(f, "ids cannot be empty for batch_delete"),
            BatchDeleteError::MissingPrimaryKey => write!(f, "primary_key cannot be empty"),
        }
    }
}

impl std::error::Error for BatchDeleteError {}

/// 批量删除请求
#[derive(Debug, Clone)]
pub struct BatchDeleteRequest {
    /// 表名
    pub table: String,
    /// 主键列名
    pub primary_key: String,
    /// 待删除的 ID 列表
    pub ids: Vec<Value>,
}

impl BatchDeleteRequest {
    /// 创建删除请求（校验 ids 非空 + primary_key 非空）
    pub fn new(
        table: impl Into<String>,
        primary_key: impl Into<String>,
        ids: Vec<Value>,
    ) -> Result<Self, BatchDeleteError> {
        let pk = primary_key.into();
        if pk.is_empty() {
            return Err(BatchDeleteError::MissingPrimaryKey);
        }
        if ids.is_empty() {
            return Err(BatchDeleteError::EmptyIds);
        }
        Ok(Self {
            table: table.into(),
            primary_key: pk,
            ids,
        })
    }

    /// 设置表名
    pub fn with_table(mut self, table: impl Into<String>) -> Self {
        self.table = table.into();
        self
    }

    /// 设置主键
    pub fn with_primary_key(mut self, pk: impl Into<String>) -> Self {
        self.primary_key = pk.into();
        self
    }
}

/// 批量删除结果（包含生成的 SQL 和影响的行数）
#[derive(Debug, Clone)]
pub struct BatchDeleteResult {
    /// 基础批量结果
    pub base: BatchResult,
    /// 生成的参数化 SQL 和参数
    pub sqls_with_params: Vec<(String, Vec<Value>)>,
}

impl BatchDeleteResult {
    pub fn new() -> Self {
        Self {
            base: BatchResult::new(),
            sqls_with_params: Vec::new(),
        }
    }
}

impl Default for BatchDeleteResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 执行批量删除（生成 SQL，不执行）
///
/// 按 chunk_size 分片生成多条 `DELETE FROM table WHERE pk IN (?, ?, ...)` SQL。
pub fn batch_delete(ops: &DefaultBatchOps, request: &BatchDeleteRequest) -> BatchDeleteResult {
    let mut result = BatchDeleteResult::new();
    let total = request.ids.len();
    let chunk_size = ops.chunk_size.max(1);
    let pk_q = DefaultBatchOps::quote(&request.primary_key);
    let table_q = DefaultBatchOps::quote(&request.table);

    for (start, end) in (0..total).step_by(chunk_size).map(move |s| {
        let e = (s + chunk_size).min(total);
        (s, e)
    }) {
        let chunk = &request.ids[start..end];
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!("DELETE FROM {table_q} WHERE {pk_q} IN ({placeholders})");
        result.sqls_with_params.push((sql, chunk.to_vec()));
        result.base.generated_sqls.push(format!(
            "DELETE FROM {table_q} WHERE {pk_q} IN ({placeholders})"
        ));
        result.base.updated += chunk.len();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn delete_request_valid() {
        let req = BatchDeleteRequest::new("users", "id", vec![json!(1), json!(2)]);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.table, "users");
        assert_eq!(req.primary_key, "id");
        assert_eq!(req.ids.len(), 2);
    }

    #[test]
    fn delete_request_empty_ids() {
        let req = BatchDeleteRequest::new("users", "id", vec![]);
        assert_eq!(req.err(), Some(BatchDeleteError::EmptyIds));
    }

    #[test]
    fn delete_request_empty_pk() {
        let req = BatchDeleteRequest::new("users", "", vec![json!(1)]);
        assert_eq!(req.err(), Some(BatchDeleteError::MissingPrimaryKey));
    }

    #[test]
    fn batch_delete_chunking() {
        let ops = DefaultBatchOps::new().with_chunk_size(1000);
        let ids: Vec<Value> = (1..=2500).map(|i| json!(i)).collect();
        let req = BatchDeleteRequest::new("users", "id", ids).unwrap();
        let result = batch_delete(&ops, &req);
        assert_eq!(result.base.generated_sqls.len(), 3);
        assert_eq!(result.base.updated, 2500);
        assert_eq!(result.sqls_with_params.len(), 3);
    }

    #[test]
    fn batch_delete_exact_chunk() {
        let ops = DefaultBatchOps::new().with_chunk_size(1000);
        let ids: Vec<Value> = (1..=1000).map(|i| json!(i)).collect();
        let req = BatchDeleteRequest::new("users", "id", ids).unwrap();
        let result = batch_delete(&ops, &req);
        assert_eq!(result.base.generated_sqls.len(), 1);
        assert_eq!(result.base.updated, 1000);
    }

    #[test]
    fn batch_delete_sql_contains_in_clause() {
        let ops = DefaultBatchOps::new();
        let req =
            BatchDeleteRequest::new("users", "id", vec![json!(1), json!(2), json!(3)]).unwrap();
        let result = batch_delete(&ops, &req);
        assert!(result.base.generated_sqls[0].contains("DELETE FROM"));
        assert!(result.base.generated_sqls[0].contains("IN ("));
        assert!(result.base.generated_sqls[0].contains("`users`"));
    }

    #[test]
    fn batch_delete_params_correct() {
        let ops = DefaultBatchOps::new();
        let req = BatchDeleteRequest::new("users", "id", vec![json!(42), json!(99)]).unwrap();
        let result = batch_delete(&ops, &req);
        assert_eq!(result.sqls_with_params.len(), 1);
        let (_, params) = &result.sqls_with_params[0];
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], json!(42));
        assert_eq!(params[1], json!(99));
    }

    #[test]
    fn batch_delete_sql_injection_table() {
        let ops = DefaultBatchOps::new();
        let req = BatchDeleteRequest::new("users` OR 1=1 --", "id", vec![json!(1)]).unwrap();
        let result = batch_delete(&ops, &req);
        let sql = &result.base.generated_sqls[0];
        assert!(sql.contains("``"));
        assert!(sql.starts_with("DELETE FROM `users`` OR 1=1 --`"));
    }
}
