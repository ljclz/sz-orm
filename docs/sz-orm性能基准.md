# SZ-ORM 性能基准报告

> 项目名称：SZ-ORM（鲜视达 ORM）
> 文档版本：v3.0（同步到 39 包 / 1970+ 测试 / v0.2.1 / AI 增强完成）
> 适用版本：SZ-ORM v0.2.1
> 更新日期：2026-07-20
> 数据来源：criterion 基准（benches/core_bench.rs）+ stress 压力测试 + 真实 DB 超大数据量集成测试（本机 + 远程云 <your-server-ip>）+ 1h Soak Test 实测
> 测试环境：Windows，MySQL 9.6.0（3306）、PostgreSQL 18（5432）、SQLite（内存/文件）、Oracle 23ai Free（1521）；远程云 MySQL 8.x（8802）、远程云 PostgreSQL（5432）

---

## 一、数据库性能对比

### 1.1 批量 INSERT 吞吐量（实测）

测试方式：10 万行批量 INSERT，通过 sqlx（MySQL/PG）、rusqlite（SQLite）、rust-oracle（Oracle）直接执行 sz-orm-core Dialect 生成的 SQL。

| 数据库 | 操作 | 吞吐量 | 相对倍数 | 环境 |
|--------|------|--------|---------|------|
| SQLite（本机文件） | 10 万行批量 INSERT | **72 万行/s** | 4.97× | 本机 |
| PostgreSQL 18 | 10 万行批量 INSERT | **26.8 万行/s** | 1.85× | 本机 |
| MySQL 9.6 | 10 万行批量 INSERT | **14.5 万行/s** | 1.0×（基准） | 本机 |
| Oracle 23ai Free | 10 万行批量 INSERT | **1.91 万行/s** | 0.13× | 本机（sysdba + PDB） |
| PostgreSQL（远程云） | 10 万行批量 INSERT | **4.11 万行/s** | 0.28× | <your-server-ip>:5432 |
| MySQL 8.x（远程云） | 10 万行批量 INSERT | **2.57 万行/s** | 0.18× | <your-server-ip>:8802 |

**结论**：

- SQLite 无网络开销，吞吐最高，适合嵌入式/边缘场景与集成测试。
- PostgreSQL 18 批量写入约为 MySQL 9.6 的 1.85 倍。
- Oracle 23ai Free 受 IDENTITY 列逐行解析与 PDB 元数据开销影响，吞吐约 1.91 万行/s；生产版 Oracle（EE + 内存优化）应有数量级提升。
- 远程云 DB 因特网 RTT 拉低吞吐：PG 云 4.11 万行/s、MySQL 云 2.57 万行/s；生产建议连接池 + 批量数组绑定（UNNEST/VALUES batch）。
- 以上为**驱动直执**数据；sz-orm-core 的 SQL 生成开销相对网络 IO 可忽略（见 3.1 查询构建基准）。

### 1.2 真实 DB 压力与一致性验证

| 场景 | 规模 | 结果 |
|------|------|------|
| MySQL 批量写入 stress | 10,000 行 INSERT（经 sz-orm-sqlx 适配器 + Pool 抽象层） | ✅ 通过 |
| PostgreSQL 批量写入 stress | 10,000 行 INSERT（经 sz-orm-sqlx 适配器 + Pool 抽象层） | ✅ 通过 |
| Oracle 23ai 批量写入 | 100,000 行 INSERT（rust-oracle 直连 PDB） | ✅ 通过 |
| 并发转账守恒（Jepsen） | 8 task × 50 并发转账，总额不变 | ✅ 守恒 |
| 真实 DB Jepsen | MySQL 5 项 + PG 5 项 | ✅ 10/10 通过 |
| 真实云 DB Jepsen | MySQL 5 项 + PG 5 项（<your-server-ip>） | ✅ 10/10 通过 |
| 真实云 DB Pool/Tx | MySQL 5 项 + PG 5 项 + SQLite 2 项 | ✅ 12/12 通过 |
| Oracle 事务回滚 | INSERT 后 ROLLBACK，行数守恒 | ✅ 通过 |
| 保存点嵌套 | 20 层嵌套 savepoint（真实 DB） | ✅ 通过 |

