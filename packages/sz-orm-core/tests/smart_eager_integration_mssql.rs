//! SmartEagerLoader MSSQL 方言集成测试（v2.4.0 任务 2.5）
//!
//! 需真实 SQL Server 服务，标注 #[ignore]。
//! 风险标记 R-01：远程连接不稳定，不可用时跳过。

mod common;

use common::equivalence;
use common::schema_builder::{TestDialect, TestSchemaBuilder};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use sz_orm_core::eager_loader::{EagerLoader, NestedEagerResult};
use sz_orm_core::relation_trait::{RelationDef, RelationKind};
use sz_orm_core::smart_eager_loader::{LoadStrategy, SmartEagerLoader, StrategyResolver};
use sz_orm_core::{Connection, DbError, Value};
use tiberius::{AuthMethod, Client, Config, ToSql};
use tokio::sync::Mutex;
use tokio_util::compat::TokioAsyncWriteCompatExt;

const MSSQL_HOST: &str = "127.0.0.1";
const MSSQL_PORT: u16 = 1433;
const MSSQL_USER: &str = "sa";
const MSSQL_PASS: &str = "SzOrmTest2026";
const MSSQL_DB: &str = "sz_orm_test";

type MsClient = Client<tokio_util::compat::Compat<tokio::net::TcpStream>>;

pub struct MssqlConnection {
    client: Mutex<MsClient>,
}

impl MssqlConnection {
    pub fn new(client: MsClient) -> Self {
        Self {
            client: Mutex::new(client),
        }
    }

    fn row_to_map(row: &tiberius::Row) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        for (i, col) in row.columns().iter().enumerate() {
            let name = col.name().to_string();
            let val = Self::try_get_mssql_value(row, i);
            map.insert(name, val);
        }
        map
    }

    fn try_get_mssql_value(row: &tiberius::Row, idx: usize) -> Value {
        if let Some(v) = row.get::<i32, _>(idx) {
            return Value::I32(v);
        }
        if let Some(v) = row.get::<i64, _>(idx) {
            return Value::I64(v);
        }
        if let Some(v) = row.get::<f64, _>(idx) {
            return Value::F64(v);
        }
        if let Some(v) = row.get::<&str, _>(idx) {
            return Value::String(v.to_string());
        }
        if let Some(v) = row.get::<bool, _>(idx) {
            return Value::Bool(v);
        }
        if let Some(v) = row.get::<&[u8], _>(idx) {
            return Value::Bytes(v.to_vec());
        }
        Value::Null
    }

    fn value_to_tiberius(v: &Value) -> Box<dyn ToSql> {
        match v {
            Value::Null => Box::new(None::<i32>),
            Value::Bool(b) => Box::new(*b),
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
            Value::Bytes(b) => Box::new(b.clone()),
            _ => Box::new(None::<i32>),
        }
    }
}

impl Connection for MssqlConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut client = self.client.lock().await;
            let result = client
                .simple_query(sql)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            let _ = result.into_results().await;
            Ok(0)
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut client = self.client.lock().await;
            let stream = client
                .simple_query(sql)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            let row_groups = stream
                .into_results()
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            let mut result = Vec::new();
            for group in row_groups {
                for row in group {
                    result.push(Self::row_to_map(&row));
                }
            }
            Ok(result)
        })
    }

    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut client = self.client.lock().await;
            let boxed: Vec<Box<dyn ToSql>> = params.iter().map(Self::value_to_tiberius).collect();
            let refs: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();
            let stream = client
                .query(sql, &refs)
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            let rows = stream
                .into_first_result()
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(rows.iter().map(Self::row_to_map).collect())
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut client = self.client.lock().await;
            client
                .simple_query("BEGIN TRANSACTION")
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut client = self.client.lock().await;
            client
                .simple_query("COMMIT")
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut client = self.client.lock().await;
            client
                .simple_query("ROLLBACK")
                .await
                .map_err(|e| DbError::QueryError(e.to_string()))?;
            Ok(())
        })
    }

    fn is_connected(&self) -> bool {
        true
    }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            self.client
                .lock()
                .await
                .simple_query("SELECT 1")
                .await
                .is_ok()
        })
    }
    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

async fn setup_mssql() -> MssqlConnection {
    let mut config = Config::new();
    config.host(MSSQL_HOST);
    config.port(MSSQL_PORT);
    config.authentication(AuthMethod::sql_server(MSSQL_USER, MSSQL_PASS));
    config.database(MSSQL_DB);
    config.trust_cert();
    let tcp = tokio::net::TcpStream::connect(config.get_addr())
        .await
        .expect("MSSQL 不可用: 127.0.0.1:1433");
    let client = Client::connect(config, tcp.compat_write())
        .await
        .expect("tiberius connect failed");
    let mut adapter = MssqlConnection::new(client);
    let builder = TestSchemaBuilder::new(TestDialect::MsSql);
    for ddl in builder.build_ddl() {
        adapter.execute(&ddl).await.unwrap();
    }
    for sql in builder.seed_data() {
        adapter.execute(&sql).await.unwrap();
    }
    adapter
}

async fn teardown_mssql(conn: &mut MssqlConnection) {
    let builder = TestSchemaBuilder::new(TestDialect::MsSql);
    for ddl in builder.teardown_ddl() {
        let _ = conn.execute(&ddl).await;
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
async fn test_hasone_equivalent_mssql() {
    let mut conn = setup_mssql().await;
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
    teardown_mssql(&mut conn).await;
}

#[tokio::test]
#[ignore]
async fn test_hasmany_equivalent_mssql() {
    let mut conn = setup_mssql().await;
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
    teardown_mssql(&mut conn).await;
}

#[tokio::test]
#[ignore]
async fn test_many_to_many_equivalent_mssql() {
    let mut conn = setup_mssql().await;
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
    teardown_mssql(&mut conn).await;
}

#[tokio::test]
#[ignore]
async fn test_join_strategy_mssql() {
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
async fn test_dataloader_strategy_mssql() {
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
async fn test_intermediate_strategy_mssql() {
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
async fn test_nested_depth_mssql() {
    let mut conn = setup_mssql().await;
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
    teardown_mssql(&mut conn).await;
}
