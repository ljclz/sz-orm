//! 真实 PostgreSQL + pgvector 实现（feature = "real-pg"）
//!
//! 通过 tokio-postgres 连接 PostgreSQL 数据库，执行真实 pgvector SQL。
//!
//! 表结构：
//! - `collections`：存储集合元信息（名称、维度、度量方式）
//! - `vectors_{name}`：每个集合独立一张表，vector 列使用 pgvector 类型
//!
//! # 安全性
//!
//! 所有数据查询使用参数化查询（`$1`、`$2`等），表名通过 `validate_identifier()`
//! 严格校验（仅允许 ASCII 字母数字+下划线），彻底防止 SQL 注入。
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! sz-orm-vector = { version = "0.2", features = ["real-pg"] }
//! ```
//!
//! ```rust,no_run
//! use sz_orm_vector::{PgVectorStore, RealPgConfig, RealPgVectorStore};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = RealPgConfig {
//!     host: "127.0.0.1".to_string(),
//!     port: 5432,
//!     database: "test".to_string(),
//!     username: "postgres".to_string(),
//!     password: "secret".to_string(),
//! };
//! let store = RealPgVectorStore::new(config)?;
//! // 第一次查询会触发连接建立
//! // store.create_collection("docs", 3, None).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::VectorError;
use crate::{PgVectorStore, SearchResult, VectorMetric, VectorRecord};
use async_trait::async_trait;
use tokio::sync::OnceCell;
use tokio_postgres::{Client, Row};

/// 真实 PostgreSQL 配置
#[derive(Debug, Clone)]
pub struct RealPgConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

impl RealPgConfig {
    /// 生成连接字符串
    fn conn_string(&self) -> String {
        format!(
            "host={} port={} dbname={} user={} password={}",
            self.host, self.port, self.database, self.username, self.password
        )
    }
}

/// 真实 PostgreSQL + pgvector 实现
///
/// 连接在第一次查询时延迟建立（解决同步 `new()` 无法 await 的问题）
pub struct RealPgVectorStore {
    config: RealPgConfig,
    client: OnceCell<Client>,
}

impl RealPgVectorStore {
    pub fn new(config: RealPgConfig) -> Result<Self, VectorError> {
        Ok(Self {
            config,
            client: OnceCell::new(),
        })
    }

