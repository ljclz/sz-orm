//! 融合执行器：按计划执行（缓存命中跳过主库；主库失败降级回缓存）

use crate::plan::{FusionConfig, FusionPlanner, FusionQuery, PlanStep};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 融合缓存抽象（Redis 或内存实现均可注入）
pub trait FusionCache: Send + Sync {
    /// 读取缓存（JSON 字符串；未命中返回 `None`）
    fn get(&self, key: &str) -> Option<String>;
    /// 写入缓存（JSON 字符串）
    fn set(&self, key: &str, value: String);
}

/// 进程内内存缓存（POC/测试用；生产可替换为 Redis 实现）
pub struct MemoryFusionCache {
    inner: Mutex<std::collections::HashMap<String, String>>,
}

impl MemoryFusionCache {
    /// 创建空缓存
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for MemoryFusionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FusionCache for MemoryFusionCache {
    fn get(&self, key: &str) -> Option<String> {
        self.inner.lock().ok()?.get(key).cloned()
    }

    fn set(&self, key: &str, value: String) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert(key.to_string(), value);
        }
    }
}

/// 一次融合查询的结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusionOutcome {
    /// 聚合后的结果行（JSON 对象）
    pub rows: Vec<serde_json::Value>,
    /// 结果是否来自缓存（主库被跳过）
    pub from_cache: bool,
    /// 是否降级（主库失败，返回缓存旧数据）
    pub degraded: bool,
    /// 实际参与的数据源（"cache" / "primary" / "search"）
    pub sources: Vec<String>,
    /// 总耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 融合执行器
pub struct FusionExecutor {
    config: FusionConfig,
    cache: Option<Arc<dyn FusionCache>>,
}

impl FusionExecutor {
    /// 创建执行器
    pub fn new(config: FusionConfig) -> Self {
        Self {
            config,
            cache: None,
        }
    }

