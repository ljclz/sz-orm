//! 迁移工具：数据库迁移 SQL 生成与版本管理。
//!
//! - [`MigrationTool`] — 迁移工具（生成 ALTER TABLE、版本追踪）
//! - [`MigrationStep`] — 单步迁移操作
//! - [`SchemaDiff`] — 模型差异比较（生成迁移 SQL）

use napi_derive::napi;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::DbType;

use crate::model_def::{FieldDefinition, IndexDefinition, ModelDefinition};

type Result<T> = napi::bindgen_prelude::Result<T>;

fn parse_db_type(s: &str) -> Result<DbType> {
    DbType::from_str(s).ok_or_else(|| napi::Error::from_reason(format!("unknown DbType: {}", s)))
}

fn dialect_or_err(db_type: DbType) -> Result<Box<dyn sz_orm_core::dialect::Dialect>> {
    get_dialect(db_type).map_err(|e| napi::Error::from_reason(e.to_string()))
}

// ============================================================================
// 迁移操作类型
// ============================================================================

/// 迁移操作类型
#[napi]
#[derive(Debug, PartialEq, Eq)]
pub enum MigrationAction {
    /// 创建表
    CreateTable,
    /// 删除表
    DropTable,
    /// 添加列
    AddColumn,
    /// 删除列
    DropColumn,
    /// 修改列类型
    AlterColumn,
    /// 添加索引
    AddIndex,
    /// 删除索引
    DropIndex,
    /// 重命名表
    RenameTable,
}

impl MigrationAction {
    /// 操作名
    pub fn as_str(&self) -> &'static str {
        match self {
            MigrationAction::CreateTable => "create_table",
            MigrationAction::DropTable => "drop_table",
            MigrationAction::AddColumn => "add_column",
            MigrationAction::DropColumn => "drop_column",
            MigrationAction::AlterColumn => "alter_column",
            MigrationAction::AddIndex => "add_index",
            MigrationAction::DropIndex => "drop_index",
            MigrationAction::RenameTable => "rename_table",
        }
    }
}

// ============================================================================
// 迁移步骤
// ============================================================================

/// 单步迁移操作
#[napi]
pub struct MigrationStep {
    action: MigrationAction,
    description: String,
    sql: String,
    reversible: bool,
    rollback_sql: String,
}

#[napi]
impl MigrationStep {
    /// 创建迁移步骤
    #[napi(constructor)]
    pub fn new(action: MigrationAction, description: String, sql: String) -> Self {
        Self {
            action,
            description,
            sql,
            reversible: true,
            rollback_sql: String::new(),
        }
    }

    /// 设置回滚 SQL（链式）
    pub fn with_rollback(mut self, sql: String) -> Self {
        self.rollback_sql = sql;
        self
    }

    /// 标记为不可逆（链式）
    pub fn irreversible(mut self) -> Self {
        self.reversible = false;
        self
    }

    /// 操作类型名
    #[napi(getter)]
    pub fn action_name(&self) -> String {
        self.action.as_str().to_string()
    }

    /// 描述
    #[napi(getter)]
    pub fn description(&self) -> String {
        self.description.clone()
    }

    /// SQL
    #[napi(getter)]
    pub fn sql(&self) -> String {
        self.sql.clone()
    }

    /// 是否可逆
    #[napi(getter)]
    pub fn is_reversible(&self) -> bool {
        self.reversible
    }

    /// 回滚 SQL
    #[napi(getter)]
    pub fn rollback_sql(&self) -> String {
        self.rollback_sql.clone()
    }
}

// ============================================================================
// 迁移工具
// ============================================================================

/// 迁移工具：生成迁移步骤、版本追踪。
#[napi]
pub struct MigrationTool {
    db_type: DbType,
    version: u32,
    steps: Vec<MigrationStep>,
}

