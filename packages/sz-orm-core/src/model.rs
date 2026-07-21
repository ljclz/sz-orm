//! 模型抽象层
//!
//! 提供核心 `Model` trait 及相关类型

use crate::async_trait;
use crate::value::Value;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

/// 所有 ORM 模型必须实现的核心 trait
pub trait Model: Send + Sync + Sized + 'static {
    /// 主键类型
    type PrimaryKey: Send + Sync + fmt::Debug + fmt::Display + Clone + Default;

    /// 获取该模型对应的表名
    fn table_name() -> &'static str;

    /// 获取主键列名（默认 `id`）
    fn pk_name() -> &'static str {
        "id"
    }

    /// 获取当前实例的主键值
    fn pk(&self) -> Self::PrimaryKey;

    /// 设置当前实例的主键值
    fn set_pk(&mut self, pk: Self::PrimaryKey);

    /// 根据关系名推导外键名（默认 `<relation>_id`）
    fn foreign_key(relation: &str) -> String {
        format!("{}_id", relation.to_lowercase())
    }

    /// 获取自动时间戳字段配置
    fn timestamp_fields() -> Option<TimestampFields> {
        None
    }

    /// 获取软删除字段名
    fn soft_delete_field() -> Option<&'static str> {
        None
    }
}

/// 时间戳字段配置
#[derive(Debug, Clone, Default)]
pub struct TimestampFields {
    /// created_at 字段名
    pub created_at: Option<&'static str>,
    /// updated_at 字段名
    pub updated_at: Option<&'static str>,
    /// 插入时是否自动设置时间戳
    pub auto_now_insert: bool,
    /// 更新时是否自动刷新时间戳
    pub auto_now_update: bool,
}

impl TimestampFields {
    pub fn new(created_at: Option<&'static str>, updated_at: Option<&'static str>) -> Self {
        Self {
            created_at,
            updated_at,
            auto_now_insert: created_at.is_some(),
            auto_now_update: updated_at.is_some(),
        }
    }

    pub fn with_both(created_at: &'static str, updated_at: &'static str) -> Self {
        Self {
            created_at: Some(created_at),
            updated_at: Some(updated_at),
            auto_now_insert: true,
            auto_now_update: true,
        }
    }
}

/// 模型间的关系描述
#[derive(Debug, Clone)]
pub enum Relation {
    /// 多对一关系（如 Order 属于 User）
    BelongsTo(BelongsTo),
    /// 一对多关系（如 User 有多个 Order）
    HasMany(HasMany),
    /// 一对一关系（如 User 有一个 Profile）
    HasOne(HasOne),
    /// 多对多关系（通过中间表，如 User 与 Role）
    BelongsToMany(BelongsToMany),
    /// 多态一对多（如 Comment 可关联 Post / Video / Image 等多种父模型）
    /// 子表通过 morph_type_column + morph_id_column 反向定位父模型
    MorphMany(MorphMany),
    /// 多态反向：当前模型可被多种父模型拥有（当前模型持有 morph_type + morph_id 两列）
    MorphTo(MorphTo),
}

/// 多对一关系配置
#[derive(Debug, Clone)]
pub struct BelongsTo {
    pub foreign_key: String,
    pub parent_model: String,
    pub parent_pk: String,
}

/// 一对多关系配置
#[derive(Debug, Clone)]
pub struct HasMany {
    pub foreign_key: String,
    pub child_model: String,
    pub child_pk: String,
}

/// 一对一关系配置
#[derive(Debug, Clone)]
pub struct HasOne {
    pub foreign_key: String,
    pub child_model: String,
    pub child_pk: String,
}

/// 多对多关系配置
///
/// 关联语义：
/// - `junction_table`：中间表名（如 `user_roles`）
/// - `foreign_key`：中间表中指向当前模型主键的列名（如 `user_id`）
/// - `other_key`：中间表中指向目标模型主键的列名（如 `role_id`）
/// - `target_model`：目标表名（如 `roles`）
/// - `target_pk`：目标表的主键列名（如 `id`），用于 JOIN 条件 `t.{target_pk} = j.{other_key}`
#[derive(Debug, Clone)]
pub struct BelongsToMany {
    pub junction_table: String,
    pub foreign_key: String,
    pub other_key: String,
    pub target_model: String,
    pub target_pk: String,
}

/// 多态一对多配置（父模型侧）
///
/// 例：Post has many Comment，Comment 表中有 `commentable_type`（值为 "Post"）和 `commentable_id` 两列。
/// 加载 Post.comments 时：`SELECT * FROM comments WHERE commentable_type = 'Post' AND commentable_id = ?`
#[derive(Debug, Clone)]
pub struct MorphMany {
    /// 子模型表名（如 "comments"）
    pub child_model: String,
    /// 子表中标识父类型的列名（如 "commentable_type"）
    pub morph_type_column: String,
    /// 子表中标识父主键的列名（如 "commentable_id"）
    pub morph_id_column: String,
    /// 父模型类型标识字符串（如 "Post"）
    pub morph_type_value: String,
}

/// 多态反向配置（子模型侧）
///
/// 例：Comment 属于 Post 或 Video，Comment 表中有 `commentable_type` + `commentable_id`。
/// 加载 Comment.commentable 时，根据 commentable_type 路由到不同表。
#[derive(Debug, Clone)]
pub struct MorphTo {
    /// 当前模型中标识父类型的列名（如 "commentable_type"）
    pub morph_type_column: String,
    /// 当前模型中标识父主键的列名（如 "commentable_id"）
    pub morph_id_column: String,
}

