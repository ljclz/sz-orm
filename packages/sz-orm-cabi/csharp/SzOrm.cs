// SZ-ORM C# 绑定：SzOrmPool（P/Invoke 封装 sz-orm-cabi）
//
// ABI 契约：packages/sz-orm-cabi/include/sz_orm_cabi.h（cbindgen 生成）
// 内存所有权：Rust 侧分配/释放；C# 侧只持有句柄，必须配对释放。
//
// 用法：
//   var pool = SzOrmPool.Create("sqlite::memory:", 4);
//   pool.Execute("CREATE TABLE ...");
//   string json = pool.Query("SELECT ...");
//   pool.Dispose();   // 释放连接池

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Threading;

namespace SzOrm
{
    /// <summary>C ABI 兼容连接池配置（对应 PoolConfigC）</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct PoolConfig
    {
        public uint MaxConnections;
        public uint MinConnections;
        public ulong ConnectTimeoutMs;
        public ulong IdleTimeoutMs;

        public static PoolConfig Default() => new PoolConfig
        {
            MaxConnections = 10,
            MinConnections = 1,
            ConnectTimeoutMs = 5000,
            IdleTimeoutMs = 300000,
        };
    }

    /// <summary>查询结果（对应 QueryJsonResult，#[repr(C)]）</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct QueryJsonResult
    {
        public int Success;
        public int ErrorCode;
        public IntPtr Json; // NUL 结尾的 JSON 字符串（Rust 侧分配）
    }

    /// <summary>写语句结果（对应 QueryResultC）</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct QueryResult
    {
        public int Success;
        public int ErrorCode;
        public ulong RowsAffected;
        public ulong LastInsertId;
    }

    /// <summary>连接池状态快照（对应 PoolStatsC，#[repr(C)]；2026-10-28 P3-2）</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct PoolStatsSnapshot
    {
        public uint Idle;
        public uint Active;
        public uint Max;
        public uint Min;
        public uint Waiters;
    }

    /// <summary>连接池累计指标（对应 PoolMetricsC，#[repr(C)]；2026-10-28 P3-2）</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct PoolMetricsSnapshot
    {
        public ulong AcquireCount;
        public ulong AcquireFailedCount;
        public ulong ReleaseCount;
        public ulong ConnectionCreatedCount;
        public ulong ConnectionClosedCount;
    }

    /// <summary>SZ-ORM 异常基类：携带 C ABI 错误码（2026-10-28 架构评测 P3-2）。
    /// 取代此前统一抛出的 InvalidOperationException——调用方可按类别捕获：</summary>
    public class SzOrmException : Exception
    {
        /// <summary>C ABI 错误码（SzOrmErrorCode）</summary>
        public int ErrorCode { get; }

        public SzOrmException(int errorCode, string message) : base(message)
        {
            ErrorCode = errorCode;
        }
    }

    /// <summary>SQL 执行失败（错误码 1 NotFound / 3 QueryFailed）</summary>
    public sealed class SzOrmQueryException : SzOrmException
    {
        public SzOrmQueryException(int errorCode, string message) : base(errorCode, message) { }
    }

    /// <summary>连接/池类失败（错误码 2 ConnectionFailed / 4 PoolExhausted / 8 RuntimeNotInitialized）</summary>
    public sealed class SzOrmConnectionException : SzOrmException
    {
        public SzOrmConnectionException(int errorCode, string message) : base(errorCode, message) { }
    }

    /// <summary>事务失败（错误码 5 TransactionAborted）</summary>
    public sealed class SzOrmTransactionException : SzOrmException
    {
        public SzOrmTransactionException(int errorCode, string message) : base(errorCode, message) { }
    }

    /// <summary>错误码 → 强类型异常的映射（与 Rust 侧 SzOrmErrorCode 数值一致）</summary>
    internal static class SzOrmError
    {
        internal const int NotFound = 1;
        internal const int ConnectionFailed = 2;
        internal const int QueryFailed = 3;
        internal const int PoolExhausted = 4;
        internal const int TransactionAborted = 5;
        internal const int InvalidArgument = 7;

        internal static SzOrmException FromCode(int code, string detail) => code switch
        {
            NotFound or QueryFailed => new SzOrmQueryException(code, detail),
            ConnectionFailed or PoolExhausted or 8 => new SzOrmConnectionException(code, detail),
            TransactionAborted => new SzOrmTransactionException(code, detail),
            _ => new SzOrmException(code, detail),
        };
    }

