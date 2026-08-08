//! SmartEagerLoader Oracle 方言集成测试（v2.4.0 任务 2.4）
//!
//! 需真实 Oracle 23ai Free 服务，标注 #[ignore]。

mod common;

use common::equivalence;
use common::schema_builder::{TestDialect, TestSchemaBuilder};
use oracle::sql_type::ToSql;
use oracle::Connection as OracleConn;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use sz_orm_core::eager_loader::{EagerLoader, NestedEagerResult};
use sz_orm_core::relation_trait::{RelationDef, RelationKind};
use sz_orm_core::smart_eager_loader::{LoadStrategy, SmartEagerLoader, StrategyResolver};
use sz_orm_core::{Connection, DbError, Value};

const ORACLE_USER: &str = "sz_orm_test";
const ORACLE_PASS: &str = "SzOrmTest2026";
const ORACLE_CONN_STR: &str = "127.0.0.1:1521/freepdb1.FALSE";

pub struct OracleConnection {
    conn: Mutex<OracleConn>,
}

impl OracleConnection {
    pub fn new(conn: OracleConn) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    fn convert_placeholders(sql: &str) -> String {
        let mut result = String::with_capacity(sql.len() + 16);
        let mut n = 1;
        let mut in_quote = false;
        for c in sql.chars() {
            if c == '\'' {
                in_quote = !in_quote;
            }
            if c == '?' && !in_quote {
                result.push_str(&format!(":{}", n));
                n += 1;
            } else {
                result.push(c);
            }
        }
        result
    }

    fn row_to_map(row: &oracle::Row) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        for (i, info) in row.column_info().iter().enumerate() {
            let name = info.name().to_string();
            let val = Self::try_get_oracle_value(row, i);
            map.insert(name, val);
        }
        map
    }

    fn try_get_oracle_value(row: &oracle::Row, idx: usize) -> Value {
        if let Ok(v) = row.get::<_, Option<i32>>(idx) {
            return v.map(Value::I32).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.get::<_, Option<i64>>(idx) {
            return v.map(Value::I64).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.get::<_, Option<f64>>(idx) {
            return v.map(Value::F64).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.get::<_, Option<String>>(idx) {
            return v.map(Value::String).unwrap_or(Value::Null);
        }
        Value::Null
    }

    fn value_to_oracle(v: &Value) -> Box<dyn ToSql> {
        match v {
            Value::Null => Box::new(None::<i64>),
            Value::Bool(b) => Box::new(*b as i32),
            Value::I8(i) => Box::new(*i as i32),
            Value::I16(i) => Box::new(*i as i32),
            Value::I32(i) => Box::new(*i),
            Value::I64(i) => Box::new(*i),
            Value::U8(u) => Box::new(*u as i32),
            Value::U16(u) => Box::new(*u as i32),
            Value::U32(u) => Box::new(*u as i64),
            Value::U64(u) => Box::new(*u as i64),
            Value::F32(f) => Box::new(*f as f64),
            Value::F64(f) => Box::new(*f),
            Value::String(s) => Box::new(s.clone()),
            _ => Box::new(None::<i64>),
        }
    }
}

impl Connection for OracleConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let ora_sql = Self::convert_placeholders(sql);
        let result = self
            .conn
            .lock()
            .unwrap()
            .execute(&ora_sql, &[])
            .map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { result.map(|r| r.row_count().unwrap_or(0)) })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        let ora_sql = Self::convert_placeholders(sql);
        let conn = self.conn.lock().unwrap();
        match conn.query(&ora_sql, &[]) {
            Ok(iter) => {
                let rows = iter
                    .filter_map(|r| r.ok().map(|row| Self::row_to_map(&row)))
                    .collect::<Vec<_>>();
                Box::pin(async move { Ok(rows) })
            }
            Err(e) => Box::pin(async move { Err(DbError::QueryError(e.to_string())) }),
        }
    }

    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        let ora_sql = Self::convert_placeholders(sql);
        let owned: Vec<Box<dyn ToSql>> = params.iter().map(Self::value_to_oracle).collect();
        let refs: Vec<&dyn ToSql> = owned.iter().map(|b| b.as_ref()).collect();
        let conn = self.conn.lock().unwrap();
        match conn.query(&ora_sql, &refs) {
            Ok(iter) => {
                let rows = iter
                    .filter_map(|r| r.ok().map(|row| Self::row_to_map(&row)))
                    .collect::<Vec<_>>();
                Box::pin(async move { Ok(rows) })
            }
            Err(e) => Box::pin(async move { Err(DbError::QueryError(e.to_string())) }),
        }
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let result = self
            .conn
            .lock()
            .unwrap()
            .execute("BEGIN", &[])
            .map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { result.map(|_| ()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let result = self
            .conn
            .lock()
            .unwrap()
            .commit()
            .map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { result })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let result = self
            .conn
            .lock()
            .unwrap()
            .rollback()
            .map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { result })
    }

    fn is_connected(&self) -> bool {
        true
    }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }
    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

