# v9.9.0 可靠性基线报告（修复后）

> 采集日期：2026-10-10
> 版本：9.9.0（从 9.8.0 升级）
> 采集环境：Windows 11，Rust 1.98.0，CARGO_TARGET_DIR=/f/cargo-target

## 0. 审计修复记录

初次交付存在 3 类问题，已全部修复并验证：

| 问题类型 | 数量 | 修复方案 | 验证结果 |
|---------|------|---------|---------|
| 幻影交付 | 3 | `fetch_all` 在 `zero-copy` feature 下委托 `fetch_all_zero_copy` | queryable.rs:482 生产调用点 |
| 弱测试 | 2 | 改为压力场景下验证计数器实际变化 | e2e 33 passed |
| 接线缺口 | 2 | 新增 `test_rebuild_triggered_via_acquire_e2e` | e2e 33 passed |

## 1. 版本变更

| 项目 | 旧版本 | 新版本 |
|------|--------|--------|
| workspace.package.version | 9.8.0 | 9.9.0 |

## 2. 生产调用点证据（幻影交付检查 — 修复后）

### 零拷贝路径（修复后已有生产调用点）

| 类型/方法 | 定义位置 | 生产调用点 | 证据 |
|----------|---------|-----------|------|
| `fetch_all_zero_copy` | queryable.rs:514 | queryable.rs:482 | `fetch_all` 内 `#[cfg(feature = "zero-copy")]` 分支调用 |
| `BorrowedRow` | queryable.rs:166 | queryable.rs:537 | `fetch_all_zero_copy` 内 `BorrowedRow::new(row)` |
| `BorrowedRow::inner` | queryable.rs:192 | queryable.rs:210 | `FromRowBorrowed` blanket impl 内 `row.inner()` |
| `FromRowBorrowed` | queryable.rs:198 | queryable.rs:520 | `fetch_all_zero_copy` trait bound |
| `FromRowBorrowed` blanket impl | queryable.rs:207 | queryable.rs:210 | `impl<T: FromQueryResult> FromRowBorrowed for T` |
| `ZeroCopyDecodeConfig` | queryable.rs:227 | queryable.rs:482 | `fetch_all` 内 `ZeroCopyDecodeConfig::default()` |

### 连接保活自愈路径

| 类型/方法 | 定义位置 | 生产调用点 | 证据 |
|----------|---------|-----------|------|
| `trigger_rebuild` | pool.rs:2838 | pool.rs:2114 | acquire 路径 ping 失败后 `tokio::spawn` 异步调用 |
| `RebuildBackoff` | auto_healer mod | pool.rs:2847 | `trigger_rebuild` 内使用 |
| `RebuildStormGuard` | auto_healer mod | pool.rs:2855 | `trigger_rebuild` 内 `allow()` 检查 |

### 降级与背压路径

| 类型/方法 | 定义位置 | 生产调用点 | 证据 |
|----------|---------|-----------|------|
| `PoolDegradationStrategy` | pool.rs:544 | pool.rs:2198/2208/2219 | acquire 路径三分支匹配 |
| `DegradationConfig` | pool.rs:556 | pool.rs:700 | PoolConfig 字段 |
| `DropReason` | pool.rs:574 | pool.rs:2062/2101 | acquire 路径事件 emit |
| `DroppedCountWindow` | pool.rs:40 | pool.rs:2065/2104 | acquire 路径 `record_drop()` |
| `PressureMetrics` | pool.rs:855 | pool.rs:2462 | `status()` 方法内调用 |
| `with_degradation` | pool.rs:814 | 公开 builder API | 用户配置入口 |

### 新增事件 → emit 调用点

| 事件变体 | emit 调用点 |
|---------|------------|
| `ConnectionDropped` | pool.rs:2060, pool.rs:2099 |
| `RebuildSucceeded` | pool.rs:2929 |
| `RebuildFailed` | pool.rs:2917 |
| `RebuildStormSuppressed` | pool.rs:2855 |
| `BackpressureWarning` | pool.rs:2212 |
| `PoolExhausted` | pool.rs:2201 |

**结论：无幻影交付。所有新增类型/方法/事件均有生产调用点。**

## 3. 端到端测试（修复后）

### e2e 测试清单（13 个 v9.9.0 新增）

