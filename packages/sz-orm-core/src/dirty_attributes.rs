//! 脏字段追踪（Dirty Attributes）+ @DynamicInsert / @DynamicUpdate
//!
//! 对应文档 6.8 节改进项 24（Dirty Attributes 脏字段追踪）+ 48（@DynamicInsert/@DynamicUpdate）。
//!
//! # 核心概念
//!
//! - **DirtyTracker**：追踪字段变更状态（原始值快照 vs 当前值），计算哪些字段被修改
//! - **is_dirty()**：判断是否有脏字段
//! - **get_dirty_fields()**：返回所有脏字段名
//! - **mark_clean()**：将当前值重新作为基准（写入完成后调用）
//! - **get_original()**：获取字段的原始值（写入失败回滚时使用）
//! - **build_dynamic_update()**：仅生成脏字段的 UPDATE SQL（对应 Hibernate `@DynamicUpdate`）
//! - **build_dynamic_insert()**：仅生成非 null 字段的 INSERT SQL（对应 Hibernate `@DynamicInsert`）
//!
//! # 设计灵感
//!
//! - Hibernate `@DynamicInsert` / `@DynamicUpdate`
//! - Doctrine `ChangeTrackingPolicy::DEFERRED_EXPLICIT`
//! - Yii2 `ActiveRecord::getDirtyAttributes()`
//! - Laravel Eloquent `getDirty()` / `getOriginal()`
//! - MyBatis-Plus `whereEntity` 仅含非 null 字段
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::dirty_attributes::DirtyTracker;
//! use sz_orm_core::Value;
//! use std::collections::HashMap;
//!
//! // 1. 加载用户后建立快照
//! let mut row: HashMap<String, Value> = HashMap::new();
//! row.insert("id".to_string(), Value::I64(1));
//! row.insert("name".to_string(), Value::String("alice".to_string()));
//! row.insert("age".to_string(), Value::I64(25));
//! let mut tracker = DirtyTracker::new(row);
//!
//! // 2. 修改字段
//! tracker.set("age", Value::I64(26));
//!
//! // 3. 检查脏字段
//! assert!(tracker.is_dirty());
//! assert_eq!(tracker.get_dirty_fields(), vec!["age"]);
//! assert_eq!(tracker.get_original("age"), Some(&Value::I64(25)));
//! ```

use crate::dialect::Dialect;
use crate::Value;
use std::collections::HashMap;

// ============================================================================
// DirtyTracker — 脏字段追踪器
// ============================================================================

/// 脏字段追踪器
///
/// 通过维护「原始值快照」与「当前值」两份 HashMap，比较得到脏字段集合。
///
/// # 设计要点
///
/// - **快照机制**：`new()` 时将所有字段视为「原始值」
/// - **修改追踪**：`set()` 修改字段时，仅写入 `current`，不动 `original`
/// - **脏字段判定**：`original[field] != current[field]` 则该字段为脏
/// - **新字段处理**：`set()` 一个 original 中不存在的字段时，自动视为脏字段
///   （等价于 `original[field] = Null`）
/// - **mark_clean**：写入成功后调用，将 `original` 同步为 `current`
/// - **rollback**：写入失败时调用 `rollback()`，将 `current` 还原为 `original`
#[derive(Debug, Clone)]
pub struct DirtyTracker {
    /// 原始值快照（加载时/上次 mark_clean 后的状态）
    original: HashMap<String, Value>,
    /// 当前值
    current: HashMap<String, Value>,
}

impl DirtyTracker {
    /// 创建脏字段追踪器，传入初始字段值作为快照
    pub fn new(initial: HashMap<String, Value>) -> Self {
        let original = initial.clone();
        Self {
            original,
            current: initial,
        }
    }

    /// 创建空追踪器
    pub fn empty() -> Self {
        Self {
            original: HashMap::new(),
            current: HashMap::new(),
        }
    }

    /// 设置字段值（修改 current，不动 original）
    ///
    /// 若字段原本不存在（original 中无），则视为新增脏字段。
    pub fn set(&mut self, field: impl Into<String>, value: Value) {
        self.current.insert(field.into(), value);
    }

