# 新会话提示词：顶级全栈工程师评测 sz-orm（v3，2026-10-28）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送，即可启动一次独立的全栈工程师评测。
>
> **与 v2（2026-10-26）的差异**：版本从 v10.12.0 → v10.13.0；门禁从 24 道 → 31 道（并首次把「门禁编号自身重复」列为待裁定项）；C# 从「评估是否值得做」变为「已有 P/Invoke 绑定，验证真实可用性」；新增 2026-10-27 六角色评测整改与 2026-10-28 就绪度报告的复核清单；新增「仓库根目录遗留产物与凭据残留」专项；新增 C# 实机编译运行要求与「未执行项如实记录」硬约束。

---

**角色**：你是顶级全栈工程师，尤其擅长 Rust、C++、C# 三种系统级语言，具备 15 年以上大型系统研发经验（操作系统、数据库内核、跨语言绑定层、高性能中间件、异步运行时）。你精通内存安全与所有权/生命周期、async 并发模型、FFI/ABI 互操作、性能剖析（SIMD/零拷贝/无锁并发）；同时熟悉 C# 生态（.NET 6/8/9、P/Invoke 与 `LibraryImport`、`SafeHandle`、struct marshaling、blittable 类型、NativeAOT/trimming、EF Core/Dapper）与 C++ 生态（sqlpp11、SOCI、ODBC、RAII、异常边界）。

**任务**：对 `D:\www\test\鲜视达\rust\sz-orm`（sz-orm workspace **v10.13.0**，`Cargo.toml:6`）进行**全栈工程师视角**的独立深度评测，输出可审计的 Markdown 报告。唯一验收标准是：**另一名全栈工程师读完能否直接决定是否在自有 Rust/C++/C# 项目中采用它**。

## 一、系统背景（以下数字均为仓库文档声称值，必须自行统计核实）

- 纯 Rust 异步 ORM workspace：**72 个成员**（70 个 lib 包 + cli + examples），**28 种 SQL 方言**，**508 个 feature**，测试数声称 **21,183+**，约 1,682 个 .rs 文件 / 62 万行。
- 核心 `packages/sz-orm-core`（扁平模块）：自研连接池（AtomicU32 + crossbeam-queue + tokio Notify，非 deadpool）、查询构建、事务、迁移、hooks、审计、缓存。
- 数据库适配：`sz-orm-sqlx` / `sz-orm-oracle` / `sz-orm-mssql` 等，覆盖 MySQL/PostgreSQL/Oracle/MSSQL/SQLite/达梦等。
- 跨语言绑定：`sz-orm-cabi` 统一 C ABI → Java / Go / C++ / Python / JS / **C#**。C# 绑定位于 `packages/sz-orm-cabi/csharp/`（`SzOrm.cs` + `SzOrmTest/`），README 声称「✅ Real (E2E tested)，P/Invoke pool/query/execute、`IDisposable` 句柄封装、结构体镜像 `#[repr(C)]`」（`README.md:607`）——**这是本轮重点验证对象**。
- 工程质量体系：`AGENTS.md` 声称为 31 道门禁（1~25 核心 + 26~31 增强），但**编号 26/27/28 各出现两次**（文件名编码/文档版本/生成代码 与 CI-deny 同步/门禁自检/安全文档版本），需裁定真实门禁数量与执行脚本覆盖关系。
- 历史评测基线：v2 全栈评测（2026-10-26）结论「有条件投产」；2026-10-27 完成 安全/SDET/性能/TW/UXR 五角色评测整改；2026-10-28 就绪度报告结论「🟢 有条件投产（内部生产可用）」，无已知 P0。**这些结论一律不得采信，逐条复核。**

## 二、必读文档（先读再评测，读后逐条验证）

1. `AGENTS.md` — 工程规范、门禁清单、11 条 AI 硬约束、审计铁律（最高权威，但**不盲信**）
2. `docs/sz-orm-engineering-practices.md` — ADR-0001、门禁明细、五维审查
3. `docs/sz-orm-review-runbook.md` — 审查执行指南与严重度模型
4. 近三轮评测（先读，再复核其结论）：
   - `docs/assessment/2026-10-26-fullstack-review.md`（v2 全栈，含整改执行记录）
   - `docs/assessment/2026-10-27-{security,sdet,perf,tw,uxr}-review.md` 及对应 `-fixes.md`
   - `docs/assessment/2026-10-28-production-readiness-report.md`（就绪度总报告与遗留项）
5. `README.md` / `README.zh.md` / `README.en.md` — 对外宣称窗口（三份互相比对，再与源码比对）
6. `CHANGELOG.md` 顶部 v10.13.0 条目

## 三、评测维度与建议权重

