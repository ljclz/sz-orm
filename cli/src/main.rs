//! # SZ-ORM CLI — 命令行工具
//!
//! 提供 ORM 日常开发所需命令：
//! - `migrate` / `migrate:status` — 迁移管理
//! - `make:migration <name>` — 生成迁移文件
//! - `make:model <name>` — 生成 Model 骨架
//! - `sql:validate <sql>` — SQL 校验
//! - `dialect list` / `dialect show <db>` — 方言信息
//! - `info` — 显示 ORM 概要信息
//!
//! ## 用法示例
//!
//! ```text
//! sz-orm info
//! sz-orm dialect list
//! sz-orm make:migration create_users
//! sz-orm make:model User
//! sz-orm sql:validate "SELECT * FROM users WHERE id = 1"
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, FileMigrationResolver, MigrationContext, MigrationResolver, Migrator};
use sz_orm_sql_validator::validate;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = r#"SZ-ORM 命令行工具

用法:
    sz-orm <command> [args]

命令:
    info                          显示 ORM 概要信息
    migrate                       执行所有待迁移（需 DSN，暂未支持）
    migrate:status                查看迁移进度
    make:migration <name>         生成迁移文件骨架（_up.sql / _down.sql）
    make:model <name>             生成 Model 骨架代码
    generate entity <table>       从 DB 表反向生成 Model 代码（需 --dsn）
    sql:validate <sql>            校验 SQL 语法 + 注入检测
    dialect list                  列出所有支持的方言
    dialect show <db_type>        显示指定方言详情
    help, --help, -h              显示本帮助
    --version, -V                 显示版本号

选项:
    --migrations <dir>            迁移文件目录（默认 ./migrations）
    --output <dir>                生成代码输出目录（默认 ./src/models 或 ./migrations）
    --dsn <url>                   数据库连接字符串（generate entity 必填）
                                 例：mysql://root:pass@127.0.0.1:3306/db
                                     postgres://user:pass@127.0.0.1:5432/db
                                     sqlite://./test.db

示例:
    sz-orm info
    sz-orm dialect list
    sz-orm dialect show mysql
    sz-orm make:migration create_users --output ./migrations
    sz-orm make:model User --output ./src/models
    sz-orm generate entity users --dsn mysql://root:test123@127.0.0.1:3306/sz_orm_test --output ./src/models
    sz-orm sql:validate "SELECT * FROM users"
"#;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("{}", HELP);
        return ExitCode::SUCCESS;
    }

    let command = args[1].as_str();
    let rest: Vec<&str> = args[2..].iter().map(|s| s.as_str()).collect();

    let exit = match command {
        "help" | "--help" | "-h" => {
            println!("{}", HELP);
            Ok(())
        }
        "--version" | "-V" => {
            println!("sz-orm {}", VERSION);
            Ok(())
        }
        "info" => cmd_info(),
        "migrate" => cmd_migrate(&rest),
        "migrate:status" => cmd_migrate_status(&rest),
        "make:migration" => cmd_make_migration(&rest),
        "make:model" => cmd_make_model(&rest),
        "generate" => cmd_generate(&rest),
        "sql:validate" => cmd_sql_validate(&rest),
        "dialect" => cmd_dialect(&rest),
        other => {
            eprintln!("未知命令: {}", other);
            eprintln!("\n{}", HELP);
            std::process::exit(2)
        }
    };

    match exit {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("错误: {}", e);
            ExitCode::FAILURE
        }
    }
}

// =====================================================================
// info — ORM 概要信息
// =====================================================================

fn cmd_info() -> Result<(), String> {
    println!("SZ-ORM {} — 鲜视达 ORM", VERSION);
    println!("工作空间: 31 个包");
    println!("数据库方言: 11 种");
    println!();
    println!("支持数据库:");
    let all = [
        DbType::MySQL,
        DbType::PostgreSQL,
        DbType::Sqlite,
        DbType::Redis,
        DbType::MongoDB,
        DbType::ClickHouse,
        DbType::Oracle,
        DbType::OceanBase,
        DbType::SqlServer,
        DbType::VectorDb,
        DbType::PureJsDb,
    ];
    for db in &all {
        println!("  - {:<12} (默认端口 {})", db.as_str(), db.default_port());
    }
    println!();
    println!("核心特性: 异步、ACID 事务、连接池、迁移系统、多级缓存、钩子系统");
    println!("生产等级: L4 金融级");
    Ok(())
}

// =====================================================================
// migrate — 执行迁移（需要 DB 连接，当前仅打印计划）
// =====================================================================

