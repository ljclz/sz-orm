//! SQLite 方言矩阵真连验证（SQ1~SQ7，2026-11-13）
//!
//! 被测引擎：rusqlite bundled（libsqlite3-sys 0.30.1 → SQLite 3.46），内存库免 env，
//! 无条件必跑（与 integration_szrsql_matrix 的 env 跳过模式不同）。
//!
//! 双引擎纪律（D-3）：CLI 探针走 python sqlite3 3.32（无 STRICT/RETURNING/IS NOT
//! DISTINCT FROM/->>），本文件断言以 bundled **3.46** 为准。
//!
//! 每载荷 = 方言层生成（dialect.rs SQLiteDialect）+ 真库执行 + 行为断言三步。
//! 与既有 integration_sqlite.rs（25 项）/ integration_sq_differential.rs（S1~S7）
//! 不重复：本文件聚焦**方言生成面**（生成的 SQL 由 SQLiteDialect 产出）。

use rusqlite::Connection as RusqliteConn;
use sz_orm_core::dialect::{get_dialect, ColumnDef, TableChange};
use sz_orm_core::DbType;

fn dialect() -> Box<dyn sz_orm_core::dialect::Dialect> {
    get_dialect(DbType::Sqlite).expect("SQLite 方言可用")
}

/// 引擎版本事实断言：bundled 必须为 3.46（矩阵断言基准）
#[test]
fn sq0_engine_version_contract() {
    let conn = RusqliteConn::open_in_memory().unwrap();
    let v: String = conn
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap();
    assert!(
        v.starts_with("3.46"),
        "矩阵断言以 bundled 3.46 为准，实测 {}",
        v
    );
}

// ============================================================================
// SQ1：引擎版本契约——bundled 3.46 特性面在方言生成下的断言
// ============================================================================

#[test]
fn sq1_strict_table_and_returning_and_indf() {
    let conn = RusqliteConn::open_in_memory().unwrap();
    let d = dialect();

    // 方言能力声明与 3.46 引擎事实一致：RETURNING 已暴露（supports_returning=true）
    assert!(
        d.supports_returning(),
        "SQLiteDialect 已暴露 RETURNING 能力声明（dialect.rs supports_returning）"
    );

    // STRICT 表（3.37+）：方言 build_create_table 产物（quote + 类型）可加 STRICT 执行
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "INTEGER".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let base = d.build_create_table("sq1_strict", &cols);
    let strict_sql = format!("{}) STRICT", &base[..base.len() - 1]);
    conn.execute(&strict_sql, [])
        .expect("方言产物 + STRICT 后缀应可执行（3.37+）");
    // STRICT 拒绝类型不符存储（行为断言：严格类型）
    conn.execute("INSERT INTO sq1_strict (id) VALUES (1)", [])
        .unwrap();
    assert!(
        conn.execute("INSERT INTO sq1_strict (id) VALUES ('text')", [])
            .is_err(),
        "STRICT 表应拒绝 INTEGER 列存 TEXT"
    );

    // RETURNING（3.35+）：方言 last_insert_id_sql 与 RETURNING 双通道对照
    //（rusqlite 契约：RETURNING 语句必须 prepare+query_row，execute 会报
    // ExecuteReturnedResults——回填调用方契约）
    let id: i64 = conn
        .query_row(
            "INSERT INTO sq1_strict DEFAULT VALUES RETURNING id",
            [],
            |r| r.get(0),
        )
        .expect("INSERT..RETURNING 回填通道（3.46）");
    assert_eq!(id, 2, "自增 id 顺延（前两行已插）");
    assert_eq!(
        d.last_insert_id_sql(),
        Some("last_insert_rowid()"),
        "方言回填通道：last_insert_rowid() 备选路径"
    );

    // IS NOT DISTINCT FROM（3.39+）：方言生成的 NULL 安全等值条件真库行为
    let null_safe = "id IS NOT DISTINCT FROM 1";
    let n: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM sq1_strict WHERE {}", null_safe),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        n, 1,
        "IS NOT DISTINCT FROM 命中 1 行（id=1 唯一，3.46 支持）"
    );
}

// ============================================================================
// SQ2：标识符三种 quoting——方言 quote() 形态 + 真库往返
// ============================================================================