/// 支持关系加载的模型 trait（ActiveRecord 模式）
#[async_trait]
pub trait ActiveRecord: Model + ModelExt + RelationLoader + Clone + Send + Sync {
    /// 预加载指定关系
    /// 用法：`user.with("orders").with("profile").load(&mut conn).await`
    fn with(self, relation: &str) -> WithRelation<Self> {
        WithRelation {
            model: self,
            relations: vec![relation.to_string()],
        }
    }

    /// 一次性预加载多个关系
    fn with_all(self, relations: Vec<&str>) -> WithRelation<Self> {
        WithRelation {
            model: self,
            relations: relations.into_iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// 关系预加载构造器
pub struct WithRelation<M: Model + ModelExt + RelationLoader> {
    model: M,
    relations: Vec<String>,
}

/// 转义 SQL 字符串字面量中的特殊字符（用于内嵌值场景）
///
/// 将单引号 `'` 替换为 `''`，将反斜杠 `\` 替换为 `\\`。
/// 该函数仅对需要内嵌到 SQL 字符串字面量中的值使用，
/// 不要用于标识符（表名/列名）的转义。
fn escape_sql_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out
}

/// 将主键值转换为安全的 SQL 字面量
///
/// - 纯数字（i64/u64/f64 可解析）→ 不加引号，直接返回
/// - 其他字符串 → 加单引号并转义内部特殊字符，防止 SQL 注入
fn pk_to_sql_string(pk: &dyn std::fmt::Display) -> String {
    let s = pk.to_string();
    if s.parse::<i64>().is_ok() || s.parse::<u64>().is_ok() || s.parse::<f64>().is_ok() {
        s
    } else {
        format!("'{}'", escape_sql_value(&s))
    }
}

/// 将任意字符串值转换为安全的 SQL 字符串字面量
///
/// 与 `pk_to_sql_string` 不同，本函数始终用单引号包裹并转义，
/// 适用于字符串类型的外键值等。
fn value_to_sql_string(s: &str) -> String {
    format!("'{}'", escape_sql_value(s))
}

/// 校验 SQL 标识符（表名/列名）是否合法
///
/// 合法标识符规则：
/// - 非空
/// - 仅包含字母、数字、下划线
/// - 首字符为字母或下划线
/// - 长度 ≤ 64（与大多数数据库一致）
///
/// 用于防止 MorphTo 关系加载中 morph_type_value 作为表名拼接时的 SQL 注入。
fn is_valid_sql_identifier(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl<M: Model + ModelExt + RelationLoader> WithRelation<M> {
    /// 追加一个待加载的关系
    pub fn with(mut self, relation: &str) -> Self {
        self.relations.push(relation.to_string());
        self
    }

    /// 加载所有指定关系并返回填充后的模型
    /// 加载结果通过 `set_relation_data` 写回模型
    pub async fn load<C>(self, conn: &mut C) -> Result<M, RelationError>
    where
        C: crate::pool::Connection + ?Sized,
    {
        let mut model = self.model;
        let relations_map = M::relations();

        for rel_name in &self.relations {
            let relation = relations_map
                .get(rel_name.as_str())
                .ok_or_else(|| RelationError::RelationNotFound(rel_name.clone()))?;

            match relation {
                Relation::HasMany(config) => {
                    let pk = model.pk();
                    let pk_str = pk_to_sql_string(&pk);
                    let sql = format!(
                        "SELECT * FROM {} WHERE {} = {}",
                        config.child_model, config.foreign_key, pk_str
                    );
                    let rows = conn
                        .query(&sql)
                        .await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
                Relation::HasOne(config) => {
                    let pk = model.pk();
                    let pk_str = pk_to_sql_string(&pk);
                    let sql = format!(
                        "SELECT * FROM {} WHERE {} = {}",
                        config.child_model, config.foreign_key, pk_str
                    );
                    let rows = conn
                        .query(&sql)
                        .await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
                Relation::BelongsTo(config) => {
                    let fk_value = model.get_relation_fk_value(&config.foreign_key);
                    let sql = format!(
                        "SELECT * FROM {} WHERE {} = {}",
                        config.parent_model,
                        config.parent_pk,
                        pk_to_sql_string(&fk_value)
                    );
                    let rows = conn
                        .query(&sql)
                        .await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
                Relation::BelongsToMany(config) => {
                    let pk = model.pk();
                    let pk_str = pk_to_sql_string(&pk);
                    // JOIN 条件：目标表 t 的主键 = 中间表 j 的 other_key
                    // 过滤条件：中间表 j 的 foreign_key = 当前模型主键
                    let sql = format!(
                        "SELECT t.* FROM {} t INNER JOIN {} j ON t.{} = j.{} WHERE j.{} = {}",
                        config.target_model,
                        config.junction_table,
                        config.target_pk,
                        config.other_key,
                        config.foreign_key,
                        pk_str
                    );
                    let rows = conn
                        .query(&sql)
                        .await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
                Relation::MorphMany(config) => {
                    let pk = model.pk();
                    let pk_str = pk_to_sql_string(&pk);
                    // SELECT * FROM comments WHERE commentable_type = 'Post' AND commentable_id = <pk>
                    let sql = format!(
                        "SELECT * FROM {} WHERE {} = {} AND {} = {}",
                        config.child_model,
                        config.morph_type_column,
                        value_to_sql_string(&config.morph_type_value),
                        config.morph_id_column,
                        pk_str
                    );
                    let rows = conn
                        .query(&sql)
                        .await
                        .map_err(|e| RelationError::QueryError(e.to_string()))?;
                    model.set_relation_data(rel_name, rows_to_values(rows));
                }
                Relation::MorphTo(config) => {
                    // 根据当前模型持有的 morph_type_column 值路由到不同表
                    // 实现侧需通过 get_relation_fk_value 提供两个值：type 与 id
                    // 为保持与 RelationLoader 接口兼容，这里采用约定：
                    //   get_relation_fk_value("<morph_type_column>") 返回 type 字符串
                    //   get_relation_fk_value("<morph_id_column>")   返回 id 字符串
                    let morph_type_value = model.get_relation_fk_value(&config.morph_type_column);
                    let morph_id_value = model.get_relation_fk_value(&config.morph_id_column);
                    if morph_type_value.is_empty() || morph_id_value.is_empty() {
                        // 无父模型关联（morph_type 为空），置空数组
                        model.set_relation_data(rel_name, Value::Array(vec![]));
                    } else {
                        // 约定：morph_type_value 即为目标表名（Post → "posts"），由调用方在 get_relation_fk_value 中映射
                        // C-2 修复：morph_type_value 作为表名拼接前必须校验为合法标识符，防止 SQL 注入
                        if !is_valid_sql_identifier(&morph_type_value) {
                            return Err(RelationError::QueryError(format!(
                                "invalid morph_type_value (not a valid SQL identifier): {}",
                                morph_type_value
                            )));
                        }
                        let sql = format!(
                            "SELECT * FROM {} WHERE id = {}",
                            morph_type_value,
                            pk_to_sql_string(&morph_id_value)
                        );
                        let rows = conn
                            .query(&sql)
                            .await
                            .map_err(|e| RelationError::QueryError(e.to_string()))?;
                        model.set_relation_data(rel_name, rows_to_values(rows));
                    }
                }
            }
        }

        Ok(model)
    }
}

/// 将查询结果行转换为 `Vec<HashMap<String, Value>>` 以便存入关系字段
pub fn rows_to_values(rows: Vec<HashMap<String, Value>>) -> Value {
    if rows.is_empty() {
        return Value::Array(vec![]);
    }
    let items: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let mut map = HashMap::new();
            for (k, v) in row {
                map.insert(k, v);
            }
            Value::from_map(map)
        })
        .collect();
    Value::Array(items)
}

/// 关系操作错误类型
#[derive(Error, Debug, Clone)]
pub enum RelationError {
    #[error("Relation '{0}' not found in model relations")]
    RelationNotFound(String),

    #[error("Query error during relation loading: {0}")]
    QueryError(String),

    #[error("Relation data not loaded. Call .with(\"{0}\") before accessing.")]
    NotLoaded(String),
}

/// 可存储已加载关系数据的模型 trait
pub trait RelationLoader: Model {
    /// 获取已加载的关系数据
    fn get_relation(&self, name: &str) -> Option<&Value>;

    /// 写入已加载的关系数据
    fn set_relation_data(&mut self, name: &str, data: Value);

    /// 获取关系对应的外键值
    fn get_relation_fk_value(&self, fk_name: &str) -> String;
}

/// `ModelExt` 的关系访问扩展方法
pub trait RelationAccess: ModelExt {
    /// 获取一对多关系数据（必须先调用 `.with(name)` 加载）
    fn get_has_many(&self, name: &str) -> Result<Vec<HashMap<String, Value>>, RelationError>
    where
        Self: RelationLoader,
    {
        let data = self
            .get_relation(name)
            .ok_or_else(|| RelationError::NotLoaded(name.to_string()))?;
        match data {
            Value::Array(items) => {
                let result: Vec<HashMap<String, Value>> = items
                    .iter()
                    .filter_map(|v| match v {
                        Value::Object(map) => Some(map.clone()),
                        _ => None,
                    })
                    .collect();
                Ok(result)
            }
            _ => Ok(vec![]),
        }
    }

    /// 获取一对一或多对一关系数据（必须先加载，返回 0 或 1 行）
    fn get_has_one(&self, name: &str) -> Result<Option<HashMap<String, Value>>, RelationError>
    where
        Self: RelationLoader,
    {
        let data = self
            .get_relation(name)
            .ok_or_else(|| RelationError::NotLoaded(name.to_string()))?;
        match data {
            Value::Array(items) => {
                if items.is_empty() {
                    Ok(None)
                } else {
                    match &items[0] {
                        Value::Object(map) => Ok(Some(map.clone())),
                        _ => Ok(None),
                    }
                }
            }
            _ => Ok(None),
        }
    }

    /// 获取多对多关系数据（必须先加载）
    fn get_belongs_to_many(&self, name: &str) -> Result<Vec<HashMap<String, Value>>, RelationError>
    where
        Self: RelationLoader,
    {
        self.get_has_many(name)
    }

    /// 获取多态一对多关系数据（必须先加载）
    /// 与 has_many 行为一致，返回多行
    fn get_morph_many(&self, name: &str) -> Result<Vec<HashMap<String, Value>>, RelationError>
    where
        Self: RelationLoader,
    {
        self.get_has_many(name)
    }

    /// 获取多态反向关系数据（必须先加载）
    /// 与 has_one 行为一致，返回 0 或 1 行
    fn get_morph_to(&self, name: &str) -> Result<Option<HashMap<String, Value>>, RelationError>
    where
        Self: RelationLoader,
    {
        self.get_has_one(name)
    }
}

/// 查询结果过滤作用域
pub trait Scope: Send + Sync {
    /// 将作用域应用到查询构造器
    fn apply<M: Model>(&self, query: &mut QueryBuilderWrapper<M>);
}

/// 查询构造器包装类型，用于挂载作用域
pub struct QueryBuilderWrapper<'a, M: Model> {
    pub builder: &'a mut dyn QueryBuilderExt<Model = M>,
}

pub trait QueryBuilderExt: Send + Sync {
    type Model: Model;

    fn and_where(&mut self, condition: &str);
    fn or_where(&mut self, condition: &str);
}

/// 模型扩展 trait，提供额外功能
pub trait ModelExt: Model {
    /// 获取 SELECT 时使用的所有列
    fn columns() -> Vec<&'static str>;

    /// 获取可批量赋值的列（INSERT/UPDATE）
    fn fillable() -> Vec<&'static str>;

    /// 获取受保护列（不可批量赋值）
    fn guarded() -> Vec<&'static str> {
        vec![Self::pk_name()]
    }

    /// 获取隐藏列（不参与序列化）
    fn hidden() -> Vec<&'static str> {
        vec![]
    }

    /// 获取可见列（参与序列化）
    fn visible() -> Vec<&'static str> {
        vec![]
    }

    /// 获取类型转换映射（列名 -> 类型字符串）
    fn casts() -> std::collections::HashMap<&'static str, &'static str> {
        std::collections::HashMap::new()
    }

    /// 获取日期列
    fn dates() -> Vec<&'static str> {
        vec![]
    }

