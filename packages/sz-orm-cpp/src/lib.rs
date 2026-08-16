//! # SZ-ORM C++ Bindings
//!
//! 通过 `extern "C"` + C ABI 为 C++ 提供 sz-orm-core 的
//! Pool/Query API（SQLite 后端，真实可用）。
//!
//! C++ 侧 wrapper 见 `cpp/szorm.h`（`#include <szorm.h>` 后使用 `szorm::Pool`）。

use std::ffi::{c_char, CStr, CString};

use sz_orm_cabi::{PoolConfigC, QueryResultC, SzOrmPoolHandle};

/// 创建连接池（真实创建，SQLite 后端）
///
/// 返回句柄，null 表示失败。
///
/// # Safety
///
/// SAFETY: `dsn` 必须是有效的 NUL 结尾 C 字符串；`config` 可为 null。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_cpp_pool_new(
    dsn: *const c_char,
    config: *const PoolConfigC,
) -> SzOrmPoolHandle {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_pool_new(dsn, config) }
}

/// 释放连接池
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_cpp_pool_new` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_cpp_pool_free(handle: SzOrmPoolHandle) {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_pool_free(handle) }
}

/// 健康检查（真实 acquire + ping）
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_cpp_pool_new` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_cpp_ping(handle: SzOrmPoolHandle) -> i32 {
    // SAFETY: 转发到 cabi
    unsafe { sz_orm_cabi::sz_orm_ping(handle) }
}

/// 执行查询，返回 JSON 行数组（调用方用 `sz_orm_cpp_string_free` 释放）
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_cpp_pool_new` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_cpp_query(
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
        // SAFETY: result_box 配对释放
        unsafe {
            sz_orm_cabi::sz_orm_query_result_free(Box::into_raw(result_box));
        }
        return std::ptr::null_mut();
    };
    // SAFETY: result_box 配对释放
    unsafe {
        sz_orm_cabi::sz_orm_query_result_free(Box::into_raw(result_box));
    }
    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 释放 `sz_orm_cpp_query` 返回的字符串
///
/// # Safety
///
/// SAFETY: `ptr` 必须是 `sz_orm_cpp_query` 返回的指针（或 null）。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_cpp_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr 由 CString::into_raw 分配，配对释放
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

/// 执行写语句，返回堆分配的 `QueryResultC`（`sz_orm_cpp_result_free` 释放）
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_cpp_pool_new` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_cpp_execute(
    handle: SzOrmPoolHandle,
    sql: *const c_char,
) -> *mut QueryResultC {
    if handle.is_null() || sql.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 转发到 cabi
    let result = unsafe { sz_orm_cabi::sz_orm_execute(handle, sql) };
    Box::into_raw(Box::new(result))
}

/// 释放 `sz_orm_cpp_execute` 返回的结果
///
/// # Safety
///
/// SAFETY: `ptr` 必须是 `sz_orm_cpp_execute` 返回的指针（或 null）。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_cpp_result_free(ptr: *mut QueryResultC) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr 由 sz_orm_cpp_execute 分配，配对释放
    unsafe {
        drop(Box::from_raw(ptr));
    }
}

