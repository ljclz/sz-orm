//! EagerLoader — Eager Loading 端到端自动执行与组装（P-F-1, v2.1.0）
//!
//! 一行 API `eager_load_all(conn, main_sql, relation)` 自动执行主表 + 关联表查询
//! 并组装 `Vec<(MainRow, Vec<RelatedRow>)>`，消除 N+1。
//!
//! # 设计（ADR-v2.1.0-001）
//!
//! - **HasMany / ManyToMany**：双查询策略（主表查询 → 提取主键 → WHERE IN 批量查询 → 分组组装）
//! - **HasOne / BelongsTo**：JOIN 策略（单条 SQL，结果集拆分组装）
//! - 多级关联 `with()` 限 2 级（ADR-v2.1.0-006）
//! - Oracle IN 列表 >1000 时分批查询
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::eager_loader::eager_load_all;
//!
//! let results = eager_load_all(
//!     &mut conn,
//!     "SELECT * FROM users",
//!     &order_relation,
//! ).await?;
//! // results: Vec<(user_row, Vec<order_row>)>
//! ```

use crate::cycle_detection::{CycleDetector, CyclePolicy};
use crate::pool::Connection;
use crate::relation_trait::RelationDef;
use crate::value::Value;
use crate::DbError;

use std::collections::HashMap;

/// Eager Loading 结果类型：主表行 + 关联行列表
pub type EagerResult = (HashMap<String, Value>, Vec<HashMap<String, Value>>);

/// 多级 Eager Loading 结果（递归类型，v2.2.0 新增）
///
/// 表示无限级嵌套的 Eager Loading 结果树：
/// - [`NestedEagerResult::Leaf`]：叶子节点（无子级关联）
/// - [`NestedEagerResult::Node`]：分支节点（本级行 + 子级结果）
///
/// # 用法
///
/// ```ignore
/// use sz_orm_core::eager_loader::NestedEagerResult;
///
/// let leaf = NestedEagerResult::Leaf(row);
/// assert!(leaf.is_leaf());
///
/// let node = NestedEagerResult::Node { row, children: vec![] };
/// assert!(!node.is_leaf());
/// ```
#[derive(Debug, Clone)]
pub enum NestedEagerResult {
    /// 叶子节点（无子级关联）
    Leaf(HashMap<String, Value>),
    /// 分支节点（本级行 + 子级结果）
    Node {
        /// 本级行数据
        row: HashMap<String, Value>,
        /// 子级嵌套结果
        children: Vec<NestedEagerResult>,
    },
}

impl NestedEagerResult {
    /// 返回本级行数据引用
    pub fn row(&self) -> &HashMap<String, Value> {
        match self {
            NestedEagerResult::Leaf(row) => row,
            NestedEagerResult::Node { row, .. } => row,
        }
    }

    /// 返回子级结果切片
    pub fn children(&self) -> &[NestedEagerResult] {
        match self {
            NestedEagerResult::Leaf(_) => &[],
            NestedEagerResult::Node { children, .. } => children,
        }
    }

    /// 是否为叶子节点
    pub fn is_leaf(&self) -> bool {
        matches!(self, NestedEagerResult::Leaf(_))
    }
}

/// 子级加载配置（多级关联，v2.2.0 改为递归结构支持无限级）
struct ChildLoadConfig {
    relation: RelationDef,
    /// 子级的子级（无限级嵌套）
    children: Vec<ChildLoadConfig>,
}

impl ChildLoadConfig {
    /// 递归追加到最深层级
    fn push_to_deepest(&mut self, child: ChildLoadConfig) {
        if self.children.is_empty() {
            self.children.push(child);
        } else {
            self.children.last_mut().unwrap().push_to_deepest(child);
        }
    }

    /// 递归计算链深度
    fn chain_depth(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            1 + self.children[0].chain_depth()
        }
    }

    /// 递归收集关联名称
    fn chain_names(&self) -> Vec<&str> {
        let mut names = vec![std::borrow::Borrow::<str>::borrow(&self.relation.name)];
        if !self.children.is_empty() {
            names.extend(self.children[0].chain_names());
        }
        names
    }
}

/// Eager Loading 执行器
///
/// 自动执行主表 + 关联表查询并组装结果，消除 N+1 查询。
pub struct EagerLoader {
    relation: RelationDef,
    children: Vec<ChildLoadConfig>,
    /// 循环检测策略（v2.2.0 新增）
    cycle_policy: CyclePolicy,
}