### 1.3 数据库选型建议

| 场景 | 推荐 | 依据 |
|------|------|------|
| 高吞吐写入 | PostgreSQL | 26.8 万行/s，约为 MySQL 1.85 倍 |
| 嵌入式/边缘/单测 | SQLite | 72 万行/s，零部署 |
| 既有 MySQL 生态 | MySQL 9.x | 14.5 万行/s，生态成熟 |
| 企业合规/存量 Oracle | Oracle 23ai | OracleDialect 全语法支持（`:N` 占位符 + OFFSET/FETCH 分页 + IDENTITY 列），1.91 万行/s |
| 跨地域云原生 | MySQL / PG | 远程云 2.57 / 4.11 万行/s，连接池 + 批量绑定可进一步优化 |

### 1.4 长稳态（Soak）性能

| 指标 | 1h Soak 实测值 | 说明 |
|------|---------------|------|
| 总操作数 | **13.8 亿次** | 混合 SELECT/INSERT/UPDATE/DELETE |
| 吞吐量衰减 | **1.16%** | 初始 382 万 ops/s → 结束 378 万 ops/s |
| P99 延迟 | **43μs → 41μs** | 稳定无漂移 |
| 错误数 | **0** | 全程无错误 |
| 连接池状态 | idle=active=max=8 | 终态一致，无泄漏 |

Soak 测试方式：`packages/sz-orm-core/tests/soak.rs`，SOAK_DURATION=1h 环境变量控制。每 60 秒采样一次（SoakMonitor 10-field snapshot），自动检测 6 类退化（吞吐衰减 >10%、P99 增长 >2x、RSS 增长 >50MB、fd 增长 >10、连接池泄漏、错误率 >0.1%）。

---

## 二、连接池性能

### 2.1 基准项（criterion，`benches/core_bench.rs`）

| 基准组 | 测量内容 |
|--------|---------|
| `pool_acquire_release` | `Pool::acquire()` + `release()` 单次往返延迟（mock ConnectionFactory） |
| `mock_execute` / `mock_query` | `Connection::execute/query` 抽象层调用开销 |
| 并发 acquire/release | 多任务竞争同一 Pool 的吞吐与公平性 |

### 2.2 压力测试结论（tests/stress.rs，77 项 Stress 之一部分）

| 场景 | 验证点 | 结果 |
|------|--------|------|
| `stress_pool_concurrent_acquire_release` | 高并发获取/归还无死锁、无泄漏 | ✅ |
| `stress_pool_max_size_enforcement` | 池满后 acquire 阻塞，超时窗口 900ms–2000ms（acquire_timeout=1s） | ✅ 实测落在窗口内 |
| 归还唤醒 | 池满时 release 后，等待者被唤醒 < 200ms | ✅（Notify 机制） |
| `stress_pool_burst_load` | 突发流量下不超过 max_size | ✅ |
| `stress_pool_exhaustion_recovery` | 耗尽后恢复获取 < 200ms | ✅ |
| `stress_high_frequency_acquire_release` | 高频获取/归还无竞态 | ✅ |
| `stress_long_transaction` / `stress_mixed_workload` | 长事务 + 混合读写下池稳定 | ✅ |
| `stress_concurrent_reap_idle` | 并发 reap_idle 安全 | ✅ |
| `stress_with_factory_faults` | 工厂故障注入下错误正确传播 | ✅ |
| `stress_close_all_drops_released` | close_all 后拒绝新 release | ✅ |

### 2.3 调参建议

| 参数 | 默认值 | 建议 |
|------|--------|------|
| `max_size` | 100 | 高并发：CPU 核数 × 2～4；避免超过数据库 max_connections |
| `min_idle` | 5 | 设为日常均值，消除冷启动建连 |
| `acquire_timeout` | 30s | 联机交易建议 1–5s，快速失败 |
| `idle_timeout` | 600s | 配合 `reap_idle()` 定期回收 |
| `max_lifetime` | 1800s | 必须小于数据库 `wait_timeout`，避免拿到服务端已断开的连接 |