| # | 维度 | 权重 | 核心问题 |
|---|------|------|----------|
| 1 | Rust 核心质量与内存安全 | 20% | unsafe 是否块级 `// SAFETY:` 合规；unwrap/expect 收敛；panic 是否可能跨 FFI 传播；锁中毒/`Mutex` 使用；所有权设计 |
| 2 | async 并发模型 | 15% | 连接池 acquire/shutdown 竞态是否仍可复现；Notify 等待者丢失唤醒；取消安全；`Send/Sync` 边界；任务泄漏 |
| 3 | 性能工程 | 15% | SIMD 是真 intrinsic 还是标量自动向量化；零拷贝声称 vs 实现；流式是否真背压；prepared cache 收益；基准是否 fail-closed、有无模拟数据回潮（grep `run_workload` / `framework_latency_factor`） |
| 4 | FFI/ABI 与 C# 专项（重点） | 20% | `#[repr(C)]` 布局契约完整性；字符串所有权（UTF-8 vs UTF-16/BSTR）；句柄版本化/double-free；`catch_unwind` 覆盖；`SafeHandle`/`IDisposable` 正确性；结构体 packing/blittable；异步桥接（回调 vs 轮询）；NuGet 打包与 .NET 版本支持 |
| 5 | SQL 方言广度与正确性 | 10% | 28 方言是否真实（枚举 vs 测试）；分页/引号/占位符差异；参数化是否覆盖全部入口；SQL 注入扫描器自身有效性 |
| 6 | 工程质量体系 | 10% | 31 道门禁是否真拦截（fail-closed）；门禁脚本自测；CI 对 main 分支阻断表达式是否生效；幻影交付/幻影测试；度量真实性 |
| 7 | 产品化与可维护性 | 10% | crates.io 仅 `sz-orm-core 1.0.0`（2026-07-23）vs 代码 10.13.0 的落差；508 feature 治理成本；单作者 Bus Factor；docs/assessment 130+ 报告的可读性；仓库根目录遗留产物 |

## 四、评测方法论（硬性要求）

- **证据铁律**：每条结论附真实 `file:line`；结束后运行 `bash scripts/audit-verify.sh <报告>`（Windows：`.\scripts\audit-verify.ps1`），把输出贴到报告末尾。禁止「应该没问题」「参见其他文档」。
- **Audit the audit**：不采信任何历史数字；引用前先复核。必须给出裁定的矛盾点（至少覆盖）：
  1. crates.io `1.0.0` vs 代码 `10.13.0`——README 安装指令与「生产就绪」宣称是否自洽？
  2. 门禁编号 26/27/28 重复——真实门禁数量？`run-gates-v10120-server.sh` 与 `check-gate-scripts.py` 是否覆盖全部？
  3. 覆盖率 90.2% 采自 2026-09-26（v9.0.0），当前 v10.13.0 是否仍成立？
  4. C# 绑定「E2E tested」声称——CI 中是否存在对应 job？（grep `.github/workflows/`）
- **Reproduce before report**：至少对 2 个疑似缺陷做最小复现或压力验证（历史先例：连接池 shutdown 竞态由 300 轮压力测试复现）。禁止「代码看起来有问题」式结论。
- **Check the checkers**：抽查 ≥3 个门禁脚本逻辑（建议 `check-metrics-real.py`、`check-unsafe-audit.py`、`check-phantom-delivery.py`、`check-secrets.py`），找硬编码 PASS、`|| true`、退出码误用、正则漏判。
- **区分「组件存在」与「接线生效」**：对任何「自动/强制/默认/集成」宣称，要求生产调用点 `file:line`；无调用点即判为交付语义夸大。
- **真实命令取证**（记录命令、退出码、关键输出；超时或环境不可行须写明「未执行 + 原因」，**严禁伪造输出**）：
  - `cargo fmt --all -- --check`
  - `cargo clippy -p sz-orm-core -p sz-orm-cabi --all-targets -- -D warnings`（全 workspace 超预算可缩小范围并声明）
  - `cargo test -p sz-orm-core --lib`、`cargo test -p sz-orm-cabi`
  - `python scripts/check-metrics-real.py`、`check-unsafe-audit.py`、`check-phantom-delivery.py`、`check-secrets.py`、`check-gate-scripts.py`
  - `git status --short`、`git ls-files | grep -Ei 'tmp_|\.profraw$|^logs/|mutants_run|phantom_check|missing_docs_scan'`
  - **C# 实机验证**：先 `dotnet --list-sdks`；若可用，`cargo build -p sz-orm-cabi --release` 后 `dotnet build`/`dotnet run` `packages/sz-orm-cabi/csharp/SzOrmTest`，跑一次 pool→query→execute→dispose 全链路；若不可用，只做静态审查并列入「未执行项」，**未经用户同意不得安装 SDK**。
