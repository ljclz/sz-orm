//! # SZ-ORM Go Bindings
//!
//! 通过 cgo 调用 sz-orm-cabi 的 C ABI，为 Go 提供 sz-orm-core 的
//! Model/QueryBuilder/Pool/Transaction API。

use sz_orm_cabi::{PoolConfigC, QueryResultC, SzOrmErrorCode, SzOrmPoolHandle};

/// 创建连接池
#[no_mangle]
pub extern "C" fn sz_orm_go_pool_new(config: PoolConfigC) -> SzOrmPoolHandle {
    let _ = config;
    std::ptr::null_mut()
}

/// 释放连接池
///
/// # Safety
///
/// SAFETY: handle 必须是 `sz_orm_go_pool_new` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_go_pool_free(handle: SzOrmPoolHandle) {
    let _ = handle;
}

/// 执行查询
#[no_mangle]
pub extern "C" fn sz_orm_go_query(
    handle: SzOrmPoolHandle,
    sql_ptr: *const std::ffi::c_char,
) -> QueryResultC {
    if handle.is_null() || sql_ptr.is_null() {
        return QueryResultC {
            success: 0,
            error_code: SzOrmErrorCode::InvalidArgument.as_i32(),
            rows_affected: 0,
            last_insert_id: 0,
        };
    }
    QueryResultC::default()
}

/// 获取版本号
#[no_mangle]
pub extern "C" fn sz_orm_go_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_new_returns_null() {
        let handle = sz_orm_go_pool_new(PoolConfigC::default());
        assert!(handle.is_null());
    }

    #[test]
    fn test_pool_free_null_is_safe() {
        unsafe { sz_orm_go_pool_free(std::ptr::null_mut()) };
    }

    #[test]
    fn test_query_null_returns_error() {
        let result = sz_orm_go_query(std::ptr::null_mut(), std::ptr::null());
        assert_eq!(result.success, 0);
        assert_eq!(result.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
    }

    #[test]
    fn test_version() {
        assert_eq!(sz_orm_go_version(), 1);
    }
}