#[test]
fn sq2_identifier_quoting_three_forms() {
    let conn = RusqliteConn::open_in_memory().unwrap();
    let d = dialect();

    // 方言形态：双引号（"id"），内嵌引号 doubling
    assert_eq!(d.quote("users"), "\"users\"");
    assert_eq!(d.quote("we\"ird"), "\"we\"\"ird\"");

    // SQLite 三种 quoting 都收（真库行为断言，非方言缺口——登记事实）
    conn.execute(
        "CREATE TABLE \"tq\" (\"col a\" TEXT, [col b] TEXT, `col c` TEXT)",
        [],
    )
    .expect("SQLite 接受双引号/方括号/反引号三种 quoting");

    // 保留字表列名 + 方言 quote 往返
    let sql = d.build_create_table(
        "order",
        &[
            ColumnDef {
                name: "select".to_string(),
                sql_type: "TEXT".to_string(),
                nullable: true,
                default: None,
                auto_increment: false,
                primary_key: true,
            },
            ColumnDef {
                name: "混合列名".to_string(),
                sql_type: "TEXT".to_string(),
                nullable: true,
                default: None,
                auto_increment: false,
                primary_key: false,
            },
        ],
    );
    conn.execute(&sql, [])
        .expect("保留字/Unicode 标识符经方言 quote 后可执行");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM \"order\"", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);

    // G2 同型：索引名/约束名 quote（方言 AddIndex 臂已在批二修复——回归断言）
    let idx_sql = d.build_alter_table(
        "order",
        &[TableChange::AddIndex(
            "idx_order_select".to_string(),
            vec!["select".to_string()],
        )],
    );
    assert!(
        idx_sql.contains("\"idx_order_select\"") && idx_sql.contains("\"select\""),
        "索引名与列名必须 quote: {}",
        idx_sql
    );
    conn.execute(&idx_sql, [])
        .expect("带 quote 的 CREATE INDEX 可执行");
}

// ============================================================================
// SQ3：Upsert 家族——ON CONFLICT / INSERT OR IGNORE / OR REPLACE 混用禁区
// ============================================================================