fn cmd_migrate(args: &[&str]) -> Result<(), String> {
    let migrations_dir =
        parse_option(args, "--migrations").unwrap_or_else(|| "./migrations".to_string());

    let resolver = FileMigrationResolver::new(PathBuf::from(&migrations_dir));
    let migrations = resolver
        .resolve(DbType::PostgreSQL)
        .map_err(|e| format!("解析迁移目录失败: {}", e))?;

    if migrations.is_empty() {
        println!("迁移目录 {} 中没有发现迁移文件", migrations_dir);
        return Ok(());
    }

    let migrator = Migrator::new(MigrationContext::default()).add_migrations(migrations);

    let pending = migrator.get_pending_migrations();
    if pending.is_empty() {
        println!("无待执行迁移（所有迁移均已应用）");
        return Ok(());
    }

    println!("待执行迁移 ({}):", pending.len());
    for m in &pending {
        println!("  - {} {}", m.version, m.name);
    }
    println!();
    println!("注意: 当前 CLI 未携带 DSN，无法实际执行 SQL。");
    println!("      请在应用层调用 Migrator::migrate() 完成实际迁移。");
    Ok(())
}

// =====================================================================
// migrate:status — 查看迁移进度
// =====================================================================

fn cmd_migrate_status(args: &[&str]) -> Result<(), String> {
    let migrations_dir =
        parse_option(args, "--migrations").unwrap_or_else(|| "./migrations".to_string());

    let resolver = FileMigrationResolver::new(PathBuf::from(&migrations_dir));
    let migrations = resolver
        .resolve(DbType::PostgreSQL)
        .map_err(|e| format!("解析迁移目录失败: {}", e))?;

    let migrator = Migrator::new(MigrationContext::default()).add_migrations(migrations);
    let progress = migrator.progress();

    println!("迁移目录: {}", migrations_dir);
    println!(
        "总计: {}  已应用: {}  待执行: {}",
        progress.total, progress.applied, progress.pending
    );
    println!("完成度: {:.1}%", progress.percent_complete());
    println!();

    let pending = migrator.get_pending_migrations();
    if !pending.is_empty() {
        println!("待执行迁移:");
        for m in &pending {
            println!("  - {} {}", m.version, m.name);
        }
    }
    Ok(())
}

// =====================================================================
// make:migration <name> — 生成迁移文件
// =====================================================================

fn cmd_make_migration(args: &[&str]) -> Result<(), String> {
    if args.is_empty() || args[0].starts_with("--") {
        return Err("用法: sz-orm make:migration <name> [--output <dir>]".into());
    }
    let name = args[0];
    let output_dir = parse_option(args, "--output").unwrap_or_else(|| "./migrations".to_string());

    fs::create_dir_all(&output_dir).map_err(|e| format!("创建目录 {} 失败: {}", output_dir, e))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let prefix = format!("{}_{}", timestamp, name);

    let up_path = PathBuf::from(&output_dir).join(format!("{}_up.sql", prefix));
    let down_path = PathBuf::from(&output_dir).join(format!("{}_down.sql", prefix));

    let up_content = format!(
        "-- Migration: {} (up)\n-- Created: {}\n\n-- TODO: 在此编写 up SQL\n",
        name,
        chrono::Utc::now().to_rfc3339()
    );
    let down_content = format!(
        "-- Migration: {} (down)\n-- Created: {}\n\n-- TODO: 在此编写 down SQL（回滚逻辑）\n",
        name,
        chrono::Utc::now().to_rfc3339()
    );

    fs::write(&up_path, up_content)
        .map_err(|e| format!("写入 {} 失败: {}", up_path.display(), e))?;
    fs::write(&down_path, down_content)
        .map_err(|e| format!("写入 {} 失败: {}", down_path.display(), e))?;

    println!("已生成迁移文件:");
    println!("  - {}", up_path.display());
    println!("  - {}", down_path.display());
    Ok(())
}

// =====================================================================
// make:model <name> — 生成 Model 骨架代码
// =====================================================================

fn cmd_make_model(args: &[&str]) -> Result<(), String> {
    if args.is_empty() || args[0].starts_with("--") {
        return Err("用法: sz-orm make:model <Name> [--output <dir>]".into());
    }
    let name = args[0];
    let output_dir = parse_option(args, "--output").unwrap_or_else(|| "./src/models".to_string());

    fs::create_dir_all(&output_dir).map_err(|e| format!("创建目录 {} 失败: {}", output_dir, e))?;

    let snake = to_snake_case(name);
    let table = pluralize(&snake);
    let code = format!(
        r#"//! Model: {name}

use sz_orm_core::model::{{Model, ModelExt, TimestampFields}};
use sz_orm_core::value::Value;

/// {name} 模型
#[derive(Debug, Clone, Default)]
pub struct {name} {{
    pub id: i64,
    // TODO: 在此添加业务字段
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}}

impl Model for {name} {{
    type PrimaryKey = i64;

    fn table_name() -> &'static str {{
        "{table}"
    }}

    fn pk_name() -> &'static str {{
        "id"
    }}

    fn pk(&self) -> Self::PrimaryKey {{
        self.id
    }}

    fn set_pk(&mut self, pk: Self::PrimaryKey) {{
        self.id = pk;
    }}

    fn timestamp_fields() -> Option<TimestampFields> {{
        Some(TimestampFields::with_both("created_at", "updated_at"))
    }}

    fn soft_delete_field() -> Option<&'static str> {{
        None
    }}
}}