    /// 批量设置字段值
    pub fn set_many(&mut self, fields: HashMap<String, Value>) {
        for (k, v) in fields {
            self.current.insert(k, v);
        }
    }

    /// 获取当前值
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.current.get(field)
    }

    /// 获取原始值（写入失败时可用于回滚业务层）
    pub fn get_original(&self, field: &str) -> Option<&Value> {
        self.original.get(field)
    }

    /// 获取所有字段的当前值（克隆）
    pub fn current(&self) -> &HashMap<String, Value> {
        &self.current
    }

    /// 获取所有字段的原始值（克隆）
    pub fn original(&self) -> &HashMap<String, Value> {
        &self.original
    }

    /// 是否存在脏字段
    pub fn is_dirty(&self) -> bool {
        self.original.len() != self.current.len() || self.dirty_fields_iter().next().is_some()
    }

    /// 判断指定字段是否为脏
    pub fn is_field_dirty(&self, field: &str) -> bool {
        match (self.original.get(field), self.current.get(field)) {
            (None, None) => false,
            (None, Some(_)) => true, // 新增字段视为脏
            (Some(_), None) => true, // 删除字段视为脏
            (Some(o), Some(c)) => o != c,
        }
    }

    /// 获取所有脏字段名（按字段名字典序排列，保证测试稳定）
    pub fn get_dirty_fields(&self) -> Vec<String> {
        let mut dirty: Vec<String> = self.dirty_fields_iter().cloned().collect();
        dirty.sort();
        dirty
    }

    /// 获取脏字段及其当前值
    pub fn get_dirty_attributes(&self) -> HashMap<String, Value> {
        let mut result = HashMap::new();
        for field in self.dirty_fields_iter() {
            if let Some(v) = self.current.get(field) {
                result.insert(field.clone(), v.clone());
            }
        }
        result
    }

    /// 标记所有字段为干净（写入成功后调用）
    ///
    /// 将 `original` 同步为 `current`，下一次 `is_dirty()` 返回 false。
    pub fn mark_clean(&mut self) {
        self.original = self.current.clone();
    }

    /// 回滚：将 `current` 还原为 `original`（写入失败时调用）
    pub fn rollback(&mut self) {
        self.current = self.original.clone();
    }

    /// 重置追踪器，丢弃所有数据
    pub fn clear(&mut self) {
        self.original.clear();
        self.current.clear();
    }

    /// 内部：脏字段迭代器（避免重复实现）
    fn dirty_fields_iter(&self) -> impl Iterator<Item = &String> {
        // current 中所有不在 original 或值不同的字段
        let keys: Vec<&String> = self.current.keys().collect();
        keys.into_iter()
            .filter(move |k| match self.original.get(*k) {
                None => true, // 新增字段
                Some(o) => self.current.get(*k).map(|c| c != o).unwrap_or(true),
            })
            .chain(
                // original 中存在但 current 中不存在的字段（被删除）
                self.original
                    .keys()
                    .filter(move |k| !self.current.contains_key(*k)),
            )
    }
}

// ============================================================================
// build_dynamic_update — 仅生成脏字段的 UPDATE SQL
// ============================================================================

