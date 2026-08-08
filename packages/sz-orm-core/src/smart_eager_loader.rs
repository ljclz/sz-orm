//! SmartEagerLoader — 智能策略选择 Eager Loading（v2.3.0 任务 C）
//!
//! 基于 `RelationKind` 自动选择最优加载策略，追平 SeaORM Smart EntityLoader：
//! - **HasOne / BelongsTo** → `JoinStrategy`（单次 JOIN 查询）
//! - **HasMany** → `DataLoaderStrategy`（批量 IN 查询，2 次）
//! - **ManyToMany（有中间表）** → `IntermediateTableStrategy`（中间表批量查询，2 次）
//! - **ManyToMany（无中间表）** → 回退 `DataLoaderStrategy` + 告警
//!
//! # 设计
//!
//! - `StrategyResolver` 为纯内存枚举匹配，无 IO，决策延迟 ≤ 100μs
//! - 向后兼容：`EagerLoader::smart()` 扩展方法返回 `SmartEagerLoader`，不修改原 API
//! - N+1 自动消除集成在 `N1Eliminator`（`n1_eliminator.rs`）
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::eager_loader::EagerLoader;
//!
//! let loader = EagerLoader::new(order_relation)
//!     .with(item_relation)
//!     .smart();
//! let tree = loader.load(&mut conn, "SELECT id, name FROM users").await?;
//! ```

use crate::cycle_detection::{CycleDetector, CyclePolicy};
use crate::eager_loader::{EagerResult, NestedEagerResult};
use crate::pool::Connection;
use crate::relation_trait::{RelationDef, RelationKind};
use crate::value::Value;
use crate::DbError;

use std::collections::HashMap;

/// 智能加载策略（v2.3.0 新增）
///
/// 由 [`StrategyResolver`] 基于 [`RelationKind`] 自动决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadStrategy {
    /// JOIN 策略：单次 JOIN 查询（HasOne / BelongsTo）
    Join,
    /// Data Loader 策略：批量 IN 查询（HasMany）
    DataLoader,
    /// 中间表批量策略：经中间表 JOIN 关联表批量查询（ManyToMany 有中间表）
    IntermediateTableBatch,
}

impl LoadStrategy {
    /// 返回该策略的预估查询次数
    pub fn estimated_query_count(self) -> usize {
        match self {
            LoadStrategy::Join => 1,
            LoadStrategy::DataLoader => 2,
            LoadStrategy::IntermediateTableBatch => 2,
        }
    }
}

/// 策略决策记录（v2.3.0 新增）
///
/// 记录单级关联的策略决策结果，供调试、审计与日志输出。
#[derive(Debug, Clone)]
pub struct StrategyDecision {
    /// 关联名称
    pub relation_name: String,
    /// 关联类型
    pub relation_kind: RelationKind,
    /// 选定的加载策略
    pub strategy: LoadStrategy,
    /// 决策原因（人类可读）
    pub reason: String,
    /// 预估查询次数
    pub estimated_query_count: usize,
}

/// 策略决策器（v2.3.0 新增）
///
/// 纯规则匹配器，基于 [`RelationDef`] 的 `kind` 与中间表配置决策加载策略。
/// 无 IO、无状态，相同输入始终返回相同输出（确定性保证）。
#[derive(Debug, Clone, Default)]
pub struct StrategyResolver;

impl StrategyResolver {
    /// 创建策略决策器
    pub fn new() -> Self {
        Self
    }

    /// 对单个关联决策加载策略
    ///
    /// 决策规则矩阵（design.md §3.1.2）：
    /// - `HasOne` / `BelongsTo` → `Join`（1 次查询）
    /// - `HasMany` → `DataLoader`（2 次查询）
    /// - `ManyToMany` 有中间表 → `IntermediateTableBatch`（2 次查询）
    /// - `ManyToMany` 无中间表 → 回退 `DataLoader` + `tracing::warn!` 告警
    pub fn resolve(&self, relation: &RelationDef) -> StrategyDecision {
        let strategy = match relation.kind {
            RelationKind::HasOne | RelationKind::BelongsTo => LoadStrategy::Join,
            RelationKind::HasMany => LoadStrategy::DataLoader,
            RelationKind::ManyToMany => {
                if relation.join_table.is_some()
                    && relation.join_from_key.is_some()
                    && relation.join_to_key.is_some()
                {
                    LoadStrategy::IntermediateTableBatch
                } else {
                    tracing::warn!(
                        relation = relation.name,
                        "ManyToMany 关联缺少中间表配置，回退至 DataLoader 策略"
                    );
                    LoadStrategy::DataLoader
                }
            }
        };

        let reason = match strategy {
            LoadStrategy::Join => format!("{:?} 关联自动选择 JOIN 策略（单次查询）", relation.kind),
            LoadStrategy::DataLoader => {
                if relation.kind == RelationKind::ManyToMany {
                    "ManyToMany 缺少中间表配置，回退至 DataLoader 策略（批量 IN 查询）".to_string()
                } else {
                    "HasMany 自动选择 Data Loader 策略（批量 IN 查询）".to_string()
                }
            }
            LoadStrategy::IntermediateTableBatch => {
                "ManyToMany 自动选择中间表批量策略（经中间表 JOIN 关联表）".to_string()
            }
        };

        StrategyDecision {
            relation_name: relation.name.to_string(),
            relation_kind: relation.kind,
            strategy,
            reason,
            estimated_query_count: strategy.estimated_query_count(),
        }
    }

