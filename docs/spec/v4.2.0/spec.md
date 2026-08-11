# sz-orm v4.2.0 需求规格说明书

> 版本：v4.2.0（跨语言分布式事务 + Go/Java/C++ 绑定 + 可视化 Schema 设计器 + OpenAPI → ORM 反向生成 + WASM 真实数据库连接）
> 基线：v4.1.0（数据 seeding/fixture + schema diff 可视化 + 缓存一致性协议 + 消息轨迹追踪 + 存储生命周期管理 + 数据质量自动检测 + 批量流式处理 + 迁移版本分支 + 备份验证自动化，9 项能力全部通过 feature gate 隔离）
> 日期：2026-08-11
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：五项任务按"P1（跨语言分布式事务，微服务互操作核心）→ P2（Go/Java/C++ 绑定 + 可视化 Schema 设计器 + OpenAPI 反向生成，跨语言生态扩展 + 低代码 + API 优先）→ P3（WASM 真实数据库连接，浏览器端 ORM 探索）"序推进
> 需求编号约定：REQ-V42-xxx（v4.2.0 需求项，REQ-V42-001 ~ REQ-V42-005）
> 缺陷来源：`docs/sz-orm与同类产品对比分析.md` 6.3 节长期规划方向（跨语言事务/多语言绑定/低代码设计器/API 优先开发流/浏览器端 ORM）
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.1.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦对比分析 6.3 节五项长期规划方向；更长期（v4.x+ Informix 真实驱动/社区生态扩展/多语言分布式事务协议标准化）在后续版本规划；crates.io 全包发布沿用既有计划，本版本不涉及发布流程变更

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.2.0 的五项跨语言与低代码扩展能力：(1) 跨语言分布式事务（扩展既有 `sz-orm-dtx` 的 Saga/TCC/XA 协调器，支持 Go/Java/C++/Python/JS 等异构语言服务作为事务参与者，通过 gRPC/HTTP 标准协议接入）；(2) Go/Java/C++ 绑定（参照既有 `sz-orm-python`（PyO3）与 `sz-orm-js`（napi-rs）模式，新增 Go（cgo/FFI）、Java（JNI）、C++（extern "C" + cxxbindgen）三套语言绑定，暴露 Model/QueryBuilder/Pool/Transaction 核心 API）；(3) 可视化 Schema 设计器（低代码图形化建表/改表/关系设计，生成迁移文件与实体代码，Web UI 输出）；(4) OpenAPI → ORM 反向生成（从 OpenAPI 3.0 spec 的 components.schemas 反向生成实体 Model 代码 + 迁移文件 + Repository 代码，与既有正向 `sz-orm-swagger` 形成 API 优先开发闭环）；(5) WASM 真实数据库连接（扩展既有 `sz-orm-wasm`，从仅内存数据库/浏览器本地存储升级为通过 HTTP/WebSocket 代理桥接后端真实数据库，或 WASI preview2 socket 直连）。所有能力通过 feature gate 隔离，不破坏现有 API 兼容性与 v4.1.0 已验收基线。

## 1.2 核心输入

1. **v4.1.0 已验收基线**：数据 seeding/fixture + schema diff 可视化 + 缓存一致性协议 + 消息轨迹追踪 + 存储生命周期管理 + 数据质量自动检测 + 批量流式处理 + 迁移版本分支 + 备份验证自动化，9 项能力全部通过 feature gate 隔离，作为本版本基准。
2. **对比分析文档 6.3 节长期规划方向**：`docs/sz-orm与同类产品对比分析.md` 6.3 节识别的五项长期方向（P1×1 + P2×3 + P3×1）。
3. **现有能力清单与缺口证据**：
   - **跨语言分布式事务**：`packages/sz-orm-dtx/src/lib.rs:19` `cross_shard`（跨分片事务）、`:20` `saga`（Saga 长事务编排）、`:21` `tcc`（TCC 三阶段提交）、`:25` `recovery`（XA 崩溃恢复，feature 隔离）、`:27` `suspension`（XA 悬挂检测）、`:29` `xa`（XA 事务）、`:37` `TransactionLogEntry`（事务日志条目）、`:53` `TransactionLogStore`（事务日志存储 trait）、`:159` `TransactionState`（事务状态机）、`:171` `ParticipantState`（参与者状态机）、`:182` `TransactionParticipant`（事务参与者）、`:266` `DistributedTransaction`（分布式事务）、`:428` `DtxManager`（事务管理器），`packages/sz-orm-dtx/Cargo.toml:31` `xa` feature、`:33` `real-db` feature。缺口：参与者仅限 Rust 内部（`TransactionParticipant` 为 Rust trait/闭包），无跨语言参与者接入协议（Go/Java/C++ 服务无法注册为 Saga/TCC 参与者），无 gRPC/HTTP 标准事务协议接口，无跨语言补偿回调序列化。
   - **Go/Java/C++ 绑定**：`packages/sz-orm-python/Cargo.toml:19` `pyo3`（Python 绑定，PyO3）、`:20` `pyo3-asyncio`（Python 异步），`packages/sz-orm-js/Cargo.toml:19` `napi`（JS 绑定，napi-rs）、`:20` `napi-derive`。缺口：无 Go 绑定（cgo/FFI 暴露 C ABI 供 Go 调用）、无 Java 绑定（JNI 暴露 C ABI 供 JVM 调用）、无 C++ 绑定（extern "C" + cxxbindgen 头文件），三套绑定均需暴露 Model/QueryBuilder/Pool/Transaction 核心 API。
   - **可视化 Schema 设计器**：`packages/sz-orm-core/src/schema_sync.rs:100` `SchemaDiff`（schema 差异结构）、`:200` `diff` 函数（差分计算）、`:361` `DdlGenerator` trait（5 方言 DDL 生成器：`MySqlDdlGenerator`/`PgDdlGenerator`/`SqliteDdlGenerator`/`OracleDdlGenerator`/`MssqlDdlGenerator`）、`:612` `SchemaSync`（schema 同步），`cli/src/main.rs:1625` `cmd_generate_schema`（CLI schema 生成）、`:1630` `cmd_generate_schema` 实现，v4.1.0 已补 schema diff 可视化（REQ-V41-002，CLI/HTML/Markdown 报告）。缺口：无可视化 Schema 设计器（低代码图形化建表/改表/关系设计 Web UI），无设计器 ↔ 迁移文件/实体代码双向生成，无表关系图（ER 图）可视化编辑。
   - **OpenAPI → ORM 反向生成**：`packages/sz-orm-swagger/src/lib.rs:28` `OpenAPISpec`（OpenAPI 3.0 规范根对象）、`:55` `Components`（组件表，含 schemas）、`:328` `Schema`（Schema 定义枚举）、`:430` `ObjectType`、`:490` `ArrayType`、`:540` `PrimitiveSchema`、`:1096` `OpenAPIGenerator`（OpenAPI 生成器）、`:1229` `SwaggerUi`（Swagger UI 渲染）、`:1325` `model_to_openapi_schema`（正向：ORM Model → OpenAPI Schema）。缺口：无反向生成（OpenAPI Schema → ORM Model 代码），无 OpenAPI → 迁移文件，无 OpenAPI → Repository 代码，未形成 API 优先开发闭环（OpenAPI 定义 → ORM 代码 → API 实现）。
   - **WASM 真实数据库连接**：`packages/sz-orm-wasm/src/lib.rs:12` `advanced`（内存限制/WASI 沙箱/异步调度/模块缓存）、`:15` `js_bindings`（JS 绑定，feature 隔离）、`:18` `persistence`（持久化，feature 隔离）、`:21` `error`（错误类型）、`:38` `WasmQuery`（WASM 查询请求）、`:67` `WasmDatabase`（内存数据库，SQL 子集），`packages/sz-orm-wasm/Cargo.toml:29` `js` feature、`:30` `persistence` feature（仅 `web-sys` Window 浏览器本地存储）。缺口：无真实数据库连接（WASM 无法直接 TCP，需 HTTP/WebSocket 代理桥接后端 DB，或 WASI preview2 socket 直连），仅内存数据库与浏览器本地存储，不能操作后端 MySQL/PostgreSQL 等真实数据库。
4. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
5. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
6. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，可视化 Schema 设计器/OpenAPI 反向生成/WASM 真实 DB 须覆盖全部方言（按方言能力适配）。
7. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature，`sz-orm-dtx` 已有 `xa`/`real-db`，`sz-orm-wasm` 已有 `js`/`persistence`，作为新能力 feature gate 隔离的基础。
8. **既有语言绑定模式参考**：`sz-orm-python`（PyO3，`crate-type = ["cdylib", "rlib"]`）与 `sz-orm-js`（napi-rs，`crate-type = ["cdylib", "rlib"]`）为 Go/Java/C++ 绑定提供模式参考（cdylib + 核心 API 暴露）。

## 1.3 核心输出