### 2.4 pgvector 向量搜索性能（sz-orm-vector）

| 场景 | 指标 | 说明 |
|------|------|------|
| 内存向量搜索（128 维） | 单次查询 < 100μs | InMemoryVectorStore，cosine 相似度，1000 向量集合 |
| pgvector IVFFlat 索引 | 大规模搜索 10-100x 加速 | 需手动创建索引：`CREATE INDEX ON vectors_{name} USING ivfflat (embedding vector_cosine_ops)` |
| 向量插入 | 与 PG 批量 INSERT 相同 | 复用 sz-orm-core 的批量写入优化（UNNEST/VALUES batch） |

---

## 三、查询构建性能

### 3.1 基准项（criterion，`benches/core_bench.rs`）

| 基准组 | 测量内容 | 说明 |
|--------|---------|------|
| `value_to_param` | `Value::to_param()` 各变体转换 | null / i64 / f64 / bool / 短字符串 / 256 字节长字符串 / 64 字节 Bytes / 10 元素 Array |
| `dialect_escape_string` | `Dialect::escape_string()` | SQL 字符串转义（防注入基础操作） |
| `dialect_build_create_table` | DDL 生成 | 建表语句构建 |
| `dialect_build_pagination` | 分页 SQL 生成 | LIMIT/OFFSET（Oracle 为 OFFSET/FETCH） |
| `in_memory_db_insert` | InMemoryDb 插入 | 测试用内存库写入 |
| `in_memory_db_select_all` / `select_where` | 全表/条件扫描 | 数据扫描路径 |

### 3.2 性能特征

- SQL 构建为**纯内存字符串操作**，无锁、无 IO；单条 SELECT/INSERT 构建开销在微秒级，相对真实数据库网络往返（通常数百微秒至毫秒级）可忽略。
- `to_param()` 返回 `Cow<str>`：数值/布尔变体零分配借用，字符串变体按需分配，避免不必要拷贝。
- `QueryBuilder` 链式调用每次消费 `self`，编译期内联后无额外运行时开销。

### 3.3 查询侧优化清单

1. 只选必要列，避免 `SELECT *`。
2. 深翻页用主键游标（`WHERE id > ?` + `LIMIT n`）替代大 `OFFSET`。
3. 批量写入按 `DEFAULT_BATCH_SIZE`（1000）分批。
4. 静态 SQL 用 `sql_string!` 在编译期完成校验，运行时零校验成本。
5. 热路径避免重复创建 Dialect：`get_dialect()` 结果可缓存复用。

---

## 四、基准测试运行方式

### 4.1 criterion 基准（微基准）

```bash
# 运行 sz-orm-core 全部基准
cargo bench --package sz-orm-core

# 只运行指定基准组
cargo bench --package sz-orm-core -- value_to_param
cargo bench --package sz-orm-core -- pool

# 查看 HTML 报告（含趋势图、箱线图）
start target/criterion/index.html
```

criterion 配置：`harness = false`（benches/core_bench.rs），启用 `html_reports` + `async_tokio`。

### 4.2 压力测试（stress）

```bash
# core 压力测试
cargo test -p sz-orm-core --test stress

# 各扩展包压力测试
cargo test -p sz-orm-dtx --test stress
cargo test -p sz-orm-limit --test stress
cargo test -p sz-orm-mqtt --test stress
cargo test -p sz-orm-queue --test stress
cargo test -p sz-orm-scheduler --test stress
cargo test -p sz-orm-storage --test stress
cargo test -p sz-orm-websocket --test stress
```

### 4.3 真实 DB 集成测试（需本机数据库）

