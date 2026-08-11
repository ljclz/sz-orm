//! # 迁移 dry-run + 影响分析（`migration-dry-run` feature）
//!
//! 提供 `Migrator::migrate_dry_run`（预览 SQL 不执行）与 `Migrator::impact_analysis`，
//! 既有 `migrate` 保留不动。

use crate::migration::{Migration, Migrator};
use serde::{Deserialize, Serialize};

/// dry-run 预览的单个迁移
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunMigration {
    /// 版本号
    pub version: String,
    /// 迁移名称
    pub name: String,
    /// 正向 SQL
    pub sql_up: String,
    /// 反向 SQL
    pub sql_down: String,
}

/// dry-run 报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunReport {
    /// 待执行迁移列表
    pub migrations: Vec<DryRunMigration>,
    /// 总数
    pub total: usize,
}

/// DDL 操作类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdlType {
    /// CREATE TABLE / CREATE INDEX
    Create,
    /// ALTER TABLE ADD COLUMN
    AlterAdd,
    /// ALTER TABLE DROP COLUMN
    AlterDrop,
    /// DROP TABLE / DROP INDEX
    Drop,
    /// 其他（DML 等）
    Other,
}

/// 锁类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockType {
    /// 表级锁
    Table,
    /// 行级锁
    Row,
    /// 无锁（如 SELECT）
    None,
}

/// 单个迁移的影响分析
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationImpact {
    /// 版本号
    pub version: String,
    /// 迁移名称
    pub name: String,
    /// DDL 类型
    pub ddl_type: DdlType,
    /// 受影响的表名
    pub affected_tables: Vec<String>,
    /// 锁类型
    pub lock_type: LockType,
    /// 是否破坏性（DROP / ALTER DROP）
    pub is_destructive: bool,
    /// 回滚是否可行（有 sql_down 且非空）
    pub rollback_possible: bool,
    /// 预估影响行数（None 表示无法预估）
    pub estimated_rows: Option<u64>,
}

/// 影响分析报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    /// 各迁移影响分析
    pub migrations: Vec<MigrationImpact>,
    /// 破坏性迁移数量
    pub destructive_count: usize,
    /// 不可回滚迁移数量
    pub non_rollbackable_count: usize,
}

impl Migrator {
    /// 预览待执行迁移的 SQL，不实际执行
    ///
    /// 复用 `check_version_conflicts()` + `get_pending_migrations()`，
    /// 收集 pending 迁移信息到 `DryRunReport`，不调用 `conn.execute`。
    pub fn migrate_dry_run(&self) -> Result<DryRunReport, crate::DbError> {
        self.check_version_conflicts()?;
        let pending = self.get_pending_migrations();
        let mut migrations = Vec::with_capacity(pending.len());
        for m in &pending {
            migrations.push(DryRunMigration {
                version: m.version.clone(),
                name: m.name.clone(),
                sql_up: m.sql_up.clone(),
                sql_down: m.sql_down.clone(),
            });
        }
        let total = migrations.len();
        Ok(DryRunReport { migrations, total })
    }

    /// 分析待执行迁移的影响
    ///
    /// 解析每个迁移的 SQL，识别 DDL 类型、受影响表、锁类型、破坏性、回滚可行性。
    pub fn impact_analysis(&self) -> Result<ImpactReport, crate::DbError> {
        self.check_version_conflicts()?;
        let pending = self.get_pending_migrations();
        let mut impacts = Vec::with_capacity(pending.len());
        for m in pending {
            impacts.push(analyze_migration(m));
        }
        let destructive_count = impacts.iter().filter(|i| i.is_destructive).count();
        let non_rollbackable_count = impacts.iter().filter(|i| !i.rollback_possible).count();
        Ok(ImpactReport {
            migrations: impacts,
            destructive_count,
            non_rollbackable_count,
        })
    }
}

/// 分析单个迁移的影响
fn analyze_migration(m: &Migration) -> MigrationImpact {
    let ddl_type = classify_ddl(&m.sql_up);
    let affected_tables = extract_table_names(&m.sql_up);
    let lock_type = if ddl_type == DdlType::Other {
        LockType::None
    } else {
        LockType::Table
    };
    let is_destructive = matches!(ddl_type, DdlType::Drop | DdlType::AlterDrop);
    let rollback_possible = !m.sql_down.trim().is_empty();
    MigrationImpact {
        version: m.version.clone(),
        name: m.name.clone(),
        ddl_type,
        affected_tables,
        lock_type,
        is_destructive,
        rollback_possible,
        estimated_rows: None,
    }
}

