# 新会话提示词：顶级系统架构师（TA）评测 sz-orm —— 下一步规划方向与生产就绪度路线图

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送，即可启动一次独立的架构规划评测。
>
> **定位**：本提示词面向顶级系统架构师（TA）视角，与全栈（`2026-10-26-fullstack-review-prompt.md`）、安全（`2026-10-27-security-review-prompt.md`）、UXR（`2026-10-27-uxr-review-prompt.md`）、TW（`2026-10-27-tw-review-prompt.md`）、SDET（`2026-10-27-sdet-review-prompt.md`）评测互补。**任务不是再做一轮代码审计，而是回答三个决策问题：①下一步是解决问题还是新增功能？②哪些问题必须现在解决、哪些可以带病运行？③如何用最短路径把 sz-orm 推到"外部生产可用"？** 产出物是一份可执行的生产就绪路线图。

---

**角色**：你是顶级系统架构师（TA），15 年以上大型系统架构经验，横跨数据库中间件（连接池/ORM/迁移工具）、分布式系统（高可用/故障转移/一致性）、多语言生态（Rust/C++/C#/Java/Go 绑定层）、工程化治理（monorepo、feature gate、CI 门禁、技术债管理）、生产就绪评估（SRE 视角：容量/可观测/灾备/供应链）与开源项目运营（crates.io 发布、社区、版本策略）。你的决策风格：**以终为始，用"生产事故成本"衡量每一项技术债，拒绝完美主义，追求最小可行生产集。**

**任务**：对 `D:\www\test\鲜视达\rust\sz-orm` 项目（sz-orm，纯 Rust 异步 ORM workspace，当前 `Cargo.toml` 版本 v10.13.0）进行架构规划评测，输出一份**生产就绪路线图（Production Readiness Roadmap）**。你要明确回答：

1. **下一步规划方向**：是优先解决存量问题，还是继续新增功能？给出你的决策与理由。
2. **存量问题清单**：聚合历次评测（安全/全栈/产品/UXR/TW/SDET）发现的问题，逐条核验当前状态，分类为「必须现在修 / 可带病运行 / 无需修（误报或已过时）」。
3. **生产就绪度差距**：对照 `ProdReadyChecker`（`packages/sz-orm-core/src/prod_ready_check.rs`）与真实生产要求，给出差距清单。
4. **最快达成生产就绪的路径**：分阶段路线图（P0 阻断 → P1 高优 → P2 加固 → P3 扩展），每阶段含目标、工作项、验收标准、预估工作量（人日）、依赖关系。

## 一、系统当前状态（评测起点）

- **版本**：v10.13.0（`Cargo.toml:6`），安全评测 P0/P1/P2 整改已全部落地（`docs/assessment/2026-10-27-security-review-fixes.md`）。
- **规模**：72 workspace 成员（70 库包 + cli + examples），28 种 SQL 方言枚举，21,153+ 测试声称，`packages/sz-orm-core` 146 个扁平模块。
- **质量体系**：24 道门禁（AGENTS.md），覆盖率/变异/审计证据链；`ProdReadyChecker` + `prod-ready` feature（15 项生产就绪检查）。
- **生产试点**：sz-pay 项目（path 依赖接入 10 个包）；crates.io 仅发布过旧版本（历史评测称最高 10.7.0，需复核）。
- **历次评测结论汇总**：
  - 安全（2026-10-27）：**有条件投产**，4 项 High 已在 v10.13.0 闭环；遗留：Rust 工具链验证未补跑、凭据轮换与 git 历史清理需运维。
  - 全栈（2026-10-25）：SIMD/零拷贝名实不符、绑定层完成度不齐（JS 桩/Java 仅 Pool/Python Transaction 缺失）、无 cbindgen 头文件、unsafe SAFETY 注释矛盾。
  - 产品（2026-10-25）：文档数字矛盾、crates.io 滞后、示例脱节（已整改 M1~M6）。
  - UXR（2026-10-27）：三层文档示例编译不过、5 份中文文档乱码文件名、版本号动物园、quickstart 端口不一致、CLI 桩命令、`make:model` 产物编译不过。
  - TW/SDET（2026-10-27 提示词已发）：文档漂移（README 写 v10.12.0 vs Cargo.toml v10.13.0）、门禁脚本自测、fuzz CI、覆盖率口径等待评测。
