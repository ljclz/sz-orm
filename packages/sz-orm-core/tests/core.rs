//! Integration tests for sz-orm-core

#[cfg(test)]
mod tests {
    use sz_orm_core::*;

    #[test]
    fn test_module_exports() {
        // Test that all key types are exported
        let _db_type = DbType::MySQL;
        let _value = Value::Null;
        let _err = DbError::query("test");
        let _pool_err = PoolError::Timeout;
        let _cache_err = CacheError::NotFound("key".to_string());
        let _tx_err = TxError::NotStarted;

        // Test constants
        assert_eq!(DEFAULT_BATCH_SIZE, 1000);
        assert_eq!(DEFAULT_ACQUIRE_TIMEOUT, 30);
    }

    #[test]
    fn test_db_types_complete() {
        // Test all database types
        let types = [
            DbType::MySQL,
            DbType::PostgreSQL,
            DbType::Sqlite,
            DbType::Redis,
            DbType::MongoDB,
            DbType::ClickHouse,
            DbType::Oracle,
            DbType::OceanBase,
            DbType::SqlServer,
            DbType::VectorDb,
            DbType::PureJsDb,
        ];

        for db_type in types {
            // Each type should have a string representation
            let s = db_type.as_str();
            assert!(!s.is_empty());

            // Should be able to parse back from string
            let parsed = DbType::from_str(s);
            assert_eq!(parsed, Some(db_type), "Failed to parse {}", s);
        }
    }

    #[test]
    fn test_value_operations() {
        // Test value conversion methods
        let val = Value::I64(42);

        // Test as_i64
        assert_eq!(val.as_i64(), Some(42));

        // Test as_f64
        assert_eq!(val.as_f64(), Some(42.0));

        // Test is_* methods
        assert!(!val.is_null());
        assert!(val.is_i64());
        assert!(!val.is_string());

        // Test to_param
        assert_eq!(val.to_param(), "42");

        // Test Display
        assert_eq!(format!("{}", val), "42");
    }

    #[test]
    fn test_value_string() {
        let val = Value::String("hello world".to_string());

        assert_eq!(val.as_str(), Some("hello world"));
        assert_eq!(val.to_param(), "'hello world'");

        // Test escaping
        let val = Value::String("it's a test".to_string());
        assert_eq!(val.to_param(), "'it''s a test'");
    }

    #[test]
    fn test_value_array() {
        let arr = Value::Array(vec![Value::I64(1), Value::I64(2), Value::I64(3)]);

        assert_eq!(arr.to_param(), "(1, 2, 3)");
    }

    #[test]
    fn test_error_hierarchy() {
        // Test DbError hierarchy
        let err = DbError::query("test query");
        assert_eq!(err.error_code(), "DB001");
        assert!(!err.is_retryable());

        let err = DbError::connection("localhost");
        assert_eq!(err.error_code(), "DB002");
        assert!(err.is_retryable());

        let err = DbError::PoolError(PoolError::Timeout);
        assert_eq!(err.error_code(), "PL002");
        assert!(err.is_retryable());
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", DbError::query("invalid SQL")),
            "Query error: invalid SQL"
        );
        assert_eq!(
            format!("{}", DbError::not_found("user 123")),
            "Not found: user 123"
        );
        assert_eq!(
            format!("{}", PoolError::Timeout),
            "Connection acquire timeout"
        );
        assert_eq!(
            format!("{}", CacheError::NotFound("cache_key".to_string())),
            "Cache key not found: cache_key"
        );
    }

    #[test]
    fn test_from_impls() {
        // Test Into<Value> implementations
        let v: Value = ().into();
        assert!(v.is_null());

        let v: Value = true.into();
        assert!(v.is_bool());
        assert_eq!(v.as_bool(), Some(true));

        let v: Value = 42i32.into();
        assert_eq!(v.as_i64(), Some(42));

        let v: Value = 2.5f64.into();
        assert_eq!(v.as_f64(), Some(2.5));

        let v: Value = "hello".into();
        assert_eq!(v.as_str(), Some("hello"));

        let arr: Vec<Value> = vec![Value::I64(1), Value::I64(2), Value::I64(3)];
        let v: Value = arr.into();
        if let Value::Array(arr) = v {
            assert_eq!(arr.len(), 3);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_value_defaults() {
        let v: Value = Default::default();
        assert!(v.is_null());
    }

    #[test]
    fn test_constants() {
        // Test constant values
        assert_eq!(DEFAULT_BATCH_SIZE, 1000);
        assert_eq!(DEFAULT_ACQUIRE_TIMEOUT, 30);
        assert_eq!(DEFAULT_IDLE_TIMEOUT, 600);
        assert_eq!(DEFAULT_MAX_LIFETIME, 1800);
        assert_eq!(DEFAULT_MIN_IDLE, 5);
        assert_eq!(DEFAULT_MAX_SIZE, 100);
    }

    #[test]
    fn test_db_type_capabilities() {
        // Test database capabilities
        assert!(DbType::MySQL.supports_schema());
        assert!(!DbType::Redis.supports_schema());

        assert!(DbType::MySQL.supports_transaction());
        assert!(!DbType::Redis.supports_transaction());

        assert!(DbType::MySQL.supports_foreign_key());
        assert!(!DbType::Sqlite.supports_foreign_key());

        assert!(DbType::MySQL.supports_stored_procedure());
        assert!(!DbType::Sqlite.supports_stored_procedure());

        assert_eq!(DbType::MySQL.default_port(), 3306);
        assert_eq!(DbType::PostgreSQL.default_port(), 5432);
        assert_eq!(DbType::Sqlite.default_port(), 0);
    }

    #[test]
    fn test_bytes_value() {
        let bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let v: Value = bytes.into();

        assert!(v.is_bytes());
        assert_eq!(v.as_bytes(), Some(&[0xDE, 0xAD, 0xBE, 0xEF][..]));
        assert_eq!(v.to_param(), "X'deadbeef'");
    }

    #[tokio::test]
    async fn test_async_export() {
        // Test async trait export works
        fn _check_sync<T: Send + Sync>() {}

        // The module should compile with async support
        struct TestStruct;
        #[allow(dead_code)]
        #[async_trait]
        trait TestTrait: Send + Sync {
            async fn do_something(&self);
        }

        #[allow(dead_code)]
        #[async_trait]
        impl TestTrait for TestStruct {
            async fn do_something(&self) {
                // Do nothing
            }
        }

        _check_sync::<TestStruct>();
    }
}