/// 分类 DDL 类型
fn classify_ddl(sql: &str) -> DdlType {
    let upper = sql.to_uppercase();
    if upper.contains("DROP TABLE") || upper.contains("DROP INDEX") {
        DdlType::Drop
    } else if upper.contains("ALTER TABLE") && upper.contains("DROP COLUMN") {
        DdlType::AlterDrop
    } else if upper.contains("ALTER TABLE") && upper.contains("ADD COLUMN") {
        DdlType::AlterAdd
    } else if upper.contains("CREATE TABLE") || upper.contains("CREATE INDEX") {
        DdlType::Create
    } else {
        DdlType::Other
    }
}

/// 从 SQL 中提取表名
fn extract_table_names(sql: &str) -> Vec<String> {
    let mut tables = Vec::new();
    let upper = sql.to_uppercase();
    for keyword in ["TABLE", "TABLE IF NOT EXISTS"] {
        if let Some(idx) = upper.find(keyword) {
            let after = &sql[idx + keyword.len()..];
            let name: String = after
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '`' || *c == '"')
                .map(|c| if c == '`' || c == '"' { ' ' } else { c })
                .collect::<String>()
                .trim()
                .to_string();
            if !name.is_empty() && !tables.contains(&name) {
                tables.push(name);
            }
        }
    }
    tables
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::{Migration, MigrationContext, Migrator};

    fn create_test_migrator() -> Migrator {
        let ctx = MigrationContext::default().with_db_type(crate::DbType::Sqlite);
        Migrator::new(ctx)
            .add_migration(Migration::new(
                "20240101",
                "create_users",
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                "DROP TABLE users",
            ))
            .add_migration(Migration::new(
                "20240102",
                "add_email_column",
                "ALTER TABLE users ADD COLUMN email TEXT",
                "ALTER TABLE users DROP COLUMN email",
            ))
            .add_migration(Migration::new(
                "20240103",
                "drop_old_table",
                "DROP TABLE old_data",
                "",
            ))
    }

    #[test]
    fn test_migrate_dry_run() {
        let migrator = create_test_migrator();
        let report = migrator.migrate_dry_run().unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.migrations[0].version, "20240101");
        assert_eq!(report.migrations[0].name, "create_users");
        assert!(report.migrations[0].sql_up.contains("CREATE TABLE"));
    }

    #[test]
    fn test_migrate_dry_run_empty() {
        let ctx = MigrationContext::default();
        let migrator = Migrator::new(ctx);
        let report = migrator.migrate_dry_run().unwrap();
        assert_eq!(report.total, 0);
        assert!(report.migrations.is_empty());
    }

    #[test]
    fn test_impact_analysis() {
        let migrator = create_test_migrator();
        let report = migrator.impact_analysis().unwrap();
        assert_eq!(report.migrations.len(), 3);

        assert_eq!(report.migrations[0].ddl_type, DdlType::Create);
        assert!(!report.migrations[0].is_destructive);
        assert!(report.migrations[0].rollback_possible);

        assert_eq!(report.migrations[1].ddl_type, DdlType::AlterAdd);
        assert!(!report.migrations[1].is_destructive);

        assert_eq!(report.migrations[2].ddl_type, DdlType::Drop);
        assert!(report.migrations[2].is_destructive);
        assert!(!report.migrations[2].rollback_possible);

        assert_eq!(report.destructive_count, 1);
        assert_eq!(report.non_rollbackable_count, 1);
    }

    #[test]
    fn test_classify_ddl() {
        assert_eq!(classify_ddl("CREATE TABLE users (id INT)"), DdlType::Create);
        assert_eq!(classify_ddl("DROP TABLE users"), DdlType::Drop);
        assert_eq!(
            classify_ddl("ALTER TABLE users ADD COLUMN email TEXT"),
            DdlType::AlterAdd
        );
        assert_eq!(
            classify_ddl("ALTER TABLE users DROP COLUMN email"),
            DdlType::AlterDrop
        );
        assert_eq!(classify_ddl("SELECT * FROM users"), DdlType::Other);
    }

    #[test]
    fn test_extract_table_names() {
        let tables = extract_table_names("CREATE TABLE users (id INT)");
        assert!(tables.contains(&"users".to_string()));

        let tables = extract_table_names("DROP TABLE old_data");
        assert!(tables.contains(&"old_data".to_string()));
    }
}