impl ModelExt for {name} {{
    fn columns() -> Vec<&'static str> {{
        vec!["id", "created_at", "updated_at"]
    }}

    fn fillable() -> Vec<&'static str> {{
        vec![]
    }}

    fn get_column_value(&self, column: &str) -> Option<Value> {{
        match column {{
            "id" => Some(Value::I64(self.id)),
            "created_at" => self.created_at.clone().map(Value::String),
            "updated_at" => self.updated_at.clone().map(Value::String),
            _ => None,
        }}
    }}

    fn from_value(&mut self, map: std::collections::HashMap<String, Value>) {{
        for (k, v) in map {{
            match k.as_str() {{
                "id" => {{ if let Some(i) = v.as_i64() {{ self.id = i; }} }},
                "created_at" => {{ if let Some(s) = v.as_str() {{ self.created_at = Some(s.to_string()); }} }},
                "updated_at" => {{ if let Some(s) = v.as_str() {{ self.updated_at = Some(s.to_string()); }} }},
                _ => {{}}
            }}
        }}
    }}
}}
"#,
        name = name,
        table = table,
    );

    let path = PathBuf::from(&output_dir).join(format!("{}.rs", snake));
    fs::write(&path, code).map_err(|e| format!("写入 {} 失败: {}", path.display(), e))?;

    println!("已生成 Model:");
    println!("  - {} (表: {})", path.display(), table);
    Ok(())
}

// =====================================================================
// sql:validate <sql> — SQL 校验
// =====================================================================

fn cmd_sql_validate(args: &[&str]) -> Result<(), String> {
    if args.is_empty() {
        return Err("用法: sz-orm sql:validate <sql>".into());
    }
    let sql = args.join(" ");
    match validate(&sql) {
        Ok(()) => {
            println!("✓ SQL 校验通过");
            Ok(())
        }
        Err(e) => {
            eprintln!("✗ SQL 校验失败: {}", e);
            std::process::exit(1)
        }
    }
}

// =====================================================================
// dialect list / dialect show <db>
// =====================================================================

fn cmd_dialect(args: &[&str]) -> Result<(), String> {
    if args.is_empty() {
        eprintln!("用法: sz-orm dialect <list|show <db_type>>");
        return Err("缺少子命令".into());
    }
    match args[0] {
        "list" => {
            println!("支持的数据库方言:");
            let all = [
                DbType::MySQL,
                DbType::PostgreSQL,
                DbType::Sqlite,
                DbType::Oracle,
                DbType::ClickHouse,
                DbType::OceanBase,
                DbType::SqlServer,
                DbType::Redis,
                DbType::MongoDB,
                DbType::VectorDb,
                DbType::PureJsDb,
            ];
            for db in &all {
                let status = if get_dialect(*db).is_ok() {
                    "SQL"
                } else {
                    "NoSQL"
                };
                println!(
                    "  - {:<12} [{}]  默认端口 {}  事务: {}  外键: {}",
                    db.as_str(),
                    status,
                    db.default_port(),
                    yes_no(db.supports_transaction()),
                    yes_no(db.supports_foreign_key()),
                );
            }
            Ok(())
        }
        "show" => {
            if args.len() < 2 {
                return Err("用法: sz-orm dialect show <db_type>".into());
            }
            let db =
                DbType::from_str(args[1]).ok_or_else(|| format!("未知数据库类型: {}", args[1]))?;
            println!("数据库类型: {:?}", db);
            println!("标识符:     {}", db.as_str());
            println!("默认端口:   {}", db.default_port());
            println!("支持 Schema:   {}", yes_no(db.supports_schema()));
            println!("支持事务:    {}", yes_no(db.supports_transaction()));
            println!("支持外键:    {}", yes_no(db.supports_foreign_key()));
            println!("支持存储过程: {}", yes_no(db.supports_stored_procedure()));
            match get_dialect(db) {
                Ok(_) => println!("SQL 方言:    可用"),
                Err(e) => println!("SQL 方言:    不可用 ({})", e),
            }
            Ok(())
        }
        other => Err(format!("未知子命令: {}", other)),
    }
}

// =====================================================================
// 工具函数
// =====================================================================

