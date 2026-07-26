# 双轨影子流量校验（Skill 5）可行性评估报告

> 评估日期：2026-07-25
> 评估对象：sz-orm 工作空间
> SKILL 来源：`.trae/skills/sz-orm-shadow-traffic/SKILL.md`

---

## 一、当前项目基础设施现状

### ✅ 已有的能力

| 能力 | 存在位置 | 说明 |
|------|---------|------|
| `tracing` 依赖 | 根 `Cargo.toml` workspace 依赖 | `tracing = "0.1"`，已用于链路追踪 |
| `metrics` 依赖 | 根 `Cargo.toml` workspace 依赖 | `metrics = "0.23"`，可用于直方图记录 |
| 指标注册中心 | `sz-orm-observability` 包 | `MetricsRegistry`：Counter/Gauge/Histogram，支持 Prometheus 文本格式 |
| SLO 监控 | `sz-orm-observability/src/slo.rs` | Google SRE 4 窗口燃烧率监控 |
| 延迟直方图 | `sz-orm-tracing` 包 | `LatencyHistogram`：排序数组实现，支持任意分位数 |
| 分布式链路追踪 | `sz-orm-tracing` 包 | Span/Tracer 抽象，W3C TraceContext，OTLP exporter |
| 连接层抽象 | `sz-orm-core` 的 `Connection` trait | 提供 `query`/`execute`/`query_with_params` 等方法 |
| sqlx 驱动适配 | `sz-orm-sqlx` 包 | MySQL/PostgreSQL/SQLite/Oracle 的完整适配器实现 |
| 长时 Soak 测试 | `.github/workflows/soak.yml` + `soak-self-hosted.yml` | 已有 6h (托管 runner) 和 24h (self-hosted) 基础设施 |

### ❌ 缺失的能力

| 缺失项 | 详细说明 |
|--------|---------|
| `tower` 直接依赖 | `tower` 仅出现在 `Cargo.lock` 的间接依赖中（v0.4.13 和 v0.5.3），**没有任何 Cargo.toml 直接引用** |
| middleware 模块 | 项目无 `src/middleware/` 目录，无 `Service` trait 实现，无中间件栈 |
| Shadow/Dual-Write 机制 | 无任何影子流量、双写或镜像连接的代码实现 |
| QueryBuilder 结果比较器 | 无 ORM 查询结果与原生 SQL 结果的结构化比较逻辑 |
| 72h 连续运行基础设施 | 最长 soak 测试为 24h（self-hosted），需要生产级 72h 部署环境 |
| `SHADOW_72H_REPORT.pdf` 生成 | 无 PDF 报告生成能力 |

---

## 二、方案本质分析

SKILL.md 描述的影子流量校验实质上是一个 **数据库查询层中间件**，在 ORM 查询执行时**同时**通过原生 sqlx 驱动执行相同的 SQL，然后比较两个结果集。这与典型的 HTTP 中间件模式不同——sz-orm 是一个 ORM 库，不是 HTTP 服务。

核心架构需要：

```
应用代码 → QueryBuilder → ORM Connection(query)
                                        │
                                        ├──→ 执行 ORM 查询 → 返回结果 A
                                        │
                                        └──→ 提取 SQL → 通过原生 sqlx 执行 → 返回结果 B
                                             │
                                             └──→ 比较 A vs B（行数、逐行、延迟）
```

### 关键设计难点

1. **`Connection` trait 是 `&mut self`** — 意味着一个连接不能在影子模式下同时服务于 ORM 查询和原始 SQL 查询。需要一个 `ShadowConnection` 包装器，内部持有两个独立的数据库连接（ORM 连接 + 原生连接）。

2. **QueryBuilder 只生成 SQL，不执行**（见 ADR-0009）— 这使得提取 SQL 字符串很自然，但需要将 `Value` 参数列表正确地翻译为原生 sqlx 绑定。

3. **结果比较的语义** — `HashMap<String, Value>` 的逐字段比较需要考虑类型敏感度（如 `I64(42)` vs `F64(42.0)`）、列顺序、NULL 处理等。

4. **影子连接的超时与故障隔离** — 影子路径失败不应影响主路径，需要 `tokio::time::timeout` + 静默降级。

---

## 三、实施阶段预估

### 阶段一：基础影子中间件（~3 天）
- 注册 `tower`/`tower-service` 作为直接依赖
- 在 `sz-orm-observability`（或新建 `sz-orm-shadow` 包）中实现 `ShadowConnection` 包装器
- 支持 `query` 和 `query_with_params` 的影子执行
- 基本的 tokio::join 并发执行 + 3s 超时

### 阶段二：结果比较与指标（~2 天）
- 实现 `HashMap<String, Value>` 的行数比较 + 逐行比较
- 通过 `MetricsRegistry` 记录 `sz_orm_shadow_mismatch_total` counter 和 `sz_orm_shadow_latency_seconds` histogram
- 差异报警（日志 + metrics）