    /// 对多级关联链逐级独立决策（REQ-C-027）
    ///
    /// 每级关联独立决策，允许不同级使用不同策略。
    pub fn resolve_chain(&self, relations: &[RelationDef]) -> Vec<StrategyDecision> {
        relations.iter().map(|r| self.resolve(r)).collect()
    }
}

/// 将 Value 转换为字符串键（复用 eager_loader 内部逻辑）
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

/// JOIN 策略执行器（HasOne / BelongsTo 自动 JOIN，v2.3.0 新增）
///
/// 生成含 INNER JOIN 的 SQL，执行单次查询，拆分扁平行为 (主表行, 关联行)。
#[derive(Debug, Clone)]
pub struct JoinStrategy;

impl JoinStrategy {
    /// 创建 JoinStrategy
    pub fn new() -> Self {
        Self
    }

    /// 生成 JOIN SQL（显式列名 + 前缀，禁止 SELECT *）
    ///
    /// # 参数
    ///
    /// - `relation`：关联关系定义
    /// - `main_columns`：主表显式列名列表
    /// - `related_columns`：关联表显式列名列表
    /// - `where_clause`：WHERE 条件 SQL 片段（已参数化，不含 WHERE 关键字）
    pub fn build_join_sql(
        &self,
        relation: &RelationDef,
        main_columns: &[&str],
        related_columns: &[&str],
        where_clause: &str,
    ) -> String {
        let main_select: Vec<String> = main_columns
            .iter()
            .map(|c| format!("main.{} AS main_{}", c, c))
            .collect();
        let related_select: Vec<String> = related_columns
            .iter()
            .map(|c| format!("related.{} AS related_{}", c, c))
            .collect();

        let join_type = relation.kind.default_join_type().as_sql();

        let where_part = if where_clause.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clause)
        };

        format!(
            "SELECT {} FROM {} AS main {} {} AS related ON main.{} = related.{}{}",
            main_select.join(", ") + ", " + &related_select.join(", "),
            relation.from_entity,
            join_type,
            relation.to_entity,
            relation.from_key,
            relation.to_key,
            where_part
        )
    }

    /// 拆分 JOIN 扁平行为 (主表行, 关联行)
    ///
    /// 按列名前缀 `main_` / `related_` 拆分。
    /// 关联表列全 Null 时返回 `None`（LEFT JOIN 无匹配行）。
    pub fn split_join_row(
        flat_row: &HashMap<String, Value>,
        main_columns: &[&str],
        related_columns: &[&str],
    ) -> (HashMap<String, Value>, Option<HashMap<String, Value>>) {
        let mut main_row = HashMap::new();
        for col in main_columns {
            let key = format!("main_{}", col);
            if let Some(v) = flat_row.get(&key) {
                main_row.insert(col.to_string(), v.clone());
            }
        }

        let mut related_row = HashMap::new();
        let mut all_null = true;
        for col in related_columns {
            let key = format!("related_{}", col);
            if let Some(v) = flat_row.get(&key) {
                if !matches!(v, Value::Null) {
                    all_null = false;
                }
                related_row.insert(col.to_string(), v.clone());
            }
        }

        let related = if all_null { None } else { Some(related_row) };
        (main_row, related)
    }

    /// 执行 JOIN 策略
    ///
    /// 生成 JOIN SQL → 执行单次查询 → 拆分扁平行为 (主表行, Option<关联行>)。
    pub async fn execute(
        &self,
        conn: &mut dyn Connection,
        relation: &RelationDef,
        main_columns: &[&str],
        related_columns: &[&str],
        where_clause: &str,
        where_params: &[Value],
    ) -> Result<Vec<(HashMap<String, Value>, Option<HashMap<String, Value>>)>, DbError> {
        let sql = self.build_join_sql(relation, main_columns, related_columns, where_clause);
        let rows = conn.query_with_params(&sql, where_params).await?;

        let results = rows
            .iter()
            .map(|flat| Self::split_join_row(flat, main_columns, related_columns))
            .collect();

        Ok(results)
    }
}

