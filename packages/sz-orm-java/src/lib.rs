//! # SZ-ORM Java Bindings
//!
//! Calls sz-orm-cabi's C ABI via JNI, providing Java with sz-orm-core's
//! Pool/Query API (SQLite backend, real and usable).
//!
//! JNI symbol names follow javac -h generated `sz_orm_java_SzOrmPool.h`:
//! Underscores in package name `sz_orm_java` are escaped as `_1`.

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::EnvUnowned;

use std::ffi::CString;

/// JNI entry: Create connection pool (real creation, SQLite backend)
///
/// Returns handle (jlong), 0 indicates failure.
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

/// JNI entry: Free connection pool
///
/// # Safety
///
/// SAFETY: handle must be a valid handle returned by `poolNew`.
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

/// JNI entry: Health check (real acquire + ping)
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `poolNew`, and not freed by `poolFree`.
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

/// JNI entry: Execute query, return JSON row array string
///
/// Returns null on failure (Java side throws IllegalStateException).
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `poolNew`, and not freed by `poolFree`;
/// `sql` must be a valid `JString` passed from JNI.
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

/// JNI entry: Execute write statement, return affected row count (-1 indicates failure)
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `poolNew`, and not freed by `poolFree`;
/// `sql` must be a valid `JString` passed from JNI.
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

/// JNI entry: Bind version number
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

/// JNI entry: Begin transaction, return transaction handle (0 indicates failure)
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

/// JNI entry: Commit transaction, return 1=success 0=failure
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

/// JNI entry: Rollback transaction, return 1=success 0=failure
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

/// JNI entry: Free transaction handle (auto rollback if still active)
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by `beginTransaction`.
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

/// JNI entry: Insert row on pool, return affected row count (-1 indicates failure)
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

/// JNI entry: Update row on pool, return affected row count (-1 indicates failure)
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

/// JNI entry: Delete row on pool, return affected row count (-1 indicates failure)
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

/// JNI entry: Query row on pool, return JSON row array string (empty string on failure)
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

/// JNI entry: Insert row in transaction, return affected row count (-1 indicates failure)
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

/// JNI entry: Update row in transaction, return affected row count (-1 indicates failure)
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

/// JNI entry: Delete row in transaction, return affected row count (-1 indicates failure)
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

/// JNI entry: Query row in transaction, return JSON row array string (empty string on failure)
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

// ============================================================================
// QueryBuilder JNI 入口（v6.9.0 REQ-BND-QB）
// ============================================================================