    /// 获取指定字段的日期格式
    fn date_format(_field: &str) -> Option<&'static str> {
        None
    }

    /// 获取关系映射
    fn relations() -> std::collections::HashMap<&'static str, Relation> {
        std::collections::HashMap::new()
    }

    /// 将模型转换为值映射
    fn to_value(&self) -> std::collections::HashMap<String, Value> {
        let mut map = std::collections::HashMap::new();
        for col in Self::columns() {
            if let Some(val) = Self::get_column_value(self, col) {
                // 跳过 hidden 字段
                if !Self::hidden().contains(&col) {
                    map.insert(col.to_string(), val);
                }
            }
        }
        map
    }

    /// 获取指定列的值（须由实现重写）
    fn get_column_value(&self, _column: &str) -> Option<Value> {
        None
    }

    /// 从值映射还原模型（须由实现重写）
    #[allow(clippy::wrong_self_convention)]
    fn from_value(&mut self, _map: std::collections::HashMap<String, Value>) {
        // 默认空实现，业务模型须重写
    }

    /// 批量赋值：只填充 fillable 字段（过滤掉 guarded 字段）
    fn fill(&mut self, mut map: std::collections::HashMap<String, Value>) {
        let guarded = Self::guarded();
        let fillable = Self::fillable();
        // 移除 guarded 字段
        for g in &guarded {
            map.remove(*g);
        }
        // 如果 fillable 非空，只保留 fillable 字段
        if !fillable.is_empty() {
            map.retain(|k, _| fillable.contains(&k.as_str()));
        }
        self.from_value(map);
    }

    /// 序列化为 JSON
    fn to_json(&self) -> serde_json::Value {
        let map = self.to_value();
        let mut obj = serde_json::Map::new();
        for (k, v) in map {
            obj.insert(k, value_to_json(v));
        }
        serde_json::Value::Object(obj)
    }
}