impl Default for JoinStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// Data Loader 策略执行器（HasMany 自动 data loader，v2.3.0 新增）
///
/// 执行主表查询 → 提取主键 → `WHERE fk IN (?,...)` 批量查询 → 按外键分组组装。
#[derive(Debug, Clone)]
pub struct DataLoaderStrategy;

impl DataLoaderStrategy {
    /// 创建 DataLoaderStrategy
    pub fn new() -> Self {
        Self
    }

    /// 按外键值分组关联行
    ///
    /// 无遗漏无错配：每行按外键值归入对应分组。
    pub fn group_by_foreign_key(
        rows: Vec<HashMap<String, Value>>,
        fk_key: &str,
    ) -> HashMap<String, Vec<HashMap<String, Value>>> {
        let mut grouped: HashMap<String, Vec<HashMap<String, Value>>> = HashMap::new();
        for row in rows {
            let fk = row.get(fk_key).cloned().unwrap_or(Value::Null);
            let key = value_to_key(&fk);
            grouped.entry(key).or_default().push(row);
        }
        grouped
    }

    /// 生成批量 IN 查询 SQL（显式列名，禁止 SELECT *）
    pub fn build_batch_sql(
        &self,
        relation: &RelationDef,
        related_columns: &[&str],
        param_count: usize,
    ) -> String {
        let select_cols: Vec<String> = related_columns.iter().map(|c| c.to_string()).collect();
        let placeholders: Vec<String> = (0..param_count).map(|_| "?".to_string()).collect();
        format!(
            "SELECT {} FROM {} WHERE {} IN ({})",
            select_cols.join(", "),
            relation.to_entity,
            relation.to_key,
            placeholders.join(", ")
        )
    }

    /// 执行 Data Loader 策略
    ///
    /// 1. 执行主表 SQL → 提取主键列表
    /// 2. 空结果跳过（REQ-C-013）
    /// 3. 生成 `WHERE fk IN (?,...)` 批量查询（参数化）
    /// 4. Oracle 方言且主键数 >1000 时分批
    /// 5. 按外键分组组装
    pub async fn execute(
        &self,
        conn: &mut dyn Connection,
        relation: &RelationDef,
        main_sql: &str,
        related_columns: &[&str],
    ) -> Result<Vec<EagerResult>, DbError> {
        let main_rows = conn.query(main_sql).await?;

        if main_rows.is_empty() {
            return Ok(Vec::new());
        }

        let pk_values: Vec<Value> = main_rows
            .iter()
            .filter_map(|row| row.get(relation.from_key).cloned())
            .collect();

        if pk_values.is_empty() {
            return Ok(main_rows.into_iter().map(|r| (r, Vec::new())).collect());
        }

        let mut all_related_rows = Vec::new();
        for chunk in pk_values.chunks(1000) {
            let sql = self.build_batch_sql(relation, related_columns, chunk.len());
            let rows = conn.query_with_params(&sql, chunk).await?;
            all_related_rows.extend(rows);
        }

        let grouped = Self::group_by_foreign_key(all_related_rows, relation.to_key);

        let results = main_rows
            .into_iter()
            .map(|row| {
                let pk = row.get(relation.from_key).cloned().unwrap_or(Value::Null);
                let pk_key = value_to_key(&pk);
                let related = grouped.get(&pk_key).cloned().unwrap_or_default();
                (row, related)
            })
            .collect();

        Ok(results)
    }
}

impl Default for DataLoaderStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// 中间表批量策略执行器（ManyToMany 自动中间表，v2.3.0 新增）
///
/// 经中间表 JOIN 关联表批量查询，按主键分组组装为 (主实体, Vec<关联实体>)。
#[derive(Debug, Clone)]
pub struct IntermediateTableStrategy;

impl IntermediateTableStrategy {
    /// 创建 IntermediateTableStrategy
    pub fn new() -> Self {
        Self
    }

