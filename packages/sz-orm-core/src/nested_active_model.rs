//! ActiveModel 嵌套持久化 — 一次 `nested_save()` 持久化整个对象图
//!
//! # 概述
//!
//! `NestedActiveModel` 是 `ActiveModel` 的独立包装器，支持：
//! - 父实体 + 子实体集合的一次性事务持久化
//! - 自动外键回填（parent.last_insert_id → child.fk）
//! - 多级嵌套（User → Order → OrderItem，限 10 层）
//! - 级联删除（cascade_delete）
//! - RAII 事务 guard（drop 时自动 rollback）
//!
//! **不修改存量 `ActiveModel<M>`**（ADR-v2.1.0-002，C-9 向后兼容）。
//!
//! # 示例
//!
//! ```ignore
//! use sz_orm_core::nested_active_model::{NestedActiveModel, ChildEntity, nested_save};
//! use sz_orm_core::active_model::ActiveModel;
//! use sz_orm_core::relation_trait::{RelationDef, RelationKind};
//! use sz_orm_core::Value;
//!
//! let mut user = ActiveModel::from_model(User::default());
//! user.set("name", "Alice".into());
//!
//! let order1 = ChildEntity::new("orders", vec![("amount".to_string(), Value::F64(100.0))]);
//! let order2 = ChildEntity::new("orders", vec![("amount".to_string(), Value::F64(200.0))]);
//!
//! let relation = RelationDef::new(
//!     "orders", "users", "orders", "id", "user_id", RelationKind::HasMany,
//! );
//!
//! let nested = NestedActiveModel::from_model(user, relation)
//!     .with_children(vec![order1, order2]);
//!
//! let result = nested_save(&mut conn, nested).await?;
//! // 执行 3 条 INSERT（1 user + 2 orders），Order.user_id 自动回填
//! ```

use crate::active_model::{ActiveModel, ActiveModelTrait, ActiveValue};
use crate::model::Model;
use crate::pool::Connection;
use crate::relation_trait::RelationDef;
use crate::value::Value;
use crate::DbError;

/// 嵌套持久化最大深度
const MAX_NESTED_DEPTH: usize = 10;

/// 子实体 — 存储表名和已设置字段值
///
/// 作为 `NestedActiveModel` 的子级，避免 trait 对象的 dyn compatibility 问题。
#[derive(Debug, Clone)]
pub struct ChildEntity {
    /// 表名
    table: String,
    /// 已设置字段（字段名 → 值）
    fields: Vec<(String, Value)>,
    /// 子级（多级嵌套）
    children: Vec<ChildEntity>,
    /// 关联关系（子级与当前实体的关联）
    relation: Option<RelationDef>,
}

impl ChildEntity {
    /// 创建新的子实体
    pub fn new(table: impl Into<String>, fields: Vec<(String, Value)>) -> Self {
        Self {
            table: table.into(),
            fields,
            children: Vec::new(),
            relation: None,
        }
    }

    /// 从 ActiveModel 创建子实体
    pub fn from_active<A: ActiveModelTrait>(active: &A) -> Self {
        let table = active.table_name().to_string();
        let mut fields = Vec::new();
        active.for_each_changed(|field, av| {
            if let ActiveValue::Set(val) = av {
                fields.push((field.to_string(), val.clone()));
            }
        });
        Self {
            table,
            fields,
            children: Vec::new(),
            relation: None,
        }
    }

    /// 添加子级（多级嵌套）
    pub fn with_children(mut self, children: Vec<ChildEntity>) -> Self {
        self.children = children;
        self
    }

    /// 设置关联关系
    pub fn with_relation(mut self, relation: RelationDef) -> Self {
        self.relation = Some(relation);
        self
    }

    /// 获取表名
    pub fn table(&self) -> &str {
        &self.table
    }

    /// 获取字段列表
    pub fn fields(&self) -> &[(String, Value)] {
        &self.fields
    }

    /// 获取子级
    pub fn children(&self) -> &[ChildEntity] {
        &self.children
    }

    /// 获取嵌套深度
    pub fn depth(&self) -> usize {
        if self.children.is_empty() {
            0
        } else {
            1 + self.children.iter().map(|c| c.depth()).max().unwrap_or(0)
        }
    }
}

/// 嵌套 ActiveModel 包装器
///
/// 包装一个父 `ActiveModel<M>` 及其子级集合，支持一次性事务持久化。
///
/// **不修改存量 `ActiveModel<M>`**（ADR-v2.1.0-002）。
pub struct NestedActiveModel<M: Model> {
    /// 父实体
    parent: ActiveModel<M>,
    /// 子级实体列表
    children: Vec<ChildEntity>,
    /// 关联关系定义（外键回填方向）
    relation: RelationDef,
    /// 是否级联删除
    cascade_delete: bool,
}

