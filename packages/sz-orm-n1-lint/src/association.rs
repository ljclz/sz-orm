//! 查询关联分析器
//!
//! 分析查询间的关联关系（如主查询 + 子查询），
//! 识别潜在的批量加载机会。

use std::collections::{HashMap, HashSet};

use crate::QueryMethod;

/// 查询关联类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationType {
    /// 一对一关联
    OneToOne,
    /// 一对多关联
    OneToMany,
    /// 多对一关联（属于）
    ManyToOne,
    /// 多对多关联
    ManyToMany,
}

impl AssociationType {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            AssociationType::OneToOne => "one-to-one",
            AssociationType::OneToMany => "one-to-many",
            AssociationType::ManyToOne => "many-to-one",
            AssociationType::ManyToMany => "many-to-many",
        }
    }

    /// 是否可批量加载
    pub fn is_batchable(&self) -> bool {
        matches!(
            self,
            AssociationType::OneToMany | AssociationType::ManyToOne | AssociationType::ManyToMany
        )
    }
}

/// 查询关联关系
#[derive(Debug, Clone)]
pub struct QueryAssociation {
    /// 主查询表
    pub parent_table: String,
    /// 子查询表
    pub child_table: String,
    /// 关联类型
    pub association_type: AssociationType,
    /// 关联键（外键字段）
    pub foreign_key: String,
    /// 使用的查询方法
    pub query_method: QueryMethod,
}

impl QueryAssociation {
    /// 创建关联关系
    pub fn new(
        parent_table: impl Into<String>,
        child_table: impl Into<String>,
        association_type: AssociationType,
        foreign_key: impl Into<String>,
    ) -> Self {
        Self {
            parent_table: parent_table.into(),
            child_table: child_table.into(),
            association_type,
            foreign_key: foreign_key.into(),
            query_method: QueryMethod::FindById,
        }
    }

    /// 设置查询方法
    pub fn with_query_method(mut self, method: QueryMethod) -> Self {
        self.query_method = method;
        self
    }

    /// 是否可批量加载
    pub fn is_batchable(&self) -> bool {
        self.association_type.is_batchable() && self.query_method.is_batchable()
    }
}

/// 查询关联分析器
///
/// 注册查询间的关联关系，分析 N+1 风险与批量加载机会。
pub struct QueryAssociationAnalyzer {
    /// 已注册的关联关系
    associations: Vec<QueryAssociation>,
    /// 表依赖图：parent -> children
    dependency_graph: HashMap<String, HashSet<String>>,
}

impl QueryAssociationAnalyzer {
    /// 创建空分析器
    pub fn new() -> Self {
        Self {
            associations: Vec::new(),
            dependency_graph: HashMap::new(),
        }
    }

    /// 注册关联关系
    pub fn register(&mut self, association: QueryAssociation) {
        self.dependency_graph
            .entry(association.parent_table.clone())
            .or_default()
            .insert(association.child_table.clone());
        self.associations.push(association);
    }

    /// 所有关联关系
    pub fn associations(&self) -> &[QueryAssociation] {
        &self.associations
    }

    /// 获取某表的所有子表
    pub fn children_of(&self, table: &str) -> Option<&HashSet<String>> {
        self.dependency_graph.get(table)
    }

    /// 查找可批量加载的关联
    pub fn batchable_associations(&self) -> Vec<&QueryAssociation> {
        self.associations
            .iter()
            .filter(|a| a.is_batchable())
            .collect()
    }

    /// 查找 N+1 风险关联（可批量但未使用批量方法）
    pub fn n1_risk_associations(&self) -> Vec<&QueryAssociation> {
        self.associations
            .iter()
            .filter(|a| a.association_type.is_batchable() && !a.query_method.is_batchable())
            .collect()
    }

    /// 检测循环依赖（A→B→A）
    pub fn has_circular_dependency(&self) -> bool {
        let all_tables: HashSet<&String> = self.dependency_graph.keys().collect();
        for &start in &all_tables {
            let mut visited = HashSet::new();
            if self.detect_cycle_from(start, start, &mut visited) {
                return true;
            }
        }
        false
    }