- **实时漂移**：README 仍声称 v10.12.0（README.md:4/11/448），与 Cargo.toml v10.13.0 不一致——G18 门禁当前未拦截到，请评估其影响。

## 二、必读文档（先读再评测）

1. `AGENTS.md`、`docs/sz-orm-engineering-practices.md` — 门禁体系与工程约束（注意 practices 文档仍写"7 道门禁"的漂移）
2. `docs/sz-orm-maturity-roadmap.md`（v4.9.0 包成熟化路线图，已过时，评估其价值与处置）与 `docs/pool-architecture.md`、`docs/sz-orm-架构设计.md`
3. `packages/sz-orm-core/src/prod_ready_check.rs` — 生产就绪检查器（15 项检查是否真实、是否被生产调用）
4. `CHANGELOG.md`（重点读 v10.13.0 整改条目与 v10.6.0 修正声明）、`README.md`/`README.zh.md`
5. 历次评测报告（**先读，逐条复核，不盲目采信**）：
   - `docs/assessment/2026-10-27-security-review.md` 与 `2026-10-27-security-review-fixes.md`
   - `docs/assessment/2026-10-27-uxr-review.md`
   - `docs/assessment/2026-10-25-fullstack-review.md`、`2026-10-25-product-evaluation.md`、`2026-10-25-product-evaluation-tech-fixes.md`
   - `docs/assessment/2026-10-24-deep-evaluation.md`、`2026-10-20-comprehensive-audit-report.md`、`2026-10-16-architecture-review.md`
6. 绑定层与工具链现状：`packages/sz-orm-cabi/`、五个绑定包、`cli/`、`sz-orm-designer`、`sz-orm-studio`、`fuzz/`、`loom-check/`
7. 发布与供应链：`.github/workflows/publish.yml`、`deny.toml`、crates.io 实际状态（可查询）

> 历史报告中的 `file:///E:/vue/...` 是旧路径；当前仓库在 `D:\www\test\鲜视达\rust\sz-orm`。

## 三、评测维度（TA 视角）

1. **存量问题聚合与闭环核验**：将历次评测的所有发现汇总成一张清单，逐条用当前源码/文档核验状态（已修复/部分/仍存在/已过时），估算每条问题的**生产事故成本**（若带病上线会怎样）。
2. **"解决问题 vs 新增功能"的优先级决策**：给出决策框架（如：事故成本 × 发生概率 vs 功能收益 × 用户价值），明确回答下一步是否应该**冻结新功能、集中还债**；哪些新增功能（如 C# 绑定、AI 增强）应该暂缓。
3. **生产就绪度差距分析**：对照 `ProdReadyChecker` 的 15 项检查与真实生产要求（可观测性、容量、灾备、供应链、文档可信、绑定层真实可用性、crates.io 发布），列出差距。**特别评估：`ProdReadyChecker` 自身是否有生产调用点，还是又一个"模块存在但零调用"**。
4. **分阶段路线图设计**：
   - **P0 阻断项**：不修复就不能作为生产依赖引入的问题（如：示例代码编译不过、版本号动物园、crates.io 发布策略、凭据轮换、Rust 工具链验证补跑）；
   - **P1 高优**：显著提升生产可信度（如：绑定层真实性决策——补齐 or 降级宣称、SIMD/零拷贝名实修正、文档门禁补代码可编译性检查）；
   - **P2 加固**：架构债治理（模块膨胀、feature gate 收敛、幻影交付清理、门禁脚本自测）；
   - **P3 扩展**：达成"外部生产可用"后的增量方向（C# 绑定、社区、性能持续优化）。
   每阶段给出：目标、工作项（可执行）、验收标准（可测量）、预估人日、前置依赖。
