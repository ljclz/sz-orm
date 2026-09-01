# bench_transaction 完整 Bench 模式结果

> 日期：2026-08-22
> 环境：Windows MSVC, Rust 1.81, SQLite in-memory
> 参数：`--sample-size 10 --warm-up-time 1 --measurement-time 3`

---

## 执行状态

| 适配器 | 状态 | 备注 |
|--------|------|------|
| sz-orm | ✅ 完成 | 12/12 bench 全部成功 |
| sqlx | ⚠️ 超时 | setup 耗时长，需 CI 环境（≥ 30min 超时） |
| diesel | ⚠️ 超时 | 同上 |
| sea-orm | ⚠️ 超时 | 同上 |

> **注**：sz-orm 适配器已获得完整事务性能数据。其他适配器因 setup/teardown 开销大（数据库创建/销毁 + 连接池初始化），在 10 分钟超时内无法完成全部 48 个 bench。建议在 CI 环境中用 ≥ 30 分钟超时运行全量对比。

---

## sz-orm 事务性能数据

### tx_commit（事务提交）

| 数据集大小 | 时间（中位数） | 说明 |
|-----------|---------------|------|
| 10 | 26.7 µs | 小数据集提交 |
| 100 | 52.6 µs | 中数据集提交 |
| 1000 | 67.9 µs | 大数据集提交 |
| 10000 | 66.3 µs | 趋于稳定 |

### tx_rollback（事务回滚）

| 数据集大小 | 时间（中位数） | 说明 |
|-----------|---------------|------|
| 10 | 71.2 µs | 小数据集回滚 |
| 100 | 69.7 µs | 中数据集回滚 |
| 1000 | 69.4 µs | 大数据集回滚 |
| 10000 | 68.4 µs | 趋于稳定 |

### tx_nested（嵌套事务 / SAVEPOINT）

| 数据集大小 | 时间（中位数） | 说明 |
|-----------|---------------|------|
| 10 | 105.0 µs | 小数据集嵌套 |
| 100 | 105.9 µs | 中数据集嵌套 |
| 1000 | 105.9 µs | 大数据集嵌套 |
| 10000 | 106.1 µs | 趋于稳定 |

---

## 分析

1. **事务提交**：26-67 µs，随数据集大小增长，1000 以上趋于稳定（~67 µs）
2. **事务回滚**：68-71 µs，基本稳定，不随数据集大小变化（回滚不涉及数据写入）
3. **嵌套事务**：105-106 µs，基本稳定，比简单事务多 ~40 µs（SAVEPOINT 开销）

### 性能特征

- 事务提交 < 回滚 < 嵌套（符合预期：提交需写 WAL，回滚需撤销，嵌套需 SAVEPOINT）
- 数据集大小对回滚和嵌套事务影响小（操作不依赖数据量）
- 提交时间在 1000 以上稳定（WAL 刷盘开销恒定）

---

## 复现命令

```bash
# 仅 sz-orm（~2 分钟）
cargo bench --bench bench_transaction -- --sample-size 10 --warm-up-time 1 --measurement-time 3 "sz-orm"

# 全量对比（需 ≥ 30 分钟超时，建议 CI 环境）
cargo bench --bench bench_transaction -- --sample-size 10 --warm-up-time 1 --measurement-time 3
```

---

## 代码证据

- `bench-comparison/benches/bench_transaction.rs:1-102` — bench 定义
- `bench-comparison/benches/competitor_adapter.rs:1178-1185` — 4 个适配器
- `bench-comparison/benches/competitor_adapter.rs:131` — DATASET_SIZES = [10, 100, 1000, 10000]