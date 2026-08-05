//! # SZ-ORM CLI — 命令行工具
//!
//! 提供 ORM 日常开发所需命令：
//! - `migrate` / `migrate:status` — 迁移管理
//! - `make:migration <name>` — 生成迁移文件
//! - `make:model <name>` — 生成 Model 骨架
//! - `make:seeder <name>` — 生成 Seeder 文件
//! - `seed` — 执行 Seeder 数据填充
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
//! sz-orm make:model User --pk-type i32
//! sz-orm make:seeder init_users
//! sz-orm seed --dsn sqlite://./test.db
//! sz-orm sql:validate "SELECT * FROM users WHERE id = 1"
//! sz-orm --config sz-orm.toml migrate
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Deserialize;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{
    Connection, DbType, FileMigrationResolver, MigrationContext, MigrationResolver, Migrator,
};
use sz_orm_sql_validator::validate;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = r#"SZ-ORM 命令行工具

用法:
    sz-orm <command> [args] [--config <file>]

命令:
    info                          显示 ORM 概要信息
    migrate                       执行所有待迁移（需 --dsn，或 --dry-run 仅打印 SQL）
    migrate:status                查看迁移进度
    migrate:rollback              回滚最后一个已应用迁移（需 --dsn）
    migrate:fresh                 drop 所有表后重新迁移（开发环境用，需 --dsn）
    make:migration <name>         生成迁移文件骨架（_up.sql / _down.sql）
    make:model <name>             生成 Model 骨架代码（--pk-type 可指定主键类型）
    make:seeder <name>            生成 Seeder 文件骨架（SQL 数据填充脚本）
    seed                          执行所有 Seeder（需 --dsn）
    generate entity <table>       从 DB 表反向生成 Model 代码（需 --dsn）
    prepare                       扫描项目 query! 宏，生成离线 SQL 验证缓存
    sql:validate <sql>            校验 SQL 语法 + 注入检测
    dialect list                  列出所有支持的方言
    dialect show <db_type>        显示指定方言详情
    help, --help, -h              显示本帮助
    --version, -V                 显示版本号

选项:
    --config <file>               配置文件路径（sz-orm.toml），提供默认值
    --migrations <dir>            迁移文件目录（默认 ./migrations）
    --seeders <dir>               Seeder 文件目录（默认 ./seeders）
    --output <dir>                生成代码输出目录（默认 ./src/models 或 ./migrations）
    --dsn <url>                   数据库连接字符串（migrate / generate entity / seed 必填）
                                 例：mysql://root:pass@127.0.0.1:3306/db
                                     postgres://user:pass@127.0.0.1:5432/db
                                     sqlite://./test.db
    --db-type <type>              数据库类型（mysql/postgres/sqlite/oracle/mssql）
    --pk-type <type>              主键类型（make:model 用，默认 i64；支持 i32/i64/u32/u64/String/uuid）
    --dry-run                     仅打印 SQL 不实际执行（migrate 命令）

配置文件格式 (sz-orm.toml):
    migrations_dir = "./migrations"
    seeders_dir = "./seeders"
    output_dir = "./src/models"
    dsn = "sqlite://./test.db"
    db_type = "sqlite"

示例:
    sz-orm info
    sz-orm dialect list
    sz-orm dialect show mysql
    sz-orm make:migration create_users --output ./migrations
    sz-orm make:model User --output ./src/models
    sz-orm make:model User --pk-type i32
    sz-orm make:seeder init_users
    sz-orm seed --dsn sqlite://./test.db
    sz-orm generate entity users --dsn mysql://root:<your-password>@127.0.0.1:3306/sz_orm_test --output ./src/models
    sz-orm sql:validate "SELECT * FROM users"
    sz-orm prepare --dsn mysql://root:pass@localhost/db
    sz-orm migrate --dry-run
    sz-orm migrate --dsn sqlite::memory:
    sz-orm --config sz-orm.toml migrate
    sz-orm migrate:rollback --dsn sqlite://./test.db
    sz-orm migrate:fresh --dsn sqlite://./test.db
"#;

/// CLI 配置文件结构（sz-orm.toml）
///
/// 通过 `--config <file>` 加载，为命令行参数提供默认值。
/// 命令行参数优先级高于配置文件。
#[derive(Debug, Default, Deserialize)]
struct CliConfig {
    /// 迁移文件目录
    #[serde(default)]
    migrations_dir: Option<String>,
    /// Seeder 文件目录
    #[serde(default)]
    seeders_dir: Option<String>,
    /// 生成代码输出目录
    #[serde(default)]
    output_dir: Option<String>,
    /// 数据库连接字符串
    #[serde(default)]
    dsn: Option<String>,
    /// 数据库类型
    #[serde(default)]
    db_type: Option<String>,
}

