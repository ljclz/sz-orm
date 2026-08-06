//! M1.4/M1.5 验证：QueryBuilder::join() / left_join() 链式方法
//!
//! 测试关联 JOIN 的 SQL 生成、五方言标识符引用、类型安全

use sz_orm_core::DbType;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::QueryBuilder;
use sz_orm_core::RelationTrait as RelationTraitMacro;
use sz_orm_core::Model;

#[derive(Clone, Default)]
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
}

#[derive(Clone, Default)]
struct Order {
    id: i64,
    user_id: i64,
}

impl Model for Order {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "orders"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

#[derive(RelationTraitMacro)]
#[table(name = "users")]
#[relation(has_many = "orders", fk = "user_id", pk = "id")]
struct UserEntity {
    id: i64,
    name: String,
}

#[derive(RelationTraitMacro)]
#[table(name = "orders")]
#[relation(belongs_to = "users", fk = "user_id", pk = "id")]
struct OrderEntity {
    id: i64,
    user_id: i64,
}

#[test]
fn test_join_generates_left_join_for_has_many() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let user = UserEntity {
        id: 1,
        name: "test".into(),
    };
    let sql = QueryBuilder::<User>::new(dialect)
        .table("users")
        .join(&user)
        .build_select();
    assert!(
        sql.contains("LEFT JOIN"),
        "HasMany should generate LEFT JOIN, got: {}",
        sql
    );
    assert!(
        sql.contains("`orders`"),
        "should contain quoted orders table, got: {}",
        sql
    );
    assert!(
        sql.contains("`users`.`id`"),
        "should contain ON users.id, got: {}",
        sql
    );
    assert!(
        sql.contains("`orders`.`user_id`"),
        "should contain ON orders.user_id, got: {}",
        sql
    );
}

#[test]
fn test_join_generates_inner_join_for_belongs_to() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let order = OrderEntity { id: 1, user_id: 1 };
    let sql = QueryBuilder::<Order>::new(dialect)
        .table("orders")
        .join(&order)
        .build_select();
    assert!(
        sql.contains("INNER JOIN"),
        "BelongsTo should generate INNER JOIN, got: {}",
        sql
    );
}

#[test]
fn test_left_join_forced() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let order = OrderEntity { id: 1, user_id: 1 };
    let sql = QueryBuilder::<Order>::new(dialect)
        .table("orders")
        .left_join(&order)
        .build_select();
    assert!(
        sql.contains("LEFT JOIN"),
        "left_join should always generate LEFT JOIN, got: {}",
        sql
    );
}

#[test]
fn test_join_postgresql_dialect() {
    let dialect = get_dialect(DbType::PostgreSQL).unwrap();
    let user = UserEntity {
        id: 1,
        name: "test".into(),
    };
    let sql = QueryBuilder::<User>::new(dialect)
        .table("users")
        .join(&user)
        .build_select();
    assert!(sql.contains("LEFT JOIN"));
    assert!(sql.contains("\"orders\""));
    assert!(sql.contains("\"users\".\"id\""));
    assert!(sql.contains("\"orders\".\"user_id\""));
}

#[test]
fn test_join_sqlite_dialect() {
    let dialect = get_dialect(DbType::Sqlite).unwrap();
    let user = UserEntity {
        id: 1,
        name: "test".into(),
    };
    let sql = QueryBuilder::<User>::new(dialect)
        .table("users")
        .join(&user)
        .build_select();
    assert!(sql.contains("LEFT JOIN"));
    assert!(sql.contains("\"orders\""));
}

#[test]
fn test_join_oracle_dialect() {
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let user = UserEntity {
        id: 1,
        name: "test".into(),
    };
    let sql = QueryBuilder::<User>::new(dialect)
        .table("users")
        .join(&user)
        .build_select();
    assert!(sql.contains("LEFT JOIN"));
    assert!(sql.contains("\"orders\""));
}

#[test]
fn test_join_mssql_dialect() {
    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let user = UserEntity {
        id: 1,
        name: "test".into(),
    };
    let sql = QueryBuilder::<User>::new(dialect)
        .table("users")
        .join(&user)
        .build_select();
    assert!(sql.contains("LEFT JOIN"));
    assert!(sql.contains("[orders]"));
}

#[test]
fn test_multi_join() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let user = UserEntity {
        id: 1,
        name: "test".into(),
    };
    let order = OrderEntity { id: 1, user_id: 1 };
    let sql = QueryBuilder::<User>::new(dialect)
        .table("users")
        .join(&user)
        .join(&order)
        .build_select();
    assert!(sql.contains("LEFT JOIN"));
    assert!(sql.contains("INNER JOIN"));
}

#[test]
fn test_join_with_params() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let user = UserEntity {
        id: 1,
        name: "test".into(),
    };
    let (sql, params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .join(&user)
        .build_select_with_params();
    assert!(sql.contains("LEFT JOIN"));
    assert!(params.is_empty());
}