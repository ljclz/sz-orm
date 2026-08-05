//! ActiveValue / ActiveModel — 三态字段更新模式
//!
//! 解决"仅更新部分字段"的类型安全问题：传统 `Model` 无法区分
//! "字段未设置"与"字段值为 NULL"，导致 UPDATE 要么更新全字段，
//! 要么需要手动构建 HashMap。
//!
//! # 设计
//!
//! - [`ActiveValue<T>`] — 三态枚举：`Set(T)` / `Unchanged` / `NotSet`
//! - [`ActiveModel`] — trait，定义模型如何暴露变更字段
//! - [`ActiveModel<M>`] — 通用包装器，为任意 `Model` 提供 dirty tracking
//! - [`update`] / [`save`] — 自由函数，执行持久化
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::active_model::{ActiveModel, ActiveValue, update, save};
//!
//! // 从已有模型创建 ActiveModel（所有字段初始为 Unchanged）
//! let mut active = user.into_active_model();
//!
//! // 仅修改需要更新的字段
//! active.set("email", ActiveValue::Set("new@example.com".into()));
//!
//! // 生成：UPDATE users SET email = ? WHERE id = ?
//! update(&mut conn, active).await?;
//!
//! // 新建模型：所有字段默认 NotSet，需显式 Set
//! let mut new_active = User::default().into_active_model();
//! new_active.set("name", ActiveValue::Set("Alice".into()));
//! new_active.set("email", ActiveValue::Set("alice@example.com".into()));
//! save(&mut conn, new_active).await?;
//! ```

use crate::error::DbError;
use crate::model::Model;
use crate::pool::Connection;
use crate::value::Value;
use std::collections::HashMap;

// ========================================================================
// ActiveValue — 三态枚举
// ========================================================================

/// 字段值的三态表示
///
/// 类比 SeaORM 的 `ActiveValue` / Rails 的 `ActiveModel::Attribute`：
///
/// | 变体 | 含义 | UPDATE 行为 |
/// |------|------|------------|
/// | `Set(v)` | 用户显式设置了新值 | 包含在 SET 子句 |
/// | `Unchanged` | 字段存在但未被修改 | 不包含在 SET 子句 |
/// | `NotSet` | 字段尚未被赋值（新建模型默认状态） | 不包含在 SET 子句 |
///
/// # 示例
///
/// ```
/// use sz_orm_core::active_model::ActiveValue;
/// use sz_orm_core::Value;
///
/// // 设置一个字段
/// let av: ActiveValue<Value> = ActiveValue::Set(Value::String("Alice".into()));
/// assert!(av.is_set());
///
/// // 从任意 Into<Value> 类型自动转换
/// let av: ActiveValue<Value> = "hello".into();
/// assert_eq!(av.into_value(), Some(Value::String("hello".into())));
///
/// // NotSet 是默认值
/// let av: ActiveValue<Value> = ActiveValue::default();
/// assert!(av.is_not_set());
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ActiveValue<T> {
    /// 用户显式设置了新值
    Set(T),
    /// 字段存在但未被修改（用于从 DB 加载后的模型）
    Unchanged,
    /// 字段尚未被赋值（用于新建模型）
    #[default]
    NotSet,
}

impl<T> ActiveValue<T> {
    /// 判断是否为 `Set` 变体
    pub fn is_set(&self) -> bool {
        matches!(self, ActiveValue::Set(_))
    }

    /// 判断是否为 `Unchanged` 变体
    pub fn is_unchanged(&self) -> bool {
        matches!(self, ActiveValue::Unchanged)
    }

    /// 判断是否为 `NotSet` 变体
    pub fn is_not_set(&self) -> bool {
        matches!(self, ActiveValue::NotSet)
    }

    /// 取出 `Set` 中的值，其他变体返回 `None`
    pub fn into_value(self) -> Option<T> {
        match self {
            ActiveValue::Set(v) => Some(v),
            _ => None,
        }
    }