    /// 生成中间表批量查询 SQL（显式列名，禁止 SELECT *）
    pub fn build_intermediate_sql(
        &self,
        relation: &RelationDef,
        related_columns: &[&str],
        param_count: usize,
    ) -> Result<String, DbError> {
        let join_table = relation.join_table.ok_or_else(|| {
            DbError::InvalidInput(format!(
                "ManyToMany 关联 {} 缺少中间表配置（join_table）",
                relation.name
            ))
        })?;
        let join_from_key = relation.join_from_key.ok_or_else(|| {
            DbError::InvalidInput(format!(
                "ManyToMany 关联 {} 缺少中间表配置（join_from_key）",
                relation.name
            ))
        })?;
        let join_to_key = relation.join_to_key.ok_or_else(|| {
            DbError::InvalidInput(format!(
                "ManyToMany 关联 {} 缺少中间表配置（join_to_key）",
                relation.name
            ))
        })?;

        let select_cols: Vec<String> = related_columns
            .iter()
            .map(|c| format!("related.{}", c))
            .collect();
        let placeholders: Vec<String> = (0..param_count).map(|_| "?".to_string()).collect();

        Ok(format!(
            "SELECT {}, jt.{} AS __join_from_key FROM {} AS jt JOIN {} AS related ON jt.{} = related.{} WHERE jt.{} IN ({})",
            select_cols.join(", "),
            join_from_key,
            join_table,
            relation.to_entity,
            join_to_key,
            relation.from_key,
            join_from_key,
            placeholders.join(", ")
        ))
    }

    /// 执行中间表批量策略
    ///
    /// 1. 校验中间表配置
    /// 2. 查主表主键
    /// 3. 经中间表 JOIN 关联表批量查询
    /// 4. 按主键分组组装为 (主实体, Vec<关联实体>)
    pub async fn execute(
        &self,
        conn: &mut dyn Connection,
        relation: &RelationDef,
        main_sql: &str,
        related_columns: &[&str],
    ) -> Result<Vec<EagerResult>, DbError> {
        let main_rows = conn.query(main_sql).await?;

        if main_rows.is_empty() {
            return Ok(Vec::new());
        }

        let pk_values: Vec<Value> = main_rows
            .iter()
            .filter_map(|row| row.get(relation.from_key).cloned())
            .collect();

        if pk_values.is_empty() {
            return Ok(main_rows.into_iter().map(|r| (r, Vec::new())).collect());
        }

        let mut all_related_rows = Vec::new();
        let mut all_join_keys = Vec::new();
        for chunk in pk_values.chunks(1000) {
            let sql = self.build_intermediate_sql(relation, related_columns, chunk.len())?;
            let rows = conn.query_with_params(&sql, chunk).await?;
            for row in &rows {
                if let Some(v) = row.get("__join_from_key") {
                    all_join_keys.push(value_to_key(v));
                }
            }
            all_related_rows.extend(rows);
        }

        let mut grouped: HashMap<String, Vec<HashMap<String, Value>>> = HashMap::new();
        for (i, row) in all_related_rows.into_iter().enumerate() {
            if i < all_join_keys.len() {
                grouped
                    .entry(all_join_keys[i].clone())
                    .or_default()
                    .push(row);
            }
        }

        let results = main_rows
            .into_iter()
            .map(|row| {
                let pk = row.get(relation.from_key).cloned().unwrap_or(Value::Null);
                let pk_key = value_to_key(&pk);
                let related = grouped.get(&pk_key).cloned().unwrap_or_default();
                (row, related)
            })
            .collect();

        Ok(results)
    }
}

impl Default for IntermediateTableStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// 子级智能加载配置（递归结构，复用 eager_loader 的 ChildLoadConfig 模式）
#[derive(Debug, Clone)]
struct SmartChildConfig {
    relation: RelationDef,
    children: Vec<SmartChildConfig>,
}

impl SmartChildConfig {
    fn push_to_deepest(&mut self, child: SmartChildConfig) {
        if self.children.is_empty() {
            self.children.push(child);
        } else {
            self.children.last_mut().unwrap().push_to_deepest(child);
        }
    }

    fn chain_depth(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            1 + self.children[0].chain_depth()
        }
    }
}

/// 智能策略选择 Eager Loading 执行器（v2.3.0 新增）
///
/// 基于 `RelationKind` 自动选择最优加载策略：
/// - HasOne / BelongsTo → JOIN（单次查询）
/// - HasMany → Data Loader（批量 IN 查询）
/// - ManyToMany → 中间表批量查询
///
/// # 向后兼容
///
/// `EagerLoader::smart()` 返回 `SmartEagerLoader`，原 `EagerLoader` API 不变。
///
/// ```ignore
/// use sz_orm_core::eager_loader::EagerLoader;
///
/// let loader = EagerLoader::new(order_rel)
///     .with(item_rel)
///     .smart();
/// let tree = loader.load(&mut conn, "SELECT id, name FROM users").await?;
/// ```
#[derive(Debug, Clone)]
pub struct SmartEagerLoader {
    relation: RelationDef,
    children: Vec<SmartChildConfig>,
    cycle_policy: CyclePolicy,
    n1_threshold: usize,
    decisions: Vec<StrategyDecision>,
}