### 阶段三：SQL 日志捕获与回放（~2 天）
- 利用 `tracing` 的事件/span 机制从真实应用中捕获 SQL
- 在本地/Staging 环境实现 SQL 回放框架

### 阶段四：72h 连续运行（~2 天）
- 配置生产级（或 Staging 级）72h 运行环境
- Grafana 仪表盘扩展（基于现有 `sz-orm-dashboard.json`）
- `SHADOW_72H_REPORT.pdf` 生成逻辑

**总计预估工作量：9 人天（不算测试和 CI 配置）**

---

## 四、可用依赖分析

### 已存在于 Cargo.lock 但需显式声明为直接依赖

| 依赖 | 用途 | 当前状态 |
|------|------|---------|
| `tower` | Service trait + Layer 中间件栈 | 仅间接依赖，需加入 Cargo.toml |
| `tower-service` | 极简 `Service` trait（如果不想引入完整的 tower） | 可选 |
| `pin-project` | 在异步中间件中安全地 pin | 未使用，可能需要 |
| `futures` | `FutureExt`, `try_join` 等 | 已在 workspace 依赖中 |
| `tokio` / `tokio-util` | 超时控制、异步执行 | 已齐全 |

### 需要新增的依赖

| 依赖 | 用途 |
|------|------|
| 无重大新增依赖 | 所有核心能力（metrics、tracing、async）均已就位 |
| `similar` 或自定义比较器 | 结果集结构化比较（可选，可用 HashMap 逐字段比较替代） |

---

## 五、本地可做的验证（不需要生产环境）

1. **单元测试** — 用 SQLite 内存数据库验证 `ShadowConnection`：
   - 同时打开两个 SQLite `:memory:` 连接
   - 在两边执行相同的 DDL + DML
   - 验证 ORM 查询 vs 原生 sqlx 查询结果一致

2. **指标验证** — 验证 MetricsRegistry 正确记录影子延迟和差异计数

3. **超时与隔离验证** — 模拟影子连接超时，验证主路径不受影响

4. **错误路径验证** — 模拟影子查询失败，验证主结果仍然返回

5. **Soak 扩展** — 在现有 24h soak 测试基础上，加入影子校验模式（可选）

---

## 六、结论

### 是否可以执行：**✅ 条件可行**

#### 可行理由
- 项目已有完善的可观测性基础设施（metrics + tracing + SLO 监控）
- `Connection` trait 抽象使 QueryBuilder → 原生 SQL 路径天然可分离
- 已有的长时 soak CI 基础设施（24h）可作为影子验证的运行平台
- 无重大的第三方依赖缺口

#### 限制条件
1. **SKILL.md 中的 `tower` 标签具有误导性**：sz-orm 是库而非 HTTP 服务，`tower::Service` 模式并不直接适用于数据库连接层。更自然的实现是 `ShadowConnection` 包装器（decorator 模式）而非 tower 中间件。
2. **生产环境 72h 连续运行需要 self-hosted runner 或专用服务器**：当前 CI 支持的最大 soak 是 24h（self-hosted）。72h 需要额外部署。
3. **SKILL.md 中的 `metrics::histogram!("orm.latency")` 宏语法与项目已有 `MetricsRegistry` 不兼容**：项目使用自定义 `MetricsRegistry` 而非 `metrics` crate 的宏。需要适配。
4. **结果比较的语义需要明确**：浮点数精度、NULL 处理、列顺序等边界情况需要明确定义通过/失败标准。

#### 建议
- **本次不执行实施**，因为该 SKILL 文档的目标是"上线重大性能优化时必触发"，属于事件驱动的流程。当有重大 ORM 查询变更时，再按上述阶段实施。
- 如果要在非生产环境做一轮可行性验证（基于 SQLite），可作为单独的任务安排。

---

## 附录

### 关键文件路径

| 文件 | 说明 |
|------|------|
| `.trae/skills/sz-orm-shadow-traffic/SKILL.md` | 影子流量 SKILL 定义 |
| `packages/sz-orm-core/src/query.rs` | QueryBuilder 定义（生成 SQL） |
| `packages/sz-orm-core/src/pool.rs` | Pool + Connection trait |
| `packages/sz-orm-sqlx/src/any.rs` | sqlx 适配器实现 |
| `packages/sz-orm-observability/src/lib.rs` | MetricsRegistry 实现 |
| `packages/sz-orm-tracing/src/lib.rs` | 链路追踪 + SLO 监控 |
| `.github/workflows/soak.yml` | 6h soak CI |
| `.github/workflows/soak-self-hosted.yml` | 24h self-hosted soak CI |
| `grafana/sz-orm-dashboard.json` | Grafana 仪表盘 |

### 与 SKILL.md 的差异说明

SKILL.md 中定义了 `tools: [tower, tracing, metrics]`，但实际项目中：
- `tower` — 无直接依赖，且不适合 ORM 库模式
- `tracing` — ✅ 已就位
- `metrics` — ✅ 已就位（但项目用自定义 `MetricsRegistry` 而非 `metrics` crate 宏）
