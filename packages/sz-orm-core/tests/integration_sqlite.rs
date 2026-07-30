//! SQLite 真实数据库集成测试
//!
//! 使用 rusqlite (bundled SQLite) 直接验证 sz-orm-core 的 SQLite 方言、
//! 值转换、SQL 转义、事务、连接池语义、分页、SQL 注入防护等核心功能。
//!
//! 超大数据量场景：10 万条记录 CRUD、8 任务并发读写、批量插入性能基线。

use rusqlite::{params, Connection as RusqliteConn};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;
use sz_orm_core::Value;
use sz_orm_core::QueryBuilder;

/// 唯一临时文件路径（避免并行测试冲突）
static SQLITE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 测试数据目录：优先 F:\test\data（用户规范），回退到环境变量或系统 temp（CI/Linux）
///
/// 注意：仅检查目录存在不足以保证可用——还需验证可写性，
/// 以避免在受限沙箱环境中因目录存在但不可写导致测试失败。
fn test_data_dir() -> std::path::PathBuf {
    let f_drive = std::path::Path::new("F:\\test\\data");
    if is_dir_writable(f_drive) {
        return f_drive.to_path_buf();
    }
    if let Ok(dir) = std::env::var("SZ_ORM_TEST_DATA_DIR") {
        let p = std::path::PathBuf::from(&dir);
        if is_dir_writable(&p) {
            return p;
        }
    }
    std::env::temp_dir()
}

/// 检查目录是否存在且可写：尝试在其中创建并删除一个探测文件
fn is_dir_writable(dir: &std::path::Path) -> bool {
    if !dir.exists() {
        return false;
    }
    let probe = dir.join(format!(".probe_{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn temp_sqlite_path() -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = SQLITE_COUNTER.fetch_add(1, Ordering::Relaxed);
    test_data_dir()
        .join(format!(
            "sz_orm_int_sqlite_{}_{}_{}.db",
            pid, nanos, counter
        ))
        .to_string_lossy()
        .to_string()
}

/// 打开一个新的 SQLite 连接（内存模式，避免 CI 磁盘 I/O 问题）
fn open_conn() -> RusqliteConn {
    let conn = RusqliteConn::open_in_memory().expect("open sqlite in-memory");
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "synchronous", "NORMAL").ok();
    conn
}

/// 使用方言生成 CREATE TABLE 并执行
fn create_test_table(conn: &RusqliteConn, table: &str) {
    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "INTEGER".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "TEXT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "INTEGER".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "data".to_string(),
            sql_type: "TEXT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let sql = dialect.build_create_table(table, &columns);
    conn.execute(&sql, []).expect("create table");
}

#[test]
fn test_sqlite_dialect_quote_and_escape() {
    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    // quote
    assert_eq!(dialect.quote("user"), "\"user\"");
    assert_eq!(dialect.quote("with\"quote"), "\"with\"\"quote\"");
    // escape_string
    assert_eq!(dialect.escape_string("it's"), "it''s");
    assert_eq!(dialect.escape_string("back\\slash"), "back\\slash");
    // supports_returning
    assert!(dialect.supports_returning());
    // auto_increment keyword
    assert_eq!(dialect.auto_increment_keyword(), "AUTOINCREMENT");
}

#[test]
fn test_sqlite_create_insert_select() {
    let conn = open_conn();
    create_test_table(&conn, "t1");

    // 插入 3 条记录
    conn.execute(
        "INSERT INTO t1 (name, value, data) VALUES (?1, ?2, ?3)",
        params!["alice", 100i64, "data1"],
    )
    .expect("insert 1");
    conn.execute(
        "INSERT INTO t1 (name, value, data) VALUES (?1, ?2, ?3)",
        params!["bob", 200i64, "data2"],
    )
    .expect("insert 2");
    conn.execute(
        "INSERT INTO t1 (name, value, data) VALUES (?1, ?2, ?3)",
        params!["carol", 300i64, "data3"],
    )
    .expect("insert 3");

    // SELECT 全部
    let mut stmt = conn
        .prepare("SELECT id, name, value FROM t1 ORDER BY id")
        .unwrap();
    let rows: Vec<(i64, String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("query")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].1, "alice");
    assert_eq!(rows[2].1, "carol");

    // Value 类型转换验证
    let v_str = Value::String("alice".to_string());
    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let escaped = dialect.escape_string(v_str.as_str().unwrap());
    let mut stmt2 = conn
        .prepare(&format!("SELECT value FROM t1 WHERE name = '{}'", escaped))
        .unwrap();
    let value: i64 = stmt2.query_row([], |row| row.get(0)).expect("query row");
    assert_eq!(value, 100);
}