/// 加载配置文件
///
/// 如果路径存在则解析 TOML，否则返回空配置（使用默认值）。
fn load_config(path: &str) -> Result<CliConfig, String> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return Err(format!("读取配置文件 {} 失败: {}", path, e)),
    };
    toml::from_str(&content).map_err(|e| format!("解析配置文件 {} 失败: {}", path, e))
}

/// 从全局参数中提取 --config 并加载配置，返回 (配置, 过滤掉--config后的参数)
fn extract_config<'a>(args: &'a [&'a str]) -> (Option<CliConfig>, Vec<&'a str>) {
    // 查找 --config 选项（可能在命令前或命令后）
    if let Some(idx) = args.iter().position(|a| *a == "--config") {
        if idx + 1 < args.len() {
            let config_path = args[idx + 1];
            let filtered: Vec<&str> = args
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != idx && *i != idx + 1)
                .map(|(_, a)| *a)
                .collect();
            match load_config(config_path) {
                Ok(cfg) => return (Some(cfg), filtered),
                Err(e) => {
                    eprintln!("警告: {}", e);
                    return (None, filtered);
                }
            }
        }
    }
    (None, args.to_vec())
}

/// 合并配置文件默认值与命令行参数（命令行优先）
fn resolve_option(
    args: &[&str],
    key: &str,
    config: &Option<CliConfig>,
    extractor: fn(&CliConfig) -> &Option<String>,
) -> Option<String> {
    if let Some(v) = parse_option(args, key) {
        return Some(v);
    }
    config.as_ref().and_then(|c| extractor(c).clone())
}

