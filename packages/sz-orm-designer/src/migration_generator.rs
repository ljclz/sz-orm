//! 模式迁移生成器
//!
//! 提供 [`MigrationGenerator`] 用于从模式差异生成迁移脚本（DDL），
//! 支持 up/down 迁移、版本号管理、事务包装等。

use std::collections::HashMap;
use std::fmt;

/// 迁移操作类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOp {
    /// 创建表
    CreateTable {
        table: String,
        columns: Vec<ColumnDef>,
    },
    /// 删除表
    DropTable { table: String },
    /// 重命名表
    RenameTable { from: String, to: String },
    /// 添加列
    AddColumn { table: String, column: ColumnDef },
    /// 删除列
    DropColumn { table: String, column: String },
    /// 修改列类型
    AlterColumnType {
        table: String,
        column: String,
        new_type: String,
    },
    /// 重命名列
    RenameColumn {
        table: String,
        from: String,
        to: String,
    },
    /// 添加索引
    AddIndex { index: IndexDef },
    /// 删除索引
    DropIndex { name: String, table: String },
    /// 添加外键
    AddForeignKey { fk: ForeignKeyDef },
    /// 删除外键
    DropForeignKey { name: String, table: String },
    /// 执行原始 SQL
    RawSql { sql: String },
}

/// 列定义
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    /// 列名
    pub name: String,
    /// 数据类型
    pub data_type: String,
    /// 是否可空
    pub nullable: bool,
    /// 默认值
    pub default_value: Option<String>,
    /// 是否主键
    pub is_primary_key: bool,
    /// 是否唯一
    pub is_unique: bool,
    /// 注释
    pub comment: Option<String>,
}

impl ColumnDef {
    /// 创建新列
    #[must_use]
    pub fn new(name: &str, data_type: &str) -> Self {
        Self {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable: true,
            default_value: None,
            is_primary_key: false,
            is_unique: false,
            comment: None,
        }
    }

    /// 设置可空
    #[must_use]
    pub fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// 设置默认值
    #[must_use]
    pub fn default_value(mut self, value: &str) -> Self {
        self.default_value = Some(value.to_string());
        self
    }

    /// 标记为主键
    #[must_use]
    pub fn primary_key(mut self) -> Self {
        self.is_primary_key = true;
        self.nullable = false;
        self
    }

    /// 标记为唯一
    #[must_use]
    pub fn unique(mut self) -> Self {
        self.is_unique = true;
        self
    }

    /// 设置注释
    #[must_use]
    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = Some(comment.to_string());
        self
    }

    /// 生成 DDL 片段
    #[must_use]
    pub fn to_ddl(&self) -> String {
        let mut parts = vec![format!("{} {}", self.name, self.data_type)];
        if !self.nullable {
            parts.push("NOT NULL".to_string());
        }
        if let Some(ref default) = self.default_value {
            parts.push(format!("DEFAULT {default}"));
        }
        if self.is_primary_key {
            parts.push("PRIMARY KEY".to_string());
        }
        if self.is_unique && !self.is_primary_key {
            parts.push("UNIQUE".to_string());
        }
        parts.join(" ")
    }
}

/// 索引定义
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    /// 索引名
    pub name: String,
    /// 表名
    pub table: String,
    /// 列名
    pub columns: Vec<String>,
    /// 是否唯一
    pub unique: bool,
    /// 是否聚集
    pub clustered: bool,
}

impl IndexDef {
    /// 创建新索引
    #[must_use]
    pub fn new(name: &str, table: &str, columns: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            table: table.to_string(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            unique: false,
            clustered: false,
        }
    }

    /// 标记为唯一
    #[must_use]
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// 标记为聚集
    #[must_use]
    pub fn clustered(mut self) -> Self {
        self.clustered = true;
        self
    }

    /// 生成 CREATE INDEX DDL
    #[must_use]
    pub fn to_create_ddl(&self) -> String {
        let kind = match (self.unique, self.clustered) {
            (true, true) => "CREATE UNIQUE CLUSTERED INDEX",
            (true, false) => "CREATE UNIQUE INDEX",
            (false, true) => "CREATE CLUSTERED INDEX",
            (false, false) => "CREATE INDEX",
        };
        format!(
            "{} {} ON {} ({})",
            kind,
            self.name,
            self.table,
            self.columns.join(", ")
        )
    }