    /// 延迟建立连接
    async fn client(&self) -> Result<&Client, VectorError> {
        self.client
            .get_or_try_init(|| async {
                let conn_str = self.config.conn_string();
                let (client, connection) =
                    tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
                        .await
                        .map_err(|e| VectorError::Connection(e.to_string()))?;
                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        eprintln!("[sz-orm-vector] postgres connection error: {}", e);
                    }
                });
                Ok::<Client, VectorError>(client)
            })
            .await
    }

    /// 获取集合对应的向量表名
    fn vectors_table_name(collection: &str) -> String {
        format!("vectors_{}", collection)
    }

    /// 执行 DDL（参数化）
    async fn execute_ddl(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<(), VectorError> {
        let client = self.client().await?;
        client
            .execute(sql, params)
            .await
            .map_err(|e| VectorError::Query(e.to_string()))?;
        Ok(())
    }

    /// 执行 INSERT/UPDATE/DELETE，返回受影响行数
    async fn execute(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64, VectorError> {
        let client = self.client().await?;
        client
            .execute(sql, params)
            .await
            .map_err(|e| VectorError::Query(e.to_string()))
    }

    /// 查询单行
    async fn query_opt(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Option<Row>, VectorError> {
        let client = self.client().await?;
        client
            .query_opt(sql, params)
            .await
            .map_err(|e| VectorError::Query(e.to_string()))
    }

    /// 查询多行
    async fn query(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Vec<Row>, VectorError> {
        let client = self.client().await?;
        client
            .query(sql, params)
            .await
            .map_err(|e| VectorError::Query(e.to_string()))
    }
}

/// 校验 SQL 标识符（表名），防止 SQL 注入
fn validate_identifier(name: &str, kind: &str) -> Result<(), VectorError> {
    if name.is_empty() || name.len() > 63 {
        return Err(VectorError::InvalidIdentifier(format!(
            "{} empty or too long (max 63 chars): {}",
            kind, name
        )));
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(VectorError::InvalidIdentifier(format!(
            "{} must start with letter or underscore, got '{}'",
            kind, name
        )));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(VectorError::InvalidIdentifier(format!(
            "{} only alphanumeric and underscore allowed, got '{}'",
            kind, name
        )));
    }
    Ok(())
}

/// 校验维度值
fn validate_dimension(dim: usize) -> Result<(), VectorError> {
    if dim == 0 || dim > 16000 {
        return Err(VectorError::InvalidConfig(format!(
            "dimension must be between 1 and 16000, got {}",
            dim
        )));
    }
    Ok(())
}

#[async_trait]
impl PgVectorStore for RealPgVectorStore {
    async fn create_collection(
        &self,
        name: &str,
        dimension: usize,
        metric: Option<VectorMetric>,
    ) -> Result<(), VectorError> {
        validate_identifier(name, "collection name")?;
        validate_dimension(dimension)?;

        let metric_val = metric.unwrap_or_default();
        let metric_str = metric_val.as_str();
        let tables_name = Self::vectors_table_name(name);

        // 创建 collections 表（如不存在）
        let create_collections_sql = "\
            CREATE TABLE IF NOT EXISTS collections (\
                name TEXT PRIMARY KEY,\
                dimension INT NOT NULL,\
                metric TEXT NOT NULL\
            )";
        self.execute_ddl(create_collections_sql, &[]).await?;

        // 创建集合对应的向量表
        let create_vectors_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (\
                id TEXT NOT NULL,\
                embedding vector({}),\
                metadata JSONB DEFAULT '{{}}'::jsonb,\
                text TEXT DEFAULT '',\
                PRIMARY KEY (id)\
            )",
            tables_name, dimension
        );
        self.execute_ddl(&create_vectors_sql, &[]).await?;

        // 插入集合元信息（ON CONFLICT DO NOTHING）
        let insert_sql = "\
            INSERT INTO collections (name, dimension, metric) \
            VALUES ($1, $2, $3) \
            ON CONFLICT (name) DO UPDATE SET dimension = $2, metric = $3";
        self.execute(insert_sql, &[&name, &(dimension as i32), &metric_str])
            .await?;

        Ok(())
    }

    async fn delete_collection(&self, name: &str) -> Result<(), VectorError> {
        validate_identifier(name, "collection name")?;
        let tables_name = Self::vectors_table_name(name);

        // 删除向量表
        let drop_sql = format!("DROP TABLE IF EXISTS {}", tables_name);
        self.execute_ddl(&drop_sql, &[]).await?;

        // 删除集合元信息
        let delete_sql = "DELETE FROM collections WHERE name = $1";
        self.execute(delete_sql, &[&name]).await?;

        Ok(())
    }

    async fn insert(
        &self,
        collection: &str,
        records: Vec<VectorRecord>,
    ) -> Result<(), VectorError> {
        validate_identifier(collection, "collection name")?;
        let tables_name = Self::vectors_table_name(collection);

        for record in &records {
            let metadata_json = record
                .metadata
                .as_ref()
                .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
                .unwrap_or(serde_json::Value::Null);
            let metadata_str = serde_json::to_string(&metadata_json)
                .map_err(|e| VectorError::Query(format!("metadata serialization: {}", e)))?;

            // UPSERT: ON CONFLICT (id) DO UPDATE
            let sql = format!(
                "INSERT INTO {} (id, embedding, metadata, text) \
                 VALUES ($1, $2::vector, $3::jsonb, $4) \
                 ON CONFLICT (id) DO UPDATE SET \
                 embedding = EXCLUDED.embedding, \
                 metadata = EXCLUDED.metadata, \
                 text = EXCLUDED.text",
                tables_name
            );

            self.execute(
                &sql,
                &[
                    &record.id,
                    &format_vec_f32(&record.vector),
                    &metadata_str,
                    &"",
                ],
            )
            .await?;
        }

        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, VectorError> {
        // M-16 修复：校验 top_k 范围（必须在 [1, MAX_TOP_K] 内）
        let top_k = crate::validate_top_k(top_k)?;

        validate_identifier(collection, "collection name")?;
        let tables_name = Self::vectors_table_name(collection);

        // 先查询集合的度量方式
        let metric_sql = "SELECT metric FROM collections WHERE name = $1";
        let metric_row = self
            .query_opt(metric_sql, &[&collection])
            .await?
            .ok_or_else(|| VectorError::CollectionNotFound(collection.to_string()))?;
        let metric_str: String = metric_row
            .try_get(0)
            .map_err(|e| VectorError::Query(format!("metric column: {}", e)))?;
        let metric = metric_str
            .parse::<VectorMetric>()
            .map_err(|e| VectorError::Query(format!("unknown metric: {}", e)))?;

        let operator = metric.pg_operator();

        // 使用 pgvector 的相似度操作符
        let sql = format!(
            "SELECT id, embedding, metadata, text, \
             (embedding {} $1::vector) AS distance \
             FROM {} \
             ORDER BY distance \
             LIMIT $2",
            operator, tables_name
        );

        let query_str = format_vec_f32(query);
        let limit = top_k as i64;

        let rows = self.query(&sql, &[&query_str, &limit]).await?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row
                .try_get(0)
                .map_err(|e| VectorError::Query(format!("id column: {}", e)))?;
            let embedding_str: String = row
                .try_get(1)
                .map_err(|e| VectorError::Query(format!("embedding column: {}", e)))?;
            let jsonb_str: String = row
                .try_get(2)
                .map_err(|e| VectorError::Query(format!("metadata column: {}", e)))?;
            let text: String = row
                .try_get(3)
                .map_err(|e| VectorError::Query(format!("text column: {}", e)))?;
            let distance: f64 = row
                .try_get(4)
                .map_err(|e| VectorError::Query(format!("distance column: {}", e)))?;

            let vector = parse_vec_f32(&embedding_str);
            let score = metric_to_similarity(metric, distance as f32);
            let metadata: Option<serde_json::Value> = serde_json::from_str(&jsonb_str).ok();

            let mut sr = SearchResult::new(id, score, vector);
            sr = sr.with_text(text);
            if let Some(md) = metadata {
                if let Some(obj) = md.as_object() {
                    let map: std::collections::HashMap<String, serde_json::Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    sr = sr.with_metadata(map);
                }
            }
            results.push(sr);
        }

        Ok(results)
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRecord>, VectorError> {
        validate_identifier(collection, "collection name")?;
        let tables_name = Self::vectors_table_name(collection);

        let sql = format!(
            "SELECT id, embedding, metadata FROM {} WHERE id = $1",
            tables_name
        );

        let row = self.query_opt(&sql, &[&id]).await?;

        match row {
            Some(r) => {
                let rec_id: String = r
                    .try_get(0)
                    .map_err(|e| VectorError::Query(format!("id column: {}", e)))?;
                let embedding_str: String = r
                    .try_get(1)
                    .map_err(|e| VectorError::Query(format!("embedding column: {}", e)))?;
                let jsonb_str: String = r
                    .try_get(2)
                    .map_err(|e| VectorError::Query(format!("metadata column: {}", e)))?;

                let vector = parse_vec_f32(&embedding_str);
                let metadata = serde_json::from_str(&jsonb_str).ok();

                Ok(Some(VectorRecord {
                    id: rec_id,
                    vector,
                    score: None,
                    metadata,
                }))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<u64, VectorError> {
        validate_identifier(collection, "collection name")?;
        let tables_name = Self::vectors_table_name(collection);

        if ids.is_empty() {
            return Ok(0);
        }

        // 构建参数化 IN 查询
        // 每个 id 作为一个参数
        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = ids
            .iter()
            .map(|id| id as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
        let sql = format!(
            "DELETE FROM {} WHERE id IN ({})",
            tables_name,
            placeholders.join(", ")
        );

        let count = self.execute(&sql, &params).await?;
        Ok(count)
    }

    async fn count(&self, collection: &str) -> Result<usize, VectorError> {
        validate_identifier(collection, "collection name")?;
        let tables_name = Self::vectors_table_name(collection);

        let sql = format!("SELECT COUNT(*) FROM {}", tables_name);
        let row = self
            .query_opt(&sql, &[])
            .await?
            .ok_or_else(|| VectorError::Query("count query returned no rows".to_string()))?;
        let count: i64 = row
            .try_get(0)
            .map_err(|e| VectorError::Query(format!("count column: {}", e)))?;
        Ok(count as usize)
    }
}

/// 将 Vec<f32> 格式化为 pgvector 文本格式：`[1,2,3]`
fn format_vec_f32(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(|x| x.to_string()).collect();
    format!("[{}]", parts.join(","))
}

/// 从 pgvector 文本格式解析 Vec<f32>
fn parse_vec_f32(s: &str) -> Vec<f32> {
    let trimmed = s.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);
    if inner.is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .filter_map(|x| x.trim().parse::<f32>().ok())
        .collect()
}

/// 将 pgvector 距离转换为相似度分数
fn metric_to_similarity(metric: VectorMetric, distance: f32) -> f32 {
    match metric {
        // 余弦距离 [0, 2] → 相似度 [1, -1]
        VectorMetric::Cosine => 1.0 - distance,
        // 欧氏距离 [0, ∞) → 相似度 [1, 0)
        VectorMetric::Euclidean => 1.0 / (1.0 + distance),
        // 内积距离（负点积）(-∞, ∞) → 使用负距离
        VectorMetric::DotProduct => -distance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("docs", "collection").is_ok());
        assert!(validate_identifier("my_collection", "collection").is_ok());
        assert!(validate_identifier("vectors_2026", "table").is_ok());
    }

    #[test]
    fn test_validate_identifier_invalid() {
        assert!(validate_identifier("docs; DROP TABLE", "collection").is_err());
        assert!(validate_identifier("col'--", "column").is_err());
        assert!(validate_identifier("1collection", "collection").is_err());
        assert!(validate_identifier("", "table").is_err());
        let long = "a".repeat(64);
        assert!(validate_identifier(&long, "table").is_err());
    }

    #[test]
    fn test_validate_dimension() {
        assert!(validate_dimension(1).is_ok());
        assert!(validate_dimension(128).is_ok());
        assert!(validate_dimension(1536).is_ok());
        assert!(validate_dimension(16000).is_ok());
        assert!(validate_dimension(0).is_err());
        assert!(validate_dimension(16001).is_err());
    }

    #[test]
    fn test_format_parse_vec_f32_roundtrip() {
        let v = vec![1.0, -2.5, 3.5, 0.0];
        let formatted = format_vec_f32(&v);
        let parsed = parse_vec_f32(&formatted);
        assert_eq!(v.len(), parsed.len());
        for (a, b) in v.iter().zip(parsed.iter()) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn test_parse_vec_f32_empty() {
        assert!(parse_vec_f32("[]").is_empty());
        assert!(parse_vec_f32("[ ]").is_empty());
    }

    #[test]
    fn test_metric_to_similarity() {
        // Cosine: distance 0 → similarity 1
        assert!((metric_to_similarity(VectorMetric::Cosine, 0.0) - 1.0).abs() < 1e-6);
        // Euclidean: distance 0 → similarity 1
        assert!((metric_to_similarity(VectorMetric::Euclidean, 0.0) - 1.0).abs() < 1e-6);
        // DotProduct: negative distance = similarity
        assert!((metric_to_similarity(VectorMetric::DotProduct, -5.0) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_vectors_table_name() {
        assert_eq!(
            RealPgVectorStore::vectors_table_name("docs"),
            "vectors_docs"
        );
        assert_eq!(
            RealPgVectorStore::vectors_table_name("my_collection"),
            "vectors_my_collection"
        );
    }

    #[test]
    fn test_new_does_not_connect() {
        let config = RealPgConfig {
            host: "nonexistent.invalid".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "postgres".to_string(),
            password: "secret".to_string(),
        };
        let _store = RealPgVectorStore::new(config).expect("new() should not connect");
    }

    #[test]
    fn test_vector_metric_methods() {
        assert_eq!(VectorMetric::Cosine.pg_operator(), "<=>");
        assert_eq!(VectorMetric::Euclidean.pg_operator(), "<->");
        assert_eq!(VectorMetric::DotProduct.pg_operator(), "<#>");

        assert_eq!(VectorMetric::Cosine.as_str(), "cosine");
        assert_eq!(
            "cosine".parse::<VectorMetric>().ok(),
            Some(VectorMetric::Cosine)
        );
        assert_eq!("unknown".parse::<VectorMetric>().ok(), None::<VectorMetric>);
    }
}