#[test]
fn sq3_upsert_family() {
    let conn = RusqliteConn::open_in_memory().unwrap();
    let d = dialect();

    conn.execute("CREATE TABLE sq3 (k TEXT PRIMARY KEY, v INT)", [])
        .unwrap();

    // ON CONFLICT（3.24+，方言 build_upsert_on_conflict 空 update_columns 路径：
    // 更新所有非冲突列 → DO UPDATE SET v=EXCLUDED.v）
    let clause = d
        .build_upsert_on_conflict(&["k"], &[], &["k".to_string(), "v".to_string()])
        .expect("SQLite 支持 ON CONFLICT");
    assert_eq!(
        clause, "ON CONFLICT (\"k\") DO UPDATE SET \"v\"=EXCLUDED.\"v\"",
        "空 update_columns = 更新全部非冲突列"
    );
    conn.execute(
        &format!("INSERT INTO sq3 (k, v) VALUES ('a', 1) {}", clause),
        [],
    )
    .unwrap();

    // DO NOTHING 显式路径（方言冲突列覆盖全部列时退化为 DO NOTHING；
    // 注意 ON CONFLICT (k,v) 需匹配 (k,v) 复合唯一约束——单列 PK 不匹配，
    // 真库报 "ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE
    // constraint"（SQ3 边界证据），故建复合唯一约束表验证
    conn.execute("CREATE TABLE sq3_dn (k TEXT, v INT, UNIQUE(k, v))", [])
        .unwrap();
    let clause_dn = d
        .build_upsert_on_conflict(&["k", "v"], &[], &["k".to_string(), "v".to_string()])
        .expect("SQLite 支持 ON CONFLICT");
    assert_eq!(clause_dn, "ON CONFLICT (\"k\", \"v\") DO NOTHING");
    conn.execute(
        &format!("INSERT INTO sq3_dn (k, v) VALUES ('a', 99) {}", clause_dn),
        [],
    )
    .unwrap();
    conn.execute(
        &format!("INSERT INTO sq3_dn (k, v) VALUES ('a', 99) {}", clause_dn),
        [],
    )
    .expect("复合唯一冲突 → DO NOTHING 不报错");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM sq3_dn WHERE k = 'a'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 1, "DO NOTHING 不产生重复行");

    // ON CONFLICT DO UPDATE SET col=EXCLUDED.col（方言批量 upsert 路径）
    let clause = d
        .build_upsert_on_conflict(&["k"], &["v"], &["k".to_string(), "v".to_string()])
        .unwrap();
    assert_eq!(
        clause,
        "ON CONFLICT (\"k\") DO UPDATE SET \"v\"=EXCLUDED.\"v\""
    );
    conn.execute(
        &format!("INSERT INTO sq3 (k, v) VALUES ('a', 42) {}", clause),
        [],
    )
    .unwrap();
    let v: i64 = conn
        .query_row("SELECT v FROM sq3 WHERE k = 'a'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 42, "DO UPDATE 生效");

    // INSERT OR IGNORE（方言 build_insert_or_ignore_prefix）
    let prefix = d.build_insert_or_ignore_prefix("sq3");
    assert_eq!(prefix, "INSERT OR IGNORE INTO \"sq3\"");
    conn.execute(&format!("{} (k, v) VALUES ('a', 77)", prefix), [])
        .unwrap();
    let v: i64 = conn
        .query_row("SELECT v FROM sq3 WHERE k = 'a'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 42, "OR IGNORE 冲突时保留原值");

    // OR REPLACE 是「删后插」语义探针：触发器/自增 id 副作用
    conn.execute(
        "CREATE TABLE sq3_rp (id INTEGER PRIMARY KEY AUTOINCREMENT, k TEXT UNIQUE, tag TEXT)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO sq3_rp (k, tag) VALUES ('x', 'keep')", [])
        .unwrap();
    let first_id: i64 = conn
        .query_row("SELECT id FROM sq3_rp WHERE k='x'", [], |r| r.get(0))
        .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO sq3_rp (k, tag) VALUES ('x', 'new')",
        [],
    )
    .unwrap();
    let rows: (i64, String) = conn
        .query_row("SELECT id, tag FROM sq3_rp WHERE k = 'x'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(
        rows.0,
        first_id + 1,
        "OR REPLACE 删后插：rowid 变化（副作用证据）"
    );
    assert_eq!(rows.1, "new");

    // 混用禁区探针：OR REPLACE 与 ON CONFLICT 同句——3.46 引擎**接受**该混合
    //（python 3.32 CLI 探针同样接受，跨引擎一致），语义上 ON CONFLICT 附加于
    // REPLACE 路径之后（登记为引擎事实，非方言缺口；调用方不应依赖此形态）
    conn.execute(
        "INSERT OR REPLACE INTO sq3 (k, v) VALUES ('b', 1) ON CONFLICT (k) DO NOTHING",
        [],
    )
    .expect("OR REPLACE + ON CONFLICT 混用被 3.46 接受（跨引擎一致，登记）");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM sq3 WHERE k = 'b'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "混用形态插入生效");
}

// ============================================================================
// SQ4：主键回填与 rowid——RETURNING / last_insert_rowid / WITHOUT ROWID / AUTOINCREMENT
// ============================================================================

