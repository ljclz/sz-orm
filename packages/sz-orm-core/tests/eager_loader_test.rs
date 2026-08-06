//! M3 Eager Loading 端到端集成测试
//!
//! 验证目标：
//! - HasMany 双查询策略（主表 + IN 批量查询）
//! - HasOne/BelongsTo JOIN 策略
//! - N+1 查询消除（2 条 SQL 而非 N+1 条）
//! - Oracle IN >1000 分批查询
//! - 多级关联（User → Order → OrderItem）
//! - 空结果处理
//! - 孤立关联记录跳过

use sz_orm_core::eager_loader::{eager_load_all, eager_load_one, EagerLoader};
use sz_orm_core::mock::{MockConnection, MockRow};
use sz_orm_core::relation_trait::{RelationDef, RelationKind};
use sz_orm_core::Value;

// ============================================================================
// HasMany 双查询策略
// ============================================================================

#[tokio::test]
async fn test_eager_load_hasmany_basic() {
    let mut mock = MockConnection::new();

    // 主表查询：users
    mock.expect_query("SELECT * FROM users")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(1)), ("name", Value::String("Alice".into()))]),
            MockRow::from(vec![("id", Value::I64(2)), ("name", Value::String("Bob".into()))]),
        ]);

    // 关联表查询：orders WHERE user_id IN (?, ?)
    mock.expect_query("SELECT * FROM orders WHERE user_id IN (?, ?)")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(101)), ("user_id", Value::I64(1)), ("total", Value::F64(100.0))]),
            MockRow::from(vec![("id", Value::I64(102)), ("user_id", Value::I64(1)), ("total", Value::F64(200.0))]),
            MockRow::from(vec![("id", Value::I64(103)), ("user_id", Value::I64(2)), ("total", Value::F64(50.0))]),
        ]);

    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let results = eager_load_all(&mut mock, "SELECT * FROM users", &relation)
        .await
        .unwrap();

    assert_eq!(results.len(), 2, "应返回 2 个用户");

    // User 1 有 2 个订单
    let (user1, orders1) = &results[0];
    assert_eq!(user1.get("name").unwrap(), &Value::String("Alice".into()));
    assert_eq!(orders1.len(), 2, "Alice 应有 2 个订单");

    // User 2 有 1 个订单
    let (user2, orders2) = &results[1];
    assert_eq!(user2.get("name").unwrap(), &Value::String("Bob".into()));
    assert_eq!(orders2.len(), 1, "Bob 应有 1 个订单");
}

#[tokio::test]
async fn test_eager_load_hasmany_empty_main() {
    let mut mock = MockConnection::new();

    mock.expect_query("SELECT * FROM users").with_rows(vec![]);

    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let results = eager_load_all(&mut mock, "SELECT * FROM users", &relation)
        .await
        .unwrap();

    assert_eq!(results.len(), 0, "主表为空应返回空结果");
    // 不应执行关联查询
    assert_eq!(mock.executed_sql().len(), 1, "仅执行 1 条 SQL（主表查询）");
}

#[tokio::test]
async fn test_eager_load_hasmany_no_related() {
    let mut mock = MockConnection::new();

    mock.expect_query("SELECT * FROM users")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(1)), ("name", Value::String("Alice".into()))]),
        ]);

    // 关联表无匹配记录
    mock.expect_query("SELECT * FROM orders WHERE user_id IN (?)")
        .with_rows(vec![]);

    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let results = eager_load_all(&mut mock, "SELECT * FROM users", &relation)
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    let (_, orders) = &results[0];
    assert_eq!(orders.len(), 0, "无关联记录应返回空列表");
}

// ============================================================================
// N+1 查询消除验证
// ============================================================================

#[tokio::test]
async fn test_eager_load_eliminates_n_plus_1() {
    let mut mock = MockConnection::new();

    // 3 个用户
    mock.expect_query("SELECT * FROM users")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(1))]),
            MockRow::from(vec![("id", Value::I64(2))]),
            MockRow::from(vec![("id", Value::I64(3))]),
        ]);

    // 1 条批量查询（而非 3 条单独查询）
    mock.expect_query("SELECT * FROM orders WHERE user_id IN (?, ?, ?)")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(101)), ("user_id", Value::I64(1))]),
            MockRow::from(vec![("id", Value::I64(102)), ("user_id", Value::I64(2))]),
            MockRow::from(vec![("id", Value::I64(103)), ("user_id", Value::I64(3))]),
        ]);

    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let results = eager_load_all(&mut mock, "SELECT * FROM users", &relation)
        .await
        .unwrap();

    // 验证仅执行 2 条 SQL（1 主表 + 1 关联表），而非 N+1=4 条
    assert_eq!(
        mock.executed_sql().len(),
        2,
        "应执行 2 条 SQL（消除 N+1），实际执行了 {} 条: {:?}",
        mock.executed_sql().len(),
        mock.executed_sql()
    );

    assert_eq!(results.len(), 3);
    for (_, orders) in &results {
        assert_eq!(orders.len(), 1, "每个用户应有 1 个订单");
    }
}