impl SmartEagerLoader {
    /// 创建 SmartEagerLoader
    pub fn new(relation: RelationDef) -> Self {
        Self {
            relation,
            children: Vec::new(),
            cycle_policy: CyclePolicy::default(),
            n1_threshold: 5,
            decisions: Vec::new(),
        }
    }

    /// 添加子级关联（链式，无限级）
    pub fn with(mut self, relation: RelationDef) -> Self {
        let new_child = SmartChildConfig {
            relation,
            children: Vec::new(),
        };
        if self.children.is_empty() {
            self.children.push(new_child);
        } else {
            self.children.last_mut().unwrap().push_to_deepest(new_child);
        }
        self
    }

    /// 设置循环检测策略
    pub fn with_cycle_policy(mut self, policy: CyclePolicy) -> Self {
        self.cycle_policy = policy;
        self
    }

    /// 设置 N+1 消除阈值（默认 5）
    pub fn with_n1_threshold(mut self, threshold: usize) -> Self {
        self.n1_threshold = threshold;
        self
    }

    /// 返回策略决策记录（供调试与审计）
    pub fn decisions(&self) -> &[StrategyDecision] {
        &self.decisions
    }

    /// 返回子级关联链深度
    pub fn children_count(&self) -> usize {
        if self.children.is_empty() {
            0
        } else {
            self.children[0].chain_depth()
        }
    }

    /// 执行智能 Eager Loading，返回嵌套结果树
    ///
    /// 按总流程（design.md §3.8）：
    /// 1. 初始化 CycleDetector
    /// 2. 执行主表查询
    /// 3. 逐级智能加载：策略决策 → 分发到策略执行器 → 递归子级
    /// 4. 返回 NestedEagerResult 嵌套树
    pub async fn load(
        mut self,
        conn: &mut dyn Connection,
        main_sql: &str,
    ) -> Result<Vec<NestedEagerResult>, DbError> {
        let mut detector = CycleDetector::new(self.cycle_policy);
        let main_rows = conn.query(main_sql).await?;

        if main_rows.is_empty() {
            return Ok(Vec::new());
        }

        const MAX_RESULT_SIZE: usize = 1_000_000;
        if main_rows.len() > MAX_RESULT_SIZE {
            return Err(DbError::InvalidInput(format!(
                "结果集超内存限制（{} 行），建议改用 Stream API 处理大结果集",
                main_rows.len()
            )));
        }

        let root_relation = self.relation.clone();
        let root_children = std::mem::take(&mut self.children);
        self.load_level_smart(
            conn,
            main_rows,
            &root_relation,
            &root_children,
            &mut detector,
        )
        .await
    }

    /// 递归加载单级关联（智能策略选择）
    async fn load_level_smart(
        &mut self,
        conn: &mut dyn Connection,
        parent_rows: Vec<HashMap<String, Value>>,
        relation: &RelationDef,
        child_configs: &[SmartChildConfig],
        detector: &mut CycleDetector,
    ) -> Result<Vec<NestedEagerResult>, DbError> {
        let can_continue = detector.check(relation.from_entity, relation.name)?;
        if !can_continue {
            return Ok(parent_rows
                .into_iter()
                .map(NestedEagerResult::Leaf)
                .collect());
        }

        detector.enter(relation.from_entity, relation.name);

        let resolver = StrategyResolver::new();
        let decision = resolver.resolve(relation);
        tracing::info!(
            relation = decision.relation_name,
            kind = ?decision.relation_kind,
            strategy = ?decision.strategy,
            reason = decision.reason,
            "策略决策"
        );
        self.decisions.push(decision.clone());

        let pk_values: Vec<Value> = parent_rows
            .iter()
            .filter_map(|row| row.get(relation.from_key).cloned())
            .collect();

        if pk_values.is_empty() {
            detector.leave();
            return Ok(parent_rows
                .into_iter()
                .map(|row| NestedEagerResult::Node {
                    row,
                    children: Vec::new(),
                })
                .collect());
        }

        let related_rows = match decision.strategy {
            LoadStrategy::Join => {
                self.execute_join_strategy(conn, relation, &pk_values)
                    .await?
            }
            LoadStrategy::DataLoader => {
                self.execute_data_loader_strategy(conn, relation, &pk_values)
                    .await?
            }
            LoadStrategy::IntermediateTableBatch => {
                self.execute_intermediate_strategy(conn, relation, &pk_values)
                    .await?
            }
        };

        let grouped = DataLoaderStrategy::group_by_foreign_key(related_rows, relation.to_key);

        let mut results = Vec::with_capacity(parent_rows.len());
        for parent_row in parent_rows {
            let pk = parent_row
                .get(relation.from_key)
                .cloned()
                .unwrap_or(Value::Null);
            let pk_key = value_to_key(&pk);
            let child_rows = grouped.get(&pk_key).cloned().unwrap_or_default();

            let children = if child_rows.is_empty() {
                Vec::new()
            } else if child_configs.is_empty() {
                child_rows
                    .into_iter()
                    .map(NestedEagerResult::Leaf)
                    .collect()
            } else {
                let next_config = &child_configs[0];
                Box::pin(self.load_level_smart(
                    conn,
                    child_rows,
                    &next_config.relation,
                    &next_config.children,
                    detector,
                ))
                .await?
            };

            results.push(NestedEagerResult::Node {
                row: parent_row,
                children,
            });
        }

        detector.leave();
        Ok(results)
    }