fn parse_option(args: &[&str], key: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if *a == key {
            if let Some(v) = iter.next() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "是"
    } else {
        "否"
    }
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn pluralize(s: &str) -> String {
    if s.ends_with('s') || s.ends_with("sh") || s.ends_with("ch") || s.ends_with('x') {
        format!("{}es", s)
    } else if s.ends_with('y')
        && !s.ends_with("ay")
        && !s.ends_with("ey")
        && !s.ends_with("iy")
        && !s.ends_with("oy")
        && !s.ends_with("uy")
    {
        format!("{}ies", &s[..s.len() - 1])
    } else {
        format!("{}s", s)
    }
}

// =====================================================================
// generate entity <table> — 从 DB 表反向生成 Model 代码
// =====================================================================

fn cmd_generate(args: &[&str]) -> Result<(), String> {
    if args.is_empty() {
        return Err("用法: sz-orm generate entity <table> --dsn <url> [--output <dir>]\n     sz-orm generate schema --dsn <url> [--output <file>]".into());
    }
    match args[0] {
        "entity" => cmd_generate_entity(&args[1..]),
        "schema" => cmd_generate_schema(&args[1..]),
        other => Err(format!("未知子命令: generate {}", other)),
    }
}

fn cmd_generate_schema(args: &[&str]) -> Result<(), String> {
    let dsn = parse_option(args, "--dsn")
        .ok_or_else(|| "缺少 --dsn 参数（例如 mysql://root:pass@host:port/db）".to_string())?;
    let output_file =
        parse_option(args, "--output").unwrap_or_else(|| "./src/schema.rs".to_string());

    // 1. 探测数据库类型
    let db_kind = detect_db_kind(&dsn)?;

    // 2. 异步运行时内查询所有表 + 列信息
    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| format!("创建 tokio runtime 失败: {}", e))?;
    let tables = runtime.block_on(fetch_all_tables(&dsn, db_kind))?;

    if tables.is_empty() {
        return Err(format!("数据库没有表（dsn={}）", dsn));
    }

    // 3. 转换为 SchemaGenerator 需要的 TableSchema 格式
    use sz_orm_core::schema_gen::{ColumnSchema, SchemaGenerator, TableSchema};
    let table_schemas: Vec<TableSchema> = tables
        .iter()
        .map(|t| TableSchema {
            name: t.name.clone(),
            columns: t
                .columns
                .iter()
                .map(|c| ColumnSchema {
                    name: c.name.clone(),
                    rust_type: sz_orm_core::schema_gen::sql_type_to_rust(&c.db_type, c.nullable),
                })
                .collect(),
        })
        .collect();

    // 4. 生成 schema.rs
    let gen = SchemaGenerator::new();
    let code = gen.generate(&table_schemas);

    // 5. 写入文件
    let path = PathBuf::from(&output_file);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录 {} 失败: {}", parent.display(), e))?;
    }
    fs::write(&path, code).map_err(|e| format!("写入 {} 失败: {}", path.display(), e))?;

    println!("已从 DB 反向生成 schema.rs:");
    println!("  - DSN:     {}", dsn);
    println!("  - 表数量:  {}", table_schemas.len());
    let total_cols: usize = table_schemas.iter().map(|t| t.columns.len()).sum();
    println!("  - 列总数:  {}", total_cols);
    println!("  - 输出:    {}", path.display());
    println!();
    println!("提示：在代码中 import 此文件即可获得编译期列名校验能力：");
    println!("  mod schema;");
    println!("  use schema::users::col_id;");
    Ok(())
}

/// 表元数据（包含列信息）
struct TableInfo {
    name: String,
    columns: Vec<ColumnInfo>,
}

async fn fetch_all_tables(dsn: &str, kind: DbKind) -> Result<Vec<TableInfo>, String> {
    use sqlx::Row;
    let table_names: Vec<String> = match kind {
        DbKind::MySql => {
            let pool = sqlx::MySqlPool::connect(dsn)
                .await
                .map_err(|e| format!("MySQL 连接失败: {}", e))?;
            let schema = extract_schema_from_dsn(dsn);
            let rows = sqlx::query(
                "SELECT CAST(TABLE_NAME AS CHAR) FROM information_schema.tables \
                 WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' \
                 ORDER BY TABLE_NAME",
            )
            .bind(&schema)
            .fetch_all(&pool)
            .await
            .map_err(|e| format!("查询表列表失败: {}", e))?;
            let mut names = Vec::with_capacity(rows.len());
            for r in rows {
                let n: String = r.try_get(0).map_err(|e| e.to_string())?;
                names.push(n);
            }
            names
        }
        DbKind::Postgres => {
            let pool = sqlx::PgPool::connect(dsn)
                .await
                .map_err(|e| format!("PostgreSQL 连接失败: {}", e))?;
            let rows = sqlx::query(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
                 ORDER BY table_name",
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| format!("查询表列表失败: {}", e))?;
            let mut names = Vec::with_capacity(rows.len());
            for r in rows {
                let n: String = r.try_get(0).map_err(|e| e.to_string())?;
                names.push(n);
            }
            names
        }
        DbKind::Sqlite => {
            let pool = sqlx::SqlitePool::connect(dsn)
                .await
                .map_err(|e| format!("SQLite 连接失败: {}", e))?;
            let rows = sqlx::query(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            )
            .fetch_all(&pool).await
            .map_err(|e| format!("查询表列表失败: {}", e))?;
            let mut names = Vec::with_capacity(rows.len());
            for r in rows {
                let n: String = r.try_get(0).map_err(|e| e.to_string())?;
                names.push(n);
            }
            names
        }
    };

    // 并行（顺序）获取每张表的列
    let mut tables = Vec::with_capacity(table_names.len());
    for name in table_names {
        let cols = fetch_columns(dsn, kind, &name).await?;
        tables.push(TableInfo {
            name,
            columns: cols,
        });
    }
    Ok(tables)
}

