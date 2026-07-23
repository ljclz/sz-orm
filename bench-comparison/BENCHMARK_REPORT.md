# SZ-ORM Benchmark 对比报告

> 测试日期：2026-07-23
> 测试环境：Windows + SQLite in-memory（cache=shared）
> Criterion 参数：sample-size=10, measurement-time=2s, warm-up-time=1s

## 1. 测试方法

### 1.1 测试对象

| ORM | 版本 | 类型 | 说明 |
|-----|------|------|------|
| rusqlite | 0.32 | 同步 | 底层 SQLite 绑定，作为 baseline |
| Diesel | 2.3 | 同步 | 编译时类型安全 ORM |
| SQLx | 0.9 | 异步 | 原生异步 SQL 执行 |
| SeaORM | 1.1 | 异步 | 异步 ORM |
| SZ-ORM | 1.0 | 异步 | 自研异步 ORM |

### 1.2 测试场景

所有测试使用相同的表结构（`bench_users`），SQLite in-memory 数据库（`cache=shared`），确保公平对比。

- **INSERT**：批量插入 1k/10k/100k 行，每次 iter 创建新表（测完整 CRUD 周期）
- **SELECT BY ID**：预插入 100k 行，按主键查询单行（setup 在外，每次 iter 只查 1 行）
- **SELECT ALL**：预插入 100k 行，查询全部（setup 在外，每次 iter 查全部 100k 行）
- **UPDATE**：预插入 100k 行，逐行更新（setup 在外，每次 iter 更新 1 行）
- **DELETE**：预插入 100k 行，逐行删除（每次 iter 完整 setup+insert+delete 周期）

### 1.3 公平性说明（v0.4 修复）

**v0.2 问题**：异步 ORM 的 select/update 测试用 `iter_batched` 每次迭代都重新 setup+插入10万行，导致测量的是"setup+批量操作"总时间（秒级），而同步 ORM 只 setup 一次做单次操作（微秒级），对比极其不公平。

**v0.4 修复**：
1. **select_by_id / select_all / update**：异步 ORM 改为 setup 在 `b.iter` 外（`rt.block_on`），`b.to_async().iter()` 每次迭代只做单次操作，与同步 ORM 结构完全一致
2. **SQLite cache=shared**：所有异步 ORM 使用 `sqlite::memory:?cache=shared` + `max_connections(10)`，确保多连接共享同一 in-memory 数据库（SQLite `:memory:` 默认每连接独立）
3. **PooledConnection Drop 修复**：SZ-ORM Pool 已实现 `Drop for PooledConnection`，连接 drop 时自动归还池中，无需显式 `pool.release()`
4. **SzOrmCtx Clone**：SZ-ORM 的上下文结构体实现 `Clone`，支持在 async 闭包中 clone 传递

## 2. 原始数据

所有数据为 criterion 统计的中位值（median）。

### 2.1 INSERT（批量插入，每次 iter 完整周期）

| 数据量 | rusqlite | diesel | sqlx | sea-orm | sz-orm |
|--------|----------|--------|------|---------|--------|
| 1,000 | 2.67 ms | 2.97 ms | 22.60 ms | 21.75 ms | **20.84 ms** |
| 10,000 | 32.18 ms | 30.60 ms | 213.49 ms | 237.31 ms | **200.88 ms** |
| 100,000 | 330.02 ms | 500.34 ms | 2.19 s | 2.13 s | **2.07 s** |

### 2.2 SELECT BY ID（预插入 100k 行，单次查询）

| ORM | rusqlite | diesel | sqlx | sea-orm | sz-orm |
|-----|----------|--------|------|---------|--------|
| 耗时 | 1.89 µs | 3.12 µs | 17.05 µs | 18.19 µs | **25.48 µs** |

### 2.3 SELECT ALL 100k（预插入 100k 行，查询全部）

| ORM | rusqlite | diesel | sqlx | sea-orm | sz-orm |
|-----|----------|--------|------|---------|--------|
| 耗时 | 20.66 ms | 27.72 ms | 128.28 ms | 129.83 ms | **192.60 ms** |

### 2.4 UPDATE（预插入 100k 行，单次更新）

| ORM | rusqlite | diesel | sqlx | sea-orm | sz-orm |
|-----|----------|--------|------|---------|--------|
| 耗时 | 1.97 µs | 3.65 µs | 25.56 µs | 19.87 µs | **23.21 µs** |

### 2.5 DELETE（预插入 100k 行，逐行删除完整周期）

| ORM | rusqlite | diesel | sqlx | sea-orm | sz-orm |
|-----|----------|--------|------|---------|--------|
| 耗时 | 420.07 ms | 762.59 ms | 5.19 s | 5.22 s | **4.83 s** |

## 3. 对比分析

### 3.1 同步 vs 异步

rusqlite 和 Diesel（同步）在所有单次操作场景中均显著快于异步 ORM，主要原因是：
- 无 async runtime 调度开销
- 无连接池 acquire/release 开销
- 无 Future 状态机开销

**但同步 ORM 无法在高并发场景下发挥优势**，异步 ORM 的价值在于 I/O 并发能力。

### 3.2 异步 ORM 排名