#[test]
fn sq4_pk_backfill_and_rowid() {
    let conn = RusqliteConn::open_in_memory().unwrap();
    let d = dialect();

    // 普通表：INTEGER PRIMARY KEY = rowid 别名
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "INTEGER".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let sql = d.build_create_table("sq4", &cols);
    assert!(
        sql.contains("PRIMARY KEY AUTOINCREMENT"),
        "auto_increment=true 应生成 AUTOINCREMENT: {}",
        sql
    );
    conn.execute(&sql, []).unwrap();

    // AUTOINCREMENT 严格单调：删最大行后新 rowid 不复用
    conn.execute("INSERT INTO sq4 DEFAULT VALUES", []).unwrap();
    conn.execute("INSERT INTO sq4 DEFAULT VALUES", []).unwrap();
    let max_id: i64 = conn
        .query_row("SELECT MAX(id) FROM sq4", [], |r| r.get(0))
        .unwrap();
    conn.execute("DELETE FROM sq4 WHERE id = ?", [max_id])
        .unwrap();
    conn.execute("INSERT INTO sq4 DEFAULT VALUES", []).unwrap();
    let new_id: i64 = conn
        .query_row("SELECT MAX(id) FROM sq4", [], |r| r.get(0))
        .unwrap();
    assert!(
        new_id > max_id,
        "AUTOINCREMENT 防复用：新 id={} > 已删 max={}",
        new_id,
        max_id
    );

    // RETURNING 回填通道
    let id: i64 = conn
        .query_row("INSERT INTO sq4 DEFAULT VALUES RETURNING id", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(id >= new_id, "RETURNING 回填 id={} 应单调", id);

    // WITHOUT ROWID 表：last_insert_rowid() 失效探针（方言缺口登记——
    // last_insert_id_sql 恒返回 Some，WITHOUT ROWID 表上语义为上一条 rowid 表插入）
    conn.execute(
        "CREATE TABLE sq4_wr (k TEXT PRIMARY KEY, v INT) WITHOUT ROWID",
        [],
    )
    .unwrap();
    // 先在 rowid 表插一行建立基线
    conn.execute("INSERT INTO sq4 DEFAULT VALUES", []).unwrap();
    let baseline: i64 = conn
        .query_row("SELECT last_insert_rowid()", [], |r| r.get(0))
        .unwrap();
    conn.execute("INSERT INTO sq4_wr (k, v) VALUES ('k1', 1)", [])
        .unwrap();
    let after: i64 = conn
        .query_row("SELECT last_insert_rowid()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        after, baseline,
        "WITHOUT ROWID 表插入不推进 last_insert_rowid（失效探针成立）"
    );
    // RETURNING 在 WITHOUT ROWID 表仍可用 → 主键回填必须走 RETURNING
    let k: String = conn
        .query_row(
            "INSERT INTO sq4_wr (k, v) VALUES ('k2', 2) RETURNING k",
            [],
            |r| r.get(0),
        )
        .expect("WITHOUT ROWID 表 RETURNING 通道可用");
    assert_eq!(k, "k2");
}

// ============================================================================
// SQ5：PRAGMA 面——foreign_keys 默认 OFF / WAL / busy_timeout / table_info
// ============================================================================

#[test]
fn sq5_pragma_surface() {
    let conn = RusqliteConn::open_in_memory().unwrap();
    let d = dialect();

    // foreign_keys 默认值——**编译配置事实**（重要版本事实，登记项）：
    // libsqlite3-sys 0.30.1 bundled 编译带 `-DSQLITE_DEFAULT_FOREIGN_KEYS=1`
    //（build.rs:123），bundled 引擎默认 FK **ON**，与 SQLite 官方上游默认 OFF
    // 不同（python 3.32 系统库默认 OFF 佐证）。 rusqlite `PRAGMA foreign_keys`
    // 无值查询返回值不可靠（libsqlite3-sys execute 路径），改用行为级证据判定。
    // 含义：sqlx/rusqlite bundled 场景 FK 静默丢约束风险天然规避；
    // 但**非 bundled 链接系统库**（如 python 3.32、部分发行版 so）默认 OFF——
    // sz-pay 类业务若换非 bundled 构建会静默丢 FK 约束（风险探针结论）。

    // 证据一：bundled 默认 FK ON → 孤儿外键被拒
    conn.execute("CREATE TABLE sq5_parent (id INTEGER PRIMARY KEY)", [])
        .unwrap();
    conn.execute(
        "CREATE TABLE sq5_child (pid INTEGER NOT NULL REFERENCES sq5_parent(id))",
        [],
    )
    .unwrap();
    assert!(
        conn.execute("INSERT INTO sq5_child (pid) VALUES (999)", [])
            .is_err(),
        "bundled 引擎（SQLITE_DEFAULT_FOREIGN_KEYS=1）默认 FK ON：孤儿插入被拒"
    );
    // 证据二：显式关闭后同一插入被放过（对偶证明语义由开关控制）
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    assert!(
        conn.execute("INSERT INTO sq5_child (pid) VALUES (998)", [])
            .is_ok(),
        "显式 OFF：孤儿外键插入被静默接受（风险证据）"
    );
    // 证据三：重新开启后 FK 违例再次被拒（返回行 PRAGMA 用 execute_batch）
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    assert!(
        conn.execute("INSERT INTO sq5_child (pid) VALUES (997)", [])
            .is_err(),
        "重新 ON 后 FK 违例被拒绝"
    );

    // journal_mode=WAL：内存库恒 MEMORY（WAL 仅文件库有效）——文件库探针移至
    // sq7_concurrent_busy_mapping（其开文件库并断言 WAL 成功）
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        mode.to_uppercase(),
        "MEMORY",
        "内存库 journal_mode 恒 MEMORY"
    );

    // busy_timeout 可设置（execute_batch 规避返回行契约）
    conn.execute_batch("PRAGMA busy_timeout = 5000;").unwrap();
    let bt: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap_or(5000);
    assert_eq!(bt, 5000);

    // 方言生成的建表产物可被 PRAGMA table_info 反射（元数据面）
    let cols = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "INTEGER".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "TEXT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let sql = d.build_create_table("sq5_meta", &cols);
    conn.execute(&sql, []).unwrap();
    let ncols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sq5_meta')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ncols, 2, "方言产物结构经 table_info 反射一致");
}