#[test]
fn test_sqlite_bulk_insert_100k() {
    let conn = open_conn();
    create_test_table(&conn, "t_bulk");

    let total: usize = 100_000;
    let start = Instant::now();
    conn.execute("BEGIN", []).expect("begin");
    {
        let mut stmt = conn
            .prepare("INSERT INTO t_bulk (name, value, data) VALUES (?1, ?2, ?3)")
            .expect("prepare");
        for i in 0..total {
            stmt.execute(params![
                format!("user_{}", i),
                i as i64,
                format!("data_{}", i % 1000)
            ])
            .expect("insert");
        }
    }
    conn.execute("COMMIT", []).expect("commit");
    let elapsed = start.elapsed();
    println!(
        "sqlite bulk insert {} rows in {:?} ({:.0} rows/s)",
        total,
        elapsed,
        total as f64 / elapsed.as_secs_f64()
    );

    // 验证总数
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_bulk", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count as usize, total);

    // 验证末尾数据
    let last_name: String = conn
        .query_row(
            "SELECT name FROM t_bulk WHERE value = ?1",
            params![(total - 1) as i64],
            |row| row.get(0),
        )
        .expect("query last");
    assert_eq!(last_name, format!("user_{}", total - 1));
}

#[test]
fn test_sqlite_update_delete() {
    let conn = open_conn();
    create_test_table(&conn, "t_ud");

    // 准备 1000 条数据
    conn.execute("BEGIN", []).expect("begin");
    for i in 0..1000i64 {
        conn.execute(
            "INSERT INTO t_ud (name, value, data) VALUES (?1, ?2, ?3)",
            params![format!("n_{}", i), i, "x"],
        )
        .expect("insert");
    }
    conn.execute("COMMIT", []).expect("commit");

    // UPDATE
    let affected = conn
        .execute("UPDATE t_ud SET value = value + 1000 WHERE value < 100", [])
        .expect("update");
    assert_eq!(affected, 100);

    // DELETE
    let deleted = conn
        .execute("DELETE FROM t_ud WHERE value >= 1000", [])
        .expect("delete");
    assert_eq!(deleted, 100);

    // 验证总数
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_ud", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 900);
}

#[test]
fn test_sqlite_transaction_commit() {
    let conn = open_conn();
    create_test_table(&conn, "t_tc");

    conn.execute("BEGIN", []).expect("begin");
    conn.execute(
        "INSERT INTO t_tc (name, value, data) VALUES (?1, ?2, ?3)",
        params!["commit_row", 1i64, "c"],
    )
    .expect("insert");
    conn.execute("COMMIT", []).expect("commit");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_tc", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_sqlite_transaction_rollback() {
    let conn = open_conn();
    create_test_table(&conn, "t_tr");

    conn.execute("BEGIN", []).expect("begin");
    conn.execute(
        "INSERT INTO t_tr (name, value, data) VALUES (?1, ?2, ?3)",
        params!["rollback_row", 1i64, "r"],
    )
    .expect("insert");
    // 模拟业务失败，回滚
    conn.execute("ROLLBACK", []).expect("rollback");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_tr", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0, "rollback should leave table empty");
}

#[test]
fn test_sqlite_pagination() {
    let conn = open_conn();
    create_test_table(&conn, "t_page");

    conn.execute("BEGIN", []).expect("begin");
    for i in 0..1000i64 {
        conn.execute(
            "INSERT INTO t_page (name, value, data) VALUES (?1, ?2, ?3)",
            params![format!("p_{}", i), i, "p"],
        )
        .expect("insert");
    }
    conn.execute("COMMIT", []).expect("commit");

    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let page_size = 50u64;
    let mut total_fetched = 0u64;
    let mut last_value = -1i64;
    for page in 1..=20 {
        let sql =
            dialect.build_pagination("SELECT value FROM t_page ORDER BY value", page, page_size);
        let mut stmt = conn.prepare(&sql).unwrap();
        let rows: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(rows.len() as u64, page_size, "page {} size mismatch", page);
        // 严格单调递增
        for v in rows {
            assert!(
                v > last_value,
                "pagination order violated: {} <= {}",
                v,
                last_value
            );
            last_value = v;
            total_fetched += 1;
        }
    }
    assert_eq!(total_fetched, 1000);
}