    fn detect_cycle_from(
        &self,
        current: &str,
        target: &str,
        visited: &mut HashSet<String>,
    ) -> bool {
        if let Some(children) = self.dependency_graph.get(current) {
            for child in children {
                if child == target && current != target {
                    return true;
                }
                if !visited.contains(child) {
                    visited.insert(child.clone());
                    if self.detect_cycle_from(child, target, visited) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 获取表的依赖深度（最大嵌套层数）
    pub fn dependency_depth(&self, table: &str) -> usize {
        let mut visited = HashSet::new();
        self.compute_depth(table, &mut visited)
    }

    fn compute_depth(&self, table: &str, visited: &mut HashSet<String>) -> usize {
        if visited.contains(table) {
            return 0;
        }
        visited.insert(table.to_string());
        let children = self.dependency_graph.get(table);
        let max_child_depth = match children {
            Some(c) if !c.is_empty() => {
                let mut max_d = 0;
                for child in c {
                    let d = self.compute_depth(child, visited);
                    if d > max_d {
                        max_d = d;
                    }
                }
                max_d
            }
            _ => 0,
        };
        1 + max_child_depth
    }

    /// 关联总数
    pub fn count(&self) -> usize {
        self.associations.len()
    }

    /// 涉及的表数
    pub fn table_count(&self) -> usize {
        let mut tables: HashSet<&String> = HashSet::new();
        for a in &self.associations {
            tables.insert(&a.parent_table);
            tables.insert(&a.child_table);
        }
        tables.len()
    }
}

impl Default for QueryAssociationAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// 批量加载策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchLoadStrategy {
    /// `where_in` 批量查询
    WhereIn,
    /// `eager_load` 预加载
    EagerLoad,
    /// JOIN 联合查询
    Join,
    /// 子查询
    Subquery,
}

impl BatchLoadStrategy {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            BatchLoadStrategy::WhereIn => "where-in",
            BatchLoadStrategy::EagerLoad => "eager-load",
            BatchLoadStrategy::Join => "join",
            BatchLoadStrategy::Subquery => "subquery",
        }
    }

    /// 推荐的批量大小上限
    pub fn recommended_batch_size(&self) -> usize {
        match self {
            BatchLoadStrategy::WhereIn => 500,
            BatchLoadStrategy::EagerLoad => 1000,
            BatchLoadStrategy::Join => 100,
            BatchLoadStrategy::Subquery => 200,
        }
    }
}

/// 批量加载建议
#[derive(Debug, Clone)]
pub struct BatchLoadSuggestion {
    /// 主表
    pub parent_table: String,
    /// 子表
    pub child_table: String,
    /// 建议的批量策略
    pub strategy: BatchLoadStrategy,
    /// 建议的批量大小
    pub batch_size: usize,
    /// 关联键
    pub foreign_key: String,
    /// 原因
    pub reason: String,
}

/// 批量加载建议器
///
/// 根据关联关系生成批量加载建议。
pub struct BatchLoadAdvisor {
    /// 默认批量大小
    default_batch_size: usize,
}

impl BatchLoadAdvisor {
    /// 创建建议器
    pub fn new(default_batch_size: usize) -> Self {
        Self {
            default_batch_size: default_batch_size.max(1),
        }
    }

    /// 使用默认批量大小 100 创建
    pub fn with_defaults() -> Self {
        Self::new(100)
    }

    /// 为单个关联生成建议
    pub fn advise(&self, association: &QueryAssociation) -> Option<BatchLoadSuggestion> {
        if !association.association_type.is_batchable() {
            return None;
        }
        let strategy = self.select_strategy(association);
        let batch_size = strategy
            .recommended_batch_size()
            .min(self.default_batch_size);
        Some(BatchLoadSuggestion {
            parent_table: association.parent_table.clone(),
            child_table: association.child_table.clone(),
            strategy,
            batch_size,
            foreign_key: association.foreign_key.clone(),
            reason: format!(
                "use {} to batch load {} for {} (batch size {})",
                strategy.as_str(),
                association.child_table,
                association.parent_table,
                batch_size
            ),
        })
    }

    /// 为多个关联批量生成建议
    pub fn advise_batch(&self, associations: &[QueryAssociation]) -> Vec<BatchLoadSuggestion> {
        associations.iter().filter_map(|a| self.advise(a)).collect()
    }