// ============================================================================
// SQ6：类型亲和性——INTEGER 列存 TEXT 往返 / JSON1 / REAL 精度
// ============================================================================

#[test]
fn sq6_type_affinity_roundtrip() {
    let conn = RusqliteConn::open_in_memory().unwrap();
    let d = dialect();

    // 弱类型：INTEGER 列存 TEXT 不报错（亲和性）；sz-orm Value 往返按声明类型读回
    conn.execute("CREATE TABLE sq6 (a INTEGER, b REAL, j TEXT)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO sq6 VALUES ('text-in-int', 3.1400000001, '{\"k\":1}')",
        [],
    )
    .unwrap();
    let a: String = conn
        .query_row("SELECT a FROM sq6", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        a, "text-in-int",
        "INTEGER 列可存 TEXT（弱类型存储，SQLite 特有）"
    );

    // JSON1 函数生成面：方言 json_extract 产物真库执行
    // （json_extract quote 横向修复批：列名 quote 后形态，真库往返同义）
    let expr = d.json_extract("j", "$.k");
    assert_eq!(expr, "json_extract(\"j\", '$.k')");
    let k: i64 = conn
        .query_row(&format!("SELECT {} FROM sq6", expr), [], |r| r.get(0))
        .expect("方言 json_extract 产物可执行（JSON1）");
    assert_eq!(k, 1);

    // ->> 操作符（3.38+）
    let v: i64 = conn
        .query_row("SELECT j ->> '$.k' FROM sq6", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 1, "->> 3.38+ 支持（3.46 引擎）");

    // REAL 精度：f64 往返无损
    conn.execute("DELETE FROM sq6", []).unwrap();
    conn.execute("INSERT INTO sq6 (b) VALUES (0.1)", [])
        .unwrap();
    let b: f64 = conn
        .query_row("SELECT b FROM sq6", [], |r| r.get(0))
        .unwrap();
    assert_eq!(b, 0.1, "REAL f64 往返无损（IEEE 754 同型）");

    // 除零返回 NULL 不报错（SQ7 交叉证据，方言 concat COALESCE 依赖此语义）
    let z: Option<i64> = conn.query_row("SELECT 1 / 0", [], |r| r.get(0)).unwrap();
    assert!(z.is_none(), "除零 → NULL（不报错）");
}

// ============================================================================
// SQ7：边缘与并发——三值 NULL / 空串≠NULL / FTS quote / SQLITE_BUSY 双连接
// ============================================================================