#[test]
fn test_sqlite_sql_injection_protection() {
    let conn = open_conn();
    create_test_table(&conn, "t_inj");

    conn.execute(
        "INSERT INTO t_inj (name, value, data) VALUES (?1, ?2, ?3)",
        params!["alice", 1i64, "x"],
    )
    .expect("insert");

    // 恶意输入：尝试通过字符串字面量注入
    let malicious = "alice' OR '1'='1";
    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let escaped = dialect.escape_string(malicious);

    // 验证：escaped 后只能匹配原始 alice 行（实际匹配不到，因为 alice' OR '1'='1 不存在）
    let sql = format!("SELECT COUNT(*) FROM t_inj WHERE name = '{}'", escaped);
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0)).unwrap();
    assert_eq!(count, 0, "escaped malicious input should match nothing");

    // 对比：不转义会注入（这条 SQL 实际会返回 1，因为 '1'='1' 恒真）
    let unescaped_sql = format!("SELECT COUNT(*) FROM t_inj WHERE name = '{}'", malicious);
    let count_unescaped: i64 = conn
        .query_row(&unescaped_sql, [], |row| row.get(0))
        .unwrap();
    assert_eq!(count_unescaped, 1, "unescaped input should be injectable");
}

#[test]
fn test_sqlite_savepoint_nested() {
    let conn = open_conn();
    create_test_table(&conn, "t_sp");

    conn.execute("BEGIN", []).expect("begin");
    conn.execute(
        "INSERT INTO t_sp (name, value, data) VALUES (?1, ?2, ?3)",
        params!["outer", 1i64, "o"],
    )
    .expect("insert outer");

    // SAVEPOINT 1
    conn.execute("SAVEPOINT sp1", []).expect("sp1");
    conn.execute(
        "INSERT INTO t_sp (name, value, data) VALUES (?1, ?2, ?3)",
        params!["inner1", 2i64, "i1"],
    )
    .expect("insert inner1");
    // 回滚到 sp1，inner1 应消失
    conn.execute("ROLLBACK TO sp1", []).expect("rollback sp1");
    conn.execute("RELEASE sp1", []).expect("release sp1");

    // SAVEPOINT 2
    conn.execute("SAVEPOINT sp2", []).expect("sp2");
    conn.execute(
        "INSERT INTO t_sp (name, value, data) VALUES (?1, ?2, ?3)",
        params!["inner2", 3i64, "i2"],
    )
    .expect("insert inner2");
    conn.execute("RELEASE sp2", []).expect("release sp2");

    conn.execute("COMMIT", []).expect("commit");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_sp", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2, "should have outer + inner2 (inner1 rolled back)");

    let names: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM t_sp ORDER BY id").unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };
    assert_eq!(names, vec!["outer".to_string(), "inner2".to_string()]);
}