    /// 选择批量策略
    fn select_strategy(&self, association: &QueryAssociation) -> BatchLoadStrategy {
        match association.association_type {
            AssociationType::ManyToOne => BatchLoadStrategy::WhereIn,
            AssociationType::OneToMany => BatchLoadStrategy::EagerLoad,
            AssociationType::ManyToMany => BatchLoadStrategy::Join,
            AssociationType::OneToOne => BatchLoadStrategy::Join,
        }
    }

    /// 默认批量大小
    pub fn default_batch_size(&self) -> usize {
        self.default_batch_size
    }
}

impl Default for BatchLoadAdvisor {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// 预加载计划
///
/// 描述一组表的预加载顺序与策略。
#[derive(Debug, Clone)]
pub struct EagerLoadPlan {
    /// 主表
    pub root_table: String,
    /// 预加载路径（按顺序）
    pub load_paths: Vec<LoadPath>,
}

/// 单条预加载路径
#[derive(Debug, Clone)]
pub struct LoadPath {
    /// 路径段（如 ["users", "orders", "items"]）
    pub segments: Vec<String>,
    /// 使用的批量策略
    pub strategy: BatchLoadStrategy,
    /// 批量大小
    pub batch_size: usize,
}

impl EagerLoadPlan {
    /// 创建预加载计划
    pub fn new(root_table: impl Into<String>) -> Self {
        Self {
            root_table: root_table.into(),
            load_paths: Vec::new(),
        }
    }

    /// 添加加载路径
    pub fn add_path(&mut self, path: LoadPath) {
        self.load_paths.push(path);
    }

    /// 路径数
    pub fn path_count(&self) -> usize {
        self.load_paths.len()
    }

    /// 最大嵌套深度
    pub fn max_depth(&self) -> usize {
        self.load_paths
            .iter()
            .map(|p| p.segments.len())
            .max()
            .unwrap_or(0)
    }

    /// 所有涉及的表
    pub fn all_tables(&self) -> Vec<String> {
        let mut tables: HashSet<String> = HashSet::new();
        tables.insert(self.root_table.clone());
        for path in &self.load_paths {
            for seg in &path.segments {
                tables.insert(seg.clone());
            }
        }
        let mut result: Vec<String> = tables.into_iter().collect();
        result.sort();
        result
    }
}

impl LoadPath {
    /// 创建加载路径
    pub fn new(segments: Vec<String>, strategy: BatchLoadStrategy) -> Self {
        let batch_size = strategy.recommended_batch_size();
        Self {
            segments,
            strategy,
            batch_size,
        }
    }

    /// 路径深度
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// 路径字符串表示（如 "users.orders.items"）
    pub fn as_path_string(&self) -> String {
        self.segments.join(".")
    }
}

/// 预加载计划生成器
///
/// 根据关联关系图生成预加载计划。
pub struct EagerLoadPlanner {
    /// 最大嵌套深度
    max_depth: usize,
}

impl EagerLoadPlanner {
    /// 创建计划生成器
    pub fn new(max_depth: usize) -> Self {
        Self { max_depth }
    }

    /// 使用默认最大深度 3 创建
    pub fn with_defaults() -> Self {
        Self::new(3)
    }

    /// 为指定根表生成预加载计划
    pub fn plan(&self, root_table: &str, analyzer: &QueryAssociationAnalyzer) -> EagerLoadPlan {
        let mut plan = EagerLoadPlan::new(root_table);
        let mut visited = HashSet::new();
        visited.insert(root_table.to_string());
        self.build_paths(root_table, analyzer, &mut plan, &mut visited, 1);
        plan
    }

    fn build_paths(
        &self,
        current: &str,
        analyzer: &QueryAssociationAnalyzer,
        plan: &mut EagerLoadPlan,
        visited: &mut HashSet<String>,
        depth: usize,
    ) {
        if depth > self.max_depth {
            return;
        }
        let children = match analyzer.children_of(current) {
            Some(c) => c,
            None => return,
        };
        for child in children {
            if visited.contains(child) {
                continue;
            }
            let strategy = self.select_strategy(current, child, analyzer);
            let segments = self.build_segments(current, child, analyzer, visited, depth);
            plan.add_path(LoadPath::new(segments, strategy));
        }
    }

