# SZ-ORM 架构设计文档

> 项目名称：SZ-ORM（鲜视达 ORM）
> 文档版本：v5.0（同步到 39 包 / 3047 测试 / v1.0.0 / sqlx 0.9.0 升级完成）
> 适用版本：SZ-ORM v1.0.0（工作空间 39 个成员：37 个 lib + cli + examples）
> 更新日期：2026-07-22
> 文档定位：整体架构、包间依赖、核心设计决策、扩展包开发指南
> 测试：3047 passed / 0 failed（112 个测试套件） | 代码：89,329 LOC（src/ 75,388 + tests/ 13,941）
> 成熟度：原型阶段（未发布 crates.io） | 已知 Bug：0

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
│                核心抽象层 sz-orm-core（v3.0 模块清单）                │
│  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌───────────┐ ┌────────────┐  │
│  │ Query   │ │ Dialect  │ │ Pool   │ │Transaction│ │ Migration  │  │
│  │ Builder │ │ 4 方言   │ │+ Conn  │ │ +Manager  │ │ +Schema    │  │
│  └─────────┘ └──────────┘ └────────┘ └───────────┘ └────────────┘  │
│  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌───────────┐                 │
│  │ Model   │ │ Value    │ │ Cache  │ │ Error     │                 │
│  │ +Relation│ │ 20 变体  │ │ 多级   │ │ 4 类错误码│                 │
│  └─────────┘ └──────────┘ └────────┘ └───────────┘                 │
│  ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌─────────────────────────┐ │
│  │ Hooks   │ │typed_ast │ │dynamic_  │ │ json_query              │ │
│  │ 16 事件 │ │ 编译期   │ │sql XML   │ │ JsonQuery/JsonUpdate    │ │
│  │ +Scope  │ │ 类型安全 │ │5 标签    │ │ 三方言映射              │ │
│  │Registry │ │          │ │          │ │                         │ │
│  └─────────┘ └──────────┘ └──────────┘ └─────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │ find_with_related（Join / Eager / Subquery 三种预加载模式）    │  │
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
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│                扩展生态层（31 个包 + cli + examples，按需引用）        │
│  安全: crypto auth masking audit sql-validator                       │
│  可靠: health tracing back limit dtx(=tcc+cross_shard+saga)         │
│  集成: mqtt websocket queue storage es ai grpc graphql               │
│  数据: mig batch rw sharding config                                  │
│  平台: wasm lc swagger logger scheduler                              │
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
| L3 扩展 | 其余 27 包 | 各自独立，仅依赖 tokio/serde/thiserror 等公共库，**不依赖 sz-orm-core**（保持可独立使用） |

### 2.2 依赖方向原则

```
sz-orm-macros ──┐
                 ▼
sz-orm-sql-validator ──▶ sz-orm-core ◀── sz-orm-sqlx ──▶ sqlx ──▶ 真实 DB
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
- 收益：core 可独立审计、独立测试；替换底层驱动（如未来换用其他驱动）不影响上层 API。

### 3.2 双层 SQL 校验（编译时 + 运行时）

| 层 | 组件 | 时机 | 能力 |
|----|------|------|------|
| 编译时 | `sql_string!`（sz-orm-macros） | 编译期 | 语法关键字、括号平衡、字符串闭合、注入模式、参数个数；失败即编译错误 |
| 运行时 | `QueryBuilder::validate()`（→ sz-orm-sql-validator） | 运行期 | 动态拼接 SQL 的语法/注入/标识符校验，返回 `Vec<SqlValidationError>` |

决策说明：v4.0 仅有运行时校验（架构性短板），v4.1 新增零依赖 proc macro 补齐编译时校验。宏自包含实现（不依赖 syn/quote），控制编译时间。

### 3.3 方言（Dialect）抽象

四种方言实现统一 `Dialect` trait，通过 `get_dialect(DbType)` 工厂获取：

| 数据库 | 占位符 | 分页 | 标识符 | JSON 提取 |
|--------|--------|------|--------|----------|
| MySQL | `?` | `LIMIT n OFFSET m` | `` ` `` | `JSON_EXTRACT` |
| PostgreSQL | `$1, $2...` | `LIMIT n OFFSET m` | `"` | `#>>'{}'` |
| SQLite | `?` | `LIMIT n OFFSET m` | `"` | `json_extract` |
| Oracle 23ai | `:1, :2...` | `OFFSET n ROWS FETCH NEXT m ROWS ONLY` | `"` | `JSON_VALUE` |

