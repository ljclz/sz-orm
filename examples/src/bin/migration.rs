//! 迁移系统 — 文件迁移 + SchemaBuilder
//!
//! 演示如何使用 FileMigrationResolver 加载迁移文件，
//! 以及使用 SchemaBuilder 程序化生成建表 SQL。
//!
//! 运行：`cargo run -p sz-orm-examples --bin migration`

use sz_orm_core::migration::{ColumnDef, ForeignKeyDef, IndexDef, MigrationContext, Migrator, SchemaBuilder};
use sz_orm_core::DbType;

fn main() {
    // ===== 1. SchemaBuilder 程序化建表 =====
    println!("=== SchemaBuilder: users 表 (MySQL) ===");
    let users_sql = SchemaBuilder::new("users")
        .add_column(
            ColumnDef::new("id", "BIGINT")
                .not_null()
                .auto_increment(),
        )
        .add_column(
            ColumnDef::new("name", "VARCHAR")
                .length(255)
                .not_null(),
        )
        .add_column(
            ColumnDef::new("email", "VARCHAR")
                .length(255)
                .not_null(),
        )
        .add_column(ColumnDef::new("age", "INT").default("0"))
        .add_column(ColumnDef::new("created_at", "TIMESTAMP").default("CURRENT_TIMESTAMP"))
        .add_column(ColumnDef::new("updated_at", "TIMESTAMP").default("CURRENT_TIMESTAMP"))
        .add_index(IndexDef::new("idx_email", vec!["email"]).unique())
        .add_index(IndexDef::new("idx_name", vec!["name"]))
        .build(DbType::MySQL);
    println!("{}\n", users_sql);

    println!("=== SchemaBuilder: posts 表 (PostgreSQL) ===");
    let posts_sql = SchemaBuilder::new("posts")
        .add_column(
            ColumnDef::new("id", "BIGINT")
                .not_null()
                .auto_increment(),
        )
        .add_column(
            ColumnDef::new("title", "VARCHAR")
                .length(255)
                .not_null(),
        )
        .add_column(ColumnDef::new("content", "TEXT"))
        .add_column(ColumnDef::new("author_id", "BIGINT").not_null())
        .add_column(ColumnDef::new("created_at", "TIMESTAMP").default("CURRENT_TIMESTAMP"))
        .add_index(IndexDef::new("idx_author", vec!["author_id"]))
        .add_foreign_key(
            ForeignKeyDef::new("fk_author", "author_id", "users", "id")
                .on_delete("CASCADE")
                .on_update("CASCADE"),
        )
        .build(DbType::PostgreSQL);
    println!("{}\n", posts_sql);

    // ===== 2. Migrator（无连接，演示状态管理）=====
    println!("=== Migrator: 空迁移上下文 ===");
    let migrator = Migrator::new(MigrationContext::default());
    let progress = migrator.progress();
    println!("总计: {}  已应用: {}  待执行: {}",
             progress.total, progress.applied, progress.pending);
    println!("完成度: {:.1}%", progress.percent_complete());

    println!("\n=== 文件迁移约定 ===");
    println!(
        r#"迁移文件命名: <version>_<name>_up.sql / <version>_<name>_down.sql
示例目录结构:
  migrations/
    ├── 001_create_users_up.sql
    ├── 001_create_users_down.sql
    ├── 002_add_posts_up.sql
    └── 002_add_posts_down.sql

加载与执行:
  let resolver = FileMigrationResolver::new(PathBuf::from("./migrations"));
  let migrations = resolver.resolve(DbType::MySQL)?;
  let mut migrator = Migrator::new(MigrationContext::default())
      .add_migrations(migrations);
  migrator.migrate().await?;          // 执行所有待迁移
  migrator.up(Some("003")).await?;    // 执行到 003
  migrator.down(Some("001")).await?;  // 回滚到 001
  migrator.rollback("002").await?;    // 回滚单个
  migrator.reset().await?;            // 全部回滚 + 重新执行
  migrator.refresh().await?;          // 同 reset
"#
    );
}
