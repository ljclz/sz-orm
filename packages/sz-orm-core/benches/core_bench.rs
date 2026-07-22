//! sz-orm-core 性能基准测试
//!
//! 使用 criterion 测量核心组件的吞吐量与延迟：
//! - Value::to_param() 各类型转换
//! - Dialect::escape_string() SQL 转义
//! - Dialect::build_create_table() DDL 生成
//! - Dialect::build_pagination() 分页 SQL
//! - Pool::acquire/release 连接池
//! - MockConnection::execute/query 模拟 IO
//! - InMemoryDb::insert/select_all/select_where 数据扫描
//!
//! 运行：cargo bench --package sz-orm-core
//! 报告：target/criterion/index.html

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_core::dialect::{get_dialect, ColumnDef, MySqlDialect};
use sz_orm_core::{Connection, ConnectionFactory, DbType, Pool, PoolConfig, Value};

// ============================================================================
// Value::to_param 性能
// ============================================================================

fn bench_value_to_param(c: &mut Criterion) {
    let mut group = c.benchmark_group("value_to_param");
    group.throughput(Throughput::Elements(1));

    group.bench_function("null", |b| {
        b.iter(|| {
            let v = Value::Null;
            black_box(v.to_param());
        })
    });

    group.bench_function("i64", |b| {
        b.iter(|| {
            let v = Value::I64(black_box(42));
            black_box(v.to_param());
        })
    });

    group.bench_function("f64", |b| {
        b.iter(|| {
            let v = Value::F64(black_box(2.5));
            black_box(v.to_param());
        })
    });

    group.bench_function("bool", |b| {
        b.iter(|| {
            let v = Value::Bool(black_box(true));
            black_box(v.to_param());
        })
    });

    group.bench_function("string_short", |b| {
        b.iter(|| {
            let v = Value::String("hello world".to_string());
            black_box(v.to_param());
        })
    });

    group.bench_function("string_long_256", |b| {
        let s: String = "a".repeat(256);
        b.iter(|| {
            let v = Value::String(s.clone());
            black_box(v.to_param());
        })
    });

    group.bench_function("bytes_64", |b| {
        let bytes: Vec<u8> = (0..64).collect();
        b.iter(|| {
            let v = Value::Bytes(bytes.clone());
            black_box(v.to_param());
        })
    });

    group.bench_function("array_10", |b| {
        let arr: Vec<Value> = (0..10).map(Value::I64).collect();
        b.iter(|| {
            let v = Value::Array(arr.clone());
            black_box(v.to_param());
        })
    });

    group.finish();
}

// ============================================================================
// Dialect::escape_string 性能
// ============================================================================

fn bench_dialect_escape_string(c: &mut Criterion) {
    let mysql = get_dialect(DbType::MySQL).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    let sqlite = get_dialect(DbType::Sqlite).unwrap();

    let mut group = c.benchmark_group("dialect_escape_string");

    let inputs: &[(&str, &str)] = &[
        ("plain_32", "abcdefghijklmnopqrstuvwxyz0123456789"),
        ("special_32", "a'b\"c\\d\ne\rf\tg\x00hijk"),
        ("long_1024", &"x".repeat(1024)),
    ];

    for (name, input) in inputs {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::new("mysql", name), input, |b, s| {
            b.iter(|| mysql.escape_string(black_box(s)))
        });
        group.bench_with_input(BenchmarkId::new("pg", name), input, |b, s| {
            b.iter(|| pg.escape_string(black_box(s)))
        });
        group.bench_with_input(BenchmarkId::new("sqlite", name), input, |b, s| {
            b.iter(|| sqlite.escape_string(black_box(s)))
        });
    }
    group.finish();
}

// ============================================================================
// Dialect::build_create_table 性能
// ============================================================================

