# SZ-ORM 架构设计文档

> 项目名称：SZ-ORM（鲜视达 ORM）
> 文档版本：v7.3.0（同步到 70 lib 包 + cli + examples / v7.3.0 / 性能极致优化 + 企业级高可用 + AI 深度集成 + 生态扩展）
> 适用版本：SZ-ORM v7.3.0（工作空间 72 个成员：70 个 lib + cli + examples）
> 更新日期：2026-09-17
> 文档定位：整体架构、包间依赖、核心设计决策、扩展包开发指南
> 代码：176,709 LOC（src/ 97,318 + tests/ 79,391）
> 成熟度：生产可用（内部项目 sz-pay 7 包 297 引用），sz-orm-core 已发布到 crates.io | 已知 Bug：0
> 生产案例：sz-pay 支付中台后端依赖 sz-orm-core/sqlx/config/auth/macros/queue 6 个包

---

## 一、整体架构

SZ-ORM 采用**分层 + 插件化**架构：核心层保持零驱动的纯粹抽象，真实 IO 全部下沉到适配器与扩展包，通过 trait 注入。

```
┌────────────────────────────────────────────────────────────────────┐
│                         应用层（用户代码）                           │
│   Model 定义 / QueryBuilder 链式调用 / sql_string! 字面量           │
│   typed_ast 强类型表达式 / DynamicSqlTemplate XML 模板              │
│   FindWithRelated 关联预加载（Join/Eager/Subquery）                  │
└───────────────────────────────┬────────────────────────────────────┘
                                │
┌───────────────────────────────▼────────────────────────────────────┐
│                核心抽象层 sz-orm-core（v7.3 模块清单）                │
│  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌───────────┐ ┌────────────┐  │
│  │ Query   │ │ Dialect  │ │ Pool   │ │Transaction│ │ Migration  │  │
│  │ Builder │ │ 4+方言   │ │+ Conn  │ │ +Manager  │ │ +Schema    │  │
│  └─────────┘ └──────────┘ └────────┘ └───────────┘ └────────────┘  │
│  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌───────────┐                 │
│  │ Model   │ │ Value    │ │ Cache  │ │ Error     │                 │
│  │ +Relation│ │ 22 变体  │ │ 多级   │ │ 4 类错误码│                 │
│  └─────────┘ └──────────┘ └────────┘ └───────────┘                 │
│  ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌─────────────────────────┐ │
│  │ Hooks   │ │typed_ast │ │dynamic_  │ │ json_query              │ │
│  │ 16 事件 │ │ 编译期   │ │sql XML   │ │ JsonQuery/JsonUpdate    │ │
│  │ +Scope  │ │ 类型安全 │ │5 标签    │ │ 三方言映射              │ │
│  │Registry │ │          │ │          │ │                         │ │
│  └─────────┘ └──────────┘ └──────────┘ └─────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │ find_with_related / bloom / tde_interceptor / perf-accel     │  │
│  └──────────────────────────────────────────────────────────────┘  │
│  依赖：sz-orm-sql-validator（运行时校验） sz-orm-macros（编译时宏）  │
└───────────────────────────────┬────────────────────────────────────┘
                                │ Connection / ConnectionFactory trait
┌───────────────────────────────▼────────────────────────────────────┐
│                    适配器层 sz-orm-sqlx                              │
│  MySqlPoolHandle │ PgPoolHandle │ SqlitePoolHandle                  │
│  Sqlx*Connection │ Sqlx*ConnectionFactory │ row_to_value_*          │
│  map_sqlx_error: sqlx::Error → DbError                              │
└───────────────────────────────┬────────────────────────────────────┘
                                │ sqlx 0.9.0 (tokio + rustls)
┌───────────────────────────────▼────────────────────────────────────┐
│                真实数据库：MySQL / PostgreSQL / SQLite / Oracle      │
│                          + MSSQL + CockroachDB + YugabyteDB         │
│                          + Snowflake + Redshift + Informix          │
│                          + SAP HANA + Firebird                      │
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│              扩展生态层（66 个包 + cli + examples，按需引用）          │
│  安全:   crypto auth masking audit sql-validator n1-lint            │
│  可靠:   health tracing back limit dtx(=tcc+cross_shard+saga)       │
│  集成:   mqtt websocket queue storage es ai grpc graphql            │
│  数据:   mig batch rw sharding config postgis timeseries search     │
│  平台:   wasm lc swagger logger scheduler axum actix                │
│  绑定:   cabi go java cpp python js                                 │
│  性能:   explain flamegraph adaptive fusion parallel stream         │
│         advisor diagnosis anomaly bench observability               │
│  AI:     ai ai-designer ai-migration agent governance               │
│         nl-query model-ops multimodal mcp                           │
│  工具:   studio lsp designer query-builder graph vector             │
│         oracle mssql                                                │
│                                                                      │
│  sz-orm-dtx 内部三子模块：                                            │
│    tcc         TccCoordinator / TccParticipant / TccManager          │
│    cross_shard CrossShardCoordinator / ShardOperation                │
│    saga        Saga / SagaStep / SagaManager                         │
└────────────────────────────────────────────────────────────────────┘
```

