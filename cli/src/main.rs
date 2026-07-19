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
use sz_orm_core::{DbType, FileMigrationResolver, MigrationContext, Migrator, MigrationResolver};
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
    sql:validate <sql>            校验 SQL 语法 + 注入检测
    dialect list                  列出所有支持的方言
    dialect show <db_type>        显示指定方言详情
    help, --help, -h              显示本帮助
    --version, -V                 显示版本号

选项:
    --migrations <dir>            迁移文件目录（默认 ./migrations）
    --output <dir>                生成代码输出目录（默认 ./src/models 或 ./migrations）

示例:
    sz-orm info
    sz-orm dialect list
    sz-orm dialect show mysql
    sz-orm make:migration create_users --output ./migrations
    sz-orm make:model User --output ./src/models
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
        println!(
            "  - {:<12} (默认端口 {})",
            db.as_str(),
            db.default_port()
        );
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
    let migrations_dir = parse_option(args, "--migrations").unwrap_or_else(|| "./migrations".to_string());

    let resolver = FileMigrationResolver::new(PathBuf::from(&migrations_dir));
    let migrations = resolver
        .resolve(DbType::PostgreSQL)
        .map_err(|e| format!("解析迁移目录失败: {}", e))?;

    if migrations.is_empty() {
        println!("迁移目录 {} 中没有发现迁移文件", migrations_dir);
        return Ok(());
    }

    let migrator = Migrator::new(MigrationContext::default())
        .add_migrations(migrations);

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
    let migrations_dir = parse_option(args, "--migrations").unwrap_or_else(|| "./migrations".to_string());

    let resolver = FileMigrationResolver::new(PathBuf::from(&migrations_dir));
    let migrations = resolver
        .resolve(DbType::PostgreSQL)
        .map_err(|e| format!("解析迁移目录失败: {}", e))?;

    let migrator = Migrator::new(MigrationContext::default()).add_migrations(migrations);
    let progress = migrator.progress();

    println!("迁移目录: {}", migrations_dir);
    println!("总计: {}  已应用: {}  待执行: {}",
             progress.total, progress.applied, progress.pending);
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
    let output_dir = parse_option(args, "--output")
        .unwrap_or_else(|| "./migrations".to_string());

    fs::create_dir_all(&output_dir)
        .map_err(|e| format!("创建目录 {} 失败: {}", output_dir, e))?;

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
    let output_dir = parse_option(args, "--output")
        .unwrap_or_else(|| "./src/models".to_string());

    fs::create_dir_all(&output_dir)
        .map_err(|e| format!("创建目录 {} 失败: {}", output_dir, e))?;

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
    fs::write(&path, code)
        .map_err(|e| format!("写入 {} 失败: {}", path.display(), e))?;

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
                let status = if get_dialect(*db).is_ok() { "SQL" } else { "NoSQL" };
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
            let db = DbType::from_str(args[1])
                .ok_or_else(|| format!("未知数据库类型: {}", args[1]))?;
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
    if b { "是" } else { "否" }
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
    } else if s.ends_with('y') && !s.ends_with("ay") && !s.ends_with("ey") && !s.ends_with("iy")
        && !s.ends_with("oy") && !s.ends_with("uy")
    {
        format!("{}ies", &s[..s.len() - 1])
    } else {
        format!("{}s", s)
    }
}
