/* ============================================================================
 * sz-orm C ABI 契约（sz-orm-cabi）
 *
 * 字符串编码：所有 `const char *` 入参与返回的 C 字符串均为 UTF-8 且以 NUL
 *   结尾。调用方不得按系统 ANSI 代码页解析（C# 用 LPUTF8Str /
 *   PtrToStringUTF8，Java 用 UTF-8 解码），否则非 ASCII 数据会乱码。
 * 后端范围：当前仅支持 SQLite DSN（`sqlite::memory:`、`sqlite://...`）；
 *   `sz_orm_pool_new` 对其它 DSN 返回 nullptr。
 * 内存所有权：返回的 `char *` 由 Rust 分配，必须用对应的 `*_free` 函数释放。
 * ========================================================================== */
#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// Connection pool handle
using SzOrmPoolHandle = void*;

/// C ABI compatible connection pool configuration
struct PoolConfigC {
  uint32_t max_connections;
  uint32_t min_connections;
  uint64_t connect_timeout_ms;
  uint64_t idle_timeout_ms;
};

/// Query rows JSON result (allocated on the Rust side; freed via
/// `sz_orm_query_result_free`)
///
/// This struct is dereferenced by C/Go/Java/C# callers, so its field layout
/// must be stable across compilers: `#[repr(C)]` guarantees that.
struct QueryJsonResult {
  int32_t success;
  int32_t error_code;
  /// C string of the rows JSON (`[{"col":val},...]`); non-null on success
  char *json;
};

/// C ABI compatible query result
struct QueryResultC {
  int32_t success;
  int32_t error_code;
  uint64_t rows_affected;
  uint64_t last_insert_id;
};

/// C ABI compatible connection pool status snapshot
struct PoolStatsC {
  uint32_t idle;
  uint32_t active;
  uint32_t max;
  uint32_t min;
  uint32_t waiters;
};

/// C ABI compatible connection pool cumulative metrics
struct PoolMetricsC {
  uint64_t acquire_count;
  uint64_t acquire_failed_count;
  uint64_t release_count;
  uint64_t connection_created_count;
  uint64_t connection_closed_count;
};

/// Transaction handle
using SzOrmTransactionHandle = void*;

/// Query builder handle
using SzOrmQueryBuilderHandle = void*;

