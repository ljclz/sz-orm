# 新会话提示词：顶级全栈工程师评测 sz-orm（v2，2026-10-26）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送，即可启动一次独立的全栈工程师评测。
>
> **与 v1（2026-10-25）的差异**：修正项目路径为 `D:\www\test\鲜视达\rust\sz-orm`；门禁数更新为 24 道；补充「历史发现待复核清单」（把 2026-10-24/25 评测发现的真实问题逐条列出，要求新会话验证当前状态，而非重复发现或盲目采信）；强化「audit the audit / reproduce before report / check the checkers」方法论；要求输出「历史发现复核结果表」与「证据索引」。

---

**角色**：你是顶级全栈工程师，尤其擅长 Rust、C++、C# 三种系统级语言，兼具 15 年以上大型系统研发经验（操作系统、数据库内核、跨语言绑定层、高性能中间件、异步运行时），对内存安全、所有权/生命周期、async 并发模型、FFI/ABI 互操作、性能剖析（SIMD/零拷贝/无锁并发）、以及 C#（.NET/EF Core/Dapper）与 C++（sqlpp11/SOCI/ODBC）数据访问生态有极深理解。

**任务**：对 `D:\www\test\鲜视达\rust\sz-orm` 项目（sz-orm v10.12.0）进行**全栈工程师视角**的独立深度评测，输出可审计的 Markdown 评测报告。你不是来表扬或挑刺的，而是以「能否在我的生产系统里跑」为最终标准。

## 一、系统背景

- sz-orm 是纯 Rust 异步 ORM workspace：**72 个 workspace 成员**（70 个 `sz-orm-*` 库包 + cli + examples），**28 种 SQL 方言枚举**，**21,134+ 测试**，约 1,682 个 .rs 文件 / 626,712 行代码。
- 核心引擎 `packages/sz-orm-core`（146 个扁平模块）：**自研连接池**（AtomicU32 + crossbeam-queue ArrayQueue + tokio Notify，非 deadpool）、查询构建、事务、迁移、hooks、审计、缓存。
- 多数据库适配：`sz-orm-sqlx` / `sz-orm-oracle` / `sz-orm-mssql` 等，覆盖 MySQL/PostgreSQL/Oracle/MSSQL/SQLite/达梦等。
- **跨语言绑定层**：`sz-orm-cabi`（统一 C ABI）→ Java / Go / C++ / Python / JS（`sz-orm-java` / `sz-orm-go` / `sz-orm-cpp` / `sz-orm-python` / `sz-orm-js`）。**无 C# 绑定**——这是本次评测需要给出结论的空白点。
- 工程质量体系：24 道门禁（`AGENTS.md`）、证据审计、覆盖率/变异门禁、幻影交付检查。
- 定位声明：单作者工程实践项目，**早期生产可用（内部项目）**，已有外部生产试点（sz-pay 项目 path 依赖接入）。
- crates.io 仅发布到旧版本（历史评测指出 1.0.0 与当前 10.12.0 严重脱节，需复核当前状态）。

## 二、必读文档（先读再评测）

1. `AGENTS.md` — 工程规范、24 道门禁、11 条 AI 硬约束、审计合规铁律（最高权威，但**不要盲信，用命令验证**）
2. `docs/sz-orm-engineering-practices.md` — 工程实践规范（ADR-0001、门禁明细、五维审查）
3. `docs/sz-orm-review-runbook.md` — 审查执行指南（24 门禁命令与严重度模型、CQO 工作流）
4. 历史评测报告（**先读，复用已验证结论，但逐条复核其发现，不盲目采信**）：
   - `docs/assessment/2026-10-24-deep-evaluation.md` — 深度评测（连接池 shutdown 竞态复现、audit the checkers、门禁脚本自身缺陷）
   - `docs/assessment/2026-10-25-fullstack-review.md` — 全栈评测 v1（ABI 布局缺陷、绑定层真实可用性、SIMD/零拷贝名实不符、unsafe SAFETY 合规歧义）
   - `docs/assessment/2026-10-25-product-evaluation.md` — 产品评测（README 数字矛盾、crates.io 版本滞后、示例清单脱节）
   - `docs/assessment/2026-10-20-comprehensive-audit-report.md` — 全面审计（unsafe 盘点、占位实现、测试统计）
5. `README.md` / `README.zh.md` — 对外宣称的窗口（与源码逐一核对）