fn cmd_generate_entity(args: &[&str]) -> Result<(), String> {
    if args.is_empty() || args[0].starts_with("--") {
        return Err("用法: sz-orm generate entity <table> --dsn <url> [--output <dir>]".into());
    }
    let table = args[0];
    let dsn = parse_option(args, "--dsn")
        .ok_or_else(|| "缺少 --dsn 参数（例如 mysql://root:pass@host:port/db）".to_string())?;
    let output_dir = parse_option(args, "--output").unwrap_or_else(|| "./src/models".to_string());

    // 1. 探测数据库类型
    let db_kind = detect_db_kind(&dsn)?;

    // 2. 异步运行时内查询列信息并生成代码
    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| format!("创建 tokio runtime 失败: {}", e))?;
    let columns = runtime.block_on(fetch_columns(&dsn, db_kind, table))?;

    if columns.is_empty() {
        return Err(format!("表 {} 不存在或无列信息（dsn={}）", table, dsn));
    }

    // 3. 生成 Rust Model 代码
    let struct_name = to_pascal_case(table);
    let code = render_model_code(&struct_name, table, &columns);

    // 4. 写入文件
    fs::create_dir_all(&output_dir).map_err(|e| format!("创建目录 {} 失败: {}", output_dir, e))?;
    let snake = to_snake_case(&struct_name);
    let path = PathBuf::from(&output_dir).join(format!("{}.rs", snake));
    fs::write(&path, code).map_err(|e| format!("写入 {} 失败: {}", path.display(), e))?;

    println!("已从 DB 反向生成 Model:");
    println!("  - DSN:  {}", dsn);
    println!("  - 表:   {}", table);
    println!("  - 列数: {}", columns.len());
    println!("  - 输出: {}", path.display());
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum DbKind {
    MySql,
    Postgres,
    Sqlite,
}

fn detect_db_kind(dsn: &str) -> Result<DbKind, String> {
    if dsn.starts_with("mysql://") || dsn.starts_with("mariadb://") {
        Ok(DbKind::MySql)
    } else if dsn.starts_with("postgres://") || dsn.starts_with("postgresql://") {
        Ok(DbKind::Postgres)
    } else if dsn.starts_with("sqlite://") || dsn.starts_with("sqlite:") {
        Ok(DbKind::Sqlite)
    } else {
        Err(format!(
            "不支持的 DSN scheme: {}（支持 mysql/postgres/sqlite）",
            dsn
        ))
    }
}

/// 列元信息
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ColumnInfo {
    name: String,
    db_type: String,
    nullable: bool,
    is_pk: bool,
    auto_increment: bool,
}