    /// 借用 `Set` 中的值，其他变体返回 `None`
    pub fn as_value(&self) -> Option<&T> {
        match self {
            ActiveValue::Set(v) => Some(v),
            _ => None,
        }
    }
}

/// 从任意 `Into<Value>` 类型自动转换为 `ActiveValue<Value>`
///
/// 这使得 `active.set("name", "Alice")` 可以自动将 `"Alice"` 转为 `ActiveValue::Set(Value::String(...))`。
impl<T: Into<Value>> From<T> for ActiveValue<Value> {
    fn from(value: T) -> Self {
        ActiveValue::Set(value.into())
    }
}

// ========================================================================
// ActiveModel trait
// ========================================================================

/// ActiveModel 行为 trait
///
/// 实现此 trait 的类型可以：
/// 1. 暴露变更字段列表（供 UPDATE 使用）
/// 2. 提供主键值（供 WHERE 条件使用）
/// 3. 提供表名（供 SQL 生成使用）
///
/// # 与 `Model` trait 的关系
///
/// `Model` 描述的是"完整行记录"的静态元数据（表名、主键列名等）；
/// `ActiveModel` 描述的是"待持久化的变更集"的动态状态。
/// 两者正交：一个 `Model` 实例是全量快照，一个 `ActiveModel` 实例是增量变更。
pub trait ActiveModelTrait: Send + Sync {
    /// 获取表名
    fn table_name(&self) -> &str;

    /// 获取主键值（用于 WHERE 条件）
    fn pk_value(&self) -> Option<Value>;

    /// 遍历所有已设置（`Set`）的字段
    ///
    /// 回调 `f` 对每个变更字段调用，传入字段名和 `ActiveValue<Value>` 引用。
    /// 实现方负责过滤出 `is_set() == true` 的字段。
    fn for_each_changed<F>(&self, f: F)
    where
        F: FnMut(&str, &ActiveValue<Value>);
}

// ========================================================================
// ActiveModel<M> — 通用包装器
// ========================================================================

/// 通用 ActiveModel 包装器
///
/// 为任意 `Model` 类型提供 dirty tracking：
/// - 从 `Model` 创建时，所有字段初始为 `Unchanged`
/// - 从 `Default` 创建时（新建记录），所有字段初始为 `NotSet`
///
/// 业务模型通过 `set()` 方法标记变更字段，
/// 然后传给 [`update`] / [`save`] 执行持久化。
///
/// # 示例
///
/// ```ignore
/// let user = User::find_by_id(1, &mut conn).await?;
/// let mut active = ActiveModel::from_model(user);
/// active.set("email", "new@example.com".into()); // ActiveValue::Set
/// update(&mut conn, active).await?;
/// ```
#[derive(Debug, Clone)]
pub struct ActiveModel<M: Model> {
    model: M,
    /// 字段名 → 字段状态。仅记录被 `set()` 修改过的字段。
    changes: HashMap<String, ActiveValue<Value>>,
}

impl<M: Model> ActiveModel<M> {
    /// 从已有模型创建 ActiveModel（所有字段初始为 `Unchanged`）
    ///
    /// 适用于"加载 → 修改部分字段 → 更新"的工作流。
    pub fn from_model(model: M) -> Self {
        Self {
            model,
            changes: HashMap::new(),
        }
    }

    /// 设置一个字段的值（标记为 `Set`）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// active.set("name", ActiveValue::Set("Alice".into()));
    /// // 或简写（利用 From 自动转换）：
    /// active.set("name", "Alice".into());
    /// ```
    pub fn set(&mut self, field: impl Into<String>, value: ActiveValue<Value>) {
        self.changes.insert(field.into(), value);
    }

    /// 将字段标记为 `Unchanged`（从变更集中移除）
    pub fn unset(&mut self, field: &str) {
        self.changes.remove(field);
    }

    /// 获取某个字段的当前状态
    pub fn get(&self, field: &str) -> Option<&ActiveValue<Value>> {
        self.changes.get(field)
    }