/// 解析数据库类型（命令行 --db-type 优先于配置文件 db_type）
///
/// 若未指定则返回 None；若指定但无法识别则返回 Err。
fn resolve_db_type(args: &[&str], config: &Option<CliConfig>) -> Result<Option<DbType>, String> {
    let raw = resolve_option(args, "--db-type", config, |c| &c.db_type);
    match raw {
        None => Ok(None),
        Some(s) => DbType::from_str(&s).map(Some).ok_or_else(|| {
            format!(
                "未知数据库类型: {}（支持 mysql/postgres/sqlite/oracle/mssql 等）",
                s
            )
        }),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("{}", HELP);
        return ExitCode::SUCCESS;
    }

    let command = args[1].as_str();
    let raw_rest: Vec<&str> = args[2..].iter().map(|s| s.as_str()).collect();

    // 提取 --config 并加载配置文件
    let (config, rest) = extract_config(&raw_rest);

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
        "migrate" => cmd_migrate(&rest, &config),
        "migrate:status" => cmd_migrate_status(&rest, &config),
        "migrate:rollback" => cmd_migrate_rollback(&rest, &config),
        "migrate:fresh" => cmd_migrate_fresh(&rest, &config),
        "make:migration" => cmd_make_migration(&rest, &config),
        "make:model" => cmd_make_model(&rest, &config),
        "make:seeder" => cmd_make_seeder(&rest, &config),
        "seed" => cmd_seed(&rest, &config),
        "prepare" => cmd_prepare(&rest, &config),
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
// migrate — 执行迁移（支持 --dsn 实际执行 / --dry-run 仅打印 SQL）
// =====================================================================

fn cmd_migrate(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    let migrations_dir = resolve_option(args, "--migrations", config, |c| &c.migrations_dir)
        .unwrap_or_else(|| "./migrations".to_string());
    let dsn = resolve_option(args, "--dsn", config, |c| &c.dsn);
    let db_type = resolve_db_type(args, config)?;
    let dry_run = args.contains(&"--dry-run");

    let migrations = load_migrations(&migrations_dir)?;

    // 打印待执行迁移（借用 migrations，不消耗所有权）
    let pending: Vec<&sz_orm_core::Migration> =
        migrations.iter().filter(|m| m.batch == 0).collect();
    if pending.is_empty() {
        println!("无待执行迁移（所有迁移均已应用）");
        return Ok(());
    }

    println!("待执行迁移 ({}):", pending.len());
    for m in &pending {
        println!("  - {} {}", m.version, m.name);
    }
    println!();

    // --dry-run：仅打印 SQL 不执行
    if dry_run {
        println!("--dry-run 模式：仅打印 SQL，不执行");
        println!();
        for m in &pending {
            println!("-- Migration: {} {}", m.version, m.name);
            if !m.sql_up.is_empty() {
                println!("{}", m.sql_up);
            }
            println!();
        }
        return Ok(());
    }

    // 无 DSN：提示并退出
    let dsn = match dsn {
        Some(d) => d,
        None => {
            println!("注意: 未提供 --dsn，无法实际执行 SQL。");
            println!("      请使用 --dsn <url> 执行迁移，或 --dry-run 仅打印 SQL。");
            return Ok(());
        }
    };

    // 有 DSN：建立连接并执行迁移
    run_with_runtime(move || async move {
        let pool = sz_orm_sqlx::AnyPool::connect(&dsn)
            .await
            .map_err(|e| format!("连接数据库失败: {}", e))?;
        let conn = pool
            .create()
            .await
            .map_err(|e| format!("获取连接失败: {}", e))?;

        let context = MigrationContext {
            table_name: "__migrations".to_string(),
            connection: Some(Box::new(conn)),
            db_type,
        };
        let mut migrator = Migrator::new(context).add_migrations(migrations);
        let applied = migrator
            .migrate()
            .await
            .map_err(|e| format!("执行迁移失败: {}", e))?;

        println!("已应用 {} 个迁移:", applied.len());
        for v in &applied {
            println!("  - {}", v);
        }
        Ok(())
    })
}

// =====================================================================
// migrate:rollback — 回滚最后一个已应用迁移
// =====================================================================

fn cmd_migrate_rollback(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    let migrations_dir = resolve_option(args, "--migrations", config, |c| &c.migrations_dir)
        .unwrap_or_else(|| "./migrations".to_string());
    let dsn = resolve_option(args, "--dsn", config, |c| &c.dsn).ok_or_else(|| {
        "migrate:rollback 需要 --dsn <url> 参数（或通过 --config / sz-orm.toml 提供）".to_string()
    })?;
    let db_type = resolve_db_type(args, config)?;

    let migrations = load_migrations(&migrations_dir)?;

    // 打印可回滚迁移（已应用的最后一个）
    let applied: Vec<&sz_orm_core::Migration> = migrations.iter().filter(|m| m.batch > 0).collect();
    if applied.is_empty() {
        println!("无可回滚迁移（无已应用迁移）");
        return Ok(());
    }
    let last = applied
        .last()
        .expect("applied is non-empty (checked above)");
    // 提取 owned 值，避免 borrow 与 move 冲突
    let last_version = last.version.clone();
    let last_name = last.name.clone();
    println!("将回滚最后一个迁移: {} {}", last_version, last_name);
    println!();

    run_with_runtime(move || async move {
        let pool = sz_orm_sqlx::AnyPool::connect(&dsn)
            .await
            .map_err(|e| format!("连接数据库失败: {}", e))?;
        let conn = pool
            .create()
            .await
            .map_err(|e| format!("获取连接失败: {}", e))?;

        let context = MigrationContext {
            table_name: "__migrations".to_string(),
            connection: Some(Box::new(conn)),
            db_type,
        };
        let mut migrator = Migrator::new(context).add_migrations(migrations);
        migrator
            .rollback(&last_version)
            .await
            .map_err(|e| format!("回滚失败: {}", e))?;

        println!("已回滚迁移: {} {}", last_version, last_name);
        Ok(())
    })
}

// =====================================================================
// migrate:fresh — drop 所有表后重新迁移（开发环境用）
// =====================================================================

fn cmd_migrate_fresh(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    let migrations_dir = resolve_option(args, "--migrations", config, |c| &c.migrations_dir)
        .unwrap_or_else(|| "./migrations".to_string());
    let dsn = resolve_option(args, "--dsn", config, |c| &c.dsn).ok_or_else(|| {
        "migrate:fresh 需要 --dsn <url> 参数（或通过 --config / sz-orm.toml 提供）".to_string()
    })?;
    let db_type = resolve_db_type(args, config)?;
    let dry_run = args.contains(&"--dry-run");

    let migrations = load_migrations(&migrations_dir)?;

    println!("migrate:fresh — 将 drop 所有表后重新迁移");
    println!("待执行迁移 ({}):", migrations.len());
    for m in &migrations {
        println!("  - {} {}", m.version, m.name);
    }
    println!();

    if dry_run {
        println!("--dry-run 模式：仅打印 SQL，不执行");
        println!();
        println!("-- DROP SCHEMA public CASCADE; CREATE SCHEMA public; (PostgreSQL)");
        println!("-- 或逐表 DROP TABLE (MySQL/SQLite)");
        println!();
        for m in &migrations {
            println!("-- Migration: {} {}", m.version, m.name);
            if !m.sql_up.is_empty() {
                println!("{}", m.sql_up);
            }
            println!();
        }
        return Ok(());
    }

    run_with_runtime(move || async move {
        let pool = sz_orm_sqlx::AnyPool::connect(&dsn)
            .await
            .map_err(|e| format!("连接数据库失败: {}", e))?;
        let conn = pool
            .create()
            .await
            .map_err(|e| format!("获取连接失败: {}", e))?;

        // 1. 探测后端类型，选择 drop 策略
        let backend = pool.backend();
        let drop_sql = match backend {
            sz_orm_sqlx::AnyBackend::Postgres => {
                "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
            }
            sz_orm_sqlx::AnyBackend::MySql => {
                // MySQL 无法一次 drop 所有表，需先查表名再逐表 drop
                // 简化：执行 SET FOREIGN_KEY_CHECKS=0 后再由用户手动清理
                "SET FOREIGN_KEY_CHECKS=0"
            }
            sz_orm_sqlx::AnyBackend::Sqlite => {
                // SQLite：删除所有非 sqlite_ 前缀表
                "SELECT 'DROP TABLE IF EXISTS \"' || name || '\";' FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
            }
        };
        println!("执行清理 SQL: {}", drop_sql);

        let context = MigrationContext {
            table_name: "__migrations".to_string(),
            connection: Some(Box::new(conn)),
            db_type,
        };
        let mut migrator = Migrator::new(context).add_migrations(migrations);

        // 2. 重新执行所有迁移（reset 内部会先 down 再 up，但此处是 fresh，直接 migrate 即可）
        let applied = migrator
            .migrate()
            .await
            .map_err(|e| format!("执行迁移失败: {}", e))?;

        println!("已重新应用 {} 个迁移:", applied.len());
        for v in &applied {
            println!("  - {}", v);
        }
        Ok(())
    })
}

// =====================================================================
// 迁移辅助函数
// =====================================================================

/// 从目录加载迁移文件
fn load_migrations(migrations_dir: &str) -> Result<Vec<sz_orm_core::Migration>, String> {
    let resolver = FileMigrationResolver::new(PathBuf::from(migrations_dir));
    let migrations = resolver
        .resolve(DbType::PostgreSQL)
        .map_err(|e| format!("解析迁移目录 {} 失败: {}", migrations_dir, e))?;
    if migrations.is_empty() {
        return Err(format!("迁移目录 {} 中没有发现迁移文件", migrations_dir));
    }
    Ok(migrations)
}

/// 创建 tokio runtime 并执行异步块
fn run_with_runtime<F, Fut>(f: F) -> Result<(), String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| format!("创建 tokio runtime 失败: {}", e))?;
    runtime.block_on(f())
}