    /// <summary>sz-orm-cabi 的 C# 封装：连接池 + 查询 + 执行 + 事务</summary>
    public sealed class SzOrmPool : IDisposable
    {
        private IntPtr _handle;
        private volatile bool _disposed;

        private const string Lib = "sz_orm_cabi";

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sz_orm_pool_new(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string dsn,
            ref PoolConfig config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern void sz_orm_pool_free(IntPtr handle);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern int sz_orm_ping(IntPtr handle);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sz_orm_query(IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string sql);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern void sz_orm_query_result_free(IntPtr result);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern QueryResult sz_orm_execute(IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string sql);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sz_orm_query_params(IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string sql,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string paramsJson);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern QueryResult sz_orm_execute_params(IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string sql,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string paramsJson);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sz_orm_last_error_message();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern void sz_orm_last_error_clear();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint sz_orm_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sz_orm_error_description(int code);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern void sz_orm_string_free(IntPtr s);

        // ===== 2026-10-28 P3-2 扩展：事务 / 批量 / 统计 / runtime workers =====

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sz_orm_transaction_begin(IntPtr handle);

        // SzOrmTransaction 是同程序集的兄弟类型，联编必须至少是 internal
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern QueryResult sz_orm_transaction_execute(IntPtr txHandle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string sql);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int sz_orm_transaction_commit(IntPtr txHandle);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int sz_orm_transaction_rollback(IntPtr txHandle);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void sz_orm_transaction_free(IntPtr txHandle);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern QueryResult sz_orm_execute_batch(IntPtr handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string sql);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern PoolStatsSnapshot sz_orm_pool_stats(IntPtr handle);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern PoolMetricsSnapshot sz_orm_pool_metrics(IntPtr handle);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern int sz_orm_set_runtime_workers(uint workers);

        /// <summary>设置共享 tokio runtime 的 worker 数（应在建池前调用；默认 2）</summary>
        public static int SetRuntimeWorkers(uint workers) => sz_orm_set_runtime_workers(workers);

        private SzOrmPool(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>创建 SQLite 连接池（dsn 如 "sqlite::memory:" 或 "sqlite://path/to.db"）</summary>
        public static SzOrmPool Create(string dsn, int maxConnections = 10)
        {
            var config = PoolConfig.Default();
            config.MaxConnections = (uint)Math.Max(1, maxConnections);
            IntPtr handle = sz_orm_pool_new(dsn, ref config);
            if (handle == IntPtr.Zero)
            {
                throw new SzOrmConnectionException(
                    SzOrmError.ConnectionFailed, "SZ-ORM pool creation failed");
            }
            return new SzOrmPool(handle);
        }

        /// <summary>健康检查（真实 acquire + ping）</summary>
        public bool IsHealthy()
        {
            EnsureNotDisposed();
            return sz_orm_ping(_handle) == 1;
        }

        /// <summary>执行查询，返回 JSON 行数组字符串</summary>
        public string Query(string sql)
        {
            EnsureNotDisposed();
            return QueryCore(sz_orm_query(_handle, sql));
        }

        /// <summary>
        /// 执行参数化查询，返回 JSON 行数组字符串。
        /// 参数一律以 `?` 占位符绑定，绝不拼进 SQL 文本，注入串保持惰性。
        /// decimal/DateTime/DBNull 会编码为保真类型（不走 double 往返）。
        /// </summary>
        public string QueryParams(string sql, params object?[] parameters)
        {
            EnsureNotDisposed();
            string paramsJson = SzOrmParam.Encode(parameters);
            return QueryCore(sz_orm_query_params(_handle, sql, paramsJson));
        }

        private string QueryCore(IntPtr result)
        {
            if (result == IntPtr.Zero)
            {
                throw new SzOrmException(-1, "SZ-ORM query failed");
            }
            try
            {
                var row = Marshal.PtrToStructure<QueryJsonResult>(result);
                if (row.Success == 0 || row.Json == IntPtr.Zero)
                {
                    throw SzOrmError.FromCode(row.ErrorCode,
                        $"SZ-ORM query error code {row.ErrorCode}: {SzOrmPool.DescribeFailure(row.ErrorCode)}");
                }
                return Marshal.PtrToStringUTF8(row.Json) ?? "[]";
            }
            finally
            {
                sz_orm_query_result_free(result);
            }
        }

        /// <summary>执行写语句，返回影响行数</summary>
        public ulong Execute(string sql)
        {
            EnsureNotDisposed();
            return ExecuteCore(sz_orm_execute(_handle, sql));
        }

        /// <summary>执行参数化写语句，返回影响行数（参数绑定规则同 <see cref="QueryParams"/>）</summary>
        public ulong ExecuteParams(string sql, params object?[] parameters)
        {
            EnsureNotDisposed();
            return ExecuteCore(sz_orm_execute_params(_handle, sql, SzOrmParam.Encode(parameters)));
        }

        internal static ulong ExecuteCore(QueryResult r)
        {
            if (r.Success == 0)
            {
                throw SzOrmError.FromCode(r.ErrorCode,
                    $"SZ-ORM execute error code {r.ErrorCode}: {DescribeFailure(r.ErrorCode)}");
            }
            return r.RowsAffected;
        }

        /// <summary>执行批量 SQL 脚本（分号分隔的多语句），返回总影响行数</summary>
        public ulong ExecuteBatch(string sql)
        {
            EnsureNotDisposed();
            return ExecuteCore(sz_orm_execute_batch(_handle, sql));
        }

        /// <summary>开启事务（独占连接；Dispose 未提交时自动回滚——Rust 侧 transaction_free 语义）。
        /// 注意：ABI 仅导出事务内 execute（写），事务内查询当前不支持。</summary>
        public SzOrmTransaction BeginTransaction()
        {
            EnsureNotDisposed();
            IntPtr tx = sz_orm_transaction_begin(_handle);
            if (tx == IntPtr.Zero)
            {
                throw new SzOrmConnectionException(SzOrmError.ConnectionFailed,
                    "SZ-ORM transaction begin failed: " + DescribeFailure(SzOrmError.ConnectionFailed));
            }
            return new SzOrmTransaction(tx, this);
        }

        /// <summary>连接池状态快照（idle/active/max/min/waiters）</summary>
        public PoolStatsSnapshot GetStats()
        {
            EnsureNotDisposed();
            return sz_orm_pool_stats(_handle);
        }

        /// <summary>连接池累计指标（acquire/release/created/closed 计数）</summary>
        public PoolMetricsSnapshot GetMetrics()
        {
            EnsureNotDisposed();
            return sz_orm_pool_metrics(_handle);
        }

        /// <summary>
        /// 读取最近一次失败（线程内）的详细错误文本，无失败则返回 null。
        /// 错误码只表达类别，这里返回数据库自身的消息（约束名、列名等）。
        /// </summary>
        public static string? LastErrorMessage()
        {
            IntPtr s = sz_orm_last_error_message();
            if (s == IntPtr.Zero)
            {
                return null;
            }
            try
            {
                return Marshal.PtrToStringUTF8(s);
            }
            finally
            {
                sz_orm_string_free(s);
            }
        }

        /// <summary>清空线程内的错误文本</summary>
        public static void ClearLastErrorMessage() => sz_orm_last_error_clear();

        internal static string DescribeFailure(int code)
        {
            string? detail = LastErrorMessage();
            return detail is null ? ErrorDescription(code) : detail;
        }

        /// <summary>错误码描述</summary>
        public static string ErrorDescription(int code)
        {
            IntPtr s = sz_orm_error_description(code);
            if (s == IntPtr.Zero)
            {
                return "(no description)";
            }
            try
            {
                return Marshal.PtrToStringUTF8(s) ?? "";
            }
            finally
            {
                sz_orm_string_free(s);
            }
        }

        /// <summary>绑定版本号</summary>
        public static uint BindVersion() => sz_orm_version();

        public void Dispose()
        {
            // 用 Interlocked.Exchange 抢走句柄：Dispose 与终结器并发时只有一方
            // 真正调用 sz_orm_pool_free，避免双重释放。
            IntPtr handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            _disposed = true;
            if (handle != IntPtr.Zero)
            {
                sz_orm_pool_free(handle);
            }
            GC.SuppressFinalize(this);
        }

        ~SzOrmPool()
        {
            IntPtr handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                sz_orm_pool_free(handle);
            }
        }

        private void EnsureNotDisposed()
        {
            if (_disposed || _handle == IntPtr.Zero)
            {
                throw new ObjectDisposedException(nameof(SzOrmPool));
            }
        }
    }