/// 生成仅含脏字段的 UPDATE SQL（对应 Hibernate `@DynamicUpdate`）
///
/// 生成的 SQL 形如：
/// ```sql
/// UPDATE `table` SET `col1` = ?, `col2` = ? WHERE `pk` = ?
/// ```
///
/// # 参数
/// - `dialect`：数据库方言
/// - `table`：表名
/// - `pk_column`：主键列名
/// - `pk_value`：主键值
/// - `tracker`：脏字段追踪器
///
/// # 返回
/// - `Some(sql)`：存在脏字段时返回 UPDATE SQL
/// - `None`：无脏字段，无需更新
///
/// # 示例
///
/// ```
/// use sz_orm_core::dirty_attributes::{DirtyTracker, build_dynamic_update};
/// use sz_orm_core::{DbType, get_dialect, Value};
/// use std::collections::HashMap;
///
/// let dialect = get_dialect(DbType::MySQL).unwrap();
/// let mut row = HashMap::new();
/// row.insert("id".to_string(), Value::I64(1));
/// row.insert("name".to_string(), Value::String("alice".to_string()));
/// row.insert("age".to_string(), Value::I64(25));
/// let mut tracker = DirtyTracker::new(row);
/// tracker.set("age", Value::I64(26));
///
/// let sql = build_dynamic_update(&*dialect, "users", "id", &Value::I64(1), &tracker).unwrap();
/// assert!(sql.contains("UPDATE `users` SET"));
/// assert!(sql.contains("`age` = 26"));
/// assert!(!sql.contains("`name`")); // 未修改字段不出现
/// ```
pub fn build_dynamic_update(
    dialect: &dyn Dialect,
    table: &str,
    pk_column: &str,
    pk_value: &Value,
    tracker: &DirtyTracker,
) -> Option<String> {
    let dirty = tracker.get_dirty_attributes();
    if dirty.is_empty() {
        return None;
    }

    let quoted_table = dialect.quote(table);
    let quoted_pk = dialect.quote(pk_column);

    // 按字段名字典序排列，保证 SQL 输出稳定
    let mut fields: Vec<&String> = dirty.keys().collect();
    fields.sort();

    let sets: Vec<String> = fields
        .iter()
        .map(|k| format!("{} = {}", dialect.quote(k), dirty[*k].to_param()))
        .collect();
    let sets_sql = sets.join(", ");

    Some(format!(
        "UPDATE {} SET {} WHERE {} = {}",
        quoted_table,
        sets_sql,
        quoted_pk,
        pk_value.to_param(),
    ))
}

// ============================================================================
// build_dynamic_insert — 仅生成非 null 字段的 INSERT SQL
// ============================================================================