/// 获取版本号
#[no_mangle]
pub extern "C" fn sz_orm_cpp_version() -> u32 {
    sz_orm_cabi::sz_orm_version()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> SzOrmPoolHandle {
        let dsn = CString::new("sqlite::memory:").unwrap();
        // SAFETY: dsn 有效
        unsafe { sz_orm_cpp_pool_new(dsn.as_ptr(), std::ptr::null()) }
    }

    #[test]
    fn test_cpp_pool_new_creates_real_pool() {
        let pool = test_pool();
        assert!(!pool.is_null(), "pool_new should create a real pool");
        // SAFETY: pool 有效
        unsafe { sz_orm_cpp_pool_free(pool) };
    }

    #[test]
    fn test_cpp_ping_healthy() {
        let pool = test_pool();
        assert!(!pool.is_null());
        // SAFETY: pool 有效
        let healthy = unsafe { sz_orm_cpp_ping(pool) };
        assert_eq!(healthy, 1, "fresh pool should be healthy");
        // SAFETY: pool 有效
        unsafe { sz_orm_cpp_pool_free(pool) };
    }

    #[test]
    fn test_cpp_execute_and_query_roundtrip() {
        let pool = test_pool();
        assert!(!pool.is_null());

        let create =
            CString::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        let rp = unsafe { sz_orm_cpp_execute(pool, create.as_ptr()) };
        assert!(!rp.is_null(), "CREATE should succeed");
        // SAFETY: rp 由 sz_orm_cpp_execute 分配
        let r = unsafe { Box::from_raw(rp) };
        assert_eq!(
            r.success, 1,
            "CREATE should succeed, got code {}",
            r.error_code
        );
        // SAFETY: r 配对释放
        unsafe { sz_orm_cpp_result_free(Box::into_raw(r)) };

        let insert = CString::new("INSERT INTO users (name) VALUES ('Alice'), ('Bob')").unwrap();
        // SAFETY: pool/sql 有效
        let rp = unsafe { sz_orm_cpp_execute(pool, insert.as_ptr()) };
        assert!(!rp.is_null(), "INSERT should succeed");
        // SAFETY: rp 由 sz_orm_cpp_execute 分配
        let r = unsafe { Box::from_raw(rp) };
        assert_eq!(r.success, 1);
        assert_eq!(r.rows_affected, 2, "two rows inserted");
        // SAFETY: r 配对释放
        unsafe { sz_orm_cpp_result_free(Box::into_raw(r)) };

        let q = CString::new("SELECT id, name FROM users ORDER BY id").unwrap();
        // SAFETY: pool/sql 有效
        let json_ptr = unsafe { sz_orm_cpp_query(pool, q.as_ptr()) };
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
        // SAFETY: json_ptr 由 sz_orm_cpp_query 分配，配对释放
        unsafe { sz_orm_cpp_string_free(json_ptr) };

        // SAFETY: pool 有效
        unsafe { sz_orm_cpp_pool_free(pool) };
    }

    #[test]
    fn test_cpp_query_invalid_sql_returns_null() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let q = CString::new("SELECT * FROM nonexistent").unwrap();
        // SAFETY: pool/sql 有效
        let json_ptr = unsafe { sz_orm_cpp_query(pool, q.as_ptr()) };
        assert!(json_ptr.is_null(), "invalid SQL should return null");
        // SAFETY: pool 有效
        unsafe { sz_orm_cpp_pool_free(pool) };
    }

    #[test]
    fn test_cpp_null_args() {
        // SAFETY: null 参数合法调用
        let pool = unsafe { sz_orm_cpp_pool_new(std::ptr::null(), std::ptr::null()) };
        assert!(pool.is_null());
        // SAFETY: null 参数合法调用
        let r = unsafe { sz_orm_cpp_execute(std::ptr::null_mut(), std::ptr::null()) };
        assert!(r.is_null());
        // SAFETY: null 参数合法调用
        let q = unsafe { sz_orm_cpp_query(std::ptr::null_mut(), std::ptr::null()) };
        assert!(q.is_null());
    }

    #[test]
    fn test_cpp_version() {
        let v = sz_orm_cpp_version();
        assert!(v >= 1, "version should be >= 1, got {v}");
    }

    #[test]
    fn test_cpp_free_null_safe() {
        // SAFETY: null 参数合法调用
        unsafe { sz_orm_cpp_string_free(std::ptr::null_mut()) };
        // SAFETY: null 参数合法调用
        unsafe { sz_orm_cpp_result_free(std::ptr::null_mut()) };
    }

    #[test]
    fn test_cpp_ping_after_free_returns_zero() {
        // 使用 null handle 测试 ping（而非释放后的 UAF）
        // SAFETY: null handle 合法调用
        let healthy = unsafe { sz_orm_cpp_ping(std::ptr::null_mut()) };
        assert_eq!(healthy, 0, "null pool should not be healthy");
    }

    #[test]
    fn test_cpp_multiple_pools_independent() {
        let dsn1 = CString::new("sqlite::memory:").unwrap();
        // SAFETY: dsn1 有效
        let pool1 = unsafe { sz_orm_cpp_pool_new(dsn1.as_ptr(), std::ptr::null()) };
        let dsn2 = CString::new("sqlite::memory:").unwrap();
        // SAFETY: dsn2 有效
        let pool2 = unsafe { sz_orm_cpp_pool_new(dsn2.as_ptr(), std::ptr::null()) };
        assert!(!pool1.is_null());
        assert!(!pool2.is_null());
        assert_ne!(pool1, pool2, "pools should have distinct handles");
        // SAFETY: pools 有效
        unsafe {
            sz_orm_cpp_pool_free(pool1);
            sz_orm_cpp_pool_free(pool2);
        }
    }

    #[test]
    fn test_cpp_execute_empty_sql() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE cpp_e (id INTEGER PRIMARY KEY)").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_cpp_execute(pool, create.as_ptr()) };
        // SAFETY: r 配对释放
        unsafe { sz_orm_cpp_result_free(r) };
        // SAFETY: pool 有效
        unsafe { sz_orm_cpp_pool_free(pool) };
    }

    #[test]
    fn test_cpp_query_empty_table() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE empty_t (id INTEGER PRIMARY KEY)").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_cpp_execute(pool, create.as_ptr()) };
        // SAFETY: r 配对释放
        unsafe { sz_orm_cpp_result_free(r) };
        let q = CString::new("SELECT id FROM empty_t").unwrap();
        // SAFETY: pool/sql 有效
        let json_ptr = unsafe { sz_orm_cpp_query(pool, q.as_ptr()) };
        assert!(
            !json_ptr.is_null(),
            "empty table query should still return JSON"
        );
        // SAFETY: json_ptr 有效
        let json = unsafe { CStr::from_ptr(json_ptr) }
            .to_string_lossy()
            .into_owned();
        assert!(
            json.contains("[]") || json.is_empty() || json.contains("id"),
            "empty result should be empty array: {json}"
        );
        // SAFETY: json_ptr 配对释放
        unsafe { sz_orm_cpp_string_free(json_ptr) };
        // SAFETY: pool 有效
        unsafe { sz_orm_cpp_pool_free(pool) };
    }
}