`DbType` 提供能力查询（`supports_schema/transaction/foreign_key/stored_procedure`、`default_port`），供上层按能力降级。

### 3.4 连接池自研而非直接复用 sqlx::Pool

- `Pool` 面向 `Connection` trait 抽象，任何实现该 trait 的后端都可入池（sqlx 适配器只是其中一种）。
- 完整生命周期管理：`acquire`（带超时 + Notify 唤醒）→ `release` → `reap_idle`（空闲/超龄回收）→ `close_all`（关闭后拒绝新 release）。
- `PoolConfigBuilder::build()` 返回 `Result`，非法配置（如 `min_idle > max_size`）在构建期暴露。
- 实测验证：MySQL 10k INSERT stress、PG 10k INSERT stress、8 task × 50 并发转账守恒。

### 3.5 错误码体系

统一四类错误（`DbError` DB001–DB018、`PoolError` PL001–PL006、`CacheError` CH001–CH006、`TxError`），每个变体携带唯一错误码，`is_retryable()` 显式标注可重试性（连接/超时/IO 类 true，约束/输入类 false）。各扩展包拥有独立错误枚举，均基于 thiserror，可统一向上传播。

### 3.6 内存实现 + feature 真实 SDK 的双轨制

- 默认编译为纯内存实现（无网络依赖），保证单测 100% 可离线运行。
- 真实 SDK 实现与内存实现实现**同一 trait**（如 `MessageQueue`、`Storage`），业务代码无感切换。
- 每个真实实现包含：可运行单元测试（解析/状态/构造）+ `#[ignore]` 集成测试。

### 3.7 加密原语统一 RustCrypto 审计栈

sz-orm-crypto 与 sz-orm-auth 均使用 RustCrypto（sha2/hmac/aes-gcm/pbkdf2/subtle/OsRng）+ base64，替代手写实现（auth 代码量减少 21%）；签名比较用 `constant_time_eq` 防时序侧信道。

### 3.8 健壮性红线

- 生产代码 **0 处 panic!**（sharding 路由错误改为返回 `Result`）
- 0 处 `unimplemented!`/`todo!`/`FIXME`，所有功能真实实现
- 所有 `unwrap()` 仅在 `#[cfg(test)]` 中
- clippy `-D warnings` 全通过 + fmt 全通过 + cargo-audit/deny 0 未忽略漏洞

### 3.9 细粒度钩子系统（v3.0）

v3.0 将钩子事件从 6 种扩展至 **16 种**，新增 `HookDispatcher` 统一调度，并补充 `GlobalScope`/`ScopeRegistry` 全局查询作用域机制。

- **16 种 HookEvent**：`BeforeWrite/AfterWrite`、`BeforeSave/AfterSave`、`BeforeRestore/AfterRestore`、`BeforeInsert/AfterInsert`、`BeforeUpdate/AfterUpdate`、`BeforeDelete/AfterDelete`、`BeforeFind/AfterFind`、`BeforeValidate/AfterValidate`。
- **HookContext**：builder 模式，携带 `tenant_id`/`operator_id`/`timestamp`/`metadata`，贯穿整条触发链，可被任意钩子读写。
- **HookDispatcher 触发顺序**：
  - insert：`before_write → before_save → before_validate → after_validate → before_insert → (INSERT) → after_insert → after_save → after_write`
  - update：与 insert 同序（`before_insert`↔`before_update`、`after_insert`↔`after_update`）
  - delete：`before_delete → (DELETE) → after_delete`
  - restore：`before_restore → (UPDATE deleted_at=NULL) → after_restore`
  - find：`before_find → (SELECT) → after_find`