1. **跨语言分布式事务**：CrossLangParticipantProtocol（跨语言参与者接入协议，gRPC/HTTP）+ CrossLangParticipant（跨语言参与者适配器，Go/Java/C++/Python/JS）+ CrossLangCompensationSerializer（补偿回调序列化）+ 跨语言事务协议 IDL（protobuf/JSON schema）+ 跨语言事务验证报告。
2. **Go/Java/C++ 绑定**：sz-orm-go（Go 绑定，cgo + C ABI + Go wrapper）+ sz-orm-java（Java 绑定，JNI + C ABI + Java wrapper）+ sz-orm-cpp（C++ 绑定，extern "C" + cxxbindgen 头文件 + C++ wrapper），每套暴露 Model/QueryBuilder/Pool/Transaction 核心 API + 异步运行时桥接 + 错误码映射。
3. **可视化 Schema 设计器**：SchemaDesigner（Schema 设计器核心）+ SchemaDesignerWebUI（Web UI，HTML/Canvas/SVG）+ ErDiagramEditor（ER 图可视化编辑）+ DesignerCodeGenerator（设计器 → 迁移文件 + 实体代码双向生成）+ CLI 集成（`sz-orm designer`）。
4. **OpenAPI → ORM 反向生成**：OpenApiReverseGenerator（OpenAPI → ORM 反向生成器）+ SchemaToModelMapper（OpenAPI Schema → ORM Model 映射）+ OpenApiToMigrationMapper（OpenAPI → 迁移文件）+ OpenApiToRepositoryMapper（OpenAPI → Repository 代码）+ CLI 集成（`sz-orm openapi:reverse`）+ API 优先开发闭环验证报告。
5. **WASM 真实数据库连接**：WasmRealDbConnection（WASM 真实 DB 连接，HTTP/WebSocket 代理桥接）+ WasmDbProxyProtocol（WASM ↔ 后端 DB 代理协议）+ WasiSocketConnection（WASI preview2 socket 直连，可选）+ WasmRealDbQueryExecutor（真实 DB 查询执行器，复用既有 `WasmQuery`）+ 连接验证报告。
6. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
7. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，沿用既有铁律。
3. **不替换既有分布式事务**：既有 `sz-orm-dtx` 的 `saga`/`tcc`/`cross_shard`/`xa`/`recovery`/`suspension`（`packages/sz-orm-dtx/src/lib.rs:19-29`）与 `DtxManager`（`:428`）、`DistributedTransaction`（`:266`）、`TransactionParticipant`（`:182`）保留，新增跨语言参与者协议扩展既有协调器，不修改既有事务执行逻辑。
4. **不替换既有 Python/JS 绑定**：既有 `sz-orm-python`（PyO3）与 `sz-orm-js`（napi-rs）保留不动，新增 Go/Java/C++ 绑定为独立包，不修改既有绑定。
5. **不替换既有 schema diff**：既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）与 `diff` 函数（`:200`）保留，v4.1.0 已补的 schema diff 可视化（REQ-V41-002）保留，新增可视化 Schema 设计器为独立低代码工具，不修改既有 diff 计算。
6. **不替换既有 OpenAPI 正向生成**：既有 `OpenAPISpec`（`packages/sz-orm-swagger/src/lib.rs:28`）、`OpenAPIGenerator`（`:1096`）、`model_to_openapi_schema`（`:1325`，正向 ORM→OpenAPI）保留，新增反向生成器复用既有 Schema 结构，不修改既有正向生成。
7. **不替换既有 WASM 内存数据库**：既有 `WasmDatabase`（`packages/sz-orm-wasm/src/lib.rs:67`，内存数据库 SQL 子集）、`advanced`（`:12`，沙箱/内存限制）、`js_bindings`（`:15`）、`persistence`（`:18`，浏览器本地存储）保留，新增真实 DB 连接为独立 feature，不修改既有内存数据库逻辑。
8. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
9. **不降低既有测试覆盖**：v4.2.0 不得使 v4.1.0 已验收测试基线回退，仅增不减。
10. **不负责更长期任务**：Informix 真实驱动/社区生态扩展/多语言分布式事务协议标准化（如 SEATA/OTLP 兼容）/移动端绑定（Swift/Kotlin）等在 v4.x+ 规划。
11. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。
12. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律（FFI/JNI/cgo 边界须显式标注 SAFETY）。
13. **不引入 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。

---

# 2. 领域术语

**跨语言分布式事务（Cross-Language Distributed Transaction）**
: 扩展既有 `sz-orm-dtx`（Saga/TCC/XA）协调器，支持异构语言服务（Go/Java/C++/Python/JS）作为事务参与者，通过 gRPC/HTTP 标准协议接入，参与者补偿回调序列化传输，协调器统一编排跨语言提交/回滚。
: 备注：既有 `TransactionParticipant`（`packages/sz-orm-dtx/src/lib.rs:182`）为 Rust 内部参与者，本版本补跨语言参与者协议。

**跨语言参与者协议（Cross-Language Participant Protocol）**
: 跨语言分布式事务参与者接入协议，定义标准接口（prepare/commit/rollback/confirm/cancel），通过 gRPC（protobuf IDL）或 HTTP（JSON）传输，异构语言服务实现该协议即可加入 Rust 协调器编排的分布式事务。
: 备注：复用既有 `sz-orm-grpc`（gRPC 包）与 `sz-orm-queue`（消息队列）做传输层。

**Go/Java/C++ 绑定（Go/Java/C++ Language Bindings）**
: 参照既有 `sz-orm-python`（PyO3）与 `sz-orm-js`（napi-rs）模式，为 sz-orm-core 核心 API（Model/QueryBuilder/Pool/Transaction）提供 Go（cgo + C ABI）、Java（JNI + C ABI）、C++（extern "C" + cxxbindgen）三套语言绑定，使 Go/Java/C++ 应用可直接调用 sz-orm。
: 备注：既有 `sz-orm-python`（`packages/sz-orm-python/Cargo.toml:19` pyo3）与 `sz-orm-js`（`:19` napi）为模式参考。

**可视化 Schema 设计器（Visual Schema Designer）**
: 低代码图形化 Schema 设计工具，提供 Web UI（HTML/Canvas/SVG）支持可视化建表/改表/字段配置/关系设计（ER 图编辑），设计器与迁移文件/实体代码双向生成（设计 → 代码，代码 → 设计），降低 Schema 设计门槛。
: 备注：既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）与 `DdlGenerator`（`:361`，5 方言）为 DDL 生成基础，v4.1.0 已补 schema diff 可视化（REQ-V41-002，报告类），本版本补交互式设计器。

**OpenAPI → ORM 反向生成（OpenAPI to ORM Reverse Generation）**
: 从 OpenAPI 3.0 规范的 `components.schemas` 反向生成 ORM 实体 Model 代码 + 迁移文件 + Repository 代码，与既有正向 `model_to_openapi_schema`（`packages/sz-orm-swagger/src/lib.rs:1325`，ORM→OpenAPI）形成 API 优先开发闭环（OpenAPI 定义 → ORM 代码 → API 实现 → OpenAPI 文档）。
: 备注：既有 `sz-orm-swagger` 仅正向生成（ORM→OpenAPI），本版本补反向生成。

**API 优先开发闭环（API-First Development Loop）**
: OpenAPI 规范作为单一事实源，先定义 API 契约，反向生成 ORM 代码骨架，开发者填充业务逻辑，正向生成验证实现与契约一致，形成闭环。

**WASM 真实数据库连接（WASM Real Database Connection）**
: 扩展既有 `sz-orm-wasm`（仅内存数据库 + 浏览器本地存储），通过 HTTP/WebSocket 代理桥接后端真实数据库（WASM 发请求 → 代理转 SQL → 后端 DB 执行 → 返回结果），或 WASI preview2 socket 直连（支持 WASI 的运行时），使浏览器端 WASM ORM 能操作真实 MySQL/PostgreSQL 等。
: 备注：既有 `WasmDatabase`（`packages/sz-orm-wasm/src/lib.rs:67`）为内存数据库，`persistence`（`:18`）为浏览器本地存储，本版本补真实 DB 连接。

**WASM DB 代理协议（WASM DB Proxy Protocol）**
: WASM 端与后端 DB 代理之间的通信协议，定义查询请求/响应/参数/事务/错误码格式（JSON/MessagePack），代理负责鉴权/限流/SQL 白名单/连接池，避免 WASM 直接执行任意 SQL。

**v4.2.0 feature gate**
: 控制本版本新能力的 feature gate 集合（`cross-lang-dtx` / `lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` / `schema-designer` / `openapi-reverse` / `wasm-real-db`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v4.2.0 五项扩展的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性的下游使用者，v4.2.0 不得破坏其既有代码。
- **微服务架构师**：使用跨语言分布式事务编排跨语言微服务事务，设计跨语言事务边界与补偿策略。
- **Go/Java/C++ 应用开发者**：使用 Go/Java/C++ 绑定在其语言应用中调用 sz-orm，是语言绑定的主要使用者。
- **低代码开发者/业务分析师**：使用可视化 Schema 设计器图形化设计数据库 Schema，无需手写 SQL/迁移文件。
- **API 优先开发者**：使用 OpenAPI → ORM 反向生成从 API 契约生成 ORM 代码骨架，遵循 API 优先开发流。
- **前端/WASM 开发者**：使用 WASM 真实数据库连接在浏览器端操作后端数据库，构建全栈 WASM 应用。
- **运维/SRE 工程师**：部署跨语言事务协调器、WASM DB 代理，监控跨语言事务与 WASM 连接健康。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：可视化 Schema 设计器/OpenAPI 反向生成/WASM 真实 DB 的五方言覆盖目标。
- **Go 运行时（cgo）**：Go 绑定的目标语言运行时，通过 cgo 调用 Rust C ABI。
- **Java JVM（JNI）**：Java 绑定的目标语言运行时，通过 JNI 调用 Rust C ABI。
- **C++ 编译器（cxxbindgen/extern "C"）**：C++ 绑定的目标语言，通过 extern "C" + 头文件互调。
- **gRPC（protobuf IDL）**：跨语言分布式事务参与者协议的传输层（复用既有 `sz-orm-grpc`）。
- **HTTP/JSON**：跨语言分布式事务参与者协议的备选传输层（轻量接入）。
- **OpenAPI 3.0 规范文件（YAML/JSON）**：OpenAPI → ORM 反向生成的输入。
- **Web 浏览器（HTML5/Canvas/SVG）**：可视化 Schema 设计器 Web UI 的运行环境。
- **WASM 运行时（浏览器 wasm-bindgen / WASI）**：WASM 真实数据库连接的运行环境。
- **WASM DB 代理（后端 HTTP/WebSocket 服务）**：WASM 真实 DB 连接的桥接代理，转发 WASM 请求到后端 DB。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.2.0 五项跨语言与低代码扩展 交互上下文

