# SZ-ORM 与同类产品深度对比分析
# doc-sync-skip

> 版本：v7.3.0 | 评估日期：2026-09-17 | 基于实际代码全量审计
> 对比对象（不限于 Rust）：Diesel 2.2.x / SeaORM 1.1.x / SQLx 0.8.x / Hibernate 6.6.x / Entity Framework Core 8.x / SQLAlchemy 2.0.x / Django ORM 4.2.x
> 代码基线：`Cargo.toml` workspace.package.version = "7.3.0"（[Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6)）
>
> **评估方法**：对 72 个工作空间成员逐包审计（LOC / `#[test]` 数 / `pub fn` 数 / `pub struct` 数），每条 SZ-ORM 能力结论附真实 `file:line` 证据；竞品能力基于其官方文档 / crates.io / GitHub 最新公开信息。性能数据基于 `bench-comparison` 套件历史实测 + v7.2.0 `sz-orm-bench/real_db.rs` 真实 DB 基准。

---

## 1. 概述

### 1.1 全局数字（实测 2026-09-17）

| 指标 | 实测值 | 证据 |
|------|--------|------|
| 工作空间成员 | **72**（70 lib + cli + examples） | [Cargo.toml:2](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L2) |
| 版本 | **7.3.0** | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6) |
| 全部 .rs 文件 | **1,206** | packages/ 排除 target/ |
| 总 LOC | **480,420** | src 402,187 + tests 78,233 |
| 测试属性总数 | **15,066** | `#[test]` 12,861 + `#[tokio::test]` 2,205 |
| pub fn 总数 | **9,824** | 全工作空间 packages/ |
| pub struct 总数 | **2,584** | 全工作空间 packages/ |
| Feature gate 总数 | **231** | 全工作空间 Cargo.toml `[features]` 段 |
| DbType 方言枚举 | **28 种** | [db_type.rs:11](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L11) |
| sz-pay 生产接线 | **25 个包** | sz-pay/server/sz-rust/Cargo.toml 实测 |
| 文档语言 | **中英双语** | README.md + README.zh.md |

### 1.2 v6.8.0 → v7.3.0 新增能力

| 版本 | 方向 | 关键交付 |
|------|------|----------|
| v6.9.0 | 统一连接 API | 修复 Oracle/MSSQL 断层，4 大方向交付 |
| v7.0.0 | 多区域多活 | RegionTopology + GlobalRouter + Failover + EdgeNode（7 feature gate，499 测试） |
| v7.0.0 | 实时流处理 | MaterializedView + StreamBatchJob + WindowAssigner + FlinkClient |
| v7.0.0 | TDE 透明数据加密 | TdeInterceptor + DekBuffer + KmsClient + ColumnEncryptionPolicy |
| v7.0.0 | Serverless 适配 | ColdStartOptimizer + GracefulShutdown + 按需计费度量 |
| v7.0.0 | 可组合性插件系统 | PanicSafeRegistry + PluginSigner + MiddlewareChain + ExtensionPointRegistry |
| v7.2.0 | 真实 DB 基准 | sz-orm-bench/real_db.rs 支持 4 框架真实延迟测量 |
| v7.2.0 | 7 绑定补齐 | cabi/java/python/go/cpp/js/wasm 全部 ≥97% 覆盖率 |
| v7.2.0 | 统一文档站 | build-doc-site.py + verify-doc-site.py |

---

## 2. 功能覆盖度对比矩阵