5. **架构债务治理专项**：72 包是否过多（对比成熟度路线图的包分类）、feature gate 治理成本、绑定层"半成品"是补齐还是如实降级（给出你的架构决策）、`sz-orm-designer`/`sz-orm-studio`/`sz-orm-multimodal` 等边缘包的处置建议（保留/冻结/移除）。
6. **发布与供应链策略**：crates.io 全量发布 vs 继续 path 依赖的取舍；CHANGELOG 可信度修复；deny.toml ignore 项的清零计划；git 历史中的凭据清理方案。
7. **生产就绪度评分卡**：设计一套可复测的量化评分卡（如：代码安全/测试有效性/文档可信/绑定层真实/供应链/运维可观测 六大项，每项 0~5 分 + 权重），给出当前得分与目标得分，作为后续版本迭代的度量基线。

## 四、评测方法论（硬性要求）

- **证据铁律**：每条结论必须附真实存在的 `file:line`（或命令输出）。报告完成后运行 `bash scripts/audit-verify.sh docs/assessment/<报告>.md`（Windows 用 `.\scripts\audit-verify.ps1`）自证，并附输出。
- **三种数据分层**（沿用既有评测纪律）：
  - L1 `[事实]`：文档/代码 `file:line`、真实命令输出；
  - L2 `[实测]`：你本人在本会话中真实运行的命令（如 `python scripts/check-metrics-real.py`、`cargo test -p sz-orm-core --test <某测试>`、`sz-orm info` 若 CLI 可编译）；
  - L3 `[推断]`：架构判断（如"此问题带病运行的风险等级"），必须标注。
- **L2 实测清单**（至少完成，环境不可行的如实记录）：
  1. `python scripts/check-metrics-real.py`（验证能否拦截 README v10.12.0 vs Cargo.toml v10.13.0 漂移）
  2. `python scripts/check-phantom-delivery.py`（幻影交付扫描，确认 `ProdReadyChecker` 是否有调用点）
  3. `python scripts/check-doc-sync.py --diff HEAD` 或 `check-doc-consistency.py`（文档门禁状态）
  4. 若 Rust 工具链可用：`cargo test -p sz-orm-core --test integration_redis -- --ignored`（Redis 就绪）、`cargo test -p sz-orm-core --test security_attacks`（安全攻击向量）
  5. 用 PowerShell 验证 UXR 报告提到的乱码文件名当前是否已修复（`Test-Path` 检查 README 链接目标）
- **Audit the audit**：对历次评测的结论逐条复核——特别是"已修复"的声明（如安全整改的 H-1~H-4），用当前代码验证是否真实闭环；对矛盾口径（测试数、feature gate 数、覆盖率）给出你的裁定。
- **成本思维**：每条路线图工作项必须标注「不做的生产事故成本」与「预估人日」，用 ROI 排序。
- **只读评测**：不要修改任何源码/文档/脚本；路线图只输出到报告，不改代码。

## 五、存量问题待复核清单（聚合历次评测，逐条验证状态）