rectangle "sz-orm v4.2.0" as orm {
  rectangle "跨语言分布式事务" as crosslangdtx
  rectangle "Go/Java/C++ 绑定" as bindings
  rectangle "可视化 Schema 设计器" as designer
  rectangle "OpenAPI → ORM 反向生成" as openapireverse
  rectangle "WASM 真实数据库连接" as wasmrealdb
}

actor "ORM 库维护者" as dev
actor "微服务架构师" as arch
actor "Go/Java/C++ 开发者" as langdev
actor "低代码开发者" as lowcode
actor "API 优先开发者" as apifirst
actor "WASM 开发者" as wasmdev
actor "sz-pay 开发者" as downstream

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "Go/Java/C++ 运行时" as runtimes
rectangle "gRPC/HTTP\n(参与者协议)" as proto
file "OpenAPI 3.0\n(YAML/JSON)" as openapispec
cloud "Web 浏览器\n(HTML5/Canvas)" as browser
cloud "WASM 运行时" as wasmruntime
rectangle "WASM DB 代理\n(后端 HTTP/WS)" as wasmproxy

dev --> orm : 开发/验证/测试
arch --> crosslangdtx : 编排跨语言事务
crosslangdtx --> proto : 参与者协议(gRPC/HTTP)
proto --> runtimes : Go/Java/C++ 服务参与
crosslangdtx --> db : 事务提交/回滚
langdev --> bindings : Go/Java/C++ 调用
bindings --> runtimes : cgo/JNI/extern C
bindings --> db : ORM 操作
lowcode --> designer : 图形化设计 Schema
designer --> browser : Web UI(ER 图编辑)
designer --> db : 生成 DDL/迁移
apifirst --> openapireverse : OpenAPI → ORM
openapireverse --> openapispec : 读取规范
openapireverse --> db : 生成迁移/Model
wasmdev --> wasmrealdb : 浏览器端操作真实 DB
wasmrealdb --> wasmruntime : WASM 查询
wasmrealdb --> wasmproxy : HTTP/WS 代理
wasmproxy --> db : 转发 SQL 执行
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **跨语言事务协调开销**：跨语言分布式事务单次参与者协调（prepare/commit/rollback）开销不超过 50ms（含 gRPC/HTTP 往返 + 参与者本地执行），Saga 单事务编排 10 个跨语言参与者不超过 1 秒。
2. **Go/Java/C++ 绑定调用开销**：Go/Java/C++ 绑定单次 FFI 调用开销不超过 10μs（含 cgo/JNI/extern "C" 边界 + 参数序列化），批量查询 1,000 行不超过 50ms（含 FFI + 反序列化）。
3. **Schema 设计器响应**：可视化 Schema 设计器 Web UI 交互响应不超过 200ms（含表/字段/关系编辑 + DDL 预览），设计器 → 迁移文件/实体代码生成不超过 1 秒（表数量 ≤100）。
4. **OpenAPI 反向生成开销**：OpenAPI → ORM 反向生成开销不超过 2 秒（含 spec 解析 + Model/迁移/Repository 代码生成，Schema 数量 ≤100）。
5. **WASM 真实 DB 查询开销**：WASM 真实数据库连接单次查询开销不超过 100ms（含 WASM → 代理 HTTP/WS 往返 + 后端 DB 执行 + 结果返回），结果集 1,000 行不超过 200ms。
6. **WASM DB 代理吞吐**：WASM DB 代理吞吐不低于 10,000 查询/秒（单实例，含鉴权 + 限流 + SQL 白名单 + 连接池复用）。

## 4.2 可靠性

1. **跨语言事务原子性**：跨语言分布式事务须保证原子性（全部提交或全部回滚），协调器崩溃恢复后须能继续未完成事务或回滚，沿用既有 `sz-orm-dtx` 的 `recovery`（`packages/sz-orm-dtx/src/lib.rs:25`）机制。
2. **跨语言参与者故障隔离**：单个跨语言参与者故障（超时/不可达）须不阻塞整个事务，协调器须标记故障参与者并触发补偿/回滚，沿用既有 `TransactionState`（`:159`）与 `ParticipantState`（`:171`）状态机。
3. **Go/Java/C++ 绑定内存安全**：FFI 边界（cgo/JNI/extern "C"）须正确管理内存（Rust 侧分配/释放，语言侧引用），不泄漏不悬空，错误码完整映射，不 panic 跨语言边界（panic 须捕获转错误码）。
4. **Schema 设计器双向一致**：可视化 Schema 设计器双向生成须保证一致（设计 → 代码 → 设计'，design' 与 design 语义等价），不丢失字段/关系/约束。
5. **OpenAPI 反向生成幂等**：OpenAPI → ORM 反向生成须幂等（同一 spec 多次生成同一代码，除时间戳/注释外），不重复生成不覆盖用户手写业务逻辑（仅生成骨架 + 标注可编辑区）。
6. **WASM 真实 DB 连接可靠**：WASM 真实数据库连接须支持重连（代理临时不可用后自动重连），查询失败须返回明确错误码，不静默丢查询。
7. **WASM DB 代理 SQL 安全**：WASM DB 代理须执行 SQL 白名单（仅允许 SELECT/INSERT/UPDATE/DELETE + 参数化，禁止 DDL/批量危险操作），不暴露后端 DB 直连凭据。
8. **v4.1.0 测试基线不回退**：v4.2.0 不得使 v4.1.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **跨语言事务协议鉴权**：跨语言参与者协议（gRPC/HTTP）须支持鉴权（mTLS/Token），禁止未授权服务加入事务协调，参与者须验证协调器签名防伪造。
2. **跨语言事务补偿幂等**：跨语言参与者补偿回调须幂等（重复 commit/rollback 不产生副作用），补偿失败须告警不静默。
3. **FFI 边界 unsafe 隔离**：FFI 边界（cgo/JNI/extern "C"）的 `unsafe` 块须有 `// SAFETY:` 注释证明内存安全，不暴露 Rust 内部指针跨语言，参数须边界检查。
4. **Schema 设计器不泄露**：可视化 Schema 设计器 Web UI 须尊重既有脱敏规则（`sz-orm-masking`），敏感表/字段名可选脱敏展示，设计器不暴露生产凭据。
5. **OpenAPI 反向生成注入防护**：OpenAPI → ORM 反向生成须对 spec 内容做注入防护（不执行 spec 内嵌代码，不信任未签名 spec），生成代码须强制参数化查询。
6. **WASM DB 代理鉴权**：WASM DB 代理须鉴权（Token/Session），禁止未授权 WASM 端连接，每个 WASM 会话须隔离（不跨会话泄露数据）。
7. **WASM DB 代理限流**：WASM DB 代理须限流（单会话 QPS 上限），防 WASM 端滥用拖垮后端 DB。
8. **WASM 真实 DB 凭据隔离**：后端 DB 凭据仅代理持有，不下发到 WASM 端，WASM 端仅持代理 Token。

## 4.4 可维护性