    /// 生成 DROP INDEX DDL
    #[must_use]
    pub fn to_drop_ddl(&self) -> String {
        format!("DROP INDEX {} ON {};", self.name, self.table)
    }
}

/// 外键定义
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKeyDef {
    /// 约束名
    pub name: String,
    /// 本表名
    pub table: String,
    /// 本表列
    pub columns: Vec<String>,
    /// 引用表名
    pub ref_table: String,
    /// 引用表列
    pub ref_columns: Vec<String>,
    /// 删除时动作
    pub on_delete: ReferenceAction,
    /// 更新时动作
    pub on_update: ReferenceAction,
}

/// 引用动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceAction {
    /// 无动作
    NoAction,
    /// 级联
    Cascade,
    /// 设为 NULL
    SetNull,
    /// 设为默认值
    SetDefault,
    /// 限制
    Restrict,
}

impl ReferenceAction {
    /// 返回 SQL 关键字
    #[must_use]
    pub fn as_sql(&self) -> &'static str {
        match self {
            ReferenceAction::NoAction => "NO ACTION",
            ReferenceAction::Cascade => "CASCADE",
            ReferenceAction::SetNull => "SET NULL",
            ReferenceAction::SetDefault => "SET DEFAULT",
            ReferenceAction::Restrict => "RESTRICT",
        }
    }
}

impl ForeignKeyDef {
    /// 创建新外键
    #[must_use]
    pub fn new(
        name: &str,
        table: &str,
        columns: &[&str],
        ref_table: &str,
        ref_columns: &[&str],
    ) -> Self {
        Self {
            name: name.to_string(),
            table: table.to_string(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            ref_table: ref_table.to_string(),
            ref_columns: ref_columns.iter().map(|s| s.to_string()).collect(),
            on_delete: ReferenceAction::NoAction,
            on_update: ReferenceAction::NoAction,
        }
    }

    /// 设置删除动作
    #[must_use]
    pub fn on_delete(mut self, action: ReferenceAction) -> Self {
        self.on_delete = action;
        self
    }

    /// 设置更新动作
    #[must_use]
    pub fn on_update(mut self, action: ReferenceAction) -> Self {
        self.on_update = action;
        self
    }

    /// 生成 ADD CONSTRAINT DDL
    #[must_use]
    pub fn to_add_ddl(&self) -> String {
        let mut sql = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            self.table,
            self.name,
            self.columns.join(", "),
            self.ref_table,
            self.ref_columns.join(", ")
        );
        if self.on_delete != ReferenceAction::NoAction {
            sql.push_str(&format!(" ON DELETE {}", self.on_delete.as_sql()));
        }
        if self.on_update != ReferenceAction::NoAction {
            sql.push_str(&format!(" ON UPDATE {}", self.on_update.as_sql()));
        }
        sql
    }

    /// 生成 DROP CONSTRAINT DDL
    #[must_use]
    pub fn to_drop_ddl(&self) -> String {
        format!("ALTER TABLE {} DROP CONSTRAINT {};", self.table, self.name)
    }
}

/// 迁移脚本
#[derive(Debug, Clone)]
pub struct MigrationScript {
    /// 版本号
    pub version: u64,
    /// 迁移名
    pub name: String,
    /// up 操作列表
    pub up_ops: Vec<MigrationOp>,
    /// down 操作列表
    pub down_ops: Vec<MigrationOp>,
}

impl MigrationScript {
    /// 创建新迁移脚本
    #[must_use]
    pub fn new(version: u64, name: &str) -> Self {
        Self {
            version,
            name: name.to_string(),
            up_ops: Vec::new(),
            down_ops: Vec::new(),
        }
    }

    /// 添加 up 操作
    #[must_use]
    pub fn up(mut self, op: MigrationOp) -> Self {
        self.up_ops.push(op);
        self
    }

    /// 添加 down 操作
    #[must_use]
    pub fn down(mut self, op: MigrationOp) -> Self {
        self.down_ops.push(op);
        self
    }