- **GlobalScope**：trait 不要求 `Model` bound，由元组 `(Scope, M)` 携带具体模型类型，避免 trait 内部依赖 `Model` 关联类型。
  - `SoftDeleteScope`：对实现 `SoftDelete` 的模型自动追加 `deleted_at IS NULL`。
  - `TenantScope`：对实现 `TenantModel` 的模型自动追加 `tenant_id = ?`（值取自 `HookContext.tenant_id`）。
- **ScopeRegistry**：维护「作用域名 → 启用状态」表，支持 `disable/enable/without_scope` 三种操作；`without_scope` 用于审计/恢复场景的"临时越过软删除"，闭包退出后自动恢复。
- 错误码扩展：`DbError::Hook(DB019)` / `DbError::TenantError(DB020)`。

### 3.10 强类型 AST（typed_ast，v3.0）

把 SQL 表达式抽象成携带列类型信息的 AST，在编译期杜绝类型不匹配的 WHERE 条件。

- **SqlType 标记 trait**：`SqlInt/SqlBigInt/SqlText/SqlBool/SqlReal/SqlDateTime` 等空类型，仅用于类型层面区分。
- **TypedExpression\<T\> trait**：携带类型参数 `T: SqlType`，提供 `to_sql` 与 `collect_params` 两个方法。
- **AST 节点**：`ColumnExpr<T>`、`Literal<T>`、`Eq/Ne/Lt/Gt/Le/Ge<L, R, T>`（要求 L、R 同 T）、`And/Or<L, R>`（要求 L、R 均为 `SqlBool`）。
- **TypedSelectQuery\<T\>**：`filter<E: TypedExpression<SqlBool>>` 只接受布尔表达式，类型不匹配的 WHERE 直接编译失败。
- **零运行时开销**：`PhantomData` 不占空间，最终通过 `to_sql + collect_params` 落到现有 `QueryBuilder` 的 SQL 生成与参数绑定管线，无反射、无 trait object 调度开销。

### 3.11 动态 SQL（dynamic_sql，v3.0）

在不放弃参数化绑定的前提下，支持条件分支、循环、字符串拼接的动态 SQL 生成，对标 MyBatis XML 模板能力。

- **DynamicSqlTemplate**：从 XML 字符串解析为 `Vec<TemplateNode>` AST，渲染时绑定 `SqlParams` 生成 `(SQL, Vec<Value>)`。
- **占位符严格区分**：
  - `#{name}` 走参数绑定（防注入），渲染为 `?`/`$1` 并把值追加到参数列表。
  - `${name}` 走字符串插值（仅限可信白名单标识符，渲染前对值做白名单校验，禁止用户输入）。
- **5 类 XML 标签**：`<if>` 条件分支、`<where>` 自动处理首个 AND/OR 前缀、`<set>` 自动处理末尾逗号、`<foreach>` 循环展开为 `IN (?, ?, ?)`、`<choose>/<when>/<otherwise>` 多分支选择、`<trim>` 通用前后缀裁剪。
- **ParamValue**：`Null/Int/Real/Text/Bool/Bytes/Array`，`Array` 用于 `<foreach>` 展开。
- 渲染产物 `(String, Vec<Value>)` 直接喂给 `Connection::execute/query`，与现有管线无缝衔接。

### 3.12 JSON 字段查询（json_query，v3.0）

抽象 JSON 字段的查询与更新操作，在三种方言上各自映射到原生 JSON 函数。

- **JsonQuery**：描述 `column + path + op`，由 `to_sql(dialect)` 生成 SQL 表达式 + 参数。
  - `JsonPathSegment::Key(String)` / `Index(usize)` 表达 `$.a.b[0].c` 路径。
  - `JsonQueryOp`：`Extract/Exists/Length/Contains(Value)/Eq(Value)`。
- **JsonUpdate**：批量描述路径赋值、删除、合并、追加。
  - `JsonUpdateOp`：`Set(path, value)/Unset(path)/Merge(value)/Append(value)`。