    fn build_segments(
        &self,
        current: &str,
        child: &str,
        analyzer: &QueryAssociationAnalyzer,
        visited: &mut HashSet<String>,
        depth: usize,
    ) -> Vec<String> {
        let mut segments = vec![current.to_string(), child.to_string()];
        visited.insert(child.to_string());
        if depth < self.max_depth {
            if let Some(grandchildren) = analyzer.children_of(child) {
                for gc in grandchildren {
                    if !visited.contains(gc) {
                        visited.insert(gc.clone());
                        segments.push(gc.clone());
                        break;
                    }
                }
            }
        }
        segments
    }

    fn select_strategy(
        &self,
        parent: &str,
        child: &str,
        analyzer: &QueryAssociationAnalyzer,
    ) -> BatchLoadStrategy {
        for assoc in analyzer.associations() {
            if assoc.parent_table == parent && assoc.child_table == child {
                return match assoc.association_type {
                    AssociationType::ManyToOne => BatchLoadStrategy::WhereIn,
                    AssociationType::OneToMany => BatchLoadStrategy::EagerLoad,
                    AssociationType::ManyToMany => BatchLoadStrategy::Join,
                    AssociationType::OneToOne => BatchLoadStrategy::Join,
                };
            }
        }
        BatchLoadStrategy::EagerLoad
    }

    /// 最大深度
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }
}

impl Default for EagerLoadPlanner {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- AssociationType tests ---

    #[test]
    fn association_type_as_str() {
        assert_eq!(AssociationType::OneToOne.as_str(), "one-to-one");
        assert_eq!(AssociationType::OneToMany.as_str(), "one-to-many");
        assert_eq!(AssociationType::ManyToOne.as_str(), "many-to-one");
        assert_eq!(AssociationType::ManyToMany.as_str(), "many-to-many");
    }

    #[test]
    fn association_type_is_batchable() {
        assert!(!AssociationType::OneToOne.is_batchable());
        assert!(AssociationType::OneToMany.is_batchable());
        assert!(AssociationType::ManyToOne.is_batchable());
        assert!(AssociationType::ManyToMany.is_batchable());
    }

    // --- QueryAssociation tests ---

    #[test]
    fn association_new() {
        let a = QueryAssociation::new("users", "orders", AssociationType::OneToMany, "user_id");
        assert_eq!(a.parent_table, "users");
        assert_eq!(a.child_table, "orders");
        assert_eq!(a.association_type, AssociationType::OneToMany);
        assert_eq!(a.foreign_key, "user_id");
    }

    #[test]
    fn association_with_query_method() {
        let a = QueryAssociation::new("users", "orders", AssociationType::OneToMany, "user_id")
            .with_query_method(QueryMethod::EagerLoad);
        assert_eq!(a.query_method, QueryMethod::EagerLoad);
    }

    #[test]
    fn association_is_batchable_true() {
        let a = QueryAssociation::new("users", "orders", AssociationType::OneToMany, "user_id")
            .with_query_method(QueryMethod::FindById);
        assert!(a.is_batchable());
    }

    #[test]
    fn association_is_batchable_false_one_to_one() {
        let a = QueryAssociation::new("users", "profiles", AssociationType::OneToOne, "user_id");
        assert!(!a.is_batchable());
    }

    #[test]
    fn association_is_batchable_false_non_batchable_method() {
        let a = QueryAssociation::new("users", "orders", AssociationType::OneToMany, "user_id")
            .with_query_method(QueryMethod::EagerLoad);
        assert!(!a.is_batchable());
    }

    // --- QueryAssociationAnalyzer tests ---

    #[test]
    fn analyzer_empty() {
        let a = QueryAssociationAnalyzer::new();
        assert_eq!(a.count(), 0);
        assert_eq!(a.table_count(), 0);
        assert!(a.associations().is_empty());
    }

    #[test]
    fn analyzer_register() {
        let mut a = QueryAssociationAnalyzer::new();
        a.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        assert_eq!(a.count(), 1);
        assert_eq!(a.table_count(), 2);
    }

    #[test]
    fn analyzer_children_of() {
        let mut a = QueryAssociationAnalyzer::new();
        a.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        a.register(QueryAssociation::new(
            "users",
            "profiles",
            AssociationType::OneToOne,
            "user_id",
        ));
        let children = a.children_of("users").unwrap();
        assert_eq!(children.len(), 2);
        assert!(children.contains("orders"));
        assert!(children.contains("profiles"));
    }

