#![allow(missing_docs)]
//! 实体变更跟踪器（Change Tracker）
//!
//! 对标 EF Core `ChangeTracker` / Hibernate `PersistenceContext`。
//!
//! 跟踪实体的生命周期状态（Added / Modified / Deleted / Unchanged / Detached），
//! 在 `SaveChanges` 时自动生成相应的 INSERT / UPDATE / DELETE 语句。
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::change_tracker::{ChangeTracker, EntityEntry, EntityState};
//! use sz_orm_core::Value;
//! use std::collections::HashMap;
//!
//! let mut tracker = ChangeTracker::new();
//!
//! // 添加新实体
//! let mut user: HashMap<String, Value> = HashMap::new();
//! user.insert("name".to_string(), Value::String("alice".into()));
//! tracker.track("users", "1", user.clone(), EntityState::Added);
//!
//! // 修改实体
//! user.insert("name".to_string(), Value::String("bob".into()));
//! tracker.track("users", "1", user, EntityState::Modified);
//!
//! let changes = tracker.get_pending_changes();
//! assert_eq!(changes.len(), 1);
//! assert_eq!(changes[0].state, EntityState::Modified);
//! ```

use crate::Value;
use std::collections::HashMap;

/// 实体状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityState {
    /// 未跟踪
    Detached,
    /// 未变更
    Unchanged,
    /// 新增
    Added,
    /// 已修改
    Modified,
    /// 已删除
    Deleted,
}

impl EntityState {
    /// 是否需要生成 SQL
    pub fn is_pending(&self) -> bool {
        matches!(
            self,
            EntityState::Added | EntityState::Modified | EntityState::Deleted
        )
    }

    /// 对应的 SQL 操作名
    pub fn as_sql_op(&self) -> &'static str {
        match self {
            EntityState::Added => "INSERT",
            EntityState::Modified => "UPDATE",
            EntityState::Deleted => "DELETE",
            EntityState::Unchanged => "NOOP",
            EntityState::Detached => "NOOP",
        }
    }
}

impl std::fmt::Display for EntityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// 实体键（表名 + 主键值）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityKey {
    pub table: String,
    pub id: String,
}

impl EntityKey {
    pub fn new(table: &str, id: &str) -> Self {
        Self {
            table: table.to_string(),
            id: id.to_string(),
        }
    }
}

/// 实体跟踪条目
#[derive(Debug, Clone)]
pub struct EntityEntry {
    pub key: EntityKey,
    pub current: HashMap<String, Value>,
    pub original: Option<HashMap<String, Value>>,
    pub state: EntityState,
}

impl EntityEntry {
    /// 获取脏字段（current 与 original 的差异）
    pub fn get_dirty_fields(&self) -> Vec<String> {
        match &self.original {
            Some(orig) => {
                let mut dirty = Vec::new();
                for (k, v) in &self.current {
                    match orig.get(k) {
                        Some(orig_v) if orig_v != v => dirty.push(k.clone()),
                        None => dirty.push(k.clone()),
                        _ => {}
                    }
                }
                for k in orig.keys() {
                    if !self.current.contains_key(k) {
                        dirty.push(k.clone());
                    }
                }
                dirty
            }
            None => self.current.keys().cloned().collect(),
        }
    }

    /// 是否有变更
    pub fn is_dirty(&self) -> bool {
        self.state.is_pending() && !self.get_dirty_fields().is_empty()
    }
}

/// 变更跟踪器
///
/// 管理多个实体的状态，提供批量变更检测和变更集生成。
pub struct ChangeTracker {
    entries: HashMap<EntityKey, EntityEntry>,
}

impl Default for ChangeTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeTracker {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// 跟踪实体
    ///
    /// 如果状态为 `Unchanged`，会保存当前值作为原始值快照。
    /// 如果状态为 `Modified`，会保留之前的原始值快照。
    pub fn track(
        &mut self,
        table: &str,
        id: &str,
        current: HashMap<String, Value>,
        state: EntityState,
    ) {
        let key = EntityKey::new(table, id);
        let original = match state {
            EntityState::Added => None,
            EntityState::Unchanged => Some(current.clone()),
            EntityState::Modified => self
                .entries
                .get(&key)
                .and_then(|e| e.original.clone())
                .or_else(|| Some(current.clone())),
            EntityState::Deleted => self
                .entries
                .get(&key)
                .and_then(|e| e.original.clone())
                .or_else(|| Some(current.clone())),
            EntityState::Detached => None,
        };

        self.entries.insert(
            key,
            EntityEntry {
                key: EntityKey::new(table, id),
                current,
                original,
                state,
            },
        );
    }

    /// 标记为新增
    pub fn mark_added(&mut self, table: &str, id: &str, entity: HashMap<String, Value>) {
        self.track(table, id, entity, EntityState::Added);
    }

    /// 标记为未变更（从数据库加载后调用）
    pub fn mark_unchanged(&mut self, table: &str, id: &str, entity: HashMap<String, Value>) {
        self.track(table, id, entity, EntityState::Unchanged);
    }