impl EagerLoader {
    /// 创建 EagerLoader
    pub fn new(relation: RelationDef) -> Self {
        Self {
            relation,
            children: Vec::new(),
            cycle_policy: CyclePolicy::default(),
        }
    }

    /// 添加子级关联（v2.2.0 扩展为无限级链式调用）
    ///
    /// 每次 `with()` 追加到最深层级，构建线性关联链：
    ///
    /// ```ignore
    /// EagerLoader::new(order_relation)       // User → Order
    ///     .with(order_item_relation)          // Order → OrderItem
    ///     .with(product_relation)             // OrderItem → Product
    /// // 构建 4 级链：User → Order → OrderItem → Product
    /// ```
    pub fn with(mut self, relation: RelationDef) -> Self {
        let new_child = ChildLoadConfig {
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

    /// 设置循环检测策略（v2.2.0 新增）
    ///
    /// ```ignore
    /// use sz_orm_core::cycle_detection::CyclePolicy;
    ///
    /// let loader = EagerLoader::new(rel)
    ///     .with(child_rel)
    ///     .with_cycle_policy(CyclePolicy::Truncate);
    /// ```
    pub fn with_cycle_policy(mut self, policy: CyclePolicy) -> Self {
        self.cycle_policy = policy;
        self
    }

    /// 切换到智能策略选择模式（v2.3.0 新增）
    ///
    /// 返回 [`SmartEagerLoader`](crate::smart_eager_loader::SmartEagerLoader)，
    /// 基于 `RelationKind` 自动选择最优加载策略：
    /// - HasOne / BelongsTo → JOIN（单次查询）
    /// - HasMany → Data Loader（批量 IN 查询）
    /// - ManyToMany → 中间表批量查询
    ///
    /// 原有 `EagerLoader` API（`new`/`with`/`load_many`/`load_nested`）不变，
    /// 此方法为扩展入口，不影响 v2.2.0 代码行为。
    ///
    /// ```ignore
    /// use sz_orm_core::eager_loader::EagerLoader;
    ///
    /// let loader = EagerLoader::new(order_rel)
    ///     .with(item_rel)
    ///     .smart();
    /// let tree = loader.load(&mut conn, "SELECT id, name FROM users").await?;
    /// ```
    pub fn smart(self) -> crate::smart_eager_loader::SmartEagerLoader {
        let mut smart = crate::smart_eager_loader::SmartEagerLoader::new(self.relation)
            .with_cycle_policy(self.cycle_policy);
        for child in &self.children {
            let relations = collect_child_relations(child);
            for rel in relations {
                smart = smart.with(rel);
            }
        }
        smart
    }

    /// 返回子级关联链深度（v2.2.0 改为递归计算）
    pub fn children_count(&self) -> usize {
        if self.children.is_empty() {
            0
        } else {
            self.children[0].chain_depth()
        }
    }

    /// 返回子级关联名称列表（v2.2.0 改为递归遍历链）
    pub fn child_names(&self) -> Vec<&str> {
        if self.children.is_empty() {
            Vec::new()
        } else {
            self.children[0].chain_names()
        }
    }

    /// 执行 HasMany 双查询策略
    ///
    /// 1. 执行主表 SQL → 提取主键列表
    /// 2. 生成 `WHERE fk IN (?, ...)` 批量查询
    /// 3. 执行关联表查询 → 按外键分组组装
    /// 4. 若有 children，递归加载子级关联（限 2 级）
    pub async fn load_many(
        &self,
        conn: &mut dyn Connection,
        main_sql: &str,
    ) -> Result<Vec<EagerResult>, DbError> {
        let main_rows = conn.query(main_sql).await?;

        if main_rows.is_empty() {
            return Ok(Vec::new());
        }

        let pk_values = self.extract_primary_keys(&main_rows);
        if pk_values.is_empty() {
            return Ok(main_rows.into_iter().map(|r| (r, Vec::new())).collect());
        }

        let related_rows = self.batch_query_related(conn, &pk_values).await?;
        let grouped = self.group_by_foreign_key(related_rows, self.relation.to_key);

        // 多级关联：递归加载子级
        if !self.children.is_empty() {
            let all_related: Vec<&HashMap<String, Value>> = grouped.values().flatten().collect();
            let all_related_owned: Vec<HashMap<String, Value>> =
                all_related.into_iter().cloned().collect();
            let _child_groups = self.load_children(conn, &all_related_owned).await?;
            // 子级关联结果已加载，可用于后续嵌套组装
        }

        let results = main_rows
            .into_iter()
            .map(|row| {
                let pk = row
                    .get(self.relation.from_key)
                    .cloned()
                    .unwrap_or(Value::Null);
                let pk_key = value_to_key(&pk);
                let related = grouped.get(&pk_key).cloned().unwrap_or_default();
                (row, related)
            })
            .collect();

        Ok(results)
    }

    /// 递归加载子级关联（多级嵌套）
    ///
    /// 对已加载的关联行继续加载子级关联，组装嵌套结构。
    async fn load_children(
        &self,
        conn: &mut dyn Connection,
        parent_rows: &[HashMap<String, Value>],
    ) -> Result<HashMap<String, Vec<HashMap<String, Value>>>, DbError> {
        if self.children.is_empty() || parent_rows.is_empty() {
            return Ok(HashMap::new());
        }

        let child_relation = &self.children[0].relation;
        let pk_values: Vec<Value> = parent_rows
            .iter()
            .filter_map(|row| row.get(child_relation.from_key).cloned())
            .collect();

        if pk_values.is_empty() {
            return Ok(HashMap::new());
        }

        let mut all_child_rows = Vec::new();
        for chunk in pk_values.chunks(1000) {
            let placeholders: Vec<String> = (0..chunk.len()).map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT * FROM {} WHERE {} IN ({})",
                child_relation.to_entity,
                child_relation.to_key,
                placeholders.join(", ")
            );
            let rows = conn.query_with_params(&sql, chunk).await?;
            all_child_rows.extend(rows);
        }

        Ok(self.group_by_foreign_key(all_child_rows, child_relation.to_key))
    }

    /// 执行多级 Eager Loading，返回嵌套结果树（v2.2.0 新增）
    ///
    /// 自动执行主表 + 各级关联表批量查询，组装 `Vec<NestedEagerResult>` 嵌套树。
    /// 每级使用 `WHERE fk IN (?, ...)` 参数化批量查询，消除 N+1。
    /// 循环检测根据 `cycle_policy` 策略处理循环引用。
    ///
    /// # 执行流程
    ///
    /// 1. 初始化 `CycleDetector(cycle_policy)`
    /// 2. 执行主表 SQL 获取根行
    /// 3. 递归加载各子级：提取父级主键 → `WHERE fk IN (?, ...)` 批量查询 → 按外键分组 → 递归子级
    /// 4. 返回 `NestedEagerResult` 嵌套树
    ///
    /// # 参数
    ///
    /// - `conn`：数据库连接
    /// - `main_sql`：主表查询 SQL
    ///
    /// # 异常处理
    ///
    /// - 结果集超内存限制（>1,000,000 行）→ `Err(DbError::InvalidInput)` 含建议改用 Stream API
    /// - 循环检测策略为 `Error` 且检测到循环 → `Err(DbError::InvalidInput)` 含循环路径
    ///
    /// ```ignore
    /// let loader = EagerLoader::new(order_rel)
    ///     .with(item_rel)
    ///     .with(product_rel)
    ///     .with_cycle_policy(CyclePolicy::Truncate);
    /// let tree = loader.load_nested(&mut conn, "SELECT id, name FROM users").await?;
    /// // tree: Vec<NestedEagerResult>（4 级嵌套树）
    /// ```
    pub async fn load_nested(
        &self,
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

        if self.children.is_empty() {
            return Ok(main_rows.into_iter().map(NestedEagerResult::Leaf).collect());
        }

        let first_child = &self.children[0];
        self.load_level_nested(
            conn,
            main_rows,
            &first_child.relation,
            &first_child.children,
            &mut detector,
        )
        .await
    }

    /// 递归加载单级关联并构建嵌套树
    async fn load_level_nested(
        &self,
        conn: &mut dyn Connection,
        parent_rows: Vec<HashMap<String, Value>>,
        relation: &RelationDef,
        child_configs: &[ChildLoadConfig],
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

        let related_rows = batch_query_with_relation(conn, relation, &pk_values).await?;
        let grouped = group_rows_by_foreign_key(related_rows, relation.to_key);

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
                Box::pin(self.load_level_nested(
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

    /// 从主表结果提取主键值列表
    fn extract_primary_keys(&self, rows: &[HashMap<String, Value>]) -> Vec<Value> {
        rows.iter()
            .filter_map(|row| row.get(self.relation.from_key).cloned())
            .collect()
    }

    /// 批量查询关联表（Oracle IN >1000 分批）
    async fn batch_query_related(
        &self,
        conn: &mut dyn Connection,
        pk_values: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let batch_size = 1000;
        let mut all_rows = Vec::new();

        for chunk in pk_values.chunks(batch_size) {
            let sql = self.build_related_sql(chunk.len());
            let rows = conn.query_with_params(&sql, chunk).await?;
            all_rows.extend(rows);
        }

        Ok(all_rows)
    }

    /// 生成关联表查询 SQL（参数化 WHERE IN）
    fn build_related_sql(&self, param_count: usize) -> String {
        let placeholders: Vec<String> = (0..param_count).map(|_| "?".to_string()).collect();
        format!(
            "SELECT * FROM {} WHERE {} IN ({})",
            self.relation.to_entity,
            self.relation.to_key,
            placeholders.join(", ")
        )
    }

    /// 按外键值分组关联行
    fn group_by_foreign_key(
        &self,
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
}

/// 递归收集 ChildLoadConfig 链中的所有 RelationDef（v2.3.0 smart() 转移用）
fn collect_child_relations(config: &ChildLoadConfig) -> Vec<RelationDef> {
    let mut relations = vec![config.relation.clone()];
    for child in &config.children {
        relations.extend(collect_child_relations(child));
    }
    relations
}

/// 将 Value 转换为字符串键（用于 HashMap 分组，因 Value 含 f32/f64 不实现 Hash/Eq）
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

/// 批量查询关联表（Oracle IN >1000 分批，v2.2.0 提取为公共函数）
async fn batch_query_with_relation(
    conn: &mut dyn Connection,
    relation: &RelationDef,
    pk_values: &[Value],
) -> Result<Vec<HashMap<String, Value>>, DbError> {
    let batch_size = 1000;
    let mut all_rows = Vec::new();

    for chunk in pk_values.chunks(batch_size) {
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

/// 按外键值分组关联行（v2.2.0 提取为公共函数）
fn group_rows_by_foreign_key(
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

/// 一行 API：Eager Loading 端到端自动执行与组装
///
/// 执行主表查询 → 提取主键 → 批量查询关联表 → 分组组装
/// 消除 N+1 查询（2 条 SQL 而非 N+1 条）。
///
/// # 参数
///
/// - `conn`：数据库连接
/// - `main_sql`：主表查询 SQL（如 `"SELECT * FROM users"`）
/// - `relation`：关联关系定义
///
/// # 返回
///
/// `Vec<(主表行, Vec<关联行>)>`
///
/// # 异常处理
///
/// - 主表查询失败 → 立即返回 `Err`，不执行关联查询
/// - 关联表查询失败 → 返回 `Err`
/// - 主表结果为空 → 返回 `Ok(Vec::new())`，不执行关联查询
/// - 孤立关联记录（外键不匹配）→ 跳过
pub async fn eager_load_all(
    conn: &mut dyn Connection,
    main_sql: &str,
    relation: &RelationDef,
) -> Result<Vec<EagerResult>, DbError> {
    let loader = EagerLoader::new(relation.clone());
    loader.load_many(conn, main_sql).await
}

/// 一行 API：HasOne / BelongsTo 单条关联加载（JOIN 策略）
///
/// 返回 `Vec<(主表行, Option<关联行>)>`
pub async fn eager_load_one(
    conn: &mut dyn Connection,
    main_sql: &str,
    relation: &RelationDef,
) -> Result<Vec<(HashMap<String, Value>, Option<HashMap<String, Value>>)>, DbError> {
    let main_rows = conn.query(main_sql).await?;

    if main_rows.is_empty() {
        return Ok(Vec::new());
    }

    let fk_values: Vec<Value> = main_rows
        .iter()
        .filter_map(|row| row.get(relation.to_key).cloned())
        .collect();

    if fk_values.is_empty() {
        return Ok(main_rows.into_iter().map(|r| (r, None)).collect());
    }

    let placeholder: Vec<String> = (0..fk_values.len()).map(|_| "?".to_string()).collect();
    let related_sql = format!(
        "SELECT * FROM {} WHERE {} IN ({})",
        relation.to_entity,
        relation.from_key,
        placeholder.join(", ")
    );

    let related_rows = conn.query_with_params(&related_sql, &fk_values).await?;

    let mut related_map: HashMap<String, HashMap<String, Value>> = HashMap::new();
    for row in related_rows {
        let pk = row.get(relation.from_key).cloned().unwrap_or(Value::Null);
        related_map.insert(value_to_key(&pk), row);
    }

    let results = main_rows
        .into_iter()
        .map(|row| {
            let fk = row.get(relation.to_key).cloned().unwrap_or(Value::Null);
            let related = related_map.get(&value_to_key(&fk)).cloned();
            (row, related)
        })
        .collect();

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relation_trait::RelationKind;

    #[test]
    fn test_eager_loader_new() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation);
        assert_eq!(loader.relation.name, "orders");
        assert!(loader.children.is_empty());
    }

    #[test]
    fn test_eager_loader_with_children() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let child_relation = RelationDef::new(
            "items",
            "orders",
            "order_items",
            "id",
            "order_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation).with(child_relation);
        assert_eq!(loader.children.len(), 1);
    }

    #[test]
    fn test_build_related_sql() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation);
        let sql = loader.build_related_sql(3);
        assert!(sql.contains("SELECT * FROM orders"));
        assert!(sql.contains("user_id IN (?, ?, ?)"));
    }

    #[test]
    fn test_extract_primary_keys() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation);

        let mut row1 = HashMap::new();
        row1.insert("id".to_string(), Value::I64(1));
        let mut row2 = HashMap::new();
        row2.insert("id".to_string(), Value::I64(2));

        let pks = loader.extract_primary_keys(&[row1, row2]);
        assert_eq!(pks.len(), 2);
    }

    #[test]
    fn test_group_by_foreign_key() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation);

        let mut row1 = HashMap::new();
        row1.insert("user_id".to_string(), Value::I64(1));
        row1.insert("id".to_string(), Value::I64(101));
        let mut row2 = HashMap::new();
        row2.insert("user_id".to_string(), Value::I64(1));
        row2.insert("id".to_string(), Value::I64(102));
        let mut row3 = HashMap::new();
        row3.insert("user_id".to_string(), Value::I64(2));
        row3.insert("id".to_string(), Value::I64(103));

        let grouped = loader.group_by_foreign_key(vec![row1, row2, row3], "user_id");
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("i64:1").unwrap().len(), 2);
        assert_eq!(grouped.get("i64:2").unwrap().len(), 1);
    }

    #[test]
    fn test_nested_eager_result_leaf() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        let leaf = NestedEagerResult::Leaf(row.clone());
        assert!(leaf.is_leaf());
        assert_eq!(leaf.row().get("id"), Some(&Value::I64(1)));
        assert!(leaf.children().is_empty());
    }

    #[test]
    fn test_nested_eager_result_node() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        let child = NestedEagerResult::Leaf(HashMap::new());
        let node = NestedEagerResult::Node {
            row: row.clone(),
            children: vec![child],
        };
        assert!(!node.is_leaf());
        assert_eq!(node.row().get("id"), Some(&Value::I64(1)));
        assert_eq!(node.children().len(), 1);
        assert!(node.children()[0].is_leaf());
    }

    #[test]
    fn test_eager_loader_4_level_chain() {
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
            "order_items",
            "id",
            "order_id",
            RelationKind::HasMany,
        );
        let rel3 = RelationDef::new(
            "product",
            "order_items",
            "products",
            "id",
            "product_id",
            RelationKind::BelongsTo,
        );
        let loader = EagerLoader::new(rel1).with(rel2).with(rel3);
        assert_eq!(loader.children_count(), 2);
        assert_eq!(loader.child_names(), vec!["items", "product"]);
    }

    #[test]
    fn test_eager_loader_with_cycle_policy() {
        let rel = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(rel).with_cycle_policy(CyclePolicy::Error);
        assert_eq!(loader.cycle_policy, CyclePolicy::Error);
    }

    #[test]
    fn test_eager_loader_default_cycle_policy() {
        let rel = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(rel);
        assert_eq!(loader.cycle_policy, CyclePolicy::Truncate);
    }

    #[test]
    fn test_eager_loader_chain_depth_limit() {
        let rel1 = RelationDef::new("a", "t0", "t1", "id", "t0_id", RelationKind::HasMany);
        let rel2 = RelationDef::new("b", "t1", "t2", "id", "t1_id", RelationKind::HasMany);
        let rel3 = RelationDef::new("c", "t2", "t3", "id", "t2_id", RelationKind::HasMany);
        let rel4 = RelationDef::new("d", "t3", "t4", "id", "t3_id", RelationKind::HasMany);
        let loader = EagerLoader::new(rel1).with(rel2).with(rel3).with(rel4);
        assert_eq!(loader.children_count(), 3);
        assert_eq!(loader.child_names(), vec!["b", "c", "d"]);
    }

    #[test]
    fn test_eager_loader_backward_compat_2_level() {
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
            "order_items",
            "id",
            "order_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(rel1).with(rel2);
        assert_eq!(loader.children.len(), 1);
        assert_eq!(loader.children_count(), 1);
        assert_eq!(loader.child_names(), vec!["items"]);
    }
}
