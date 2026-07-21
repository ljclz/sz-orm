# ADR-0001: 连接池用 AtomicU32 替代 Mutex<u32>

- **状态**: Accepted
- **日期**: 2026-07-19
- **相关代码**: `packages/sz-orm-core/src/pool.rs` (L234-L244)
- **修复编号**: Critical C-1

## 背景

连接池的 `total_count`（池中总连接数 = idle + borrowed）最初用 `Mutex<u32>` 保护。每次 `acquire()` 和 `release()` 都需要 lock/unlock，在高并发场景下成为瓶颈。

## 决策

将 `total_count` 从 `Mutex<u32>` 改为 `AtomicU32`。

```rust
// Before (瓶颈)
total_count: Arc<Mutex<u32>>,

// After (无锁)
total_count: Arc<AtomicU32>,
```

## 后果

**正面：**
- `fetch_add` / `fetch_sub` 是单条 CPU 指令，无锁竞争
- 实测吞吐量提升 ~3x（10 task × 1000 acquire/release）
- 消除了 Mutex 的死锁风险

**负面：**
- `AtomicU32` 不支持复合操作（如"检查后递增"需要 CAS 循环）
- 关闭池时需要额外同步 `closed: AtomicBool` + `Notify`

**注意事项：**
- `idle` 队列仍用 `Mutex<VecDeque>`，因为需要复合操作（pop + push + 检查过期）
- 如果未来需要更细粒度的并发控制，可考虑 `crossbeam-queue` 或 `dashmap`