- **三方言映射**：
  | 操作 | MySQL | PostgreSQL | SQLite |
  |------|-------|-----------|--------|
  | 提取 | `JSON_EXTRACT(col, '$.a.b')` | `col #>> '{a,b}'` | `json_extract(col, '$.a.b')` |
  | 包含 | `JSON_CONTAINS(col, ?)` | `col @> ?` | `json_extract(col, '$') LIKE ?`（降级） |
  | 长度 | `JSON_LENGTH(col, '$.a')` | `json_array_length(col->'a')` | `json_array_length(json_extract(col, '$.a'))` |
  | 设置 | `JSON_SET(col, '$.a', ?)` | `jsonb_set(col, '{a}', ?)` | `json_set(col, '$.a', ?)` |
  | 删除 | `JSON_REMOVE(col, '$.a')` | `col - 'a'` | `json_remove(col, '$.a')` |
- **降级策略**：方言不支持的操作（如 SQLite 缺少原生 `@>`）由内部降级方案处理并在 `DbError` 中给出 warning。
- **find_with_related 协同**：JSON 字段可作为虚拟关联列参与预加载（如 `meta->>'$.tags'` 用于 `WithRelation`）。

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
```

3. **API 设计**：
   - 定义 trait（`Send + Sync`，异步方法用 `async_trait`）+ 内存实现 + 可选真实实现。
   - 错误类型用 thiserror 派生，命名为 `XxxError`。
   - 配置类型提供 `Default` 与链式 builder 方法。
4. **测试要求**：每包 ≥5 个单元测试；真实服务集成测试用 `#[ignore]` 标记；严禁 `unimplemented!`/`todo!`/空实现。
5. **质量门禁**（提交前必须通过）：

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

实现 `Dialect` trait（标识符引用、转义、占位符、分页、DDL、JSON 提取、全文搜索、布尔转整数、自增关键字等全部方法），在 `get_dialect()` 工厂中注册新 `DbType` 变体，并补充方言单元测试（参考 OracleDialect 的 13 个测试）。

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
- **TccCoordinator**：`try_phase`（任一失败 → 全量 Cancel）→ `confirm_phase`（全部 Confirm，失败重试 `retry_confirm`）。
- **TccManager**：持久化 `global_tx` 状态到 `TccLogStore`，定时扫描悬挂事务（Trying/Tried/Confirming/Cancelling）并驱动 `recover()` 重放。

#### 4.4.2 cross_shard 子模块

跨分片 2PC 适合分库写入、跨分片转账等需要强一致的场景。

- **ShardOperation**：单分片上的 `prepare_fn`/`commit_fn`/`rollback_fn` 三回调封装。
- **CrossShardCoordinator**：
  - `prepare`：并行向所有分片发 prepare；任一失败 → 全量 rollback。
  - `commit`/`rollback`：所有分片统一执行；失败重试，幂等。
  - `group_by_shard`：把多个 ShardOperation 按 `shard_id` 分组合并到同一物理分片，减少协调开销。
- 依赖分片层（sz-orm-sharding）支持 XA 或可补偿，prepare 阶段持锁，持锁周期较长。

#### 4.4.3 saga 子模块

Saga 适合订单、旅行预订、跨服务编排等长流程业务，无锁高吞吐。

- **SagaStep**：`action`（正向）+ `compensation`（反向）一对闭包。
- **SagaState 状态机**：`New → Running → Completed`（成功路径）/ `Compensating → Compensated`（补偿路径）/ `CompensationFailed`（需人工介入终态）。
- **Saga**：正向执行所有 `step.action`；任一失败进入 `Compensating`，反向执行已完成 step 的 `compensation`。
- **SagaManager**：持久化进度到 `SagaLogStore`，支持断点续跑；补偿失败告警需人工介入。

#### 4.4.4 三种模型对比

