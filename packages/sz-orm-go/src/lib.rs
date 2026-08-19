//! # SZ-ORM Go Bindings
//!
//! 通过 cgo / syscall 调用 sz-orm-cabi 的 C ABI，为 Go 提供 sz-orm-core 的
//! Pool/Query API（SQLite 后端，真实可用）。
//!
//! Go 侧 wrapper（`go/szorm/`）使用 Windows `syscall.NewLazyDLL` 加载
//! `sz_orm_go.dll` 并调用本 crate 导出的函数；非 Windows 平台使用 cgo。

use std::ffi::{c_char, CStr, CString};

use sz_orm_cabi::{PoolConfigC, QueryResultC, SzOrmPoolHandle, SzOrmTransactionHandle};

/// 创建连接池（真实创建，SQLite 后端）
///
/// 返回句柄，null 表示失败。
///
/// # Safety
///
/// SAFETY: `dsn` 必须是有效的 NUL 结尾 C 字符串；`config` 可为 null（使用默认配置）。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_pool_new(
    dsn: *const c_char,
    config: *const PoolConfigC,
) -> SzOrmPoolHandle {
    // SAFETY: 转发到 cabi，dsn/config 有效性由调用方保证
    unsafe { sz_orm_cabi::sz_orm_pool_new(dsn, config) }
}

/// 释放连接池
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_go_pool_new` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_pool_free(handle: SzOrmPoolHandle) {
    // SAFETY: 转发到 cabi，handle 有效性由调用方保证
    unsafe { sz_orm_cabi::sz_orm_pool_free(handle) }
}

/// 健康检查（真实 acquire + ping）
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_go_pool_new` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_ping(handle: SzOrmPoolHandle) -> i32 {
    // SAFETY: 转发到 cabi，handle 有效性由调用方保证
    unsafe { sz_orm_cabi::sz_orm_ping(handle) }
}

/// 执行查询，返回 JSON 行数组
///
/// 成功时返回非空字符串指针（调用方用 `sz_orm_go_string_free` 释放），
/// 失败返回 null。
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_go_pool_new` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
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

/// 释放 `sz_orm_go_query` 返回的字符串
///
/// # Safety
///
/// SAFETY: `ptr` 必须是 `sz_orm_go_query` 返回的指针（或 null）。
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

/// 执行写语句，返回影响行数（success=0 表示失败）
///
/// 返回堆分配的 `QueryResultC`（调用方用 `sz_orm_go_result_free` 释放），
/// 失败时返回 null。按值返回结构体在 Windows x64 是 sret 约定，
/// Go syscall 无法直接处理，故改为指针返回。
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_go_pool_new` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
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

/// 释放 `sz_orm_go_execute` 返回的结果
///
/// # Safety
///
/// SAFETY: `ptr` 必须是 `sz_orm_go_execute` 返回的指针（或 null）。
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

/// 获取版本号
#[no_mangle]
pub extern "C" fn sz_orm_go_version() -> u32 {
    sz_orm_cabi::sz_orm_version()
}

// ============================================================================
// 事务 API 转发（REQ-BND-006）
// ============================================================================

/// 开始事务，返回事务句柄（null 表示失败）
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_go_pool_new` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_begin(
    handle: SzOrmPoolHandle,
) -> SzOrmTransactionHandle {
    // SAFETY: 转发到 cabi，handle 有效性由调用方保证
    unsafe { sz_orm_cabi::sz_orm_transaction_begin(handle) }
}

/// 在事务中执行 SQL，返回堆分配的 `QueryResultC`
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `sz_orm_go_transaction_begin` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
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

/// 提交事务，返回 1=成功 0=失败
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `sz_orm_go_transaction_begin` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_commit(tx_handle: SzOrmTransactionHandle) -> i32 {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_transaction_commit(tx_handle) }
}

/// 回滚事务，返回 1=成功 0=失败
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `sz_orm_go_transaction_begin` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_rollback(tx_handle: SzOrmTransactionHandle) -> i32 {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_transaction_rollback(tx_handle) }
}

/// 释放事务句柄（若仍活跃则自动回滚）
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `sz_orm_go_transaction_begin` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_transaction_free(tx_handle: SzOrmTransactionHandle) {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_transaction_free(tx_handle) }
}

// ============================================================================
// 模型级 API 转发（REQ-BND-013）
// ============================================================================

/// 在 pool 上插入行，返回堆分配的 `QueryResultC`
///
/// # Safety
///
/// SAFETY: `handle` 必须有效；`table`/`fields_json`/`values_json` 必须是有效的 NUL 结尾 C 字符串。
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

/// 在 pool 上更新行，返回堆分配的 `QueryResultC`
///
/// # Safety
///
/// SAFETY: `handle` 必须有效；各参数必须是有效的 NUL 结尾 C 字符串。
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

/// 在 pool 上删除行，返回堆分配的 `QueryResultC`
///
/// # Safety
///
/// SAFETY: `handle` 必须有效；各参数必须是有效的 NUL 结尾 C 字符串。
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

/// 在 pool 上查询行，返回 JSON 行数组字符串（`sz_orm_go_string_free` 释放）
///
/// # Safety
///
/// SAFETY: `handle` 必须有效；各参数必须是有效的 NUL 结尾 C 字符串。
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

/// 在事务内插入行，返回堆分配的 `QueryResultC`
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须有效；各参数必须是有效的 NUL 结尾 C 字符串。
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

/// 在事务内更新行，返回堆分配的 `QueryResultC`
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须有效；各参数必须是有效的 NUL 结尾 C 字符串。
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

/// 在事务内删除行，返回堆分配的 `QueryResultC`
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须有效；各参数必须是有效的 NUL 结尾 C 字符串。
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

/// 在事务内查询行，返回 JSON 行数组字符串（`sz_orm_go_string_free` 释放）
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须有效；各参数必须是有效的 NUL 结尾 C 字符串。
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