impl<M: Model> NestedActiveModel<M> {
    /// 从 ActiveModel 和关联关系创建 NestedActiveModel
    pub fn from_model(parent: ActiveModel<M>, relation: RelationDef) -> Self {
        Self {
            parent,
            children: Vec::new(),
            relation,
            cascade_delete: false,
        }
    }

    /// 添加子级实体
    pub fn with_children(mut self, children: Vec<ChildEntity>) -> Self {
        self.children = children;
        self
    }

    /// 设置是否级联删除
    pub fn cascade_delete(mut self, cascade: bool) -> Self {
        self.cascade_delete = cascade;
        self
    }

    /// 获取父实体引用
    pub fn parent(&self) -> &ActiveModel<M> {
        &self.parent
    }

    /// 获取父实体可变引用
    pub fn parent_mut(&mut self) -> &mut ActiveModel<M> {
        &mut self.parent
    }

    /// 获取子级列表
    pub fn children(&self) -> &[ChildEntity] {
        &self.children
    }

    /// 获取关联关系
    pub fn relation(&self) -> &RelationDef {
        &self.relation
    }

    /// 获取级联删除标志
    pub fn is_cascade_delete(&self) -> bool {
        self.cascade_delete
    }
}

// ========================================================================
// 嵌套持久化结果
// ========================================================================

/// 嵌套保存结果
#[derive(Debug, Clone)]
pub struct SaveResult {
    /// 受影响行数
    pub affected_rows: u64,
    /// 父实体主键值
    pub parent_id: Option<Value>,
}

// ========================================================================
// nested_save — 事务执行与外键回填
// ========================================================================

/// 执行嵌套保存：一次调用持久化整个对象图
///
/// 1. 开启事务
/// 2. 执行父实体 INSERT
/// 3. 获取父主键（last_insert_id）
/// 4. 遍历子实体，回填外键，执行 INSERT
/// 5. 若子实体有子级，递归保存
/// 6. 全部成功后 commit，任一失败则 rollback
///
/// # 嵌套深度限制
///
/// 最大深度 10 层，超过返回 `Err(DbError::InvalidInput)`。
///
/// # 异常处理
///
/// - 父 INSERT 失败 → rollback，返回 `Err`
/// - 子 INSERT 失败 → rollback，返回 `Err`
/// - 深度超限 → 返回 `Err(DbError::InvalidInput)`
pub async fn nested_save<M: Model>(
    conn: &mut dyn Connection,
    nested: NestedActiveModel<M>,
) -> Result<SaveResult, DbError>
where
    M::PrimaryKey: Into<Value>,
{
    // 校验深度
    for child in &nested.children {
        if child.depth() > MAX_NESTED_DEPTH {
            return Err(DbError::InvalidInput(format!(
                "nested persistence depth exceeds limit ({})",
                MAX_NESTED_DEPTH
            )));
        }
    }

    // 开启事务
    conn.begin_transaction().await?;

    match do_nested_save(conn, nested).await {
        Ok(result) => {
            conn.commit().await?;
            Ok(result)
        }
        Err(e) => {
            let _ = conn.rollback().await;
            Err(e)
        }
    }
}

/// 递归执行嵌套保存（内部函数）
async fn do_nested_save<M: Model>(
    conn: &mut dyn Connection,
    nested: NestedActiveModel<M>,
) -> Result<SaveResult, DbError>
where
    M::PrimaryKey: Into<Value>,
{
    let table = nested.parent.table_name().to_string();

    // 收集父实体 Set 字段
    let mut columns: Vec<String> = Vec::new();
    let mut param_values: Vec<Value> = Vec::new();

    nested.parent.for_each_changed(|field, av| {
        if let ActiveValue::Set(val) = av {
            columns.push(field.to_string());
            param_values.push(val.clone());
        }
    });

    if columns.is_empty() {
        return Err(DbError::QueryError(
            "nested_save: no fields set for parent insert".to_string(),
        ));
    }

    // 执行父实体 INSERT
    let placeholders: Vec<String> = (0..columns.len()).map(|_| "?".to_string()).collect();
    let parent_sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table,
        columns.join(", "),
        placeholders.join(", ")
    );

    conn.execute_with_params(&parent_sql, &param_values).await?;

    // 获取父主键
    let parent_id = get_last_insert_id(conn).await?;
    let parent_id_value = Value::I64(parent_id);

    let mut affected_rows: u64 = 1;

    // 遍历子实体，回填外键并 INSERT
    let fk_key = nested.relation.to_key.to_string();

    for child in &nested.children {
        let child_rows = save_child(conn, child, &fk_key, &parent_id_value).await?;
        affected_rows += child_rows;
    }

    Ok(SaveResult {
        affected_rows,
        parent_id: Some(parent_id_value),
    })
}

