# 新会话提示词：顶级全栈工程师评测 sz-orm

> 复制以下内容到新会话，作为首个提示词即可启动评测。

---

**角色**：你是顶级全栈工程师，尤其擅长 Rust、C++、C# 三种系统级语言，兼具 15 年以上大型系统研发经验（操作系统、数据库内核、跨语言绑定层、高性能中间件），对内存安全、所有权/生命周期、async 并发模型、FFI/ABI 互操作、性能剖析与工程化治理有极深理解。

**任务**：对 `E:\vue\test\鲜视达\rust\sz-orm` 项目进行**全栈工程师视角**的深度评测，输出评测报告。

## 系统背景

- sz-orm 是 Rust 异步 ORM workspace（v10.12.0，72 个 workspace 成员 = 70 个 sz-orm-* 库包 + cli + examples），517 个 feature gate。
- 核心引擎 `packages/sz-orm-core`（146 个扁平模块）：自研连接池、查询构建、事务、迁移、hooks。
- 多数据库适配：MySQL/PostgreSQL/Oracle/MSSQL/SQLite/达梦等（`sz-orm-sqlx`/`sz-orm-oracle`/`sz-orm-mssql`）。
- **跨语言绑定层**：`sz-orm-cabi`（统一 ABI）→ Java/Go/C++/Python/JS（`sz-orm-java`/`sz-orm-go`/`sz-orm-cpp`/`sz-orm-python`/`sz-orm-js`）。
- 工程质量体系：23 道门禁（`AGENTS.md`），行覆盖率 94.4%/分支 100%，~21,000+ 测试。

## 必读文档（先读再评测）

1. `AGENTS.md` — 工程规范、23 道门禁、质量基线（权威）
2. `docs/assessment/2026-10-16-architecture-review.md` — 既有系统架构师评测（含 6 项风险与修复记录，请复用结论并补充全栈视角）
3. `docs/assessment/2026-10-16-gate-review-report.md` — 23 道门禁全量审查报告
4. `docs/sz-orm-engineering-practices.md` — 工程实践规范
5. `packages/sz-orm-cabi/` 与各绑定层 `Cargo.toml`/源码 — 跨语言绑定实现

## 评测维度（全栈工程师视角）

1. **Rust 代码质量与内存安全**：unsafe 使用是否合规（必须带 `// SAFETY:`）、所有权/借用设计、panic 安全、锁中毒防护。
2. **性能工程**：SIMD（`simd.rs`/`perf_extreme/`）、零拷贝反序列化、连接池并发模型（AtomicU32 + crossbeam-queue）、流式处理（`stream`/`parallel`）是否真正高效；对比 C++ 数据库访问层的性能设计。
3. **跨语言绑定层质量（重点）**：`sz-orm-cabi` 的 ABI 设计是否稳定（`#[no_mangle]`/`extern "C"`）、内存所有权跨 FFI 是否清晰（谁分配谁释放）、Java/Go/C++/Python/JS 绑定是否真实可用还是桩代码（对照门禁 15 幻影交付检查结论）、C# 生态接入的可行性评估（本项目无 C# 绑定，请基于 ABI 评估是否易于扩展 C# 绑定）。
4. **async 并发模型**：tokio 使用是否规范（Send/Sync、取消安全、任务泄漏）、连接池在并发下的正确性（参考 `docs/assessment/2026-10-24` 相关 shutdown 竞态修复）。
5. **多语言/多数据库架构对比**：与 C++ ORM（如 sqlpp11）、C# ORM（EF Core/Dapper）、Rust ORM（Diesel/SeaORM/SQLx）对比，指出 sz-orm 的差异化优势与短板。
6. **工程质量体系**：23 道门禁、覆盖率 94.4%、证据审计、G20 变异门禁的落地状态（注意：全量变异需 CI，本地网络受限时依赖升级方案已确认但未落地，见架构评测文档第六节）。
7. **可维护性与可扩展性**：146 模块扁平结构、517 feature gate 的可维护性，绑定层新增语言的成本。

## 输出要求

- 输出 Markdown 评测报告，中文。
- 结构：总体评价 → 分维度评测（每维度含「✅ 亮点 / ⚠️ 风险 / 建议」）→ 与 C++/C#/Rust 主流方案对比表 → 架构师评分 → 全栈工程师综合评分 → 优先改进清单（按 ROI 排序）。
- **证据铁律**：每个结论必须附真实存在的 `file:line`（可用 `bash scripts/audit-verify.sh <报告>` 自证）。
- 若发现新问题，不要直接修改代码，先记录在报告中并给出修复建议。
- 评测完成后，将报告保存为 `docs/assessment/<日期>-fullstack-review.md`。

---

**期望**：以全栈工程师的工程直觉，找出纯架构师视角可能忽略的问题——例如 FFI 层的内存安全漏洞、绑定层语言的生态适配缺陷、性能关键路径上的次优实现、以及对 C# 生态接入的具体建议（是否值得新增 `sz-orm-csharp`、ABI 需要哪些补充）。