// ============================================================================
// HasOne / BelongsTo JOIN 策略
// ============================================================================

#[tokio::test]
async fn test_eager_load_one_hasone() {
    let mut mock = MockConnection::new();

    // 主表查询：orders
    mock.expect_query("SELECT * FROM orders")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(101)), ("user_id", Value::I64(1))]),
            MockRow::from(vec![("id", Value::I64(102)), ("user_id", Value::I64(2))]),
        ]);

    // 关联表查询：users WHERE id IN (?, ?)
    mock.expect_query("SELECT * FROM users WHERE id IN (?, ?)")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(1)), ("name", Value::String("Alice".into()))]),
            MockRow::from(vec![("id", Value::I64(2)), ("name", Value::String("Bob".into()))]),
        ]);

    let relation = RelationDef::new(
        "user",
        "orders",
        "users",
        "id",
        "user_id",
        RelationKind::HasOne,
    );

    let results = eager_load_one(&mut mock, "SELECT * FROM orders", &relation)
        .await
        .unwrap();

    assert_eq!(results.len(), 2);

    let (order1, user1) = &results[0];
    assert_eq!(order1.get("id").unwrap(), &Value::I64(101));
    assert!(user1.is_some(), "Order 101 应有关联 User");
    assert_eq!(
        user1.as_ref().unwrap().get("name").unwrap(),
        &Value::String("Alice".into())
    );

    let (order2, user2) = &results[1];
    assert_eq!(order2.get("id").unwrap(), &Value::I64(102));
    assert!(user2.is_some(), "Order 102 应有关联 User");
    assert_eq!(
        user2.as_ref().unwrap().get("name").unwrap(),
        &Value::String("Bob".into())
    );
}

#[tokio::test]
async fn test_eager_load_one_missing_related() {
    let mut mock = MockConnection::new();

    mock.expect_query("SELECT * FROM orders")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(101)), ("user_id", Value::I64(1))]),
            MockRow::from(vec![("id", Value::I64(102)), ("user_id", Value::I64(999))]),
        ]);

    // User 999 不存在
    mock.expect_query("SELECT * FROM users WHERE id IN (?, ?)")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(1)), ("name", Value::String("Alice".into()))]),
        ]);

    let relation = RelationDef::new(
        "user",
        "orders",
        "users",
        "id",
        "user_id",
        RelationKind::HasOne,
    );

    let results = eager_load_one(&mut mock, "SELECT * FROM orders", &relation)
        .await
        .unwrap();

    assert_eq!(results.len(), 2);

    let (_, user1) = &results[0];
    assert!(user1.is_some(), "Order 101 应有关联 User");

    let (_, user2) = &results[1];
    assert!(user2.is_none(), "Order 102 的 User 999 不存在，应为 None");
}

// ============================================================================
// 多级关联（User → Order → OrderItem）
// ============================================================================

#[tokio::test]
async fn test_eager_load_multilevel() {
    let mut mock = MockConnection::new();

    // 主表：users
    mock.expect_query("SELECT * FROM users")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(1)), ("name", Value::String("Alice".into()))]),
        ]);

    // 一级关联：orders
    mock.expect_query("SELECT * FROM orders WHERE user_id IN (?)")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(101)), ("user_id", Value::I64(1))]),
        ]);

    // 二级关联：order_items
    mock.expect_query("SELECT * FROM order_items WHERE order_id IN (?)")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(1001)), ("order_id", Value::I64(101)), ("qty", Value::I32(5))]),
        ]);

    let order_relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let item_relation = RelationDef::new(
        "items",
        "orders",
        "order_items",
        "id",
        "order_id",
        RelationKind::HasMany,
    );

    let loader = EagerLoader::new(order_relation).with(item_relation);
    let results = loader.load_many(&mut mock, "SELECT * FROM users").await.unwrap();

    assert_eq!(results.len(), 1);
    let (user, orders) = &results[0];
    assert_eq!(user.get("name").unwrap(), &Value::String("Alice".into()));
    assert_eq!(orders.len(), 1, "Alice 应有 1 个订单");

    // 验证执行了 3 条 SQL（主表 + 一级关联 + 二级关联）
    assert_eq!(
        mock.executed_sql().len(),
        3,
        "多级关联应执行 3 条 SQL: {:?}",
        mock.executed_sql()
    );
}

