//! CDC Schema 变更适配器
//!
//! 检测源表 Schema 变更并适配目标端。

use std::collections::HashMap;

/// 列定义
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    /// 列名
    pub name: String,
    /// 数据类型
    pub data_type: String,
    /// 是否可空
    pub nullable: bool,
}

/// 表 Schema
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    /// 表名
    pub table_name: String,
    /// 列列表
    pub columns: Vec<ColumnDef>,
}

/// Schema 变更类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaChange {
    /// 新增列
    AddColumn { table: String, column: ColumnDef },
    /// 删除列
    DropColumn { table: String, column_name: String },
    /// 修改列类型
    AlterColumn {
        table: String,
        column_name: String,
        old_type: String,
        new_type: String,
    },
    /// 新增表
    CreateTable { schema: TableSchema },
    /// 删除表
    DropTable { table_name: String },
}

/// Schema 变更适配器
pub struct SchemaEvolutionAdapter {
    schemas: HashMap<String, TableSchema>,
}

impl SchemaEvolutionAdapter {
    /// 创建适配器
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// 注册初始 Schema
    pub fn register_schema(&mut self, schema: TableSchema) {
        self.schemas.insert(schema.table_name.clone(), schema);
    }

    /// 检测 Schema 变更
    pub fn detect_changes(&self, new_schema: &TableSchema) -> Vec<SchemaChange> {
        let mut changes = Vec::new();
        let table_name = &new_schema.table_name;

        match self.schemas.get(table_name) {
            None => {
                changes.push(SchemaChange::CreateTable {
                    schema: new_schema.clone(),
                });
            }
            Some(old_schema) => {
                for new_col in &new_schema.columns {
                    match old_schema.columns.iter().find(|c| c.name == new_col.name) {
                        None => {
                            changes.push(SchemaChange::AddColumn {
                                table: table_name.clone(),
                                column: new_col.clone(),
                            });
                        }
                        Some(old_col) if old_col.data_type != new_col.data_type => {
                            changes.push(SchemaChange::AlterColumn {
                                table: table_name.clone(),
                                column_name: new_col.name.clone(),
                                old_type: old_col.data_type.clone(),
                                new_type: new_col.data_type.clone(),
                            });
                        }
                        _ => {}
                    }
                }
                for old_col in &old_schema.columns {
                    if !new_schema.columns.iter().any(|c| c.name == old_col.name) {
                        changes.push(SchemaChange::DropColumn {
                            table: table_name.clone(),
                            column_name: old_col.name.clone(),
                        });
                    }
                }
            }
        }
        changes
    }

    /// 应用变更并更新内部 Schema
    pub fn apply_changes(&mut self, new_schema: TableSchema) -> Vec<SchemaChange> {
        let changes = self.detect_changes(&new_schema);
        self.schemas
            .insert(new_schema.table_name.clone(), new_schema);
        changes
    }

    /// 获取当前 Schema
    pub fn get_schema(&self, table: &str) -> Option<&TableSchema> {
        self.schemas.get(table)
    }

    /// 检查表是否已注册
    pub fn has_table(&self, table: &str) -> bool {
        self.schemas.contains_key(table)
    }
}

impl Default for SchemaEvolutionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_column(name: &str, ty: &str) -> ColumnDef {
        ColumnDef {
            name: name.to_string(),
            data_type: ty.to_string(),
            nullable: true,
        }
    }

    #[test]
    fn test_detect_new_table() {
        let mut adapter = SchemaEvolutionAdapter::new();
        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("id", "INT"), make_column("name", "VARCHAR")],
        };
        let changes = adapter.detect_changes(&schema);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], SchemaChange::CreateTable { .. }));
    }

    #[test]
    fn test_detect_add_column() {
        let mut adapter = SchemaEvolutionAdapter::new();
        let old = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("id", "INT")],
        };
        adapter.register_schema(old);
        let new = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("id", "INT"), make_column("email", "VARCHAR")],
        };
        let changes = adapter.detect_changes(&new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], SchemaChange::AddColumn { .. }));
    }

    #[test]
    fn test_detect_drop_column() {
        let mut adapter = SchemaEvolutionAdapter::new();
        let old = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("id", "INT"), make_column("name", "VARCHAR")],
        };
        adapter.register_schema(old);
        let new = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("id", "INT")],
        };
        let changes = adapter.detect_changes(&new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], SchemaChange::DropColumn { .. }));
    }

    #[test]
    fn test_detect_alter_column_type() {
        let mut adapter = SchemaEvolutionAdapter::new();
        let old = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("age", "INT")],
        };
        adapter.register_schema(old);
        let new = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("age", "BIGINT")],
        };
        let changes = adapter.detect_changes(&new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], SchemaChange::AlterColumn { .. }));
    }

    #[test]
    fn test_apply_changes_updates_schema() {
        let mut adapter = SchemaEvolutionAdapter::new();
        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("id", "INT")],
        };
        adapter.apply_changes(schema);
        assert!(adapter.has_table("users"));
    }

    #[test]
    fn test_no_changes_when_identical() {
        let mut adapter = SchemaEvolutionAdapter::new();
        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![make_column("id", "INT")],
        };
        adapter.register_schema(schema.clone());
        let changes = adapter.detect_changes(&schema);
        assert!(changes.is_empty());
    }
}