async fn fetch_columns(dsn: &str, kind: DbKind, table: &str) -> Result<Vec<ColumnInfo>, String> {
    use sqlx::Row;
    match kind {
        DbKind::MySql => {
            let pool = sqlx::MySqlPool::connect(dsn)
                .await
                .map_err(|e| format!("MySQL 连接失败: {}", e))?;
            // 拆出 schema/database 名称
            let schema = extract_schema_from_dsn(dsn);
            // 用 CAST(... AS CHAR) 规避 sqlx 把某些列识别为 BLOB 的问题
            let rows = sqlx::query(
                "SELECT CAST(COLUMN_NAME AS CHAR), CAST(DATA_TYPE AS CHAR), \
                 CAST(IS_NULLABLE AS CHAR), CAST(COLUMN_KEY AS CHAR), CAST(EXTRA AS CHAR) \
                 FROM information_schema.columns \
                 WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
                 ORDER BY ORDINAL_POSITION",
            )
            .bind(&schema)
            .bind(table)
            .fetch_all(&pool)
            .await
            .map_err(|e| format!("查询 information_schema 失败: {}", e))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                let name: String = r.try_get(0).map_err(|e| e.to_string())?;
                let db_type: String = r.try_get(1).map_err(|e| e.to_string())?;
                let nullable: String = r.try_get(2).map_err(|e| e.to_string())?;
                let column_key: String = r.try_get(3).map_err(|e| e.to_string())?;
                let extra: String = r.try_get(4).map_err(|e| e.to_string())?;
                out.push(ColumnInfo {
                    name,
                    db_type,
                    nullable: nullable == "YES",
                    is_pk: column_key == "PRI",
                    auto_increment: extra.to_lowercase().contains("auto_increment"),
                });
            }
            Ok(out)
        }
        DbKind::Postgres => {
            let pool = sqlx::PgPool::connect(dsn)
                .await
                .map_err(|e| format!("PostgreSQL 连接失败: {}", e))?;
            let rows = sqlx::query(
                "SELECT column_name, data_type, is_nullable, column_default \
                 FROM information_schema.columns \
                 WHERE table_name = $1 \
                 ORDER BY ordinal_position",
            )
            .bind(table)
            .fetch_all(&pool)
            .await
            .map_err(|e| format!("查询 information_schema 失败: {}", e))?;
            // 查询主键
            let pk_rows = sqlx::query(
                "SELECT kcu.column_name \
                 FROM information_schema.table_constraints tc \
                 JOIN information_schema.key_column_usage kcu \
                   ON tc.constraint_name = kcu.constraint_name \
                 WHERE tc.table_name = $1 AND tc.constraint_type = 'PRIMARY KEY'",
            )
            .bind(table)
            .fetch_all(&pool)
            .await
            .map_err(|e| format!("查询主键失败: {}", e))?;
            let mut pk_set = std::collections::HashSet::new();
            for r in pk_rows {
                let s: String = r.try_get(0).map_err(|e| e.to_string())?;
                pk_set.insert(s);
            }
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                let name: String = r.try_get(0).map_err(|e| e.to_string())?;
                let db_type: String = r.try_get(1).map_err(|e| e.to_string())?;
                let nullable: String = r.try_get(2).map_err(|e| e.to_string())?;
                let default: Option<String> = r.try_get(3).map_err(|e| e.to_string())?;
                let is_pk = pk_set.contains(&name);
                let auto_increment = default
                    .as_deref()
                    .map(|d| d.contains("nextval") || d.contains("::regclass"))
                    .unwrap_or(false);
                out.push(ColumnInfo {
                    name,
                    db_type,
                    nullable: nullable == "YES",
                    is_pk,
                    auto_increment,
                });
            }
            Ok(out)
        }
        DbKind::Sqlite => {
            // sqlx 支持 sqlite://path/to/db.db 或直接 file path 或 sqlite::memory:
            // 这里直接用完整 DSN 传给 sqlx
            let pool = sqlx::SqlitePool::connect(dsn)
                .await
                .map_err(|e| format!("SQLite 连接失败: {}", e))?;
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*format!("PRAGMA table_info({})", table)))
                .fetch_all(&pool)
                .await
                .map_err(|e| format!("PRAGMA table_info 失败: {}", e))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                use sqlx::Row;
                let name: String = r.try_get("name").map_err(|e| e.to_string())?;
                let db_type: String = r.try_get("type").map_err(|e| e.to_string())?;
                let notnull: i64 = r.try_get("notnull").map_err(|e| e.to_string())?;
                let pk: i64 = r.try_get("pk").map_err(|e| e.to_string())?;
                out.push(ColumnInfo {
                    name,
                    db_type,
                    nullable: notnull == 0,
                    is_pk: pk > 0,
                    auto_increment: false, // SQLite AUTOINCREMENT 难以从 PRAGMA 直接判断，保守 false
                });
            }
            Ok(out)
        }
    }
}

/// 从 DSN 提取数据库名（MySQL/PG 用）
fn extract_schema_from_dsn(dsn: &str) -> String {
    // mysql://user:pass@host:port/dbname?params
    if let Some(idx) = dsn.rfind('/') {
        let tail = &dsn[idx + 1..];
        if let Some(q) = tail.find('?') {
            tail[..q].to_string()
        } else {
            tail.to_string()
        }
    } else {
        String::new()
    }
}

/// 将 DB 列类型映射到 Rust 类型
fn map_db_type_to_rust(db_type: &str, nullable: bool) -> &'static str {
    let t = db_type.to_lowercase();
    let base: &str = if t.contains("int") && t.contains("big") {
        "i64"
    } else if t.contains("int") || t.contains("tinyint") || t.contains("smallint") {
        "i32"
    } else if t.contains("bool") || t.contains("bit") {
        "bool"
    } else if t.contains("real")
        || t.contains("float")
        || t.contains("double")
        || t.contains("numeric")
        || t.contains("decimal")
    {
        "f64"
    } else if t.contains("json")
        || t.contains("jsonb")
        || t.contains("date")
        || t.contains("time")
        || t.contains("timestamp")
        || t.contains("text")
        || t.contains("char")
        || t.contains("varchar")
        || t.contains("uuid")
    {
        "String"
    } else if t.contains("blob") || t.contains("binary") || t.contains("bytea") {
        "Vec<u8>"
    } else {
        "String"
    };
    if nullable {
        match base {
            "i64" => "Option<i64>",
            "i32" => "Option<i32>",
            "bool" => "Option<bool>",
            "f64" => "Option<f64>",
            "String" => "Option<String>",
            "Vec<u8>" => "Option<Vec<u8>>",
            _ => "Option<String>",
        }
    } else {
        base
    }
}

