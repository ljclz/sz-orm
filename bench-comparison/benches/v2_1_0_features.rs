//! v2.1.0 新功能基准测试
//!
//! 对比 sz-orm v2.1.0 新功能性能：
//! - Eager Loading（M3）vs N+1 查询
//! - Nested Save（M4）vs 逐条 INSERT
//! - Schema Sync（M5）diff 性能
//! - Stream API（M6）vs 全量收集

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sz_orm_core::eager_loader::eager_load_all;
use sz_orm_core::mock::{MockConnection, MockRow};
use sz_orm_core::nested_active_model::{ChildEntity, NestedActiveModel};
use sz_orm_core::relation_trait::{RelationDef, RelationKind};
use sz_orm_core::schema_sync::{diff, ColumnDef, TableDef};
use sz_orm_core::Value;
use sz_orm_core::Connection;
use sz_orm_core::active_model::ActiveModel;
use sz_orm_core::Model;

// ============================================================================
// 测试模型
// ============================================================================

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
    fn pk_as_value(&self) -> Value {
        Value::I64(self.id)
    }
}

fn order_relation() -> RelationDef {
    RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    )
}

// ============================================================================
// Eager Loading 基准
// ============================================================================

fn bench_eager_loading(c: &mut Criterion) {
    let mut group = c.benchmark_group("eager_loading");

    for n in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*n as u64));

        // Eager Loading（2 条 SQL）
        group.bench_with_input(BenchmarkId::new("eager_load", n), n, |b, &n| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async move {
                    let mut mock = MockConnection::new();

                    let main_rows: Vec<MockRow> = (1..=n)
                        .map(|i| MockRow::from(vec![("id", Value::I64(i as i64))]))
                        .collect();
                    mock.expect_any().with_rows(main_rows);

                    let related_rows: Vec<MockRow> = (1..=n)
                        .map(|i| {
                            MockRow::from(vec![
                                ("id", Value::I64(10000 + i as i64)),
                                ("user_id", Value::I64(i as i64)),
                            ])
                        })
                        .collect();
                    mock.expect_any().with_rows(related_rows);

                    let relation = order_relation();
                    let results = eager_load_all(&mut mock, "SELECT * FROM users", &relation)
                        .await
                        .unwrap();
                    black_box(results);
                });
        });

        // N+1 查询（n+1 条 SQL）
        group.bench_with_input(BenchmarkId::new("n_plus_1", n), n, |b, &n| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async move {
                    let mut mock = MockConnection::new();

                    let main_rows: Vec<MockRow> = (1..=n)
                        .map(|i| MockRow::from(vec![("id", Value::I64(i as i64))]))
                        .collect();
                    mock.expect_any().with_rows(main_rows);

                    // N+1：逐条查询
                    for i in 1..=n {
                        mock.expect_any().with_rows(vec![MockRow::from(vec![
                            ("id", Value::I64(10000 + i as i64)),
                            ("user_id", Value::I64(i as i64)),
                        ])]);
                    }

                    let main_rows = mock.query("SELECT * FROM users").await.unwrap();
                    let mut all_results = Vec::new();
                    for row in main_rows {
                        let pk = row.get("id").cloned().unwrap_or(Value::Null);
                        let related = mock
                            .query_with_params("SELECT * FROM orders WHERE user_id = ?", &[pk])
                            .await
                            .unwrap();
                        all_results.push((row, related));
                    }
                    black_box(all_results);
                });
        });
    }

    group.finish();
}

// ============================================================================
// Nested Save 基准
// ============================================================================

fn bench_nested_save(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_save");

    for n in [1, 10, 100].iter() {
        group.throughput(Throughput::Elements(*n as u64));

        // Nested Save（事务内 1 + n 条 INSERT）
        group.bench_with_input(BenchmarkId::new("nested_save", n), n, |b, &n| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async move {
                    let mut mock = MockConnection::new();

                    mock.expect_any().with_rows(vec![]); // begin
                    mock.expect_any().with_rows(vec![]); // INSERT parent
                    mock.expect_any()
                        .with_rows(vec![MockRow::from(vec![("id", Value::I64(1))])]); // last_insert_id
                    for _ in 0..n {
                        mock.expect_any().with_rows(vec![]); // INSERT child
                    }

                    let mut user = ActiveModel::from_model(User::default());
                    user.set("name", "Alice".into());

                    let children: Vec<ChildEntity> = (0..n)
                        .map(|_| {
                            ChildEntity::new(
                                "orders",
                                vec![("amount".to_string(), Value::F64(100.0))],
                            )
                        })
                        .collect();

                    let nested = NestedActiveModel::from_model(user, order_relation())
                        .with_children(children);

                    let result =
                        sz_orm_core::nested_active_model::nested_save(&mut mock, nested)
                            .await
                            .unwrap();
                    black_box(result);
                });
        });
    }

    group.finish();
}

// ============================================================================
// Schema Sync diff 基准
// ============================================================================

fn bench_schema_diff(c: &mut Criterion) {
    let mut group = c.benchmark_group("schema_diff");

    for n in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*n as u64));

        group.bench_with_input(BenchmarkId::new("diff", n), n, |b, &n| {
            b.iter(|| {
                let entity_tables: Vec<TableDef> = (0..n)
                    .map(|i| {
                        TableDef::new(
                            format!("table_{}", i),
                            vec![
                                ColumnDef::new("id", "BIGINT", false, true, None),
                                ColumnDef::new("name", "VARCHAR(255)", true, false, None),
                                ColumnDef::new("email", "VARCHAR(255)", true, false, None),
                            ],
                        )
                    })
                    .collect();

                let db_tables: Vec<TableDef> = (0..n)
                    .map(|i| {
                        TableDef::new(
                            format!("table_{}", i),
                            vec![
                                ColumnDef::new("id", "BIGINT", false, true, None),
                                ColumnDef::new("name", "VARCHAR(255)", true, false, None),
                            ],
                        )
                    })
                    .collect();

                let result = diff(black_box(&entity_tables), black_box(&db_tables));
                black_box(result);
            });
        });
    }

    group.finish();
}

// ============================================================================
// Stream API 基准
// ============================================================================

fn bench_stream_api(c: &mut Criterion) {
    let mut group = c.benchmark_group("stream_api");

    for n in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*n as u64));

        group.bench_with_input(BenchmarkId::new("stream_buffered", n), n, |b, &n| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async move {
                    let mut mock = MockConnection::new();

                    let rows: Vec<MockRow> = (0..n)
                        .map(|i| MockRow::from(vec![("id", Value::I64(i as i64))]))
                        .collect();
                    mock.expect_any().with_rows(rows);

                    use futures::StreamExt;
                    use sz_orm_core::stream_api::StreamApiExt;

                    let dialect = sz_orm_core::dialect::get_dialect(sz_orm_core::DbType::Sqlite)
                        .unwrap();
                    let query =
                        sz_orm_core::QueryBuilder::<User>::new(dialect).table("users");

                    let mut stream = query.stream_buffered(&mut mock);
                    let mut count = 0;
                    while stream.next().await.is_some() {
                        count += 1;
                    }
                    black_box(count);
                });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_eager_loading,
    bench_nested_save,
    bench_schema_diff,
    bench_stream_api,
);
criterion_main!(benches);