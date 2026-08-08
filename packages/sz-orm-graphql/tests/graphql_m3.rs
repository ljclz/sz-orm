#![cfg(all(
    feature = "graphql-n1",
    feature = "graphql-schema-gen",
    feature = "graphql-complexity"
))]

//! M3 GraphQL 集成测试
//!
//! 覆盖：
//! - M3-T13: DataLoader 差分测试（批量 vs 逐条结果完全一致）
//! - M3-T14.1: N+1 消除集成测试（查询次数 ≤ 2）
//! - M3-T14.2: Schema 自动生成集成测试
//! - M3-T14.3: 复杂度限制集成测试
//! - M3-T14.4: GraphQL 变量注入防护集成测试

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sz_orm_graphql::complexity::{ComplexityCalculator, ComplexityConfig, ComplexityError};
use sz_orm_graphql::dataloader::{BatchLoadError, BatchLoader, DataLoader, DataLoaderResolver};
use sz_orm_graphql::query_ir::{parse_query, GraphQLOperation, GraphQLValue};
use sz_orm_graphql::schema_gen::{ColumnMeta, GraphQLModelInfo, SchemaGenerator};

// =========================================================================
// M3-T13: DataLoader 差分测试
// =========================================================================

struct OrderByUserLoader;

impl BatchLoader<i64, Vec<(i64, String)>> for OrderByUserLoader {
    fn batch_load(
        &self,
        keys: Vec<i64>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<HashMap<i64, Vec<(i64, String)>>, BatchLoadError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let mut map = HashMap::new();
            for &k in &keys {
                let orders: Vec<(i64, String)> = (1..=3)
                    .map(|i| (k * 100 + i, format!("order_{k}_{i}")))
                    .collect();
                map.insert(k, orders);
            }
            Ok(map)
        })
    }
}

#[tokio::test]
async fn test_differential_batch_vs_sequential() {
    let batch_results = {
        let loader = DataLoader::new(Arc::new(OrderByUserLoader));
        let f1 = loader.load(1);
        let f2 = loader.load(2);
        let f3 = loader.load(3);
        let f4 = loader.load(4);
        let f5 = loader.load(5);
        let (r1, r2, r3, r4, r5) = tokio::join!(f1, f2, f3, f4, f5);
        vec![
            r1.unwrap(),
            r2.unwrap(),
            r3.unwrap(),
            r4.unwrap(),
            r5.unwrap(),
        ]
    };

    let sequential_results: Vec<Vec<(i64, String)>> = {
        let loader = DataLoader::new(Arc::new(OrderByUserLoader));
        vec![
            loader.load(1).await.unwrap(),
            loader.load(2).await.unwrap(),
            loader.load(3).await.unwrap(),
            loader.load(4).await.unwrap(),
            loader.load(5).await.unwrap(),
        ]
    };

    assert_eq!(batch_results.len(), sequential_results.len());
    for (i, (batch, sequential)) in batch_results
        .iter()
        .zip(sequential_results.iter())
        .enumerate()
    {
        assert_eq!(batch, sequential, "mismatch at index {i}");
    }
}

// =========================================================================
// M3-T14.1: N+1 消除集成测试
// =========================================================================

struct CountingOrderLoader {
    load_count: Arc<AtomicUsize>,
}

impl BatchLoader<i64, String> for CountingOrderLoader {
    fn batch_load(
        &self,
        keys: Vec<i64>,
    ) -> Pin<Box<dyn Future<Output = Result<HashMap<i64, String>, BatchLoadError>> + Send + '_>>
    {
        let count = self.load_count.clone();
        Box::pin(async move {
            count.fetch_add(1, Ordering::SeqCst);
            let map: HashMap<i64, String> = keys
                .into_iter()
                .map(|k| (k, format!("order_for_user_{k}")))
                .collect();
            Ok(map)
        })
    }
}

#[tokio::test]
async fn test_n1_elimination_query_count_le_2() {
    let load_count = Arc::new(AtomicUsize::new(0));
    let resolver = DataLoaderResolver::new(Arc::new(CountingOrderLoader {
        load_count: load_count.clone(),
    }));

    let f1 = resolver.resolve_relation(1);
    let f2 = resolver.resolve_relation(2);
    let f3 = resolver.resolve_relation(3);
    let f4 = resolver.resolve_relation(4);
    let f5 = resolver.resolve_relation(5);
    let f6 = resolver.resolve_relation(6);
    let f7 = resolver.resolve_relation(7);
    let f8 = resolver.resolve_relation(8);
    let f9 = resolver.resolve_relation(9);
    let f10 = resolver.resolve_relation(10);

    let results = tokio::join!(f1, f2, f3, f4, f5, f6, f7, f8, f9, f10);
    let all = [
        results.0, results.1, results.2, results.3, results.4, results.5, results.6, results.7,
        results.8, results.9,
    ];

    for (i, r) in all.iter().enumerate() {
        assert!(r.is_ok(), "load {} failed", i + 1);
    }

    let batch_calls = load_count.load(Ordering::SeqCst);
    assert!(
        batch_calls <= 2,
        "expected ≤2 batch calls (main + relation), got {batch_calls}"
    );
}

