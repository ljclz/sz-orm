//! NL2SQL 缓存管线（v6.8.0 AI-NL2SQL-01 + AI-NL2SQL-03）
//!
//! 缓存 NL2SQL 生成结果，避免重复调用 LLM。
//! 缓存键 = hash(问句) + hash(schema 摘要) + hash(方言)。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use std::time::{Duration, Instant};

use parking_lot::RwLock;
use sz_orm_ai::nl2sql::{Nl2SqlEngine, Nl2SqlError, SchemaContext, SqlDialect, SqlQuery};

use crate::dialect_renderer::DialectAwareNl2SqlRenderer;

/// NL2SQL 结果
#[derive(Debug, Clone)]
pub struct Nl2SqlResult {
    /// 生成的 SQL 查询
    pub query: SqlQuery,
    /// 置信度
    pub confidence: f32,
    /// 是否命中缓存
    pub cache_hit: bool,
    /// 警告
    pub warning: Option<Nl2SqlWarning>,
}

/// NL2SQL 警告
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nl2SqlWarning {
    /// 置信度低于阈值
    LowConfidence,
    /// LLM 不可达，使用降级策略
    LlmUnreachable,
    /// SQL 校验失败
    InvalidSql,
}

/// 缓存条目
struct CacheEntry {
    result: Nl2SqlResult,
    inserted_at: Instant,
}

/// NL2SQL 缓存管线
pub struct CachedNl2SqlPipeline<E: Nl2SqlEngine> {
    renderer: DialectAwareNl2SqlRenderer<E>,
    cache: RwLock<HashMap<u64, CacheEntry>>,
    ttl: Duration,
    confidence_threshold: f32,
    llm_call_count: AtomicU64,
}

impl<E: Nl2SqlEngine> CachedNl2SqlPipeline<E> {
    /// 创建缓存管线
    pub fn new(renderer: DialectAwareNl2SqlRenderer<E>, ttl: Duration) -> Self {
        Self {
            renderer,
            cache: RwLock::new(HashMap::new()),
            ttl,
            confidence_threshold: 0.7,
            llm_call_count: AtomicU64::new(0),
        }
    }

    /// 设置置信度阈值
    pub fn with_confidence_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    /// 执行缓存查询
    pub async fn execute_cached(
        &self,
        nl_query: &str,
        schema: &SchemaContext,
        dialect: SqlDialect,
    ) -> Result<Nl2SqlResult, Nl2SqlError> {
        let cache_key = compute_cache_key(nl_query, schema, dialect);

        {
            let cache = self.cache.read();
            if let Some(entry) = cache.get(&cache_key) {
                if entry.inserted_at.elapsed() < self.ttl {
                    let mut result = entry.result.clone();
                    result.cache_hit = true;
                    result.query.cache_hit = true;
                    return Ok(result);
                }
            }
        }

        self.llm_call_count.fetch_add(1, Ordering::SeqCst);
        let query = self.renderer.render(nl_query, schema, dialect).await?;

        let warning = if query.confidence < self.confidence_threshold {
            Some(Nl2SqlWarning::LowConfidence)
        } else {
            None
        };

        let result = Nl2SqlResult {
            query: query.clone(),
            confidence: query.confidence,
            cache_hit: false,
            warning,
        };

        {
            let mut cache = self.cache.write();
            cache.insert(
                cache_key,
                CacheEntry {
                    result: result.clone(),
                    inserted_at: Instant::now(),
                },
            );
        }

        Ok(result)
    }

    /// 清除缓存
    pub fn clear_cache(&self) {
        self.cache.write().clear();
    }

    /// 返回缓存条目数
    pub fn cache_size(&self) -> usize {
        self.cache.read().len()
    }

    /// 返回置信度阈值
    pub fn confidence_threshold(&self) -> f32 {
        self.confidence_threshold
    }

    /// 返回 LLM 调用次数（未命中缓存时递增）
    pub fn llm_call_count(&self) -> u64 {
        self.llm_call_count.load(Ordering::SeqCst)
    }
}

fn compute_cache_key(nl_query: &str, schema: &SchemaContext, dialect: SqlDialect) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    nl_query.hash(&mut hasher);
    dialect.hash(&mut hasher);
    for table in &schema.tables {
        table.name.hash(&mut hasher);
        for col in &table.columns {
            col.name.hash(&mut hasher);
            col.data_type.hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_ai::nl2sql::{ColumnInfo, SimpleNl2SqlEngine, TableInfo};

    fn make_schema() -> SchemaContext {
        SchemaContext {
            tables: vec![TableInfo {
                name: "users".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                        is_primary_key: true,
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        data_type: "TEXT".to_string(),
                        nullable: true,
                        is_primary_key: false,
                    },
                ],
            }],
        }
    }

    fn make_pipeline() -> CachedNl2SqlPipeline<SimpleNl2SqlEngine> {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        CachedNl2SqlPipeline::new(renderer, Duration::from_secs(60))
    }

    #[tokio::test]
    async fn first_call_misses_cache() {
        let pipeline = make_pipeline();
        let schema = make_schema();
        let result = pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert!(!result.cache_hit);
        assert_eq!(pipeline.cache_size(), 1);
    }

    #[tokio::test]
    async fn second_call_hits_cache() {
        let pipeline = make_pipeline();
        let schema = make_schema();
        pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        let result = pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert!(result.cache_hit);
    }

    #[tokio::test]
    async fn different_queries_have_different_cache_keys() {
        let pipeline = make_pipeline();
        let schema = make_schema();
        pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        let result = pipeline
            .execute_cached("count users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert!(!result.cache_hit);
        assert_eq!(pipeline.cache_size(), 2);
    }

    #[tokio::test]
    async fn clear_cache_resets() {
        let pipeline = make_pipeline();
        let schema = make_schema();
        pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(pipeline.cache_size(), 1);
        pipeline.clear_cache();
        assert_eq!(pipeline.cache_size(), 0);
    }

    #[tokio::test]
    async fn low_confidence_produces_warning() {
        let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
        let pipeline = CachedNl2SqlPipeline::new(renderer, Duration::from_secs(60))
            .with_confidence_threshold(0.99);
        let schema = make_schema();
        let result = pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(result.warning, Some(Nl2SqlWarning::LowConfidence));
    }

    #[tokio::test]
    async fn high_confidence_no_warning() {
        let pipeline = make_pipeline();
        let schema = make_schema();
        let result = pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(result.warning, None);
    }

    #[tokio::test]
    async fn llm_call_count_increments_on_miss() {
        let pipeline = make_pipeline();
        let schema = make_schema();
        assert_eq!(pipeline.llm_call_count(), 0);
        pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(pipeline.llm_call_count(), 1);
    }

    #[tokio::test]
    async fn llm_call_count_unchanged_on_hit() {
        let pipeline = make_pipeline();
        let schema = make_schema();
        pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(pipeline.llm_call_count(), 1);
        pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(pipeline.llm_call_count(), 1);
    }

    #[tokio::test]
    async fn cache_hit_returns_same_sql() {
        let pipeline = make_pipeline();
        let schema = make_schema();
        let first = pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        let second = pipeline
            .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(first.query.sql, second.query.sql);
        assert!(second.cache_hit);
        assert!(second.query.cache_hit);
    }
}