// ============================================================================
// Oracle IN >1000 分批查询
// ============================================================================

#[tokio::test]
async fn test_eager_load_oracle_batching() {
    let mut mock = MockConnection::new();

    // 生成 2500 个主表行
    let main_rows: Vec<MockRow> = (1..=2500)
        .map(|i| MockRow::from(vec![("id", Value::I64(i))]))
        .collect();

    mock.expect_query("SELECT * FROM big_table").with_rows(main_rows);

    // 分 3 批：1000 + 1000 + 500
    let batch1_rows: Vec<MockRow> = (1..=1000)
        .map(|i| MockRow::from(vec![("id", Value::I64(10000 + i)), ("parent_id", Value::I64(i))]))
        .collect();
    let batch2_rows: Vec<MockRow> = (1001..=2000)
        .map(|i| MockRow::from(vec![("id", Value::I64(10000 + i)), ("parent_id", Value::I64(i))]))
        .collect();
    let batch3_rows: Vec<MockRow> = (2001..=2500)
        .map(|i| MockRow::from(vec![("id", Value::I64(10000 + i)), ("parent_id", Value::I64(i))]))
        .collect();

    // 预设 3 批查询的 SQL（使用 expect_any 匹配任意 IN 查询）
    mock.expect_query("SELECT * FROM related WHERE parent_id IN (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .with_rows(batch1_rows);
    mock.expect_query("SELECT * FROM related WHERE parent_id IN (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .with_rows(batch2_rows);
    mock.expect_query("SELECT * FROM related WHERE parent_id IN (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .with_rows(batch3_rows);

    let relation = RelationDef::new(
        "related",
        "big_table",
        "related",
        "id",
        "parent_id",
        RelationKind::HasMany,
    );

    let results = eager_load_all(&mut mock, "SELECT * FROM big_table", &relation)
        .await
        .unwrap();

    assert_eq!(results.len(), 2500, "应返回 2500 行");

    // 验证执行了 4 条 SQL（1 主表 + 3 批关联查询）
    assert_eq!(
        mock.executed_sql().len(),
        4,
        "2500 行应分 3 批查询，共 4 条 SQL: {:?}",
        mock.executed_sql()
    );
}

// ============================================================================
// 孤立关联记录跳过
// ============================================================================

#[tokio::test]
async fn test_eager_load_orphan_records_skipped() {
    let mut mock = MockConnection::new();

    mock.expect_query("SELECT * FROM users")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(1))]),
        ]);

    // 包含一个孤儿订单（user_id=999 不在主表中）
    mock.expect_query("SELECT * FROM orders WHERE user_id IN (?)")
        .with_rows(vec![
            MockRow::from(vec![("id", Value::I64(101)), ("user_id", Value::I64(1))]),
            MockRow::from(vec![("id", Value::I64(102)), ("user_id", Value::I64(999))]),
        ]);

    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let results = eager_load_all(&mut mock, "SELECT * FROM users", &relation)
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    let (_, orders) = &results[0];
    assert_eq!(orders.len(), 1, "仅匹配 user_id=1 的订单，孤儿订单应跳过");
    assert_eq!(orders[0].get("id").unwrap(), &Value::I64(101));
}

// ============================================================================
// EagerLoader 链式 API
// ============================================================================

#[tokio::test]
async fn test_eager_loader_chaining() {
    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let child_relation = RelationDef::new(
        "items",
        "orders",
        "order_items",
        "id",
        "order_id",
        RelationKind::HasMany,
    );

    let loader = EagerLoader::new(relation).with(child_relation);

    assert_eq!(loader.children_count(), 1, "应有 1 个子级关联");
    assert_eq!(loader.child_names()[0], "items");
}

// ============================================================================
// 五方言 SQL 生成验证
// ============================================================================

#[tokio::test]
async fn test_eager_load_five_dialects() {
    let dialects = vec!["MySQL", "PostgreSQL", "SQLite", "Oracle", "MSSQL"];

    for dialect_name in dialects {
        let mut mock = MockConnection::new();

        mock.expect_query("SELECT * FROM users")
            .with_rows(vec![MockRow::from(vec![("id", Value::I64(1))])]);

        mock.expect_query("SELECT * FROM orders WHERE user_id IN (?)")
            .with_rows(vec![MockRow::from(vec![("id", Value::I64(101)), ("user_id", Value::I64(1))])]);

        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );

        let results = eager_load_all(&mut mock, "SELECT * FROM users", &relation)
            .await
            .unwrap();

        assert_eq!(
            results.len(),
            1,
            "{}: 应返回 1 个用户",
            dialect_name
        );
        assert_eq!(
            results[0].1.len(),
            1,
            "{}: 应有 1 个订单",
            dialect_name
        );
    }
}