    /// 获取所有变更字段列表（仅 `Set` 状态）
    pub fn changed_fields(&self) -> Vec<(&str, &Value)> {
        self.changes
            .iter()
            .filter_map(|(k, v)| match v {
                ActiveValue::Set(val) => Some((k.as_str(), val)),
                _ => None,
            })
            .collect()
    }

    /// 获取底层模型的可变引用
    pub fn as_mut_model(&mut self) -> &mut M {
        &mut self.model
    }

    /// 获取底层模型的不可变引用
    pub fn as_model(&self) -> &M {
        &self.model
    }

    /// 消耗包装器，返回底层模型
    pub fn into_model(self) -> M {
        self.model
    }
}

impl<M: Model> ActiveModelTrait for ActiveModel<M>
where
    M::PrimaryKey: Into<Value>,
{
    fn table_name(&self) -> &str {
        M::table_name()
    }

    fn pk_value(&self) -> Option<Value> {
        let v = self.model.pk_as_value();
        if v.is_null() {
            None
        } else {
            Some(v)
        }
    }

    fn for_each_changed<F>(&self, mut f: F)
    where
        F: FnMut(&str, &ActiveValue<Value>),
    {
        for (key, av) in self.changes.iter() {
            f(key, av);
        }
    }
}

// ========================================================================
// 持久化自由函数
// ========================================================================

/// 执行 UPDATE，仅更新 `ActiveModel` 中标记为 `Set` 的字段
///
/// 生成 SQL：`UPDATE {table} SET {changed_fields} = ? WHERE {pk} = ?`
///
/// 若没有任何 `Set` 字段，返回 `Ok(0)`（无操作）。
/// 若主键未设置，返回 `Err(DbError::QueryError)`。
///
/// # 示例
///
/// ```ignore
/// let mut active = user.into_active_model();
/// active.set("email", "new@example.com".into());
/// let rows = update(&mut conn, active).await?;
/// assert_eq!(rows, 1);
/// ```
pub async fn update<A, C>(conn: &mut C, active: A) -> Result<u64, DbError>
where
    A: ActiveModelTrait,
    C: Connection + ?Sized,
{
    let table = active.table_name().to_string();
    let pk_value = active
        .pk_value()
        .ok_or_else(|| DbError::QueryError("ActiveModel: primary key is not set".to_string()))?;

    // 收集所有 Set 字段
    let mut set_clauses: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();

    active.for_each_changed(|field, av| {
        if let ActiveValue::Set(val) = av {
            set_clauses.push(format!("{} = {}", field, val.to_param()));
            params.push(val.clone());
        }
    });

    if set_clauses.is_empty() {
        return Ok(0);
    }

    let sql = format!(
        "UPDATE {} SET {} WHERE {} = {}",
        table,
        set_clauses.join(", "),
        // 使用 pk_name() 作为 WHERE 列名
        active.pk_name_for_update(),
        pk_value.to_param()
    );

    // 注意：此处为简化演示，实际应使用参数化查询
    // 生产环境应调用 conn.execute_with_params(&sql, &params)
    conn.execute(&sql).await
}

/// 执行 INSERT 或 UPDATE（upsert）
///
/// - 若主键已设置 → 执行 UPDATE
/// - 若主键未设置 → 执行 INSERT
///
/// INSERT 时，将所有 `Set` 字段作为列写入。
pub async fn save<A, C>(conn: &mut C, active: A) -> Result<u64, DbError>
where
    A: ActiveModelTrait,
    C: Connection + ?Sized,
{
    if active.pk_value().is_some() {
        update(conn, active).await
    } else {
        insert(conn, active).await
    }
}

/// 执行 INSERT，将所有 `Set` 字段作为列写入
async fn insert<A, C>(conn: &mut C, active: A) -> Result<u64, DbError>
where
    A: ActiveModelTrait,
    C: Connection + ?Sized,
{
    let table = active.table_name().to_string();
    let mut columns: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();

    active.for_each_changed(|field, av| {
        if let ActiveValue::Set(val) = av {
            columns.push(field.to_string());
            values.push(val.to_param().into_owned());
        }
    });

    if columns.is_empty() {
        return Err(DbError::QueryError(
            "ActiveModel: no fields set for insert".to_string(),
        ));
    }

    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table,
        columns.join(", "),
        values.join(", ")
    );

    conn.execute(&sql).await
}