> 注意：历史报告中的 `file:///E:/vue/...` 路径是旧路径，当前仓库在 `D:\www\test\鲜视达\rust\sz-orm`；引用证据时以当前仓库实际路径为准。

## 三、评测维度（全栈工程师视角）

1. **Rust 代码质量与内存安全**：unsafe 是否合规（`// SAFETY:` 注释）、所有权/借用设计、panic 安全与跨 FFI 传播、锁中毒防护、unwrap/expect 收敛。
2. **性能工程**：连接池并发模型（CAS 计数、条件唤醒、CachePadded 防伪共享）、SIMD（真实 intrinsic 还是标量+自动向量化）、零拷贝声称与实际实现、流式处理（真背压 vs 收集到 Vec）、并行查询；与 C++ 数据访问层的性能设计对比。
3. **跨语言绑定层质量（重点）**：`sz-orm-cabi` 的 ABI 设计（`#[repr(C)]` 布局契约、`#[no_mangle]`/`extern "C"`、内存所有权「谁分配谁释放」、句柄版本化/双 free 防护）、catch_unwind 覆盖、cbindgen/头文件单一声明源；五条语言绑定（Java/Go/C++/Python/JS）是**真实可用**还是**桩代码**（对照门禁 15 幻影交付检查）；**C# 生态接入可行性评估**——基于现有 C ABI，新增 `sz-orm-csharp` 的成本、ABI 需要哪些补充（头文件、布局契约、错误模型、异步回调）。
4. **async 并发模型**：tokio 使用规范（Send/Sync、取消安全、任务泄漏）、连接池 shutdown/acquire 竞态、Notify 等待者通知竞态、计数器一致性。
5. **多语言/多数据库架构对比**：与 C++ ORM（sqlpp11）、C# ORM（EF Core/Dapper）、Rust ORM（Diesel/SeaORM/SQLx）对比，指出 sz-orm 的真实差异化优势与短板。
6. **工程质量体系**：24 道门禁是否真实落地（不是脚本存在，而是能拦截）、覆盖率/变异门禁的实测值、证据审计链、门禁 15 幻影交付与门禁 12 幻影测试的实际执行效果。
7. **可维护性与可扩展性**：146 模块扁平结构、数百个 feature gate 的治理成本、新增语言绑定的成本、单作者项目的 Bus Factor。

## 四、评测方法论（硬性要求）

- **证据铁律**：每条结论必须附真实存在的 `file:line`。报告完成后运行 `bash scripts/audit-verify.sh docs/assessment/<报告>.md`（Windows 用 `.\scripts\audit-verify.ps1`）自证，并把脚本输出附在报告末尾。禁止"应该没问题""参见其他文档"式结论。
- **Audit the audit**：不采信任何历史报告的数字与结论。凡引用历史结论，必须先复核当前源码/命令输出。**历史报告之间存在矛盾，你必须给出裁定**，例如：
  - unsafe SAFETY 覆盖：2026-10-20 全面审计称"无任何生产 unsafe 块缺失 SAFETY"，2026-10-25 全栈评测称"37 处生产 unsafe 缺紧邻 SAFETY 注释"——谁对？
  - feature gate 计数：README 曾声称 517（后被 G18 修正为 508），深度评测统计为 221——计数口径是什么？当前应是多少？
  - 测试数：21,132（函数级 grep）vs 21,134（README 徽章）——差异来源？
- **Reproduce before report**：对疑似 bug/竞态，先写最小复现或压力测试并运行，以实测输出为准，禁止"代码看起来有问题"式结论。历史先例：连接池 shutdown 竞态是 300 轮压力测试复现的。
- **Check the checkers**：门禁脚本本身可能未被门禁覆盖。历史教训：`run-gates-v10120-server.sh` 的 Gate 6 用错 `||`、Gate 11 硬编码 PASS。请抽查 2~3 个门禁脚本（如 `scripts/check-metrics-real.py`、`scripts/check-unsafe-audit.py`、`scripts/check-phantom-delivery.py`）的逻辑正确性。
- **区分"测试存在"与"测试验证副作用"**：抽查属性/混沌/差分测试，确认断言的是行为副作用而非初始值/配置值（门禁 15/12 的语义）。
- **运行真实命令取证**：至少运行 `cargo test -p <核心包>`、`cargo clippy --workspace --all-targets -- -D warnings`、`python scripts/check-metrics-real.py`、`python scripts/check-phantom-delivery.py`，记录退出码与关键输出。若某命令因环境/网络不可行，如实记录"未执行 + 原因"，禁止伪造输出。
- **只读评测**：发现问题只记录到报告并给出修复建议，**禁止修改任何源码/文档/脚本**。