/// JNI entry: Create QueryBuilder, return handle (0 indicates failure)
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmQueryBuilder_qbNew<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    db_type: jint,
) -> jlong {
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            // SAFETY: db_type 是 u32 范围内的整数
            let handle = unsafe { sz_orm_cabi::sz_orm_qb_new(db_type as u32) };
            Ok(handle as jlong)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Set table name on QueryBuilder, return 1=success 0=failure
///
/// # Safety
///
/// SAFETY: `qb` must be a valid handle returned by `qbNew`.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmQueryBuilder_qbTable<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    qb: jlong,
    table: JString<'local>,
) -> jint {
    if qb == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> {
            let table_s: String = table.to_string();
            let c_table = CString::new(table_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: qb 来自 qbNew，c_table 有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_qb_table(
                    qb as sz_orm_cabi::SzOrmQueryBuilderHandle,
                    c_table.as_ptr(),
                )
            };
            Ok(r)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Add WHERE eq condition, return 1=success 0=failure
///
/// # Safety
///
/// SAFETY: `qb` must be a valid handle returned by `qbNew`.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmQueryBuilder_qbWhereEq<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    qb: jlong,
    field: JString<'local>,
    value_json: JString<'local>,
) -> jint {
    if qb == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> {
            let field_s: String = field.to_string();
            let value_s: String = value_json.to_string();
            let c_field = CString::new(field_s).map_err(|_| jni::errors::Error::JavaException)?;
            let c_value = CString::new(value_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: qb 来自 qbNew，C 字符串有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_qb_where_eq(
                    qb as sz_orm_cabi::SzOrmQueryBuilderHandle,
                    c_field.as_ptr(),
                    c_value.as_ptr(),
                )
            };
            Ok(r)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Add ORDER BY clause, return 1=success 0=failure
///
/// # Safety
///
/// SAFETY: `qb` must be a valid handle returned by `qbNew`.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmQueryBuilder_qbOrderBy<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    qb: jlong,
    field: JString<'local>,
    desc: jint,
) -> jint {
    if qb == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> {
            let field_s: String = field.to_string();
            let c_field = CString::new(field_s).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: qb 来自 qbNew，c_field 有效
            let r = unsafe {
                sz_orm_cabi::sz_orm_qb_order_by(
                    qb as sz_orm_cabi::SzOrmQueryBuilderHandle,
                    c_field.as_ptr(),
                    desc,
                )
            };
            Ok(r)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Set LIMIT, return 1=success 0=failure
///
/// # Safety
///
/// SAFETY: `qb` must be a valid handle returned by `qbNew`.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmQueryBuilder_qbLimit<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    qb: jlong,
    limit: jlong,
) -> jint {
    if qb == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> {
            // SAFETY: qb 来自 qbNew
            let r = unsafe {
                sz_orm_cabi::sz_orm_qb_limit(
                    qb as sz_orm_cabi::SzOrmQueryBuilderHandle,
                    limit as u64,
                )
            };
            Ok(r)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Build SQL + params JSON, return JSON string (null on failure)
///
/// # Safety
///
/// SAFETY: `qb` must be a valid handle returned by `qbNew`.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmQueryBuilder_qbBuild<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    qb: jlong,
) -> jstring {
    if qb == 0 {
        return std::ptr::null_mut();
    }
    unowned_env
        .with_env(|env| -> jni::errors::Result<jstring> {
            // SAFETY: qb 来自 qbNew
            let ptr =
                unsafe { sz_orm_cabi::sz_orm_qb_build(qb as sz_orm_cabi::SzOrmQueryBuilderHandle) };
            if ptr.is_null() {
                return Ok(std::ptr::null_mut());
            }
            // SAFETY: ptr 由 sz_orm_qb_build 分配
            let json = unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_str()
                .map(|s| s.to_string())
                .unwrap_or_default();
            // SAFETY: ptr 配对释放
            unsafe { sz_orm_cabi::sz_orm_qb_result_free(ptr) };
            let jstr = env.new_string(&json)?;
            Ok(jstr.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Free QueryBuilder handle
///
/// # Safety
///
/// SAFETY: `qb` must be a valid handle returned by `qbNew`.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmQueryBuilder_qbFree<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    qb: jlong,
) {
    if qb == 0 {
        return;
    }
    let _ = unowned_env.with_env(|_env| -> jni::errors::Result<()> {
        // SAFETY: 调用方保证 qb 来自 qbNew
        unsafe {
            sz_orm_cabi::sz_orm_qb_free(qb as sz_orm_cabi::SzOrmQueryBuilderHandle);
        }
        Ok(())
    });
}

// ============================================================================
// ActiveModel JNI 入口（v6.9.0 REQ-BND-AM）
// ============================================================================

/// JNI entry: ActiveModel create (insert), return affected rows (-1=failure)
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmActiveModel_create<'local>(
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

/// JNI entry: ActiveModel set (update), return affected rows (-1=failure)
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmActiveModel_set<'local>(
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

/// JNI entry: ActiveModel get (find), return JSON rows (null on failure)
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmActiveModel_get<'local>(
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

/// JNI entry: ActiveModel save (insert or update), return affected rows (-1=failure)
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmActiveModel_save<'local>(
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

/// JNI entry: ActiveModel delete, return affected rows (-1=failure)
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmActiveModel_delete<'local>(
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

/// JNI entry: Get pool statistics as JSON
///
/// # Safety
///
/// SAFETY: handle must be a valid handle returned by `poolNew`.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_poolStats<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jstring {
    if handle == 0 {
        return std::ptr::null_mut();
    }
    unowned_env
        .with_env(|env| -> jni::errors::Result<jstring> {
            // SAFETY: handle 来自 poolNew
            let stats =
                unsafe { sz_orm_cabi::sz_orm_pool_stats(handle as sz_orm_cabi::SzOrmPoolHandle) };
            let json = format!(
                r#"{{"idle":{},"active":{},"max":{},"min":{},"waiters":{}}}"#,
                stats.idle, stats.active, stats.max, stats.min, stats.waiters
            );
            let jstr = env.new_string(&json)?;
            Ok(jstr.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Get pool metrics as JSON
///
/// # Safety
///
/// SAFETY: handle must be a valid handle returned by `poolNew`.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_poolMetrics<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jstring {
    if handle == 0 {
        return std::ptr::null_mut();
    }
    unowned_env
        .with_env(|env| -> jni::errors::Result<jstring> {
            // SAFETY: handle 来自 poolNew
            let m = unsafe {
                sz_orm_cabi::sz_orm_pool_metrics(handle as sz_orm_cabi::SzOrmPoolHandle)
            };
            let json = format!(
                r#"{{"acquire_count":{},"acquire_failed_count":{},"release_count":{},"connection_created_count":{},"connection_closed_count":{}}}"#,
                m.acquire_count, m.acquire_failed_count, m.release_count,
                m.connection_created_count, m.connection_closed_count
            );
            let jstr = env.new_string(&json)?;
            Ok(jstr.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Query single row, return JSON string
///
/// # Safety
///
/// SAFETY: handle must be valid; sql must be a valid JString.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_queryOne<'local>(
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
                sz_orm_cabi::sz_orm_query_one(
                    handle as sz_orm_cabi::SzOrmPoolHandle,
                    c_sql.as_ptr(),
                )
            };
            if result.is_null() {
                return Ok(std::ptr::null_mut());
            }
            // SAFETY: result 由 sz_orm_query_one 分配
            let result_box = unsafe { Box::from_raw(result) };
            let json = if !result_box.json.is_null() {
                // SAFETY: json 是有效 NUL 结尾 C 字符串
                unsafe { std::ffi::CStr::from_ptr(result_box.json) }
                    .to_str()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            // SAFETY: 配对释放
            unsafe {
                sz_orm_cabi::sz_orm_query_result_free(Box::into_raw(result_box));
            }
            let jstr = env.new_string(&json)?;
            Ok(jstr.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Execute batch SQL, return affected row count
///
/// # Safety
///
/// SAFETY: handle must be valid; sql must be a valid JString.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_executeBatch<'local>(
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
                sz_orm_cabi::sz_orm_execute_batch(
                    handle as sz_orm_cabi::SzOrmPoolHandle,
                    c_sql.as_ptr(),
                )
            };
            if result.success == 0 {
                Ok(-1)
            } else {
                Ok(result.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Execute SQL within transaction, return affected row count
///
/// # Safety
///
/// SAFETY: txHandle must be a valid transaction handle.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_transactionExecute<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    tx_handle: jlong,
    sql: JString<'local>,
) -> jlong {
    if tx_handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let sql_str: String = sql.to_string();
            let c_sql = CString::new(sql_str).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: tx_handle 来自 beginTransaction，c_sql 有效
            let result = unsafe {
                sz_orm_cabi::sz_orm_transaction_execute(
                    tx_handle as sz_orm_cabi::SzOrmTransactionHandle,
                    c_sql.as_ptr(),
                )
            };
            if result.success == 0 {
                Ok(-1)
            } else {
                Ok(result.rows_affected as jlong)
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Get error description string
#[no_mangle]
pub extern "system" fn Java_sz_1orm_1java_SzOrmPool_errorDescription<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    code: jint,
) -> jstring {
    unowned_env
        .with_env(|env| -> jni::errors::Result<jstring> {
            // SAFETY: error_description 总是安全调用
            let ptr = unsafe { sz_orm_cabi::sz_orm_error_description(code) };
            if ptr.is_null() {
                let jstr = env.new_string("")?;
                return Ok(jstr.into_raw());
            }
            // SAFETY: ptr 由 error_description 分配
            let c_str = unsafe { std::ffi::CStr::from_ptr(ptr) };
            let s = c_str.to_str().unwrap_or("").to_string();
            // SAFETY: 配对释放
            unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
            let jstr = env.new_string(&s)?;
            Ok(jstr.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Free native string
///
/// # Safety
///
/// SAFETY: ptr must be a valid pointer returned by native code.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_stringFree<'local>(
    _unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) {
    if ptr == 0 {
        return;
    }
    // SAFETY: ptr 来自 native 分配
    unsafe { sz_orm_cabi::sz_orm_string_free(ptr as *mut std::ffi::c_char) };
}

/// JNI entry: Check if table exists (1=yes, 0=no)
///
/// # Safety
///
/// SAFETY: handle must be valid; table must be a valid JString.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_tableExists<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
    table: JString<'local>,
) -> jint {
    if handle == 0 {
        return 0;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jint> {
            let table_str: String = table.to_string();
            let c_table = CString::new(table_str).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: handle 来自 poolNew，c_table 有效
            let exists = unsafe {
                sz_orm_cabi::sz_orm_table_exists(
                    handle as sz_orm_cabi::SzOrmPoolHandle,
                    c_table.as_ptr(),
                )
            };
            Ok(exists)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// JNI entry: Count rows in table
///
/// # Safety
///
/// SAFETY: handle must be valid; table must be a valid JString.
#[no_mangle]
pub unsafe extern "system" fn Java_sz_1orm_1java_SzOrmPool_count<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    handle: jlong,
    table: JString<'local>,
) -> jlong {
    if handle == 0 {
        return -1;
    }
    unowned_env
        .with_env(|_env| -> jni::errors::Result<jlong> {
            let table_str: String = table.to_string();
            let c_table = CString::new(table_str).map_err(|_| jni::errors::Error::JavaException)?;
            // SAFETY: handle 来自 poolNew，c_table 有效
            let count = unsafe {
                sz_orm_cabi::sz_orm_count(handle as sz_orm_cabi::SzOrmPoolHandle, c_table.as_ptr())
            };
            Ok(count)
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

    // ===== v6.9.0 新增测试：QueryBuilder + ActiveModel JNI 转发 =====

    #[test]
    fn test_java_qb_new_and_free() {
        // SAFETY: db_type=1 (SQLite) 合法
        let qb = unsafe { sz_orm_cabi::sz_orm_qb_new(1) };
        assert!(!qb.is_null(), "qb_new should return non-null handle");
        // SAFETY: qb 来自 qb_new
        unsafe { sz_orm_cabi::sz_orm_qb_free(qb) };
    }

    #[test]
    fn test_java_qb_full_chain() {
        // SAFETY: db_type=0 (MySQL) 合法
        let qb = unsafe { sz_orm_cabi::sz_orm_qb_new(0) };
        assert!(!qb.is_null());

        let table = CString::new("users").unwrap();
        // SAFETY: qb 有效
        assert_eq!(
            unsafe { sz_orm_cabi::sz_orm_qb_table(qb, table.as_ptr()) },
            sz_orm_cabi::SzOrmErrorCode::Ok.as_i32()
        );

        let field = CString::new("status").unwrap();
        let value = CString::new(r#""active""#).unwrap();
        // SAFETY: qb 有效
        assert_eq!(
            unsafe { sz_orm_cabi::sz_orm_qb_where_eq(qb, field.as_ptr(), value.as_ptr()) },
            sz_orm_cabi::SzOrmErrorCode::Ok.as_i32()
        );

        let order_field = CString::new("id").unwrap();
        // SAFETY: qb 有效
        assert_eq!(
            unsafe { sz_orm_cabi::sz_orm_qb_order_by(qb, order_field.as_ptr(), 1) },
            sz_orm_cabi::SzOrmErrorCode::Ok.as_i32()
        );

        // SAFETY: qb 有效
        assert_eq!(
            unsafe { sz_orm_cabi::sz_orm_qb_limit(qb, 10) },
            sz_orm_cabi::SzOrmErrorCode::Ok.as_i32()
        );

        // SAFETY: qb 有效
        let result_ptr = unsafe { sz_orm_cabi::sz_orm_qb_build(qb) };
        assert!(!result_ptr.is_null(), "qb_build should return JSON");
        // SAFETY: result_ptr 有效
        let json = unsafe { std::ffi::CStr::from_ptr(result_ptr) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: result_ptr 配对释放
        unsafe { sz_orm_cabi::sz_orm_qb_result_free(result_ptr) };

        assert!(
            json.contains("users"),
            "SQL should contain table name: {json}"
        );
        assert!(
            json.contains("status"),
            "SQL should contain where field: {json}"
        );

        // SAFETY: qb 有效
        unsafe { sz_orm_cabi::sz_orm_qb_free(qb) };
    }

    #[test]
    fn test_java_qb_invalid_db_type() {
        // SAFETY: 无效 db_type 返回 null
        let qb = unsafe { sz_orm_cabi::sz_orm_qb_new(999) };
        assert!(qb.is_null(), "invalid db_type should return null");
    }

    #[test]
    fn test_java_qb_free_null_is_noop() {
        // SAFETY: null 指针 free 是安全空操作
        unsafe { sz_orm_cabi::sz_orm_qb_free(std::ptr::null_mut()) };
    }

    #[test]
    fn test_java_active_model_create_get_delete() {
        let pool = model_test_pool();

        let table = CString::new("jm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["AMUser",42]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        let r = unsafe {
            sz_orm_cabi::sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };
        assert_eq!(r.success, 1, "active_model create should succeed");

        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["AMUser"]"#).unwrap();
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
        // SAFETY: ptr 配对释放
        unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
        assert!(json.contains("AMUser"), "get should find AMUser: {json}");

        // SAFETY: pool/table/where 有效
        let r = unsafe {
            sz_orm_cabi::sz_orm_model_delete(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert_eq!(r.success, 1, "active_model delete should succeed");

        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_java_active_model_set_update() {
        let pool = model_test_pool();

        let table = CString::new("jm_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["SetUser",20]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        unsafe {
            sz_orm_cabi::sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr())
        };

        let set_json = CString::new(r#"{"age":21}"#).unwrap();
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["SetUser"]"#).unwrap();
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
        assert_eq!(r.success, 1, "active_model set should succeed");

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
        // SAFETY: ptr 配对释放
        unsafe { sz_orm_cabi::sz_orm_string_free(ptr) };
        assert!(
            json.contains("21"),
            "get after set should contain age 21: {json}"
        );

        // SAFETY: pool 有效
        unsafe { sz_orm_cabi::sz_orm_pool_free(pool) };
    }
}