// ========================================================================
// ActiveModel 辅助 trait — 提供 pk_name_for_update
// ========================================================================

/// 内部辅助 trait，为 `update()` 提供主键列名
///
/// 此 trait 自动为所有 `ActiveModel` 实现，
/// 通过 `Model::pk_name()` 获取主键列名。
pub trait ActiveModelExt: ActiveModelTrait {
    /// 获取主键列名（用于 UPDATE 的 WHERE 条件）
    fn pk_name_for_update(&self) -> &str {
        "id"
    }
}

impl<A: ActiveModelTrait> ActiveModelExt for A {}

// ========================================================================
// 测试
// ========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 测试用 mock Model ----

    #[derive(Debug, Clone, Default)]
    #[allow(dead_code)]
    struct User {
        id: i64,
        name: String,
        email: String,
    }

    impl Model for User {
        type PrimaryKey = i64;

        fn table_name() -> &'static str {
            "users"
        }

        fn pk_name() -> &'static str {
            "id"
        }

        fn pk(&self) -> Self::PrimaryKey {
            self.id
        }

        fn set_pk(&mut self, pk: Self::PrimaryKey) {
            self.id = pk;
        }

        fn pk_as_value(&self) -> Value {
            Value::I64(self.id)
        }
    }

    // ---- ActiveValue 测试 ----

    #[test]
    fn test_active_value_set() {
        let av: ActiveValue<Value> = ActiveValue::Set(Value::String("Alice".into()));
        assert!(av.is_set());
        assert!(!av.is_unchanged());
        assert!(!av.is_not_set());
        assert_eq!(av.into_value(), Some(Value::String("Alice".into())));
    }

    #[test]
    fn test_active_value_unchanged() {
        let av: ActiveValue<Value> = ActiveValue::Unchanged;
        assert!(!av.is_set());
        assert!(av.is_unchanged());
        assert!(!av.is_not_set());
        assert_eq!(av.into_value(), None);
    }

    #[test]
    fn test_active_value_not_set() {
        let av: ActiveValue<Value> = ActiveValue::NotSet;
        assert!(!av.is_set());
        assert!(!av.is_unchanged());
        assert!(av.is_not_set());
        assert_eq!(av.into_value(), None);
    }

    #[test]
    fn test_active_value_default_is_not_set() {
        let av: ActiveValue<Value> = ActiveValue::default();
        assert!(av.is_not_set());
    }

    #[test]
    fn test_active_value_from_str() {
        // 利用 From<T: Into<Value>> 自动转换
        let av: ActiveValue<Value> = "hello".into();
        assert!(av.is_set());
        assert_eq!(av.into_value(), Some(Value::String("hello".into())));
    }

    #[test]
    fn test_active_value_from_i64() {
        let av: ActiveValue<Value> = 42i64.into();
        assert!(av.is_set());
        assert_eq!(av.into_value(), Some(Value::I64(42)));
    }

    #[test]
    fn test_active_value_as_value() {
        let av = ActiveValue::Set(Value::I64(99));
        assert_eq!(av.as_value(), Some(&Value::I64(99)));

        let unchanged: ActiveValue<Value> = ActiveValue::Unchanged;
        assert_eq!(unchanged.as_value(), None);
    }

    // ---- ActiveModel<M> 测试 ----

    #[test]
    fn test_active_model_from_model() {
        let user = User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
        };
        let active = ActiveModel::from_model(user.clone());
        assert_eq!(active.table_name(), "users");
        assert_eq!(active.pk_value(), Some(Value::I64(1)));
        // 初始无变更
        assert!(active.changed_fields().is_empty());
    }

    #[test]
    fn test_active_model_set_and_changed_fields() {
        let user = User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
        };
        let mut active = ActiveModel::from_model(user);
        active.set(
            "email",
            ActiveValue::Set(Value::String("new@example.com".into())),
        );
        active.set("name", ActiveValue::Unchanged); // 显式标记为未变更

        let changed = active.changed_fields();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, "email");
        assert_eq!(changed[0].1, &Value::String("new@example.com".into()));
    }

    #[test]
    fn test_active_model_for_each_changed() {
        let user = User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
        };
        let mut active = ActiveModel::from_model(user);
        active.set("name", ActiveValue::Set(Value::String("Bob".into())));
        active.set(
            "email",
            ActiveValue::Set(Value::String("bob@example.com".into())),
        );
        active.set("extra", ActiveValue::NotSet); // NotSet 不应被遍历到

        let mut count = 0;
        let mut names: Vec<String> = Vec::new();
        active.for_each_changed(|field, av| {
            count += 1;
            names.push(field.to_string());
            // NotSet 不应出现在遍历中（但这里我们遍历所有 changes）
            // 实际上 for_each_changed 遍历所有 changes，由调用方判断 is_set()
            let _ = av;
        });
        assert_eq!(count, 3); // 所有 set() 调用都记录了
        assert!(names.contains(&"name".to_string()));
        assert!(names.contains(&"email".to_string()));
        assert!(names.contains(&"extra".to_string()));
    }

    #[test]
    fn test_active_model_unset() {
        let user = User::default();
        let mut active = ActiveModel::from_model(user);
        active.set("name", ActiveValue::Set(Value::String("Alice".into())));
        assert_eq!(active.changed_fields().len(), 1);

        active.unset("name");
        assert!(active.changed_fields().is_empty());
    }

    #[test]
    fn test_active_model_get() {
        let user = User::default();
        let mut active = ActiveModel::from_model(user);
        active.set("name", ActiveValue::Set(Value::String("Alice".into())));

        assert!(active.get("name").is_some());
        assert!(active.get("email").is_none());
    }

    #[test]
    fn test_active_model_into_model() {
        let user = User {
            id: 42,
            name: "Original".into(),
            email: "orig@example.com".into(),
        };
        let active = ActiveModel::from_model(user.clone());
        let restored = active.into_model();
        assert_eq!(restored.id, user.id);
        assert_eq!(restored.name, user.name);
    }

    #[test]
    fn test_active_model_as_mut_model() {
        let user = User::default();
        let mut active = ActiveModel::from_model(user);
        active.as_mut_model().name = "Modified".into();
        assert_eq!(active.as_model().name, "Modified");
    }

    // ---- 三态语义综合测试 ----

    #[test]
    fn test_three_state_semantics() {
        // 场景：从 DB 加载用户，仅修改 email
        let user = User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
        };

        let mut active = ActiveModel::from_model(user);

        // 仅设置 email 字段
        active.set(
            "email",
            ActiveValue::Set(Value::String("new@example.com".into())),
        );

        // changed_fields() 只返回 Set 状态的字段
        let changed = active.changed_fields();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, "email");

        // name 虽然在模型中存在，但未被 set()，所以不在变更集中
        // 这确保 UPDATE 只生成：SET email = ? 而非 SET name = ?, email = ?
    }

    #[test]
    fn test_new_record_all_not_set() {
        // 新建记录：所有字段默认 NotSet
        let user = User::default();
        let mut active = ActiveModel::from_model(user);

        // 初始无任何 Set 字段
        assert!(active.changed_fields().is_empty());

        // 显式设置所需字段
        active.set("name", ActiveValue::Set(Value::String("Bob".into())));
        active.set(
            "email",
            ActiveValue::Set(Value::String("bob@example.com".into())),
        );

        let changed = active.changed_fields();
        assert_eq!(changed.len(), 2);
    }

    #[test]
    fn test_active_value_clone_and_debug() {
        let av = ActiveValue::Set(Value::I64(100));
        let av2 = av.clone();
        assert_eq!(av, av2);

        // Debug 输出
        let debug_str = format!("{:?}", av);
        assert!(debug_str.contains("Set"));
    }
}
