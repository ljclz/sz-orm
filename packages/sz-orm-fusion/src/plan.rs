//! 融合查询规划：结构化查询 → 执行计划（缓存下推 / 搜索下推 / 主库）

/// 缓存后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheBackend {
    /// Redis（生产）
    Redis,
    /// 进程内内存（POC/测试）
    Memory,
}

/// 搜索后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBackend {
    /// 向量检索（复用 sz-orm-vector 混合搜索能力）
    Vector,
}

/// 融合查询配置
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusionConfig {
    /// 主库标识（如 "mysql" / "postgres"）
    pub primary: String,
    /// 缓存后端（None = 禁用缓存下推）
    pub cache: Option<CacheBackend>,
    /// 搜索后端（None = 禁用搜索下推）
    pub search: Option<SearchBackend>,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            primary: "primary".into(),
            cache: None,
            search: None,
        }
    }
}

/// 结构化查询描述（由调用方从 QueryBuilder 或直接构造）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusionQuery {
    /// 目标表
    pub table: String,
    /// 参数化等值条件（`(列, 值)`，可构成缓存键 / 可下推搜索）
    pub eq_conditions: Vec<(String, String)>,
    /// 其他条件（仅主库可执行）
    pub other_conditions: Vec<String>,
    /// 行数限制
    pub limit: Option<u64>,
}

impl FusionQuery {
    /// 创建查询
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            eq_conditions: Vec::new(),
            other_conditions: Vec::new(),
            limit: None,
        }
    }

    /// 添加等值条件（参数化，可下推）
    pub fn eq(mut self, column: impl Into<String>, value: impl Into<String>) -> Self {
        self.eq_conditions.push((column.into(), value.into()));
        self
    }

    /// 添加其他条件（仅主库）
    pub fn cond(mut self, condition: impl Into<String>) -> Self {
        self.other_conditions.push(condition.into());
        self
    }

    /// 设置行数限制
    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// 执行计划步骤
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStep {
    /// 缓存键查找（命中则跳过主库）
    CacheLookup { key: String, table: String },
    /// 搜索下推（向量/全文检索候选集）
    SearchPushdown { table: String, term: String },
    /// 主库查询（含其他条件与限制）
    Primary { table: String },
}

/// 融合执行计划
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusionPlan {
    /// 按序执行的步骤
    pub steps: Vec<PlanStep>,
    /// 缓存键（可下推时存在）
    pub cache_key: Option<String>,
}

/// 融合查询规划器（纯静态分析，无副作用）
pub struct FusionPlanner;

impl FusionPlanner {
    /// 分析查询 → 生成执行计划
    ///
    /// 规则（POC 保守范围，仅可证明安全的拆分）：
    /// 1. 存在等值条件且配置缓存 → `CacheLookup` 步骤（键 = `table:col=val:...`）
    /// 2. 配置搜索且存在搜索词（`other_conditions` 中以 `search:` 前缀标记）→ `SearchPushdown`
    /// 3. 主库步骤始终存在（缓存/搜索仅加速，不替代主库正确性）
    pub fn plan(query: &FusionQuery, config: &FusionConfig) -> FusionPlan {
        let mut steps = Vec::new();
        let mut cache_key = None;

        // 1. 缓存下推：等值条件构成确定性键
        if let Some(backend) = config.cache {
            if !query.eq_conditions.is_empty() {
                let key = build_cache_key(query);
                cache_key = Some(key.clone());
                steps.push(PlanStep::CacheLookup {
                    key,
                    table: query.table.clone(),
                });
                let _ = backend; // 后端类型仅用于配置记录，执行由注入的 trait 完成
            }
        }

        // 2. 搜索下推：`search: <term>` 前缀条件
        if config.search.is_some() {
            if let Some(term) = query
                .other_conditions
                .iter()
                .find_map(|c| c.strip_prefix("search: "))
            {
                steps.push(PlanStep::SearchPushdown {
                    table: query.table.clone(),
                    term: term.to_string(),
                });
            }
        }

        // 3. 主库步骤
        steps.push(PlanStep::Primary {
            table: query.table.clone(),
        });

        FusionPlan { steps, cache_key }
    }
}

/// 构造确定性缓存键：`table:col1=val1:col2=val2`
pub fn build_cache_key(query: &FusionQuery) -> String {
    let mut parts: Vec<String> = query
        .eq_conditions
        .iter()
        .map(|(c, v)| format!("{c}={v}"))
        .collect();
    parts.sort_unstable(); // 等值条件无序，排序保证键确定性
    format!("{}:{}", query.table, parts.join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_pushdown_with_eq_conditions() {
        let config = FusionConfig {
            cache: Some(CacheBackend::Memory),
            ..Default::default()
        };
        let q = FusionQuery::new("users").eq("id", "42");
        let plan = FusionPlanner::plan(&q, &config);
        assert!(matches!(
            plan.steps[0],
            PlanStep::CacheLookup { ref key, .. } if key == "users:id=42"
        ));
        assert_eq!(plan.cache_key.as_deref(), Some("users:id=42"));
        assert_eq!(plan.steps.len(), 2); // CacheLookup + Primary
    }

    #[test]
    fn cache_key_is_order_independent() {
        let a = FusionQuery::new("users").eq("a", "1").eq("b", "2");
        let b = FusionQuery::new("users").eq("b", "2").eq("a", "1");
        assert_eq!(build_cache_key(&a), build_cache_key(&b));
        assert_eq!(build_cache_key(&a), "users:a=1:b=2");
    }

    #[test]
    fn no_cache_config_means_primary_only() {
        let config = FusionConfig::default(); // cache = None
        let q = FusionQuery::new("users").eq("id", "42");
        let plan = FusionPlanner::plan(&q, &config);
        assert!(plan.cache_key.is_none());
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(
            plan.steps[0],
            PlanStep::Primary {
                table: "users".into()
            }
        );
    }

    #[test]
    fn search_pushdown_with_search_prefix() {
        let config = FusionConfig {
            search: Some(SearchBackend::Vector),
            ..Default::default()
        };
        let q = FusionQuery::new("products")
            .cond("search: 无线耳机")
            .cond("price < 500");
        let plan = FusionPlanner::plan(&q, &config);
        assert!(plan.steps.contains(&PlanStep::SearchPushdown {
            table: "products".into(),
            term: "无线耳机".into()
        }));
        assert!(plan.steps.contains(&PlanStep::Primary {
            table: "products".into()
        }));
    }

    #[test]
    fn search_prefix_without_backend_is_primary_only() {
        let config = FusionConfig::default();
        let q = FusionQuery::new("products").cond("search: 耳机");
        let plan = FusionPlanner::plan(&q, &config);
        assert_eq!(plan.steps.len(), 1);
    }
}