    #[test]
    fn analyzer_children_of_none() {
        let a = QueryAssociationAnalyzer::new();
        assert!(a.children_of("nonexistent").is_none());
    }

    #[test]
    fn analyzer_batchable_associations() {
        let mut a = QueryAssociationAnalyzer::new();
        a.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        a.register(QueryAssociation::new(
            "users",
            "profiles",
            AssociationType::OneToOne,
            "user_id",
        ));
        let batchable = a.batchable_associations();
        assert_eq!(batchable.len(), 1);
    }

    #[test]
    fn analyzer_n1_risk_associations() {
        let mut a = QueryAssociationAnalyzer::new();
        a.register(
            QueryAssociation::new("users", "orders", AssociationType::OneToMany, "user_id")
                .with_query_method(QueryMethod::EagerLoad),
        );
        a.register(
            QueryAssociation::new("users", "posts", AssociationType::OneToMany, "user_id")
                .with_query_method(QueryMethod::FindById),
        );
        let risks = a.n1_risk_associations();
        assert_eq!(risks.len(), 1);
        assert_eq!(risks[0].child_table, "orders");
    }

    #[test]
    fn analyzer_no_circular_dependency() {
        let mut a = QueryAssociationAnalyzer::new();
        a.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        a.register(QueryAssociation::new(
            "orders",
            "items",
            AssociationType::OneToMany,
            "order_id",
        ));
        assert!(!a.has_circular_dependency());
    }

    #[test]
    fn analyzer_circular_dependency() {
        let mut a = QueryAssociationAnalyzer::new();
        a.register(QueryAssociation::new(
            "a",
            "b",
            AssociationType::OneToMany,
            "a_id",
        ));
        a.register(QueryAssociation::new(
            "b",
            "a",
            AssociationType::OneToMany,
            "b_id",
        ));
        assert!(a.has_circular_dependency());
    }

    #[test]
    fn analyzer_dependency_depth() {
        let mut a = QueryAssociationAnalyzer::new();
        a.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        a.register(QueryAssociation::new(
            "orders",
            "items",
            AssociationType::OneToMany,
            "order_id",
        ));
        assert_eq!(a.dependency_depth("users"), 3);
        assert_eq!(a.dependency_depth("orders"), 2);
        assert_eq!(a.dependency_depth("items"), 1);
    }

    #[test]
    fn analyzer_dependency_depth_no_children() {
        let a = QueryAssociationAnalyzer::new();
        assert_eq!(a.dependency_depth("nonexistent"), 1);
    }

    #[test]
    fn analyzer_default() {
        let a = QueryAssociationAnalyzer::default();
        assert_eq!(a.count(), 0);
    }

    #[test]
    fn analyzer_table_count() {
        let mut a = QueryAssociationAnalyzer::new();
        a.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        a.register(QueryAssociation::new(
            "users",
            "posts",
            AssociationType::OneToMany,
            "user_id",
        ));
        assert_eq!(a.table_count(), 3);
    }

    // --- BatchLoadStrategy tests ---

    #[test]
    fn batch_strategy_as_str() {
        assert_eq!(BatchLoadStrategy::WhereIn.as_str(), "where-in");
        assert_eq!(BatchLoadStrategy::EagerLoad.as_str(), "eager-load");
        assert_eq!(BatchLoadStrategy::Join.as_str(), "join");
        assert_eq!(BatchLoadStrategy::Subquery.as_str(), "subquery");
    }

    #[test]
    fn batch_strategy_recommended_size() {
        assert!(BatchLoadStrategy::WhereIn.recommended_batch_size() > 0);
        assert!(BatchLoadStrategy::EagerLoad.recommended_batch_size() > 0);
        assert!(BatchLoadStrategy::Join.recommended_batch_size() > 0);
        assert!(BatchLoadStrategy::Subquery.recommended_batch_size() > 0);
    }

    #[test]
    fn batch_strategy_distinct() {
        assert_ne!(BatchLoadStrategy::WhereIn, BatchLoadStrategy::EagerLoad);
        assert_ne!(BatchLoadStrategy::Join, BatchLoadStrategy::Subquery);
    }