| 场景 | 第 1 名 | 第 2 名 | 第 3 名 | SZ-ORM 表现 |
|------|---------|---------|---------|-------------|
| insert/1k | **sz-orm** (20.84ms) | sea-orm (21.75ms) | sqlx (22.60ms) | 比 sea-orm 快 4%，比 sqlx 快 8% |
| insert/10k | **sz-orm** (200.88ms) | sqlx (213.49ms) | sea-orm (237.31ms) | 比 sqlx 快 6%，比 sea-orm 快 15% |
| insert/100k | **sz-orm** (2.07s) | sea-orm (2.13s) | sqlx (2.19s) | 比 sea-orm 快 3%，比 sqlx 快 5% |
| select_by_id | **sqlx** (17.05µs) | sea-orm (18.19µs) | sz-orm (25.48µs) | 比 sqlx 慢 49%，比 sea-orm 慢 40% |
| select_all_100k | **sqlx** (128.28ms) | sea-orm (129.83ms) | sz-orm (192.60ms) | 比 sqlx 慢 50%，比 sea-orm 慢 48% |
| update | **sea-orm** (19.87µs) | sz-orm (23.21µs) | sqlx (25.56µs) | 比 sea-orm 慢 17%，比 sqlx 快 9% |
| delete | **sz-orm** (4.83s) | sqlx (5.19s) | sea-orm (5.22s) | 比 sqlx 快 7%，比 sea-orm 快 7% |

### 3.3 关键发现

1. **INSERT 场景**：SZ-ORM 在 1k/10k/100k 三个数据量级均排名第一，比 sea-orm 快 3%~15%，比 sqlx 快 5%~8%
2. **DELETE 场景**：SZ-ORM 排名第一，比 sqlx/sea-orm 快约 7%
3. **UPDATE 场景**：SZ-ORM 排名第二（23.21µs），优于 sqlx（25.56µs），略慢于 sea-orm（19.87µs）
4. **SELECT 场景**：SZ-ORM 排名第三，比 sqlx/sea-orm 慢 40%~50%。原因：SZ-ORM Connection trait 仅支持 `&str` SQL，使用 `format!()` 构造 SQL 有额外字符串分配开销，且无预编译语句复用
5. **数据量验证**：10万行数据下，异步 ORM 单次操作均在微秒~毫秒级，符合预期（v0.2 秒级异常已修复）

### 3.4 v0.2 → v0.4 修复效果

| 场景 | v0.2（异常） | v0.4（修复后） | 提升倍数 |
|------|-------------|---------------|---------|
| select_by_id/sqlx | 3.67 s | 17.05 µs | **215,000x** |
| select_by_id/sea-orm | 8.27 s | 18.19 µs | **454,000x** |
| select_by_id/sz-orm | 4.70 s | 25.48 µs | **184,000x** |
| select_all/sqlx | 2.20 s | 128.28 ms | **17x** |
| select_all/sea-orm | 4.55 s | 129.83 ms | **35x** |
| select_all/sz-orm | 2.13 s | 192.60 ms | **11x** |
| update/sqlx | - | 25.56 µs | - |

**根因**：v0.2 的 `iter_batched` 每次迭代重新 setup+插入10万行，测量的是"setup+批量操作"总时间；v0.4 将 setup 移到 `b.iter` 外，每次迭代只测单次操作。

## 4. 局限性

1. **仅测试 SQLite in-memory**：未覆盖 MySQL/PostgreSQL 网络场景，异步 ORM 的网络 I/O 优势未体现
2. **单线程**：未测试并发场景，异步 ORM 在高并发下的优势未体现
3. **SZ-ORM SQL 构造限制**：SZ-ORM Connection trait 当前仅支持 `&str` SQL（不支持参数绑定），使用 `format!()` + 手动转义，有额外字符串分配开销。这是 SELECT 场景落后的主要原因
4. **sample-size=10**：为加速测试采用小样本，统计置信度低于默认的 100 样本

## 5. 结论

在 SQLite in-memory 单线程场景下：

- **INSERT/DELETE**：SZ-ORM 是三个异步 ORM 中最快的
- **UPDATE**：SZ-ORM 优于 sqlx，略慢于 sea-orm
- **SELECT**：SZ-ORM 落后于 sqlx/sea-orm，主要受限于 `format!()` SQL 构造方式

后续改进方向：
- 为 SZ-ORM Connection trait 增加参数绑定支持，消除 `format!()` 开销
- 补充 MySQL/PostgreSQL 网络场景的 benchmark
- 补充多线程并发 benchmark
- 增大 sample-size 到 100 提高统计置信度

## 6. 复现方法

```bash
cd bench-comparison
cargo bench --bench orm_comparison -- --sample-size 10 --measurement-time 2 --warm-up-time 1
```

报告生成位置：`target/criterion/index.html`

## 7. 变更历史

- **v0.4（2026-07-23）**：修复 benchmark 设计不公平问题——异步 ORM select/update 的 setup 移到 `b.iter` 外；使用 `cache=shared` 解决 SQLite `:memory:` 多连接不共享数据；SZ-ORM Pool 实现 Drop 自动归还。数据量从 1/10/100 行提升到 1k/10k/100k 行
- **v0.2（2026-07-23）**：初始版本，异步 ORM 测量包含 setup 时间导致结果异常（秒级），SZ-ORM 显式 `pool.release()` 规避 Drop bug