    /// <summary>
    /// SZ-ORM 事务（独占连接；2026-10-28 P3-2 新增 C# 封装）。
    ///
    /// 所有权：句柄由 Rust 侧分配；Dispose 时若尚未 commit/rollback，
    /// Rust 侧 sz_orm_transaction_free 会先执行 ROLLBACK 再释放——
    /// 忘记显式结束事务不会泄漏连接，但应显式 Commit/Rollback 以明确意图。
    /// </summary>
    public sealed class SzOrmTransaction : IDisposable
    {
        private IntPtr _handle;
        private readonly SzOrmPool _owner;

        internal SzOrmTransaction(IntPtr handle, SzOrmPool owner)
        {
            _handle = handle;
            _owner = owner;
        }

        /// <summary>事务内执行写语句（ABI 仅导出事务内 execute，查询不支持），返回影响行数</summary>
        public ulong Execute(string sql)
        {
            EnsureActive();
            return SzOrmPool.ExecuteCore(SzOrmPool.sz_orm_transaction_execute(_handle, sql));
        }

        /// <summary>提交事务；成功后句柄失效</summary>
        public void Commit()
        {
            EnsureActive();
            int rc = SzOrmPool.sz_orm_transaction_commit(_handle);
            // ABI 语义：1 = 成功，0 = 失败
            if (rc != 1)
            {
                throw new SzOrmTransactionException(SzOrmError.TransactionAborted,
                    $"SZ-ORM transaction commit failed (rc={rc}): " + SzOrmPool.DescribeFailure(SzOrmError.TransactionAborted));
            }
            MarkDone();
        }