    // --- BatchLoadAdvisor tests ---

    #[test]
    fn advisor_advise_one_to_many() {
        let advisor = BatchLoadAdvisor::with_defaults();
        let a = QueryAssociation::new("users", "orders", AssociationType::OneToMany, "user_id");
        let suggestion = advisor.advise(&a).unwrap();
        assert_eq!(suggestion.parent_table, "users");
        assert_eq!(suggestion.child_table, "orders");
        assert_eq!(suggestion.strategy, BatchLoadStrategy::EagerLoad);
    }

    #[test]
    fn advisor_advise_many_to_one() {
        let advisor = BatchLoadAdvisor::with_defaults();
        let a = QueryAssociation::new("orders", "users", AssociationType::ManyToOne, "user_id");
        let suggestion = advisor.advise(&a).unwrap();
        assert_eq!(suggestion.strategy, BatchLoadStrategy::WhereIn);
    }

    #[test]
    fn advisor_advise_many_to_many() {
        let advisor = BatchLoadAdvisor::with_defaults();
        let a = QueryAssociation::new("users", "roles", AssociationType::ManyToMany, "user_id");
        let suggestion = advisor.advise(&a).unwrap();
        assert_eq!(suggestion.strategy, BatchLoadStrategy::Join);
    }

    #[test]
    fn advisor_advise_one_to_one_returns_none() {
        let advisor = BatchLoadAdvisor::with_defaults();
        let a = QueryAssociation::new("users", "profiles", AssociationType::OneToOne, "user_id");
        assert!(advisor.advise(&a).is_none());
    }

    #[test]
    fn advisor_advise_batch() {
        let advisor = BatchLoadAdvisor::with_defaults();
        let associations = vec![
            QueryAssociation::new("users", "orders", AssociationType::OneToMany, "user_id"),
            QueryAssociation::new("orders", "users", AssociationType::ManyToOne, "user_id"),
        ];
        let suggestions = advisor.advise_batch(&associations);
        assert_eq!(suggestions.len(), 2);
    }

    #[test]
    fn advisor_default_batch_size() {
        let advisor = BatchLoadAdvisor::new(50);
        assert_eq!(advisor.default_batch_size(), 50);
    }

    #[test]
    fn advisor_default() {
        let advisor = BatchLoadAdvisor::default();
        assert_eq!(advisor.default_batch_size(), 100);
    }

    #[test]
    fn advisor_batch_size_clamped() {
        let advisor = BatchLoadAdvisor::new(0);
        assert_eq!(advisor.default_batch_size(), 1);
    }

    #[test]
    fn advisor_suggestion_contains_reason() {
        let advisor = BatchLoadAdvisor::with_defaults();
        let a = QueryAssociation::new("users", "orders", AssociationType::OneToMany, "user_id");
        let suggestion = advisor.advise(&a).unwrap();
        assert!(!suggestion.reason.is_empty());
        assert!(suggestion.reason.contains("eager-load"));
    }

    // --- EagerLoadPlan tests ---

    #[test]
    fn eager_load_plan_new() {
        let plan = EagerLoadPlan::new("users");
        assert_eq!(plan.root_table, "users");
        assert_eq!(plan.path_count(), 0);
    }

    #[test]
    fn eager_load_plan_add_path() {
        let mut plan = EagerLoadPlan::new("users");
        plan.add_path(LoadPath::new(
            vec!["users".to_string(), "orders".to_string()],
            BatchLoadStrategy::EagerLoad,
        ));
        assert_eq!(plan.path_count(), 1);
    }

    #[test]
    fn eager_load_plan_max_depth() {
        let mut plan = EagerLoadPlan::new("users");
        plan.add_path(LoadPath::new(
            vec!["users".to_string(), "orders".to_string()],
            BatchLoadStrategy::EagerLoad,
        ));
        plan.add_path(LoadPath::new(
            vec![
                "users".to_string(),
                "orders".to_string(),
                "items".to_string(),
            ],
            BatchLoadStrategy::EagerLoad,
        ));
        assert_eq!(plan.max_depth(), 3);
    }

