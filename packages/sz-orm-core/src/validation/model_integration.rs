//! # Model 集成（`validate-on-write` feature）
//!
//! 为 `QueryBuilder<M: Model + ModelExt + Validate>` 提供 `insert_validated`/`update_validated`，
//! 在写入前自动调用 `validate()`，校验失败拒绝写入。

use crate::model::ModelExt;
use crate::pool::Connection;
use crate::query::QueryBuilder;
use crate::validation::Validate;

/// 启用 validate-on-write feature 时，insert 前自动校验
impl<M: ModelExt + Validate> QueryBuilder<M> {
    /// 校验后 insert（validate-on-write feature）
    ///
    /// 先调用 `model.validate()`，失败返回 `DbError::Validation`；
    /// 成功后构建 INSERT SQL 并通过 `conn.execute()` 执行。
    pub async fn insert_validated(
        &self,
        model: &M,
        conn: &mut dyn Connection,
    ) -> Result<u64, crate::DbError> {
        model
            .validate()
            .map_err(|e| crate::DbError::Validation(e.to_string()))?;
        let data = model.to_value();
        let sql = self.build_insert(&data);
        if sql.is_empty() {
            return Err(crate::DbError::InvalidInput(
                "empty insert data after validation".to_string(),
            ));
        }
        conn.execute(&sql).await
    }

    /// 校验后 update（validate-on-write feature）
    ///
    /// 先调用 `model.validate()`，失败返回 `DbError::Validation`；
    /// 成功后构建 UPDATE SQL 并通过 `conn.execute()` 执行。
    pub async fn update_validated(
        &self,
        model: &M,
        conn: &mut dyn Connection,
    ) -> Result<u64, crate::DbError> {
        model
            .validate()
            .map_err(|e| crate::DbError::Validation(e.to_string()))?;
        let data = model.to_value();
        let sql = self.build_update(&data);
        if sql.is_empty() {
            return Err(crate::DbError::InvalidInput(
                "empty update data after validation".to_string(),
            ));
        }
        conn.execute(&sql).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::MySqlDialect;
    use crate::model::Model;
    use crate::query::QueryBuilder;
    use crate::validation::Validate as _;
    use crate::value::Value;
    use std::collections::HashMap;

    // 测试用 Model
    #[derive(Clone, Debug)]
    struct TestUser {
        id: i64,
        email: String,
        name: String,
    }

    impl Model for TestUser {
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

    impl ModelExt for TestUser {
        fn columns() -> Vec<&'static str> {
            vec!["id", "email", "name"]
        }
        fn fillable() -> Vec<&'static str> {
            vec!["email", "name"]
        }
        fn to_value(&self) -> HashMap<String, Value> {
            let mut map = HashMap::new();
            map.insert("id".to_string(), Value::I64(self.id));
            map.insert("email".to_string(), Value::String(self.email.clone()));
            map.insert("name".to_string(), Value::String(self.name.clone()));
            map
        }
    }

    // 为 TestUser 实现 Validate trait
    impl Validate for TestUser {
        fn validate(&self) -> Result<(), crate::validation::ValidationError> {
            let mut results = Vec::new();
            results.push(crate::validation::rules::validate_email(
                "email",
                &self.email,
            ));
            results.push(crate::validation::rules::validate_length(
                "name", &self.name, 2, 50,
            ));
            crate::validation::aggregate(results)
        }
    }

    // Mock 连接
    struct MockConnection {
        executed: std::sync::Mutex<Vec<String>>,
    }

    impl MockConnection {
        fn new() -> Self {
            Self {
                executed: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl Connection for MockConnection {
        fn execute<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<u64, crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async move {
                self.executed.lock().unwrap().push(sql.to_string());
                Ok(1)
            })
        }
        fn query<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<crate::pool::QueryRows, crate::DbError>>
                    + Send
                    + 'a,
            >,
        > {
            let _ = sql;
            Box::pin(async { Ok(crate::pool::QueryRows::new()) })
        }
        fn begin_transaction<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async { Ok(()) })
        }
        fn commit<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async { Ok(()) })
        }
        fn rollback<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async { Ok(()) })
        }
        fn is_connected(&self) -> bool {
            true
        }
        fn ping<'a>(
            &'a mut self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }
        fn close<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn test_insert_validated_pass() {
        let user = TestUser {
            id: 1,
            email: "test@example.com".to_string(),
            name: "Alice".to_string(),
        };
        let qb = QueryBuilder::<TestUser>::new(Box::new(MySqlDialect));
        let mut conn = MockConnection::new();
        let result = qb.insert_validated(&user, &mut conn).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_insert_validated_fail_email() {
        let user = TestUser {
            id: 1,
            email: "not-an-email".to_string(),
            name: "Alice".to_string(),
        };
        let qb = QueryBuilder::<TestUser>::new(Box::new(MySqlDialect));
        let mut conn = MockConnection::new();
        let result = qb.insert_validated(&user, &mut conn).await;
        assert!(matches!(result, Err(crate::DbError::Validation(_))));
        // 验证失败时不应执行 SQL
        assert!(conn.executed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_insert_validated_fail_length() {
        let user = TestUser {
            id: 1,
            email: "test@example.com".to_string(),
            name: "A".to_string(),
        };
        let qb = QueryBuilder::<TestUser>::new(Box::new(MySqlDialect));
        let mut conn = MockConnection::new();
        let result = qb.insert_validated(&user, &mut conn).await;
        assert!(matches!(result, Err(crate::DbError::Validation(_))));
        assert!(conn.executed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_update_validated_pass() {
        let user = TestUser {
            id: 1,
            email: "test@example.com".to_string(),
            name: "Alice".to_string(),
        };
        let qb = QueryBuilder::<TestUser>::new(Box::new(MySqlDialect));
        let mut conn = MockConnection::new();
        let result = qb.update_validated(&user, &mut conn).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_validated_fail() {
        let user = TestUser {
            id: 1,
            email: "bad".to_string(),
            name: "Alice".to_string(),
        };
        let qb = QueryBuilder::<TestUser>::new(Box::new(MySqlDialect));
        let mut conn = MockConnection::new();
        let result = qb.update_validated(&user, &mut conn).await;
        assert!(matches!(result, Err(crate::DbError::Validation(_))));
        assert!(conn.executed.lock().unwrap().is_empty());
    }
}