fn bench_dialect_build_create_table(c: &mut Criterion) {
    let mysql = get_dialect(DbType::MySQL).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    let sqlite = get_dialect(DbType::Sqlite).unwrap();

    fn make_columns(n: usize) -> Vec<ColumnDef> {
        (0..n)
            .map(|i| ColumnDef {
                name: format!("col_{}", i),
                sql_type: if i == 0 {
                    "BIGINT".to_string()
                } else if i % 3 == 0 {
                    "VARCHAR(255)".to_string()
                } else if i % 3 == 1 {
                    "BIGINT".to_string()
                } else {
                    "TEXT".to_string()
                },
                nullable: i != 0,
                default: None,
                auto_increment: i == 0,
                primary_key: i == 0,
            })
            .collect()
    }

    let mut group = c.benchmark_group("dialect_build_create_table");

    for n in [5, 20, 50, 100].iter() {
        let cols = make_columns(*n);
        group.throughput(Throughput::Elements(*n as u64));
        group.bench_with_input(BenchmarkId::new("mysql", n), &cols, |b, c| {
            b.iter(|| mysql.build_create_table(black_box("bench_table"), black_box(c)))
        });
        group.bench_with_input(BenchmarkId::new("pg", n), &cols, |b, c| {
            b.iter(|| pg.build_create_table(black_box("bench_table"), black_box(c)))
        });
        group.bench_with_input(BenchmarkId::new("sqlite", n), &cols, |b, c| {
            b.iter(|| sqlite.build_create_table(black_box("bench_table"), black_box(c)))
        });
    }
    group.finish();
}

// ============================================================================
// Dialect::build_pagination 性能
// ============================================================================

fn bench_dialect_build_pagination(c: &mut Criterion) {
    let mysql = get_dialect(DbType::MySQL).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    let sqlite = get_dialect(DbType::Sqlite).unwrap();

    let base_sql =
        "SELECT id, name, value, data, meta FROM large_table WHERE value > 100 ORDER BY id";

    let mut group = c.benchmark_group("dialect_build_pagination");
    group.throughput(Throughput::Elements(1));

    for page in [1u64, 100, 10_000, 1_000_000].iter() {
        group.bench_with_input(BenchmarkId::new("mysql", page), page, |b, p| {
            b.iter(|| mysql.build_pagination(black_box(base_sql), black_box(*p), black_box(50)))
        });
        group.bench_with_input(BenchmarkId::new("pg", page), page, |b, p| {
            b.iter(|| pg.build_pagination(black_box(base_sql), black_box(*p), black_box(50)))
        });
        group.bench_with_input(BenchmarkId::new("sqlite", page), page, |b, p| {
            b.iter(|| sqlite.build_pagination(black_box(base_sql), black_box(*p), black_box(50)))
        });
    }
    group.finish();
}

// ============================================================================
// 异步基准：Pool acquire/release
// ============================================================================

mod bench_helpers {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    pub struct BenchConnection {
        pub connected: bool,
    }

    impl Connection for BenchConnection {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(1) })
        }
        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<Vec<HashMap<String, Value>>, sz_orm_core::DbError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move { Ok(vec![]) })
        }
        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }
        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }
        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async move { self.connected })
        }
        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async move {
                self.connected = false;
                Ok(())
            })
        }
    }

    pub struct BenchConnectionFactory;

    #[async_trait::async_trait]
    impl ConnectionFactory for BenchConnectionFactory {
        async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
            Ok(Box::new(BenchConnection { connected: true }))
        }
    }
}

fn bench_pool_acquire_release(c: &mut Criterion) {
    use bench_helpers::BenchConnectionFactory;

    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("pool_acquire_release");

    for max_size in [8u32, 32, 128].iter() {
        group.bench_with_input(
            BenchmarkId::new("pool_size", max_size),
            max_size,
            |b, &size| {
                b.iter(|| {
                    rt.block_on(async {
                        let config = PoolConfig {
                            max_size: size,
                            min_idle: 1,
                            ..PoolConfig::default()
                        };
                        let factory = Arc::new(BenchConnectionFactory);
                        let pool = Pool::new(config, factory).unwrap();
                        // acquire + release 循环 100 次
                        for _ in 0..100 {
                            let conn = pool.acquire().await.unwrap();
                            pool.release(conn).await;
                        }
                        pool.close_all().await;
                        black_box(());
                    })
                })
            },
        );
    }
    group.finish();
}

// ============================================================================
// InMemoryDb 数据扫描性能（10万行）
// ============================================================================