#[tokio::test]
async fn test_n1_elimination_reduction_ge_90_percent() {
    let batch_count = Arc::new(AtomicUsize::new(0));
    let resolver = DataLoaderResolver::new(Arc::new(CountingOrderLoader {
        load_count: batch_count.clone(),
    }));

    let f1 = resolver.resolve_relation(1);
    let f2 = resolver.resolve_relation(2);
    let f3 = resolver.resolve_relation(3);
    let f4 = resolver.resolve_relation(4);
    let f5 = resolver.resolve_relation(5);
    let f6 = resolver.resolve_relation(6);
    let f7 = resolver.resolve_relation(7);
    let f8 = resolver.resolve_relation(8);
    let f9 = resolver.resolve_relation(9);
    let f10 = resolver.resolve_relation(10);
    let f11 = resolver.resolve_relation(11);
    let f12 = resolver.resolve_relation(12);
    let f13 = resolver.resolve_relation(13);
    let f14 = resolver.resolve_relation(14);
    let f15 = resolver.resolve_relation(15);
    let f16 = resolver.resolve_relation(16);
    let f17 = resolver.resolve_relation(17);
    let f18 = resolver.resolve_relation(18);
    let f19 = resolver.resolve_relation(19);
    let f20 = resolver.resolve_relation(20);

    let _ = tokio::join!(
        f1, f2, f3, f4, f5, f6, f7, f8, f9, f10, f11, f12, f13, f14, f15, f16, f17, f18, f19, f20
    );

    let batch_calls = batch_count.load(Ordering::SeqCst) as f64;
    let sequential_calls = 20.0;
    let reduction = (sequential_calls - batch_calls) / sequential_calls * 100.0;
    assert!(
        reduction >= 90.0,
        "expected ≥90% reduction, got {reduction:.1}% (batch={batch_calls}, sequential={sequential_calls})"
    );
}

// =========================================================================
// M3-T14.2: Schema 自动生成集成测试
// =========================================================================

struct Product;

impl GraphQLModelInfo for Product {
    fn table_name() -> &'static str {
        "products"
    }
    fn columns() -> Vec<ColumnMeta> {
        vec![
            ColumnMeta {
                name: "id".into(),
                rust_type: "i64".into(),
                nullable: false,
            },
            ColumnMeta {
                name: "name".into(),
                rust_type: "String".into(),
                nullable: false,
            },
            ColumnMeta {
                name: "price".into(),
                rust_type: "f64".into(),
                nullable: false,
            },
            ColumnMeta {
                name: "description".into(),
                rust_type: "Option<String>".into(),
                nullable: true,
            },
            ColumnMeta {
                name: "in_stock".into(),
                rust_type: "bool".into(),
                nullable: false,
            },
        ]
    }
}

#[test]
fn test_schema_auto_generation_for_execution() {
    let result = SchemaGenerator::from_model::<Product>();
    assert!(result.warnings.is_empty());

    let schema = result.schema;
    let sdl = schema.to_sdl();
    assert!(sdl.contains("type Product {"));
    assert!(sdl.contains("id: BigInt!"));
    assert!(sdl.contains("name: String!"));
    assert!(sdl.contains("price: Float!"));
    assert!(sdl.contains("description: String"));
    assert!(sdl.contains("in_stock: Boolean!"));
    assert!(sdl.contains("type Query {"));
    assert!(sdl.contains("getProduct: Product"));
    assert!(sdl.contains("listProducts: [Product!]!"));
    assert!(sdl.contains("type Mutation {"));
    assert!(sdl.contains("createProduct: Product"));
    assert!(sdl.contains("updateProduct: Product"));
    assert!(sdl.contains("deleteProduct: Boolean!"));
}

// =========================================================================
// M3-T14.3: 复杂度限制集成测试
// =========================================================================