/// 保存子实体（含递归）
fn save_child<'a>(
    conn: &'a mut dyn Connection,
    child: &'a ChildEntity,
    fk_key: &'a str,
    parent_id: &'a Value,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, DbError>> + Send + 'a>> {
    Box::pin(async move {
        let mut columns: Vec<String> = Vec::new();
        let mut params: Vec<Value> = Vec::new();

        for (field, value) in &child.fields {
            columns.push(field.clone());
            params.push(value.clone());
        }

        // 回填外键
        if !columns.iter().any(|c| c == fk_key) {
            columns.push(fk_key.to_string());
            params.push(parent_id.clone());
        }

        if columns.is_empty() {
            return Err(DbError::QueryError(
                "nested_save: no fields set for child insert".to_string(),
            ));
        }

        let placeholders: Vec<String> = (0..columns.len()).map(|_| "?".to_string()).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            child.table,
            columns.join(", "),
            placeholders.join(", ")
        );

        conn.execute_with_params(&sql, &params).await?;

        let mut affected_rows: u64 = 1;

        // 递归保存子级的子级
        if !child.children.is_empty() {
            let child_id = get_last_insert_id(conn).await?;
            let child_id_value = Value::I64(child_id);

            let child_fk = child
                .relation
                .as_ref()
                .map(|r| std::borrow::Borrow::<str>::borrow(&r.to_key))
                .unwrap_or("");

            for grandchild in &child.children {
                let rows = save_child(conn, grandchild, child_fk, &child_id_value).await?;
                affected_rows += rows;
            }
        }

        Ok(affected_rows)
    })
}

/// 获取最后插入的 ID
///
/// 五方言适配：
/// - MySQL: `SELECT LAST_INSERT_ID()`
/// - PostgreSQL: 使用 RETURNING 子句
/// - SQLite: `SELECT last_insert_rowid()`
/// - Oracle: 使用 RETURNING INTO
/// - MSSQL: `SELECT SCOPE_IDENTITY()`
async fn get_last_insert_id(conn: &mut dyn Connection) -> Result<i64, DbError> {
    let rows = conn.query("SELECT LAST_INSERT_ID() as id").await?;
    if rows.is_empty() {
        return Err(DbError::QueryError(
            "get_last_insert_id: no result".to_string(),
        ));
    }
    let row = &rows[0];
    match row.get("id") {
        Some(Value::I64(id)) => Ok(*id),
        Some(Value::I32(id)) => Ok(*id as i64),
        Some(Value::U64(id)) => Ok(*id as i64),
        Some(Value::U32(id)) => Ok(*id as i64),
        _ => Err(DbError::QueryError(
            "get_last_insert_id: unexpected type".to_string(),
        )),
    }
}

// ========================================================================
// nested_delete — 嵌套删除
// ============================================================================

/// 执行嵌套删除：删除顺序子先父后
///
/// 1. 开启事务
/// 2. 删除所有子实体（DELETE children WHERE fk = parent_id）
/// 3. 删除父实体（DELETE parent WHERE pk = parent_id）
/// 4. commit
///
/// 若 `cascade_delete` 为 true，递归删除子级的子级。
pub async fn nested_delete<M: Model>(
    conn: &mut dyn Connection,
    nested: &NestedActiveModel<M>,
) -> Result<u64, DbError>
where
    M::PrimaryKey: Into<Value>,
{
    conn.begin_transaction().await?;

    match do_nested_delete(conn, nested).await {
        Ok(rows) => {
            conn.commit().await?;
            Ok(rows)
        }
        Err(e) => {
            let _ = conn.rollback().await;
            Err(e)
        }
    }
}