---

## 二、包间依赖关系

### 2.1 依赖分层

| 层级 | 包 | 依赖 |
|------|----|------|
| L0 基础 | sz-orm-sql-validator | 仅 thiserror（零运行时依赖） |
| L0 基础 | sz-orm-macros | 仅 proc_macro（零外部依赖） |
| L1 核心 | sz-orm-core | sz-orm-sql-validator + sz-orm-macros + tokio/async-trait/thiserror/serde/chrono/bytes |
| L2 适配 | sz-orm-sqlx | sz-orm-core + sqlx 0.9.0 + rust_decimal |
| L2 适配 | sz-orm-oracle | sz-orm-core + oracle crate（ODPI-C binding） |
| L2 适配 | sz-orm-mssql | sz-orm-core + tiberius（TDS 协议） |
| L3 扩展 | 其余 64 包 | 各自独立，仅依赖 tokio/serde/thiserror 等公共库，**不依赖 sz-orm-core**（保持可独立使用） |
| L4 绑定 | sz-orm-cabi | sz-orm-sqlx（FFI 导出） |
| L4 绑定 | sz-orm-go/java/cpp | sz-orm-cabi（cgo/JNI/extern C） |
| L4 绑定 | sz-orm-python | sz-orm-sqlx + pyo3 0.20 |
| L4 绑定 | sz-orm-js | sz-orm-core + napi-rs |

### 2.2 依赖方向原则

```
sz-orm-macros ──┐
                 ▼
sz-orm-sql-validator ──▶ sz-orm-core ◀── sz-orm-sqlx ──▶ sqlx ──▶ 真实 DB
                                              ▲
sz-orm-cabi ◀── sz-orm-go / sz-orm-java / sz-orm-cpp
sz-orm-python（pyo3）/ sz-orm-js（napi-rs）独立绑定
```

- **单向依赖**：扩展包不反向依赖 core，core 不依赖任何数据库驱动（src 中 0 处 sqlx）。
- **trait 注入**：core 定义 `Connection`/`ConnectionFactory` 抽象，sqlx 适配器实现后通过 `Arc<dyn ConnectionFactory>` 注入 `Pool`。
- **feature 隔离**：真实云 SDK 全部通过 `default = []` + feature flag 控制编译：

| 包 | feature | 引入的真实 SDK |
|----|---------|---------------|
| sz-orm-mqtt | `real-broker` | rumqttc 0.25 |
| sz-orm-websocket | `server` | tokio-tungstenite 0.30 |
| sz-orm-queue | `rabbitmq` | lapin 4.10 |
| sz-orm-storage | `s3-sdk` | rust-s3 0.37 |

默认编译保留内存实现，启用 feature 才引入真实 SDK；真实服务测试用 `#[ignore]` 标记，CI 默认不运行。

---

## 三、核心设计决策

### 3.1 sz-orm-core 是"SQL 生成器 + 抽象连接池框架"

- core src 中 **0 处使用 sqlx**，保持纯粹抽象层，可独立作为 SQL 生成器使用。
- `Connection` trait 是异步抽象接口（`Pin<Box<dyn Future>>` 返回类型，v3.0 引入 `QueryRows` type alias 规避 `clippy::type_complexity`）。
- 真实 DB 集成测试用 sqlx/rusqlite **直接执行** dialect 生成的 SQL，验证 SQL 正确性；sz-orm-sqlx 适配器再验证 Pool/Transaction 抽象层的端到端连通。
- 收益：core 可独立审计、独立测试；替换底层驱动不影响上层 API。

### 3.2 双层 SQL 校验（编译时 + 运行时）

| 层 | 组件 | 时机 | 能力 |
|----|------|------|------|
| 编译时 | `sql_string!`（sz-orm-macros） | 编译期 | 语法关键字、括号平衡、字符串闭合、注入模式、参数个数；失败即编译错误 |
| 编译时 | `query!`（sz-orm-macros，db-verify feature） | 编译期 | 连真 DB 执行 EXPLAIN 验证（MySQL/PG/SQLite） |
| 运行时 | `QueryBuilder::validate()`（→ sz-orm-sql-validator） | 运行期 | 动态拼接 SQL 的语法/注入/标识符校验，返回 `Vec<SqlValidationError>` |

决策说明：v4.0 仅有运行时校验，v4.1 新增零依赖 proc macro 补齐编译时校验。宏自包含实现（不依赖 syn/quote），控制编译时间。v7.3.0 `query!` 宏支持连真 DB 验证（需 `DATABASE_URL` + `SZ_ORM_QUERY_VERIFY=1`）。