mod db_helpers {
    use super::*;
    pub struct InMemoryTable {
        pub rows: Vec<HashMap<String, Value>>,
    }

    impl InMemoryTable {
        pub fn new() -> Self {
            Self { rows: Vec::new() }
        }

        pub fn insert(&mut self, row: HashMap<String, Value>) {
            self.rows.push(row);
        }

        pub fn select_all(&self) -> &[HashMap<String, Value>] {
            &self.rows
        }

        pub fn select_where_eq(&self, field: &str, value: &Value) -> Vec<&HashMap<String, Value>> {
            self.rows
                .iter()
                .filter(|r| r.get(field) == Some(value))
                .collect()
        }

        pub fn count(&self) -> usize {
            self.rows.len()
        }
    }
}

fn bench_in_memory_scan(c: &mut Criterion) {
    use db_helpers::InMemoryTable;

    let mut group = c.benchmark_group("in_memory_scan");

    for n in [1_000usize, 10_000, 100_000].iter() {
        // 准备数据
        let mut table = InMemoryTable::new();
        for i in 0..*n {
            let mut row = HashMap::new();
            row.insert("id".to_string(), Value::I64(i as i64));
            row.insert("name".to_string(), Value::String(format!("user_{}", i)));
            row.insert("value".to_string(), Value::I64((i % 100) as i64));
            table.insert(row);
        }

        group.throughput(Throughput::Elements(*n as u64));

        // select_all
        group.bench_with_input(BenchmarkId::new("select_all", n), &table, |b, t| {
            b.iter(|| {
                let rows = black_box(t.select_all());
                black_box(rows.len());
            })
        });

        // count
        group.bench_with_input(BenchmarkId::new("count", n), &table, |b, t| {
            b.iter(|| black_box(t.count()))
        });

        // select_where_eq (命中 1% 数据)
        let target = Value::I64(50);
        group.bench_with_input(
            BenchmarkId::new("select_where_eq_1pct", n),
            &table,
            |b, t| {
                b.iter(|| {
                    let result = t.select_where_eq(black_box("value"), black_box(&target));
                    black_box(result.len());
                })
            },
        );
    }
    group.finish();
}

// ============================================================================
// JSON 操作性能（mock：解析与提取）
// ============================================================================

fn bench_json_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_parsing");

    let small = r#"{"name":"alice","age":30,"active":true}"#;
    let medium = serde_json::json!({
        "id": 42,
        "name": "user_profile",
        "tags": ["admin", "user", "vip"],
        "metadata": {
            "created_at": "2026-01-01T00:00:00Z",
            "login_count": 100,
            "last_ip": "192.168.1.1"
        }
    })
    .to_string();
    let large: String = serde_json::json!({
        "items": (0..100).map(|i| {
            serde_json::json!({
                "id": i,
                "name": format!("item_{}", i),
                "value": i * 10,
                "nested": { "a": i, "b": i + 1 }
            })
        }).collect::<Vec<_>>()
    })
    .to_string();

    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function("small_60b", |b| {
        b.iter(|| serde_json::from_str::<serde_json::Value>(black_box(small)).unwrap())
    });

    group.throughput(Throughput::Bytes(medium.len() as u64));
    group.bench_function("medium_200b", |b| {
        b.iter(|| serde_json::from_str::<serde_json::Value>(black_box(&medium)).unwrap())
    });

    group.throughput(Throughput::Bytes(large.len() as u64));
    group.bench_function("large_3kb", |b| {
        b.iter(|| serde_json::from_str::<serde_json::Value>(black_box(&large)).unwrap())
    });

    group.finish();
}

// ============================================================================
// QueryBuilder::build_select 性能（不同复杂度的 SELECT）
// ============================================================================

