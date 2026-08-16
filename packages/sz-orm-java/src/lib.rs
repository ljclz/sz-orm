//! # SZ-ORM Java Bindings
//!
//! 通过 JNI 调用 sz-orm-cabi 的 C ABI，为 Java 提供 sz-orm-core 的
//! Pool/Query API（SQLite 后端，真实可用）。
//!
//! JNI 符号名遵循 javac -h 生成的 `sz_orm_java_SzOrmPool.h`：
//! 包名 `sz_orm_java` 中的下划线转义为 `_1`。

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::EnvUnowned;

use std::ffi::CString;

/// JNI 入口：创建连接池（真实创建，SQLite 后端）
///
/// 返回句柄（jlong），0 表示失败。
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_poolNew<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    dsn: JString<'local>,
    max_connections: jint,
) -> jlong {
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let dsn_str: String = dsn.to_string();
            let c_dsn = CString::new(dsn_str).map_err(|_| jni::errors::Error::JavaException)?;
            let config = sz_orm_cabi::PoolConfigC {
                max_connections: max_connections.max(1) as u32,
                min_connections: 1,
                connect_timeout_ms: 5000,
                idle_timeout_ms: 300000,
            };
            // SAFETY: c_dsn 是有效的 NUL 结尾 C 字符串
            let handle = unsafe { sz_orm_cabi::sz_orm_pool_new(c_dsn.as_ptr(), &config) };
            Ok(handle as jlong)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：释放连接池
///
/// # Safety
///
/// SAFETY: handle 必须是 `poolNew` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_poolFree<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }
    let _ = unowned_env.with_env(|_env| -> jni::errors::Result<()> {
        // SAFETY: 调用方保证 handle 来自 poolNew
        unsafe {
            sz_orm_cabi::sz_orm_pool_free(handle as sz_orm_cabi::SzOrmPoolHandle);
        }
        Ok(())
    });
}

/// JNI 入口：健康检查（真实 acquire + ping）
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `poolNew` 返回的有效句柄，且未被 `poolFree` 释放。
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_ping<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> {
            // SAFETY: 调用方保证 handle 来自 poolNew
            let healthy =
                unsafe { sz_orm_cabi::sz_orm_ping(handle as sz_orm_cabi::SzOrmPoolHandle) };
            Ok(healthy)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：执行查询，返回 JSON 行数组字符串
///
/// 失败返回 null（Java 侧抛 IllegalStateException）。
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `poolNew` 返回的有效句柄，且未被 `poolFree` 释放；
/// `sql` 必须是 JNI 传入的有效 `JString`。
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_query<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
    sql: JString<'local>,
) -> jstring {
    if handle == 0 {
        return std::ptr::null_mut();
    }
    unowned_env
        .with_env(|env| -> jni::errors::Result<jstring> {
            let sql_str: String = sql.to_string();
            let c_sql = CString::new(sql_str).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: handle 来自 poolNew，c_sql 有效
            let result = unsafe {
                sz_orm_cabi::sz_orm_query(handle as sz_orm_cabi::SzOrmPoolHandle, c_sql.as_ptr())
            };
            if result.is_null() {
                return Ok(std::ptr::null_mut());
            }
            // SAFETY: result 由 sz_orm_query 分配
            let result_box = unsafe { Box::from_raw(result) };
            let json = if result_box.success != 0 && !result_box.json.is_null() {
                // SAFETY: json 是有效 NUL 结尾 C 字符串
                unsafe { std::ffi::CStr::from_ptr(result_box.json) }
                    .to_str()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            // SAFETY: result_box 由 sz_orm_query 分配，配对释放
            unsafe {
                sz_orm_cabi::sz_orm_query_result_free(Box::into_raw(result_box));
            }
            let jstr = env.new_string(&json)?;
            Ok(jstr.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：执行写语句，返回影响行数（-1 表示失败）
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `poolNew` 返回的有效句柄，且未被 `poolFree` 释放；
/// `sql` 必须是 JNI 传入的有效 `JString`。
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_execute<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
    sql: JString<'local>,
) -> jlong {
    if handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let sql_str: String = sql.to_string();
            let c_sql = CString::new(sql_str).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: handle 来自 poolNew，c_sql 有效
            let result = unsafe {
                sz_orm_cabi::sz_orm_execute(handle as sz_orm_cabi::SzOrmPoolHandle, c_sql.as_ptr())
            };
            if result.success == 0 {
                Ok(-1)
            } else {
                Ok(result.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：绑定版本号
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_version<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
) -> jint {
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> { Ok(sz_orm_cabi::sz_orm_version() as jint) })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;
    use sz_orm_cabi::{PoolConfigC, SzOrmPoolHandle};

    fn test_pool() -> SzOrmPoolHandle {
        let dsn = CString::new("sqlite::memory:").unwrap();
        let config = PoolConfigC {
            max_connections: 1,
            min_connections: 1,
            connect_timeout_ms: 5000,
            idle_timeout_ms: 300000,
        };
        // SAFETY: dsn 有效
        unsafe { sz_orm_cabi::sz_orm_pool_new(dsn.as_ptr(), &config) }
    }

    #[test]
    fn test_java_binding_pool_create() {
        let pool = test_pool();
        assert!(!pool.is_null(), "pool should be created via cabi");
        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_java_binding_ping() {
        let pool = test_pool();
        assert!(!pool.is_null());
        // SAFETY: pool 有效
        let healthy = unsafe { sz_orm_cabi::sz_orm_ping(pool) };
        assert_eq!(healthy, 1);
        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_java_binding_version() {
        let v = sz_orm_cabi::sz_orm_version();
        assert!(v >= 1, "version should be >= 1, got {v}");
    }

    #[test]
    fn test_java_binding_execute_and_query() {
        let pool = test_pool();
        assert!(!pool.is_null());

        let create = CString::new("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_cabi::sz_orm_execute(pool, create.as_ptr()) };
        assert_eq!(r.success, 1);

        let insert = CString::new("INSERT INTO t (v) VALUES ('x')").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_cabi::sz_orm_execute(pool, insert.as_ptr()) };
        assert_eq!(r.success, 1);
        assert_eq!(r.rows_affected, 1);

        let q = CString::new("SELECT v FROM t").unwrap();
        // SAFETY: pool/sql 有效
        let result_ptr = unsafe { sz_orm_cabi::sz_orm_query(pool, q.as_ptr()) };
        assert!(!result_ptr.is_null());
        // SAFETY: result_ptr 由 sz_orm_query 分配
        let result_box = unsafe { Box::from_raw(result_ptr) };
        assert_eq!(result_box.success, 1);
        // SAFETY: result_box 配对释放
        unsafe { sz_orm_cabi::sz_orm_query_result_free(Box::into_raw(result_box)) };

        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_java_binding_null_dsn_returns_null() {
        // SAFETY: null dsn 合法调用
        let pool = unsafe { sz_orm_cabi::sz_orm_pool_new(std::ptr::null(), std::ptr::null()) };
        assert!(pool.is_null(), "null dsn should return null pool");
    }

    #[test]
    fn test_java_binding_pool_config_clamped() {
        let dsn = CString::new("sqlite::memory:").unwrap();
        let config = PoolConfigC {
            max_connections: 0,
            min_connections: 0,
            connect_timeout_ms: 100,
            idle_timeout_ms: 100,
        };
        // SAFETY: dsn 有效
        let pool = unsafe { sz_orm_cabi::sz_orm_pool_new(dsn.as_ptr(), &config) };
        // cabi 内部会 clamp max_connections 到 >= 1
        if !pool.is_null() {
            // SAFETY: pool 有效
            unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
        }
    }
}