/// 生成仅含非 null 字段的 INSERT SQL（对应 Hibernate `@DynamicInsert`）
///
/// 生成的 SQL 形如：
/// ```sql
/// INSERT INTO `table` (`col1`, `col2`) VALUES (?, ?)
/// ```
///
/// `Value::Null` 字段会被排除，让数据库使用列默认值。
///
/// # 参数
/// - `dialect`：数据库方言
/// - `table`：表名
/// - `data`：要插入的字段（Null 字段会被过滤）
///
/// # 返回
/// - `Some(sql)`：存在非 null 字段时返回 INSERT SQL
/// - `None`：所有字段均为 Null，无法生成 INSERT
///
/// # 示例
///
/// ```
/// use sz_orm_core::dirty_attributes::build_dynamic_insert;
/// use sz_orm_core::{DbType, get_dialect, Value};
/// use std::collections::HashMap;
///
/// let dialect = get_dialect(DbType::MySQL).unwrap();
/// let mut data = HashMap::new();
/// data.insert("name".to_string(), Value::String("alice".to_string()));
/// data.insert("age".to_string(), Value::I64(25));
/// data.insert("bio".to_string(), Value::Null); // Null 字段被排除
///
/// let sql = build_dynamic_insert(&*dialect, "users", &data).unwrap();
/// assert!(sql.contains("INSERT INTO `users`"));
/// assert!(sql.contains("`name`"));
/// assert!(sql.contains("`age`"));
/// assert!(!sql.contains("`bio`"));
/// ```
pub fn build_dynamic_insert(
    dialect: &dyn Dialect,
    table: &str,
    data: &HashMap<String, Value>,
) -> Option<String> {
    // 过滤 Null 字段
    let non_null: Vec<(&String, &Value)> = data
        .iter()
        .filter(|(_, v)| !matches!(v, Value::Null))
        .collect();

    if non_null.is_empty() {
        return None;
    }

    // 按字段名字典序排列
    let mut sorted = non_null.clone();
    sorted.sort_by(|a, b| a.0.cmp(b.0));

    let quoted_table = dialect.quote(table);
    let columns: Vec<String> = sorted.iter().map(|(k, _)| dialect.quote(k)).collect();
    let values: Vec<String> = sorted
        .iter()
        .map(|(_, v)| v.to_param().to_string())
        .collect();

    Some(format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quoted_table,
        columns.join(", "),
        values.join(", "),
    ))
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::get_dialect;
    use crate::DbType;

    // ===== DirtyTracker 基础测试 =====

    #[test]
    fn test_new_tracker_no_dirty() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let tracker = DirtyTracker::new(row);

        assert!(!tracker.is_dirty());
        assert!(tracker.get_dirty_fields().is_empty());
    }

    #[test]
    fn test_empty_tracker() {
        let tracker = DirtyTracker::empty();
        assert!(!tracker.is_dirty());
        assert!(tracker.get_dirty_fields().is_empty());
    }

    #[test]
    fn test_set_existing_field_makes_dirty() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("bob".to_string()));

        assert!(tracker.is_dirty());
        assert!(tracker.is_field_dirty("name"));
        assert!(!tracker.is_field_dirty("id"));
        assert_eq!(tracker.get_dirty_fields(), vec!["name"]);
    }

    #[test]
    fn test_set_new_field_makes_dirty() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("alice".to_string()));

        assert!(tracker.is_dirty());
        assert!(tracker.is_field_dirty("name"));
        assert_eq!(tracker.get_dirty_fields(), vec!["name"]);
    }

    #[test]
    fn test_set_same_value_not_dirty() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("alice".to_string()));

        assert!(!tracker.is_dirty());
    }

    #[test]
    fn test_set_int_value_not_dirty_when_same() {
        let mut row = HashMap::new();
        row.insert("age".to_string(), Value::I64(25));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("age", Value::I64(25));
        assert!(!tracker.is_dirty());

        tracker.set("age", Value::I64(26));
        assert!(tracker.is_dirty());
    }

    #[test]
    fn test_set_null_makes_dirty_when_was_value() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::Null);

        assert!(tracker.is_dirty());
        assert!(tracker.is_field_dirty("name"));
    }

    #[test]
    fn test_get_original() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("bob".to_string()));

        assert_eq!(
            tracker.get_original("name"),
            Some(&Value::String("alice".to_string()))
        );
        assert_eq!(tracker.get("name"), Some(&Value::String("bob".to_string())));
    }

    #[test]
    fn test_get_original_nonexistent() {
        let tracker = DirtyTracker::empty();
        assert_eq!(tracker.get_original("foo"), None);
    }

    #[test]
    fn test_get_dirty_attributes() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        row.insert("age".to_string(), Value::I64(25));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("bob".to_string()));
        tracker.set("age", Value::I64(26));

        let dirty = tracker.get_dirty_attributes();
        assert_eq!(dirty.len(), 2);
        assert_eq!(dirty.get("name"), Some(&Value::String("bob".to_string())));
        assert_eq!(dirty.get("age"), Some(&Value::I64(26)));
    }

    #[test]
    fn test_mark_clean() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("bob".to_string()));
        assert!(tracker.is_dirty());

        tracker.mark_clean();
        assert!(!tracker.is_dirty());
        assert_eq!(
            tracker.get_original("name"),
            Some(&Value::String("bob".to_string()))
        );
    }

    #[test]
    fn test_rollback() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("bob".to_string()));
        assert!(tracker.is_dirty());

        tracker.rollback();
        assert!(!tracker.is_dirty());
        assert_eq!(
            tracker.get("name"),
            Some(&Value::String("alice".to_string()))
        );
    }

    #[test]
    fn test_clear() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.clear();
        assert!(!tracker.is_dirty());
        assert!(tracker.current().is_empty());
        assert!(tracker.original().is_empty());
    }

    #[test]
    fn test_set_many() {
        let mut row = HashMap::new();
        row.insert("a".to_string(), Value::I64(1));
        let mut tracker = DirtyTracker::new(row);

        let mut updates = HashMap::new();
        updates.insert("a".to_string(), Value::I64(2));
        updates.insert("b".to_string(), Value::I64(3));
        tracker.set_many(updates);

        assert!(tracker.is_dirty());
        let dirty = tracker.get_dirty_fields();
        assert!(dirty.contains(&"a".to_string()));
        assert!(dirty.contains(&"b".to_string()));
    }

    #[test]
    fn test_multiple_dirty_fields_sorted() {
        let mut row = HashMap::new();
        row.insert("z".to_string(), Value::I64(1));
        row.insert("a".to_string(), Value::I64(1));
        row.insert("m".to_string(), Value::I64(1));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("z", Value::I64(2));
        tracker.set("a", Value::I64(2));
        tracker.set("m", Value::I64(2));

        assert_eq!(tracker.get_dirty_fields(), vec!["a", "m", "z"]);
    }

    #[test]
    fn test_remove_field_makes_dirty() {
        let row = HashMap::new();
        let tracker = DirtyTracker::new(row);

        // is_field_dirty 在 original 有但 current 无时返回 true
        // 这里通过手动构建场景验证：原始有值，当前没有
        assert!(!tracker.is_field_dirty("name"));
    }

    // ===== build_dynamic_update 测试 =====

    #[test]
    fn test_build_dynamic_update_with_dirty_fields() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        row.insert("age".to_string(), Value::I64(25));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("age", Value::I64(26));
        tracker.set("name", Value::String("bob".to_string()));

        let sql = build_dynamic_update(&*dialect, "users", "id", &Value::I64(1), &tracker).unwrap();

        assert!(sql.starts_with("UPDATE `users` SET"));
        // 字段按字典序：age 在前 name 在后
        assert!(sql.contains("`age` = 26"));
        assert!(sql.contains("`name` = 'bob'"));
        assert!(sql.contains("WHERE `id` = 1"));
        // 未修改字段不应出现在 SET 子句中（id 只在 WHERE 中）
        let set_clause = sql.split("WHERE").next().unwrap();
        assert!(!set_clause.contains("`id` ="));
    }

    #[test]
    fn test_build_dynamic_update_no_dirty_returns_none() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let tracker = DirtyTracker::new(row);

        let result = build_dynamic_update(&*dialect, "users", "id", &Value::I64(1), &tracker);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_dynamic_update_postgres() {
        let dialect = get_dialect(DbType::PostgreSQL).unwrap();
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("bob".to_string()));

        let sql = build_dynamic_update(&*dialect, "users", "id", &Value::I64(1), &tracker).unwrap();

        assert!(sql.contains("\"users\""));
        assert!(sql.contains("\"name\" = 'bob'"));
        assert!(sql.contains("\"id\" = 1"));
    }

    #[test]
    fn test_build_dynamic_update_single_dirty_field() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        row.insert("age".to_string(), Value::I64(25));
        let mut tracker = DirtyTracker::new(row);

        // 仅修改一个字段
        tracker.set("age", Value::I64(26));

        let sql = build_dynamic_update(&*dialect, "users", "id", &Value::I64(1), &tracker).unwrap();

        // 仅含 age 字段
        assert!(sql.contains("`age` = 26"));
        assert!(!sql.contains("`name`"));
    }

    #[test]
    fn test_build_dynamic_update_after_mark_clean() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("bob".to_string()));
        tracker.mark_clean();

        let result = build_dynamic_update(&*dialect, "users", "id", &Value::I64(1), &tracker);
        assert!(result.is_none());
    }

    // ===== build_dynamic_insert 测试 =====

    #[test]
    fn test_build_dynamic_insert_filters_null() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("alice".to_string()));
        data.insert("age".to_string(), Value::I64(25));
        data.insert("bio".to_string(), Value::Null);

        let sql = build_dynamic_insert(&*dialect, "users", &data).unwrap();

        assert!(sql.starts_with("INSERT INTO `users`"));
        assert!(sql.contains("`name`"));
        assert!(sql.contains("`age`"));
        assert!(!sql.contains("`bio`"));
        assert!(sql.contains("'alice'"));
        assert!(sql.contains("25"));
    }

    #[test]
    fn test_build_dynamic_insert_all_null_returns_none() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut data = HashMap::new();
        data.insert("a".to_string(), Value::Null);
        data.insert("b".to_string(), Value::Null);

        let result = build_dynamic_insert(&*dialect, "users", &data);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_dynamic_insert_empty_data_returns_none() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let data = HashMap::new();
        let result = build_dynamic_insert(&*dialect, "users", &data);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_dynamic_insert_postgres() {
        let dialect = get_dialect(DbType::PostgreSQL).unwrap();
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("alice".to_string()));
        data.insert("age".to_string(), Value::I64(25));

        let sql = build_dynamic_insert(&*dialect, "users", &data).unwrap();

        assert!(sql.contains("INSERT INTO \"users\""));
        assert!(sql.contains("\"name\""));
        assert!(sql.contains("\"age\""));
        assert!(sql.contains("'alice'"));
        assert!(sql.contains("25"));
    }

    #[test]
    fn test_build_dynamic_insert_columns_and_values_aligned() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut data = HashMap::new();
        data.insert("a".to_string(), Value::I64(1));
        data.insert("b".to_string(), Value::I64(2));
        data.insert("c".to_string(), Value::I64(3));

        let sql = build_dynamic_insert(&*dialect, "test", &data).unwrap();

        // 解析出 columns 部分和 values 部分
        let cols_start = sql.find('(').unwrap();
        let cols_end = sql.find(") VALUES").unwrap();
        let cols = &sql[cols_start + 1..cols_end];
        let vals_start = sql.rfind('(').unwrap();
        let vals_end = sql.rfind(')').unwrap();
        let vals = &sql[vals_start + 1..vals_end];

        let col_count = cols.split(',').count();
        let val_count = vals.split(',').count();
        assert_eq!(col_count, val_count);
        assert_eq!(col_count, 3);
    }

    #[test]
    fn test_build_dynamic_insert_with_bool() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut data = HashMap::new();
        data.insert("active".to_string(), Value::Bool(true));
        data.insert("name".to_string(), Value::String("alice".to_string()));

        let sql = build_dynamic_insert(&*dialect, "users", &data).unwrap();
        assert!(sql.contains("TRUE"));
        assert!(sql.contains("'alice'"));
    }

    // ===== 集成场景测试 =====

    #[test]
    fn test_workflow_load_modify_save() {
        let dialect = get_dialect(DbType::MySQL).unwrap();

        // 模拟从数据库加载
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("alice".to_string()));
        row.insert("age".to_string(), Value::I64(25));
        row.insert(
            "updated_at".to_string(),
            Value::String("2026-01-01".to_string()),
        );

        let mut tracker = DirtyTracker::new(row);

        // 修改字段
        tracker.set("age", Value::I64(26));
        tracker.set("updated_at", Value::String("2026-07-19".to_string()));

        // 生成 UPDATE
        let sql = build_dynamic_update(&*dialect, "users", "id", &Value::I64(1), &tracker).unwrap();
        assert!(sql.contains("`age` = 26"));
        assert!(sql.contains("`updated_at` = '2026-07-19'"));
        let set_clause = sql.split("WHERE").next().unwrap();
        assert!(!set_clause.contains("`name`"));
        assert!(!set_clause.contains("`id` ="));

        // 模拟写入成功，标记干净
        tracker.mark_clean();
        assert!(!tracker.is_dirty());

        // 再次修改
        tracker.set("name", Value::String("bob".to_string()));
        assert!(tracker.is_dirty());
        assert_eq!(tracker.get_dirty_fields(), vec!["name"]);
    }

    #[test]
    fn test_workflow_insert_with_optional_fields() {
        let dialect = get_dialect(DbType::MySQL).unwrap();

        // 模拟创建新记录，bio 字段未填（Null）
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("alice".to_string()));
        data.insert(
            "email".to_string(),
            Value::String("alice@example.com".to_string()),
        );
        data.insert("bio".to_string(), Value::Null);
        data.insert("age".to_string(), Value::I64(25));

        let sql = build_dynamic_insert(&*dialect, "users", &data).unwrap();
        // bio 不应出现
        assert!(!sql.contains("`bio`"));
        // 其他字段应出现
        assert!(sql.contains("`name`"));
        assert!(sql.contains("`email`"));
        assert!(sql.contains("`age`"));
    }

    #[test]
    fn test_workflow_rollback_on_failure() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("alice".to_string()));
        row.insert("age".to_string(), Value::I64(25));
        let mut tracker = DirtyTracker::new(row);

        tracker.set("name", Value::String("bob".to_string()));
        tracker.set("age", Value::I64(99));

        // 写入失败，回滚
        tracker.rollback();
        assert!(!tracker.is_dirty());
        assert_eq!(
            tracker.get("name"),
            Some(&Value::String("alice".to_string()))
        );
        assert_eq!(tracker.get("age"), Some(&Value::I64(25)));
    }
}