#[test]
fn test_sqlite_concurrent_8tasks_10k_ops() {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let path = temp_sqlite_path();
    // 主连接初始化表 + 预填充 10000 条
    {
        let conn = RusqliteConn::open(&path).expect("open");
        create_test_table(&conn, "t_conc");
        // 启用 WAL 模式 + 设置 busy_timeout（生产环境最佳实践）
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.busy_timeout(Duration::from_secs(30)).ok();
        conn.execute("BEGIN", []).expect("begin");
        for i in 0..10_000i64 {
            conn.execute(
                "INSERT INTO t_conc (name, value, data) VALUES (?1, ?2, ?3)",
                params![format!("u_{}", i), i, "init"],
            )
            .expect("insert");
        }
        conn.execute("COMMIT", []).expect("commit");
    }

    let path = Arc::new(path);
    let (tx, rx) = mpsc::channel();
    let ops_per_task: u64 = 10_000;

    for task_id in 0..8u64 {
        let path_clone = path.clone();
        let tx_clone = tx.clone();
        thread::spawn(move || {
            let conn = RusqliteConn::open(&*path_clone).expect("open");
            // 启用 busy_timeout：SQLite 在 WAL 模式下并发写入冲突时，
            // 内部自动等待最多 30s 而非立即返回 SQLITE_BUSY。
            // 这是 SQLite 多连接并发的生产用法。
            conn.busy_timeout(Duration::from_secs(30)).ok();
            let mut success = 0u64;
            let mut errors = 0u64;
            let mut retries = 0u64;
            for op in 0..ops_per_task {
                let key = (task_id * ops_per_task + op) as i64;
                loop {
                    let res = conn.execute(
                        "UPDATE t_conc SET data = ?1 WHERE value = ?2",
                        params![format!("task_{}_op_{}", task_id, op), key],
                    );
                    match res {
                        Ok(_) => {
                            success += 1;
                            break;
                        }
                        Err(e) => {
                            // SQLITE_BUSY (5) 或 SQLITE_LOCKED (6)：重试
                            // rusqlite::Error::SqliteFailure(ffi::Error, Option<String>)
                            let ext = match &e {
                                rusqlite::Error::SqliteFailure(err, _) => err.extended_code,
                                _ => 0,
                            };
                            if ext == 5 || ext == 6 {
                                retries += 1;
                                thread::sleep(Duration::from_millis(1));
                                continue;
                            }
                            errors += 1;
                            eprintln!(
                                "task {} op {} fatal error: {} (ext={})",
                                task_id, op, e, ext
                            );
                            break;
                        }
                    }
                }
            }
            tx_clone
                .send((task_id, success, errors, retries))
                .expect("send");
        });
    }
    drop(tx);

    let mut total_success = 0u64;
    let mut total_errors = 0u64;
    let mut total_retries = 0u64;
    for (task_id, success, errors, retries) in rx {
        println!(
            "task {} success={} errors={} retries={}",
            task_id, success, errors, retries
        );
        total_success += success;
        total_errors += errors;
        total_retries += retries;
    }
    println!(
        "sqlite concurrent totals: success={}, errors={}, retries={} (retries are expected under SQLite WAL single-writer constraint)",
        total_success, total_errors, total_retries
    );
    // SQLite WAL 模式下所有 8*10k 操作最终都必须成功（busy_timeout + retry 保证）
    assert_eq!(
        total_success,
        8 * ops_per_task,
        "all 8 tasks * 10k ops should succeed after retry"
    );
    assert_eq!(
        total_errors, 0,
        "no fatal errors allowed (busy retries are not errors)"
    );

    // 清理
    let path_str: &str = &path;
    let _ = std::fs::remove_file(path_str);
    let _ = std::fs::remove_file(format!("{}-wal", path_str));
    let _ = std::fs::remove_file(format!("{}-shm", path_str));
}