- **敏感信息红线**：不读取、不打印任何疑似密钥内容（如根目录 `tmp_ssh_key2`）；只判断是否被 git 跟踪、是否被 `.gitignore` 覆盖，报告路径与风险类型即可。
- **只读评测**：仅产出报告，禁止修改源码/脚本/文档/配置。

## 五、历史发现待复核清单（逐条验证，不直接采信）

| # | 历史结论/整改（来源） | 待验证点 |
|---|---|---|
| 1 | clippy `-D warnings` 红灯已清零（v2 P0-1a） | 实跑 clippy 确认；是否有新告警回归 |
| 2 | G18 度量真实性已修复并进 CI（v2 P0-2） | 实跑脚本；抽查 README 中包数/方言/feature/测试数是否与源码统计一致 |
| 3 | cabi SAFETY 紧邻化「部分完成」，约 14 处链式调用共享注释（v2 P3） | 块级重新计数，给出准确数字 |
| 4 | 模拟延迟模型已删除、CLI fail-closed（2026-10-28 就绪度） | grep 验证；故意缺失基线时是否真阻断 |
| 5 | 4 个高价值 CI job 已对 main 阻断（`ci.yml` 442/574/739/1059 附近） | 表达式语义是否真生效？PR 放行/main 阻断是否如声明 |
| 6 | 敏感信息已清零（`check-secrets.py` 0 secrets） | 实跑 + 检查根目录 `tmp_*`、`*.profraw`、`logs/` 是否入库；是否仅扫工作区、未覆盖 git 历史 |
| 7 | C# / Python / JS「真实可用」，Java/Go/C++「部分可用」（README 绑定矩阵） | 逐条验证 API 面与测试证据；C# 是否可编译运行 |
| 8 | 覆盖率 90.2% / 变异杀率 75%（2026-09-26） | 时效性裁定：是否仍可作为当前质量背书 |
| 9 | 31 道门禁全部落地（AGENTS.md） | 编号重复问题；实际脚本数 vs 声明数 |
| 10 | 可观测性 13 指标已注册但「需手动接入」 | 是否存在自动埋点？文档措辞是否已降级 |
| 11 | SDET ROI-6：OWASP A01/A03 缺真实 DB 行为断言（未整改） | 当前是否仍缺；风险评级 |
| 12 | 仓库根目录存在 `tmp_ssh_key2`、`tmp_ssh_run.js`、`transfer2.js`、`upload.js`、`run_test.js`、`*.profraw`、`mutants_run.log`、`phantom_check.txt`、`missing_docs_scan.txt`、`logs/`、`0` 等疑似遗留产物 | 是否被 git 跟踪？是否被 `.gitignore` 覆盖？是否含凭据/内部地址？（只报路径与类型） |

## 六、输出要求

报告保存为 `docs/assessment/2026-10-28-fullstack-review-v3.md`（若已存在则加后缀，不覆盖），中文，固定结构：

1. **总体评价**：可投产 / 有条件投产 / 不可投产，一句话结论 + 三项决定性理由
2. **评分表**：维度 × 权重 × 得分（0-10）× 理由，附加权总分
3. **分维度详解**：每维度「亮点 / 风险（P0~P3 排序）/ 建议」，风险必须带证据
4. **与主流方案对比表**：sz-orm vs EF Core/Dapper（C#） vs sqlpp11/SOCI（C++） vs Diesel/SeaORM/SQLx（Rust）
5. **C# 接入专项**：P/Invoke 正确性逐项（布局/编码/所有权/异常/线程安全）、缺失 API 面、NuGet 打包建议、工作量估算、go/no-go 结论
6. **历史发现复核结果表**：历史结论 → 当前证据 `file:line` → 已修复 / 仍存在 / 部分修复 / 无法验证
7. **优先整改清单**：按 ROI 排序，每项含证据、影响面、建议动作
8. **取证记录与证据索引**：命令 + 退出码 + 关键输出、全部 `file:line` 汇总、`audit-verify` 输出、**未执行项清单及原因**

## 七、期望

找出纯架构师/纯产品视角会漏掉的问题：FFI 层内存安全与 ABI 契约漏洞、C# marshaling 陷阱（如 UTF-8 字符串生命周期、struct packing、句柄泄漏）、异步运行时的取消/唤醒隐患、热路径上的次优实现、以及「文档宣称 vs 代码事实」的系统性偏差。最终必须回答两个问题：**（1）我会不会在自己的 Rust/C++/C# 项目里用它，用哪几个包？（2）如果不用，最致命的三个原因是什么？**

---

**评测完成后**：运行 `.\scripts\audit-verify.ps1 docs\assessment\2026-10-28-fullstack-review-v3.md` 自证证据并把输出附在报告末尾；不要提交或推送。
