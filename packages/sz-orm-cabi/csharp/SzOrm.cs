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
using System.Runtime.InteropServices;
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
        private static extern uint sz_orm_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sz_orm_error_description(int code);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern void sz_orm_string_free(IntPtr s);

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
                throw new InvalidOperationException("SZ-ORM pool creation failed");
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
            IntPtr result = sz_orm_query(_handle, sql);
            if (result == IntPtr.Zero)
            {
                throw new InvalidOperationException("SZ-ORM query failed");
            }
            try
            {
                var row = Marshal.PtrToStructure<QueryJsonResult>(result);
                if (row.Success == 0 || row.Json == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        $"SZ-ORM query error code {row.ErrorCode}: {ErrorDescription(row.ErrorCode)}");
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
            var r = sz_orm_execute(_handle, sql);
            if (r.Success == 0)
            {
                throw new InvalidOperationException(
                    $"SZ-ORM execute error code {r.ErrorCode}: {ErrorDescription(r.ErrorCode)}");
            }
            return r.RowsAffected;
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
}