use sz_orm_core::{CacheError, DbError, PoolError, TxError};

#[test]
fn test_db_error_query() {
    let e = DbError::query("test error");
    assert_eq!(e.error_code(), "DB001");
    assert!(!e.is_retryable());
}

#[test]
fn test_db_error_connection() {
    let e = DbError::connection("conn failed");
    assert_eq!(e.error_code(), "DB002");
    assert!(e.is_retryable());
}

#[test]
fn test_db_error_not_found() {
    let e = DbError::not_found("user");
    assert_eq!(e.error_code(), "DB012");
    assert!(!e.is_retryable());
}

#[test]
fn test_db_error_connection_timeout() {
    let e = DbError::ConnectionTimeout("timeout".to_string());
    assert_eq!(e.error_code(), "DB004");
    assert!(e.is_retryable());
}

#[test]
fn test_db_error_connection_refused() {
    let e = DbError::ConnectionRefused("refused".to_string());
    assert_eq!(e.error_code(), "DB003");
    assert!(!e.is_retryable());
}

#[test]
fn test_db_error_pool_timeout() {
    let e = DbError::PoolError(PoolError::Timeout);
    assert!(e.is_retryable());
}

#[test]
fn test_db_error_unsupported() {
    let e = DbError::Unsupported("unsupported".to_string());
    assert_eq!(e.error_code(), "DB009");
}

#[test]
fn test_db_error_config() {
    let e = DbError::ConfigError("bad config".to_string());
    assert_eq!(e.error_code(), "DB010");
}

#[test]
fn test_db_error_not_found_variant() {
    let e = DbError::NotFound("not found".to_string());
    assert_eq!(e.error_code(), "DB012");
}

#[test]
fn test_db_error_already_exists() {
    let e = DbError::AlreadyExists("exists".to_string());
    assert_eq!(e.error_code(), "DB013");
}

#[test]
fn test_db_error_constraint_violation() {
    let e = DbError::ConstraintViolation("violation".to_string());
    assert_eq!(e.error_code(), "DB014");
}

#[test]
fn test_db_error_unique_violation() {
    let e = DbError::UniqueViolation("unique".to_string());
    assert_eq!(e.error_code(), "DB022");
}

#[test]
fn test_db_error_foreign_key_violation() {
    let e = DbError::ForeignKeyViolation("fk".to_string());
    assert_eq!(e.error_code(), "DB023");
}

#[test]
fn test_db_error_null_value() {
    let e = DbError::NullValue("null".to_string());
    assert_eq!(e.error_code(), "DB015");
}

#[test]
fn test_db_error_invalid_input() {
    let e = DbError::InvalidInput("invalid".to_string());
    assert_eq!(e.error_code(), "DB016");
}

#[test]
fn test_db_error_internal() {
    let e = DbError::Internal("internal".to_string());
    assert_eq!(e.error_code(), "DB017");
}

#[test]
fn test_db_error_tenant_error() {
    let e = DbError::TenantError("tenant".to_string());
    assert_eq!(e.error_code(), "DB020");
}

#[test]
fn test_db_error_validation() {
    let e = DbError::Validation("validation".to_string());
    assert_eq!(e.error_code(), "DB021");
}

#[test]
fn test_db_error_with_context() {
    let e = DbError::query("test").with_context("fetching user");
    assert!(e.context().is_some());
    assert_eq!(e.root_cause().error_code(), "DB001");
}

#[test]
fn test_db_error_with_context_chain() {
    let e = DbError::query("test")
        .with_context("fetching user")
        .with_context("handling request");
    let chain = e.format_context_chain();
    assert!(!chain.is_empty());
}

#[test]
fn test_db_error_with_context_in_span() {
    let e = DbError::query("test").with_context_in_span("fetching user", "user_service");
    assert!(e.context().is_some());
}

#[test]
fn test_db_error_root_cause() {
    let e = DbError::query("test").with_context("ctx1").with_context("ctx2");
    let root = e.root_cause();
    assert_eq!(root.error_code(), "DB001");
}

#[test]
fn test_db_error_no_context() {
    let e = DbError::query("test");
    assert!(e.context().is_none());
    assert!(e.format_context_chain().is_empty());
}

#[test]
fn test_db_error_display() {
    let e = DbError::query("test error");
    let s = format!("{}", e);
    assert!(s.contains("test error"));
}

#[test]
fn test_pool_error_display() {
    let e = PoolError::Timeout;
    let s = format!("{}", e);
    assert!(!s.is_empty());
}

#[test]
fn test_cache_error_display() {
    let e = CacheError::NotFound("key".to_string());
    let s = format!("{}", e);
    assert!(!s.is_empty());
}

#[test]
fn test_db_error_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::Other, "io error");
    let db_err: DbError = io_err.into();
    assert_eq!(db_err.error_code(), "DB018");
}

#[test]
fn test_db_error_from_serde_json() {
    let serde_err = serde_json::from_str::<i32>("invalid").unwrap_err();
    let db_err: DbError = serde_err.into();
    assert_eq!(db_err.error_code(), "DB011");
}

#[test]
fn test_db_error_contextual_is_retryable() {
    let e = DbError::connection("fail").with_context("ctx");
    assert!(e.is_retryable());
}

#[test]
fn test_db_error_contextual_not_retryable() {
    let e = DbError::query("fail").with_context("ctx");
    assert!(!e.is_retryable());
}

#[test]
fn test_tx_error_display() {
    let e = TxError::NotStarted;
    let s = format!("{}", e);
    assert!(!s.is_empty());
}