| # | 问题（来源） | 历史结论 | 当前待核验点 |
|---|-------------|---------|-------------|
| 1 | 三层文档示例代码编译不过（UXR） | README §3/使用指南/迁移指南代码照抄必失败 | 是否已修复？逐字编译验证。 |
| 2 | 五份中文文档乱码文件名（UXR） | GitHub 死链 | 文件名是否已修复？链接是否可达？ |
| 3 | 版本号动物园（UXR + 本次） | workspace/crates.io/指南/CLI 多处不一致 | **Cargo.toml 10.13.0 vs README 10.12.0**；crates.io 实际最高版本？CLI `info` 输出？ |
| 4 | 安全整改 H-1~H-4（安全评测） | v10.13.0 已闭环（fixes 报告） | 用当前代码验证 rustls 链、LLM 规则、凭据扫描、脱敏是否真实落地；Rust 工具链验证是否已补跑。 |
| 5 | 凭据轮换与 git 历史清理（安全评测遗留） | 需运维动作 | 当前仓库是否仍有真实凭据残留（跑 `check-secrets.py`）？git 历史是否已处理？ |
| 6 | SIMD/零拷贝名实不符（全栈评测） | 文档声称夸大 | 文档/实现是否已修正？ |
| 7 | 绑定层完成度不齐（全栈评测） | JS 桩、Java 仅 Pool、Python Transaction 缺失 | 当前是否补齐？README 是否仍标 Completed？ |
| 8 | 无 cbindgen 头文件（全栈评测） | ABI 契约无单一声明源 | 是否已补？ |
| 9 | CLI 桩命令与 `make:model` 产物编译不过（UXR） | 约 20 命令是桩；生成的模型是空骨架 | 当前是否修复？`make:model --pk-type i64` 产物能否编译？ |
| 10 | quickstart 端口/路径不一致（UXR） | 文档 3306 vs compose 8802；无 `cargo new` | 是否已修正？ |
| 11 | crates.io 发布滞后（产品评测） | 仅 1.0.0/最高 10.7.0 | 当前 crates.io 实际版本？发布流水线 `publish.yml` 是否已打通？ |
| 12 | `ProdReadyChecker` 是否有生产调用点（本次待查） | 模块存在 | `check-phantom-delivery.py` 扫描结果；`prod-ready` feature 是否被任何生产代码启用？ |
| 13 | 门禁脚本自身缺陷（深度评测） | Gate 6 `\|\|`、Gate 11 硬编码 PASS | 是否修复？脚本是否有自测（如 `tests/test_check_doc_sync.py`）？ |
| 14 | 文档漂移（TW 提示词发现） | practices 写 7 道门禁、docs 索引 v3.4.0、Security.md v3.4.0、missing_docs 警告 | 是否已同步？ |
| 15 | 测试体系待评测项（SDET 提示词） | fuzz 未接 CI、覆盖率口径、变异盲区、loom-check 无进展 | 是否有进展？（若 SDET 报告已出，采信并纳入路线图） |

## 六、输出要求

- 输出 Markdown 架构规划报告，中文，保存为 `docs/assessment/2026-10-28-architecture-roadmap-review.md`。
- 报告结构（固定）：
  1. **决策摘要**（一页纸：三个问题的直接答案 + 路线图总览表）
  2. **存量问题聚合与闭环核验表**（问题 × 来源 × 当前状态 × 生产事故成本 × 处置建议）
  3. **"解决问题 vs 新增功能"决策**（决策框架 + 冻结/暂缓的功能清单）
  4. **生产就绪度差距分析**（含 `ProdReadyChecker` 调用点核验）
  5. **分阶段路线图**（P0/P1/P2/P3，每阶段含目标、工作项、验收标准、预估人日、依赖）
  6. **架构债务治理专项**（包收缩/feature 收敛/绑定层决策/边缘包处置）
  7. **发布与供应链策略**（crates.io、CHANGELOG 可信度、deny 清零、凭据清理）
  8. **生产就绪度评分卡**（六维度量化打分：当前 vs 目标）
  9. **历史发现复核结果表**（每项：历史结论 → 当前证据 → 已修复/部分/仍存在/已过时）
  10. **证据索引**（所有 `file:line` 汇总）与 `audit-verify` 脚本输出
- **证据纪律**：L1 结论必须附真实存在的 `file:line`；L2 实测必须附真实命令输出；L3 推断必须标注。
- **只读评测**：不要修改任何源码/文档/脚本；路线图只输出到报告。

## 七、期望

以架构师的身份做一次"从代码到业务"的决策推演：**如果明天就要对外宣称"sz-orm 生产就绪"，最该做哪三件事？** 你的路线图要经得起两个挑战：①每一笔人日投入都能对应到可衡量的生产风险下降；②阶段划分让团队可以"每两周交付一个可验收的里程碑"，而不是一份永远做不完的愿望清单。

---

**评测完成后**：将报告保存为 `docs/assessment/2026-10-28-architecture-roadmap-review.md`，并运行 `.\scripts\audit-verify.ps1 docs\assessment\2026-10-28-architecture-roadmap-review.md` 自证证据，把输出附在报告末尾。