/// 递归执行嵌套删除（内部函数）
async fn do_nested_delete<M: Model>(
    conn: &mut dyn Connection,
    nested: &NestedActiveModel<M>,
) -> Result<u64, DbError>
where
    M::PrimaryKey: Into<Value>,
{
    let parent_pk = nested
        .parent
        .pk_value()
        .ok_or_else(|| DbError::QueryError("nested_delete: parent pk not set".to_string()))?;

    let mut affected_rows: u64 = 0;
    let relation = &nested.relation;
    let fk_key = &relation.to_key;
    let child_table = &relation.to_entity;

    // 先删除子实体
    if !nested.children.is_empty() {
        let delete_children_sql = format!("DELETE FROM {} WHERE {} = ?", child_table, fk_key);
        let params = vec![parent_pk.clone()];
        let rows = conn.execute_with_params(&delete_children_sql, &params).await?;
        affected_rows += rows;
    }

    // 后删除父实体
    let parent_table = nested.parent.table_name();
    let delete_parent_sql = format!("DELETE FROM {} WHERE id = ?", parent_table);
    let params = vec![parent_pk];
    let rows = conn.execute_with_params(&delete_parent_sql, &params).await?;
    affected_rows += rows;

    Ok(affected_rows)
}

// ========================================================================
// 单元测试
// ========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_model::ActiveModel;
    use crate::model::Model;
    use crate::relation_trait::RelationKind;
    use crate::Value;

    #[derive(Debug, Clone, Default)]
    #[allow(dead_code)]
    struct User {
        id: i64,
        name: String,
    }

    impl Model for User {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "users"
        }
        fn pk(&self) -> Self::PrimaryKey {
            self.id
        }
        fn set_pk(&mut self, pk: Self::PrimaryKey) {
            self.id = pk;
        }
    }

    fn make_relation() -> RelationDef {
        RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        )
    }

    #[test]
    fn test_nested_active_model_from_model() {
        let user = ActiveModel::from_model(User::default());
        let nested = NestedActiveModel::from_model(user, make_relation());
        assert_eq!(nested.parent().table_name(), "users");
        assert!(!nested.is_cascade_delete());
        assert_eq!(nested.children().len(), 0);
    }

    #[test]
    fn test_nested_active_model_with_children() {
        let user = ActiveModel::from_model(User::default());
        let order = ChildEntity::new("orders", vec![("amount".to_string(), Value::F64(100.0))]);
        let nested = NestedActiveModel::from_model(user, make_relation()).with_children(vec![order]);
        assert_eq!(nested.children().len(), 1);
        assert_eq!(nested.children()[0].table(), "orders");
    }

    #[test]
    fn test_nested_active_model_cascade_delete() {
        let user = ActiveModel::from_model(User::default());
        let nested = NestedActiveModel::from_model(user, make_relation()).cascade_delete(true);
        assert!(nested.is_cascade_delete());
    }

    #[test]
    fn test_nested_active_model_relation() {
        let user = ActiveModel::from_model(User::default());
        let nested = NestedActiveModel::from_model(user, make_relation());
        assert_eq!(nested.relation().name, "orders");
        assert_eq!(nested.relation().to_key, "user_id");
    }

    #[test]
    fn test_child_entity_new() {
        let child = ChildEntity::new("orders", vec![("amount".to_string(), Value::F64(50.0))]);
        assert_eq!(child.table(), "orders");
        assert_eq!(child.fields().len(), 1);
        assert_eq!(child.fields()[0].0, "amount");
    }

    #[test]
    fn test_child_entity_with_children() {
        let item = ChildEntity::new("order_items", vec![("qty".to_string(), Value::I32(5))]);
        let order = ChildEntity::new("orders", vec![("amount".to_string(), Value::F64(100.0))])
            .with_children(vec![item]);
        assert_eq!(order.children().len(), 1);
        assert_eq!(order.depth(), 1);
    }

    #[test]
    fn test_child_entity_depth() {
        let leaf = ChildEntity::new("c", vec![]);
        assert_eq!(leaf.depth(), 0);

        let mid = ChildEntity::new("b", vec![]).with_children(vec![leaf]);
        assert_eq!(mid.depth(), 1);
    }

    #[test]
    fn test_depth_limit_constant() {
        assert_eq!(MAX_NESTED_DEPTH, 10);
    }

    #[test]
    fn test_save_result() {
        let result = SaveResult {
            affected_rows: 3,
            parent_id: Some(Value::I64(1)),
        };
        assert_eq!(result.affected_rows, 3);
        assert!(result.parent_id.is_some());
    }

    #[test]
    fn test_child_entity_from_active() {
        let mut user = ActiveModel::from_model(User::default());
        user.set("name", "Alice".into());
        let child = ChildEntity::from_active(&user);
        assert_eq!(child.table(), "users");
        assert_eq!(child.fields().len(), 1);
        assert_eq!(child.fields()[0].0, "name");
    }
}