fn map_db_type_to_value_variant(db_type: &str) -> &'static str {
    let t = db_type.to_lowercase();
    if t.contains("int") && t.contains("big") {
        "Value::I64"
    } else if t.contains("int") || t.contains("tinyint") || t.contains("smallint") {
        "Value::I32"
    } else if t.contains("bool") || t.contains("bit") {
        "Value::Bool"
    } else if t.contains("real")
        || t.contains("float")
        || t.contains("double")
        || t.contains("numeric")
        || t.contains("decimal")
    {
        "Value::F64"
    } else if t.contains("json") || t.contains("jsonb") {
        "Value::Json"
    } else if t.contains("blob") || t.contains("binary") || t.contains("bytea") {
        "Value::Bytes"
    } else if t.contains("date") || t.contains("time") || t.contains("timestamp") {
        "Value::DateTime"
    } else {
        "Value::String"
    }
}

fn render_model_code(struct_name: &str, table: &str, columns: &[ColumnInfo]) -> String {
    // 1. 结构体字段
    let mut fields = String::new();
    for c in columns {
        let rust_type = map_db_type_to_rust(&c.db_type, c.nullable);
        fields.push_str(&format!("    pub {}: {},\n", c.name, rust_type));
    }

    // 2. columns() 列表
    let cols_list: Vec<String> = columns.iter().map(|c| format!("\"{}\"", c.name)).collect();
    let cols_join = cols_list.join(", ");

    // 3. fillable() 列表（排除主键）
    let fillable: Vec<String> = columns
        .iter()
        .filter(|c| !c.is_pk)
        .map(|c| format!("\"{}\"", c.name))
        .collect();
    let fillable_join = fillable.join(", ");

    // 4. get_column_value
    let mut get_col = String::new();
    for c in columns {
        let variant = map_db_type_to_value_variant(&c.db_type);
        // 处理 nullable 字段
        let expr = if c.nullable {
            match variant {
                "Value::I64" => format!("self.{}.map(Value::I64)", c.name),
                "Value::I32" => format!("self.{}.map(Value::I32)", c.name),
                "Value::Bool" => format!("self.{}.map(Value::Bool)", c.name),
                "Value::F64" => format!("self.{}.map(Value::F64)", c.name),
                "Value::String" => format!("self.{}.clone().map(Value::String)", c.name),
                "Value::DateTime" => format!("self.{}.clone().map(Value::DateTime)", c.name),
                "Value::Json" => format!("self.{}.clone().map(Value::Json)", c.name),
                "Value::Bytes" => format!("self.{}.clone().map(Value::Bytes)", c.name),
                _ => format!("self.{}.clone().map(Value::String)", c.name),
            }
        } else {
            match variant {
                "Value::I64" => format!("Some(Value::I64(self.{}))", c.name),
                "Value::I32" => format!("Some(Value::I32(self.{}))", c.name),
                "Value::Bool" => format!("Some(Value::Bool(self.{}))", c.name),
                "Value::F64" => format!("Some(Value::F64(self.{}))", c.name),
                "Value::String" => format!("Some(Value::String(self.{}.clone()))", c.name),
                "Value::DateTime" => format!("Some(Value::DateTime(self.{}.clone()))", c.name),
                "Value::Json" => format!("Some(Value::Json(self.{}.clone()))", c.name),
                "Value::Bytes" => format!("Some(Value::Bytes(self.{}.clone()))", c.name),
                _ => format!("Some(Value::String(self.{}.clone()))", c.name),
            }
        };
        get_col.push_str(&format!("            \"{}\" => {},\n", c.name, expr));
    }

    // 5. from_value
    let mut from_val = String::new();
    for c in columns {
        let variant = map_db_type_to_value_variant(&c.db_type);
        let parse = if c.nullable {
            match variant {
                "Value::I64" => format!("if let Some(Value::I64(v)) = map.get(\"{}\") {{ self.{} = Some(*v); }}", c.name, c.name),
                "Value::I32" => format!("if let Some(Value::I32(v)) = map.get(\"{}\") {{ self.{} = Some(*v); }}", c.name, c.name),
                "Value::Bool" => format!("if let Some(Value::Bool(v)) = map.get(\"{}\") {{ self.{} = Some(*v); }}", c.name, c.name),
                "Value::F64" => format!("if let Some(Value::F64(v)) = map.get(\"{}\") {{ self.{} = Some(*v); }}", c.name, c.name),
                "Value::String" | "Value::DateTime" | "Value::Json" =>
                    format!("if let Some(Value::String(v)) = map.get(\"{}\") {{ self.{} = Some(v.clone()); }}", c.name, c.name),
                "Value::Bytes" => format!("if let Some(Value::Bytes(v)) = map.get(\"{}\") {{ self.{} = Some(v.clone()); }}", c.name, c.name),
                _ => format!("if let Some(Value::String(v)) = map.get(\"{}\") {{ self.{} = Some(v.clone()); }}", c.name, c.name),
            }
        } else {
            match variant {
                "Value::I64" => format!(
                    "if let Some(Value::I64(v)) = map.get(\"{}\") {{ self.{} = *v; }}",
                    c.name, c.name
                ),
                "Value::I32" => format!(
                    "if let Some(Value::I32(v)) = map.get(\"{}\") {{ self.{} = *v; }}",
                    c.name, c.name
                ),
                "Value::Bool" => format!(
                    "if let Some(Value::Bool(v)) = map.get(\"{}\") {{ self.{} = *v; }}",
                    c.name, c.name
                ),
                "Value::F64" => format!(
                    "if let Some(Value::F64(v)) = map.get(\"{}\") {{ self.{} = *v; }}",
                    c.name, c.name
                ),
                "Value::String" | "Value::DateTime" | "Value::Json" => format!(
                    "if let Some(Value::String(v)) = map.get(\"{}\") {{ self.{} = v.clone(); }}",
                    c.name, c.name
                ),
                "Value::Bytes" => format!(
                    "if let Some(Value::Bytes(v)) = map.get(\"{}\") {{ self.{} = v.clone(); }}",
                    c.name, c.name
                ),
                _ => format!(
                    "if let Some(Value::String(v)) = map.get(\"{}\") {{ self.{} = v.clone(); }}",
                    c.name, c.name
                ),
            }
        };
        from_val.push_str(&format!("            {}\n", parse));
    }

    // 6. 主键列名
    let pk_col = columns
        .iter()
        .find(|c| c.is_pk)
        .map(|c| c.name.as_str())
        .unwrap_or("id");
    let pk_field_type = columns
        .iter()
        .find(|c| c.is_pk)
        .map(|c| map_db_type_to_rust(&c.db_type, false))
        .unwrap_or("i64");

    // 7. 生成完整代码
    format!(
        r#"//! Model: {struct_name}（由 sz-orm-cli generate entity 从表 `{table}` 反向生成）

use sz_orm_core::model::{{Model, ModelExt, TimestampFields}};
use sz_orm_core::value::Value;
use std::collections::HashMap;

/// {struct_name} 模型（自动生成自 DB 表 `{table}`）
#[derive(Debug, Clone, Default)]
pub struct {struct_name} {{
{fields}}}

impl Model for {struct_name} {{
    type PrimaryKey = {pk_field_type};

    fn table_name() -> &'static str {{
        "{table}"
    }}

    fn pk_name() -> &'static str {{
        "{pk_col}"
    }}

    fn pk(&self) -> Self::PrimaryKey {{
        self.{pk_col}.clone()
    }}

    fn set_pk(&mut self, pk: Self::PrimaryKey) {{
        self.{pk_col} = pk;
    }}

    fn timestamp_fields() -> Option<TimestampFields> {{
        None
    }}

    fn soft_delete_field() -> Option<&'static str> {{
        None
    }}
}}

impl ModelExt for {struct_name} {{
    fn columns() -> Vec<&'static str> {{
        vec![{cols_join}]
    }}

    fn fillable() -> Vec<&'static str> {{
        vec![{fillable_join}]
    }}

    fn get_column_value(&self, column: &str) -> Option<Value> {{
        match column {{
{get_col}            _ => None,
        }}
    }}

    fn from_value(&mut self, map: HashMap<String, Value>) {{
{from_val}    }}
}}
"#,
        struct_name = struct_name,
        table = table,
        fields = fields,
        pk_col = pk_col,
        pk_field_type = pk_field_type,
        cols_join = cols_join,
        fillable_join = fillable_join,
        get_col = get_col,
        from_val = from_val,
    )
}

fn to_pascal_case(s: &str) -> String {
    // table_name → TableName，user_orders → UserOrders
    let mut out = String::new();
    let mut upper_next = true;
    for c in s.chars() {
        if c == '_' || c == '-' || c == ' ' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.push(c.to_ascii_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    // 处理末尾的 's'（如 users → User）— 保守做法：仅当结尾是 's' 且非 'ss' 时去 s
    if out.ends_with('s') && !out.ends_with("ss") {
        out.pop();
    }
    out
}