    /// 标记为已修改
    pub fn update(&mut self, table: &str, id: &str, entity: HashMap<String, Value>) {
        let key = EntityKey::new(table, id);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.current = entity;
            if entry.state == EntityState::Unchanged {
                entry.state = EntityState::Modified;
            }
        } else {
            self.track(table, id, entity, EntityState::Modified);
        }
    }

    /// 标记为已删除
    pub fn mark_deleted(&mut self, table: &str, id: &str) {
        let key = EntityKey::new(table, id);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.state = EntityState::Deleted;
        }
    }

    /// 分离实体（停止跟踪）
    pub fn detach(&mut self, table: &str, id: &str) {
        let key = EntityKey::new(table, id);
        self.entries.remove(&key);
    }

    /// 自动检测变更
    ///
    /// 遍历所有 `Unchanged` 实体，如果 current 与 original 不同，自动转为 `Modified`。
    pub fn detect_changes(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.state == EntityState::Unchanged && !entry.get_dirty_fields().is_empty() {
                entry.state = EntityState::Modified;
            }
        }
    }

    /// 获取所有待提交的变更
    pub fn get_pending_changes(&self) -> Vec<&EntityEntry> {
        self.entries
            .values()
            .filter(|e| e.state.is_pending())
            .collect()
    }

    /// 按表分组获取待提交的变更
    pub fn get_pending_changes_by_table(&self) -> HashMap<String, Vec<&EntityEntry>> {
        let mut result: HashMap<String, Vec<&EntityEntry>> = HashMap::new();
        for entry in self.entries.values() {
            if entry.state.is_pending() {
                result
                    .entry(entry.key.table.clone())
                    .or_default()
                    .push(entry);
            }
        }
        result
    }

    /// 获取条目
    pub fn entry(&self, table: &str, id: &str) -> Option<&EntityEntry> {
        self.entries.get(&EntityKey::new(table, id))
    }

    /// 跟踪的实体数量
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// 待提交的变更数量
    pub fn pending_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.state.is_pending())
            .count()
    }

    /// 清除所有跟踪状态（SaveChanges 成功后调用）
    pub fn accept_changes(&mut self) {
        self.entries.retain(|_, entry| {
            if entry.state == EntityState::Deleted {
                false
            } else {
                entry.state = EntityState::Unchanged;
                entry.original = Some(entry.current.clone());
                true
            }
        });
    }

    /// 将待提交的变更转换为 SQL 语句列表（生产接线点）
    ///
    /// 这是 ChangeTracker 与 SQL 生成的集成点。
    /// 生成的 SQL 使用参数化占位符 `?`，参数通过返回值一并给出。
    pub fn build_sql_operations(&self) -> Vec<(String, Vec<Value>)> {
        let mut ops = Vec::new();
        for entry in self.entries.values() {
            if !entry.state.is_pending() {
                continue;
            }
            match entry.state {
                EntityState::Added => {
                    let columns: Vec<&str> = entry.current.keys().map(|s| s.as_str()).collect();
                    let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();
                    let params: Vec<Value> = entry.current.values().cloned().collect();
                    let sql = format!(
                        "INSERT INTO {} ({}) VALUES ({})",
                        entry.key.table,
                        columns.join(", "),
                        placeholders.join(", ")
                    );
                    ops.push((sql, params));
                }
                EntityState::Modified => {
                    let dirty = entry.get_dirty_fields();
                    if dirty.is_empty() {
                        continue;
                    }
                    let set_clauses: Vec<String> =
                        dirty.iter().map(|c| format!("{} = ?", c)).collect();
                    let mut params: Vec<Value> = dirty
                        .iter()
                        .filter_map(|c| entry.current.get(c).cloned())
                        .collect();
                    params.push(Value::String(entry.key.id.clone()));
                    let sql = format!(
                        "UPDATE {} SET {} WHERE id = ?",
                        entry.key.table,
                        set_clauses.join(", ")
                    );
                    ops.push((sql, params));
                }
                EntityState::Deleted => {
                    let sql = format!("DELETE FROM {} WHERE id = ?", entry.key.table);
                    ops.push((sql, vec![Value::String(entry.key.id.clone())]));
                }
                _ => {}
            }
        }
        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entity(name: &str, age: i64) -> HashMap<String, Value> {
        let mut m = HashMap::new();
        m.insert("name".to_string(), Value::String(name.to_string()));
        m.insert("age".to_string(), Value::I64(age));
        m
    }

    #[test]
    fn test_track_added() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_added("users", "1", make_entity("alice", 25));
        assert_eq!(tracker.pending_count(), 1);
        let changes = tracker.get_pending_changes();
        assert_eq!(changes[0].state, EntityState::Added);
    }

    #[test]
    fn test_track_unchanged_then_detect_modified() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_unchanged("users", "1", make_entity("alice", 25));
        assert_eq!(tracker.pending_count(), 0);

        tracker.update("users", "1", make_entity("alice", 26));
        tracker.detect_changes();
        assert_eq!(tracker.pending_count(), 1);
        let entry = tracker.entry("users", "1").unwrap();
        assert_eq!(entry.state, EntityState::Modified);
        assert_eq!(entry.get_dirty_fields(), vec!["age"]);
    }

    #[test]
    fn test_mark_deleted() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_unchanged("users", "1", make_entity("alice", 25));
        assert_eq!(tracker.pending_count(), 0);

        tracker.mark_deleted("users", "1");
        assert_eq!(tracker.pending_count(), 1);
        let entry = tracker.entry("users", "1").unwrap();
        assert_eq!(entry.state, EntityState::Deleted);
    }

    #[test]
    fn test_accept_changes() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_added("users", "1", make_entity("alice", 25));
        tracker.mark_unchanged("users", "2", make_entity("bob", 30));
        tracker.mark_unchanged("users", "3", make_entity("charlie", 35));
        tracker.mark_deleted("users", "3");

        assert_eq!(tracker.count(), 3);
        tracker.accept_changes();

        assert_eq!(tracker.count(), 2);
        assert_eq!(tracker.pending_count(), 0);
        assert!(tracker.entry("users", "3").is_none());
    }

    #[test]
    fn test_pending_changes_by_table() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_added("users", "1", make_entity("alice", 25));
        tracker.mark_added("orders", "1", make_entity("order1", 100));

        let by_table = tracker.get_pending_changes_by_table();
        assert_eq!(by_table["users"].len(), 1);
        assert_eq!(by_table["orders"].len(), 1);
    }

    #[test]
    fn test_detach() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_added("users", "1", make_entity("alice", 25));
        assert_eq!(tracker.count(), 1);

        tracker.detach("users", "1");
        assert_eq!(tracker.count(), 0);
        assert!(tracker.entry("users", "1").is_none());
    }

    #[test]
    fn test_entity_state_is_pending() {
        assert!(EntityState::Added.is_pending());
        assert!(EntityState::Modified.is_pending());
        assert!(EntityState::Deleted.is_pending());
        assert!(!EntityState::Unchanged.is_pending());
        assert!(!EntityState::Detached.is_pending());
    }

    #[test]
    fn test_entity_state_as_sql_op() {
        assert_eq!(EntityState::Added.as_sql_op(), "INSERT");
        assert_eq!(EntityState::Modified.as_sql_op(), "UPDATE");
        assert_eq!(EntityState::Deleted.as_sql_op(), "DELETE");
        assert_eq!(EntityState::Unchanged.as_sql_op(), "NOOP");
    }

    #[test]
    fn test_dirty_fields_detection() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_unchanged("users", "1", make_entity("alice", 25));

        let mut modified = make_entity("alice", 26);
        modified.insert("email".to_string(), Value::String("new@email.com".into()));
        tracker.update("users", "1", modified);

        let entry = tracker.entry("users", "1").unwrap();
        let dirty = entry.get_dirty_fields();
        assert!(dirty.contains(&"age".to_string()));
        assert!(dirty.contains(&"email".to_string()));
    }

    #[test]
    fn test_multiple_entities_same_table() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_added("users", "1", make_entity("alice", 25));
        tracker.mark_added("users", "2", make_entity("bob", 30));
        tracker.mark_added("users", "3", make_entity("charlie", 35));

        assert_eq!(tracker.pending_count(), 3);
        let by_table = tracker.get_pending_changes_by_table();
        assert_eq!(by_table["users"].len(), 3);
    }

    #[test]
    fn test_e2e_change_tracker_to_sql() {
        let mut tracker = ChangeTracker::new();

        tracker.mark_added("users", "1", make_entity("alice", 25));
        tracker.mark_unchanged("users", "2", make_entity("bob", 30));
        tracker.update("users", "2", make_entity("bob", 31));
        tracker.mark_unchanged("users", "3", make_entity("charlie", 35));
        tracker.mark_deleted("users", "3");

        tracker.detect_changes();
        let ops = tracker.build_sql_operations();
        assert_eq!(ops.len(), 3);

        let has_insert = ops
            .iter()
            .any(|(sql, _)| sql.starts_with("INSERT INTO users"));
        let has_update = ops
            .iter()
            .any(|(sql, _)| sql.starts_with("UPDATE users SET"));
        let has_delete = ops
            .iter()
            .any(|(sql, _)| sql.starts_with("DELETE FROM users"));
        assert!(has_insert, "missing INSERT");
        assert!(has_update, "missing UPDATE");
        assert!(has_delete, "missing DELETE");

        let update_op = ops
            .iter()
            .find(|(sql, _)| sql.starts_with("UPDATE"))
            .unwrap();
        assert!(update_op.0.contains("age = ?"));
        assert_eq!(update_op.1.len(), 2);
    }

    #[test]
    fn test_e2e_change_tracker_accept_then_clean() {
        let mut tracker = ChangeTracker::new();
        tracker.mark_added("users", "1", make_entity("alice", 25));
        assert_eq!(tracker.pending_count(), 1);

        let ops = tracker.build_sql_operations();
        assert_eq!(ops.len(), 1);

        tracker.accept_changes();
        assert_eq!(tracker.pending_count(), 0);
        let ops_after = tracker.build_sql_operations();
        assert_eq!(ops_after.len(), 0);
    }
}
