//! FFI 全局 runtime worker 数配置的进程级 E2E 测试（整改 P2，2026-10-27）。
//!
//! 独立测试二进制 = 独立进程：`sz_orm_cabi` 的全局 OnceLock runtime 在本进程
//! 内未被其他测试污染，因此可以确定性地验证「先登记 worker 数 → 首次建池
//! 时按登记值构建 runtime」的时序契约。
//!
//! 说明：tokio 的 worker 数本身需要 `tokio_unstable` 的 Runtime::metrics 才能
//! 从外部观测，故此处验证的是可观测行为链——登记成功（返回 0）、建池成功
//! （即 runtime 按 resolve_worker_count 构建成功）、查询真实执行；worker 数
//! 语义（钳制/默认值）由 lib.rs 内的 `test_resolve_worker_count_semantics`
//! 单元测试覆盖。

use std::ffi::CString;

#[test]
fn runtime_workers_registered_before_first_pool_takes_effect() {
    // 1. 建池前登记 worker 数（本进程 runtime 尚未初始化）
    let rc = unsafe { sz_orm_cabi::sz_orm_set_runtime_workers(3) };
    assert_eq!(rc, 0, "runtime 初始化前登记应返回 0");

    // 2. 首次建池触发 runtime 按 3 workers 构建（构建失败会 panic 暴露）
    let dsn = CString::new("sqlite::memory:").unwrap();
    let handle = unsafe { sz_orm_cabi::sz_orm_pool_new(dsn.as_ptr(), std::ptr::null()) };
    assert!(!handle.is_null(), "pool creation should succeed");

    // 3. 真实查询走 block_on(runtime)，证明 runtime 功能可用
    let sql = CString::new("SELECT 1 AS one").unwrap();
    // SAFETY: handle/sql 均为刚创建的有效句柄与 NUL 结尾字符串
    let result = unsafe { sz_orm_cabi::sz_orm_query(handle, sql.as_ptr()) };
    assert!(!result.is_null(), "query result should not be null");
    // SAFETY: result 来自 sz_orm_query，#[repr(C)] 布局契约允许跨 ABI 读取
    let r = unsafe { &*result };
    assert_eq!(r.success, 1, "query should succeed");

    // 4. runtime 已构建后再次登记：仍返回 0（登记语义），但不再生效
    let rc2 = unsafe { sz_orm_cabi::sz_orm_set_runtime_workers(8) };
    assert_eq!(rc2, 0, "初始化后登记不报错（文档化语义：不再生效）");

    // 5. 清理（配对释放）
    // SAFETY: result/handle 均为本次测试创建且未释放
    unsafe {
        sz_orm_cabi::sz_orm_query_result_free(result);
        sz_orm_cabi::sz_orm_pool_free(handle);
    }
}