#[test]
fn test_sqlite_value_to_param_roundtrip() {
    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let conn = open_conn();
    create_test_table(&conn, "t_vp");

    // 使用 Value::to_param 生成 SQL 字面量插入
    let values: Vec<Value> = vec![
        Value::Null,
        Value::I64(42),
        Value::String("hello world".to_string()),
        Value::String("with'quote".to_string()),
        Value::Bool(true),
        Value::F64(2.5),
    ];

    for (i, v) in values.iter().enumerate() {
        let name_value = Value::String(format!("row_{}", i));
        let name_param = name_value.to_param();
        // 对于 Bool/F64/Null，先转换为字符串并 escape
        let data_str = match v {
            Value::Null => "NULL".to_string(),
            Value::Bool(b) => {
                if *b {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            Value::I64(n) => n.to_string(),
            Value::F64(f) => format!("{:.6}", f),
            Value::String(s) => format!("'{}'", dialect.escape_string(s)),
            _ => v.to_param().into_owned(),
        };
        let sql = format!(
            "INSERT INTO t_vp (name, value, data) VALUES ({}, {}, {})",
            name_param, i as i64, data_str
        );
        conn.execute(&sql, []).expect("insert value");
    }

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_vp", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count as usize, values.len());
}
// ============================================================================
// P2-6 真实数据库 Upsert 执行验证
//
// 以下测试与 `e2e_batch_upsert.rs`（仅 SQL 生成）形成互补，
// 验证 `build_batch_upsert_with_params` 生成的 SQL 在真实 SQLite 数据库
// 上的执行语义：冲突检测、更新生效、数据一致性。
// ============================================================================

/// 将 sz-orm Value 转换为 rusqlite 可接受的 Box<dyn ToSql>，
/// 用于参数化执行 build_batch_upsert_with_params 生成的 SQL。
fn value_to_rusqlite(v: &Value) -> Box<dyn rusqlite::ToSql> {
    match v {
        Value::Null => Box::new(rusqlite::types::Null),
        Value::Bool(b) => Box::new(*b),
        Value::I8(n) => Box::new(*n as i64),
        Value::I16(n) => Box::new(*n as i64),
        Value::I32(n) => Box::new(*n),
        Value::I64(n) => Box::new(*n),
        Value::U8(n) => Box::new(*n as i64),
        Value::U16(n) => Box::new(*n as i64),
        Value::U32(n) => Box::new(*n as i64),
        Value::U64(n) => Box::new(*n as i64),
        Value::F32(f) => Box::new(*f as f64),
        Value::F64(f) => Box::new(*f),
        Value::Decimal(s) | Value::String(s) | Value::Uuid(s) | Value::Date(s)
        | Value::DateTime(s) | Value::Time(s) | Value::Json(s) => Box::new(s.clone()),
        Value::Bytes(b) => Box::new(b.clone()),
        // Array/Object 在 SQLite 中以 JSON 字符串存储
        Value::Array(_) | Value::Object(_) => Box::new(serde_json::to_string(v).unwrap_or_default()),
        // Value 标记为 non-exhaustive，未来可能新增变体——统一回退为 NULL
        _ => Box::new(rusqlite::types::Null),
    }
}

/// 构造 upsert 测试用表（含主键 id + 唯一约束 name）
fn create_upsert_table(conn: &RusqliteConn, table: &str) {
    conn.execute(
        &format!(
            "CREATE TABLE {} (
                id   INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                age  INTEGER NOT NULL,
                email TEXT
            )",
            table
        ),
        [],
    )
    .expect("create upsert table");
}

#[test]
fn test_sqlite_upsert_basic_insert_path() {
    // 验证：初次 upsert 应作为 INSERT 生效（无冲突）
    let conn = open_conn();
    create_upsert_table(&conn, "t_upsert_basic");

    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let builder = QueryBuilder::<DummyModel>::new(dialect).table("t_upsert_basic");

    let rows = vec![row_for_upsert(1, "Alice", 30, "alice@t.com")];
    let (sql, params) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .expect("build upsert sql");

    // 执行 SQL：将 Value 转换为 rusqlite 参数
    let rusqlite_params: Vec<Box<dyn rusqlite::ToSql>> =
        params.iter().map(value_to_rusqlite).collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        rusqlite_params.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .expect("execute upsert insert");

    // 验证：1 行被插入
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_upsert_basic", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1, "首次 upsert 应插入 1 行");

    // 验证数据正确
    let (name, age, email): (String, i64, String) = conn
        .query_row(
            "SELECT name, age, email FROM t_upsert_basic WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(name, "Alice");
    assert_eq!(age, 30);
    assert_eq!(email, "alice@t.com");
}

#[test]
fn test_sqlite_upsert_conflict_update_path() {
    // 验证：主键冲突时，upsert 应更新而非插入新行
    let conn = open_conn();
    create_upsert_table(&conn, "t_upsert_conflict");

    // 1) 先插入一行
    conn.execute(
        "INSERT INTO t_upsert_conflict (id, name, age, email) VALUES (?1, ?2, ?3, ?4)",
        params![1i64, "Alice", 30i64, "alice@old.com"],
    )
    .expect("seed insert");

    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let builder = QueryBuilder::<DummyModel>::new(dialect).table("t_upsert_conflict");

    // 2) upsert 同一 id，新数据
    let rows = vec![row_for_upsert(1, "Alice", 31, "alice@new.com")];
    let (sql, params) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .expect("build upsert sql");

    let rusqlite_params: Vec<Box<dyn rusqlite::ToSql>> =
        params.iter().map(value_to_rusqlite).collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        rusqlite_params.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .expect("execute upsert update");

    // 验证：总行数仍为 1（无重复）
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_upsert_conflict", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1, "冲突时应更新而非插入新行");

    // 验证：age 和 email 已更新
    let (age, email): (i64, String) = conn
        .query_row(
            "SELECT age, email FROM t_upsert_conflict WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(age, 31, "age 应被更新");
    assert_eq!(email, "alice@new.com", "email 应被更新");
}

#[test]
fn test_sqlite_upsert_batch_mixed_insert_update() {
    // 验证：批量 upsert 中既有新行也有冲突行
    let conn = open_conn();
    create_upsert_table(&conn, "t_upsert_mix");

    // 预置 id=1 一行
    conn.execute(
        "INSERT INTO t_upsert_mix (id, name, age, email) VALUES (?1, ?2, ?3, ?4)",
        params![1i64, "Alice", 30i64, "alice@old.com"],
    )
    .expect("seed");

    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let builder = QueryBuilder::<DummyModel>::new(dialect).table("t_upsert_mix");

    // 批量 upsert：id=1 冲突（更新），id=2/id=3 新增（插入）
    let rows = vec![
        row_for_upsert(1, "Alice", 31, "alice@new.com"),
        row_for_upsert(2, "Bob", 25, "bob@t.com"),
        row_for_upsert(3, "Carol", 28, "carol@t.com"),
    ];
    let (sql, params) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .expect("build upsert sql");

    let rusqlite_params: Vec<Box<dyn rusqlite::ToSql>> =
        params.iter().map(value_to_rusqlite).collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        rusqlite_params.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .expect("execute batch upsert");

    // 验证：总行数为 3（1 已存在 + 2 新增）
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_upsert_mix", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 3, "应有 3 行（1 更新 + 2 新增）");

    // 验证 id=1 已更新
    let alice_age: i64 = conn
        .query_row(
            "SELECT age FROM t_upsert_mix WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(alice_age, 31, "Alice 的 age 应已更新");

    // 验证 id=2/id=3 已插入
    let bob_name: String = conn
        .query_row(
            "SELECT name FROM t_upsert_mix WHERE id = 2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bob_name, "Bob");

    let carol_name: String = conn
        .query_row(
            "SELECT name FROM t_upsert_mix WHERE id = 3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(carol_name, "Carol");
}

#[test]
fn test_sqlite_upsert_specific_update_columns_only() {
    // 验证：指定 update_columns 时，仅更新指定列，其他列保持不变
    let conn = open_conn();
    create_upsert_table(&conn, "t_upsert_cols");

    // 预置一行
    conn.execute(
        "INSERT INTO t_upsert_cols (id, name, age, email) VALUES (?1, ?2, ?3, ?4)",
        params![1i64, "Alice", 30i64, "alice@keep.com"],
    )
    .expect("seed");

    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let builder = QueryBuilder::<DummyModel>::new(dialect).table("t_upsert_cols");

    // 仅更新 age，email 保持不变
    let rows = vec![row_for_upsert(1, "Alice", 99, "alice@changed.com")];
    let (sql, params) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &["age"])
        .expect("build upsert sql with specific columns");

    let rusqlite_params: Vec<Box<dyn rusqlite::ToSql>> =
        params.iter().map(value_to_rusqlite).collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        rusqlite_params.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .expect("execute upsert");

    // 验证：age 已更新为 99
    let age: i64 = conn
        .query_row(
            "SELECT age FROM t_upsert_cols WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(age, 99, "age 应被更新");

    // 验证：email 保持不变（未在 update_columns 中）
    let email: String = conn
        .query_row(
            "SELECT email FROM t_upsert_cols WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(email, "alice@keep.com", "email 不应被更新");
}

#[test]
fn test_sqlite_upsert_null_value_handling() {
    // 验证：upsert 处理 NULL 值（从有值更新为 NULL）
    let conn = open_conn();
    create_upsert_table(&conn, "t_upsert_null");

    // 预置一行有 email
    conn.execute(
        "INSERT INTO t_upsert_null (id, name, age, email) VALUES (?1, ?2, ?3, ?4)",
        params![1i64, "Alice", 30i64, "alice@t.com"],
    )
    .expect("seed");

    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let builder = QueryBuilder::<DummyModel>::new(dialect).table("t_upsert_null");

    // upsert 将 email 设为 NULL
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    row.insert("age".to_string(), Value::I32(30));
    row.insert("email".to_string(), Value::Null);

    let (sql, params) = builder
        .build_batch_upsert_with_params(&[row], &["id"], &["email"])
        .expect("build upsert sql with null");

    let rusqlite_params: Vec<Box<dyn rusqlite::ToSql>> =
        params.iter().map(value_to_rusqlite).collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        rusqlite_params.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .expect("execute upsert with null");

    // 验证：email 已变为 NULL
    let email: Option<String> = conn
        .query_row(
            "SELECT email FROM t_upsert_null WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(email.is_none(), "email 应为 NULL");
}

#[test]
fn test_sqlite_upsert_unicode_and_special_chars() {
    // 验证：upsert 处理 Unicode 和特殊字符（防 SQL 注入 + 数据完整性）
    let conn = open_conn();
    create_upsert_table(&conn, "t_upsert_uni");

    let dialect = get_dialect(DbType::Sqlite).expect("sqlite dialect");
    let builder = QueryBuilder::<DummyModel>::new(dialect).table("t_upsert_uni");

    // 第一行：Unicode + 引号（尝试注入）
    let rows = vec![row_for_upsert(
        1,
        "张三'; DROP TABLE t_upsert_uni; --",
        25,
        "zhang's@example.com",
    )];
    let (sql, params) = builder
        .build_batch_upsert_with_params(&rows, &["id"], &[])
        .expect("build upsert sql");

    let rusqlite_params: Vec<Box<dyn rusqlite::ToSql>> =
        params.iter().map(value_to_rusqlite).collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        rusqlite_params.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .expect("execute upsert with unicode");

    // 验证：表未被删除（参数化绑定阻止注入）
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_upsert_uni", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1, "表应仍存在且有 1 行");

    // 验证：name 完整保留（含特殊字符）
    let name: String = conn
        .query_row(
            "SELECT name FROM t_upsert_uni WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name, "张三'; DROP TABLE t_upsert_uni; --");

    // 第二次 upsert（同 id），验证更新路径也安全
    let rows2 = vec![row_for_upsert(1, "李四", 26, "li@t.com")];
    let (sql2, params2) = builder
        .build_batch_upsert_with_params(&rows2, &["id"], &[])
        .expect("build upsert sql 2");

    let rusqlite_params2: Vec<Box<dyn rusqlite::ToSql>> =
        params2.iter().map(value_to_rusqlite).collect();
    let param_refs2: Vec<&dyn rusqlite::ToSql> =
        rusqlite_params2.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql2, param_refs2.as_slice())
        .expect("execute upsert 2");

    let count2: i64 = conn
        .query_row("SELECT COUNT(*) FROM t_upsert_uni", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count2, 1, "应仍为 1 行（更新而非插入）");

    let name2: String = conn
        .query_row(
            "SELECT name FROM t_upsert_uni WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name2, "李四");
}

// ============================================================================
// Upsert 测试辅助类型与函数
// ============================================================================

/// 占位 Model（仅为满足 QueryBuilder<M> 泛型约束，无实际业务意义）
#[derive(Clone, Debug)]
struct DummyModel;

impl sz_orm_core::Model for DummyModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "dummy"
    }
    fn pk(&self) -> Self::PrimaryKey {
        0
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

impl sz_orm_core::ModelExt for DummyModel {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "age", "email"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name", "age", "email"]
    }
    fn guarded() -> Vec<&'static str> {
        vec!["id"]
    }
    fn hidden() -> Vec<&'static str> {
        vec![]
    }
    fn relations() -> std::collections::HashMap<&'static str, sz_orm_core::Relation> {
        std::collections::HashMap::new()
    }
    fn fill(&mut self, _data: std::collections::HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

/// 构造一行 upsert 测试数据
fn row_for_upsert(id: i64, name: &str, age: i32, email: &str) -> std::collections::HashMap<String, Value> {
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), Value::I64(id));
    row.insert("name".to_string(), Value::String(name.to_string()));
    row.insert("age".to_string(), Value::I32(age));
    row.insert("email".to_string(), Value::String(email.to_string()));
    row
}