#[test]
fn test_complexity_depth_limit_rejects() {
    let query = "{ a { b { c { d { e { f { x } } } } } } }";
    let ir = parse_query(query, None).unwrap();
    let config = ComplexityConfig::builder().max_depth(5).build();
    let calc = ComplexityCalculator::new(config);
    let result = calc.validate(&ir);
    assert!(matches!(result, Err(ComplexityError::DepthExceeded { .. })));
}

#[test]
fn test_complexity_fields_limit_rejects() {
    let mut fields = String::from("{ ");
    for i in 0..101 {
        fields.push_str(&format!("f{i} "));
    }
    fields.push('}');
    let ir = parse_query(&fields, None).unwrap();
    let config = ComplexityConfig::builder().max_fields(100).build();
    let calc = ComplexityCalculator::new(config);
    let result = calc.validate(&ir);
    assert!(matches!(
        result,
        Err(ComplexityError::FieldsExceeded { .. })
    ));
}

#[test]
fn test_complexity_cost_limit_rejects() {
    let query = "{ expensive { a b c d e f g h i j } }";
    let ir = parse_query(query, None).unwrap();
    let config = ComplexityConfig::builder()
        .field_weight("expensive", 100)
        .max_cost(50)
        .build();
    let calc = ComplexityCalculator::new(config);
    let result = calc.validate(&ir);
    assert!(matches!(result, Err(ComplexityError::CostExceeded { .. })));
}

#[test]
fn test_complexity_legal_query_passes() {
    let query = "{ user { id name } }";
    let ir = parse_query(query, None).unwrap();
    let calc = ComplexityCalculator::with_defaults();
    assert!(calc.validate(&ir).is_ok());
}

// =========================================================================
// M3-T14.4: GraphQL 变量注入防护集成测试
// =========================================================================

struct ParameterizedLoader {
    received_keys: Arc<std::sync::Mutex<Vec<String>>>,
}

impl BatchLoader<String, String> for ParameterizedLoader {
    fn batch_load(
        &self,
        keys: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<HashMap<String, String>, BatchLoadError>> + Send + '_>>
    {
        let received = self.received_keys.clone();
        Box::pin(async move {
            received.lock().unwrap().extend(keys.clone());
            let map: HashMap<String, String> = keys
                .into_iter()
                .map(|k| (k.clone(), format!("result_for_{k}")))
                .collect();
            Ok(map)
        })
    }
}

#[tokio::test]
async fn test_variable_injection_protection() {
    let received_keys = Arc::new(std::sync::Mutex::new(Vec::new()));
    let loader = DataLoader::new(Arc::new(ParameterizedLoader {
        received_keys: received_keys.clone(),
    }));

    let injection_payloads = vec![
        "1; DROP TABLE users; --",
        "' OR '1'='1",
        "1 UNION SELECT * FROM passwords",
        "'; EXEC xp_cmdshell('dir'); --",
    ];

    let f1 = loader.load(injection_payloads[0].to_string());
    let f2 = loader.load(injection_payloads[1].to_string());
    let f3 = loader.load(injection_payloads[2].to_string());
    let f4 = loader.load(injection_payloads[3].to_string());
    let (r1, r2, r3, r4) = tokio::join!(f1, f2, f3, f4);

    assert!(r1.is_ok());
    assert!(r2.is_ok());
    assert!(r3.is_ok());
    assert!(r4.is_ok());

    let received = received_keys.lock().unwrap();
    for payload in &injection_payloads {
        assert!(
            received.contains(&payload.to_string()),
            "injection payload should be received as parameter value: {payload}"
        );
    }
    assert_eq!(received.len(), injection_payloads.len());
}

#[test]
fn test_graphql_variable_preserved_as_parameter() {
    let query = "query GetUser($id: ID!) { user(id: $id) { id name } }";
    let ir = parse_query(query, None).unwrap();
    let arg = ir.selection_set[0].arguments.get("id").unwrap();
    assert!(
        matches!(arg, GraphQLValue::Variable(name) if name == "id"),
        "variable should be preserved as GraphQLValue::Variable, not interpolated"
    );
}

#[test]
fn test_graphql_operation_type_correct() {
    let queries = vec![
        ("{ user { id } }", GraphQLOperation::Query),
        ("query Foo { user { id } }", GraphQLOperation::Query),
        (
            "mutation Foo { createUser { id } }",
            GraphQLOperation::Mutation,
        ),
        (
            "subscription Foo { onUpdate { id } }",
            GraphQLOperation::Subscription,
        ),
    ];
    for (query, expected_op) in queries {
        let ir = parse_query(query, None).unwrap();
        assert_eq!(ir.operation, expected_op, "query: {query}");
    }
}