### 3.3 方言（Dialect）抽象

四种核心方言 + 七种扩展方言实现统一 `Dialect` trait，通过 `get_dialect(DbType)` 工厂获取：

| 数据库 | 占位符 | 分页 | 标识符 | JSON 提取 |
|--------|--------|------|--------|----------|
| MySQL | `?` | `LIMIT n OFFSET m` | `` ` `` | `JSON_EXTRACT` |
| PostgreSQL | `$1, $2...` | `LIMIT n OFFSET m` | `"` | `#>>'{}'` |
| SQLite | `?` | `LIMIT n OFFSET m` | `"` | `json_extract` |
| Oracle 23ai | `:1, :2...` | `OFFSET n ROWS FETCH NEXT m ROWS ONLY` | `"` | `JSON_VALUE` |

扩展方言（feature gate 隔离，默认启用 SQL 生成，无真实 DB 驱动）：

| 方言 | feature | 说明 |
|------|---------|------|
| CockroachDB | `dialect-cockroachdb` | PG 兼容分布式数据库 |
| YugabyteDB | `dialect-yugabytedb` | PG 兼容分布式数据库 |
| Snowflake | `dialect-snowflake` | 云数仓，VARIANT/OBJECT/ARRAY + COPY INTO |
| Redshift | `dialect-redshift` | AWS 云数仓，COPY/UNLOAD 特性 |
| Informix | `dialect-informix` | SERIAL/ROW 类型 + PUT 语句 |
| SAP HANA | `dialect-saphana` | 计算列 + CE 函数 |
| Firebird | `dialect-firebird` | GENERATOR/SEQUENCE + EXECUTE BLOCK |

`DbType` 提供能力查询（`supports_schema/transaction/foreign_key/stored_procedure`、`default_port`），供上层按能力降级。

### 3.4 连接池自研而非直接复用 sqlx::Pool

- `Pool` 面向 `Connection` trait 抽象，任何实现该 trait 的后端都可入池（sqlx 适配器只是其中一种）。
- 完整生命周期管理：`acquire`（带超时 + Notify 唤醒）→ `release` → `reap_idle`（空闲/超龄回收）→ `close_all`（关闭后拒绝新 release）。
- `PoolConfigBuilder::build()` 返回 `Result`，非法配置在构建期暴露。
- v6.7.0 `pool-elastic` feature：动态扩缩容 + 健康检查 + 多级熔断 + 预热。
- v7.3.0 `auto-failover` feature：自动主备故障转移（默认关闭，不改变单库行为）。

### 3.5 错误码体系

统一四类错误（`DbError` DB001–DB020、`PoolError` PL001–PL006、`CacheError` CH001–CH006、`TxError` DTX001–DTX018），每个变体携带唯一错误码，`is_retryable()` 显式标注可重试性。各扩展包拥有独立错误枚举，均基于 thiserror，可统一向上传播。

### 3.6 内存实现 + feature 真实 SDK 的双轨制

- 默认编译为纯内存实现（无网络依赖），保证单测 100% 可离线运行。
- 真实 SDK 实现与内存实现实现**同一 trait**（如 `MessageQueue`、`Storage`），业务代码无感切换。
- 每个真实实现包含：可运行单元测试（解析/状态/构造）+ `#[ignore]` 集成测试。

### 3.7 加密原语统一 RustCrypto 审计栈

sz-orm-crypto 与 sz-orm-auth 均使用 RustCrypto（sha2/hmac/aes-gcm/pbkdf2/subtle/OsRng）+ base64；签名比较用 `constant_time_eq` 防时序侧信道。v7.0.0 `tde-interceptor` feature：TDE 透明加密拦截器（挂钩 before_insert/before_update/after_select），DEK 内存清零用 `zeroize`。

### 3.8 健壮性红线

- 生产代码 **0 处 panic!**（sharding 路由错误改为返回 `Result`）
- 0 处 `unimplemented!`/`todo!`/`FIXME`，所有功能真实实现
- 所有 `unwrap()` 仅在 `#[cfg(test)]` 中
- clippy `-D warnings` 全通过 + fmt 全通过 + cargo-audit/deny 0 未忽略漏洞

### 3.9 细粒度钩子系统（v3.0）

v3.0 将钩子事件从 6 种扩展至 **16 种**，新增 `HookDispatcher` 统一调度，并补充 `GlobalScope`/`ScopeRegistry` 全局查询作用域机制。