extern "C" {

/// 设置全局 FFI runtime 的 worker 数（仅在下一次 runtime 构建前生效）。
///
/// 应在任何 `sz_orm_pool_new` 之前调用；runtime 已构建后调用不报错但设置
/// 不再生效（OnceLock 已就绪）。0 按 1 处理；超过 64 截断到 64（`build_runtime`
/// 侧经 `resolve_worker_count` 统一钳制）。
///
/// # Safety
///
/// SAFETY: 本函数不解引用任何指针，可为任意线程任意时机调用。
///
/// 返回：恒为 0（登记语义；预留非 0 错误码扩展位）。
int32_t sz_orm_set_runtime_workers(uint32_t workers);

/// Create a connection pool (SQLite backend; real creation)
///
/// `dsn` supports SQLite URLs: `sqlite::memory:`, `sqlite://path/to/db.sqlite`.
/// All strings are NUL-terminated UTF-8. Non-SQLite DSNs are rejected
/// explicitly (returns null) instead of failing deep inside the driver.
///
/// # Safety
///
/// SAFETY: `dsn` must be a valid NUL-terminated C string.
SzOrmPoolHandle sz_orm_pool_new(const char *dsn, const PoolConfigC *config);

/// Free a connection pool
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new` and
/// not yet freed.
void sz_orm_pool_free(SzOrmPoolHandle handle);

/// Connection pool health check (acquire + ping for real liveness probing)
///
/// Returns 1 = healthy, 0 = unhealthy
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`.
int32_t sz_orm_ping(SzOrmPoolHandle handle);

/// Execute a query and return rows as JSON
///
/// On success returns a non-null `QueryJsonResult`; the caller frees it with
/// `sz_orm_query_result_free`. Returns `nullptr` on invalid arguments or
/// internal panic.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
QueryJsonResult *sz_orm_query(SzOrmPoolHandle handle, const char *sql);

/// Read a detailed error text for the most recent failure **on the calling
/// thread**, e.g. `query execution failed: table users has no column named x`.
///
/// Returns a NUL-terminated UTF-8 string owned by the caller (free it with
/// `sz_orm_string_free`), or nullptr when no failure has been recorded on this
/// thread. The text is a copy: the channel is not cleared by reading, use
/// `sz_orm_last_error_clear` for that.
///
/// # Safety
///
/// SAFETY: the returned pointer must be freed via `sz_orm_string_free`.
char *sz_orm_last_error_message();

/// Clear the detailed error text recorded for the calling thread.
void sz_orm_last_error_clear();

/// Free a query result
///
/// # Safety
///
/// SAFETY: `result` must be a pointer returned by `sz_orm_query` (or nullptr).
void sz_orm_query_result_free(QueryJsonResult *result);

/// Execute a write statement (INSERT/UPDATE/DELETE) and return the number of
/// affected rows
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
QueryResultC sz_orm_execute(SzOrmPoolHandle handle, const char *sql);

/// Execute a parameterized query and return rows as JSON.
///
/// `params_json` is a JSON array of bound parameters (`[1,"a",null]`). Typed
/// objects map to the corresponding `Value` variants so no precision is lost:
/// `{"$type":"decimal","value":"123.45600000000000000000000000000000000001"}`,
/// `{"$type":"datetime"|"date"|"time"|"uuid","value":"..."}`. A null
/// `params_json` means "no parameters" (same as `sz_orm_query`).
///
/// Parameters are always bound as `?` placeholders — never interpolated into
/// the SQL text, so injection payloads inside parameter values stay inert.
///
/// On failure returns a non-null result whose `error_code` is set; read the
/// database's own message via `sz_orm_last_error_message`.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` (and `params_json`, when non-null) must be valid NUL-terminated C
/// strings.
QueryJsonResult *sz_orm_query_params(SzOrmPoolHandle handle,
                                     const char *sql,
                                     const char *params_json);

/// Execute a parameterized write statement (INSERT/UPDATE/DELETE) and return
/// the number of affected rows.
///
/// Parameter encoding is identical to [`sz_orm_query_params`]; values are
/// bound as `?` placeholders and never concatenated into the SQL text.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` (and `params_json`, when non-null) must be valid NUL-terminated C
/// strings.
QueryResultC sz_orm_execute_params(SzOrmPoolHandle handle,
                                   const char *sql,
                                   const char *params_json);

/// Get the version number
uint32_t sz_orm_version();

/// Get a connection pool status snapshot
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`.
PoolStatsC sz_orm_pool_stats(SzOrmPoolHandle handle);

/// Get connection pool cumulative metrics
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`.
PoolMetricsC sz_orm_pool_metrics(SzOrmPoolHandle handle);

/// Batch-execute multiple SQL statements (semicolon-separated) and return the
/// cumulative number of affected rows
///
/// If any SQL statement fails, an error is returned immediately and
/// subsequent SQL is not executed.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
QueryResultC sz_orm_execute_batch(SzOrmPoolHandle handle, const char *sql);

/// Query a single row and return the first row as a JSON object (not an array)
///
/// On success returns a non-null `QueryJsonResult`; the caller frees it with
/// `sz_orm_query_result_free`. When there are no result rows, success=1 but
/// json is null.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
QueryJsonResult *sz_orm_query_one(SzOrmPoolHandle handle, const char *sql);

/// Check whether a table exists (SQLite backend)
///
/// Returns 1 = exists, 0 = does not exist, -1 = error
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table` must be a valid NUL-terminated C string.
int32_t sz_orm_table_exists(SzOrmPoolHandle handle, const char *table);

/// Count the number of rows in a table
///
/// Returns the row count; returns -1 on error
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table` must be a valid NUL-terminated C string.
int64_t sz_orm_count(SzOrmPoolHandle handle, const char *table);

/// Begin a transaction (holds an exclusive connection until
/// commit/rollback/free)
///
/// Returns a non-null handle on success, nullptr on failure
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`.
SzOrmTransactionHandle sz_orm_transaction_begin(SzOrmPoolHandle handle);

/// Execute SQL within a transaction
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`; `sql` must be a valid NUL-terminated C string.
QueryResultC sz_orm_transaction_execute(SzOrmTransactionHandle tx_handle, const char *sql);

/// Commit a transaction
///
/// Returns 1 = success, 0 = failure
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`.
int32_t sz_orm_transaction_commit(SzOrmTransactionHandle tx_handle);

/// Roll back a transaction
///
/// Returns 1 = success, 0 = failure
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`.
int32_t sz_orm_transaction_rollback(SzOrmTransactionHandle tx_handle);

/// Free a transaction handle (automatically rolls back if the transaction is
/// still active)
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin` and not yet freed.
void sz_orm_transaction_free(SzOrmTransactionHandle tx_handle);

/// Get the error code description string (the caller must free it with
/// `sz_orm_string_free`)
///
/// # Safety
///
/// SAFETY: the returned pointer must be freed via `sz_orm_string_free`.
char *sz_orm_error_description(int32_t code);

/// Free the string returned by `sz_orm_error_description`
///
/// # Safety
///
/// SAFETY: `s` must be a pointer returned by `sz_orm_error_description`
/// (or nullptr).
void sz_orm_string_free(char *s);

/// Insert a row on a pool (parameterized, REQ-BND-007).
///
/// `fields_json` is a field-name array `["name","age"]`; `values_json` is the
/// corresponding values array `["Alice",30]`.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table`/`fields_json`/`values_json` must be valid NUL-terminated C strings.
QueryResultC sz_orm_model_insert(SzOrmPoolHandle handle,
                                 const char *table,
                                 const char *fields_json,
                                 const char *values_json);

/// Update rows on a pool (parameterized, REQ-BND-007).
///
/// `set_json` is an object `{"name":"Alice"}`; `where_clause` is a
/// parameterized condition `id = ?`; `where_params_json` is a parameter
/// array `[1]`.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table`/`set_json`/`where_clause`/`where_params_json` must be valid
/// NUL-terminated C strings.
QueryResultC sz_orm_model_update(SzOrmPoolHandle handle,
                                 const char *table,
                                 const char *set_json,
                                 const char *where_clause,
                                 const char *where_params_json);

/// Delete rows on a pool (parameterized, REQ-BND-007).
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table`/`where_clause`/`where_params_json` must be valid NUL-terminated C
/// strings.
QueryResultC sz_orm_model_delete(SzOrmPoolHandle handle,
                                 const char *table,
                                 const char *where_clause,
                                 const char *where_params_json);

/// Query rows on a pool and return a JSON row-array string (parameterized,
/// REQ-BND-007).
///
/// On success returns a non-null `*mut c_char` (the caller frees it with
/// `sz_orm_string_free`); null indicates failure. Returns `[]` when no rows
/// match.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table`/`where_clause`/`where_params_json` must be valid NUL-terminated C
/// strings.
char *sz_orm_model_find(SzOrmPoolHandle handle,
                        const char *table,
                        const char *where_clause,
                        const char *where_params_json);

/// Insert a row within a transaction (parameterized, REQ-BND-014).
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`; `table`/`fields_json`/`values_json` must be
/// valid NUL-terminated C strings.
QueryResultC sz_orm_model_insert_tx(SzOrmTransactionHandle tx_handle,
                                    const char *table,
                                    const char *fields_json,
                                    const char *values_json);

/// Update rows within a transaction (parameterized, REQ-BND-014).
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`;
/// `table`/`set_json`/`where_clause`/`where_params_json` must be valid
/// NUL-terminated C strings.
QueryResultC sz_orm_model_update_tx(SzOrmTransactionHandle tx_handle,
                                    const char *table,
                                    const char *set_json,
                                    const char *where_clause,
                                    const char *where_params_json);

/// Delete rows within a transaction (parameterized, REQ-BND-014).
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`; `table`/`where_clause`/`where_params_json` must
/// be valid NUL-terminated C strings.
QueryResultC sz_orm_model_delete_tx(SzOrmTransactionHandle tx_handle,
                                    const char *table,
                                    const char *where_clause,
                                    const char *where_params_json);

/// Query rows within a transaction and return a JSON row-array string
/// (parameterized, REQ-BND-014).
///
/// On success returns a non-null `*mut c_char` (the caller frees it with
/// `sz_orm_string_free`); null indicates failure.
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`; `table`/`where_clause`/`where_params_json` must
/// be valid NUL-terminated C strings.
char *sz_orm_model_find_tx(SzOrmTransactionHandle tx_handle,
                           const char *table,
                           const char *where_clause,
                           const char *where_params_json);

/// 创建 QueryBuilder 句柄。
///
/// `db_type` 值：0=MySQL, 1=PostgreSQL, 2=SQLite, 6=Oracle, 8=SqlServer 等。
///
/// # Safety
///
/// SAFETY: 返回的句柄必须通过 `sz_orm_qb_free` 释放。
SzOrmQueryBuilderHandle sz_orm_qb_new(uint32_t db_type);

/// 设置表名。
///
/// # Safety
///
/// SAFETY: `qb` 必须是 `sz_orm_qb_new` 返回的有效句柄；`table` 必须是有效的 NUL 终止 C 字符串。
int32_t sz_orm_qb_table(SzOrmQueryBuilderHandle qb,
                        const char *table);

/// 添加 WHERE eq 条件（参数化）。
///
/// `value_json` 为原始 JSON 值字符串，如 `42`（整数）、`"active"`（字符串）、`true`（布尔）。
///
/// # Safety
///
/// SAFETY: `qb` 必须有效；`field` 和 `value_json` 必须是有效的 NUL 终止 C 字符串。
int32_t sz_orm_qb_where_eq(SzOrmQueryBuilderHandle qb,
                           const char *field,
                           const char *value_json);

/// 添加 ORDER BY。
///
/// `desc` = 0 表示升序，非 0 表示降序。
///
/// # Safety
///
/// SAFETY: `qb` 必须有效；`field` 必须是有效的 NUL 终止 C 字符串。
int32_t sz_orm_qb_order_by(SzOrmQueryBuilderHandle qb, const char *field, int32_t desc);

/// 设置 LIMIT。
///
/// # Safety
///
/// SAFETY: `qb` 必须有效。
int32_t sz_orm_qb_limit(SzOrmQueryBuilderHandle qb, uint64_t limit);

/// 构建 SQL + 参数 JSON。
///
/// 返回 JSON 字符串 `{"sql": "...", "params": [...]}`，调用方需用 `sz_orm_qb_result_free` 释放。
/// 失败返回 null。
///
/// # Safety
///
/// SAFETY: `qb` 必须有效；返回的字符串必须通过 `sz_orm_qb_result_free` 释放。
char *sz_orm_qb_build(SzOrmQueryBuilderHandle qb);

/// 释放 QueryBuilder 句柄。
///
/// # Safety
///
/// SAFETY: `qb` 必须是 `sz_orm_qb_new` 返回且尚未释放的句柄。
void sz_orm_qb_free(SzOrmQueryBuilderHandle qb);

/// 释放 `sz_orm_qb_build` 返回的字符串。
///
/// # Safety
///
/// SAFETY: `result` 必须是 `sz_orm_qb_build` 返回且尚未释放的指针。
void sz_orm_qb_result_free(char *result);

} // extern "C"