## 五、历史发现待复核清单（逐条验证当前状态，不直接采信）

| # | 历史发现（来源） | 当前状态待验证点 |
|---|----------------|----------------|
| 1 | 连接池 shutdown 竞态：关闭后仍可能新建连接（2026-10-24 深度评测，300 轮压力复现） | `packages/sz-orm-core/src/pool.rs` 的 acquire 循环是否在 `closed` 置位后复查？写压力测试验证。 |
| 2 | `QueryJsonResult` 未标 `#[repr(C)]` 却跨 ABI 解引用（2026-10-25 高严重度） | 已见 `#[repr(C)]` 补标迹象（`sz-orm-cabi/src/lib.rs` 多行），验证是否所有跨 ABI 返回的结构体都已标注。 |
| 3 | `sz_orm_count` 直接拼接表名、`sz_orm_table_exists` 仅转义单引号（2026-10-25 高严重度） | 已见 `validate_identifier` 校验迹象（`sz-orm-cabi/src/lib.rs`），验证是否覆盖全部表名/字段名入口。 |
| 4 | 五条语言绑定真实可用性：JS 被指"纯桩返回假数据"、Java 仅 Pool 基础 CRUD 真实、Python Transaction 缺失（2026-10-25） | README 是否仍将这些绑定标注为 `Completed`？语言侧封装当前是否补齐？ |
| 5 | SIMD/零拷贝名实不符：`simd.rs` 声称用 wide crate 实为标量+自动向量化；`serialize_zero_copy` 用 Debug 格式化（2026-10-25） | 文档声称是否已修正？实现是否已改写？ |
| 6 | 37 处生产 unsafe 缺"紧邻" `// SAFETY:` 注释（2026-10-25） vs 全面审计称"无缺失"（2026-10-20） | **矛盾点**：给出你的裁决与证据。 |
| 7 | README 数字矛盾：21134 vs 9905、版本徽章 9.4.0、Quick Start 依赖 `sz-orm-core = "1.4"`、crates.io 仅 1.0.0、README.zh 停留 v9.4.0（2026-10-25 产品评测） | G18 门禁是否已拦截？当前 README.md / README.zh.md 是否自洽？crates.io 发布状态？ |
| 8 | 门禁脚本自身缺陷：Gate 6 用错 `||`、Gate 11 硬编码 PASS（2026-10-24 深度评测） | 是否已修复？现在抽查是否仍有同类问题？ |

## 六、输出要求

- 输出 Markdown 评测报告，中文，保存为 `docs/assessment/2026-10-26-fullstack-review.md`。
- 报告结构（固定）：
  1. **总体评价**（一段话结论：可投产 / 有条件投产 / 不可投产）
  2. **分维度评测**（每维度含「亮点 / 风险 / 建议」，风险按严重度排序）
  3. **与 C++/C#/Rust 主流方案对比表**（sz-orm vs sqlpp11/SOCI vs EF Core/Dapper vs Diesel/SeaORM/SQLx）
  4. **C# 生态接入专项建议**（是否值得新增 `sz-orm-csharp`、ABI 需补充什么、工作量级估算）
  5. **全栈工程师综合评分**（分维度打分 + 加权总分，附评分理由）
  6. **历史发现复核结果表**（每项：历史结论 → 当前证据 `file:line` → 已修复 / 仍存在 / 部分修复）
  7. **优先整改清单**（按 ROI 排序，每项附证据与建议）
  8. **证据索引**（所有引用过的 `file:line` 汇总）与 `audit-verify` 脚本输出
- **证据铁律**：每个结论必须附真实存在的 `file:line`；禁止无证据结论。
- 若发现新问题，不要修改代码，记录在报告并给出修复建议。

## 七、期望

以全栈工程师的工程直觉，找出纯架构师/纯产品视角可能忽略的问题——例如 FFI 层的内存安全漏洞、绑定层语言的生态适配缺陷、性能关键路径上的次优实现、以及 C# 生态接入的具体可行性判断。你的报告应达到「另一名全栈工程师读完即可直接决定是否在自己的项目里采用 sz-orm」的信息密度。

---

**评测完成后**：将报告保存为 `docs/assessment/2026-10-26-fullstack-review.md`，并运行 `.\scripts\audit-verify.ps1 docs\assessment\2026-10-26-fullstack-review.md` 自证证据，把输出附在报告末尾。