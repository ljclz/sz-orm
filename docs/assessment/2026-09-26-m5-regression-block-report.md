# 门禁阻断报告：M5 连接池快速路径回归（2026-09-26）

- 分支 / commit：`main` @ `0f21d46a788e52eaa1bad71b7f29e8eeacd9ce40`（v9.0.0 M5，审计在隔离 worktree `F:/sz-orm-audit-wt` 进行）
- 失败门禁：**G4 单元/集成测试（`cargo test --workspace`）**
- 状态机：`G1 ✅(16包提交内漂移已修) → G2 ✅ → G3 ✅(2处遗留修复重放) → G4 ❌ → FAILED`
- 失败命令：

```bash
cargo test --workspace
# 最小复现：
cargo test -p sz-orm-core --test chaos chaos_network_partition_idle
```

## 失败输出（确定性复现 ×2，0.00s 纯逻辑失败，非时序抖动）

```
thread 'chaos_network_partition_idle_connections_dropped' panicked at packages\sz-orm-core\tests\chaos.rs:382:5:
acquire should return healthy conn

test result: FAILED. 37 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.22s
```

## 根因：M5（0f21d46）删除了 v8.8.0 网络分区检测语义

M5 提交"连接池acquire快速路径"在 [pool.rs:1696](../../packages/sz-orm-core/src/pool.rs#L1696) 引入 `if !pooled.known_good` 门控，使 release 时标记 `known_good=true`（[pool.rs:1904](../../packages/sz-orm-core/src/pool.rs#L1904)）的 idle 连接**跳过 `is_connected()` 健康检查**。

关键事实链：

1. **所有 idle 连接都经过 release**，因此 `known_good` 恒为 true，`is_connected()` 检查对 idle 路径**永不执行** —— v8.8.0 的分区检测成为死代码。
2. 被跳过的检查，其保留理由就写在同一处注释里（[pool.rs:1698-1699](../../packages/sz-orm-core/src/pool.rs#L1698)）：
   - `is_connected() 是同步内存检查，不涉及 I/O` —— 优化收益仅一次内存布尔读，可忽略；
   - `v8.8.0：保留此检查以正确处理网络分区场景（chaos 测试依赖）` —— 契约明确。
3. chaos 契约测试 [chaos.rs:382](../../packages/sz-orm-core/tests/chaos.rs#L382) 断言"分区后 acquire 必须返回健康连接"，被 M5 确定性打破：分区标记的死连接经快速路径直接返回调用方。
4. M5 自带的 whitehat 测试（[v900_whitehat.rs:115-118](../../packages/sz-orm-core/tests/v900_whitehat.rs#L115)）**显式断言了跳过行为**（`known_good=true 时不应调用 is_connected`），说明这是有意的行为变更，但与池的正确性契约冲突，且提交信息"既有2082测试无退化"与 chaos 测试结果不符。

## 语义分析

`release` 时连接可用，不代表 `acquire` 时仍可用 —— 网络分区恰发生在连接 idle 期间。`known_good`（"归还时是好的"）不能推导出"取出时是好的"，该推断在分布式环境不成立。正确设计若要保留快速路径，必须引入失效机制（如 TTL 重验证、后台探活、或仅在"从未释放过的新建连接"上跳过检查），当前实现无任何失效机制。

## 证据清单（file:line 已核验）

| 证据 | 位置 |
|------|------|
| 快速路径门控 | packages/sz-orm-core/src/pool.rs:1696 |
| v8.8.0 契约注释（无 I/O + 分区依赖） | packages/sz-orm-core/src/pool.rs:1698-1699 |
| release 标记 known_good=true | packages/sz-orm-core/src/pool.rs:1904 |
| chaos 失败断言 | packages/sz-orm-core/tests/chaos.rs:382 |
| whitehat 断言跳过行为 | packages/sz-orm-core/tests/v900_whitehat.rs:115-118 |
| M5 提交 | 0f21d46（2026-09-26 00:08:55 +0800） |

## 建议修复（交由 v9.0.0 会话执行）

**推荐：撤销快速路径门控**，恢复 v8.8.0 无条件检查（收益本就仅是一次内存读）：

```rust
// pool.rs acquire idle 路径，删除 if !pooled.known_good 门控，恢复：
if !pooled.conn.is_connected() {
    to_close.push(pooled);
    continue;
}
```

同步处理：`v900_whitehat.rs` 中断言"跳过检查"的用例（test_known_good_skips_is_connected_on_reacquire 等）需改写为断言"acquire 始终执行健康检查"；`known_good` 字段若无其他用途一并移除。若坚持保留快速路径，必须先给出分区场景下的失效机制设计（TTL/探活），并使 chaos 测试通过。

## 复跑要求

修复后从 **G4** 重跑（G5~G23 未执行）；审计 worktree `F:/sz-orm-audit-wt` 可复用于验证。
