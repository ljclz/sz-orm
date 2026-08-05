//! DuckDB 真实数据库集成测试
//!
//! 使用 duckdb crate（bundled）直接验证 sz-orm-core 的 DuckDBDialect：
//! 建表、参数化插入、INSERT OR IGNORE、分页等生成的 SQL 在真实 DuckDB 上可执行。

use duckdb::{params, Connection};
use sz_orm_core::dialect::{get_dialect, ColumnDef, TableChange};
use sz_orm_core::DbType;

fn dialect() -> Box<dyn sz_orm_core::dialect::Dialect> {
    get_dialect(DbType::DuckDB).expect("DuckDB 方言可用")
}

/// 内存 DuckDB 连接（每个测试独立，避免并行冲突）
fn open_conn() -> Connection {
    Connection::open_in_memory().expect("打开内存 DuckDB")
}

#[test]
fn duckdb_create_table_executes() {
    let conn = open_conn();
    let d = dialect();
    let cols = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "DOUBLE".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let sql = d.build_create_table("users", &cols);
    assert!(sql.contains("\"users\""), "应使用双引号标识符: {}", sql);
    conn.execute(&sql, [])
        .expect("CREATE TABLE 应在 DuckDB 上成功");
}

#[test]
fn duckdb_insert_and_select() {
    let conn = open_conn();

    conn.execute(
        "CREATE TABLE users (id BIGINT PRIMARY KEY, name VARCHAR, value DOUBLE)",
        [],
    )
    .unwrap();

    // 参数化插入
    let sql = "INSERT INTO users (id, name, value) VALUES (?, ?, ?)";
    conn.execute(sql, params![1i64, "Alice", 100.5f64]).unwrap();
    conn.execute(sql, params![2i64, "Bob", 200.0f64]).unwrap();

    let mut stmt = conn
        .prepare("SELECT id, name, value FROM users ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], (1, "Alice".to_string(), 100.5));
}

#[test]
fn duckdb_insert_or_ignore_prefix() {
    let conn = open_conn();
    let d = dialect();
    conn.execute("CREATE TABLE t (id BIGINT PRIMARY KEY)", [])
        .unwrap();

    let prefix = d.build_insert_or_ignore_prefix("t");
    assert_eq!(
        prefix, "INSERT OR IGNORE INTO \"t\"",
        "DuckDB 应生成 INSERT OR IGNORE"
    );

    // 第一次插入成功
    let sql = format!("{} (id) VALUES (?)", prefix);
    conn.execute(&sql, params![1i64]).unwrap();
    // 重复插入被忽略（不报错）
    conn.execute(&sql, params![1i64]).unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "重复插入应被忽略");
}

#[test]
fn duckdb_pagination_sql() {
    let d = dialect();
    let sql = d.build_pagination("SELECT * FROM users", 2, 10);
    assert_eq!(sql, "SELECT * FROM users LIMIT 10 OFFSET 10");
}

#[test]
fn duckdb_alter_table_executes() {
    let conn = open_conn();
    let d = dialect();
    conn.execute("CREATE TABLE users (id BIGINT)", []).unwrap();

    let changes = vec![TableChange::AddColumn(ColumnDef {
        name: "age".to_string(),
        sql_type: "INTEGER".to_string(),
        nullable: true,
        default: None,
        auto_increment: false,
        primary_key: false,
    })];
    let sql = d.build_alter_table("users", &changes);
    assert!(
        sql.contains("ALTER TABLE \"users\" ADD COLUMN \"age\" INTEGER"),
        "{}",
        sql
    );
    conn.execute(&sql, [])
        .expect("ALTER TABLE 应在 DuckDB 上成功");
}

#[test]
fn duckdb_escape_string() {
    let d = dialect();
    // DuckDB 标准转义：单引号双写
    assert_eq!(d.escape_string("O'Brien"), "O''Brien");
    assert_eq!(d.escape_string("plain"), "plain");
}

#[test]
fn duckdb_build_drop_table() {
    let conn = open_conn();
    let d = dialect();
    conn.execute("CREATE TABLE t (id BIGINT)", []).unwrap();
    let sql = d.build_drop_table("t", true);
    assert_eq!(sql, "DROP TABLE IF EXISTS \"t\"");
    conn.execute(&sql, [])
        .expect("DROP TABLE 应在 DuckDB 上成功");
}