1. **跨语言事务可观测**：跨语言分布式事务须输出结构化事务日志（事务 ID/参与者列表/语言/状态/耗时/补偿结果），接入既有 `sz-orm-tracing` + `sz-orm-observability`，跨语言参与者 span 关联。
2. **跨语言事务协议版本化**：跨语言参与者协议须版本化（协议版本号 + 兼容性声明），支持协议演进不破坏既有参与者。
3. **Go/Java/C++ 绑定文档**：三套绑定须提供目标语言惯用文档（Go doc/Java Javadoc/C++ Doxygen）+ 示例代码 + 错误码表，可被目标语言开发者直接消费。
4. **Schema 设计器可导出**：可视化 Schema 设计器须支持多格式导出（DDL/迁移文件/实体代码/ER 图 PNG/SVG/JSON 设计文件），可被 CLI/CI 消费。
5. **OpenAPI 反向生成可配置**：OpenAPI → ORM 反向生成须可配置（目标方言/代码风格/命名约定/可编辑区标注/是否覆盖），配置文件版本化管理。
6. **WASM 真实 DB 可观测**：WASM 真实数据库连接须输出查询指标（查询数/延迟/错误率/重连次数），接入既有 Prometheus，代理侧须输出会话/QPS/SQL 白名单命中指标。
7. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（Schema 设计器/OpenAPI 反向生成/WASM 真实 DB 按方言能力适配）。
4. **既有分布式事务保留**：既有 `sz-orm-dtx` 的 `saga`/`tcc`/`cross_shard`/`xa`/`recovery`/`suspension`（`packages/sz-orm-dtx/src/lib.rs:19-29`）与 `DtxManager`（`:428`）、`DistributedTransaction`（`:266`）、`TransactionParticipant`（`:182`）保留不动，新增跨语言参与者协议扩展。
5. **既有 Python/JS 绑定保留**：既有 `sz-orm-python`（PyO3）与 `sz-orm-js`（napi-rs）保留不动，新增 Go/Java/C++ 绑定为独立包。
6. **既有 schema diff 保留**：既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）与 `diff` 函数（`:200`）与 v4.1.0 schema diff 可视化（REQ-V41-002）保留不动，新增可视化 Schema 设计器为独立工具。
7. **既有 OpenAPI 正向生成保留**：既有 `OpenAPISpec`（`packages/sz-orm-swagger/src/lib.rs:28`）、`OpenAPIGenerator`（`:1096`）、`model_to_openapi_schema`（`:1325`）保留不动，新增反向生成器复用既有 Schema 结构。
8. **既有 WASM 内存数据库保留**：既有 `WasmDatabase`（`packages/sz-orm-wasm/src/lib.rs:67`）、`advanced`（`:12`）、`js_bindings`（`:15`）、`persistence`（`:18`）保留不动，新增真实 DB 连接为独立 feature。
9. **既有 feature 组合不破坏**：v4.2.0 新增 feature（`cross-lang-dtx` / `lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` / `schema-designer` / `openapi-reverse` / `wasm-real-db`）与既有 feature 任意组合编译通过。

---

# 5. 核心能力

## 5.1 跨语言分布式事务（REQ-V42-001）

### 5.1.1 业务规则

1. **跨语言参与者接入协议**（EARS: Ubiquitous）
   系统应当提供跨语言参与者接入协议（gRPC protobuf IDL + HTTP/JSON 备选），定义标准接口（prepare/commit/rollback/confirm/cancel），异构语言服务（Go/Java/C++/Python/JS）实现该协议即可注册为 `sz-orm-dtx` 协调器的事务参与者。
   a. 验收条件：[Go 服务实现 ParticipantProtocol gRPC 接口，注册到 DtxManager] → [Go 服务可参与 Saga/TCC 事务，协调器调用其 prepare/commit/rollback]
2. **跨语言参与者适配器**（EARS: Ubiquitous）
   系统应当提供 `CrossLangParticipant` 适配器，将跨语言参与者（通过 gRPC/HTTP 远程调用）适配为既有 `TransactionParticipant`（`packages/sz-orm-dtx/src/lib.rs:182`），协调器透明编排 Rust 内部参与者与跨语言参与者。
   a. 验收条件：[事务含 Rust 参与者 A + Go 参与者 B + Java 参与者 C] → [协调器统一编排 A/B/C，对 A 本地调用，对 B/C 通过 gRPC/HTTP 远程调用]
3. **补偿回调序列化**（EARS: Ubiquitous）
   系统应当提供补偿回调序列化（将 Rust 闭包的补偿逻辑序列化为跨语言可执行的协议消息），跨语言参与者收到补偿请求后执行其语言侧补偿逻辑，补偿结果回传协调器。
   a. 验收条件：[Saga 事务 Go 参与者 B 失败，触发补偿] → [协调器发送 rollback 给 B，B 执行 Go 侧补偿，结果回传]
4. **复用既有 DtxManager**（EARS: Ubiquitous）
   系统应当复用既有 `DtxManager`（`packages/sz-orm-dtx/src/lib.rs:428`）、`DistributedTransaction`（`:266`）、`TransactionState`（`:159`）、`ParticipantState`（`:171`）、`TransactionLogStore`（`:53`）与 `recovery`（`:25`）/`suspension`（`:27`），新增跨语言参与者协议扩展既有协调器，不修改既有事务执行逻辑。
   a. 验收条件：[CrossLangParticipant] → [适配为既有 TransactionParticipant，复用 DtxManager 编排，不重复实现事务状态机]
5. **复用既有 gRPC/消息队列**（EARS: Ubiquitous）
   系统应当复用既有 `sz-orm-grpc`（gRPC 包）与 `sz-orm-queue`（消息队列）做跨语言参与者协议传输层，不重复实现 gRPC/消息队列。
   a. 验收条件：[跨语言参与者协议传输] → [基于既有 sz-orm-grpc/sz-orm-queue，不引入新传输依赖]
6. **协议鉴权**（EARS: Ubiquitous）
   跨语言参与者协议须支持鉴权（gRPC mTLS / HTTP Token），禁止未授权服务加入事务协调，参与者须验证协调器签名防伪造。
   a. 验收条件：[未授权 Go 服务尝试注册为参与者] → [拒绝注册，提示鉴权失败]
7. **协调器崩溃恢复**（EARS: Event-driven）
   当协调器崩溃后重启时，系统应当从既有 `TransactionLogStore`（`packages/sz-orm-dtx/src/lib.rs:53`）恢复未完成跨语言事务，继续提交或回滚跨语言参与者，沿用既有 `recovery`（`:25`）机制。
   a. 验收条件：[协调器在跨语言事务提交中崩溃，重启] → [从日志恢复事务，继续提交或回滚跨语言参与者]
8. **参与者故障隔离**（EARS: Unwanted）
   如果单个跨语言参与者故障（超时/不可达），则系统应当标记故障参与者并触发补偿/回滚其他参与者，不阻塞整个事务，沿用既有 `ParticipantState`（`packages/sz-orm-dtx/src/lib.rs:171`）状态机。
   a. 验收条件：[事务含 A/B/C，B 超时不可达] → [标记 B 故障，回滚 A/C，告警 B 需人工处理]
9. **跨语言事务可观测**（EARS: Ubiquitous）
   系统应当输出跨语言事务结构化日志（事务 ID/参与者列表/语言/状态/耗时/补偿结果），接入既有 `sz-orm-tracing` + `sz-orm-observability`，跨语言参与者 span 关联。
   a. 验收条件：[跨语言事务执行] → [日志含参与者语言/状态/耗时，tracing span 关联跨语言参与者]