    /// 执行 JOIN 策略（HasOne / BelongsTo）
    async fn execute_join_strategy(
        &self,
        conn: &mut dyn Connection,
        relation: &RelationDef,
        pk_values: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let placeholders: Vec<String> = (0..pk_values.len()).map(|_| "?".to_string()).collect();
        let where_clause = format!(
            "main.{} IN ({})",
            relation.from_key,
            placeholders.join(", ")
        );
        let sql = format!(
            "SELECT related.* FROM {} AS main INNER JOIN {} AS related ON main.{} = related.{} WHERE {}",
            relation.from_entity,
            relation.to_entity,
            relation.from_key,
            relation.to_key,
            where_clause
        );
        let rows = conn.query_with_params(&sql, pk_values).await?;
        Ok(rows)
    }

    /// 执行 Data Loader 策略（HasMany）
    async fn execute_data_loader_strategy(
        &self,
        conn: &mut dyn Connection,
        relation: &RelationDef,
        pk_values: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let mut all_rows = Vec::new();
        for chunk in pk_values.chunks(1000) {
            let placeholders: Vec<String> = (0..chunk.len()).map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT * FROM {} WHERE {} IN ({})",
                relation.to_entity,
                relation.to_key,
                placeholders.join(", ")
            );
            let rows = conn.query_with_params(&sql, chunk).await?;
            all_rows.extend(rows);
        }
        Ok(all_rows)
    }

    /// 执行中间表批量策略（ManyToMany）
    async fn execute_intermediate_strategy(
        &self,
        conn: &mut dyn Connection,
        relation: &RelationDef,
        pk_values: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let join_table = relation.join_table.ok_or_else(|| {
            DbError::InvalidInput(format!(
                "ManyToMany 关联 {} 缺少中间表配置（join_table）",
                relation.name
            ))
        })?;
        let join_from_key = relation.join_from_key.ok_or_else(|| {
            DbError::InvalidInput(format!(
                "ManyToMany 关联 {} 缺少中间表配置（join_from_key）",
                relation.name
            ))
        })?;
        let join_to_key = relation.join_to_key.ok_or_else(|| {
            DbError::InvalidInput(format!(
                "ManyToMany 关联 {} 缺少中间表配置（join_to_key）",
                relation.name
            ))
        })?;

        let mut all_rows = Vec::new();
        for chunk in pk_values.chunks(1000) {
            let placeholders: Vec<String> = (0..chunk.len()).map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT related.*, jt.{} AS __join_from_key FROM {} AS jt JOIN {} AS related ON jt.{} = related.{} WHERE jt.{} IN ({})",
                join_from_key,
                join_table,
                relation.to_entity,
                join_to_key,
                relation.from_key,
                join_from_key,
                placeholders.join(", ")
            );
            let rows = conn.query_with_params(&sql, chunk).await?;
            all_rows.extend(rows);
        }
        Ok(all_rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_strategy_estimated_query_count() {
        assert_eq!(LoadStrategy::Join.estimated_query_count(), 1);
        assert_eq!(LoadStrategy::DataLoader.estimated_query_count(), 2);
        assert_eq!(
            LoadStrategy::IntermediateTableBatch.estimated_query_count(),
            2
        );
    }

    #[test]
    fn test_strategy_resolver_hasone() {
        let resolver = StrategyResolver::new();
        let rel = RelationDef::new(
            "profile",
            "users",
            "profiles",
            "id",
            "user_id",
            RelationKind::HasOne,
        );
        let decision = resolver.resolve(&rel);
        assert_eq!(decision.strategy, LoadStrategy::Join);
        assert_eq!(decision.estimated_query_count, 1);
        assert_eq!(decision.relation_name, "profile");
    }

    #[test]
    fn test_strategy_resolver_belongsto() {
        let resolver = StrategyResolver::new();
        let rel = RelationDef::new(
            "owner",
            "orders",
            "users",
            "user_id",
            "id",
            RelationKind::BelongsTo,
        );
        let decision = resolver.resolve(&rel);
        assert_eq!(decision.strategy, LoadStrategy::Join);
        assert_eq!(decision.estimated_query_count, 1);
    }

    #[test]
    fn test_strategy_resolver_hasmany() {
        let resolver = StrategyResolver::new();
        let rel = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let decision = resolver.resolve(&rel);
        assert_eq!(decision.strategy, LoadStrategy::DataLoader);
        assert_eq!(decision.estimated_query_count, 2);
    }

    #[test]
    fn test_strategy_resolver_manytomany_with_join_table() {
        let resolver = StrategyResolver::new();
        let rel = RelationDef::new_many_to_many(
            "roles",
            "users",
            "roles",
            "id",
            "id",
            "user_roles",
            "user_id",
            "role_id",
        );
        let decision = resolver.resolve(&rel);
        assert_eq!(decision.strategy, LoadStrategy::IntermediateTableBatch);
        assert_eq!(decision.estimated_query_count, 2);
    }

    #[test]
    fn test_strategy_resolver_manytomany_without_join_table() {
        let resolver = StrategyResolver::new();
        let rel = RelationDef::new(
            "roles",
            "users",
            "roles",
            "id",
            "role_id",
            RelationKind::ManyToMany,
        );
        let decision = resolver.resolve(&rel);
        assert_eq!(decision.strategy, LoadStrategy::DataLoader);
    }

    #[test]
    fn test_strategy_resolver_resolve_chain() {
        let resolver = StrategyResolver::new();
        let rels = vec![
            RelationDef::new(
                "orders",
                "users",
                "orders",
                "id",
                "user_id",
                RelationKind::HasMany,
            ),
            RelationDef::new(
                "items",
                "orders",
                "items",
                "id",
                "order_id",
                RelationKind::HasMany,
            ),
            RelationDef::new(
                "product",
                "items",
                "products",
                "product_id",
                "id",
                RelationKind::BelongsTo,
            ),
        ];
        let decisions = resolver.resolve_chain(&rels);
        assert_eq!(decisions.len(), 3);
        assert_eq!(decisions[0].strategy, LoadStrategy::DataLoader);
        assert_eq!(decisions[1].strategy, LoadStrategy::DataLoader);
        assert_eq!(decisions[2].strategy, LoadStrategy::Join);
    }

    #[test]
    fn test_join_strategy_build_join_sql() {
        let strategy = JoinStrategy::new();
        let rel = RelationDef::new(
            "profile",
            "users",
            "profiles",
            "id",
            "user_id",
            RelationKind::HasOne,
        );
        let sql = strategy.build_join_sql(&rel, &["id", "name"], &["bio"], "main.id > ?");
        assert!(sql.contains("INNER JOIN"));
        assert!(sql.contains("main.id AS main_id"));
        assert!(sql.contains("related.bio AS related_bio"));
        assert!(sql.contains("WHERE main.id > ?"));
        assert!(!sql.contains("SELECT *"));
    }

    #[test]
    fn test_join_strategy_split_join_row() {
        let mut flat = HashMap::new();
        flat.insert("main_id".to_string(), Value::I64(1));
        flat.insert("main_name".to_string(), Value::String("Alice".to_string()));
        flat.insert(
            "related_bio".to_string(),
            Value::String("Hello".to_string()),
        );

        let (main, related) = JoinStrategy::split_join_row(&flat, &["id", "name"], &["bio"]);
        assert_eq!(main.get("id"), Some(&Value::I64(1)));
        assert_eq!(main.get("name"), Some(&Value::String("Alice".to_string())));
        assert!(related.is_some());
        assert_eq!(
            related.unwrap().get("bio"),
            Some(&Value::String("Hello".to_string()))
        );
    }

    #[test]
    fn test_join_strategy_split_join_row_all_null() {
        let mut flat = HashMap::new();
        flat.insert("main_id".to_string(), Value::I64(1));
        flat.insert("related_bio".to_string(), Value::Null);

        let (main, related) = JoinStrategy::split_join_row(&flat, &["id"], &["bio"]);
        assert_eq!(main.get("id"), Some(&Value::I64(1)));
        assert!(related.is_none());
    }

    #[test]
    fn test_data_loader_strategy_group_by_foreign_key() {
        let mut row1 = HashMap::new();
        row1.insert("user_id".to_string(), Value::I64(1));
        row1.insert("name".to_string(), Value::String("Order1".to_string()));

        let mut row2 = HashMap::new();
        row2.insert("user_id".to_string(), Value::I64(1));
        row2.insert("name".to_string(), Value::String("Order2".to_string()));

        let mut row3 = HashMap::new();
        row3.insert("user_id".to_string(), Value::I64(2));
        row3.insert("name".to_string(), Value::String("Order3".to_string()));

        let grouped = DataLoaderStrategy::group_by_foreign_key(vec![row1, row2, row3], "user_id");
        assert_eq!(grouped.get("i64:1").unwrap().len(), 2);
        assert_eq!(grouped.get("i64:2").unwrap().len(), 1);
    }

    #[test]
    fn test_data_loader_strategy_build_batch_sql() {
        let strategy = DataLoaderStrategy::new();
        let rel = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let sql = strategy.build_batch_sql(&rel, &["id", "user_id", "total"], 3);
        assert!(sql.contains("SELECT id, user_id, total FROM orders"));
        assert!(sql.contains("WHERE user_id IN (?, ?, ?)"));
        assert!(!sql.contains("SELECT *"));
    }

    #[test]
    fn test_intermediate_table_strategy_build_sql() {
        let strategy = IntermediateTableStrategy::new();
        let rel = RelationDef::new_many_to_many(
            "roles",
            "users",
            "roles",
            "id",
            "id",
            "user_roles",
            "user_id",
            "role_id",
        );
        let sql = strategy
            .build_intermediate_sql(&rel, &["id", "name"], 2)
            .unwrap();
        assert!(sql.contains("FROM user_roles AS jt"));
        assert!(sql.contains("JOIN roles AS related"));
        assert!(sql.contains("ON jt.role_id = related.id"));
        assert!(sql.contains("WHERE jt.user_id IN (?, ?)"));
        assert!(!sql.contains("SELECT *"));
    }

    #[test]
    fn test_intermediate_table_strategy_missing_config() {
        let strategy = IntermediateTableStrategy::new();
        let rel = RelationDef::new(
            "roles",
            "users",
            "roles",
            "id",
            "role_id",
            RelationKind::ManyToMany,
        );
        let result = strategy.build_intermediate_sql(&rel, &["id"], 1);
        assert!(result.is_err());
        match result.unwrap_err() {
            DbError::InvalidInput(msg) => assert!(msg.contains("缺少中间表配置")),
            other => panic!("预期 InvalidInput，得到 {:?}", other),
        }
    }

    #[test]
    fn test_smart_eager_loader_new() {
        let rel = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = SmartEagerLoader::new(rel);
        assert_eq!(loader.children_count(), 0);
        assert_eq!(loader.n1_threshold, 5);
        assert!(loader.decisions.is_empty());
    }

    #[test]
    fn test_smart_eager_loader_with_chain() {
        let rel1 = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let rel2 = RelationDef::new(
            "items",
            "orders",
            "items",
            "id",
            "order_id",
            RelationKind::HasMany,
        );
        let loader = SmartEagerLoader::new(rel1).with(rel2);
        assert_eq!(loader.children_count(), 1);
    }

    #[test]
    fn test_smart_eager_loader_with_n1_threshold() {
        let rel = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = SmartEagerLoader::new(rel).with_n1_threshold(10);
        assert_eq!(loader.n1_threshold, 10);
    }

    #[test]
    fn test_relation_def_new_many_to_many() {
        let rel = RelationDef::new_many_to_many(
            "roles",
            "users",
            "roles",
            "id",
            "id",
            "user_roles",
            "user_id",
            "role_id",
        );
        assert_eq!(rel.kind, RelationKind::ManyToMany);
        assert_eq!(rel.join_table, Some("user_roles"));
        assert_eq!(rel.join_from_key, Some("user_id"));
        assert_eq!(rel.join_to_key, Some("role_id"));
    }

    #[test]
    fn test_relation_def_new_backward_compat() {
        let rel = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        assert_eq!(rel.join_table, None);
        assert_eq!(rel.join_from_key, None);
        assert_eq!(rel.join_to_key, None);
    }
}