```bash
# 真实 DB 测试默认 #[ignore]，需加 --ignored 运行
cargo test -p sz-orm-core --test integration_mysql -- --ignored
cargo test -p sz-orm-core --test integration_pg -- --ignored
cargo test -p sz-orm-core --test integration_sqlite
cargo test -p sz-orm-core --test integration_oracle -- --ignored   # 需 Oracle 23ai + OCI_LIB_DIR

# 真实 DB Jepsen + Pool/Tx（sz-orm-sqlx 包）
cargo test -p sz-orm-sqlx -- --ignored
```

数据库连接（本机测试环境）：

| 数据库 | URL / 环境变量 |
|--------|-----|
| MySQL 9.6（本机） | `mysql://root:<your-password>@127.0.0.1:3306/sz_orm_test` |
| PostgreSQL 18（本机） | `postgres://postgres:<your-password>@127.0.0.1:5432/sz_orm_test` |
| Oracle 23ai Free（本机） | `Connector::new("sys","<your-password>","127.0.0.1:1521/freepdb1.FALSE").privilege(Sysdba)` |
| MySQL 8.x（远程云） | `SZ_ORM_MYSQL_URL=mysql://root:***REMOVED***@<your-server-ip>:8802/shop` |
| PostgreSQL（远程云） | `SZ_ORM_PG_URL=postgres://lewuli:<your-pg-password>@<your-server-ip>:5432/lewuli` |
| Oracle 23ai（环境覆盖） | `SZ_ORM_ORACLE_USER` / `SZ_ORM_ORACLE_PASSWORD` / `SZ_ORM_ORACLE_CONNECT_STRING` |

Oracle 23ai 测试环境额外要求：
- Oracle Client 库（oci.dll 等）位于 PATH 中
- 设置 `OCI_LIB_DIR=C:\app\Administrator\product\23ai\dbhomeFree\bin`
- 服务名注意 db_domain 后缀（如 `freepdb1.FALSE`，可通过 `lsnrctl status` 查询）

### 4.4 真实云服务性能相关测试

```bash
cargo test -p sz-orm-mqtt --features real-broker -- --ignored     # 需 MQTT broker
cargo test -p sz-orm-websocket --features server -- --ignored     # WebSocket server
cargo test -p sz-orm-queue --features rabbitmq -- --ignored       # 需 RabbitMQ
cargo test -p sz-orm-storage --features s3-sdk -- --ignored       # 需 MinIO/AWS S3
cargo test -p sz-orm-ai --features real -- --ignored              # 需 OPENAI_API_KEY
cargo test -p sz-orm-grpc --features real -- --ignored            # tonic gRPC 端到端
cargo test -p sz-orm-graphql --features real --                   # async-graphql + axum
```

### 4.5 全量回归

```bash
cargo test --workspace          # 1970+ 通过，0 失败，72 忽略（112 个测试套件；忽略项需真实 DB/云服务凭证）
```

---

## 五、测试环境说明

### 5.1 本机环境

| 组件 | 版本 | 部署 |
|------|------|------|
| CPU/内存 | 本机开发机 | Windows |
| PostgreSQL | 18 | `<your-pg-install-path>`，数据目录 `E:\db\pgsql18-data`，端口 5432 |
| MySQL | 9.6.0 | `E:\db\mysql\`，数据目录 `E:\db\mysql-data`，端口 3306 |
| Oracle | 23ai Free | `C:\app\Administrator\product\23ai\dbhomeFree`，端口 1521，PDB 服务名 `freepdb1.FALSE` |
| 运行时 | tokio 1.40 | full features |

### 5.2 远程云 DB 环境

| 组件 | 版本 | 部署 |
|------|------|------|
| 服务器 | <your-server-ip> | 远程云主机 |
| MySQL | 8.x | 端口 8802，数据库 `shop`，用户 `root` |
| PostgreSQL | - | 端口 5432，数据库 `lewuli`，schema `public`，用户 `lewuli` |

> 注：本文所有性能数字均在上述环境测得，用于**相对比较**（方言/驱动/池配置之间的差异），不作为绝对 SLA 承诺。生产环境请按 4.1–4.3 节命令在目标硬件上重新测量。