/// 将 Value 转换为 serde_json::Value（递归处理 Array）
pub fn value_to_json(v: Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::I8(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::I16(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::I32(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::I64(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::U8(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::U16(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::U32(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::U64(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        Value::F32(n) => serde_json::Number::from_f64(n as f64)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::F64(n) => serde_json::Number::from_f64(n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s),
        Value::Bytes(b) => {
            serde_json::Value::String(b.iter().map(|byte| format!("{:02x}", byte)).collect())
        }
        Value::Uuid(s) | Value::Date(s) | Value::DateTime(s) | Value::Time(s) | Value::Json(s) => {
            serde_json::Value::String(s)
        }
        Value::Array(arr) => serde_json::Value::Array(arr.into_iter().map(value_to_json).collect()),
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k, value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_fields() {
        let ts = TimestampFields::new(Some("created_at"), Some("updated_at"));
        assert!(ts.created_at.is_some());
        assert!(ts.updated_at.is_some());

        let ts2 = TimestampFields::with_both("created_at", "updated_at");
        assert!(ts2.auto_now_insert);
        assert!(ts2.auto_now_update);
    }

    #[test]
    fn test_foreign_key() {
        struct TestModel;
        impl Model for TestModel {
            type PrimaryKey = i64;

            fn table_name() -> &'static str {
                "test_models"
            }

            fn pk(&self) -> Self::PrimaryKey {
                1
            }

            fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
        }

        let fk = TestModel::foreign_key("user");
        assert_eq!(fk, "user_id");

        let fk = TestModel::foreign_key("Role");
        assert_eq!(fk, "role_id");
    }

    #[test]
    fn test_relation_documentation() {
        // 验证 Relation 枚举的语义
        let belongs_to = Relation::BelongsTo(BelongsTo {
            foreign_key: "user_id".to_string(),
            parent_model: "User".to_string(),
            parent_pk: "id".to_string(),
        });
        if let Relation::BelongsTo(ref bt) = belongs_to {
            assert_eq!(bt.parent_model, "User");
        }

        let has_one = Relation::HasOne(HasOne {
            foreign_key: "user_id".to_string(),
            child_model: "Profile".to_string(),
            child_pk: "id".to_string(),
        });
        if let Relation::HasOne(ref ho) = has_one {
            assert_eq!(ho.child_model, "Profile");
        }

        let has_many = Relation::HasMany(HasMany {
            foreign_key: "user_id".to_string(),
            child_model: "Order".to_string(),
            child_pk: "id".to_string(),
        });
        if let Relation::HasMany(ref hm) = has_many {
            assert_eq!(hm.child_model, "Order");
        }

        let many_to_many = Relation::BelongsToMany(BelongsToMany {
            junction_table: "user_role".to_string(),
            foreign_key: "user_id".to_string(),
            other_key: "role_id".to_string(),
            target_model: "Role".to_string(),
            target_pk: "id".to_string(),
        });
        if let Relation::BelongsToMany(ref mtm) = many_to_many {
            assert_eq!(mtm.junction_table, "user_role");
            assert_eq!(mtm.target_pk, "id");
        }
    }

    #[test]
    fn test_model_ext_implementation() {
        /// 测试用的完整 ModelExt 实现
        struct UserModel {
            id: i64,
            name: String,
            email: String,
            password: String, // hidden
        }

        impl Model for UserModel {
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

        impl ModelExt for UserModel {
            fn columns() -> Vec<&'static str> {
                vec!["id", "name", "email", "password"]
            }

            fn fillable() -> Vec<&'static str> {
                vec!["name", "email", "password"]
            }

            fn hidden() -> Vec<&'static str> {
                vec!["password"]
            }

            fn get_column_value(&self, column: &str) -> Option<Value> {
                match column {
                    "id" => Some(Value::I64(self.id)),
                    "name" => Some(Value::String(self.name.clone())),
                    "email" => Some(Value::String(self.email.clone())),
                    "password" => Some(Value::String(self.password.clone())),
                    _ => None,
                }
            }

            fn from_value(&mut self, map: std::collections::HashMap<String, Value>) {
                if let Some(Value::I64(id)) = map.get("id") {
                    self.id = *id;
                }
                if let Some(Value::String(name)) = map.get("name") {
                    self.name = name.clone();
                }
                if let Some(Value::String(email)) = map.get("email") {
                    self.email = email.clone();
                }
                if let Some(Value::String(password)) = map.get("password") {
                    self.password = password.clone();
                }
            }
        }

        let user = UserModel {
            id: 1,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            password: "secret".to_string(),
        };

        // 测试 to_value（应该跳过 hidden 字段）
        let values = user.to_value();
        assert!(values.contains_key("name"));
        assert!(values.contains_key("email"));
        // password 是 hidden，不应出现在 to_value 结果中
        assert!(!values.contains_key("password"));

        // 测试 to_json
        let json = user.to_json();
        assert!(json.is_object());
        assert!(json.get("name").is_some());
        assert!(json.get("password").is_none());

        // 测试 fill（应该过滤 guarded 字段）
        let mut user2 = UserModel {
            id: 0,
            name: String::new(),
            email: String::new(),
            password: String::new(),
        };
        let mut fill_data = std::collections::HashMap::new();
        fill_data.insert("id".to_string(), Value::I64(999)); // guarded, 应被过滤
        fill_data.insert("name".to_string(), Value::String("Bob".to_string()));
        fill_data.insert(
            "email".to_string(),
            Value::String("bob@example.com".to_string()),
        );
        fill_data.insert("password".to_string(), Value::String("hashed".to_string()));

        user2.fill(fill_data);
        // id 应保持 0（被过滤）
        assert_eq!(user2.id, 0);
        assert_eq!(user2.name, "Bob");
        assert_eq!(user2.email, "bob@example.com");
    }

    // ============= ActiveRecord 测试 =============

    use crate::pool::Connection;
    use std::pin::Pin;

    /// 模拟数据库连接，用于测试关系加载
    struct MockConnection {
        query_results: HashMap<String, Vec<HashMap<String, Value>>>,
    }

    impl Connection for MockConnection {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<u64, crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(1) })
        }

        fn query<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Vec<HashMap<String, Value>>, crate::DbError>,
                    > + Send
                    + 'a,
            >,
        > {
            let result = self.query_results.get(sql).cloned().unwrap_or_default();
            Box::pin(async move { Ok(result) })
        }

        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }

        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }

        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }

        fn is_connected(&self) -> bool {
            true
        }

        fn ping<'a>(&'a mut self) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }

        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    /// 测试用的 UserModel（带关系支持）
    #[derive(Clone)]
    #[allow(dead_code)]
    struct UserModel {
        id: i64,
        name: String,
        email: String,
        password: String,
        team_id: i64,
        relations: HashMap<String, Value>,
    }

    impl Model for UserModel {
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

    impl ModelExt for UserModel {
        fn columns() -> Vec<&'static str> {
            vec!["id", "name", "email", "team_id"]
        }
        fn fillable() -> Vec<&'static str> {
            vec!["name", "email"]
        }
        fn hidden() -> Vec<&'static str> {
            vec!["password"]
        }
        fn relations() -> HashMap<&'static str, Relation> {
            let mut map = HashMap::new();
            map.insert(
                "orders",
                Relation::HasMany(HasMany {
                    foreign_key: "user_id".to_string(),
                    child_model: "orders".to_string(),
                    child_pk: "id".to_string(),
                }),
            );
            map.insert(
                "profile",
                Relation::HasOne(HasOne {
                    foreign_key: "user_id".to_string(),
                    child_model: "profiles".to_string(),
                    child_pk: "id".to_string(),
                }),
            );
            map.insert(
                "team",
                Relation::BelongsTo(BelongsTo {
                    foreign_key: "team_id".to_string(),
                    parent_model: "teams".to_string(),
                    parent_pk: "id".to_string(),
                }),
            );
            map.insert(
                "roles",
                Relation::BelongsToMany(BelongsToMany {
                    junction_table: "user_roles".to_string(),
                    foreign_key: "user_id".to_string(),
                    other_key: "role_id".to_string(),
                    target_model: "roles".to_string(),
                    target_pk: "id".to_string(),
                }),
            );
            map.insert(
                "comments",
                Relation::MorphMany(MorphMany {
                    child_model: "comments".to_string(),
                    morph_type_column: "commentable_type".to_string(),
                    morph_id_column: "commentable_id".to_string(),
                    morph_type_value: "User".to_string(),
                }),
            );
            map
        }
        fn get_column_value(&self, column: &str) -> Option<Value> {
            match column {
                "id" => Some(Value::I64(self.id)),
                "name" => Some(Value::String(self.name.clone())),
                "email" => Some(Value::String(self.email.clone())),
                "team_id" => Some(Value::I64(self.team_id)),
                _ => None,
            }
        }
        fn from_value(&mut self, map: HashMap<String, Value>) {
            if let Some(Value::I64(id)) = map.get("id") {
                self.id = *id;
            }
            if let Some(Value::String(name)) = map.get("name") {
                self.name = name.clone();
            }
            if let Some(Value::String(email)) = map.get("email") {
                self.email = email.clone();
            }
            if let Some(Value::I64(tid)) = map.get("team_id") {
                self.team_id = *tid;
            }
        }
    }

    impl RelationLoader for UserModel {
        fn get_relation(&self, name: &str) -> Option<&Value> {
            self.relations.get(name)
        }
        fn set_relation_data(&mut self, name: &str, data: Value) {
            self.relations.insert(name.to_string(), data);
        }
        fn get_relation_fk_value(&self, fk_name: &str) -> String {
            match fk_name {
                "user_id" => format!("{}", self.id),
                "team_id" => format!("{}", self.team_id),
                _ => "0".to_string(),
            }
        }
    }

    impl ActiveRecord for UserModel {}
    impl RelationAccess for UserModel {}

    fn make_user() -> UserModel {
        UserModel {
            id: 1,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            password: "secret".to_string(),
            team_id: 10,
            relations: HashMap::new(),
        }
    }

    fn make_order_row(id: i64, user_id: i64, total: &str) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(id));
        row.insert("user_id".to_string(), Value::I64(user_id));
        row.insert("total".to_string(), Value::String(total.to_string()));
        row
    }

    fn make_profile_row(user_id: i64, bio: &str) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(100));
        row.insert("user_id".to_string(), Value::I64(user_id));
        row.insert("bio".to_string(), Value::String(bio.to_string()));
        row
    }

    fn make_team_row(id: i64, name: &str) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(id));
        row.insert("name".to_string(), Value::String(name.to_string()));
        row
    }

    fn make_role_row(id: i64, name: &str) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(id));
        row.insert("name".to_string(), Value::String(name.to_string()));
        row
    }

    #[tokio::test]
    async fn test_active_record_with_has_many() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM orders WHERE user_id = 1".to_string(),
                    vec![
                        make_order_row(1, 1, "99.99"),
                        make_order_row(2, 1, "149.50"),
                    ],
                );
                m
            },
        };

        let user = user.with("orders").load(&mut conn).await.unwrap();
        let data = user.get_relation("orders");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected Array");
        }
    }

    #[tokio::test]
    async fn test_active_record_with_has_one() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM profiles WHERE user_id = 1".to_string(),
                    vec![make_profile_row(1, "Hello world")],
                );
                m
            },
        };

        let user = user.with("profile").load(&mut conn).await.unwrap();
        let data = user.get_relation("profile");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_active_record_with_belongs_to() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM teams WHERE id = 10".to_string(),
                    vec![make_team_row(10, "Engineering")],
                );
                m
            },
        };

        let user = user.with("team").load(&mut conn).await.unwrap();
        let data = user.get_relation("team");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_active_record_with_belongs_to_many() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT t.* FROM roles t INNER JOIN user_roles j ON t.id = j.role_id WHERE j.user_id = 1".to_string(),
                    vec![
                        make_role_row(1, "admin"),
                        make_role_row(2, "editor"),
                    ],
                );
                m
            },
        };

        let user = user.with("roles").load(&mut conn).await.unwrap();
        let data = user.get_relation("roles");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 2);
        }
    }

    #[tokio::test]
    async fn test_active_record_with_all() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM orders WHERE user_id = 1".to_string(),
                    vec![make_order_row(1, 1, "99.99")],
                );
                m.insert(
                    "SELECT * FROM profiles WHERE user_id = 1".to_string(),
                    vec![make_profile_row(1, "Bio")],
                );
                m
            },
        };

        let user = user
            .with_all(vec!["orders", "profile"])
            .load(&mut conn)
            .await
            .unwrap();

        assert!(user.get_relation("orders").is_some());
        assert!(user.get_relation("profile").is_some());
    }

    #[tokio::test]
    async fn test_active_record_relation_not_found() {
        let user = make_user();
        let mut conn = MockConnection {
            query_results: HashMap::new(),
        };

        let result = user.with("nonexistent").load(&mut conn).await;
        assert!(result.is_err());
        match result {
            Err(RelationError::RelationNotFound(name)) => {
                assert_eq!(name, "nonexistent");
            }
            _ => panic!("Expected RelationNotFound"),
        }
    }

    #[test]
    fn test_active_record_not_loaded() {
        let user = make_user();
        let result = user.get_has_many("orders");
        assert!(result.is_err());
        match result {
            Err(RelationError::NotLoaded(name)) => {
                assert_eq!(name, "orders");
            }
            _ => panic!("Expected NotLoaded"),
        }
    }

    #[test]
    fn test_rows_to_values_empty() {
        let rows: Vec<HashMap<String, Value>> = vec![];
        let result = rows_to_values(rows);
        assert_eq!(result, Value::Array(vec![]));
    }

    #[test]
    fn test_rows_to_values_with_data() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("test".to_string()));
        let rows = vec![row];
        let result = rows_to_values(rows);

        match &result {
            Value::Array(items) => {
                assert_eq!(items.len(), 1);
                assert!(items[0].is_object());
            }
            _ => panic!("Expected Array"),
        }
    }

    #[tokio::test]
    async fn test_relation_access_has_many() {
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM orders WHERE user_id = 1".to_string(),
                    vec![
                        make_order_row(1, 1, "99.99"),
                        make_order_row(2, 1, "149.50"),
                    ],
                );
                m
            },
        };

        let user = make_user().with("orders").load(&mut conn).await.unwrap();
        let orders = user.get_has_many("orders").unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(
            orders[0].get("total").unwrap(),
            &Value::String("99.99".to_string())
        );
    }

    #[tokio::test]
    async fn test_relation_access_has_one() {
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM profiles WHERE user_id = 1".to_string(),
                    vec![make_profile_row(1, "My bio")],
                );
                m
            },
        };

        let user = make_user().with("profile").load(&mut conn).await.unwrap();
        let profile = user.get_has_one("profile").unwrap();
        assert!(profile.is_some());
        assert_eq!(
            profile.unwrap().get("bio").unwrap(),
            &Value::String("My bio".to_string())
        );
    }

    #[test]
    fn test_value_object() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), Value::String("value".to_string()));
        let obj = Value::from_map(map);
        assert!(obj.is_object());

        if let Value::Object(m) = &obj {
            assert_eq!(m.get("key").unwrap(), &Value::String("value".to_string()));
        } else {
            panic!("Expected Object");
        }
    }

    // ============= 多态关联（MorphMany / MorphTo）测试 =============

    fn make_comment_row(
        id: i64,
        commentable_type: &str,
        commentable_id: i64,
        body: &str,
    ) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(id));
        row.insert(
            "commentable_type".to_string(),
            Value::String(commentable_type.to_string()),
        );
        row.insert("commentable_id".to_string(), Value::I64(commentable_id));
        row.insert("body".to_string(), Value::String(body.to_string()));
        row
    }

    /// CommentModel：带 MorphTo 关系，演示多态反向关联
    /// comments 表结构：id, commentable_type ('User'/'Post'/'Video'), commentable_id, body
    #[derive(Clone)]
    #[allow(dead_code)]
    struct CommentModel {
        id: i64,
        commentable_type: String,
        commentable_id: i64,
        body: String,
        relations: HashMap<String, Value>,
    }

    impl Model for CommentModel {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "comments"
        }
        fn pk(&self) -> Self::PrimaryKey {
            self.id
        }
        fn set_pk(&mut self, pk: Self::PrimaryKey) {
            self.id = pk;
        }
    }

    impl ModelExt for CommentModel {
        fn columns() -> Vec<&'static str> {
            vec!["id", "commentable_type", "commentable_id", "body"]
        }
        fn fillable() -> Vec<&'static str> {
            vec!["commentable_type", "commentable_id", "body"]
        }
        fn relations() -> HashMap<&'static str, Relation> {
            let mut map = HashMap::new();
            map.insert(
                "commentable",
                Relation::MorphTo(MorphTo {
                    morph_type_column: "commentable_type".to_string(),
                    morph_id_column: "commentable_id".to_string(),
                }),
            );
            map
        }
        fn get_column_value(&self, column: &str) -> Option<Value> {
            match column {
                "id" => Some(Value::I64(self.id)),
                "commentable_type" => Some(Value::String(self.commentable_type.clone())),
                "commentable_id" => Some(Value::I64(self.commentable_id)),
                "body" => Some(Value::String(self.body.clone())),
                _ => None,
            }
        }
        fn from_value(&mut self, map: HashMap<String, Value>) {
            if let Some(Value::I64(id)) = map.get("id") {
                self.id = *id;
            }
            if let Some(Value::String(s)) = map.get("commentable_type") {
                self.commentable_type = s.clone();
            }
            if let Some(Value::I64(n)) = map.get("commentable_id") {
                self.commentable_id = *n;
            }
            if let Some(Value::String(s)) = map.get("body") {
                self.body = s.clone();
            }
        }
    }

    impl RelationLoader for CommentModel {
        fn get_relation(&self, name: &str) -> Option<&Value> {
            self.relations.get(name)
        }
        fn set_relation_data(&mut self, name: &str, data: Value) {
            self.relations.insert(name.to_string(), data);
        }
        fn get_relation_fk_value(&self, fk_name: &str) -> String {
            // MorphTo 约定：
            //  - 当 fk_name == morph_type_column 时，返回目标表名（这里 'User' → 'users'）
            //  - 当 fk_name == morph_id_column  时，返回父模型主键值
            match fk_name {
                "commentable_type" => match self.commentable_type.as_str() {
                    "User" => "users".to_string(),
                    "Post" => "posts".to_string(),
                    "Video" => "videos".to_string(),
                    _ => String::new(),
                },
                "commentable_id" => format!("{}", self.commentable_id),
                _ => "0".to_string(),
            }
        }
    }

    impl ActiveRecord for CommentModel {}
    impl RelationAccess for CommentModel {}

    fn make_comment() -> CommentModel {
        CommentModel {
            id: 50,
            commentable_type: "User".to_string(),
            commentable_id: 1,
            body: "Hello!".to_string(),
            relations: HashMap::new(),
        }
    }

    #[test]
    fn test_morph_many_struct_fields() {
        let m = MorphMany {
            child_model: "comments".to_string(),
            morph_type_column: "commentable_type".to_string(),
            morph_id_column: "commentable_id".to_string(),
            morph_type_value: "Post".to_string(),
        };
        assert_eq!(m.child_model, "comments");
        assert_eq!(m.morph_type_column, "commentable_type");
        assert_eq!(m.morph_id_column, "commentable_id");
        assert_eq!(m.morph_type_value, "Post");
    }

    #[test]
    fn test_morph_to_struct_fields() {
        let m = MorphTo {
            morph_type_column: "commentable_type".to_string(),
            morph_id_column: "commentable_id".to_string(),
        };
        assert_eq!(m.morph_type_column, "commentable_type");
        assert_eq!(m.morph_id_column, "commentable_id");
    }

    #[test]
    fn test_is_valid_sql_identifier_accepts_valid() {
        // 合法标识符
        assert!(is_valid_sql_identifier("users"));
        assert!(is_valid_sql_identifier("UserProfiles"));
        assert!(is_valid_sql_identifier("_private"));
        assert!(is_valid_sql_identifier("table_123"));
        assert!(is_valid_sql_identifier("a"));
    }

    #[test]
    fn test_is_valid_sql_identifier_rejects_invalid() {
        // 空
        assert!(!is_valid_sql_identifier(""));
        // 数字开头
        assert!(!is_valid_sql_identifier("1table"));
        // 包含特殊字符（SQL 注入尝试）
        assert!(!is_valid_sql_identifier("users; DROP TABLE users;--"));
        assert!(!is_valid_sql_identifier("users' OR '1'='1"));
        assert!(!is_valid_sql_identifier("users--"));
        assert!(!is_valid_sql_identifier("users /* comment */"));
        // 包含空格
        assert!(!is_valid_sql_identifier("users table"));
        // 包含点（schema.table 形式）
        assert!(!is_valid_sql_identifier("public.users"));
        // 超长（>64 字符）
        assert!(!is_valid_sql_identifier(&"a".repeat(65)));
        // 中文字符
        assert!(!is_valid_sql_identifier("用户表"));
    }

    #[test]
    fn test_is_valid_sql_identifier_boundary() {
        // 恰好 64 字符（合法）
        assert!(is_valid_sql_identifier(&"a".repeat(64)));
        // 恰好 65 字符（非法）
        assert!(!is_valid_sql_identifier(&"a".repeat(65)));
        // 单个下划线
        assert!(is_valid_sql_identifier("_"));
        // 单个字母
        assert!(is_valid_sql_identifier("x"));
    }

    #[test]
    fn test_relation_enum_has_morph_variants() {
        let morph_many = Relation::MorphMany(MorphMany {
            child_model: "comments".to_string(),
            morph_type_column: "commentable_type".to_string(),
            morph_id_column: "commentable_id".to_string(),
            morph_type_value: "User".to_string(),
        });
        if let Relation::MorphMany(ref m) = morph_many {
            assert_eq!(m.morph_type_value, "User");
        } else {
            panic!("Expected MorphMany");
        }

        let morph_to = Relation::MorphTo(MorphTo {
            morph_type_column: "commentable_type".to_string(),
            morph_id_column: "commentable_id".to_string(),
        });
        if let Relation::MorphTo(ref m) = morph_to {
            assert_eq!(m.morph_type_column, "commentable_type");
        } else {
            panic!("Expected MorphTo");
        }
    }

    #[tokio::test]
    async fn test_active_record_with_morph_many() {
        // Post → comments (morph_type='Post')
        let post = make_user(); // 复用 UserModel 但修改 morph_type_value 需要单独配置
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                // UserModel 中配置的 MorphMany morph_type_value = "User"
                m.insert(
                    "SELECT * FROM comments WHERE commentable_type = 'User' AND commentable_id = 1"
                        .to_string(),
                    vec![
                        make_comment_row(1, "User", 1, "Nice user"),
                        make_comment_row(2, "User", 1, "Cool"),
                    ],
                );
                m
            },
        };

        let user = post.with("comments").load(&mut conn).await.unwrap();
        let data = user.get_relation("comments");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected Array");
        }
    }

    #[tokio::test]
    async fn test_active_record_with_morph_to() {
        let comment = make_comment();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                // CommentModel.commentable 路由到 users 表
                m.insert(
                    "SELECT * FROM users WHERE id = 1".to_string(),
                    vec![make_team_row(1, "Alice")], // 复用 make_team_row 构造一个 id+name 行
                );
                m
            },
        };

        let comment = comment.with("commentable").load(&mut conn).await.unwrap();
        let data = comment.get_relation("commentable");
        assert!(data.is_some());
        if let Some(Value::Array(items)) = data {
            assert_eq!(items.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_active_record_morph_to_empty_type() {
        // morph_type 为空时，应返回空数组而非查询错误
        let mut comment = make_comment();
        comment.commentable_type = String::new(); // 空类型
        let mut conn = MockConnection {
            query_results: HashMap::new(),
        };

        let comment = comment.with("commentable").load(&mut conn).await.unwrap();
        let data = comment.get_relation("commentable").unwrap();
        match data {
            Value::Array(items) => assert!(items.is_empty()),
            _ => panic!("Expected empty Array"),
        }
    }

    #[tokio::test]
    async fn test_relation_access_morph_many() {
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM comments WHERE commentable_type = 'User' AND commentable_id = 1"
                        .to_string(),
                    vec![make_comment_row(10, "User", 1, "via morph many")],
                );
                m
            },
        };

        let user = make_user().with("comments").load(&mut conn).await.unwrap();
        let comments = user.get_morph_many("comments").unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(
            comments[0].get("body").unwrap(),
            &Value::String("via morph many".to_string())
        );
    }

    #[tokio::test]
    async fn test_relation_access_morph_to() {
        let comment = make_comment();
        let mut conn = MockConnection {
            query_results: {
                let mut m = HashMap::new();
                m.insert(
                    "SELECT * FROM users WHERE id = 1".to_string(),
                    vec![make_team_row(1, "Alice")],
                );
                m
            },
        };

        let comment = comment.with("commentable").load(&mut conn).await.unwrap();
        let parent = comment.get_morph_to("commentable").unwrap();
        assert!(parent.is_some());
        assert_eq!(
            parent.unwrap().get("name").unwrap(),
            &Value::String("Alice".to_string())
        );
    }

    #[test]
    fn test_morph_to_not_loaded() {
        let comment = make_comment();
        let result = comment.get_morph_to("commentable");
        assert!(result.is_err());
        match result {
            Err(RelationError::NotLoaded(name)) => assert_eq!(name, "commentable"),
            _ => panic!("Expected NotLoaded"),
        }
    }

    #[test]
    fn test_morph_many_not_loaded() {
        let user = make_user();
        let result = user.get_morph_many("comments");
        assert!(result.is_err());
    }
}