- **16 种 HookEvent**：`BeforeWrite/AfterWrite`、`BeforeSave/AfterSave`、`BeforeRestore/AfterRestore`、`BeforeInsert/AfterInsert`、`BeforeUpdate/AfterUpdate`、`BeforeDelete/AfterDelete`、`BeforeFind/AfterFind`、`BeforeValidate/AfterValidate`。
- **HookContext**：builder 模式，携带 `tenant_id`/`operator_id`/`timestamp`/`metadata`，贯穿整条触发链。
- **HookDispatcher 触发顺序**：
  - insert：`before_write → before_save → before_validate → after_validate → before_insert → (INSERT) → after_insert → after_save → after_write`
  - update：与 insert 同序（`before_insert`↔`before_update`、`after_insert`↔`after_update`）
  - delete：`before_delete → (DELETE) → after_delete`
  - restore：`before_restore → (UPDATE deleted_at=NULL) → after_restore`
  - find：`before_find → (SELECT) → after_find`
- **GlobalScope**：trait 不要求 `Model` bound，由元组 `(Scope, M)` 携带具体模型类型。
  - `SoftDeleteScope`：对实现 `SoftDelete` 的模型自动追加 `deleted_at IS NULL`。
  - `TenantScope`：对实现 `TenantModel` 的模型自动追加 `tenant_id = ?`。
- **ScopeRegistry**：维护「作用域名 → 启用状态」表，支持 `disable/enable/without_scope` 三种操作。
- 错误码扩展：`DbError::Hook(DB019)` / `DbError::TenantError(DB020)`。

### 3.10 强类型 AST（typed_ast，v3.0）

把 SQL 表达式抽象成携带列类型信息的 AST，在编译期杜绝类型不匹配的 WHERE 条件。

- **SqlType 标记 trait**：`SqlInt/SqlBigInt/SqlText/SqlBool/SqlReal/SqlDateTime` 等空类型。
- **TypedExpression\<T\> trait**：携带类型参数 `T: SqlType`，提供 `to_sql` 与 `collect_params`。
- **AST 节点**：`ColumnExpr<T>`、`Literal<T>`、`Eq/Ne/Lt/Gt/Le/Ge<L, R, T>`（要求 L、R 同 T）、`And/Or<L, R>`（要求 L、R 均为 `SqlBool`）。
- **TypedSelectQuery\<T\>**：`filter<E: TypedExpression<SqlBool>>` 只接受布尔表达式。
- **零运行时开销**：`PhantomData` 不占空间，最终通过 `to_sql + collect_params` 落到现有 `QueryBuilder`。

### 3.11 动态 SQL（dynamic_sql，v3.0）

在不放弃参数化绑定的前提下，支持条件分支、循环、字符串拼接的动态 SQL 生成，对标 MyBatis XML 模板能力。

- **DynamicSqlTemplate**：从 XML 字符串解析为 `Vec<TemplateNode>` AST，渲染时绑定 `SqlParams` 生成 `(SQL, Vec<Value>)`。
- **占位符严格区分**：`#{name}` 走参数绑定（防注入），`${name}` 走字符串插值（仅限可信白名单标识符）。
- **5 类 XML 标签**：`<if>`、`<where>`、`<set>`、`<foreach>`、`<choose>/<when>/<otherwise>`、`<trim>`。
- 渲染产物 `(String, Vec<Value>)` 直接喂给 `Connection::execute/query`。

### 3.12 JSON 字段查询（json_query，v3.0）

抽象 JSON 字段的查询与更新操作，在三种方言上各自映射到原生 JSON 函数。

- **JsonQuery**：描述 `column + path + op`，由 `to_sql(dialect)` 生成 SQL 表达式 + 参数。
- **JsonUpdate**：批量描述路径赋值、删除、合并、追加。
- **三方言映射**：

| 操作 | MySQL | PostgreSQL | SQLite |
|------|-------|-----------|--------|
| 提取 | `JSON_EXTRACT(col, '$.a.b')` | `col #>> '{a,b}'` | `json_extract(col, '$.a.b')` |
| 包含 | `JSON_CONTAINS(col, ?)` | `col @> ?` | `json_extract(col, '$') LIKE ?`（降级） |
| 长度 | `JSON_LENGTH(col, '$.a')` | `json_array_length(col->'a')` | `json_array_length(json_extract(col, '$.a'))` |
| 设置 | `JSON_SET(col, '$.a', ?)` | `jsonb_set(col, '{a}', ?)` | `json_set(col, '$.a', ?)` |
| 删除 | `JSON_REMOVE(col, '$.a')` | `col - 'a'` | `json_remove(col, '$.a')` |

### 3.13 v7.3.0 四大方向

#### 3.13.1 性能极致优化