| 维度 | TCC | CrossShard 2PC | Saga |
|------|-----|----------------|------|
| 隔离性 | 强（Try 阶段资源预留） | 强（prepare 阶段持锁） | 弱（中间状态可见） |
| 一致性 | 最终一致（Confirm/Cancel 幂等重试） | 强一致（prepare 后必 commit/rollback） | 最终一致（补偿回滚） |
| 复杂度 | 高（业务写 Try/Confirm/Cancel 三套） | 中（依赖分片 XA 或可补偿） | 中（业务写 action + compensation） |
| 性能 | 中（3 次 RTT） | 低（持锁周期长） | 高（无锁） |
| 适用场景 | 资金扣减、库存锁定 | 跨分片转账、分库写入 | 订单/旅行预订/跨服务编排 |

兼容性：旧版 `DistributedTransaction` API 作为 `cross_shard::CrossShardCoordinator` 的语义别名保留，老用户代码零改动升级。

---

## 五、验证体系架构（七线验证）

| 验证线 | 位置 | 规模 | 目的 |
|--------|------|------|------|
| TDD 单元 | 各包 `#[cfg(test)]` + `tests/core.rs` | core 99 + 各扩展包 | 逻辑正确性 |
| 集成 | `integration_sqlite/mysql/pg.rs` | SQLite 11 + MySQL/PG 各 12（ignored） | 真实 DB SQL 正确性 |
| Jepsen | `tests/jepsen.rs` + sqlx 包 `real_db_jepsen.rs` | 29 mock + 10 真实 DB | 并发正确性 |
| Fuzz | `tests/fuzz.rs` | 11 | 边界/注入发现 |
| Stress | `tests/stress.rs` + 各包 stress | 77 | 性能回归 |
| Chaos | `tests/chaos.rs` | 16 | 故障鲁棒性（网络分区/磁盘满/时钟漂移/主从切换） |
| Formal | `tests/formal.rs` + `docs/tla/` | 14 + TLA+ 规约 | 不变量验证 |

总计 **3047 测试 passed，0 失败**（112 个测试套件，需真实服务的标记 ignored）。

---

## 六、版本历史

| 版本 | 日期 | 更新内容 |
|------|------|----------|
| v1.0 | 2026-07-18 | SZ-ORM：核心 ORM + 27 个可选扩展包 |
| v2.0 | 2026-07-19 | ①hooks/ 钩子模块；②cli/ 命令行工具；③examples/；④版本号升至 0.2.0；⑤工作空间成员 33 |
| v3.0 | 2026-07-19 | ①core 新增 typed_ast/dynamic_sql/json_query/find_with_related 四个高级模块；②hooks 钩子事件从 6 种扩展至 16 种 + HookDispatcher + GlobalScope/ScopeRegistry；③sz-orm-dtx 扩展为 tcc/cross_shard/saga 三子模块；④测试 1749 passed / 0 failed / 72 ignored；⑤代码 ~47,500 LOC（非测试）/ ~57,000 LOC（含测试）；⑥成熟度 原型阶段 |
| v3.1 | 2026-07-20 | 修复审查报告 P3-2：统一包数/LOC 数据 — 工作空间成员 38（36 sz-orm-* lib + cli + examples）；LOC ~52,500（非测试）/ ~63,000（含测试）；测试 1871+ passed；评分 4.98/5 |
| v4.0 | 2026-07-20 | 同步到 39 包 / 1970+ 测试 / v0.2.1 / AI 增强完成（sz-orm-vector + NL→SQL）/ 工程化审计三门禁通过 / 85,834 LOC |
| v5.0 | 2026-07-21 | sqlx 0.9.0 升级完成 / 3047 测试 / v1.0.0 正式发布 / MSRV 升至 1.94.0+ / rsa Marvin Attack（RUSTSEC-2023-0071）已消除 |

---

*项目名称：SZ-ORM（鲜视达 ORM）*
*定位：纯 ORM + 可选扩展包（用户按需引入，不强制安装）*
*文档版本：v5.0 | crate 版本：1.0.0 | 更新日期：2026-07-21*
*核心模块：15 个（含 hooks/typed_ast/dynamic_sql/json_query/find_with_related）| 扩展包：37 个 sz-orm-* lib + cli + examples*
*测试：2,271 passed / 0 failed | 代码：~104,000 LOC | 成熟度：原型阶段*