| # | 测试名 | 验证内容 | 真实行为 |
|---|--------|---------|---------|
| 1 | test_connection_dropped_event_e2e | ConnectionDropped 事件 + dropped_count | Mock is_connected=false → acquire 触发断开检测 |
| 2 | test_rebuild_backoff_e2e | RebuildFailed×3 + RebuildSucceeded | Mock create 失败 3 次后成功 |
| 3 | test_rebuild_storm_suppressed_e2e | RebuildStormSuppressed | 3 次 trigger_rebuild，第 3 次被风暴保护抑制 |
| 4 | **test_rebuild_triggered_via_acquire_e2e** | **acquire 路径间接触发 rebuild** | **test_before_acquire=true + ping 失败 → trigger_rebuild** |
| 5 | test_degradation_fast_fail_e2e | FastFail 立即返回 + rejected_count | max_size=1 池满后 FastFail < 100ms |
| 6 | test_degradation_default_unchanged_e2e | 默认行为等待超时 | 默认配置等待 ~100ms |
| 7 | test_backpressure_warning_e2e | BackpressureWarning 事件 | 并发 acquire 触发背压阈值 |
| 8 | **test_pressure_metrics_e2e** | **压力场景下 rejected_count/exhausted_count 变化** | **FastFail 后 rejected_count >= 1** |
| 9 | test_dropped_count_sliding_window_e2e | 滑动窗口过期 | 5 次 record_drop → 等 1.2s → 3 次 → count=3 |
| 10 | **test_pool_status_enhanced_fields_e2e** | **压力场景下 status.rejected_count 变化** | **FastFail 后 status.rejected_count >= 1** |
| 11 | test_zero_copy_decode_correctness_e2e | 零拷贝结果与 HashMap 一致 | 真实 SQLite 10000 行 |
| 12 | test_zero_copy_fallback_on_small_resultset_e2e | 小结果集回退 | 真实 SQLite 100 行 < min_rows=1000 |
| 13 | test_zero_copy_default_disabled_e2e | 默认禁用行为 | 真实 SQLite + default config |

**加粗项为修复后新增/重写的测试。**

### 接线验证

| 接线路径 | 验证测试 | 验证方式 |
|---------|---------|---------|
| acquire → is_connected 检测 → ConnectionDropped | test_connection_dropped_event_e2e | Mock is_connected=false |
| acquire → ping 失败 → ConnectionDropped → trigger_rebuild | **test_rebuild_triggered_via_acquire_e2e** | **test_before_acquire=true + ping_ok=false + 等待 31s** |
| acquire → 池耗尽 → FastFail → PoolExhausted | test_degradation_fast_fail_e2e | max_size=1 池满 |
| acquire → 池耗尽 → BackpressureNotify → BackpressureWarning | test_backpressure_warning_e2e | 并发 acquire |
| fetch_all → zero-copy → fetch_all_zero_copy | test_zero_copy_decode_correctness_e2e | 真实 SQLite 10000 行 |

## 4. 门禁结果（修复后）

| # | 门禁 | 结果 | 证据 |
|---|------|------|------|
| 1 | fmt | ✅ | `cargo fmt -p sz-orm-core -- --check` + `cargo fmt -p sz-orm-sqlx -- --check` 退出 0 |
| 2 | check | ✅ | `cargo check --workspace --all-targets` Finished |
| 3 | clippy -D warnings | ✅ | 0 warnings |
| 4 | test | ✅ | sz-orm-core 3412 passed / sz-orm-sqlx 194 passed / 0 failed |
| 5 | doc | ✅ | `cargo doc --workspace --no-deps --all-features` Finished |
| 7 | e2e | ✅ | 33 passed / 0 failed（含 13 个 v9.9.0 新增） |
| 8 | 占位实现 | ✅ | pool.rs:1483 在 doc comment 隐藏行 |
| 10 | all-features check | ✅ | Finished |

## 5. 向后兼容性

- `PoolDegradationStrategy::None` 为默认值，现有行为不变
- `ZeroCopyDecodeConfig::default()` 的 `enabled=false`，`fetch_all` 行为不变
- `FromRowBorrowed` blanket impl 从 `FromRow` 改为 `FromQueryResult`，覆盖范围更广
- `fetch_all` 在 `zero-copy` feature 下委托 `fetch_all_zero_copy`，默认配置 `enabled=false` 等价于原路径
- sz-pay 项目兼容性验证通过

## 6. 修改文件清单

| 文件 | 修改内容 |
|------|---------|
| Cargo.toml | 版本号 9.8.0 → 9.9.0 |
| packages/sz-orm-core/src/pool.rs | M1-M5 全部实现 |
| packages/sz-orm-core/src/queryable.rs | M1+M6 零拷贝集成 + 修复：fetch_all 委托 fetch_all_zero_copy |
| packages/sz-orm-sqlx/Cargo.toml | zero-copy feature 转发 |
| packages/sz-%orm-sqlx/tests/wiring_e2e.rs | M7 e2e 测试 + 修复：弱测试重写 + 接线测试新增 |
| packages/sz-orm-core/tests/*.rs | 测试辅助更新 |