#[napi]
impl MigrationTool {
    /// 创建迁移工具
    #[napi(constructor)]
    pub fn new(db_type: Option<String>, version: u32) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        Ok(Self {
            db_type: parse_db_type(&dt)?,
            version,
            steps: vec![],
        })
    }

    /// 当前版本
    #[napi(getter)]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// 步骤数
    #[napi(getter)]
    pub fn step_count(&self) -> u32 {
        self.steps.len() as u32
    }

    /// 所有步骤的 SQL
    #[napi]
    pub fn all_sql(&self) -> Vec<String> {
        self.steps.iter().map(|s| s.sql()).collect()
    }

    /// 所有回滚 SQL
    #[napi]
    pub fn all_rollback_sql(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter(|s| s.is_reversible())
            .map(|s| s.rollback_sql())
            .collect()
    }

    /// 从模型定义生成创建表迁移
    pub fn create_table(&mut self, model: &ModelDefinition) -> Result<()> {
        let up_sql = model.to_create_table_sql()?;
        let down_sql = model.to_drop_table_sql()?;
        let table = model.table_name();
        self.steps.push(
            MigrationStep::new(
                MigrationAction::CreateTable,
                format!("Create table {}", table),
                up_sql,
            )
            .with_rollback(down_sql),
        );
        Ok(())
    }

    /// 从模型定义生成删除表迁移
    pub fn drop_table(&mut self, model: &ModelDefinition) -> Result<()> {
        let down_sql = model.to_create_table_sql()?;
        let up_sql = model.to_drop_table_sql()?;
        let table = model.table_name();
        self.steps.push(
            MigrationStep::new(
                MigrationAction::DropTable,
                format!("Drop table {}", table),
                up_sql,
            )
            .with_rollback(down_sql),
        );
        Ok(())
    }

    /// 添加列迁移
    pub fn add_column(&mut self, table: String, field: &FieldDefinition) -> Result<()> {
        let dialect = dialect_or_err(self.db_type)?;
        let col_sql = field.to_column_sql(self.db_type);
        let up_sql = format!(
            "ALTER TABLE {} ADD COLUMN {}",
            dialect.quote(&table),
            col_sql
        );
        let down_sql = format!(
            "ALTER TABLE {} DROP COLUMN {}",
            dialect.quote(&table),
            dialect.quote(&field.name())
        );
        self.steps.push(
            MigrationStep::new(
                MigrationAction::AddColumn,
                format!("Add column {} to {}", field.name(), table),
                up_sql,
            )
            .with_rollback(down_sql),
        );
        Ok(())
    }

    /// 删除列迁移
    #[napi]
    pub fn drop_column(&mut self, table: String, column_name: String) -> Result<()> {
        let dialect = dialect_or_err(self.db_type)?;
        let up_sql = format!(
            "ALTER TABLE {} DROP COLUMN {}",
            dialect.quote(&table),
            dialect.quote(&column_name)
        );
        self.steps.push(
            MigrationStep::new(
                MigrationAction::DropColumn,
                format!("Drop column {} from {}", column_name, table),
                up_sql,
            )
            .irreversible(),
        );
        Ok(())
    }

    /// 添加索引迁移
    pub fn add_index(&mut self, table: String, index: &IndexDefinition) -> Result<()> {
        let dialect = dialect_or_err(self.db_type)?;
        let up_sql = index.to_sql(&table, self.db_type);
        let down_sql = format!("DROP INDEX {}", dialect.quote(&index.name()));
        self.steps.push(
            MigrationStep::new(
                MigrationAction::AddIndex,
                format!("Add index {} on {}", index.name(), table),
                up_sql,
            )
            .with_rollback(down_sql),
        );
        Ok(())
    }

    /// 删除索引迁移
    #[napi]
    pub fn drop_index(&mut self, index_name: String) -> Result<()> {
        let dialect = dialect_or_err(self.db_type)?;
        let up_sql = format!("DROP INDEX {}", dialect.quote(&index_name));
        self.steps.push(
            MigrationStep::new(
                MigrationAction::DropIndex,
                format!("Drop index {}", index_name),
                up_sql,
            )
            .irreversible(),
        );
        Ok(())
    }

    /// 重命名表迁移
    #[napi]
    pub fn rename_table(&mut self, old_name: String, new_name: String) -> Result<()> {
        let dialect = dialect_or_err(self.db_type)?;
        let up_sql = format!(
            "ALTER TABLE {} RENAME TO {}",
            dialect.quote(&old_name),
            dialect.quote(&new_name)
        );
        let down_sql = format!(
            "ALTER TABLE {} RENAME TO {}",
            dialect.quote(&new_name),
            dialect.quote(&old_name)
        );
        self.steps.push(
            MigrationStep::new(
                MigrationAction::RenameTable,
                format!("Rename table {} to {}", old_name, new_name),
                up_sql,
            )
            .with_rollback(down_sql),
        );
        Ok(())
    }

    /// 清空所有步骤
    #[napi]
    pub fn clear(&mut self) {
        self.steps.clear();
    }

    /// 生成迁移摘要
    #[napi]
    pub fn summary(&self) -> String {
        format!(
            "Migration(v={}, steps={}, reversible={})",
            self.version,
            self.steps.len(),
            self.steps.iter().filter(|s| s.is_reversible()).count()
        )
    }
}

// ============================================================================
// 模型差异比较
// ============================================================================