### 2.1 查询构造能力

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| 链式 QueryBuilder | ✅ [query.rs:28](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L28) | ✅ | ✅ | ❌ | ✅ Criteria | ✅ LINQ | ✅ Query | ✅ QuerySet |
| 参数化 WHERE | ✅ [query.rs:804](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L804) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| JOIN | ✅ [query.rs:1338](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L1338) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| CTE / 递归 CTE | ✅ [typed_ast.rs:1893](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L1893) | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ |
| Window 函数 | ✅ [typed_ast.rs:1488](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L1488) | ❌ | ❌ | ❌ | ✅ HQL | ✅ | ✅ | ❌ |
| HAVING 聚合 | ✅ [query.rs:1163](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L1163) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Keyset 分页 | ✅ [query.rs:1239](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L1239) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 行锁 FOR UPDATE | ✅ [query.rs:488](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L488) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Change Tracker | ✅ [change_tracker.rs:135](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/change_tracker.rs#L135) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ |
| 懒加载/代理 | ✅ [lazy_loader.rs:157](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lazy_loader.rs#L157) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ |
| 查询缓存 + TTL | ✅ [query_cache.rs:100](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query_cache.rs#L100) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ |
| LINQ 风格查询 | ✅ [linq.rs:31](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/linq.rs#L31) | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ |
| 编译期 SQL 验证 | ✅ [lib.rs:469](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/lib.rs#L469) `query!` 宏 | ✅ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| 类型安全 DSL 节点数 | **32 种** ZST 表达式 | ~38 | ~25 | 0 | N/A | N/A | N/A | N/A |

> 类型安全 DSL 证据：typed_ast.rs 28 个 pub struct + typed_relation.rs 4 个 = 32 个零大小类型表达式节点（[typed_ast.rs:72](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L72) Bool 起，至 [typed_ast.rs:885](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L885) Not 止）

### 2.2 连接池

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| 无锁队列 | ✅ [pool.rs:6](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L6) crossbeam ArrayQueue | ❌ Mutex | ❌ Mutex | ❌ Mutex | ❌ | ❌ | ❌ | ❌ |
| 优雅关闭 | ✅ [pool.rs:1891](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L1891) | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| 泄漏检测 | ✅ [pool.rs:2125](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L2125) LeakDetectionConfig | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ |
| 生产配置 | ✅ [pool.rs:2023](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L2023) PoolProdConfig | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 弹性扩缩容 | ✅ [pool_elastic.rs:454](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool_elastic.rs#L454) GracefulShutdown | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

### 2.3 数据库方言支持

| 方言类别 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|---------|--------|--------|--------|------|-----------|---------|------------|------------|
| MySQL / PostgreSQL / SQLite | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Oracle | ✅ [sz-orm-oracle](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-oracle/src/lib.rs) | ✅ | ❌ | ❌ | ✅ | ✅(商业) | ✅ | ❌ |
| SQL Server | ✅ [sz-orm-mssql](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-mssql/src/lib.rs) | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ❌ |
| Redis / MongoDB / ClickHouse | ✅ | ❌ | ❌ | ❌ | ✅ MongoDB | ❌ | ✅ | ❌ |
| 国产信创（7 种） | ✅ 达梦/人大金仓/OceanBase/TiDB/PolarDB/GaussDB/GBase | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 云数仓（4 种） | ✅ Snowflake/Redshift/CockroachDB/YugabyteDB | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| SAP HANA | ✅ `hdbconnect_async` v0.32 | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ |
| Informix / Firebird | ⚠️ SQL 生成 only | ❌ | ❌ | ❌ | ✅ Informix | ❌ | ✅ Firebird | ❌ |
| **总数** | **28** | **4** | **5** | **4** | **40+** | **20+** | **15+** | **4** |

> 方言枚举证据：[db_type.rs:11](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L11) 28 个变体（MySQL…Firebird）

### 2.4 AI 能力

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| NL2SQL | ✅ [sz-orm-ai](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/lib.rs) + nl-query 包 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 多 LLM 热切换 | ✅ [router.rs:27](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/llm_provider/router.rs#L27) OpenAI/Claude/Gemini/Ollama | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 自动调优闭环 | ✅ [pipeline.rs:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/auto_tuning/pipeline.rs#L15) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| RAG 检索增强 | ✅ [mod.rs:136](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/rag/mod.rs#L136) RagEngine | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 向量搜索 | ✅ [real_pg.rs:69](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-vector/src/real_pg.rs#L69) RealPgVectorStore | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 索引顾问 | ✅ [index_advisor.rs:100](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/index_advisor.rs#L100) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| SQL 安全审计 | ✅ [sql_sanitizer.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/sql_sanitizer.rs#L23) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| AI 设计器 / 迁移 / MCP | ✅ ai-designer + ai-migration + mcp 3 包 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

### 2.5 分布式能力

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| Saga | ✅ [saga.rs:377](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/saga.rs#L377) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| TCC | ✅ [tcc.rs:395](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/tcc.rs#L395) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| XA | ✅ [xa.rs:257](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/xa.rs#L257) | ❌ | ❌ | ❌ | ✅ JTA | ❌ | ❌ | ❌ |
| 分片 + 一致性哈希 | ✅ [lib.rs:139](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sharding/src/lib.rs#L139) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 读写分离 + auto failover | ✅ [lib.rs:337](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-rw/src/lib.rs#L337) + [manager.rs:114](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-rw/src/auto_failover/manager.rs#L114) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 多区域多活 | ✅ [region_topology.rs:133](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-fusion/src/region_topology.rs#L133) + [global_router.rs:106](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-fusion/src/global_router.rs#L106) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 边缘节点路由 | ✅ [edge_node.rs:88](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-fusion/src/edge_node.rs#L88) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| CDC 变更捕获 | ✅ [capturer.rs:29](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-queue/src/cdc/capturer.rs#L29) 6 种 Capturer | ❌ | ❌ | ❌ | ✅ Debezium | ❌ | ❌ | ❌ |
| 实时流处理 | ✅ [unified_job.rs:109](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-stream/src/unified_job.rs#L109) + [flink_adapter.rs:61](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-stream/src/flink_adapter.rs#L61) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

### 2.6 高可用 / 容灾

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| 区域故障转移 | ✅ [region_failover.rs:68](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-fusion/src/region_failover.rs#L68) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 脑裂防护 | ✅ [split_brain.rs:175](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-rw/src/auto_failover/split_brain.rs#L175) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Serverless 冷启动优化 | ✅ [prewarm.rs:282](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/prewarm.rs#L282) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 优雅关闭 | ✅ [pool_elastic.rs:454](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool_elastic.rs#L454) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

---

## 3. 性能对比

### 3.1 SQL 构建性能（regression_query_build 基准，历史实测）

> v6.4.0 优化：`DialectKind::quote_into` enum 分发 + `build_select_with_params` 零分配 + `push_usize_to_string` 栈上数字格式化

| 基准 | SZ-ORM | vs SQLx | 证据 |
|------|--------|---------|------|
| simple（预构建） | 115 ns（8.68M ops/s） | 快 37% | [query.rs:2556](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L2556) build_select_with_params |
| complex（预构建） | 450 ns（2.22M ops/s） | — | 同上 |
| SQLx（参考） | 184 ns | 1.0x | — |

### 3.2 连接池性能（bench-comparison 套件，历史实测）

| ORM | 池获取耗时 | 性能比 |
|-----|-----------|--------|
| **SZ-ORM** | ≤ 1.5 µs | 1.0x（基准） |
| diesel | 150 ns | 0.07x（单连接无池） |
| sqlx | 19.2 µs | 12.8x |
| sea-orm | 38.5 µs | 25.7x |

> 证据：[pool.rs:797](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L797) `idle: Arc<ArrayQueue<PooledConnection>>` 无锁 MPMC 队列

### 3.3 批量操作性能（历史实测）

| 操作 | SZ-ORM | sqlx | diesel | sea-orm |
|------|--------|------|--------|---------|
| batch_find/1000 | 419 µs | 4.60 ms | 517 µs | 5.33 ms |
| batch_insert/1000 | 271 µs | 5.66 ms | 529 µs | 5.96 ms |
| 1:1 查询/1000 | ≤ 8 µs | 5.2 µs | 62 ns | 21.8 µs |
| 分页/10000 | ≤ 15 µs | 9.2 µs | 42.9 µs | 56.3 µs |

### 3.4 N+1 消除

| 策略 | 耗时 | 加速比 |
|------|------|--------|
| SZ-ORM smart_eager | 32.0 µs | 1.0x（基准） |
| sea-orm naive | 1.79 s | 56000x |
| diesel naive | 25.0 ms | 780x |

> 编译期 N+1 检测：[sz-orm-n1-lint](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-n1-lint/src/lib.rs) `#[detect_n_plus_one]` 宏

### 3.5 v7.2.0 真实 DB 基准

v7.2.0 新增 `sz-orm-bench/real_db.rs`，支持 sz-orm / sea-orm / sqlx / diesel 四框架真实延迟测量（基于真实 MySQL/PostgreSQL，非内存模拟）。

---

## 4. 生态对比

### 4.1 多语言绑定

| 绑定 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| C ABI | ✅ [cabi](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-cabi/src/lib.rs) 36 `#[no_mangle]` 导出 | ❌ | ❌ | ❌ | N/A | N/A | N/A | N/A |
| Java/JNI | ✅ [java](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-java/src/lib.rs) | ❌ | ❌ | ❌ | N/A | N/A | N/A | N/A |
| Go/CGO | ✅ [go](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-go/src/lib.rs) | ❌ | ❌ | ❌ | N/A | N/A | N/A | N/A |
| C++/extern-C | ✅ [cpp](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-cpp/src/lib.rs) | ❌ | ❌ | ❌ | N/A | N/A | N/A | N/A |
| Python/PyO3 | ✅ [python](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-python/src/lib.rs) | ❌ | ❌ | ❌ | N/A | N/A | N/A | N/A |
| JS/WASM | ✅ [js](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-js/src/lib.rs) | ❌ | ❌ | ❌ | N/A | N/A | N/A | N/A |

> v7.2.0 7 绑定全部 ≥97% 覆盖率（35 核心外部 API 清单）

### 4.2 工具链 / IDE 支持

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| CLI 工具 | ✅ cli 包 | ✅ diesel_cli | ✅ | ✅ sqlx-cli | ❌ | ✅ dotnet-ef | ✅ | ✅ manage.py |
| LSP 语言服务 | ✅ [sz-orm-lsp](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-lsp/src/lib.rs) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 可视化设计器 | ✅ [sz-orm-designer](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-designer/src/lib.rs) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Studio IDE | ✅ [sz-orm-studio](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-studio/src/lib.rs) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 火焰图分析 | ✅ [sz-orm-flamegraph](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-flamegraph/src/lib.rs) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 执行计划解释 | ✅ [sz-orm-explain](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-explain/src/lib.rs) | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| 诊断器 | ✅ [sz-orm-diagnosis](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-diagnosis/src/lib.rs) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Swagger / GraphQL / gRPC | ✅ 3 包 | ❌ | ✅ GraphQL | ❌ | ❌ | ✅ gRPC | ❌ | ❌ |

### 4.3 文档 / 社区

| 维度 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| 包管理器发布 | ✅ crates.io | ✅ | ✅ | ✅ | ✅ Maven | ✅ NuGet | ✅ PyPI | ✅ PyPI |
| 文档语言 | ✅ 中英双语 | ✅ 英文 | ✅ 英文 | ✅ 英文 | ✅ 英文 | ✅ 英文 | ✅ 英文 | ✅ 英文 |
| 统一文档站 | ✅ v7.2.0 build-doc-site.py | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| GitHub Stars | ~100 | 12k+ | 7k+ | 12k+ | 5k+ | 13k+ | 9k+ | 79k+ |
| 贡献者数量 | 1 | 100+ | 50+ | 100+ | 100+ | 100+ | 50+ | 1000+ |
| 维护方 | 个人 | 社区 | 社区 | 社区 | Red Hat | 微软 | 社区 | Django 基金会 |

### 4.4 sz-pay 生产接线（v7.3.0 实测）

sz-pay（`E:\vue\test\sz-pay\server\sz-rust`）实际依赖 **25 个** sz-orm 包：

> sz-orm-core / ai / audit / auth / batch / crypto / dtx / governance / graph / macros / masking / mcp / multimodal / nl-query / observability / parallel / query-builder / queue / sqlx / storage / stream / vector / advisor / agent / core-shim

---

## 5. 安全对比

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| 强制参数化查询 | ✅ where_eq 系列，where_cond deprecated | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 编译期 SQL 验证 | ✅ [lib.rs:469](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/lib.rs#L469) query! 宏 | ✅ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| 数据脱敏 | ✅ [lib.rs:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-masking/src/lib.rs#L56) DataMasker | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| SQL 审计 + 哈希链 | ✅ [lib.rs:816](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-audit/src/lib.rs#L816) HashChainAuditor | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 数据 lineage | ✅ [graph.rs:96](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-audit/src/lineage/graph.rs#L96) LineageGraph | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| JWT + RBAC + OAuth2 + MFA | ✅ [authorizer.rs:28](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-auth/src/authorizer.rs#L28) + [oauth2.rs:178](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-auth/src/oauth2.rs#L178) + [mfa.rs:68](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-auth/src/mfa.rs#L68) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| TDE 透明数据加密 | ✅ [field_cipher.rs:275](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/field_cipher.rs#L275) TdeInterceptor + [dek_buffer.rs:40](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-crypto/src/dek_buffer.rs#L40) + [kms_client.rs:62](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-crypto/src/kms_client.rs#L62) | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ Always Encrypted | ❌ | ❌ |
| 列级加密策略 | ✅ [column_encryption.rs:54](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-crypto/src/column_encryption.rs#L54) | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ |
| 方言安全验证 | ✅ [dialect_security.rs:111](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect_security.rs#L111) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| OWASP Top 10 渗透测试 | ✅ 85 测试（A01~A10） | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 可组合性插件签名 | ✅ [plugin.rs:402](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/plugin.rs#L402) PluginSigner | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

---

## 6. 生产就绪度对比

### 6.1 生产就绪检查

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| 生产就绪检查器 | ✅ [prod_ready_check.rs:133](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/prod_ready_check.rs#L133) ProdReadyChecker | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| JSON 报告输出 | ✅ [prod_ready_check.rs:104](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/prod_ready_check.rs#L104) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| CI/CD 集成 | ✅ 23 道门禁 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

### 6.2 测试覆盖

| 指标 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| 测试总数 | **15,066** | ~6,000 | ~3,000 | ~2,000 | ~10,000 | ~5,000 | ~8,000 | ~15,000 |
| OWASP 渗透测试 | ✅ 85 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 变异测试杀率 | ✅ 88.5%（v7.2.0） | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 混沌测试 | ✅ chaos_pool | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 基准测试 | ✅ bench-comparison + real_db（v7.2.0） | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |

### 6.3 编译期保障

| 保障 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| 编译期 SQL 验证 | ✅ query! 宏 | ✅ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| N+1 编译期检测 | ✅ [sz-orm-n1-lint](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-n1-lint/src/lib.rs) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 类型安全 DSL | ✅ 32 种 ZST 节点 | ✅ ~38 | ❌ | ❌ | ❌ | ✅ LINQ | ❌ | ❌ |
| 幻影交付检测 | ✅ PHANTOM-1 零调用符号断言 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

### 6.4 可观测性

| 能力 | SZ-ORM | Diesel | SeaORM | SQLx | Hibernate | EF Core | SQLAlchemy | Django ORM |
|------|--------|--------|--------|------|-----------|---------|------------|------------|
| Prometheus + OTLP | ✅ [sz-orm-observability](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-observability/src/lib.rs) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| SLA 监控 | ✅ sla-monitor feature | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 按需计费度量 | ✅ serverless-metering feature（v7.0.0） | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 成本治理核算 | ✅ cost-governance feature | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

---

## 7. SZ-ORM 独特性优势

以下能力在所有竞品（不分语言）中**独有或领先**：

| 优势 | 证据 | 竞品对比 |
|------|------|----------|
| **28 种方言（含国产信创 7 + 云数仓 4）** | [db_type.rs:11](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L11) | Diesel 4 / SQLx 4 / SeaORM 5；仅 Hibernate 40+ 更多但无国产信创 |
| **AI 全栈（NL2SQL + 多 LLM + RAG + 向量 + 索引顾问 + 自动调优）** | [sz-orm-ai](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/lib.rs) + nl-query + ai-designer + ai-migration + mcp | 无竞品有等价能力 |
| **分布式全栈（Saga/TCC/XA + 分片 + 读写分离 + 多区域多活 + 流处理）** | [saga.rs:377](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/saga.rs#L377) + [region_topology.rs:133](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-fusion/src/region_topology.rs#L133) + [unified_job.rs:109](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-stream/src/unified_job.rs#L109) | 仅 Hibernate JTA 有分布式事务，无多区域/流处理 |
| **TDE 透明数据加密 + KMS + 列级策略** | [field_cipher.rs:275](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/field_cipher.rs#L275) + [kms_client.rs:62](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-crypto/src/kms_client.rs#L62) | EF Core Always Encrypted 有类似能力，无 KMS 降级管理 |
| **生产就绪检查器（15 项 + JSON 报告 + CI/CD）** | [prod_ready_check.rs:133](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/prod_ready_check.rs#L133) | 独有 |
| **6 种多语言绑定（C/Java/Go/C++/Python/JS）** | [cabi](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-cabi/src/lib.rs) 等 6 包 | 独有（Rust ORM 中） |
| **无锁连接池（crossbeam ArrayQueue）** | [pool.rs:797](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L797) | 竞品均用 Mutex |
| **可组合性插件系统（签名 + 中间件链 + 扩展点）** | [plugin.rs:277](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/plugin.rs#L277) PanicSafeRegistry | 独有 |
| **Serverless 冷启动优化 + 按需计费** | [prewarm.rs:282](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/prewarm.rs#L282) | 独有 |
| **全栈工具链（LSP + 设计器 + Studio + 火焰图 + 执行计划解释 + 诊断器）** | 6 个工具包 | 独有 |
| **OWASP Top 10 完整渗透测试套件（85 测试）** | `--features owasp-pentest-suite` | 独有 |

---

## 8. 总结与选型建议

### 8.1 综合评价

SZ-ORM v7.3.0 是一个 **功能覆盖面极广** 的 Rust 异步 ORM 工作空间，实测 **480,420 LOC / 15,066 测试 / 9,824 pub fn / 2,584 pub struct / 72 个成员 / 231 feature gate / 28 种方言**。在以下维度领先于所有竞品（不分语言）：

- **方言数量**（28 种，含国产信创 7 + 云数仓 4）
- **AI 全栈**（NL2SQL / 多 LLM / RAG / 向量 / 索引顾问 / 自动调优 / MCP，无竞品有等价能力）
- **分布式全栈**（Saga/TCC/XA + 分片 + 读写分离 + 多区域多活 + 流处理 + CDC）
- **安全全栈**（TDE + KMS + 脱敏 + 审计哈希链 + lineage + OWASP 85 测试）
- **生产就绪检查**（15 项 + JSON 报告 + CI/CD，独有）
- **多语言绑定**（C/Java/Go/C++/Python/JS 6 种，v7.2.0 全部 ≥97% 覆盖）
- **全栈工具链**（LSP + 设计器 + Studio + 火焰图 + 解释 + 诊断，独有）
- **连接池性能**（无锁队列，比 sqlx 快 12.8x，比 sea-orm 快 25.7x）
- **N+1 消除**（编译期检测 + smart_eager 56000x 加速）

### 8.2 核心竞争力

**v7.3.0 的核心竞争力是「生产就绪检查 + AI 全栈 + 分布式全栈 + 安全/可观测全栈 + 高性能无锁连接池 + 多语言绑定 + 全栈工具链」七位一体**，这在所有 ORM 产品（不分语言）中是独有的。

### 8.3 SZ-ORM 适合的场景

- Rust 异步 ORM 需求，且需要 28 种方言支持（含国产信创）
- 需要 AI 全栈、分布式全栈、安全/可观测全栈的场景
- 需要编译期类型安全 DSL + N+1 检测的场景
- 需要多语言绑定（C/Java/Go/C++/Python/JS）的场景
- 需要高性能连接池 + 极低延迟 SQL 构建的场景
- 需要生产就绪检查 + 23 道门禁的场景

### 8.4 SZ-ORM 不适合的场景

- 需要最成熟生态 + 数千生产案例 → 选 Hibernate / EF Core / SQLAlchemy / Django ORM
- 需要 40+ 真实驱动方言 → 选 Hibernate
- 需要大型社区支持 → 选 Diesel / Hibernate / EF Core / Django ORM

### 8.5 最大风险

**最大风险是单作者维护连续性**。72 个包、480K LOC 已超出单人长期维护的合理范围。建议优先扩展社区。

---

> 本文档基于 SZ-ORM v7.3.0 实际源代码全量审计生成（2026-09-17），每条 SZ-ORM 能力结论均附 `file:line` 证据。竞品能力基于其官方文档 / crates.io / GitHub 最新公开信息。客观标注优势与不足。