fn bench_query_builder_select(c: &mut Criterion) {
    use sz_orm_core::Model;
    use sz_orm_core::QueryBuilder;

    struct BenchModel;
    impl Model for BenchModel {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "bench_table"
        }
        fn pk(&self) -> Self::PrimaryKey {
            0
        }
        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
    }

    let mut group = c.benchmark_group("query_builder_select");
    group.throughput(Throughput::Elements(1));

    // 简单：SELECT * FROM table WHERE id = ?
    group.bench_function("simple_where", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .table("bench_table")
                .where_cond("id = 1");
            black_box(qb.build_select())
        })
    });

    // 中等：SELECT col1,col2 FROM table WHERE a=? AND b IN(?) ORDER BY c LIMIT 10
    group.bench_function("medium_where_in_orderby", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .table("bench_table")
                .select(vec!["col1", "col2", "col3"])
                .where_cond("a = 1")
                .where_cond("b > 10")
                .where_in("status", vec![Value::I64(1), Value::I64(2)])
                .order_by("created_at")
                .limit(10);
            black_box(qb.build_select())
        })
    });

    // 复杂：多条件 + BETWEEN + ORDER BY + LIMIT + OFFSET
    group.bench_function("complex_between_limit_offset", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .table("bench_table")
                .select(vec!["id", "name", "email", "status", "created_at"])
                .where_cond("status = 'active'")
                .where_between("age", Value::I64(18), Value::I64(65))
                .where_not_null("email")
                .order_by("created_at")
                .limit(20);
            black_box(qb.build_select())
        })
    });

    group.finish();
}

// ============================================================================
// QueryBuilder::build_insert / build_update 性能
// ============================================================================

fn bench_query_builder_insert_update(c: &mut Criterion) {
    use sz_orm_core::Model;
    use sz_orm_core::QueryBuilder;

    struct BenchModel;
    impl Model for BenchModel {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "bench_table"
        }
        fn pk(&self) -> Self::PrimaryKey {
            0
        }
        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
    }

    let mut group = c.benchmark_group("query_builder_insert_update");
    group.throughput(Throughput::Elements(1));

    // 构造测试数据
    fn make_data(n: usize) -> HashMap<String, Value> {
        let mut data = HashMap::new();
        data.insert("id".to_string(), Value::I64(1));
        for i in 0..n {
            data.insert(format!("col_{}", i), Value::String(format!("value_{}", i)));
        }
        data
    }

    // INSERT 5 列
    let data_5 = make_data(4);
    group.bench_function("insert_5_cols", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect)).table("bench_table");
            black_box(qb.build_insert(black_box(&data_5)))
        })
    });

    // INSERT 20 列
    let data_20 = make_data(19);
    group.bench_function("insert_20_cols", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect)).table("bench_table");
            black_box(qb.build_insert(black_box(&data_20)))
        })
    });

    // UPDATE 5 列 + WHERE
    group.bench_function("update_5_cols_where", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .table("bench_table")
                .where_cond("id = 1");
            black_box(qb.build_update(black_box(&data_5)))
        })
    });

    // DELETE + WHERE
    group.bench_function("delete_where", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .table("bench_table")
                .where_cond("id = 1");
            black_box(qb.build_delete())
        })
    });

    group.finish();
}

// ============================================================================
// Value::to_param 批量性能（模拟批量 INSERT 场景）
// ============================================================================

fn bench_value_batch_to_param(c: &mut Criterion) {
    let mut group = c.benchmark_group("value_batch_to_param");

    for n in [10usize, 100, 1000].iter() {
        // 准备混合类型 Values
        let values: Vec<Value> = (0..*n)
            .map(|i| match i % 4 {
                0 => Value::I64(i as i64),
                1 => Value::F64(i as f64 * 1.5),
                2 => Value::String(format!("str_{}", i)),
                _ => Value::Bool(i % 2 == 0),
            })
            .collect();

        group.throughput(Throughput::Elements(*n as u64));
        group.bench_with_input(BenchmarkId::new("mixed_types", n), &values, |b, vals| {
            b.iter(|| {
                let params: Vec<_> = vals.iter().map(|v| v.to_param()).collect();
                black_box(params)
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_value_to_param,
    bench_dialect_escape_string,
    bench_dialect_build_create_table,
    bench_dialect_build_pagination,
    bench_pool_acquire_release,
    bench_in_memory_scan,
    bench_json_parsing,
    bench_query_builder_select,
    bench_query_builder_insert_update,
    bench_value_batch_to_param,
);
criterion_main!(benches);