    #[test]
    fn eager_load_plan_all_tables() {
        let mut plan = EagerLoadPlan::new("users");
        plan.add_path(LoadPath::new(
            vec!["users".to_string(), "orders".to_string()],
            BatchLoadStrategy::EagerLoad,
        ));
        plan.add_path(LoadPath::new(
            vec!["users".to_string(), "posts".to_string()],
            BatchLoadStrategy::EagerLoad,
        ));
        let tables = plan.all_tables();
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
        assert!(tables.contains(&"posts".to_string()));
    }

    #[test]
    fn eager_load_plan_empty_max_depth() {
        let plan = EagerLoadPlan::new("users");
        assert_eq!(plan.max_depth(), 0);
    }

    // --- LoadPath tests ---

    #[test]
    fn load_path_new() {
        let path = LoadPath::new(
            vec!["users".to_string(), "orders".to_string()],
            BatchLoadStrategy::EagerLoad,
        );
        assert_eq!(path.depth(), 2);
        assert_eq!(
            path.batch_size,
            BatchLoadStrategy::EagerLoad.recommended_batch_size()
        );
    }

    #[test]
    fn load_path_as_path_string() {
        let path = LoadPath::new(
            vec![
                "users".to_string(),
                "orders".to_string(),
                "items".to_string(),
            ],
            BatchLoadStrategy::EagerLoad,
        );
        assert_eq!(path.as_path_string(), "users.orders.items");
    }

    #[test]
    fn load_path_depth() {
        let path = LoadPath::new(
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string(),
            ],
            BatchLoadStrategy::Join,
        );
        assert_eq!(path.depth(), 4);
    }

    // --- EagerLoadPlanner tests ---

    #[test]
    fn planner_empty_analyzer() {
        let analyzer = QueryAssociationAnalyzer::new();
        let planner = EagerLoadPlanner::with_defaults();
        let plan = planner.plan("users", &analyzer);
        assert_eq!(plan.root_table, "users");
        assert_eq!(plan.path_count(), 0);
    }

    #[test]
    fn planner_single_level() {
        let mut analyzer = QueryAssociationAnalyzer::new();
        analyzer.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        let planner = EagerLoadPlanner::with_defaults();
        let plan = planner.plan("users", &analyzer);
        assert!(plan.path_count() >= 1);
    }

    #[test]
    fn planner_multi_level() {
        let mut analyzer = QueryAssociationAnalyzer::new();
        analyzer.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        analyzer.register(QueryAssociation::new(
            "orders",
            "items",
            AssociationType::OneToMany,
            "order_id",
        ));
        let planner = EagerLoadPlanner::with_defaults();
        let plan = planner.plan("users", &analyzer);
        assert!(plan.path_count() >= 1);
        assert!(plan.max_depth() >= 2);
    }

    #[test]
    fn planner_max_depth_limit() {
        let mut analyzer = QueryAssociationAnalyzer::new();
        analyzer.register(QueryAssociation::new(
            "a",
            "b",
            AssociationType::OneToMany,
            "a_id",
        ));
        analyzer.register(QueryAssociation::new(
            "b",
            "c",
            AssociationType::OneToMany,
            "b_id",
        ));
        analyzer.register(QueryAssociation::new(
            "c",
            "d",
            AssociationType::OneToMany,
            "c_id",
        ));
        let planner = EagerLoadPlanner::new(2);
        let plan = planner.plan("a", &analyzer);
        assert!(plan.max_depth() <= 3);
    }

    #[test]
    fn planner_max_depth_getter() {
        let planner = EagerLoadPlanner::new(5);
        assert_eq!(planner.max_depth(), 5);
    }

    #[test]
    fn planner_default() {
        let planner = EagerLoadPlanner::default();
        assert_eq!(planner.max_depth(), 3);
    }

    #[test]
    fn planner_selects_correct_strategy() {
        let mut analyzer = QueryAssociationAnalyzer::new();
        analyzer.register(QueryAssociation::new(
            "users",
            "orders",
            AssociationType::OneToMany,
            "user_id",
        ));
        analyzer.register(QueryAssociation::new(
            "orders",
            "users",
            AssociationType::ManyToOne,
            "user_id",
        ));
        let planner = EagerLoadPlanner::with_defaults();
        let plan = planner.plan("users", &analyzer);
        assert!(plan.path_count() >= 1);
    }
}
