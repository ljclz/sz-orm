# sz-orm-loom-check — 连接池并发状态机模型检查

独立工作区（不属于主 workspace），用 [loom](https://github.com/tokio-rs/loom) 对
`packages/sz-orm-core/src/pool.rs` 的 acquire/release/shutdown 核心状态机做**穷举交错**模型检查。

## 为什么独立成工作区

loom 模型检查需要在被测代码中使用 `loom::sync::*` 替代 `std::sync::*`，无法直接嵌入
70+ 包的主 workspace（会污染主依赖树，且与 `RUSTFLAGS=-D warnings` 冲突——2026-08-14
评估记录）。独立工作区只依赖 loom 一个 crate，依赖树极小，构建快。

## 建模范围

| 真实实现（pool.rs） | 模型（src/lib.rs） |
|---|---|
| `total_count: Arc<AtomicU32>` + CAS | `total: AtomicU32` + `compare_exchange` |
| `idle: Arc<ArrayQueue<PooledConnection>>` | `idle: Mutex<VecDeque<u32>>`（有界） |
| `closed: Arc<AtomicBool>` | `closed: AtomicBool` |
| acquire 等待（tokio 超时 + 指数退避） | 有界 spin（`MAX_SPINS=1` 次 yield 后重试） |
| shutdown/close_all | `closed=true` + 清空空闲队列 + 通知 |

模型复刻了 2026-10-24 深度评测修复的两处 `closed` 复查（循环顶部 + CAS 成功后），
并验证：

1. **shutdown 后绝不新建连接**（`shutdown_prevents_new_connection_after_close`）
2. **total 永不超过 max_size 且计数守恒**（`total_never_exceeds_max`）
3. **shutdown 后 acquire 全部被拒绝，已借出连接归还后 total 归零**
   （`shutdown_rejects_new_acquire_and_drains`）

## 验证结果

- 有修复：`cargo test --offline` → **3 passed**
- 移除两处修复检查后：`shutdown_prevents_new_connection_after_close` **FAILED**
  （`shutdown 后新建了连接: created=4`）——模型能抓到真实竞态。

## 运行方式

```bash
cd loom-check
cargo test --offline   # 使用本地缓存（若网络可用也可去掉 --offline）
```

注意：loom 穷举交错，状态空间与线程数/重试次数相关。当前模型参数
（`MAX=3`、`MAX_SPINS=1`、等待线程 1 个）在 2 秒内跑完；增大参数会显著放大状态空间。

## 与主工作区回归测试的关系

`chaos_pool.rs::chaos_pool_shutdown_race_no_new_connection` 是真实代码上的**确定性回归测试**
（单线程调度构造竞态），本工作区是用 loom 做的**穷举交错验证**。两者互补：
回归测试防回归，模型检查证明修复在任意交错下成立（在模型范围内）。