| feature | 版本 | 能力 |
|---------|------|------|
| `perf-accel` | v7.3.0 | 性能加速聚合（SIMD + 零拷贝 + 预热 + 计划缓存 + zero-copy-deep） |
| `simd-deep` | v6.8.0 | SIMD 向量化聚合深度（f32/f64/bool 全类型） |
| `zero-copy-deep` | v6.8.0 | 零拷贝结果集传输深化（Bytes 引用计数切片 + 命中/回退统计） |
| `perf-diff` | v6.8.0 | 性能差分正确性测试套件（零拷贝 vs 拷贝、SIMD vs 标量结果一致性） |
| `pool-io-reuse` | v6.8.0 | 连接池 IO 复用（PreparedStatement 通道复用） |
| `io-uring` | v6.8.0 | io_uring 集成评估与降级（非 Linux 自动降级 tokio） |
| `executor-opt` | v6.8.0 | 执行器优化（谓词下推 + 投影裁剪） |
| `olap-vectorized` | v7.1.0 | OLAP 向量化分析查询（聚合下推 + Star Schema 优化 + 物化视图匹配） |

#### 3.13.2 企业级高可用

| feature | 版本 | 能力 |
|---------|------|------|
| `auto-failover` | v7.3.0 | 自动主备故障转移（默认关闭，不改变单库行为） |
| `dist-cache-cluster` | v6.7.0 | 分布式缓存集群（一致性哈希 + 故障转移 + 击穿/穿透/雪崩防护） |
| `rw-split-enhanced` | v6.7.0 | 读写分离/分库分表增强（权重路由 + 延迟回退 + 分片键推断 + 跨片聚合） |
| `zero-downtime-mig` | v6.7.0 | 零停机迁移（expand-contract + 回滚自动化 + 预演 + 兼容性检查） |
| `pool-elastic` | v6.7.0 | 连接池弹性（动态扩缩容 + 健康检查 + 多级熔断 + 预热） |
| `obs-enhanced` | v6.7.0 | 可观测性增强（慢查询 Top-N + 执行计划回归 + Prometheus + 告警） |
| `serverless-adapt` | v7.0.0 | Serverless 适配（冷启动优化 + 优雅缩容 + 请求驱动扩容） |
| `composable-plugin` | v7.0.0 | 可组合性插件系统（panic 安全注册表 + 签名验证 + 中间件链） |
| `cdc-realtime-sync` | v7.1.0 | CDC 实时数据同步（Kafka Sink + Schema 变更适配 + 快照模式） |

#### 3.13.3 AI 深度集成

| 包 | 版本 | 能力 |
|----|------|------|
| sz-orm-ai | v1.0+ | AI 增强基础（NL→SQL + 向量查询） |
| sz-orm-ai-designer | v7.x | LLM 驱动 Schema 设计 + 迁移影响分析 + 反范式建议 |
| sz-orm-ai-migration | v7.x | LLM 驱动迁移脚本生成 + 回滚验证 |
| sz-orm-agent | v7.x | AI Agent 自主数据库操作（perceive-decide-act 循环） |
| sz-orm-governance | v7.x | AI 驱动数据治理（血缘 + 质量 + 合规 + 脱敏） |
| sz-orm-nl-query | v7.x | 自然语言查询管线（NL2SQL → 执行 → 可视化 → 洞察） |
| sz-orm-model-ops | v7.x | AI 模型运维（本地推理 + 路由 + 微调 + 评估） |
| sz-orm-multimodal | v7.x | 多模态数据库交互（语音 + 图表 + ER 图 + 截图） |
| sz-orm-mcp | v7.x | MCP 协议服务器（NL 查询 + SQL 执行作为 AI 工具） |

#### 3.13.4 生态扩展

| 包/feature | 版本 | 能力 |
|------------|------|------|
| sz-orm-studio | v5.1.0 | Web GUI 数据浏览器（axum HTTP server） |
| sz-orm-lsp | v5.1.0 | LSP 服务器（补全 + 悬停 + 诊断，VS Code 扩展） |
| sz-orm-designer | v6.x | 可视化 Schema 设计器（图形化建表/改表/ER 图/双向代码生成） |
| sz-orm-bench | v7.x | 基准套件（sz-orm vs SeaORM vs Diesel vs SQLx，P50/P95/P99） |
| sz-orm-cabi/go/java/cpp/python/js | v5.x | 多语言绑定（C ABI / cgo / JNI / extern C / PyO3 / napi-rs） |
| `eco-config` | v7.3.0 | 生态扩展配置聚合（EcoConfig，默认关闭） |

---

## 四、扩展包开发指南

### 4.1 开发规范

1. **目录结构**：新包放入 `packages/sz-orm-<name>/`，并在根 `Cargo.toml` 的 `workspace.members` 注册。
2. **Cargo.toml 模板**：

```toml
[package]
name = "sz-orm-example"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "SZ-ORM Example Extension"

[features]
default = []                # 真实 SDK 必须 feature 隔离
real-sdk = ["dep:some-sdk"]

[dependencies]
async-trait.workspace = true
tokio = { workspace = true, features = ["full"] }
thiserror.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true

[lints]
workspace = true
```

3. **API 设计**：
   - 定义 trait（`Send + Sync`，异步方法用 `async_trait`）+ 内存实现 + 可选真实实现。
   - 错误类型用 thiserror 派生，命名为 `XxxError`。
   - 配置类型提供 `Default` 与链式 builder 方法。
