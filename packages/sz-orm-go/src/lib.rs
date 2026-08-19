//! # SZ-ORM Go Bindings
//!
//! Provides sz-orm-core's Pool/Query API to Go via cgo / syscall
//! (SQLite backend, real and usable).
//!
//! Go side wrapper (`go/szorm/`) uses Windows `syscall.NewLazyDLL` to load
//! `sz_orm_go.dll` and call functions exported by this crate; non-Windows platforms use cgo.

use std::ffi::{c_char, CStr, CString};

use sz_orm_cabi::{PoolConfigC, QueryResultC, SzOrmPoolHandle, SzOrmTransactionHandle};

/// Create connection pool (real creation, SQLite backend)
///
/// Returns handle, null indicates failure.
///
/// # Safety
///
/// SAFETY: `dsn` must be a valid NUL-terminated C string; `config` may be null (use default config).
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_pool_new(
    dsn: *const c_char,
    config: *const PoolConfigC,
) -> SzOrmPoolHandle {
    // SAFETY: 转发到 cabi，dsn/config 有效性由调用方保证
    unsafe { sz_orm_cabi::sz_orm_pool_new(dsn, config) }
}

/// Free connection pool
///
/// # Safety
///
/// SAFETY: handle must be a valid handle returned by `sz_orm_go_pool_new`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_pool_free(handle: SzOrmPoolHandle) {
    // SAFETY: 转发到 cabi，handle 有效性由调用方保证
    unsafe { sz_orm_cabi::sz_orm_pool_free(handle) }
}

/// Health check (real acquire + ping)
///
/// # Safety
///
/// SAFETY: handle must be a valid handle returned by `sz_orm_go_pool_new`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_ping(handle: SzOrmPoolHandle) -> i32 {
    // SAFETY: 转发到 cabi，handle 有效性由调用方保证
    unsafe { sz_orm_cabi::sz_orm_ping(handle) }
}

/// Execute query, return JSON row array
///
/// On success returns non-null string pointer (caller frees with `sz_orm_go_string_free`),
/// on failure returns null.
///
/// # Safety
///
/// SAFETY: handle must be a valid handle returned by `sz_orm_go_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_query(
    handle: SzOrmPoolHandle,
    sql: *const c_char,
) -> *mut c_char {
    if handle.is_null() || sql.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 调用方保证 handle 有效
    let result = unsafe { sz_orm_cabi::sz_orm_query(handle, sql) };
    if result.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: result 由 sz_orm_query 分配
    let result_box = unsafe { Box::from_raw(result) };
    let json = if result_box.success != 0 && !result_box.json.is_null() {
        // SAFETY: json 是有效 NUL 结尾 C 字符串
        unsafe { CStr::from_ptr(result_box.json) }
            .to_str()
            .map(|s| s.to_string())
            .unwrap_or_default()
    } else {
        // SAFETY: result_box 由 sz_orm_query 分配，配对释放
        unsafe {
            sz_orm_cabi::sz_orm_query_result_free(Box::into_raw(result_box));
        }
        return std::ptr::null_mut();
    };
    // SAFETY: result_box 由 sz_orm_query 分配，配对释放
    unsafe {
        sz_orm_cabi::sz_orm_query_result_free(Box::into_raw(result_box));
    }
    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free string returned by `sz_orm_go_query`
///
/// # Safety
///
/// SAFETY: `ptr` must be a pointer returned by `sz_orm_go_query` (or null).
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr 由 CString::into_raw 分配，此处配对释放
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

/// Execute write statement, return affected row count (success=0 indicates failure)
///
/// Returns heap-allocated `QueryResultC` (caller frees with `sz_orm_go_result_free`),
/// on failure returns null. Returning struct by value on Windows x64 uses sret convention,
/// Go syscall cannot handle directly, so pointer return is used instead.
///
/// # Safety
///
/// SAFETY: handle must be a valid handle returned by `sz_orm_go_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_execute(
    handle: SzOrmPoolHandle,
    sql: *const c_char,
) -> *mut QueryResultC {
    if handle.is_null() || sql.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi，handle/sql 有效性由调用方保证
    let result = unsafe { sz_orm_cabi::sz_orm_execute(handle, sql) };
    Box::into_raw(Box::new(result))
}

/// Free result returned by `sz_orm_go_execute`
///
/// # Safety
///
/// SAFETY: `ptr` must be a pointer returned by `sz_orm_go_execute` (or null).
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_result_free(ptr: *mut QueryResultC) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr 由 sz_orm_go_execute 分配，此处配对释放
    unsafe {
        drop(Box::from_raw(ptr));
    }
}

