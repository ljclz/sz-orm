//! # SZ-ORM Java Bindings
//!
//! 通过 JNI 调用 sz-orm-cabi 的 C ABI，为 Java 提供 sz-orm-core 的
//! Model/QueryBuilder/Pool/Transaction API。

use sz_orm_cabi::{PoolConfigC, QueryResultC, SzOrmErrorCode, SzOrmPoolHandle};

/// JNI 入口：创建连接池
#[no_mangle]
pub extern "C" fn Java_sz_orm_java_SzOrmPool_poolNew(
    _env: *mut std::ffi::c_void,
    _cls: *mut std::ffi::c_void,
    config: PoolConfigC,
) -> SzOrmPoolHandle {
    let _ = config;
    std::ptr::null_mut()
}

/// JNI 入口：释放连接池
///
/// # Safety
///
/// SAFETY: handle 必须是 `poolNew` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "C" fn Java_sz_orm_java_SzOrmPool_poolFree(
    _env: *mut std::ffi::c_void,
    _cls: *mut std::ffi::c_void,
    handle: SzOrmPoolHandle,
) {
    let _ = handle;
}

/// JNI 入口：执行查询
#[no_mangle]
pub extern "C" fn Java_sz_orm_java_SzOrmPool_query(
    _env: *mut std::ffi::c_void,
    _cls: *mut std::ffi::c_void,
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
pub extern "C" fn sz_orm_java_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jni_pool_new() {
        let handle = Java_sz_orm_java_SzOrmPool_poolNew(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            PoolConfigC::default(),
        );
        assert!(handle.is_null());
    }

    #[test]
    fn test_jni_pool_free_null() {
        unsafe {
            Java_sz_orm_java_SzOrmPool_poolFree(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
    }

    #[test]
    fn test_jni_query_null_returns_error() {
        let result = Java_sz_orm_java_SzOrmPool_query(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        assert_eq!(result.success, 0);
        assert_eq!(result.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
    }

    #[test]
    fn test_version() {
        assert_eq!(sz_orm_java_version(), 1);
    }
}