4. **测试要求**：每包 ≥5 个单元测试；真实服务集成测试用 `#[ignore]` 标记；严禁 `unimplemented!`/`todo!`/空实现。
5. **质量门禁**（提交前必须通过，23 道门禁详见 AGENTS.md）：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit && cargo deny check
```

### 4.2 接入 sz-orm-core 的连接抽象

若扩展包需要操作数据库，实现 core 的两个 trait 即可复用 Pool/Transaction 全套能力：

```rust
use sz_orm_core::{Connection, ConnectionFactory, DbError, QueryRows};
use std::future::Future;
use std::pin::Pin;

struct MyConnection;

impl Connection for MyConnection {
    fn execute<'a>(&'a mut self, sql: &'a str)
        -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>>
    {
        Box::pin(async move { /* 执行写操作，返回影响行数 */ todo!() })
    }
    fn query<'a>(&'a mut self, sql: &'a str)
        -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>>
    {
        Box::pin(async move { /* 执行读操作，返回行集 */ todo!() })
    }
    // begin_transaction / commit / rollback 同理
}

struct MyFactory;
#[sz_orm_core::async_trait]
impl ConnectionFactory for MyFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MyConnection))
    }
}
```

随后 `Pool::new(config, Arc::new(MyFactory))?` 即可获得连接池管理能力。

### 4.3 新增数据库方言

实现 `Dialect` trait（标识符引用、转义、占位符、分页、DDL、JSON 提取、全文搜索、布尔转整数、自增关键字等全部方法），在 `get_dialect()` 工厂中注册新 `DbType` 变体，并补充方言单元测试。

### 4.4 sz-orm-dtx 三子模块架构（v3.0）

sz-orm-dtx v3.0 从单体 2PC 扩展为 3 个并列子模块，覆盖金融场景中三类典型分布式事务模式。模块布局：

```
packages/sz-orm-dtx/src/
├── lib.rs              # 统一入口：re-export tcc / cross_shard / saga
├── error.rs            # TxError（DTX001–DTX018）
├── tcc/                # TCC 子模块
│   ├── coordinator.rs  # TccCoordinator
│   ├── participant.rs  # TccParticipant + TccState 状态机
│   └── manager.rs      # TccManager 全局事务管理 + 异常恢复
├── cross_shard/        # 跨分片 2PC 子模块
│   ├── coordinator.rs  # CrossShardCoordinator
│   ├── shard_op.rs     # ShardOperation 单分片操作
│   └── grouping.rs     # 按 shard_id 分组合并
└── saga/               # Saga 长流程补偿子模块
    ├── step.rs         # SagaStep（action + compensation）
    ├── state.rs        # SagaState 状态机
    └── manager.rs      # SagaManager
