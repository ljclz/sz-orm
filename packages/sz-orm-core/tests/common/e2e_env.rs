//! v8.3.0: 真实数据库 e2e 测试环境辅助函数
//!
//! 提供 3 个数据库（MySQL/PostgreSQL/Oracle）的连接辅助函数，
//! 读取 `SZ_ORM_E2E_*` 环境变量，连接失败返回 `None`（跳过测试，非 panic）。
//!
//! 环境变量清单：
//! - `SZ_ORM_E2E_MYSQL_URL_TEST` / `SZ_ORM_E2E_MYSQL_URL_SHOP` / `SZ_ORM_E2E_MYSQL_URL_NJSZJT`
//! - `SZ_ORM_E2E_PG_URL`
//! - `SZ_ORM_E2E_ORACLE_USER` / `SZ_ORM_E2E_ORACLE_PASSWORD` / `SZ_ORM_E2E_ORACLE_CONNECT_STRING`
//!
//! 旧变量兼容：`MYSQL_URL` / `POSTGRES_URL` / `SZ_ORM_ORACLE_*` 仍可读取。

use oracle::Connection as OracleConn;
use oracle::Connector;
use oracle::Privilege;

/// MySQL 数据库名称枚举，对应第二台服务器 3 个库
pub enum E2eMysqlDb {
    Test,
    Shop,
    Njszjt,
}

impl E2eMysqlDb {
    fn env_var(&self) -> &'static str {
        match self {
            E2eMysqlDb::Test => "SZ_ORM_E2E_MYSQL_URL_TEST",
            E2eMysqlDb::Shop => "SZ_ORM_E2E_MYSQL_URL_SHOP",
            E2eMysqlDb::Njszjt => "SZ_ORM_E2E_MYSQL_URL_NJSZJT",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            E2eMysqlDb::Test => "test",
            E2eMysqlDb::Shop => "shop",
            E2eMysqlDb::Njszjt => "njszjt",
        }
    }
}

/// 获取 MySQL 连接池（第二台服务器 121.204.253.75:8802）
///
/// 读取 `SZ_ORM_E2E_MYSQL_URL_{DB}` 环境变量。
/// 环境变量未设置或连接失败返回 `None`（跳过测试，非 panic）。
pub async fn e2e_mysql_pool(db: E2eMysqlDb) -> Option<sqlx::MySqlPool> {
    let url = match std::env::var(db.env_var()) {
        Ok(u) => u,
        Err(_) => {
            eprintln!(
                "E2E_MYSQL_UNREACHABLE: 环境变量 {} 未设置，跳过 MySQL {} 库 e2e 测试",
                db.env_var(),
                db.label()
            );
            return None;
        }
    };
    match sqlx::MySqlPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!(
                "E2E_MYSQL_UNREACHABLE: MySQL {} 库连接失败 ({}): {}",
                db.label(),
                db.env_var(),
                e
            );
            None
        }
    }
}

/// 获取 PostgreSQL 连接池（第二台服务器）
///
/// 读取 `SZ_ORM_E2E_PG_URL` 环境变量，回退到 `POSTGRES_URL`。
/// 环境变量未设置或连接失败返回 `None`（跳过测试，非 panic）。
pub async fn e2e_pg_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("SZ_ORM_E2E_PG_URL")
        .or_else(|_| std::env::var("POSTGRES_URL"))
        .ok();
    let url = match url {
        Some(u) => u,
        None => {
            eprintln!(
                "E2E_PG_UNREACHABLE: 环境变量 SZ_ORM_E2E_PG_URL / POSTGRES_URL 均未设置，跳过 PostgreSQL e2e 测试"
            );
            return None;
        }
    };
    match sqlx::PgPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!("E2E_PG_UNREACHABLE: PostgreSQL 连接失败: {}", e);
            None
        }
    }
}

/// 获取 Oracle 连接（本机 23ai Free，sys/Sysdba）
///
/// 读取 `SZ_ORM_E2E_ORACLE_USER` / `SZ_ORM_E2E_ORACLE_PASSWORD` / `SZ_ORM_E2E_ORACLE_CONNECT_STRING`，
/// 回退到 `SZ_ORM_ORACLE_*` 旧变量，再回退到默认值 `sys` / `test123` / `127.0.0.1:1521/freepdb1.FALSE`。
///
/// 当用户为 `sys` 时自动使用 `Privilege::Sysdba`，否则使用普通连接。
pub fn e2e_oracle_conn() -> Option<OracleConn> {
    let user = std::env::var("SZ_ORM_E2E_ORACLE_USER")
        .or_else(|_| std::env::var("SZ_ORM_ORACLE_USER"))
        .unwrap_or_else(|_| "sys".to_string());
    let password = std::env::var("SZ_ORM_E2E_ORACLE_PASSWORD")
        .or_else(|_| std::env::var("SZ_ORM_ORACLE_PASSWORD"))
        .unwrap_or_else(|_| "test123".to_string());
    let connect_string = std::env::var("SZ_ORM_E2E_ORACLE_CONNECT_STRING")
        .or_else(|_| std::env::var("SZ_ORM_ORACLE_CONNECT_STRING"))
        .unwrap_or_else(|_| "127.0.0.1:1521/freepdb1.FALSE".to_string());

    let is_sys = user.eq_ignore_ascii_case("sys");
    let result = if is_sys {
        Connector::new(user, password, connect_string)
            .privilege(Privilege::Sysdba)
            .connect()
    } else {
        OracleConn::connect(user, password, connect_string)
    };
    match result {
        Ok(conn) => Some(conn),
        Err(e) => {
            eprintln!(
                "E2E_ORACLE_UNREACHABLE: Oracle 连接失败 (127.0.0.1:1521): {}",
                e
            );
            None
        }
    }
}