    /// 注入缓存后端实现
    pub fn with_cache(mut self, cache: Arc<dyn FusionCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// 当前配置
    pub fn config(&self) -> &FusionConfig {
        &self.config
    }

    /// 执行一次融合查询。
    ///
    /// `primary` 闭包执行主库查询（参数化，由调用方保证）；
    /// 流程：规划 → 缓存命中（返回，主库跳过）→ 主库执行（回填缓存）→
    /// 主库失败且缓存可读（降级返回旧数据，标记 `degraded`）。
    pub fn execute<F>(&self, query: &FusionQuery, primary: F) -> Result<FusionOutcome, String>
    where
        F: FnOnce(&FusionQuery) -> Result<Vec<serde_json::Value>, String>,
    {
        let start = Instant::now();
        let plan = FusionPlanner::plan(query, &self.config);
        let mut sources = Vec::new();

        // 1. 缓存下推：命中直接返回（主库跳过）
        if let Some(key) = &plan.cache_key {
            if let Some(cache) = &self.cache {
                if let Some(cached) = cache.get(key) {
                    let rows = parse_rows(&cached)?;
                    return Ok(FusionOutcome {
                        rows,
                        from_cache: true,
                        degraded: false,
                        sources: vec!["cache".into()],
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    });
                }
            }
        }

        // 2. 搜索下推：POC 阶段仅记录数据源（真实向量检索由调用方在 primary 闭包内完成）
        if plan
            .steps
            .iter()
            .any(|s| matches!(s, PlanStep::SearchPushdown { .. }))
        {
            sources.push("search".into());
        }

        // 3. 主库执行
        sources.push("primary".into());
        match primary(query) {
            Ok(rows) => {
                // 回填缓存（有缓存键时）
                if let (Some(key), Some(cache)) = (&plan.cache_key, &self.cache) {
                    if let Ok(json) = serde_json::to_string(&rows) {
                        cache.set(key, json);
                    }
                }
                Ok(FusionOutcome {
                    rows,
                    from_cache: false,
                    degraded: false,
                    sources,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                })
            }
            Err(primary_err) => {
                // 4. 降级：主库失败且缓存可读 → 返回缓存旧数据
                if let Some(key) = &plan.cache_key {
                    if let Some(cache) = &self.cache {
                        if let Some(cached) = cache.get(key) {
                            let rows = parse_rows(&cached)?;
                            return Ok(FusionOutcome {
                                rows,
                                from_cache: true,
                                degraded: true,
                                sources: vec!["cache".into()],
                                elapsed_ms: start.elapsed().as_millis() as u64,
                            });
                        }
                    }
                }
                Err(format!(
                    "primary query failed and no cache fallback: {primary_err}"
                ))
            }
        }
    }
}

/// 解析缓存中的 JSON 行数组
fn parse_rows(json: &str) -> Result<Vec<serde_json::Value>, String> {
    serde_json::from_str(json).map_err(|e| format!("cache payload parse failed: {e}"))
}

/// 校验配置辅助：缓存配置存在但未注入实现时给出明确提示
pub fn cache_configured_without_backend(
    config: &FusionConfig,
    cache: Option<&Arc<dyn FusionCache>>,
) -> Option<String> {
    if config.cache.is_some() && cache.is_none() {
        Some(format!(
            "cache backend configured ({:?}) but no FusionCache implementation injected",
            config.cache
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::CacheBackend;

    fn row(id: &str) -> serde_json::Value {
        serde_json::json!({ "id": id })
    }

    #[test]
    fn cache_hit_skips_primary() {
        let config = FusionConfig {
            cache: Some(CacheBackend::Memory),
            ..Default::default()
        };
        let cache = Arc::new(MemoryFusionCache::new());
        cache.set(
            "users:id=42",
            serde_json::to_string(&vec![row("42")]).unwrap(),
        );
        let ex = FusionExecutor::new(config).with_cache(cache);

        let mut primary_calls = 0;
        let out = ex
            .execute(&FusionQuery::new("users").eq("id", "42"), |_q| {
                primary_calls += 1;
                Ok(vec![row("fresh")])
            })
            .unwrap();
        assert!(out.from_cache);
        assert_eq!(out.rows, vec![row("42")]);
        assert_eq!(primary_calls, 0, "primary must be skipped on cache hit");
        assert_eq!(out.sources, vec!["cache"]);
    }

    #[test]
    fn primary_executes_and_backfills_cache() {
        let config = FusionConfig {
            cache: Some(CacheBackend::Memory),
            ..Default::default()
        };
        let ex = FusionExecutor::new(config).with_cache(Arc::new(MemoryFusionCache::new()));

        let out = ex
            .execute(&FusionQuery::new("users").eq("id", "1"), |_q| {
                Ok(vec![row("1")])
            })
            .unwrap();
        assert!(!out.from_cache);
        assert!(!out.degraded);
        assert!(out.sources.contains(&"primary".to_string()));

        // 第二次应命中缓存
        let second = ex
            .execute(&FusionQuery::new("users").eq("id", "1"), |_q| {
                Ok(vec![row("fresh")])
            })
            .unwrap();
        assert!(second.from_cache);
        assert_eq!(second.rows, vec![row("1")]);
    }

    #[test]
    fn primary_failure_degrades_to_cache() {
        let config = FusionConfig {
            cache: Some(CacheBackend::Memory),
            ..Default::default()
        };
        let cache = Arc::new(MemoryFusionCache::new());
        let cache_clone = Arc::clone(&cache);
        let ex = FusionExecutor::new(config).with_cache(cache);

        let out = ex
            .execute(&FusionQuery::new("users").eq("id", "9"), |_q| {
                // 模拟并发：主库执行期间缓存被其他请求回填（旧数据仍可读）
                cache_clone.set(
                    "users:id=9",
                    serde_json::to_string(&vec![row("stale")]).unwrap(),
                );
                Err("primary db down".into())
            })
            .unwrap();
        assert!(out.degraded, "must be flagged as degraded");
        assert!(out.from_cache);
        assert_eq!(out.rows, vec![row("stale")]);
    }

    #[test]
    fn primary_failure_without_cache_is_error() {
        let config = FusionConfig::default(); // 无缓存
        let ex = FusionExecutor::new(config);
        let err = ex
            .execute(&FusionQuery::new("users").eq("id", "9"), |_q| {
                Err("primary db down".into())
            })
            .unwrap_err();
        assert!(err.contains("primary query failed"));
    }

    #[test]
    fn search_pushdown_records_source() {
        let config = FusionConfig {
            search: Some(crate::plan::SearchBackend::Vector),
            ..Default::default()
        };
        let ex = FusionExecutor::new(config);
        let out = ex
            .execute(&FusionQuery::new("products").cond("search: 耳机"), |_q| {
                Ok(vec![row("p1")])
            })
            .unwrap();
        assert!(out.sources.contains(&"search".to_string()));
        assert!(out.sources.contains(&"primary".to_string()));
        assert!(!out.degraded);
    }

    #[test]
    fn cache_config_without_backend_hints() {
        let config = FusionConfig {
            cache: Some(CacheBackend::Redis),
            ..Default::default()
        };
        let hint = cache_configured_without_backend(&config, None);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("Redis"));
    }

    #[test]
    fn primary_only_when_no_cache() {
        let ex = FusionExecutor::new(FusionConfig::default());
        let out = ex
            .execute(&FusionQuery::new("users"), |_q| Ok(vec![row("a")]))
            .unwrap();
        assert_eq!(out.sources, vec!["primary"]);
    }
}