10. **禁止项**（EARS: Unwanted）
    如果跨语言分布式事务影响默认 feature 编译或运行时行为，则系统应当通过 `cross-lang-dtx` feature gate 隔离，默认不启用跨语言参与者协议。
    a. 验收条件：[`cargo build` 默认编译] → [无跨语言参与者协议，行为与 v4.1.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title 跨语言分布式事务 交互流程
actor "微服务架构师" as arch
participant "DtxManager" as dtx
participant "CrossLangParticipant" as crosslang
rectangle "sz-orm-grpc\n(gRPC 协议)" as grpc
participant "Go 服务" as go
participant "Java 服务" as java
participant "Rust 参与者" as rust
database "DB" as db

arch -> dtx : begin_cross_lang_tx(Saga, [Go, Java, Rust])
dtx -> dtx : 创建 DistributedTransaction
loop 编排参与者
  alt Rust 内部参与者
    dtx -> rust : prepare/commit/rollback(本地)
  else 跨语言参与者
    dtx -> crosslang : 适配为 TransactionParticipant
    crosslang -> grpc : prepare/commit/rollback(远程)
    grpc -> go : gRPC 调用
    grpc -> java : gRPC 调用
    go --> grpc : 结果
    java --> grpc : 结果
    grpc --> crosslang : 结果
  end
end
alt 全部成功
  dtx -> dtx : commit
else 任一失败
  dtx -> dtx : rollback(补偿所有参与者)
end
dtx -> arch : 事务结果 + 跨语言日志
@enduml
```

### 5.1.3 异常场景

1. **跨语言参与者超时**
   a. 触发条件：跨语言参与者 prepare/commit/rollback 超时（gRPC/HTTP 往返超时）
   b. 系统行为：标记参与者超时，触发补偿/回滚其他参与者，告警
   c. 用户感知：告警"cross-lang participant X timeout, compensating others, manual intervention required"
2. **协议鉴权失败**
   a. 触发条件：跨语言参与者未授权（mTLS/Token 无效）
   b. 系统行为：拒绝注册，提示鉴权失败
   c. 用户感知：错误提示"cross-lang participant auth failed, mTLS/Token invalid"
3. **协调器崩溃恢复冲突**
   a. 触发条件：协调器崩溃恢复后发现参与者已被其他协调器回滚
   b. 系统行为：检测冲突，告警人工处理，不盲目重试
   c. 用户感知：告警"dtx recovery conflict, participant X already rolled back, manual resolution required"

## 5.2 Go/Java/C++ 绑定（REQ-V42-002）

### 5.2.1 业务规则

1. **Go 绑定**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-go` 包，通过 cgo + C ABI 暴露 sz-orm-core 核心 API（Model/QueryBuilder/Pool/Transaction），Go 应用通过 cgo 调用，包含 Go wrapper（惯用 Go API 风格）+ 异步运行时桥接（tokio ↔ goroutine）+ 错误码映射。
   a. 验收条件：[Go 应用调用 sz_orm_go.QueryBuilder().where_eq("id", 1).first()] → [通过 cgo 调用 Rust C ABI，返回 Go 结构体，行为与 Rust 一致]
2. **Java 绑定**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-java` 包，通过 JNI + C ABI 暴露 sz-orm-core 核心 API，Java 应用通过 JNI 调用，包含 Java wrapper（惯用 Java API 风格）+ 异步运行时桥接（tokio ↔ CompletableFuture/虚拟线程）+ 错误码映射。
   a. 验收条件：[Java 应用调用 SzOrmJava.queryBuilder().whereEq("id", 1).first()] → [通过 JNI 调用 Rust C ABI，返回 Java 对象，行为与 Rust 一致]
3. **C++ 绑定**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-cpp` 包，通过 extern "C" + cxxbindgen 头文件暴露 sz-orm-core 核心 API，C++ 应用通过头文件调用，包含 C++ wrapper（惯用 C++ API 风格，RAII/智能指针）+ 异步运行时桥接（tokio ↔ std::future/协程）+ 错误码映射。
   a. 验收条件：[C++ 应用调用 sz_orm_cpp::QueryBuilder().where_eq("id", 1).first()] → [通过 extern "C" 调用 Rust，返回 C++ 对象，行为与 Rust 一致]
4. **核心 API 覆盖**（EARS: Ubiquitous）
   三套绑定应当统一暴露 sz-orm-core 核心 API：(a) Model（定义/CRUD）；(b) QueryBuilder（where_eq/or_where_eq/order/limit/first/get 等）；(c) Pool（连接池获取/配置）；(d) Transaction（begin/commit/rollback），API 行为与 Rust 一致。
   a. 验收条件：[Go/Java/C++ 调用相同 CRUD 序列] → [行为与 Rust sz-orm-core 一致，结果相同]
5. **复用既有绑定模式**（EARS: Ubiquitous）
   系统应当参照既有 `sz-orm-python`（PyO3，`packages/sz-orm-python/Cargo.toml:19`）与 `sz-orm-js`（napi-rs，`packages/sz-orm-js/Cargo.toml:19`）的 `crate-type = ["cdylib", "rlib"]` 模式，Go/Java/C++ 绑定采用 cdylib + C ABI + 语言 wrapper 架构。
   a. 验收条件：[sz-orm-go/java/cpp Cargo.toml] → [crate-type 含 cdylib，与 python/js 模式一致]
6. **FFI 内存安全**（EARS: Ubiquitous）
   FFI 边界（cgo/JNI/extern "C"）须正确管理内存（Rust 侧分配/释放，语言侧引用），不泄漏不悬空，panic 须捕获转错误码不跨语言边界，`unsafe` 块须有 `// SAFETY:` 注释。
   a. 验收条件：[Go/Java/C++ 调用触发 Rust panic] → [panic 捕获转错误码，不跨语言边界，无内存泄漏]
7. **异步运行时桥接**（EARS: Ubiquitous）
   三套绑定应当桥接 Rust tokio 异步运行时与目标语言异步机制（Go goroutine/Java CompletableFuture 或虚拟线程/C++ std::future 或协程），异步查询不阻塞目标语言主线程。
   a. 验收条件：[Go 异步调用 first()] → [不阻塞 goroutine 调度，结果通过 channel 返回]
8. **错误码映射**（EARS: Ubiquitous）
   三套绑定应当完整映射 sz-orm-core 错误码到目标语言错误类型（Go error/Java Exception/C++ std::exception），不丢失错误信息。
   a. 验收条件：[Rust 返回 NotFound 错误] → [Go 返回 ErrNotFound、Java 抛 NotFoundException、C++ 抛 SzOrmNotFound]
9. **目标语言文档**（EARS: Ubiquitous）
   三套绑定应当提供目标语言惯用文档（Go doc/Java Javadoc/C++ Doxygen）+ 示例代码 + 错误码表，可被目标语言开发者直接消费。
   a. 验收条件：[查看 sz-orm-go 文档] → [Go doc 格式，含示例与错误码表]
10. **禁止项**（EARS: Unwanted）
    如果 Go/Java/C++ 绑定影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` feature gate 隔离，默认不启用三套绑定。
    a. 验收条件：[`cargo build` 默认编译] → [无 Go/Java/C++ 绑定，行为与 v4.1.0 一致]

### 5.2.2 交互流程

```plantuml
@startuml
title Go/Java/C++ 绑定 交互流程
actor "Go/Java/C++ 开发者" as langdev
participant "语言 wrapper\n(Go/Java/C++)" as wrapper
participant "C ABI 边界\n(cdylib)" as cabi
participant "sz-orm-core\n(Rust)" as core
participant "tokio 运行时" as tokio
database "DB" as db

langdev -> wrapper : QueryBuilder().where_eq("id", 1).first()
wrapper -> wrapper : 参数序列化
wrapper -> cabi : cgo/JNI/extern "C" 调用
cabi -> core : 调用 Rust 核心 API
core -> tokio : 异步查询
tokio -> db : SQL 执行
db --> tokio : 结果
tokio --> core : 结果
core --> cabi : 结果(序列化)
cabi --> wrapper : 结果(反序列化)
wrapper --> langdev : 语言对象(Go struct/Java Object/C++ Object)
@enduml
```

### 5.2.3 异常场景

1. **FFI 内存泄漏**
   a. 触发条件：FFI 边界未正确释放 Rust 侧分配的内存
   b. 系统行为：检测泄漏，告警，提供释放函数
   c. 用户感知：告警"FFI memory leak detected, call sz_orm_free to release"
2. **Rust panic 跨语言边界**
   a. 触发条件：Rust 侧 panic 未捕获传播到 FFI 边界
   b. 系统行为：panic 捕获转错误码，不跨语言边界（UB 防护）
   c. 用户感知：错误码"sz_orm_panic: [panic message]"
3. **异步运行时桥接失败**
   a. 触发条件：tokio 运行时未初始化或桥接失败
   b. 系统行为：返回运行时错误，提示初始化
   c. 用户感知：错误提示"tokio runtime not initialized, call sz_orm_init first"

## 5.3 可视化 Schema 设计器（REQ-V42-003）

### 5.3.1 业务规则

1. **Web UI 设计器**（EARS: Ubiquitous）
   系统应当提供可视化 Schema 设计器 Web UI（HTML5/Canvas/SVG），支持图形化建表/改表/字段配置（类型/约束/默认值/注释）/关系设计（外键/索引），交互响应不超过 200ms。
   a. 验收条件：[浏览器打开设计器，新建 users 表含 id/name/email 字段] → [图形化编辑，实时预览 DDL，响应 < 200ms]
2. **ER 图可视化编辑**（EARS: Ubiquitous）
   系统应当提供 ER 图可视化编辑（表为节点，外键为边），支持拖拽布局/关系连线/一对多/多对多标注，可导出 ER 图 PNG/SVG/JSON。
   a. 验收条件：[users 与 orders 一对多关系] → [ER 图显示 users→orders 连线，标注 1:N，可导出 SVG]
3. **设计器 → 代码生成**（EARS: Ubiquitous）
   系统应当从设计器设计生成迁移文件（up/down SQL，复用既有 `DdlGenerator` trait `packages/sz-orm-core/src/schema_sync.rs:361`，5 方言）+ 实体 Model 代码（Rust struct + derive Model），生成不超过 1 秒（表数量 ≤100）。
   a. 验收条件：[设计器设计 users 表] → [生成迁移文件（5 方言 DDL）+ Rust User struct + derive Model]
4. **代码 → 设计器反向解析**（EARS: Ubiquitous）
   系统应当从既有迁移文件/实体 Model 代码反向解析为设计器设计（代码 → 设计），支持双向往返（设计 → 代码 → 设计'，design' 与 design 语义等价）。
   a. 验收条件：[设计 users 表 → 生成代码 → 反向解析] → [解析结果与原设计语义等价]
5. **复用既有 SchemaDiff/DdlGenerator**（EARS: Ubiquitous）
   系统应当复用既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）、`diff` 函数（`:200`）、`DdlGenerator` trait（`:361`，5 方言）、`SchemaSync`（`:612`），设计器 DDL 生成与 diff 计算复用既有能力，不重复实现。
   a. 验收条件：[设计器生成 DDL] → [基于既有 DdlGenerator 生成，不重复实现 5 方言 DDL]
6. **五方言 DDL 生成**（EARS: Ubiquitous）
   系统应当复用既有 5 方言 DdlGenerator（MySql/Pg/Sqlite/Oracle/Mssql），设计器生成五方言 DDL，标注方言特定差异（如 MySQL AUTO_INCREMENT vs PostgreSQL SERIAL）。
   a. 验收条件：[设计器设计含自增主键表] → [生成 MySQL AUTO_INCREMENT、PostgreSQL SERIAL、Oracle SEQUENCE、MSSQL IDENTITY]
7. **CLI 集成**（EARS: Ubiquitous）
   系统应当提供 CLI 命令 `sz-orm designer`（启动 Web UI 服务器）与 `sz-orm designer:export`（从设计文件导出迁移/实体代码），复用既有 CLI 框架。
   a. 验收条件：[执行 `sz-orm designer`] → [启动 Web UI 服务器，浏览器可访问设计器]
8. **多格式导出**（EARS: Ubiquitous）
   系统应当支持多格式导出（DDL SQL/迁移文件/实体 Rust 代码/ER 图 PNG/SVG/JSON 设计文件），可被 CLI/CI 消费。
   a. 验收条件：[设计器导出] → [输出 DDL + 迁移 + Rust 代码 + ER 图 SVG + JSON 设计文件]
9. **脱敏展示**（EARS: Ubiquitous）
   可视化 Schema 设计器 Web UI 须尊重既有脱敏规则（`sz-orm-masking`），敏感表/字段名可选脱敏展示，设计器不暴露生产凭据。
   a. 验收条件：[含敏感字段 password 的表] → [设计器可选脱敏展示为 ******]
10. **禁止项**（EARS: Unwanted）
    如果可视化 Schema 设计器影响默认 feature 编译或引入不必要的依赖（Web UI 框架），则系统应当通过 `schema-designer` feature gate 隔离，默认不启用设计器。
    a. 验收条件：[`cargo build` 默认编译] → [无设计器，行为与 v4.1.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title 可视化 Schema 设计器 交互流程
actor "低代码开发者" as lowcode
participant "SchemaDesignerWebUI" as ui
participant "SchemaDesigner" as designer
participant "DdlGenerator" as ddl
participant "SchemaDiff" as diff
database "DB" as db

lowcode -> ui : 浏览器打开设计器
ui -> designer : 加载既有 schema(可选)
designer -> db : 读取当前 schema
db --> designer : 表/字段/关系
designer --> ui : ER 图可视化
loop 图形化编辑
  lowcode -> ui : 新建/修改表/字段/关系
  ui -> designer : 设计变更
  designer -> ddl : 实时预览 DDL(5 方言)
  ddl --> ui : DDL 预览
end
lowcode -> ui : 导出(迁移+实体+ER 图)
ui -> designer : 生成代码
designer -> ddl : 生成 5 方言 DDL
designer -> designer : 生成 Rust Model 代码
designer --> ui : 迁移文件 + 实体代码 + ER 图
ui --> lowcode : 下载导出文件
@enduml
```

### 5.3.3 异常场景

1. **设计器 ↔ 代码双向不一致**
   a. 触发条件：设计 → 代码 → 设计' 往返后 design' 与 design 语义不一致
   b. 系统行为：检测不一致，标注差异，告警
   c. 用户感知：告警"designer round-trip inconsistency: field X type changed"
2. **DDL 生成失败**
   a. 触发条件：设计含方言不支持的特性（如 SQLite 不支持某些约束）
   b. 系统行为：降级生成，标注不支持特性
   c. 用户感知：提示"DDL generation partial: SQLite does not support CHECK constraint, skipped"
3. **Web UI 加载失败**
   a. 触发条件：浏览器不兼容或资源加载失败
   b. 系统行为：降级提示，提供 CLI 导出备选
   c. 用户感知：提示"Web UI unavailable, use `sz-orm designer:export` CLI"

## 5.4 OpenAPI → ORM 反向生成（REQ-V42-004）

### 5.4.1 业务规则

1. **OpenAPI → Model 反向生成**（EARS: Ubiquitous）
   系统应当提供 `OpenApiReverseGenerator`，从 OpenAPI 3.0 规范的 `components.schemas`（`packages/sz-orm-swagger/src/lib.rs:55` `Components.schemas`）反向生成 ORM 实体 Model 代码（Rust struct + derive Model），字段类型映射（string→String/integer→i64/number→f64/boolean→bool/array→Vec/object→嵌套 struct）。
   a. 验收条件：[OpenAPI spec 含 User schema（id:integer, name:string, email:string）] → [生成 Rust User struct + derive Model，字段类型正确映射]
2. **OpenAPI → 迁移文件生成**（EARS: Ubiquitous）
   系统应当从 OpenAPI Schema 生成迁移文件（up/down SQL，复用既有 `DdlGenerator` trait 5 方言），Schema 字段约束（required→NOT NULL、maxLength→VARCHAR(n)、format:date-time→TIMESTAMP）映射到 DDL 约束。
   a. 验收条件：[OpenAPI User schema 含 required:id, maxLength:255 name] → [生成迁移 CREATE TABLE users (id NOT NULL, name VARCHAR(255))，5 方言]
3. **OpenAPI → Repository 代码生成**（EARS: Ubiquitous）
   系统应当从 OpenAPI Schema 生成 Repository 代码骨架（CRUD 方法：find_by_id/find_all/create/update/delete），标注可编辑区（用户业务逻辑填充区），不覆盖用户手写业务逻辑。
   a. 验收条件：[OpenAPI User schema] → [生成 UserRepository 骨架含 CRUD 方法 + 可编辑区标注]
4. **API 优先开发闭环**（EARS: Ubiquitous）
   系统应当形成 API 优先开发闭环：OpenAPI 定义 → 反向生成 ORM 代码骨架 → 开发者填充业务逻辑 → 正向生成（既有 `model_to_openapi_schema` `packages/sz-orm-swagger/src/lib.rs:1325`）验证实现与契约一致，闭环验证报告标注差异。
   a. 验收条件：[OpenAPI spec → 反向生成 ORM → 正向生成 OpenAPI'] → [spec 与 OpenAPI' 一致（除可编辑区），闭环验证报告标注差异]
5. **复用既有 OpenAPI 结构**（EARS: Ubiquitous）
   系统应当复用既有 `OpenAPISpec`（`packages/sz-orm-swagger/src/lib.rs:28`）、`Components`（`:55`）、`Schema`（`:328`）、`ObjectType`（`:430`）、`ArrayType`（`:490`）、`PrimitiveSchema`（`:540`），反向生成解析既有结构，不重复实现 OpenAPI 解析。
   a. 验收条件：[OpenApiReverseGenerator] → [基于既有 OpenAPISpec/Schema 解析，不重复实现]
6. **复用既有 DdlGenerator**（EARS: Ubiquitous）
   系统应当复用既有 `DdlGenerator` trait（`packages/sz-orm-core/src/schema_sync.rs:361`，5 方言）生成迁移文件，不重复实现 DDL 生成。
   a. 验收条件：[OpenAPI → 迁移文件] → [基于既有 DdlGenerator 生成 5 方言 DDL]
7. **幂等生成**（EARS: Ubiquitous）
   OpenAPI → ORM 反向生成须幂等（同一 spec 多次生成同一代码，除时间戳/注释外），不重复生成不覆盖用户手写业务逻辑（仅生成骨架 + 标注可编辑区）。
   a. 验收条件：[同一 spec 反向生成两次] → [生成代码一致（除时间戳），用户手写逻辑保留]
8. **可配置生成**（EARS: Ubiquitous）
   系统应当支持配置（目标方言/代码风格/命名约定/可编辑区标注/是否覆盖），配置文件版本化管理。
   a. 验收条件：[配置 target_dialect=postgresql, naming=snake_case] → [生成 PostgreSQL 迁移 + snake_case 命名]
9. **注入防护**（EARS: Ubiquitous）
   OpenAPI → ORM 反向生成须对 spec 内容做注入防护（不执行 spec 内嵌代码，不信任未签名 spec），生成代码须强制参数化查询。
   a. 验收条件：[spec 含恶意内嵌代码] → [不执行，生成代码强制参数化查询]
10. **CLI 集成**（EARS: Ubiquitous）
    系统应当提供 CLI 命令 `sz-orm openapi:reverse --spec=openapi.yaml --dialect=postgresql`，输出 Model/迁移/Repository 代码。
    a. 验收条件：[执行 `sz-orm openapi:reverse --spec=openapi.yaml`] → [输出 Model + 迁移 + Repository 代码文件]
11. **禁止项**（EARS: Unwanted）
    如果 OpenAPI 反向生成影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `openapi-reverse` feature gate 隔离，默认不启用反向生成。
    a. 验收条件：[`cargo build` 默认编译] → [无反向生成，行为与 v4.1.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title OpenAPI → ORM 反向生成 交互流程
actor "API 优先开发者" as apifirst
participant "OpenApiReverseGenerator" as reverse
participant "OpenAPISpec\n(既有)" as spec
participant "DdlGenerator\n(既有)" as ddl
file "OpenAPI YAML/JSON" as input
file "输出代码" as output

apifirst -> reverse : openapi:reverse --spec=openapi.yaml
reverse -> input : 读取 OpenAPI spec
input --> reverse : spec 内容
reverse -> spec : 解析 components.schemas(既有)
spec --> reverse : Schema 列表
reverse -> reverse : Schema → Model 字段映射
reverse -> ddl : 生成 5 方言迁移 DDL
ddl --> reverse : 迁移文件
reverse -> reverse : 生成 Repository 骨架(可编辑区)
reverse -> output : Model + 迁移 + Repository 代码
reverse -> reverse : 闭环验证(正向生成对比)
reverse --> apifirst : 代码文件 + 闭环验证报告
@enduml
```

### 5.4.3 异常场景

1. **OpenAPI spec 解析失败**
   a. 触发条件：spec 格式错误或不符合 OpenAPI 3.0
   b. 系统行为：解析失败，提示错误位置
   c. 用户感知：错误提示"OpenAPI spec parse failed at components.schemas.User: invalid type"
2. **不支持的 Schema 特性**
   a. 触发条件：spec 含 ORM 不支持的特性（如 allOf/oneOf 复杂组合）
   b. 系统行为：降级生成，标注不支持特性，跳过或生成占位注释
   c. 用户感知：提示"unsupported schema construct allOf at User, skipped with TODO comment"
3. **闭环验证差异**
   a. 触发条件：反向生成后正向生成 OpenAPI' 与原 spec 不一致
   b. 系统行为：标注差异，告警，不阻断生成
   c. 用户感知：闭环验证报告"spec vs OpenAPI' diff: field X type mismatch, manual review required"

## 5.5 WASM 真实数据库连接（REQ-V42-005）

### 5.5.1 业务规则

1. **HTTP/WebSocket 代理桥接**（EARS: Ubiquitous）
   系统应当提供 `WasmRealDbConnection`，通过 HTTP/WebSocket 代理桥接后端真实数据库（WASM 发请求 → 代理转 SQL → 后端 DB 执行 → 返回结果），使浏览器端 WASM ORM 能操作真实 MySQL/PostgreSQL 等，复用既有 `WasmQuery`（`packages/sz-orm-wasm/src/lib.rs:38`）。
   a. 验收条件：[WASM 端调用 query("SELECT * FROM users WHERE id = ?", [1])] → [通过 HTTP/WS 发送到代理，代理转发后端 DB，返回结果]
2. **WASM DB 代理协议**（EARS: Ubiquitous）
   系统应当定义 WASM DB 代理协议（查询请求/响应/参数/事务/错误码格式，JSON/MessagePack），代理负责鉴权/限流/SQL 白名单/连接池，避免 WASM 直接执行任意 SQL。
   a. 验收条件：[WASM 发查询请求] → [代理鉴权 + 限流 + SQL 白名单检查 + 连接池执行，返回结果]
3. **WASI socket 直连（可选）**（EARS: Optional）
   当目标 WASM 运行时支持 WASI preview2 socket 时，系统应当提供 `WasiSocketConnection` 直连后端 DB（不经代理），适用于可信 WASI 环境（非浏览器）。
   a. 验收条件：[WASI 运行时支持 socket，配置直连模式] → [WASM 直连后端 DB，不经代理]
4. **复用既有 WasmQuery/WasmDatabase**（EARS: Ubiquitous）
   系统应当复用既有 `WasmQuery`（`packages/sz-orm-wasm/src/lib.rs:38`）、`WasmDatabase`（`:67`，内存数据库）、`js_bindings`（`:15`）、`advanced`（`:12`，沙箱/内存限制），真实 DB 连接复用既有查询结构与 JS 绑定，不重复实现。
   a. 验收条件：[WasmRealDbConnection] → [复用既有 WasmQuery 结构与 js_bindings，不重复实现]
5. **查询执行器**（EARS: Ubiquitous）
   系统应当提供 `WasmRealDbQueryExecutor`，执行真实 DB 查询（SELECT/INSERT/UPDATE/DELETE，参数化），结果集序列化回 WASM 端，单次查询不超过 100ms，结果集 1,000 行不超过 200ms。
   a. 验收条件：[执行 SELECT 返回 1,000 行] → [结果集序列化回 WASM，耗时 < 200ms]
6. **代理鉴权与隔离**（EARS: Ubiquitous）
   WASM DB 代理须鉴权（Token/Session），禁止未授权 WASM 端连接，每个 WASM 会话须隔离（不跨会话泄露数据），后端 DB 凭据仅代理持有不下发 WASM 端。
   a. 验收条件：[未授权 WASM 端连接代理] → [拒绝，提示鉴权失败；WASM 端无后端 DB 凭据]
7. **代理限流与 SQL 白名单**（EARS: Ubiquitous）
   WASM DB 代理须限流（单会话 QPS 上限）+ SQL 白名单（仅允许 SELECT/INSERT/UPDATE/DELETE + 参数化，禁止 DDL/批量危险操作），防 WASM 端滥用拖垮后端 DB。
   a. 验收条件：[WASM 端发 DDL 或超 QPS] → [代理拒绝，提示白名单/限流]
8. **重连与错误处理**（EARS: Ubiquitous）
   WASM 真实数据库连接须支持重连（代理临时不可用后自动重连），查询失败须返回明确错误码，不静默丢查询。
   a. 验收条件：[代理临时不可用] → [WASM 端自动重连，查询失败返回明确错误码]
9. **可观测**（EARS: Ubiquitous）
   系统应当输出查询指标（查询数/延迟/错误率/重连次数），接入既有 Prometheus，代理侧须输出会话/QPS/SQL 白名单命中指标。
   a. 验收条件：[启用 WASM 真实 DB] → [Prometheus 抓取查询指标 + 代理会话/QPS 指标]
10. **禁止项**（EARS: Unwanted）
    如果 WASM 真实数据库连接影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `wasm-real-db` feature gate 隔离，默认不启用真实 DB 连接（保留既有内存数据库/浏览器本地存储）。
    a. 验收条件：[`cargo build` 默认编译] → [无真实 DB 连接，行为与 v4.1.0 一致（内存数据库 + 本地存储）]

### 5.5.2 交互流程

```plantuml
@startuml
title WASM 真实数据库连接 交互流程
actor "WASM 开发者" as wasmdev
participant "WasmRealDbConnection" as wasmconn
participant "WasmQuery\n(既有)" as wasmquery
rectangle "WASM DB 代理\n(后端 HTTP/WS)" as proxy
database "后端 DB\n(MySQL/PG/...)" as db

wasmdev -> wasmconn : query("SELECT * FROM users WHERE id = ?", [1])
wasmconn -> wasmquery : 构造 WasmQuery(既有)
wasmconn -> proxy : HTTP/WS 请求(Token + SQL + 参数)
proxy -> proxy : 鉴权 + 限流 + SQL 白名单
alt 鉴权/白名单通过
  proxy -> db : 执行参数化查询(连接池)
  db --> proxy : 结果集
  proxy -> proxy : 序列化结果(JSON/MessagePack)
  proxy --> wasmconn : 结果
  wasmconn --> wasmdev : 结果集
else 鉴权/白名单失败
  proxy --> wasmconn : 错误码
  wasmconn --> wasmdev : 错误提示
end
@enduml
```

### 5.5.3 异常场景

1. **代理不可用**
   a. 触发条件：WASM DB 代理临时不可用（网络/代理宕机）
   b. 系统行为：自动重连，查询失败返回明确错误码，不静默丢查询
   c. 用户感知：错误提示"wasm db proxy unavailable, reconnecting, query failed"
2. **SQL 白名单拒绝**
   a. 触发条件：WASM 端发 DDL 或禁止操作
   b. 系统行为：代理拒绝，提示白名单策略
   c. 用户感知：错误提示"SQL rejected by whitelist: DDL not allowed"
3. **限流触发**
   a. 触发条件：单 WASM 会话 QPS 超上限
   b. 系统行为：代理限流，提示限流
   c. 用户感知：错误提示"rate limit exceeded, QPS > N"
4. **后端 DB 凭据泄露防护**
   a. 触发条件：WASM 端尝试获取后端 DB 凭据
   b. 系统行为：代理不下发凭据，仅持 Token，拒绝凭据请求
   c. 用户感知：错误提示"db credentials not exposed to WASM, use proxy Token only"

---

# 6. 数据约束

## 6.1 需求项

1. **需求 ID**：唯一标识，格式 `REQ-V42-xxx`（xxx = 001~005），必填。
2. **需求名称**：人类可读名称，必填。
3. **优先级**：P1 / P2 / P3，必填。
4. **分类**：跨语言事务 / 多语言绑定 / 低代码 / API 优先 / 浏览器端 ORM，必填。
5. **EARS 分类**：Ubiquitous / Event-driven / State-driven / Optional / Unwanted，每条业务规则必填。
6. **验证方法**：可执行的验证命令或测试描述，必填。
7. **代码证据**：相关 file:line 引用，必填，遵循审计合规铁律。
8. **验收条件**：触发场景 → 预期行为，必填。
9. **状态**：PASS / FAIL / PENDING，必填。
10. **与 v4.1.0 兼容性**：feature gate 隔离 / 既有 API 保留 / 测试基线不回退，必填。

## 6.2 输出对象

1. **CrossLangParticipant**：跨语言事务参与者（语言/协议端点/补偿回调/鉴权凭据）。
2. **CrossLangTxProtocol**：跨语言事务协议（gRPC IDL/HTTP JSON schema，prepare/commit/rollback/confirm/cancel 接口）。
3. **CrossLangTxLog**：跨语言事务日志（事务 ID/参与者列表/语言/状态/耗时/补偿结果）。
4. **GoBinding**：Go 绑定（cgo + C ABI + Go wrapper，Model/QueryBuilder/Pool/Transaction）。
5. **JavaBinding**：Java 绑定（JNI + C ABI + Java wrapper，Model/QueryBuilder/Pool/Transaction）。
6. **CppBinding**：C++ 绑定（extern "C" + cxxbindgen + C++ wrapper，Model/QueryBuilder/Pool/Transaction）。
7. **SchemaDesign**：Schema 设计（表/字段/关系/约束，设计器中间表示）。
8. **SchemaDesignerWebUI**：设计器 Web UI（HTML5/Canvas/SVG，ER 图编辑）。
9. **DesignerExport**：设计器导出（DDL/迁移文件/实体代码/ER 图 PNG/SVG/JSON 设计文件）。
10. **OpenApiReverseResult**：OpenAPI 反向生成结果（Model 代码/迁移文件/Repository 代码/闭环验证报告）。
11. **ApiFirstLoopReport**：API 优先开发闭环验证报告（spec vs 正向生成 OpenAPI' 差异）。
12. **WasmRealDbConnection**：WASM 真实 DB 连接（代理端点/Token/会话/重连配置）。
13. **WasmDbProxyProtocol**：WASM DB 代理协议（查询请求/响应/参数/事务/错误码格式）。
14. **WasmRealDbQueryResult**：WASM 真实 DB 查询结果（序列化结果集/错误码/延迟）。

---

# 7. 需求追溯矩阵

| 需求编号 | 需求项 | 优先级 | 分类 | EARS 分类 | 验收条件（节选） | 现有代码证据 | 与 v4.1.0 兼容性 |
|---------|--------|--------|------|----------|----------------|-------------|----------------|
| REQ-V42-001 | 跨语言分布式事务 | P1 | 跨语言事务 | Ubiquitous/Event-driven/Unwanted | 跨语言参与者协议 + 适配器 + 补偿序列化 + 鉴权 + 崩溃恢复 + 故障隔离 | `packages/sz-orm-dtx/src/lib.rs:19` cross_shard、`:20` saga、`:21` tcc、`:25` recovery、`:27` suspension、`:29` xa、`:53` TransactionLogStore、`:159` TransactionState、`:171` ParticipantState、`:182` TransactionParticipant、`:266` DistributedTransaction、`:428` DtxManager、`Cargo.toml:31` xa feature | `cross-lang-dtx` feature gate，既有 DtxManager/saga/tcc/xa 保留 |
| REQ-V42-002 | Go/Java/C++ 绑定 | P2 | 多语言绑定 | Ubiquitous/Unwanted | Go(cgo) + Java(JNI) + C++(extern C) + 核心 API 覆盖 + FFI 内存安全 + 异步桥接 + 错误码映射 | `packages/sz-orm-python/Cargo.toml:19` pyo3（模式参考）、`packages/sz-orm-js/Cargo.toml:19` napi（模式参考）、`packages/sz-orm-core/src/lib.rs` 核心 API | `lang-binding-go`/`lang-binding-java`/`lang-binding-cpp` feature gate，既有 python/js 绑定保留 |
| REQ-V42-003 | 可视化 Schema 设计器 | P2 | 低代码 | Ubiquitous/Unwanted | Web UI + ER 图编辑 + 设计↔代码双向 + 五方言 DDL + 多格式导出 + 脱敏 | `packages/sz-orm-core/src/schema_sync.rs:100` SchemaDiff、`:200` diff、`:361` DdlGenerator（5 方言）、`:612` SchemaSync、`cli/src/main.rs:1625` cmd_generate_schema | `schema-designer` feature gate，既有 SchemaDiff/diff/DdlGenerator 保留 |
| REQ-V42-004 | OpenAPI → ORM 反向生成 | P2 | API 优先 | Ubiquitous/Unwanted | OpenAPI→Model + →迁移 + →Repository + API 优先闭环 + 幂等 + 注入防护 | `packages/sz-orm-swagger/src/lib.rs:28` OpenAPISpec、`:55` Components、`:328` Schema、`:430` ObjectType、`:490` ArrayType、`:540` PrimitiveSchema、`:1096` OpenAPIGenerator、`:1325` model_to_openapi_schema（正向）、`packages/sz-orm-core/src/schema_sync.rs:361` DdlGenerator | `openapi-reverse` feature gate，既有 OpenAPISpec/OpenAPIGenerator/正向生成保留 |
| REQ-V42-005 | WASM 真实数据库连接 | P3 | 浏览器端 ORM | Ubiquitous/Optional/Unwanted | HTTP/WS 代理桥接 + 代理协议 + WASI socket(可选) + 鉴权隔离 + 限流白名单 + 重连 | `packages/sz-orm-wasm/src/lib.rs:12` advanced、`:15` js_bindings、`:18` persistence、`:38` WasmQuery、`:67` WasmDatabase、`Cargo.toml:29` js feature、`:30` persistence feature | `wasm-real-db` feature gate，既有 WasmDatabase/advanced/js_bindings/persistence 保留 |

---

# 8. 验收标准总览

## 8.1 P1 类（最高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V42-001 | 跨语言参与者协议(gRPC/HTTP) + 适配器 + 补偿序列化 + 鉴权 + 崩溃恢复 + 故障隔离 + 复用既有 DtxManager | Go/Java 服务实现参与者协议注册 DtxManager 验证参与 Saga/TCC；协调器崩溃验证恢复；参与者超时验证故障隔离 |

## 8.2 P2 类（高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V42-002 | Go(cgo) + Java(JNI) + C++(extern C) 三套绑定 + 核心 API 覆盖 + FFI 内存安全 + 异步桥接 + 错误码映射 + 文档 | 三套绑定调用相同 CRUD 序列验证行为与 Rust 一致；FFI 边界验证 panic 捕获转错误码；异步验证不阻塞主线程 |
| REQ-V42-003 | Web UI + ER 图编辑 + 设计↔代码双向 + 五方言 DDL + 多格式导出 + 脱敏 + 复用既有 SchemaDiff/DdlGenerator | 浏览器设计表验证实时预览；设计→代码→设计验证双向一致；五方言验证 DDL 生成；导出验证多格式 |
| REQ-V42-004 | OpenAPI→Model/迁移/Repository + API 优先闭环 + 幂等 + 注入防护 + 可配置 + 复用既有 OpenAPI/DdlGenerator | OpenAPI spec 反向生成验证 Model/迁移/Repository；闭环验证 spec vs 正向生成；重复生成验证幂等；恶意 spec 验证注入防护 |

## 8.3 P3 类（中优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V42-005 | HTTP/WS 代理桥接 + 代理协议 + WASI socket(可选) + 鉴权隔离 + 限流白名单 + 重连 + 可观测 + 复用既有 WasmQuery | WASM 端查询验证经代理到后端 DB；未授权验证拒绝；DDL 验证白名单拒绝；超 QPS 验证限流；代理宕机验证重连 |

## 8.4 全局验收条件

1. **API 兼容性**：v4.2.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响。
2. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`cross-lang-dtx` / `lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` / `schema-designer` / `openapi-reverse` / `wasm-real-db`），默认 feature 行为不变。
3. **测试基线不回退**：v4.1.0 已验收测试基线不回退，v4.2.0 仅增不减。
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（Schema 设计器/OpenAPI 反向生成/WASM 真实 DB 按方言能力适配）。
5. **审计证据**：每项需求结论附 file:line 证据，遵循 AGENTS.md 审计合规铁律。
6. **14 道门禁通过**：v4.2.0 须通过 AGENTS.md 定义的 14 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入/feature 全组合/上游未改/文档一致/审计证据/文档同步）。
7. **无占位实现**：禁止 `todo!` / `unimplemented!` / `unreachable!`，所有新增代码须完整实现。
8. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释（FFI/JNI/cgo 边界须显式标注 SAFETY）。
9. **复用优先**：优先复用既有能力，不重复实现（如跨语言事务复用既有 DtxManager/saga/tcc/xa + sz-orm-grpc/sz-orm-queue；Go/Java/C++ 绑定复用既有 python/js 模式 + sz-orm-core API；Schema 设计器复用既有 SchemaDiff/diff/DdlGenerator；OpenAPI 反向生成复用既有 OpenAPISpec/Schema + DdlGenerator；WASM 真实 DB 复用既有 WasmQuery/WasmDatabase/js_bindings）。
10. **无 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。
11. **依赖关系**：REQ-V42-001（跨语言事务）复用既有 `sz-orm-dtx` + `sz-orm-grpc` + `sz-orm-queue`；REQ-V42-002（Go/Java/C++ 绑定）复用既有 `sz-orm-core` + python/js 模式；REQ-V42-003（Schema 设计器）复用既有 `sz-orm-core/schema_sync`；REQ-V42-004（OpenAPI 反向生成）复用既有 `sz-orm-swagger` + `sz-orm-core/schema_sync`；REQ-V42-005（WASM 真实 DB）复用既有 `sz-orm-wasm`；五项需求相互独立，可并行开发。

## 8.5 需求依赖关系

```plantuml
@startuml
title v4.2.0 需求依赖关系图
REQ-V42-001 "跨语言分布式事务" : 复用 sz-orm-dtx + sz-orm-grpc + sz-orm-queue
REQ-V42-002 "Go/Java/C++ 绑定" : 复用 sz-orm-core + python/js 模式
REQ-V42-003 "可视化 Schema 设计器" : 复用 sz-orm-core/schema_sync
REQ-V42-004 "OpenAPI → ORM 反向生成" : 复用 sz-orm-swagger + sz-orm-core/schema_sync
REQ-V42-005 "WASM 真实数据库连接" : 复用 sz-orm-wasm

REQ-V42-001 ..> REQ-V42-002 : 跨语言事务参与者可经语言绑定接入(可选协同)
@enduml
```

> 说明：五项需求主体相互独立，可并行开发。REQ-V42-001 与 REQ-V42-002 存在可选协同（跨语言事务参与者可通过语言绑定接入，但非强依赖，参与者亦可经 gRPC/HTTP 协议直接接入）。