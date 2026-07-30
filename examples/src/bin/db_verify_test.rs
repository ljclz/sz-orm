//! 编译期连真 DB 验证示例 — query! 宏 db-verify 功能演示
//!
//! 本示例展示 `query!` 宏在启用 `db-verify` feature 且设置
//! `SZ_ORM_QUERY_VERIFY=1` 环境变量时，会在编译期连接真实数据库
//! 执行 EXPLAIN/EXPLAIN PLAN FOR，验证 SQL 语法与表/列存在性。
//!
//! 支持的数据库（5 种）：
//! - MySQL：`DATABASE_URL=mysql://user:pass@host:3306/db`
//! - PostgreSQL：`DATABASE_URL=postgres://user:pass@host:5432/db`
//! - SQLite：`DATABASE_URL=sqlite::memory:`
//! - Oracle：`DATABASE_URL=oracle://user:pass@host:port/service?sysdba=1`
//! - SQL Server：`DATABASE_URL=sqlserver://user:pass@host:port/db`
//!
//! 运行步骤：
//! 1. 准备 users 表（id, name, email, age, created_at）
//! 2. 设置环境变量：
//!    ```powershell
//!    $env:DATABASE_URL = "mysql://root:test123@127.0.0.1:3306/sz_orm_test"
//!    $env:SZ_ORM_QUERY_VERIFY = "1"
//!    ```
//! 3. 编译并运行：
//!    ```powershell
//!    cargo run -p sz-orm-examples --features sz-orm-macros/db-verify --bin db_verify_test
//!    ```
//!
//! 若 SQL 语法错误或引用了不存在的表/列，编译期就会报错。

use sz_orm_macros::query;

fn main() {
    // 5 条 SQL 语句，编译期连真 DB 执行 EXPLAIN 验证
    let sql1 = query!("SELECT id, name FROM users WHERE id = ?");
    println!("Verified SQL 1 (SELECT): {}", sql1);

    let sql2 = query!("SELECT id, name, email, age FROM users ORDER BY id");
    println!("Verified SQL 2 (SELECT all): {}", sql2);

    let sql3 = query!("INSERT INTO users (name, email, age) VALUES (?, ?, ?)");
    println!("Verified SQL 3 (INSERT): {}", sql3);

    let sql4 = query!("UPDATE users SET age = ? WHERE id = ?");
    println!("Verified SQL 4 (UPDATE): {}", sql4);

    let sql5 = query!("DELETE FROM users WHERE id = ?");
    println!("Verified SQL 5 (DELETE): {}", sql5);

    println!("\n=== 所有 5 条 SQL 编译期真 DB 验证通过 ===");
}