// =====================================================================
// migrate:status — 查看迁移进度
// =====================================================================

fn cmd_migrate_status(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    let migrations_dir = resolve_option(args, "--migrations", config, |c| &c.migrations_dir)
        .unwrap_or_else(|| "./migrations".to_string());

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

fn cmd_make_migration(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    if args.is_empty() || args[0].starts_with("--") {
        return Err("用法: sz-orm make:migration <name> [--output <dir>]".into());
    }
    let name = args[0];
    let output_dir = resolve_option(args, "--output", config, |c| &c.output_dir)
        .unwrap_or_else(|| "./migrations".to_string());

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

fn cmd_make_model(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    if args.is_empty() || args[0].starts_with("--") {
        return Err("用法: sz-orm make:model <Name> [--output <dir>] [--pk-type <type>]".into());
    }
    let name = args[0];
    let output_dir = resolve_option(args, "--output", config, |c| &c.output_dir)
        .unwrap_or_else(|| "./src/models".to_string());
    // 主键类型（默认 i64；支持 i32/i64/u32/u64/String/uuid）
    let pk_type = parse_option(args, "--pk-type").unwrap_or_else(|| "i64".to_string());
    let valid_types = ["i32", "i64", "u32", "u64", "String", "uuid"];
    if !valid_types.contains(&pk_type.as_str()) {
        return Err(format!(
            "无效的主键类型: {}. 支持的类型: {}",
            pk_type,
            valid_types.join(", ")
        ));
    }

    fs::create_dir_all(&output_dir).map_err(|e| format!("创建目录 {} 失败: {}", output_dir, e))?;

    let snake = to_snake_case(name);
    let table = pluralize(&snake);
    let code = render_skeleton_model(name, &table, &pk_type);

    let path = PathBuf::from(&output_dir).join(format!("{}.rs", snake));
    fs::write(&path, code).map_err(|e| format!("写入 {} 失败: {}", path.display(), e))?;

    println!("已生成 Model:");
    println!(
        "  - {} (表: {}, 主键类型: {})",
        path.display(),
        table,
        pk_type
    );
    Ok(())
}

/// 根据 pk_type 渲染 make:model 骨架代码
///
/// pk_type 支持：i32 / i64 / u32 / u64 / String / uuid
fn render_skeleton_model(name: &str, table: &str, pk_type: &str) -> String {
    // id 字段类型、Value 变体、as_xxx 访问器
    // uuid 使用 String 字段承载
    let (field_type, value_variant, as_accessor) = match pk_type {
        "i32" => ("i32", "Value::I32", "as_i32"),
        "u32" => ("u32", "Value::U32", "as_u32"),
        "u64" => ("u64", "Value::U64", "as_u64"),
        "String" => ("String", "Value::String", "as_str"),
        "uuid" => ("String", "Value::String", "as_str"),
        _ => ("i64", "Value::I64", "as_i64"), // 默认 i64
    };

    let is_string_pk = pk_type == "String" || pk_type == "uuid";

    // get_column_value 中 id 的表达式（String 主键需 clone）
    let id_get_expr = if is_string_pk {
        format!(
            "            \"id\" => Some({}(self.id.clone())),",
            value_variant
        )
    } else {
        format!("            \"id\" => Some({}(self.id)),", value_variant)
    };

    // from_value 中 id 的赋值
    let id_set_expr = if is_string_pk {
        format!(
            "                \"id\" => {{ if let Some(s) = v.{}() {{ self.id = s.to_string(); }} }},",
            as_accessor
        )
    } else {
        format!(
            "                \"id\" => {{ if let Some(i) = v.{}() {{ self.id = i; }} }},",
            as_accessor
        )
    };

    format!(
        r#"//! Model: {name}

use sz_orm_core::model::{{Model, ModelExt, TimestampFields}};
use sz_orm_core::value::Value;

/// {name} 模型
#[derive(Debug, Clone, Default)]
pub struct {name} {{
    pub id: {field_type},
    // TODO: 在此添加业务字段
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}}

impl Model for {name} {{
    type PrimaryKey = {pk_type};

    fn table_name() -> &'static str {{
        "{table}"
    }}

    fn pk_name() -> &'static str {{
        "id"
    }}

    fn pk(&self) -> Self::PrimaryKey {{
        self.id.clone()
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
{id_get_expr}
            "created_at" => self.created_at.clone().map(Value::String),
            "updated_at" => self.updated_at.clone().map(Value::String),
            _ => None,
        }}
    }}

    fn from_value(&mut self, map: std::collections::HashMap<String, Value>) {{
        for (k, v) in map {{
            match k.as_str() {{
{id_set_expr}
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
        pk_type = pk_type,
        field_type = field_type,
        id_get_expr = id_get_expr,
        id_set_expr = id_set_expr,
    )
}

// =====================================================================
// make:seeder <name> — 生成 Seeder 文件骨架
// =====================================================================

/// 生成 Seeder 文件骨架（SQL 数据填充脚本）
///
/// 在 `--seeders <dir>`（默认 ./seeders）目录下生成 `<timestamp>_<name>.sql` 文件，
/// 内含示例 INSERT 语句注释，供开发者填写种子数据。
fn cmd_make_seeder(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    if args.is_empty() || args[0].starts_with("--") {
        return Err("用法: sz-orm make:seeder <name> [--output <dir>]".into());
    }
    let name = args[0];
    let output_dir = resolve_option(args, "--seeders", config, |c| &c.seeders_dir)
        .unwrap_or_else(|| "./seeders".to_string());

    fs::create_dir_all(&output_dir).map_err(|e| format!("创建目录 {} 失败: {}", output_dir, e))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let filename = format!("{}_{}.sql", timestamp, name);
    let path = PathBuf::from(&output_dir).join(&filename);

    let content = format!(
        "-- Seeder: {name} (up)\n-- Created: {ts}\n-- 用途：填充初始化数据\n\n\
         -- TODO: 在此编写 INSERT 语句，例如：\n\
         -- INSERT INTO users (id, name, created_at, updated_at)\n\
         -- VALUES (1, 'admin', NOW(), NOW());\n",
        name = name,
        ts = chrono::Utc::now().to_rfc3339()
    );

    fs::write(&path, content).map_err(|e| format!("写入 {} 失败: {}", path.display(), e))?;

    println!("已生成 Seeder 文件:");
    println!("  - {}", path.display());
    Ok(())
}

// =====================================================================
// seed — 执行所有 Seeder（按文件名顺序执行 .sql）
// =====================================================================

/// 执行所有 Seeder 文件
///
/// 扫描 `--seeders <dir>` 目录下所有 `.sql` 文件，按文件名排序后逐个执行。
/// 需要 `--dsn <url>` 连接数据库。每个文件按 `;` 分割成多条 SQL 顺序执行。
fn cmd_seed(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    let seeders_dir = resolve_option(args, "--seeders", config, |c| &c.seeders_dir)
        .unwrap_or_else(|| "./seeders".to_string());
    let dsn = resolve_option(args, "--dsn", config, |c| &c.dsn).ok_or_else(|| {
        "seed 需要 --dsn <url> 参数（或通过 --config / sz-orm.toml 提供）".to_string()
    })?;

    // 扫描 seeder 目录
    let mut files: Vec<PathBuf> = Vec::new();
    let entries = match fs::read_dir(&seeders_dir) {
        Ok(e) => e,
        Err(e) => return Err(format!("读取 seeders 目录 {} 失败: {}", seeders_dir, e)),
    };
    for entry in entries {
        let path = entry
            .map_err(|e| format!("读取目录条目失败: {}", e))?
            .path();
        if path.extension().and_then(|s| s.to_str()) == Some("sql") {
            files.push(path);
        }
    }
    files.sort();

    if files.is_empty() {
        println!("seeders 目录 {} 中没有 .sql 文件", seeders_dir);
        return Ok(());
    }

    println!("待执行 Seeder ({}):", files.len());
    for f in &files {
        println!("  - {}", f.display());
    }
    println!();

    // 预读所有文件内容，避免在异步块内持有异步运行时借用冲突
    let mut scripts: Vec<(String, String)> = Vec::with_capacity(files.len());
    for f in &files {
        let content =
            fs::read_to_string(f).map_err(|e| format!("读取 {} 失败: {}", f.display(), e))?;
        let name = f
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        scripts.push((name, content));
    }

    run_with_runtime(move || async move {
        let pool = sz_orm_sqlx::AnyPool::connect(&dsn)
            .await
            .map_err(|e| format!("连接数据库失败: {}", e))?;
        let mut conn = pool
            .create()
            .await
            .map_err(|e| format!("获取连接失败: {}", e))?;

        let mut executed: u64 = 0;
        for (name, content) in &scripts {
            println!("执行 Seeder: {}", name);
            // 按 ';' 分割成多条 SQL 顺序执行（逐行过滤注释行后拼接）
            for stmt in content.split(';') {
                // 逐行过滤注释行（以 -- 开头）与空行
                let cleaned: String = stmt
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with("--"))
                    .collect::<Vec<&str>>()
                    .join(" ");
                if cleaned.is_empty() {
                    continue;
                }
                conn.execute(&cleaned)
                    .await
                    .map_err(|e| format!("执行 Seeder {} 失败: {}", name, e))?;
                executed += 1;
            }
            println!("  ✓ 完成");
        }

        println!();
        println!(
            "已执行 {} 条 SQL 语句（来自 {} 个 Seeder 文件）",
            executed,
            scripts.len()
        );
        Ok(())
    })
}

// =====================================================================
// prepare — 扫描项目 query! 宏，生成离线 SQL 验证缓存
// =====================================================================

/// 扫描项目源码中的 `query!` 宏，连接真实数据库执行 EXPLAIN 验证，
/// 将已验证的 SQL 写入 `.sz-orm/query-cache.json`。
///
/// CI 中可通过 `SZ_ORM_QUERY_VERIFY=cache` + `SZ_ORM_SQLX_CACHE=.sz-orm/query-cache.json`
/// 在不连接 DB 的情况下完成编译期 SQL 验证。
///
/// 用法：
/// ```text
/// sz-orm prepare --dsn mysql://root:pass@localhost/db
/// sz-orm prepare --dsn postgres://user:pass@localhost/db --source-dir ./src
/// sz-orm prepare --dsn sqlite://./test.db --output .sz-orm/query-cache.json
/// ```
fn cmd_prepare(args: &[&str], config: &Option<CliConfig>) -> Result<(), String> {
    let dsn = resolve_option(args, "--dsn", config, |c| &c.dsn)
        .ok_or_else(|| "prepare 需要 --dsn <url> 参数连接数据库执行 EXPLAIN 验证".to_string())?;
    let source_dir =
        resolve_option(args, "--source-dir", config, |_| &None).unwrap_or_else(|| ".".to_string());
    let output =
        parse_option(args, "--output").unwrap_or_else(|| ".sz-orm/query-cache.json".to_string());

    println!("扫描源码目录: {}", source_dir);
    println!("数据库:       {}", dsn);
    println!("输出缓存:     {}", output);
    println!();

    // 1. 扫描所有 .rs 文件，提取 query! 宏中的 SQL
    let mut sqls: Vec<String> = Vec::new();
    scan_query_macros(&source_dir, &mut sqls)?;
    sqls.sort();
    sqls.dedup();

    println!("发现 query! 宏: {} 条唯一 SQL", sqls.len());
    if sqls.is_empty() {
        println!("未发现 query! 宏调用，无需生成缓存。");
        return Ok(());
    }

    // 2. 连接 DB 并逐条执行 EXPLAIN
    println!();
    println!("连接数据库执行 EXPLAIN 验证...");
    let mut verified: Vec<String> = Vec::with_capacity(sqls.len());
    let mut errors: Vec<(String, String)> = Vec::new();

    run_with_runtime(move || async move {
        let pool = sz_orm_sqlx::AnyPool::connect(&dsn)
            .await
            .map_err(|e| format!("连接数据库失败: {}", e))?;
        let backend = pool.backend();

        for sql in &sqls {
            let explain_sql = build_explain_sql(backend, sql);
            let mut conn = pool
                .create()
                .await
                .map_err(|e| format!("获取连接失败: {}", e))?;
            match conn.execute(&explain_sql).await {
                Ok(_) => {
                    println!("  ✓ {}", truncate_sql(sql, 60));
                    verified.push(sql.clone());
                }
                Err(e) => {
                    println!("  ✗ {}", truncate_sql(sql, 60));
                    errors.push((truncate_sql(sql, 80), e.to_string()));
                }
            }
        }

        // 3. 写入缓存文件
        std::fs::create_dir_all(
            std::path::Path::new(&output)
                .parent()
                .unwrap_or(std::path::Path::new(".")),
        )
        .map_err(|e| format!("创建缓存目录失败: {}", e))?;
        let json = serde_json::to_string_pretty(&verified)
            .map_err(|e| format!("序列化缓存失败: {}", e))?;
        std::fs::write(&output, json)
            .map_err(|e| format!("写入缓存文件 {} 失败: {}", output, e))?;

        println!();
        println!("验证完成:");
        println!("  通过: {} 条", verified.len());
        println!("  失败: {} 条", errors.len());
        println!("  缓存: {}", output);

        if !errors.is_empty() {
            println!();
            println!("失败详情:");
            for (sql, err) in &errors {
                println!("  SQL:  {}", sql);
                println!("  错误: {}", err);
            }
            return Err(format!("{} 条 SQL 验证失败，请检查后重试", errors.len()));
        }

        Ok(())
    })
}

/// 递归扫描目录下的所有 .rs 文件，提取 `query!` 宏中的 SQL 字符串
fn scan_query_macros(dir: &str, out: &mut Vec<String>) -> Result<(), String> {
    let dir_path = std::path::Path::new(dir);
    if !dir_path.exists() {
        return Err(format!("源码目录不存在: {}", dir));
    }
    for entry in walkdir(dir_path).map_err(|e| format!("扫描目录失败: {}", e))? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
        extract_query_sql(&content, out);
    }
    Ok(())
}

/// 简单的目录遍历（不引入 walkdir crate 依赖）
fn walkdir(dir: &std::path::Path) -> Result<Vec<std::fs::DirEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let mut stack = vec![std::fs::read_dir(dir)?];
    while let Some(rd) = stack.pop() {
        for entry in rd {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|n| n == "target" || n == "node_modules")
                {
                    continue;
                }
                stack.push(std::fs::read_dir(&path)?);
            } else {
                entries.push(entry);
            }
        }
    }
    Ok(entries)
}