    /// 生成 up SQL
    #[must_use]
    pub fn up_sql(&self) -> String {
        self.up_ops
            .iter()
            .map(|op| op.to_up_sql())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 生成 down SQL
    #[must_use]
    pub fn down_sql(&self) -> String {
        self.down_ops
            .iter()
            .map(|op| op.to_up_sql())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl MigrationOp {
    /// 生成 up SQL
    #[must_use]
    pub fn to_up_sql(&self) -> String {
        match self {
            MigrationOp::CreateTable { table, columns } => {
                let cols: Vec<String> = columns.iter().map(|c| c.to_ddl()).collect();
                format!("CREATE TABLE {} (\n  {}\n);", table, cols.join(",\n  "))
            }
            MigrationOp::DropTable { table } => {
                format!("DROP TABLE IF EXISTS {};", table)
            }
            MigrationOp::RenameTable { from, to } => {
                format!("ALTER TABLE {} RENAME TO {};", from, to)
            }
            MigrationOp::AddColumn { table, column } => {
                format!("ALTER TABLE {} ADD COLUMN {};", table, column.to_ddl())
            }
            MigrationOp::DropColumn { table, column } => {
                format!("ALTER TABLE {} DROP COLUMN {};", table, column)
            }
            MigrationOp::AlterColumnType {
                table,
                column,
                new_type,
            } => {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE {};",
                    table, column, new_type
                )
            }
            MigrationOp::RenameColumn { table, from, to } => {
                format!("ALTER TABLE {} RENAME COLUMN {} TO {};", table, from, to)
            }
            MigrationOp::AddIndex { index } => {
                format!("{};", index.to_create_ddl())
            }
            MigrationOp::DropIndex { name, table } => {
                format!("DROP INDEX {} ON {};", name, table)
            }
            MigrationOp::AddForeignKey { fk } => {
                format!("{};", fk.to_add_ddl())
            }
            MigrationOp::DropForeignKey { name, table } => {
                format!("ALTER TABLE {} DROP CONSTRAINT {};", table, name)
            }
            MigrationOp::RawSql { sql } => sql.clone(),
        }
    }

    /// 生成 down SQL（反向操作）
    #[must_use]
    pub fn to_down_sql(&self) -> String {
        match self {
            MigrationOp::CreateTable { table, .. } => {
                format!("DROP TABLE IF EXISTS {};", table)
            }
            MigrationOp::DropTable { table } => {
                format!("-- Cannot restore dropped table: {}", table)
            }
            MigrationOp::AddColumn { table, column } => {
                format!("ALTER TABLE {} DROP COLUMN {};", table, column.name)
            }
            MigrationOp::DropColumn { table, column } => {
                format!("-- Cannot restore dropped column: {}.{}", table, column)
            }
            MigrationOp::AddIndex { index } => {
                format!("{};", index.to_drop_ddl())
            }
            MigrationOp::DropIndex { .. } => "-- Cannot restore dropped index".to_string(),
            MigrationOp::AddForeignKey { fk } => {
                format!("{};", fk.to_drop_ddl())
            }
            _ => "-- No automatic down migration".to_string(),
        }
    }
}

/// 迁移生成器
#[derive(Debug, Default)]
pub struct MigrationGenerator {
    /// 已生成的迁移脚本
    scripts: Vec<MigrationScript>,
}

impl MigrationGenerator {
    /// 创建新的迁移生成器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加迁移脚本
    pub fn add_script(&mut self, script: MigrationScript) {
        self.scripts.push(script);
    }

    /// 生成所有 up 迁移 SQL
    #[must_use]
    pub fn generate_all_up(&self) -> String {
        self.scripts
            .iter()
            .map(|s| format!("-- Migration {} {}\n{}", s.version, s.name, s.up_sql()))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// 生成所有 down 迁移 SQL
    #[must_use]
    pub fn generate_all_down(&self) -> String {
        self.scripts
            .iter()
            .rev()
            .map(|s| format!("-- Rollback {} {}\n{}", s.version, s.name, s.down_sql()))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// 生成指定版本的 up 迁移
    #[must_use]
    pub fn generate_up_to(&self, target_version: u64) -> String {
        self.scripts
            .iter()
            .filter(|s| s.version <= target_version)
            .map(|s| s.up_sql())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 生成指定版本的 down 迁移
    #[must_use]
    pub fn generate_down_from(&self, from_version: u64) -> String {
        self.scripts
            .iter()
            .rev()
            .filter(|s| s.version >= from_version)
            .map(|s| s.down_sql())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 生成事务包装的迁移
    #[must_use]
    pub fn generate_transactional(&self, version: u64) -> String {
        let script = self.scripts.iter().find(|s| s.version == version);
        match script {
            Some(s) => format!("BEGIN;\n{}\nCOMMIT;", s.up_sql()),
            None => String::new(),
        }
    }

    /// 迁移数量
    #[must_use]
    pub fn count(&self) -> usize {
        self.scripts.len()
    }

    /// 获取最新版本号
    #[must_use]
    pub fn latest_version(&self) -> Option<u64> {
        self.scripts.iter().map(|s| s.version).max()
    }

    /// 获取所有版本号
    #[must_use]
    pub fn versions(&self) -> Vec<u64> {
        self.scripts.iter().map(|s| s.version).collect()
    }

    /// 生成迁移文件内容（SQL 格式）
    #[must_use]
    pub fn to_sql_files(&self) -> HashMap<String, String> {
        let mut files = HashMap::new();
        for script in &self.scripts {
            let up_name = format!("V{}__{}.up.sql", script.version, script.name);
            let down_name = format!("V{}__{}.down.sql", script.version, script.name);
            files.insert(up_name, script.up_sql());
            files.insert(down_name, script.down_sql());
        }
        files
    }
}

impl fmt::Display for MigrationGenerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MigrationGenerator(count={})", self.count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_def_new() {
        let col = ColumnDef::new("id", "INT");
        assert_eq!(col.name, "id");
        assert!(col.nullable);
    }

    #[test]
    fn test_column_def_primary_key() {
        let col = ColumnDef::new("id", "INT").primary_key();
        assert!(col.is_primary_key);
        assert!(!col.nullable);
    }

    #[test]
    fn test_column_def_to_ddl() {
        let col = ColumnDef::new("id", "INT").primary_key();
        let ddl = col.to_ddl();
        assert!(ddl.contains("id INT"));
        assert!(ddl.contains("NOT NULL"));
        assert!(ddl.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_column_def_with_default() {
        let col = ColumnDef::new("status", "INT").default_value("0");
        let ddl = col.to_ddl();
        assert!(ddl.contains("DEFAULT 0"));
    }

    #[test]
    fn test_index_def_create_ddl() {
        let idx = IndexDef::new("ix_email", "users", &["email"]);
        let ddl = idx.to_create_ddl();
        assert_eq!(ddl, "CREATE INDEX ix_email ON users (email)");
    }

    #[test]
    fn test_index_def_unique() {
        let idx = IndexDef::new("ix_email", "users", &["email"]).unique();
        let ddl = idx.to_create_ddl();
        assert!(ddl.contains("CREATE UNIQUE INDEX"));
    }

    #[test]
    fn test_index_def_clustered() {
        let idx = IndexDef::new("pk", "users", &["id"]).clustered();
        let ddl = idx.to_create_ddl();
        assert!(ddl.contains("CLUSTERED"));
    }

    #[test]
    fn test_foreign_key_def() {
        let fk = ForeignKeyDef::new("fk_order_user", "orders", &["user_id"], "users", &["id"]);
        let ddl = fk.to_add_ddl();
        assert!(ddl.contains("FOREIGN KEY (user_id)"));
        assert!(ddl.contains("REFERENCES users (id)"));
    }

    #[test]
    fn test_foreign_key_cascade() {
        let fk = ForeignKeyDef::new("fk", "orders", &["user_id"], "users", &["id"])
            .on_delete(ReferenceAction::Cascade);
        let ddl = fk.to_add_ddl();
        assert!(ddl.contains("ON DELETE CASCADE"));
    }

    #[test]
    fn test_reference_action_as_sql() {
        assert_eq!(ReferenceAction::Cascade.as_sql(), "CASCADE");
        assert_eq!(ReferenceAction::SetNull.as_sql(), "SET NULL");
    }

    #[test]
    fn test_migration_op_create_table() {
        let op = MigrationOp::CreateTable {
            table: "users".to_string(),
            columns: vec![ColumnDef::new("id", "INT").primary_key()],
        };
        let sql = op.to_up_sql();
        assert!(sql.contains("CREATE TABLE users"));
        assert!(sql.contains("id INT"));
    }

    #[test]
    fn test_migration_op_drop_table() {
        let op = MigrationOp::DropTable {
            table: "old".to_string(),
        };
        let sql = op.to_up_sql();
        assert!(sql.contains("DROP TABLE IF EXISTS old"));
    }

    #[test]
    fn test_migration_op_add_column() {
        let op = MigrationOp::AddColumn {
            table: "users".to_string(),
            column: ColumnDef::new("email", "VARCHAR(255)"),
        };
        let sql = op.to_up_sql();
        assert!(sql.contains("ALTER TABLE users ADD COLUMN"));
    }

    #[test]
    fn test_migration_op_raw_sql() {
        let op = MigrationOp::RawSql {
            sql: "SELECT 1;".to_string(),
        };
        assert_eq!(op.to_up_sql(), "SELECT 1;");
    }

    #[test]
    fn test_migration_script() {
        let script = MigrationScript::new(1, "initial")
            .up(MigrationOp::CreateTable {
                table: "users".to_string(),
                columns: vec![ColumnDef::new("id", "INT").primary_key()],
            })
            .down(MigrationOp::DropTable {
                table: "users".to_string(),
            });
        let up = script.up_sql();
        let down = script.down_sql();
        assert!(up.contains("CREATE TABLE"));
        assert!(down.contains("DROP TABLE"));
    }

    #[test]
    fn test_migration_generator_add() {
        let mut gen = MigrationGenerator::new();
        gen.add_script(MigrationScript::new(1, "init"));
        assert_eq!(gen.count(), 1);
    }

    #[test]
    fn test_migration_generator_all_up() {
        let mut gen = MigrationGenerator::new();
        gen.add_script(
            MigrationScript::new(1, "init").up(MigrationOp::CreateTable {
                table: "t".to_string(),
                columns: vec![],
            }),
        );
        let sql = gen.generate_all_up();
        assert!(sql.contains("Migration 1 init"));
    }

    #[test]
    fn test_migration_generator_latest_version() {
        let mut gen = MigrationGenerator::new();
        gen.add_script(MigrationScript::new(1, "a"));
        gen.add_script(MigrationScript::new(3, "c"));
        gen.add_script(MigrationScript::new(2, "b"));
        assert_eq!(gen.latest_version(), Some(3));
    }

    #[test]
    fn test_migration_generator_versions() {
        let mut gen = MigrationGenerator::new();
        gen.add_script(MigrationScript::new(1, "a"));
        gen.add_script(MigrationScript::new(2, "b"));
        let versions = gen.versions();
        assert!(versions.contains(&1));
        assert!(versions.contains(&2));
    }

    #[test]
    fn test_migration_generator_up_to() {
        let mut gen = MigrationGenerator::new();
        gen.add_script(MigrationScript::new(1, "a").up(MigrationOp::RawSql {
            sql: "SELECT 1;".to_string(),
        }));
        gen.add_script(MigrationScript::new(2, "b").up(MigrationOp::RawSql {
            sql: "SELECT 2;".to_string(),
        }));
        let sql = gen.generate_up_to(1);
        assert!(sql.contains("SELECT 1;"));
        assert!(!sql.contains("SELECT 2;"));
    }

    #[test]
    fn test_migration_generator_transactional() {
        let mut gen = MigrationGenerator::new();
        gen.add_script(MigrationScript::new(1, "a").up(MigrationOp::RawSql {
            sql: "SELECT 1;".to_string(),
        }));
        let sql = gen.generate_transactional(1);
        assert!(sql.contains("BEGIN;"));
        assert!(sql.contains("COMMIT;"));
    }

    #[test]
    fn test_migration_generator_to_sql_files() {
        let mut gen = MigrationGenerator::new();
        gen.add_script(MigrationScript::new(1, "init"));
        let files = gen.to_sql_files();
        assert!(files.contains_key("V1__init.up.sql"));
        assert!(files.contains_key("V1__init.down.sql"));
    }

    #[test]
    fn test_migration_generator_display() {
        let gen = MigrationGenerator::new();
        let s = format!("{}", gen);
        assert!(s.contains("count=0"));
    }

    #[test]
    fn test_migration_op_down_create_table() {
        let op = MigrationOp::CreateTable {
            table: "t".to_string(),
            columns: vec![],
        };
        let sql = op.to_down_sql();
        assert!(sql.contains("DROP TABLE"));
    }

    #[test]
    fn test_migration_op_down_add_column() {
        let op = MigrationOp::AddColumn {
            table: "t".to_string(),
            column: ColumnDef::new("c", "INT"),
        };
        let sql = op.to_down_sql();
        assert!(sql.contains("DROP COLUMN c"));
    }
}
