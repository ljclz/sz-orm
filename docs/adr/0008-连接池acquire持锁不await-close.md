# ADR-0008: 连接池 acquire 持锁期间不 await close()

- **状态**: Accepted
- **日期**: 2026-07-22
- **相关代码**: `packages/sz-orm-core/src/pool.rs` (L310-L407, L454-L500)
- **修复编号**: Critical C-1（v0.2.1）

## 背景

连接池 `acquire()` 需要从 `idle: Mutex<VecDeque<PooledConnection>>` 中取出连接。取出的连接可能已过期（`max_lifetime`）、空闲过久（`idle_timeout`）或已断开（`is_connected() == false`），需要 close 并从 `total_count` 减 1。

最初的实现在持锁期间直接 `await close()`：

```rust
// ❌ 错误做法（已废弃）
let mut idle = self.idle.lock().await;
while let Some(pooled) = idle.pop_front() {
    if pooled.is_expired(...) {
        pooled.conn.close().await;  // 🔴 持锁期间 await！
        self.total_count.fetch_sub(1, Ordering::SeqCst);
        continue;
    }
    // ...
}
// 锁在这里才释放
```

问题：`close()` 是异步 I/O 操作（发送 TCP FIN、等待数据库响应），持锁期间 await 会导致：
1. 其他所有 `acquire()` 线程阻塞在同一把锁上
2. 高并发下 acquire 超时（10s）频繁触发
3. 表现为 `total_count` 正常但 `idle` 波动剧烈（连接被 close 后新创建，但创建也需要锁）

## 决策

持锁期间**仅做内存操作**（pop_front + 检查时间），需要 close 的连接放入本地 `to_close: Vec`，**释放锁后**批量 close：

```rust
// ✅ 正确做法
let mut to_close: Vec<PooledConnection> = Vec::new();
let acquired: Option<PooledConnection> = {
    let mut idle = self.idle.lock().await;       // 持锁
    while let Some(pooled) = idle.pop_front() {
        if pooled.is_expired(...) {
            to_close.push(pooled);                // 仅移到本地 Vec
            continue;
        }
        // ...
    }
    found
};  // 🔓 锁在这里释放

// 释放锁后批量 close（不持任何锁）
for mut pooled in to_close {
    let _ = pooled.conn.close().await;
    self.total_count.fetch_sub(1, Ordering::SeqCst);
}
```

同样的模式应用于 `close_all()` 和 `reap_idle()`。

## 后果

**正面：**
- 持锁时间从"close 的 I/O 延迟"降到"内存 pop + 时间比较"，微秒级
- 高并发下 acquire 不再因 close 而阻塞
- `close_all` 时其他 acquire 可继续工作（仅在 pop 阶段短暂持锁）

**负面：**
- `to_close` 中的连接在 close 失败时可能泄漏（`let _ = close().await` 忽略错误），但 `total_count` 已 `fetch_sub`，计数不会偏
- 需要额外分配 `to_close: Vec`，但通常为空或少量元素

**注意事项：**
- **Bug 定位提示**：如果生产出现 acquire 超时 + idle 波动剧烈 + total_count 正常：
  1. 检查 `is_connected()` 是否为同步内存检查（若是异步或涉及 I/O，持锁期间调用会重新引入问题）
  2. 检查 `is_expired()` / `is_idle_too_long()` 是否有耗时计算（当前是 `Instant` 比较，O(1)）
  3. 检查连接泄漏：acquire 后是否所有路径都调用了 release（包括错误路径）
  4. 检查 `factory.create()` 是否在锁外执行（当前实现 L382-L397 在锁外，正确）
- **close 失败不回滚计数**：`total_count` 已 `fetch_sub` 但连接可能未正确关闭，导致数据库侧连接数 > `total_count`。生产中应监控数据库侧连接数。
- `acquire` / `release` / `close_all` / `reap_idle` 均已加 `#[tracing::instrument]`，生产可通过 tracing span 定位持锁时间