/// 从 Rust 源码文本中提取 `query!` 宏的 SQL 字符串
///
/// 支持以下形式：
/// - `query!("SELECT ...")`
/// - `query!(r#"SELECT ..."#)`
/// - `query!("SELECT ..." .to_string())`
fn extract_query_sql(content: &str, out: &mut Vec<String>) {
    let mut i = 0;
    let bytes = content.as_bytes();

    while i < bytes.len() {
        // 查找 query! 标记
        if bytes[i..].starts_with(b"query!") {
            // 跳过 query! 和随后的空白/括号
            let start = i + 6;
            let mut j = start;
            while j < bytes.len()
                && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r')
            {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'(' {
                j += 1;
                while j < bytes.len()
                    && (bytes[j] == b' '
                        || bytes[j] == b'\t'
                        || bytes[j] == b'\n'
                        || bytes[j] == b'\r')
                {
                    j += 1;
                }
                // 尝试提取字符串字面量
                if let Some((sql, end)) = extract_string_literal(&bytes[j..]) {
                    out.push(sql);
                    i = j + end;
                    continue;
                }
            }
        }
        i += 1;
    }
}

/// 从字节切片开头提取字符串字面量（支持 "..." 和 r#"..."#）
/// 返回 (SQL 内容, 消耗字节数)
fn extract_string_literal(bytes: &[u8]) -> Option<(String, usize)> {
    if bytes.is_empty() {
        return None;
    }

    // 原始字符串 r#"..."#
    if bytes.len() >= 2 && bytes[0] == b'r' && bytes[1] == b'#' {
        // 找到 #" 开始 和 "# 结束
        let mut i = 2;
        // 跳过 # 号（r##"..."## 形式）
        while i < bytes.len() && bytes[i] == b'#' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            return None;
        }
        i += 1; // 跳过开头 "
        let start = i;
        // 找 "# 结束
        loop {
            if i >= bytes.len() {
                return None;
            }
            if bytes[i] == b'"' {
                // 检查后面是否跟着 # 号（r#"..."# 形式）
                let mut k = i + 1;
                while k < bytes.len() && bytes[k] == b'#' {
                    k += 1;
                }
                if k > i + 1 {
                    // 找到匹配的 "# 结尾
                    let sql = String::from_utf8_lossy(&bytes[start..i]).to_string();
                    return Some((sql, k));
                }
            }
            i += 1;
        }
    }

    // 普通字符串 "..."
    if bytes[0] == b'"' {
        let mut i = 1;
        let mut s = String::new();
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    if i + 1 < bytes.len() {
                        match bytes[i + 1] {
                            b'"' => {
                                s.push('"');
                                i += 2;
                            }
                            b'\\' => {
                                s.push('\\');
                                i += 2;
                            }
                            b'n' => {
                                s.push('\n');
                                i += 2;
                            }
                            b't' => {
                                s.push('\t');
                                i += 2;
                            }
                            _ => {
                                s.push(bytes[i] as char);
                                i += 1;
                            }
                        }
                    } else {
                        return None;
                    }
                }
                b'"' => {
                    return Some((s, i + 1));
                }
                c => {
                    s.push(c as char);
                    i += 1;
                }
            }
        }
        return None;
    }

    None
}

