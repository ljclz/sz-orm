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

// ============================================================================
// 事务 JNI 入口（REQ-BND-006）
// ============================================================================

/// JNI 入口：开始事务，返回事务句柄（0 表示失败）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_beginTransaction<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    pool_handle: jlong,
) -> jlong {
    if pool_handle == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            // SAFETY: 调用方保证 pool_handle 来自 poolNew
            let tx = unsafe {
                sz_orm_cabi::sz_orm_transaction_begin(pool_handle as sz_orm_cabi::SzOrmPoolHandle)
            };
            Ok(tx as jlong)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：提交事务，返回 1=成功 0=失败
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_commitTransaction<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    tx_handle: jlong,
) -> jint {
    if tx_handle == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> {
            // SAFETY: 调用方保证 tx_handle 来自 beginTransaction
            let r = unsafe {
                sz_orm_cabi::sz_orm_transaction_commit(
                    tx_handle as sz_orm_cabi::SzOrmTransactionHandle,
                )
            };
            Ok(r)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：回滚事务，返回 1=成功 0=失败
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_rollbackTransaction<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    tx_handle: jlong,
) -> jint {
    if tx_handle == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> {
            // SAFETY: 调用方保证 tx_handle 来自 beginTransaction
            let r = unsafe {
                sz_orm_cabi::sz_orm_transaction_rollback(
                    tx_handle as sz_orm_cabi::SzOrmTransactionHandle,
                )
            };
            Ok(r)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：释放事务句柄（若仍活跃则自动回滚）
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `beginTransaction` 返回的有效句柄。
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_freeTransaction<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    tx_handle: jlong,
) {
    if tx_handle == 0 {
        return;
    }
    let _ = unowned_env.with_env(|_env| -> jni::errors::Result<()> {
        // SAFETY: 调用方保证 tx_handle 来自 beginTransaction
        unsafe {
            sz_orm_cabi::sz_orm_transaction_free(tx_handle as sz_orm_cabi::SzOrmTransactionHandle);
        }
        Ok(())
    });
}

// ============================================================================
// 模型级 JNI 入口（REQ-BND-013）
// ============================================================================

/// JNI 入口：在 pool 上插入行，返回受影响行数（-1 表示失败）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_modelInsert<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    pool_handle: jlong,
    table: JString<'local>,
    fields_json: JString<'local>,
    values_json: JString<'local>,
) -> jlong {
    if pool_handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let table_s: String = table.to_string();
            let fields_s: String = fields_json.to_string();
            let values_s: String = values_json.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_fields = CString::new(fields_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_values = CString::new(values_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: pool_handle 来自 poolNew，C 字符串有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_model_insert(
                    pool_handle as sz_orm_cabi::SzOrmPoolHandle,
                    c_table.as_ptr(),
                    c_fields.as_ptr(),
                    c_values.as_ptr(),
                )
            };
            if r.success == 0 {
                Ok(-1)
            } else {
                Ok(r.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：在 pool 上更新行，返回受影响行数（-1 表示失败）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_modelUpdate<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    pool_handle: jlong,
    table: JString<'local>,
    set_json: JString<'local>,
    where_clause: JString<'local>,
    where_params_json: JString<'local>,
) -> jlong {
    if pool_handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let table_s: String = table.to_string();
            let set_s: String = set_json.to_string();
            let where_s: String = where_clause.to_string();
            let where_params_s: String = where_params_json.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_set = CString::new(set_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where = CString::new(where_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where_params =
                CString::new(where_params_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: pool_handle 来自 poolNew，C 字符串有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_model_update(
                    pool_handle as sz_orm_cabi::SzOrmPoolHandle,
                    c_table.as_ptr(),
                    c_set.as_ptr(),
                    c_where.as_ptr(),
                    c_where_params.as_ptr(),
                )
            };
            if r.success == 0 {
                Ok(-1)
            } else {
                Ok(r.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：在 pool 上删除行，返回受影响行数（-1 表示失败）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_modelDelete<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    pool_handle: jlong,
    table: JString<'local>,
    where_clause: JString<'local>,
    where_params_json: JString<'local>,
) -> jlong {
    if pool_handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let table_s: String = table.to_string();
            let where_s: String = where_clause.to_string();
            let where_params_s: String = where_params_json.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where = CString::new(where_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where_params =
                CString::new(where_params_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: pool_handle 来自 poolNew，C 字符串有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_model_delete(
                    pool_handle as sz_orm_cabi::SzOrmPoolHandle,
                    c_table.as_ptr(),
                    c_where.as_ptr(),
                    c_where_params.as_ptr(),
                )
            };
            if r.success == 0 {
                Ok(-1)
            } else {
                Ok(r.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：在 pool 上查询行，返回 JSON 行数组字符串（失败返回空字符串）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_modelFind<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    pool_handle: jlong,
    table: JString<'local>,
    where_clause: JString<'local>,
    where_params_json: JString<'local>,
) -> jstring {
    if pool_handle == 0 {
        return std::ptr::null_mut();
    }
    unowned_env
        .with_env(|env| -> jni::errors::Result<jstring> {
            let table_s: String = table.to_string();
            let where_s: String = where_clause.to_string();
            let where_params_s: String = where_params_json.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where = CString::new(where_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where_params =
                CString::new(where_params_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: pool_handle 来自 poolNew，C 字符串有效
            let ptr = unsafe {
                sz_orm_cabi::sz_orm_model_find(
                    pool_handle as sz_orm_cabi::SzOrmPoolHandle,
                    c_table.as_ptr(),
                    c_where.as_ptr(),
                    c_where_params.as_ptr(),
                )
            };
            let json = if ptr.is_null() {
                String::new()
            } else {
                // SAFETY: ptr 有效
                let s = unsafe { std::ffi::CStr::from_ptr(ptr) }
                    .to_str()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                // SAFETY: ptr 配对释放
                unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
                s
            };
            let jstr = env.new_string(&json)?;
            Ok(jstr.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：在事务内插入行，返回受影响行数（-1 表示失败）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_modelInsertTx<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    tx_handle: jlong,
    table: JString<'local>,
    fields_json: JString<'local>,
    values_json: JString<'local>,
) -> jlong {
    if tx_handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let table_s: String = table.to_string();
            let fields_s: String = fields_json.to_string();
            let values_s: String = values_json.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_fields = CString::new(fields_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_values = CString::new(values_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: tx_handle 来自 beginTransaction，C 字符串有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_model_insert_tx(
                    tx_handle as sz_orm_cabi::SzOrmTransactionHandle,
                    c_table.as_ptr(),
                    c_fields.as_ptr(),
                    c_values.as_ptr(),
                )
            };
            if r.success == 0 {
                Ok(-1)
            } else {
                Ok(r.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：在事务内更新行，返回受影响行数（-1 表示失败）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_modelUpdateTx<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    tx_handle: jlong,
    table: JString<'local>,
    set_json: JString<'local>,
    where_clause: JString<'local>,
    where_params_json: JString<'local>,
) -> jlong {
    if tx_handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let table_s: String = table.to_string();
            let set_s: String = set_json.to_string();
            let where_s: String = where_clause.to_string();
            let where_params_s: String = where_params_json.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_set = CString::new(set_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where = CString::new(where_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where_params =
                CString::new(where_params_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: tx_handle 来自 beginTransaction，C 字符串有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_model_update_tx(
                    tx_handle as sz_orm_cabi::SzOrmTransactionHandle,
                    c_table.as_ptr(),
                    c_set.as_ptr(),
                    c_where.as_ptr(),
                    c_where_params.as_ptr(),
                )
            };
            if r.success == 0 {
                Ok(-1)
            } else {
                Ok(r.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：在事务内删除行，返回受影响行数（-1 表示失败）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_modelDeleteTx<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    tx_handle: jlong,
    table: JString<'local>,
    where_clause: JString<'local>,
    where_params_json: JString<'local>,
) -> jlong {
    if tx_handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let table_s: String = table.to_string();
            let where_s: String = where_clause.to_string();
            let where_params_s: String = where_params_json.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where = CString::new(where_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where_params =
                CString::new(where_params_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: tx_handle 来自 beginTransaction，C 字符串有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_model_delete_tx(
                    tx_handle as sz_orm_cabi::SzOrmTransactionHandle,
                    c_table.as_ptr(),
                    c_where.as_ptr(),
                    c_where_params.as_ptr(),
                )
            };
            if r.success == 0 {
                Ok(-1)
            } else {
                Ok(r.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI 入口：在事务内查询行，返回 JSON 行数组字符串（失败返回空字符串）
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_modelFindTx<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    tx_handle: jlong,
    table: JString<'local>,
    where_clause: JString<'local>,
    where_params_json: JString<'local>,
) -> jstring {
    if tx_handle == 0 {
        return std::ptr::null_mut();
    }
    unowned_env
        .with_env(|env| -> jni::errors::Result<jstring> {
            let table_s: String = table.to_string();
            let where_s: String = where_clause.to_string();
            let where_params_s: String = where_params_json.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where = CString::new(where_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_where_params =
                CString::new(where_params_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: tx_handle 来自 beginTransaction，C 字符串有效
            let ptr = unsafe {
                sz_orm_cabi::sz_orm_model_find_tx(
                    tx_handle as sz_orm_cabi::SzOrmTransactionHandle,
                    c_table.as_ptr(),
                    c_where.as_ptr(),
                    c_where_params.as_ptr(),
                )
            };
            let json = if ptr.is_null() {
                String::new()
            } else {
                // SAFETY: ptr 有效
                let s = unsafe { std::ffi::CStr::from_ptr(ptr) }
                    .to_str()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                // SAFETY: ptr 配对释放
                unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
                s
            };
            let jstr = env.new_string(&json)?;
            Ok(jstr.into_raw())
        })
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

    // ===== 新增测试：事务 + 模型级 API 转发（REQ-BND-006/013/014）=====

    fn model_test_pool() -> SzOrmPoolHandle {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create =
            CString::new("CREATE TABLE jm_t (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
                .unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_cabi::sz_orm_execute(pool, create.as_ptr()) };
        assert_eq!(r.success, 1, "CREATE should succeed");
        pool
    }

    #[test]
    fn test_java_model_insert_find_roundtrip() {
        let pool = model_test_pool();
        let table = CString::new("jm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["Alice",30]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        let r = unsafe {
            sz_orm_cabi::sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };
        assert_eq!(
            r.success, 1,
            "java model_insert should succeed, code={}",
            r.error_code
        );
        assert_eq!(r.rows_affected, 1);

        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["Alice"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_cabi::sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(!ptr.is_null());
        // SAFETY: ptr 有效
        let json = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: ptr 配对释放
        unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
        assert!(
            json.contains("Alice"),
            "java model_find should contain Alice: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_java_model_update_delete_find() {
        let pool = model_test_pool();
        let table = CString::new("jm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["Bob",25]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        unsafe {
            sz_orm_cabi::sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };

        let set_json = CString::new(r#"{"age":26}"#).unwrap();
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["Bob"]"#).unwrap();
        // SAFETY: pool/table/set/where 有效
        let r = unsafe {
            sz_orm_cabi::sz_orm_model_update(
                pool,
                table.as_ptr(),
                set_json.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert_eq!(
            r.success, 1,
            "java model_update should succeed, code={}",
            r.error_code
        );

        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_cabi::sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
        assert!(
            json.contains("26"),
            "java find after update should contain age 26: {json}"
        );

        // SAFETY: pool/table/where 有效
        let r = unsafe {
            sz_orm_cabi::sz_orm_model_delete(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert_eq!(
            r.success, 1,
            "java model_delete should succeed, code={}",
            r.error_code
        );

        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_cabi::sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
        assert!(
            json == "[]" || json == "null" || json.is_empty(),
            "java find after delete should be empty: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_java_model_insert_illegal_table() {
        let pool = model_test_pool();
        let table = CString::new("jm_t; DROP--").unwrap();
        let fields = CString::new(r#"["name"]"#).unwrap();
        let values = CString::new(r#"["X"]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效（table 含注入向量）
        let r = unsafe {
            sz_orm_cabi::sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };
        assert_eq!(r.success, 0, "illegal table should fail");
        assert_eq!(
            r.error_code,
            sz_orm_cabi::SzOrmErrorCode::InvalidArgument.as_i32()
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_java_transaction_model_rollback() {
        let pool = model_test_pool();
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_cabi::sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());

        let table = CString::new("jm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["TxUser",99]"#).unwrap();
        // SAFETY: tx/table/fields/values 有效
        let r = unsafe {
            sz_orm_cabi::sz_orm_model_insert_tx(
                tx,
                table.as_ptr(),
                fields.as_ptr(),
                values.as_ptr(),
            )
        };
        assert_eq!(
            r.success, 1,
            "java model_insert_tx should succeed, code={}",
            r.error_code
        );

        // SAFETY: tx 有效
        assert_eq!(unsafe { sz_orm_cabi::sz_orm_transaction_rollback(tx) }, 1);
        // SAFETY: tx 有效
        unsafe { sz_orm_cabi::sz_orm_transaction_free(tx) };

        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["TxUser"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_cabi::sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
        assert!(
            json == "[]" || json == "null" || json.is_empty(),
            "java find after rollback should be empty: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_java_transaction_model_commit() {
        let pool = model_test_pool();
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_cabi::sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());

        let table = CString::new("jm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["CommitUser",50]"#).unwrap();
        // SAFETY: tx/table/fields/values 有效
        unsafe {
            sz_orm_cabi::sz_orm_model_insert_tx(
                tx,
                table.as_ptr(),
                fields.as_ptr(),
                values.as_ptr(),
            )
        };

        // SAFETY: tx 有效
        assert_eq!(unsafe { sz_orm_cabi::sz_orm_transaction_commit(tx) }, 1);
        // SAFETY: tx 有效
        unsafe { sz_orm_cabi::sz_orm_transaction_free(tx) };

        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["CommitUser"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_cabi::sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
        assert!(
            json.contains("CommitUser"),
            "java find after commit should contain CommitUser: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }
}