fn setup_oracle() -> OracleConnection {
    let conn = OracleConn::connect(ORACLE_USER, ORACLE_PASS, ORACLE_CONN_STR)
        .expect("Oracle 不可用: 127.0.0.1:1521/freepdb1");
    let adapter = OracleConnection::new(conn);
    let builder = TestSchemaBuilder::new(TestDialect::Oracle);
    for ddl in builder.build_ddl() {
        adapter.conn.lock().unwrap().execute(&ddl, &[]).unwrap();
    }
    for sql in builder.seed_data() {
        adapter.conn.lock().unwrap().execute(&sql, &[]).unwrap();
    }
    adapter
}

fn teardown_oracle(conn: &OracleConnection) {
    let builder = TestSchemaBuilder::new(TestDialect::Oracle);
    for ddl in builder.teardown_ddl() {
        let _ = conn.conn.lock().unwrap().execute(&ddl, &[]);
    }
}

fn extract_children_rows(nested: &[NestedEagerResult]) -> Vec<HashMap<String, Value>> {
    nested
        .iter()
        .flat_map(|n| n.children().iter().map(|c| c.row().clone()))
        .collect()
}

fn extract_related_rows(
    eager: &[(HashMap<String, Value>, Vec<HashMap<String, Value>>)],
) -> Vec<HashMap<String, Value>> {
    eager.iter().flat_map(|(_, r)| r.iter().cloned()).collect()
}

#[tokio::test]
#[ignore]
async fn test_hasone_equivalent_oracle() {
    let mut conn = setup_oracle();
    let rel = RelationDef::new(
        "profile",
        "users",
        "profiles",
        "id",
        "user_id",
        RelationKind::HasOne,
    );
    let smart = SmartEagerLoader::new(rel.clone())
        .load(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();
    let manual = EagerLoader::new(rel)
        .load_many(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();
    let sr: Vec<_> = smart.iter().map(|n| n.row().clone()).collect();
    let mr: Vec<_> = manual.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&sr, &mr, RelationKind::HasOne, "");
    equivalence::assert_eager_equivalent(
        &extract_children_rows(&smart),
        &extract_related_rows(&manual),
        RelationKind::HasOne,
        "user_id",
    );
    teardown_oracle(&conn);
}

#[tokio::test]
#[ignore]
async fn test_hasmany_equivalent_oracle() {
    let mut conn = setup_oracle();
    let rel = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );
    let smart = SmartEagerLoader::new(rel.clone())
        .load(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();
    let manual = EagerLoader::new(rel)
        .load_many(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();
    let sr: Vec<_> = smart.iter().map(|n| n.row().clone()).collect();
    let mr: Vec<_> = manual.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&sr, &mr, RelationKind::HasMany, "");
    equivalence::assert_eager_equivalent(
        &extract_children_rows(&smart),
        &extract_related_rows(&manual),
        RelationKind::HasMany,
        "user_id",
    );
    teardown_oracle(&conn);
}

#[tokio::test]
#[ignore]
async fn test_many_to_many_equivalent_oracle() {
    let mut conn = setup_oracle();
    let rel = RelationDef::new_many_to_many(
        "roles",
        "users",
        "roles",
        "id",
        "id",
        "user_roles",
        "user_id",
        "role_id",
    );
    let smart = SmartEagerLoader::new(rel.clone())
        .load(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();
    let manual = EagerLoader::new(rel)
        .load_many(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();
    let sr: Vec<_> = smart.iter().map(|n| n.row().clone()).collect();
    let mr: Vec<_> = manual.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&sr, &mr, RelationKind::ManyToMany, "");
    teardown_oracle(&conn);
}

#[tokio::test]
#[ignore]
async fn test_join_strategy_oracle() {
    let rel = RelationDef::new(
        "profile",
        "users",
        "profiles",
        "id",
        "user_id",
        RelationKind::HasOne,
    );
    equivalence::assert_strategy_selected(
        &StrategyResolver::new().resolve(&rel),
        LoadStrategy::Join,
    );
}

#[tokio::test]
#[ignore]
async fn test_dataloader_strategy_oracle() {
    let rel = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );
    equivalence::assert_strategy_selected(
        &StrategyResolver::new().resolve(&rel),
        LoadStrategy::DataLoader,
    );
}

#[tokio::test]
#[ignore]
async fn test_intermediate_strategy_oracle() {
    let rel = RelationDef::new_many_to_many(
        "roles",
        "users",
        "roles",
        "id",
        "id",
        "user_roles",
        "user_id",
        "role_id",
    );
    equivalence::assert_strategy_selected(
        &StrategyResolver::new().resolve(&rel),
        LoadStrategy::IntermediateTableBatch,
    );
}

#[tokio::test]
#[ignore]
async fn test_nested_depth_oracle() {
    let mut conn = setup_oracle();
    let rel = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );
    let smart = SmartEagerLoader::new(rel.clone())
        .load(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();
    let manual = EagerLoader::new(rel)
        .load_many(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();
    assert_eq!(smart.len(), manual.len());
    for (s, (_, m)) in smart.iter().zip(manual.iter()) {
        assert_eq!(s.children().len(), m.len());
    }
    teardown_oracle(&conn);
}