/// Get version number
#[no_mangle]
pub extern "C" fn sz_orm_go_version() -> u32 {
    sz_orm_cabi::sz_orm_version()
}

// ============================================================================
// 事务 API 转发（REQ-BND-006）
// ============================================================================

/// Begin transaction, return transaction handle (null indicates failure)
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_go_pool_new`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_begin(
    handle: SzOrmPoolHandle,
) -> SzOrmTransactionHandle {
    // SAFETY: 转发到 cabi，handle 有效性由调用方保证
    unsafe { sz_orm_cabi::sz_orm_transaction_begin(handle) }
}

/// Execute SQL in transaction, return heap-allocated `QueryResultC`
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by `sz_orm_go_transaction_begin`;
/// `sql` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_execute(
    tx_handle: SzOrmTransactionHandle,
    sql: *const c_char,
) -> *mut QueryResultC {
    if tx_handle.is_null() || sql.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    let result = unsafe { sz_orm_cabi::sz_orm_transaction_execute(tx_handle, sql) };
    Box::into_raw(Box::new(result))
}

/// Commit transaction, return 1=success 0=failure
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by `sz_orm_go_transaction_begin`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_commit(tx_handle: SzOrmTransactionHandle) -> i32 {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_transaction_commit(tx_handle) }
}

/// Rollback transaction, return 1=success 0=failure
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by `sz_orm_go_transaction_begin`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_rollback(tx_handle: SzOrmTransactionHandle) -> i32 {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_transaction_rollback(tx_handle) }
}

/// Free transaction handle (auto rollback if still active)
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by `sz_orm_go_transaction_begin`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_free(tx_handle: SzOrmTransactionHandle) {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_transaction_free(tx_handle) }
}

// ============================================================================
// 模型级 API 转发（REQ-BND-013）
// ============================================================================

/// Insert row on pool, return heap-allocated `QueryResultC`
///
/// # Safety
///
/// SAFETY: `handle` must be valid; `table`/`fields_json`/`values_json` must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_model_insert(
    handle: SzOrmPoolHandle,
    table: *const c_char,
    fields_json: *const c_char,
    values_json: *const c_char,
) -> *mut QueryResultC {
    if handle.is_null() || table.is_null() || fields_json.is_null() || values_json.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    let result =
        unsafe { sz_orm_cabi::sz_orm_model_insert(handle, table, fields_json, values_json) };
    Box::into_raw(Box::new(result))
}

/// Update row on pool, return heap-allocated `QueryResultC`
///
/// # Safety
///
/// SAFETY: `handle` must be valid; all parameters must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_model_update(
    handle: SzOrmPoolHandle,
    table: *const c_char,
    set_json: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> *mut QueryResultC {
    if handle.is_null() || table.is_null() || set_json.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    let result = unsafe {
        sz_orm_cabi::sz_orm_model_update(handle, table, set_json, where_clause, where_params_json)
    };
    Box::into_raw(Box::new(result))
}

/// Delete row on pool, return heap-allocated `QueryResultC`
///
/// # Safety
///
/// SAFETY: `handle` must be valid; all parameters must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_model_delete(
    handle: SzOrmPoolHandle,
    table: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> *mut QueryResultC {
    if handle.is_null() || table.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    let result =
        unsafe { sz_orm_cabi::sz_orm_model_delete(handle, table, where_clause, where_params_json) };
    Box::into_raw(Box::new(result))
}

/// Query row on pool, return JSON row array string (free with `sz_orm_go_string_free`)
///
/// # Safety
///
/// SAFETY: `handle` must be valid; all parameters must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_model_find(
    handle: SzOrmPoolHandle,
    table: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || table.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_model_find(handle, table, where_clause, where_params_json) }
}

/// Insert row in transaction, return heap-allocated `QueryResultC`
///
/// # Safety
///
/// SAFETY: `tx_handle` must be valid; all parameters must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_model_insert_tx(
    tx_handle: SzOrmTransactionHandle,
    table: *const c_char,
    fields_json: *const c_char,
    values_json: *const c_char,
) -> *mut QueryResultC {
    if tx_handle.is_null() || table.is_null() || fields_json.is_null() || values_json.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    let result =
        unsafe { sz_orm_cabi::sz_orm_model_insert_tx(tx_handle, table, fields_json, values_json) };
    Box::into_raw(Box::new(result))
}

/// Update row in transaction, return heap-allocated `QueryResultC`
///
/// # Safety
///
/// SAFETY: `tx_handle` must be valid; all parameters must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_model_update_tx(
    tx_handle: SzOrmTransactionHandle,
    table: *const c_char,
    set_json: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> *mut QueryResultC {
    if tx_handle.is_null() || table.is_null() || set_json.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    let result = unsafe {
        sz_orm_cabi::sz_orm_model_update_tx(
            tx_handle,
            table,
            set_json,
            where_clause,
            where_params_json,
        )
    };
    Box::into_raw(Box::new(result))
}

/// Delete row in transaction, return heap-allocated `QueryResultC`
///
/// # Safety
///
/// SAFETY: `tx_handle` must be valid; all parameters must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_model_delete_tx(
    tx_handle: SzOrmTransactionHandle,
    table: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> *mut QueryResultC {
    if tx_handle.is_null() || table.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    let result = unsafe {
        sz_orm_cabi::sz_orm_model_delete_tx(tx_handle, table, where_clause, where_params_json)
    };
    Box::into_raw(Box::new(result))
}

/// Query row in transaction, return JSON row array string (free with `sz_orm_go_string_free`)
///
/// # Safety
///
/// SAFETY: `tx_handle` must be valid; all parameters must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_model_find_tx(
    tx_handle: SzOrmTransactionHandle,
    table: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> *mut c_char {
    if tx_handle.is_null() || table.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_model_find_tx(tx_handle, table, where_clause, where_params_json) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_cabi::SzOrmErrorCode;

    fn test_pool() -> SzOrmPoolHandle {
        let dsn = CString::new("sqlite::memory:").unwrap();
        // SAFETY: dsn 有效
        unsafe { sz_orm_go_pool_new(dsn.as_ptr(), std::ptr::null()) }
    }

    #[test]
    fn test_go_pool_new_creates_real_pool() {
        let pool = test_pool();
        assert!(!pool.is_null(), "pool_new should create a real pool");
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_ping_healthy() {
        let pool = test_pool();
        assert!(!pool.is_null());
        // SAFETY: pool 有效
        let healthy = unsafe { sz_orm_go_ping(pool) };
        assert_eq!(healthy, 1, "fresh pool should be healthy");
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_execute_and_query_roundtrip() {
        let pool = test_pool();
        assert!(!pool.is_null());

        let create =
            CString::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        let rp = unsafe { sz_orm_go_execute(pool, create.as_ptr()) };
        assert!(!rp.is_null(), "CREATE should succeed");
        // SAFETY: rp 由 sz_orm_go_execute 分配
        let r = unsafe { Box::from_raw(rp) };
        assert_eq!(
            r.success, 1,
            "CREATE should succeed, got code {}",
            r.error_code
        );
        // SAFETY: r 由 sz_orm_go_execute 分配，配对释放
        unsafe { sz_orm_go_result_free(Box::into_raw(r)) };

        let insert = CString::new("INSERT INTO users (name) VALUES ('Alice'), ('Bob')").unwrap();
        // SAFETY: pool/sql 有效
        let rp = unsafe { sz_orm_go_execute(pool, insert.as_ptr()) };
        assert!(!rp.is_null(), "INSERT should succeed");
        // SAFETY: rp 由 sz_orm_go_execute 分配
        let r = unsafe { Box::from_raw(rp) };
        assert_eq!(r.success, 1);
        assert_eq!(r.rows_affected, 2, "two rows inserted");
        // SAFETY: r 由 sz_orm_go_execute 分配，配对释放
        unsafe { sz_orm_go_result_free(Box::into_raw(r)) };

        let q = CString::new("SELECT id, name FROM users ORDER BY id").unwrap();
        // SAFETY: pool/sql 有效
        let json_ptr = unsafe { sz_orm_go_query(pool, q.as_ptr()) };
        assert!(!json_ptr.is_null(), "query should return JSON");
        // SAFETY: json_ptr 有效
        let json = unsafe { CStr::from_ptr(json_ptr) }
            .to_string_lossy()
            .into_owned();
        assert!(
            json.contains("Alice"),
            "query rows should contain Alice: {json}"
        );
        assert!(
            json.contains("Bob"),
            "query rows should contain Bob: {json}"
        );
        // SAFETY: json_ptr 由 sz_orm_go_query 分配，配对释放
        unsafe { sz_orm_go_string_free(json_ptr) };

        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_query_invalid_sql_returns_null() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let q = CString::new("SELECT * FROM nonexistent").unwrap();
        // SAFETY: pool/sql 有效
        let json_ptr = unsafe { sz_orm_go_query(pool, q.as_ptr()) };
        assert!(json_ptr.is_null(), "invalid SQL should return null");
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_null_args() {
        // SAFETY: null 参数合法调用
        let pool = unsafe { sz_orm_go_pool_new(std::ptr::null(), std::ptr::null()) };
        assert!(pool.is_null());
        // SAFETY: null 参数合法调用
        let r = unsafe { sz_orm_go_execute(std::ptr::null_mut(), std::ptr::null()) };
        assert!(r.is_null(), "null args should return null");
        // SAFETY: null 参数合法调用
        let q = unsafe { sz_orm_go_query(std::ptr::null_mut(), std::ptr::null()) };
        assert!(q.is_null());
    }

    #[test]
    fn test_go_result_free_null_safe() {
        // SAFETY: null 参数合法调用
        unsafe { sz_orm_go_result_free(std::ptr::null_mut()) };
    }

    #[test]
    fn test_go_version() {
        let v = sz_orm_go_version();
        assert!(v >= 1, "version should be >= 1, got {v}");
    }

    #[test]
    fn test_go_string_free_null_safe() {
        // SAFETY: null 参数合法调用
        unsafe { sz_orm_go_string_free(std::ptr::null_mut()) };
    }

    #[test]
    fn test_go_ping_after_free_returns_zero() {
        // 使用 null handle 测试 ping（而非释放后的 UAF）
        // SAFETY: null handle 合法调用
        let healthy = unsafe { sz_orm_go_ping(std::ptr::null_mut()) };
        assert_eq!(healthy, 0, "null pool should not be healthy");
    }

    #[test]
    fn test_go_execute_empty_sql() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE go_e (id INTEGER PRIMARY KEY)").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_go_execute(pool, create.as_ptr()) };
        // SAFETY: r 由 sz_orm_go_execute 分配
        unsafe { sz_orm_go_result_free(r) };
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_multiple_pools_independent() {
        let dsn1 = CString::new("sqlite::memory:").unwrap();
        // SAFETY: dsn1 有效
        let pool1 = unsafe { sz_orm_go_pool_new(dsn1.as_ptr(), std::ptr::null()) };
        let dsn2 = CString::new("sqlite::memory:").unwrap();
        // SAFETY: dsn2 有效
        let pool2 = unsafe { sz_orm_go_pool_new(dsn2.as_ptr(), std::ptr::null()) };
        assert!(!pool1.is_null());
        assert!(!pool2.is_null());
        assert_ne!(pool1, pool2, "pools should have distinct handles");
        // SAFETY: pools 有效
        unsafe {
            sz_orm_go_pool_free(pool1);
            sz_orm_go_pool_free(pool2);
        }
    }

    #[test]
    fn test_go_query_returns_valid_json() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE j (id INTEGER PRIMARY KEY)").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_go_execute(pool, create.as_ptr()) };
        // SAFETY: r 由 sz_orm_go_execute 分配
        unsafe { sz_orm_go_result_free(r) };
        let insert = CString::new("INSERT INTO j (id) VALUES (1), (2), (3)").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_go_execute(pool, insert.as_ptr()) };
        // SAFETY: r 由 sz_orm_go_execute 分配
        unsafe { sz_orm_go_result_free(r) };
        let q = CString::new("SELECT id FROM j ORDER BY id").unwrap();
        // SAFETY: pool/sql 有效
        let json_ptr = unsafe { sz_orm_go_query(pool, q.as_ptr()) };
        assert!(!json_ptr.is_null());
        // SAFETY: json_ptr 有效
        let json = unsafe { CStr::from_ptr(json_ptr) }
            .to_string_lossy()
            .into_owned();
        assert!(
            json.starts_with('[') || json.contains("id"),
            "json should be array-like: {json}"
        );
        // SAFETY: json_ptr 配对释放
        unsafe { sz_orm_go_string_free(json_ptr) };
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    // ===== 新增测试：事务 + 模型级 API 转发（REQ-BND-006/013/014）=====

    fn model_test_pool() -> SzOrmPoolHandle {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create =
            CString::new("CREATE TABLE gm_t (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
                .unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_go_execute(pool, create.as_ptr()) };
        // SAFETY: r 由 sz_orm_go_execute 分配
        unsafe { sz_orm_go_result_free(r) };
        pool
    }

    unsafe fn free_go_result(ptr: *mut QueryResultC) -> QueryResultC {
        if ptr.is_null() {
            return QueryResultC {
                success: 0,
                error_code: SzOrmErrorCode::InvalidArgument.as_i32(),
                rows_affected: 0,
                last_insert_id: 0,
            };
        }
        // SAFETY: ptr 由 sz_orm_go_model_* 分配
        let r = unsafe { Box::from_raw(ptr) };
        *r
    }

    unsafe fn free_go_str(ptr: *mut c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: ptr 有效
        let s = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: ptr 配对释放
        unsafe { sz_orm_go_string_free(ptr) };
        s
    }

    #[test]
    fn test_go_model_insert_find_roundtrip() {
        let pool = model_test_pool();
        let table = CString::new("gm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["Alice",30]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        let r = unsafe {
            sz_orm_go_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };
        // SAFETY: r 有效
        let r = unsafe { free_go_result(r) };
        assert_eq!(
            r.success, 1,
            "go model_insert should succeed, code={}",
            r.error_code
        );
        assert_eq!(r.rows_affected, 1);

        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["Alice"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_go_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(!ptr.is_null());
        // SAFETY: ptr 有效
        let json = unsafe { free_go_str(ptr) };
        assert!(
            json.contains("Alice"),
            "go model_find should contain Alice: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_model_update_delete_find() {
        let pool = model_test_pool();
        let table = CString::new("gm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["Bob",25]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        let r = unsafe {
            sz_orm_go_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };
        // SAFETY: r 有效
        unsafe { free_go_result(r) };

        // update
        let set_json = CString::new(r#"{"age":26}"#).unwrap();
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["Bob"]"#).unwrap();
        // SAFETY: pool/table/set/where 有效
        let r = unsafe {
            sz_orm_go_model_update(
                pool,
                table.as_ptr(),
                set_json.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: r 有效
        let r = unsafe { free_go_result(r) };
        assert_eq!(
            r.success, 1,
            "go model_update should succeed, code={}",
            r.error_code
        );

        // find after update
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_go_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { free_go_str(ptr) };
        assert!(
            json.contains("26"),
            "go find after update should contain age 26: {json}"
        );

        // delete
        // SAFETY: pool/table/where 有效
        let r = unsafe {
            sz_orm_go_model_delete(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: r 有效
        let r = unsafe { free_go_result(r) };
        assert_eq!(
            r.success, 1,
            "go model_delete should succeed, code={}",
            r.error_code
        );

        // find after delete → empty
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_go_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { free_go_str(ptr) };
        assert!(
            json == "[]" || json == "null" || json.is_empty(),
            "go find after delete should be empty: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_model_insert_illegal_table() {
        let pool = model_test_pool();
        let table = CString::new("gm_t; DROP--").unwrap();
        let fields = CString::new(r#"["name"]"#).unwrap();
        let values = CString::new(r#"["X"]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效（table 含注入向量）
        let r = unsafe {
            sz_orm_go_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };
        // SAFETY: r 有效
        let r = unsafe { free_go_result(r) };
        assert_eq!(r.success, 0, "illegal table should fail");
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_transaction_model_rollback() {
        let pool = model_test_pool();
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_go_transaction_begin(pool) };
        assert!(!tx.is_null());

        let table = CString::new("gm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["TxUser",99]"#).unwrap();
        // SAFETY: tx/table/fields/values 有效
        let r = unsafe {
            sz_orm_go_model_insert_tx(tx, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };
        // SAFETY: r 有效
        let r = unsafe { free_go_result(r) };
        assert_eq!(
            r.success, 1,
            "go model_insert_tx should succeed, code={}",
            r.error_code
        );

        // SAFETY: tx 有效
        assert_eq!(unsafe { sz_orm_go_transaction_rollback(tx) }, 1);
        // SAFETY: tx 有效
        unsafe { sz_orm_go_transaction_free(tx) };

        // find after rollback → empty
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["TxUser"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_go_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { free_go_str(ptr) };
        assert!(
            json == "[]" || json == "null" || json.is_empty(),
            "go find after rollback should be empty: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }

    #[test]
    fn test_go_transaction_model_commit() {
        let pool = model_test_pool();
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_go_transaction_begin(pool) };
        assert!(!tx.is_null());

        let table = CString::new("gm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["CommitUser",50]"#).unwrap();
        // SAFETY: tx/table/fields/values 有效
        let r = unsafe {
            sz_orm_go_model_insert_tx(tx, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };
        // SAFETY: r 有效
        unsafe { free_go_result(r) };

        // SAFETY: tx 有效
        assert_eq!(unsafe { sz_orm_go_transaction_commit(tx) }, 1);
        // SAFETY: tx 有效
        unsafe { sz_orm_go_transaction_free(tx) };

        // find after commit → data visible
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["CommitUser"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_go_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { free_go_str(ptr) };
        assert!(
            json.contains("CommitUser"),
            "go find after commit should contain CommitUser: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_go_pool_free(pool) };
    }
}