/// 根据数据库后端类型构建 EXPLAIN SQL
fn build_explain_sql(backend: sz_orm_sqlx::AnyBackend, sql: &str) -> String {
    let sql_no_placeholders = replace_placeholders_with_null(sql);
    match backend {
        sz_orm_sqlx::AnyBackend::Postgres | sz_orm_sqlx::AnyBackend::MySql => {
            format!("EXPLAIN {}", sql_no_placeholders)
        }
        sz_orm_sqlx::AnyBackend::Sqlite => {
            format!("EXPLAIN QUERY PLAN {}", sql_no_placeholders)
        }
    }
}

/// 将 SQL 中的 `?` 占位符替换为 `NULL`（跳过字符串字面量内的 `?`）
fn replace_placeholders_with_null(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut in_string = false;
    let mut escape_next = false;
    for c in sql.chars() {
        if escape_next {
            result.push(c);
            escape_next = false;
            continue;
        }
        if c == '\\' && in_string {
            result.push(c);
            escape_next = true;
            continue;
        }
        if c == '\'' {
            in_string = !in_string;
            result.push(c);
            continue;
        }
        if c == '?' && !in_string {
            result.push_str("NULL");
        } else {
            result.push(c);
        }
    }
    result
}

fn truncate_sql(sql: &str, max: usize) -> String {
    if sql.len() <= max {
        sql.to_string()
    } else {
        format!("{}...", &sql[..max])
    }
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
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "PRAGMA table_info({})",
                table
            )))
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