        /// <summary>回滚事务；成功后句柄失效</summary>
        public void Rollback()
        {
            EnsureActive();
            int rc = SzOrmPool.sz_orm_transaction_rollback(_handle);
            // ABI 语义：1 = 成功，0 = 失败
            if (rc != 1)
            {
                throw new SzOrmTransactionException(SzOrmError.TransactionAborted,
                    $"SZ-ORM transaction rollback failed (rc={rc}): " + SzOrmPool.DescribeFailure(SzOrmError.TransactionAborted));
            }
            MarkDone();
        }

        public void Dispose()
        {
            // Interlocked.Exchange 防双重释放（与 SzOrmPool.Dispose 同模式）；
            // 未 commit/rollback 时 Rust 侧 free 隐式 ROLLBACK
            IntPtr handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                SzOrmPool.sz_orm_transaction_free(handle);
            }
            GC.SuppressFinalize(this);
        }

        ~SzOrmTransaction()
        {
            IntPtr handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                SzOrmPool.sz_orm_transaction_free(handle);
            }
        }

        private void MarkDone()
        {
            IntPtr handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                SzOrmPool.sz_orm_transaction_free(handle);
            }
        }

        private void EnsureActive()
        {
            if (_handle == IntPtr.Zero)
            {
                throw new ObjectDisposedException(nameof(SzOrmTransaction));
            }
        }
    }

    /// <summary>
    /// 参数编码器：把 CLR 值编码成 C ABI 的 params_json 数组。
    ///
    /// 保真原则：
    /// - DBNull.Value / null → JSON null（绑定 SQL NULL）
    /// - decimal → {"$type":"decimal","value":"..."}，不经 double 往返
    /// - DateTime / DateTimeOffset → {"$type":"datetime","value":"<ISO 8601>"}
    /// - TimeSpan → {"$type":"time",...}；Guid → {"$type":"uuid",...}
    /// - 超出 decimal 范围的高精度数值用 <see cref="DecimalString"/> 传字符串
    /// </summary>
    public static class SzOrmParam
    {
        /// <summary>把参数数组编码为 params_json 字符串</summary>
        public static string Encode(IReadOnlyList<object?> parameters)
        {
            var sb = new StringBuilder("[");
            for (int i = 0; i < parameters.Count; i++)
            {
                if (i > 0)
                {
                    sb.Append(',');
                }
                AppendValue(sb, parameters[i]);
            }
            sb.Append(']');
            return sb.ToString();
        }

        /// <summary>任意精度 DECIMAL（超过 .NET decimal 的 28~29 位有效数字时使用）</summary>
        public static Dictionary<string, string> DecimalString(string value) => new Dictionary<string, string>
        {
            ["$type"] = "decimal",
            ["value"] = value,
        };

        private static void AppendValue(StringBuilder sb, object? value)
        {
            switch (value)
            {
                case null:
                case DBNull:
                    sb.Append("null");
                    return;
                case bool b:
                    sb.Append(b ? "true" : "false");
                    return;
                case string s:
                    AppendJsonString(sb, s);
                    return;
                case decimal d:
                    AppendTyped(sb, "decimal", d.ToString(CultureInfo.InvariantCulture));
                    return;
                case DateTime dt:
                    AppendTyped(sb, "datetime", dt.ToString("o", CultureInfo.InvariantCulture));
                    return;
                case DateTimeOffset dto:
                    AppendTyped(sb, "datetime", dto.ToString("o", CultureInfo.InvariantCulture));
                    return;
                case TimeSpan ts:
                    AppendTyped(sb, "time", ts.ToString("c", CultureInfo.InvariantCulture));
                    return;
                case Guid g:
                    AppendTyped(sb, "uuid", g.ToString("D"));
                    return;
                case sbyte or short or int or long or byte or ushort or uint or ulong:
                    sb.Append(Convert.ToString(value, CultureInfo.InvariantCulture));
                    return;
                case float f:
                    sb.Append(f.ToString("R", CultureInfo.InvariantCulture));
                    return;
                case double db:
                    sb.Append(db.ToString("R", CultureInfo.InvariantCulture));
                    return;
                case IReadOnlyDictionary<string, string> typed:
                    AppendStringMap(sb, typed);
                    return;
                default:
                    throw new ArgumentException(
                        $"unsupported parameter type: {value.GetType().FullName}", nameof(value));
            }
        }

        private static void AppendTyped(StringBuilder sb, string type, string payload)
        {
            sb.Append("{\"$type\":");
            AppendJsonString(sb, type);
            sb.Append(",\"value\":");
            AppendJsonString(sb, payload);
            sb.Append('}');
        }

        private static void AppendStringMap(StringBuilder sb, IReadOnlyDictionary<string, string> map)
        {
            sb.Append('{');
            bool first = true;
            foreach (var kv in map)
            {
                if (!first)
                {
                    sb.Append(',');
                }
                first = false;
                AppendJsonString(sb, kv.Key);
                sb.Append(':');
                AppendJsonString(sb, kv.Value);
            }
            sb.Append('}');
        }

        private static void AppendJsonString(StringBuilder sb, string s)
        {
            sb.Append('"');
            foreach (char c in s)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < ' ')
                        {
                            sb.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                        }
                        else
                        {
                            sb.Append(c);
                        }
                        break;
                }
            }
            sb.Append('"');
        }
    }

    /// <summary>
    /// 行值解码器：把 sz-orm 的行 JSON（值形如 {"I64":1} / {"Decimal":"..."}）
    /// 还原为 .NET 类型。空值（{"Null":null}）一律返回 null。
    /// </summary>
    public static class SzOrmValue
    {
        public static bool IsNull(JsonElement value) =>
            value.ValueKind == JsonValueKind.Null
            // serde 单元变体 Value::Null 序列化为字符串 "Null"
            || (value.ValueKind == JsonValueKind.String && value.GetString() == "Null")
            || (value.ValueKind == JsonValueKind.Object && value.TryGetProperty("Null", out _));

        /// <summary>取 DECIMAL 精确字符串（不做浮点转换）；空值返回 null</summary>
        public static string? GetDecimalString(JsonElement value)
        {
            if (IsNull(value))
            {
                return null;
            }
            return TryUnwrap(value, "Decimal") ?? TryUnwrap(value, "String") ?? UnwrapScalar(value);
        }

        /// <summary>取 DECIMAL 为 .NET decimal（超过 28 位有效数字时须改用 <see cref="GetDecimalString"/>）</summary>
        public static decimal? GetDecimal(JsonElement value)
        {
            string? raw = GetDecimalString(value);
            return raw is null ? null : decimal.Parse(raw, NumberStyles.Number, CultureInfo.InvariantCulture);
        }

        /// <summary>取 DATETIME/DATE/TIME 为 DateTime；空值返回 null</summary>
        public static DateTime? GetDateTime(JsonElement value)
        {
            // TEXT 列回读是 String 变体，也要能解析
            string? raw = IsNull(value)
                ? null
                : TryUnwrap(value, "DateTime")
                    ?? TryUnwrap(value, "Date")
                    ?? TryUnwrap(value, "Time")
                    ?? TryUnwrap(value, "String");
            if (raw is null)
            {
                return null;
            }
            return System.DateTime.Parse(raw, CultureInfo.InvariantCulture,
                DateTimeStyles.RoundtripKind);
        }

        /// <summary>取字符串；空值返回 null</summary>
        public static string? GetString(JsonElement value)
        {
            if (IsNull(value))
            {
                return null;
            }
            return TryUnwrap(value, "String") ?? UnwrapScalar(value);
        }

        private static string? TryUnwrap(JsonElement value, string variant) =>
            value.ValueKind == JsonValueKind.Object && value.TryGetProperty(variant, out var inner)
                ? inner.GetString()
                : null;

        private static string? UnwrapScalar(JsonElement value) => value.ValueKind switch
        {
            JsonValueKind.String => value.GetString(),
            JsonValueKind.Number => value.GetRawText(),
            JsonValueKind.True => "true",
            JsonValueKind.False => "false",
            _ => null,
        };
    }
}