```

#### 4.4.1 tcc 子模块

TCC（Try-Confirm-Cancel）适合资金扣减、库存锁定等需要强隔离的场景。

- **TccState 状态机**：`Init → Trying → Tried → Confirming → Confirmed`（成功路径）/ `Cancelling → Cancelled`（补偿路径）/ `Failed`（不可恢复终态）。
- **TccParticipant**：持有 `try_fn`/`confirm_fn`/`cancel_fn` 三个闭包，全部 `Send + Sync`，`confirm`/`cancel` 必须幂等。
- **TccCoordinator**：`try_phase`（任一失败 → 全量 Cancel）→ `confirm_phase`（全部 Confirm，失败重试）。
- **TccManager**：持久化 `global_tx` 状态，定时扫描悬挂事务并驱动 `recover()` 重放。

#### 4.4.2 cross_shard 子模块

跨分片 2PC 适合分库写入、跨分片转账等需要强一致的场景。

- **ShardOperation**：单分片上的 `prepare_fn`/`commit_fn`/`rollback_fn` 三回调封装。
- **CrossShardCoordinator**：
  - `prepare`：并行向所有分片发 prepare；任一失败 → 全量 rollback。
  - `commit`/`rollback`：所有分片统一执行；失败重试，幂等。
  - `group_by_shard`：把多个 ShardOperation 按 `shard_id` 分组合并。

#### 4.4.3 saga 子模块

Saga 适合订单、旅行预订、跨服务编排等长流程业务，无锁高吞吐。

- **SagaStep**：`action`（正向）+ `compensation`（反向）一对闭包。
- **SagaState 状态机**：`New → Running → Completed`（成功路径）/ `Compensating → Compensated`（补偿路径）/ `CompensationFailed`（需人工介入终态）。
- **Saga**：正向执行所有 `step.action`；任一失败进入 `Compensating`，反向执行已完成 step 的 `compensation`。
- **SagaManager**：持久化进度到 `SagaLogStore`，支持断点续跑。

#### 4.4.4 三种模型对比

| 维度 | TCC | CrossShard 2PC | Saga |
|------|-----|----------------|------|
| 隔离性 | 强（Try 阶段资源预留） | 强（prepare 阶段持锁） | 弱（中间状态可见） |
| 一致性 | 最终一致 | 强一致 | 最终一致（补偿回滚） |
| 复杂度 | 高（三套闭包） | 中（依赖分片 XA） | 中（action + compensation） |
| 性能 | 中（3 次 RTT） | 低（持锁周期长） | 高（无锁） |
| 适用场景 | 资金扣减、库存锁定 | 跨分片转账、分库写入 | 订单/旅行预订/跨服务编排 |

兼容性：旧版 `DistributedTransaction` API 作为 `cross_shard::CrossShardCoordinator` 的语义别名保留，老用户代码零改动升级。

### 4.5 v7.3.0 新增 feature gate 速查

以下 feature gate 均在 sz-orm-core 中定义，默认关闭（`perf-accel` 除外，已加入 default），按需启用：

| feature | 版本 | 依赖 | 说明 |
|---------|------|------|------|
| `perf-accel` | v7.3.0 | simd+zero-copy+auto-prewarm+plan-cache+zero-copy-deep | 性能加速聚合（**默认启用**） |
| `auto-failover` | v7.3.0 | rw-split-enhanced | 自动主备故障转移 |
| `eco-config` | v7.3.0 | — | 生态扩展配置聚合（EcoConfig） |
| `tde-interceptor` | v7.0.0 | field-encryption+sz-orm-crypto | TDE 透明加密拦截器 |
| `serverless-adapt` | v7.0.0 | pool-elastic+auto-prewarm | Serverless 冷启动优化 |
| `composable-plugin` | v7.0.0 | sz-orm-crypto | 可组合性插件系统 |
| `cdc-realtime-sync` | v7.1.0 | — | CDC 实时数据同步（Kafka Sink） |
| `rbac-abac-enhanced` | v7.1.0 | multi-tenant-enhanced | RBAC + ABAC 权限模型增强 |
| `olap-vectorized` | v7.1.0 | zero-copy+simd+executor-opt+query-result-cache+rw-split-enhanced | OLAP 向量化分析查询 |
| `simd-deep` | v6.8.0 | — | SIMD 向量化聚合深度 |
| `zero-copy-deep` | v6.8.0 | zero-copy | 零拷贝结果集传输深化 |
| `perf-diff` | v6.8.0 | zero-copy-deep+simd | 性能差分正确性测试套件 |
| `cdc-mysql` | v6.8.0 | sqlx+dist-cache-cluster+masking+search | CDC MySQL Binlog 变更捕获 |
| `cdc-postgres` | v6.8.0 | sqlx | CDC PostgreSQL WAL 变更捕获 |
| `cdc-sqlite` | v6.8.0 | sqlx | CDC SQLite update-hook 变更捕获 |
| `executor-opt` | v6.8.0 | sqlparser | 执行器优化（谓词下推 + 投影裁剪） |
| `pool-io-reuse` | v6.8.0 | prepared-stmt-cache+pool-elastic | 连接池 IO 复用 |
| `io-uring` | v6.8.0 | zero-copy-deep | io_uring 集成（非 Linux 自动降级） |
| `dist-cache-cluster` | v6.7.0 | dist-cache+l1-cache | 分布式缓存集群 |
| `rw-split-enhanced` | v6.7.0 | rand | 读写分离/分库分表增强 |
| `zero-downtime-mig` | v6.7.0 | — | 零停机迁移 |
| `pool-elastic` | v6.7.0 | circuit-breaker+auto-prewarm | 连接池弹性 |
| `obs-enhanced` | v6.7.0 | — | 可观测性增强 |
| `field-encryption` | v6.7.0 | sz-orm-crypto | 字段级加密（AES-GCM + 密钥轮换） |
| `anomaly-detection` | v4.9.0 | sz-orm-anomaly | 异常检测（指标采集 + 滑动窗口 + 突增/耗尽检测） |
| `n1-lint` | v4.3.0 | sz-orm-macros/n1-lint | N+1 静态检测标注宏 `#[detect_n_plus_one]` |
| `compile-governance` | v4.3.0 | sz-orm-macros/governance-derive | 编译期数据治理（PII 标注强制） |
| `zero-downtime-rollback` | v4.6.0 | — | 迁移回滚自动化（影子表 + 反向 DDL） |
| `forward-compat-sandbox` | v4.7.0 | zero-downtime-rollback+migration-dry-run | 前向兼容性检查与沙箱预演 |
| `tenant-quota-rls-enhanced` | v4.7.0 | connection-level-tenant | 租户资源配额与行级安全增强 |

---

## 五、验证体系架构（七线验证）