/// 模型差异比较结果
#[napi(object)]
pub struct SchemaDiffResult {
    /// 需要添加的列
    pub added_columns: Vec<String>,
    /// 需要删除的列
    pub removed_columns: Vec<String>,
    /// 生成的迁移 SQL
    pub migration_sqls: Vec<String>,
}

/// 模型差异比较器：比较两个模型定义，生成迁移 SQL。
#[napi]
pub struct SchemaDiff {
    db_type: DbType,
}

#[napi]
impl SchemaDiff {
    /// 创建差异比较器
    #[napi(constructor)]
    pub fn new(db_type: Option<String>) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        Ok(Self {
            db_type: parse_db_type(&dt)?,
        })
    }

    /// 比较新旧模型，生成从 old 到 new 的迁移
    pub fn compare(
        &self,
        old_model: &ModelDefinition,
        new_model: &ModelDefinition,
    ) -> Result<SchemaDiffResult> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = new_model.table_name();

        let old_fields: Vec<String> = vec![]; // ModelDefinition 不暴露字段列表，用空集合
        let _ = old_fields;

        // 由于 ModelDefinition 不暴露内部字段列表，
        // 这里基于表名生成基础迁移
        let mut added_columns = vec![];
        let mut removed_columns = vec![];
        let mut migration_sqls = vec![];

        // 如果表名不同，生成重命名
        if old_model.table_name() != new_model.table_name() {
            migration_sqls.push(format!(
                "ALTER TABLE {} RENAME TO {}",
                dialect.quote(&old_model.table_name()),
                dialect.quote(&new_model.table_name())
            ));
        }

        Ok(SchemaDiffResult {
            added_columns,
            removed_columns,
            migration_sqls,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_def::FieldType;

    // ----- MigrationAction -----

    #[test]
    fn migration_action_as_str() {
        assert_eq!(MigrationAction::CreateTable.as_str(), "create_table");
        assert_eq!(MigrationAction::DropTable.as_str(), "drop_table");
        assert_eq!(MigrationAction::AddColumn.as_str(), "add_column");
        assert_eq!(MigrationAction::DropColumn.as_str(), "drop_column");
        assert_eq!(MigrationAction::AlterColumn.as_str(), "alter_column");
        assert_eq!(MigrationAction::AddIndex.as_str(), "add_index");
        assert_eq!(MigrationAction::DropIndex.as_str(), "drop_index");
        assert_eq!(MigrationAction::RenameTable.as_str(), "rename_table");
    }

    // ----- MigrationStep -----

    #[test]
    fn migration_step_new() {
        let step = MigrationStep::new(
            MigrationAction::CreateTable,
            "Create users".to_string(),
            "CREATE TABLE users".to_string(),
        );
        assert_eq!(step.action_name(), "create_table");
        assert_eq!(step.description(), "Create users");
        assert_eq!(step.sql(), "CREATE TABLE users");
        assert!(step.is_reversible());
        assert_eq!(step.rollback_sql(), "");
    }

    #[test]
    fn migration_step_with_rollback() {
        let step = MigrationStep::new(
            MigrationAction::CreateTable,
            "test".to_string(),
            "UP".to_string(),
        )
        .with_rollback("DOWN".to_string());
        assert_eq!(step.rollback_sql(), "DOWN");
    }

    #[test]
    fn migration_step_irreversible() {
        let step = MigrationStep::new(
            MigrationAction::DropColumn,
            "test".to_string(),
            "UP".to_string(),
        )
        .irreversible();
        assert!(!step.is_reversible());
    }

    // ----- MigrationTool -----

    #[test]
    fn migration_tool_new() {
        let tool = MigrationTool::new(None, 1).unwrap();
        assert_eq!(tool.version(), 1);
        assert_eq!(tool.step_count(), 0);
    }

    #[test]
    fn migration_tool_create_table() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        let mut model = ModelDefinition::new(None, "users".to_string()).unwrap();
        model.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        tool.create_table(&model).unwrap();
        assert_eq!(tool.step_count(), 1);
        let sqls = tool.all_sql();
        assert!(sqls[0].contains("CREATE TABLE"));
    }

    #[test]
    fn migration_tool_drop_table() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        let mut model = ModelDefinition::new(None, "users".to_string()).unwrap();
        model.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        tool.drop_table(&model).unwrap();
        assert_eq!(tool.step_count(), 1);
        let sqls = tool.all_sql();
        assert!(sqls[0].contains("DROP TABLE"));
    }

    #[test]
    fn migration_tool_add_column() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        let field = FieldDefinition::new("email".to_string(), FieldType::String);
        tool.add_column("users".to_string(), &field).unwrap();
        assert_eq!(tool.step_count(), 1);
        let sqls = tool.all_sql();
        assert!(sqls[0].contains("ALTER TABLE"));
        assert!(sqls[0].contains("ADD COLUMN"));
    }

    #[test]
    fn migration_tool_drop_column() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        tool.drop_column("users".to_string(), "email".to_string())
            .unwrap();
        assert_eq!(tool.step_count(), 1);
        let sqls = tool.all_sql();
        assert!(sqls[0].contains("DROP COLUMN"));
    }

    #[test]
    fn migration_tool_add_index() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        let idx = IndexDefinition::new("idx_email".to_string(), vec!["email".to_string()]);
        tool.add_index("users".to_string(), &idx).unwrap();
        assert_eq!(tool.step_count(), 1);
        let sqls = tool.all_sql();
        assert!(sqls[0].contains("CREATE INDEX"));
    }

    #[test]
    fn migration_tool_drop_index() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        tool.drop_index("idx_email".to_string()).unwrap();
        assert_eq!(tool.step_count(), 1);
        let sqls = tool.all_sql();
        assert!(sqls[0].contains("DROP INDEX"));
    }

    #[test]
    fn migration_tool_rename_table() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        tool.rename_table("old_table".to_string(), "new_table".to_string())
            .unwrap();
        assert_eq!(tool.step_count(), 1);
        let sqls = tool.all_sql();
        assert!(sqls[0].contains("RENAME"));
    }

    #[test]
    fn migration_tool_all_rollback_sql() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        let mut model = ModelDefinition::new(None, "users".to_string()).unwrap();
        model.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        tool.create_table(&model).unwrap();
        let rollbacks = tool.all_rollback_sql();
        assert_eq!(rollbacks.len(), 1);
        assert!(rollbacks[0].contains("DROP TABLE"));
    }

    #[test]
    fn migration_tool_clear() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        let mut model = ModelDefinition::new(None, "users".to_string()).unwrap();
        model.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        tool.create_table(&model).unwrap();
        tool.clear();
        assert_eq!(tool.step_count(), 0);
    }

    #[test]
    fn migration_tool_summary() {
        let mut tool = MigrationTool::new(None, 5).unwrap();
        let mut model = ModelDefinition::new(None, "users".to_string()).unwrap();
        model.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        tool.create_table(&model).unwrap();
        let s = tool.summary();
        assert!(s.contains("v=5"));
        assert!(s.contains("steps=1"));
    }

    #[test]
    fn migration_tool_multiple_steps() {
        let mut tool = MigrationTool::new(None, 1).unwrap();
        let mut model = ModelDefinition::new(None, "users".to_string()).unwrap();
        model.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        model.add_field(FieldDefinition::new("name".to_string(), FieldType::String));
        tool.create_table(&model).unwrap();
        tool.add_column(
            "users".to_string(),
            &FieldDefinition::new("email".to_string(), FieldType::String),
        )
        .unwrap();
        tool.add_index(
            "users".to_string(),
            &IndexDefinition::new("idx_email".to_string(), vec!["email".to_string()]),
        )
        .unwrap();
        assert_eq!(tool.step_count(), 3);
    }

    #[test]
    fn migration_tool_postgres_quoting() {
        let mut tool = MigrationTool::new(Some("postgres".to_string()), 1).unwrap();
        let field = FieldDefinition::new("email".to_string(), FieldType::String);
        tool.add_column("users".to_string(), &field).unwrap();
        let sqls = tool.all_sql();
        assert!(sqls[0].contains("\"users\""));
    }

    #[test]
    fn migration_tool_unknown_db_type() {
        assert!(MigrationTool::new(Some("unknown".to_string()), 1).is_err());
    }

    // ----- SchemaDiff -----

    #[test]
    fn schema_diff_new() {
        let diff = SchemaDiff::new(None).unwrap();
        let _ = diff;
    }

    #[test]
    fn schema_diff_same_table() {
        let diff = SchemaDiff::new(None).unwrap();
        let old = ModelDefinition::new(None, "users".to_string()).unwrap();
        let new = ModelDefinition::new(None, "users".to_string()).unwrap();
        let result = diff.compare(&old, &new).unwrap();
        assert!(result.migration_sqls.is_empty());
    }

    #[test]
    fn schema_diff_renamed_table() {
        let diff = SchemaDiff::new(None).unwrap();
        let old = ModelDefinition::new(None, "old_users".to_string()).unwrap();
        let new = ModelDefinition::new(None, "new_users".to_string()).unwrap();
        let result = diff.compare(&old, &new).unwrap();
        assert_eq!(result.migration_sqls.len(), 1);
        assert!(result.migration_sqls[0].contains("RENAME"));
    }
}