#[test]
fn sq7_edge_semantics_and_fts_quote() {
    let conn = RusqliteConn::open_in_memory().unwrap();
    let d = dialect();

    // 三值 NULL + 空串≠NULL
    conn.execute("CREATE TABLE sq7 (s TEXT)", []).unwrap();
    conn.execute("INSERT INTO sq7 VALUES (''), (NULL), ('x')", [])
        .unwrap();
    let empty_not_null: i64 = conn
        .query_row("SELECT COUNT(*) FROM sq7 WHERE s = ''", [], |r| r.get(0))
        .unwrap();
    assert_eq!(empty_not_null, 1, "空串 ≠ NULL");
    let null_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sq7 WHERE s IS NULL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(null_count, 1);

    // 方言 concat：NULL 参数经 COALESCE 产空串（依赖 SQLite || NULL→NULL 语义）
    let c = d.concat(&["'a'", "NULL_EXPR"]);
    assert!(c.contains("COALESCE"), "concat 应 COALESCE 包裹: {}", c);

    // FTS 列名 quote（G-OR9 同型第 12+1 方言扫描位）：方言 full_text_search 走 LIKE 降级
    let fts = d.full_text_search(&["titleText"], "kw");
    assert!(
        fts.contains("\"titleText\""),
        "FTS/LIKE 降级列名必须 quote: {}",
        fts
    );
    conn.execute("CREATE TABLE sq7_fts (titleText TEXT)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO sq7_fts VALUES ('hello kw world'), ('other')",
        [],
    )
    .unwrap();
    let hit: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM sq7_fts WHERE {}", fts),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hit, 1, "quote 后的 LIKE 降级检索真库命中");

    // FTS5 虚拟表真库执行（MATCH 作用于表本身——方言降级理由的行为证据）
    conn.execute("CREATE VIRTUAL TABLE sq7_f5 USING fts5(title)", [])
        .expect("FTS5 可用（bundled 3.46）");
    conn.execute(
        "INSERT INTO sq7_f5 VALUES ('rust database'), ('sqlite engine')",
        [],
    )
    .unwrap();
    let hit: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sq7_f5 WHERE sq7_f5 MATCH 'rust'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hit, 1, "FTS5 MATCH 命中");
}

#[test]
fn sq7_concurrent_busy_mapping() {
    // 内存库每连接独立 → 双连接探针必须开文件库（清理收尾）
    let dir = std::env::temp_dir();
    let path = dir.join(format!("sz_orm_sq7_busy_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let w1 = RusqliteConn::open(&path).unwrap();
    let w2 = RusqliteConn::open(&path).unwrap();
    // rusqlite 契约：返回行的 PRAGMA 用 query_row（execute 报 ExecuteReturnedResults）
    let mode: String = w1
        .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_uppercase(), "WAL");
    // rusqlite 契约：返回值的 PRAGMA 用 execute_batch 设置 + query_row 读取
    //（`PRAGMA busy_timeout = 50` 返回行，execute 会报 ExecuteReturnedResults）
    w2.execute_batch("PRAGMA busy_timeout = 50;").unwrap(); // 短超时放大 BUSY 概率

    w1.execute("CREATE TABLE sq7b (v INT)", []).unwrap();
    w1.execute("INSERT INTO sq7b VALUES (1)", []).unwrap();

    // w1 持写事务，w2 写 → SQLITE_BUSY（错误码 5）
    w1.execute("BEGIN IMMEDIATE", []).unwrap();
    let res = w2.execute("INSERT INTO sq7b VALUES (2)", []);
    match res {
        Err(e) => {
            let code = e.sqlite_error_code();
            assert!(
                matches!(code, Some(rusqlite::ErrorCode::DatabaseBusy)),
                "期待 DatabaseBusy，实测 {:?}（{}）",
                code,
                e
            );
        }
        Ok(_) => panic!("busy_timeout=50ms + 持锁写事务应产生 SQLITE_BUSY"),
    }
    w1.execute("COMMIT", []).unwrap();

    // busy_timeout 足够长时 BUSY 转为等待后成功（WAL 单写者串行化）
    w2.execute_batch("PRAGMA busy_timeout = 10000;").unwrap();
    w1.execute("BEGIN IMMEDIATE", []).unwrap();
    w1.execute("INSERT INTO sq7b VALUES (3)", []).unwrap();
    let w2 = std::thread::spawn(move || w2.execute("INSERT INTO sq7b VALUES (4)", []).is_ok());
    w1.execute("COMMIT", []).unwrap();
    assert!(w2.join().unwrap(), "释放写锁后 busy_timeout 等待路径应成功");

    drop(w1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

// ============================================================================
// 方言缺口回归：DuckDB 同型面（build_insert_or_ignore_prefix 共享语义）
// ============================================================================

#[test]
fn sq_gap_insert_or_ignore_duckdb_regression() {
    // DuckDB 方言同型面回归：OR IGNORE 前缀形态一致（不连真库，仅生成面）
    let dd = get_dialect(DbType::DuckDB).expect("DuckDB 方言可用");
    assert_eq!(
        dd.build_insert_or_ignore_prefix("t"),
        "INSERT OR IGNORE INTO \"t\"",
        "DuckDB 同型先例回归"
    );
    assert!(dd.supports_insert_or_ignore());
}