| 验证线 | 位置 | 规模 | 目的 |
|--------|------|------|------|
| TDD 单元 | 各包 `#[cfg(test)]` + `tests/core.rs` | core 99+ + 各扩展包 | 逻辑正确性 |
| 集成 | `integration_sqlite/mysql/pg.rs` | SQLite 11 + MySQL/PG 各 12（ignored） | 真实 DB SQL 正确性 |
| Jepsen | `tests/jepsen.rs` + sqlx 包 `real_db_jepsen.rs` | 29 mock + 10 真实 DB | 并发正确性 |
| Fuzz | `tests/fuzz.rs` | 11 | 边界/注入发现 |
| Stress | `tests/stress.rs` + 各包 stress | 77+ | 性能回归 |
| Chaos | `tests/chaos.rs` | 16 | 故障鲁棒性（网络分区/磁盘满/时钟漂移/主从切换） |
| Formal | `tests/formal.rs` + `docs/tla/` | 14 + TLA+ 规约 | 不变量验证 |

v4.9.0 新增 OWASP Top 10 完整覆盖渗透测试套件（`--features owasp-pentest-suite`，85 个测试覆盖 A01~A10 + XSS/CSRF/文件上传/竞态）。

---

## 六、版本历史

| 版本 | 日期 | 更新内容 |
|------|------|----------|
| v1.0 | 2026-07-18 | SZ-ORM：核心 ORM + 27 个可选扩展包 |
| v2.0 | 2026-07-19 | hooks 钩子模块 + cli + examples；工作空间成员 33 |
| v3.0 | 2026-07-19 | core 新增 typed_ast/dynamic_sql/json_query/find_with_related；hooks 16 事件 + HookDispatcher + GlobalScope/ScopeRegistry；dtx 扩展为 tcc/cross_shard/saga 三子模块 |
| v4.0 | 2026-07-20 | 39 包 / AI 增强（vector + NL→SQL）/ 工程化审计三门禁通过 |
| v5.0 | 2026-07-21 | sqlx 0.9.0 升级 / v1.0.0 正式发布 / MSRV 1.94.0+ / rsa Marvin Attack 已消除 |
| v6.0 | 2026-07-29 | 43 包 / 修复 SQL 注入与 panic! 健壮性问题 / 软删除与多租户集成到 lambda.rs |
| v3.3.0 | 2026-08-08 | 分布式缓存一致性 + GraphQL + 多租户 + AI NL 查询增强 |
| v3.4.0 | 2026-08-09 | 测试覆盖补齐 + 编译期类型安全 + sz-pay 生产案例（7 包/297 引用） |
| v4.3.0 | — | 新增 explain/flamegraph/adaptive/fusion/n1-lint 5 包 |
| v4.4.0 | — | 新增 advisor/diagnosis 2 包 |
| v4.5.0 | — | 新增 parallel/stream 2 包 |
| v4.6.0 | — | 不新增包，7 个 feature gate 扩展既有包 |
| v4.7.0 | — | 不新增包，7 个 feature gate 扩展既有包 |
| v4.9.0 | — | OWASP Top 10 完整覆盖渗透测试套件（85 测试 + A06 脚本）；新增 anomaly 包 |
| v5.0.0 | — | 新增 graph/postgis/timeseries/search/vector/oracle/mssql/axum/actix/query-builder/observability 11 包 |
| v5.1.0 | — | 新增 studio/lsp 2 包 |
| v6.5.0 | — | 异步并行查询 + 查询计划缓存 + 流式结果集 |
| v6.6.0 | — | 查询结果缓存 + Saga 分布式事务 + 多租户隔离 + AI 查询优化（20 任务，45 测试） |
| v6.7.0 | — | 分布式缓存集群 + 读写分离增强 + 零停机迁移 + 连接池弹性 + 可观测性增强 + 安全合规增强（32 任务，87 测试） |
| v6.8.0 | — | SIMD 向量化深化 + 零拷贝深化 + CDC（MySQL/PG/SQLite）+ 执行器优化 + io_uring 评估 |
| v7.0.0 | — | TDE 透明加密 + Serverless 适配 + 可组合性插件系统 |
| v7.1.0 | — | CDC 实时数据同步 + RBAC/ABAC 权限增强 + OLAP 向量化分析查询 |
| v7.3.0 | 2026-09-17 | 性能加速聚合（perf-accel）+ 自动主备故障转移（auto-failover）+ 生态扩展配置聚合（eco-config）/ 70 lib + cli + examples / 176,709 LOC |

---

*项目名称：SZ-ORM（鲜视达 ORM）*
*定位：纯 ORM + 可选扩展包（用户按需引入，不强制安装）*
*文档版本：v7.3.0 | crate 版本：7.3.0 | 更新日期：2026-09-17*
*工作空间：72 成员（70 lib + cli + examples）| 代码：176,709 LOC（src/ 97,318 + tests/ 79,391）*
*成熟度：生产可用（内部项目 sz-pay 6 包引用），sz-orm-core 已发布 crates.io*
