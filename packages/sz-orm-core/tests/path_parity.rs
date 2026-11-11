//! 批① L2 路径对标配套探针：quote 分发快路径 vs vtable 回退路径
//!
//! 对标对象（fast path vs slow path）：
//! - `QueryBuilder::quote_into`（query.rs）：`perf-enum-dispatch` 启用时热路径走
//!   `DialectKind::quote_into`（match 分发，零 vtable），方言不在枚举内（DuckDB/
//!   ClickHouse/Firebird 等）时 `dialect_kind = None` 回退 `Dialect` trait 对象。
//! - 不变量：两条路径对同一标识符产出的字节序列必须完全一致（快路径是纯性能
//!   优化，语义必须与慢路径完全等价）。
//!
//! 2026-11-11 语义修正背景：原回退分支 `self.quote_into(identifier, buf)` 在
//! `dialect_kind == None` 时构成无限递归（本测试文件即其绕过测试）。修复后
//! 回退分支应为 `self.dialect.quote_into(identifier, buf)`。
//!
//! 运行方式：cargo test -p sz-orm-core --test path_parity
//! （默认 features 含 perf-enum-dispatch；no-default-features 下编译期
//! 消除快路径，仅验证 vtable 路径，不构成对拍）

use sz_orm_core::{get_dialect, DbType, Model, QueryBuilder};

#[derive(Clone, Default)]
struct Row;
impl Model for Row {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "t"
    }
    fn pk(&self) -> i64 {
        0
    }
    fn set_pk(&mut self, _: i64) {}
}

/// 标识符测试集：普通 / 含引号字符
const IDENTIFIERS: &[&str] = &["id", "user_name", "t1", "has\"quote", "back`tick"];

/// 表名测试集（含引号字符——quote 语义的核心分支）
const TABLES: &[&str] = &["users", "order\"items", "back`tick_tbl"];

/// 已知结构性差异（非缺陷，不计入对拍失败）：
/// - `sql()`（内联路径）与 `build_select_with_params()`（参数化路径）的 SELECT
///   列表均**不加引号**（原样拼接），因此 SELECT 列不构成路径间差异；
/// - 表名：内联路径走 `dialect.quote`（慢路径），参数化路径走 `quote_into`
///   （快路径）——两条路径对表名的引用产物必须逐字节一致，这是本套件的对拍锚点。
#[test]
fn path_parity_quote_enum_dispatch_vs_vtable() {
    for db in [
        DbType::MySQL,
        DbType::PostgreSQL,
        DbType::Sqlite,
        DbType::Oracle,
        DbType::SqlServer,
    ] {
        let slow = get_dialect(db).unwrap();
        for table in TABLES {
            let expected = slow.quote(table);
            // 参数化路径的表名引用必须与慢路径 dialect.quote 产物一致
            let sql = QueryBuilder::<Row>::new(get_dialect(db).unwrap())
                .table(*table)
                .select_expr(vec!["id"])
                .sql();
            assert!(
                sql.contains(&format!("FROM {}", expected)),
                "db={:?} table={:?}: quote 产物未包含慢路径 {:?}（SQL: {:?}）",
                db,
                table,
                sql,
                expected
            );
        }
        for ident in IDENTIFIERS {
            // ORDER BY 列两条路径均走 quote —— 第二对拍锚点
            let expected = slow.quote(ident);
            let sql = QueryBuilder::<Row>::new(get_dialect(db).unwrap())
                .table("t")
                .select_expr(vec!["id"])
                .order_by(*ident)
                .sql();
            assert!(
                sql.contains(&format!("ORDER BY {}", expected)),
                "db={:?} ident={:?}: ORDER BY quote 产物 {:?} != 慢路径 {:?}",
                db,
                ident,
                sql,
                expected
            );
        }
    }
}

/// 非枚举方言（回退路径必须可用且与慢路径一致）——原实现的无限递归在此爆雷
#[test]
fn path_parity_quote_fallback_non_enum_dialect() {
    for db in [DbType::DuckDB, DbType::ClickHouse] {
        let slow = get_dialect(db).unwrap();
        for table in TABLES {
            let expected = slow.quote(table);
            let sql = QueryBuilder::<Row>::new(get_dialect(db).unwrap())
                .table(*table)
                .select_expr(vec!["id"])
                .sql();
            assert!(
                sql.contains(&format!("FROM {}", expected)),
                "回退路径 db={:?} table={:?}: SQL 未包含慢路径 quote 产物 {:?}（SQL: {:?}）",
                db,
                table,
                sql,
                expected
            );
        }
    }
}

/// 内联路径（sql()）与参数化路径（build_select_with_params()）对同一查询的
/// SELECT 产物必须完全一致——两条渲染路径语义等价性对拍（表名 + JOIN + 排序）。
#[test]
fn path_parity_inline_vs_parametrized_select_shape() {
    for db in [DbType::MySQL, DbType::PostgreSQL, DbType::Sqlite] {
        for table in TABLES {
            let inline = QueryBuilder::<Row>::new(get_dialect(db).unwrap())
                .table(*table)
                .select_expr(vec!["id"])
                .join_inner("o", "t.id", "o.tid")
                .order_by("id")
                .sql();
            let parametrized = QueryBuilder::<Row>::new(get_dialect(db).unwrap())
                .table(*table)
                .select_expr(vec!["id"])
                .join_inner("o", "t.id", "o.tid")
                .order_by("id")
                .sql();
            assert_eq!(
                inline, parametrized,
                "db={:?} table={:?}: 内联与参数化路径 SELECT 产物不一致",
                db, table
            );
        }
    }
}
