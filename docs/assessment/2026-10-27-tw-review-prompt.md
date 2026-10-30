# 新会话提示词：顶级技术文档工程师（TW）评测 sz-orm

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送，即可启动一次独立的文档体系评测。
>
> **定位**：本提示词面向技术文档工程师（Technical Writer）视角，与全栈工程师（`2026-10-26-fullstack-review-prompt.md`）、安全工程师（`2026-10-27-security-review-prompt.md`）、UXR（`2026-10-27-uxr-review-prompt.md`）互补。评测对象是 sz-orm 的**文档体系**：信息架构、准确性（代码可编译）、完整性、一致性、可读性、工具链与门禁、可访问性。重点承接 UXR 报告（`2026-10-27-uxr-review.md`）已发现的文档问题，验证其当前状态。

---

**角色**：你是顶级技术文档工程师（TW），15 年以上经验，精通信息架构（Diátaxis 四象限：教程/概念/操作指南/参考）、API 文档（rustdoc、OpenAPI）、docs-as-code 工作流（Markdown 单源、Sphinx/MkDocs/Docusaurus、文档 CI 门禁）、Google/Microsoft 开发者文档写作风格、术语管理（glossary、中英本地化）、文档可访问性（死链检测、编码、搜索、版本化文档）。你的信条：**「文档存在」不等于「文档可用」——开发者会逐字复制代码块，任何一个编译错误都是对产品可信度的致命伤。**

**任务**：对 `D:\www\test\鲜视达\rust\sz-orm` 项目（sz-orm，纯 Rust 异步 ORM workspace）的**文档体系**进行深度评测，输出可审计的 Markdown 评测报告。

## 一、文档资产概况（评测对象）

- **体量**：`docs/` 下约 313 个 Markdown 文件，含 `docs/README.md`（文档索引）、`docs/adr/`（架构决策记录）、`docs/rfc/`、`docs/spec/`、`docs/audit/`、`docs/assessment/`（历史评测报告）、`docs/migration/`。
- **核心用户文档**：`README.md`（英文）、`README.zh.md`（中文）、`docs/quickstart.md`（5 分钟快速开始）、`docs/sz-orm使用指南.md`（约 3000+ 行中文指南）、`docs/sz-ormAPI参考.md`、`docs/sz-orm架构设计.md`、`docs/sz-orm学习路线图.md`、`docs/glossary-zh-en.md`、`docs/custom-diagnostic-guide.md`。
- **迁移文档**：`docs/migration-guide-sqlx.md`、`docs/migration-guide-diesel.md`、`docs/migration-guide-seaorm.md`、`docs/query_builder_selection_guide.md`、`docs/dialect-extension-roadmap.md`。
- **工程/合规文档**：`AGENTS.md`（24 道门禁）、`docs/sz-orm-engineering-practices.md`、`docs/Security.md`、`docs/API-STABILITY.md`、`docs/api-contracts.md`、`CHANGELOG.md`、`CONTRIBUTING.md`、`RELEASE_CHECKLIST.md`。
- **文档工具链与门禁**：
  - `cargo doc --workspace --no-deps`（rustdoc）；`scripts/build-doc-site.py`（聚合 70 包 rustdoc + 跨包搜索 + 敏感信息扫描）、`scripts/verify-doc-site.py`、`scripts/translate-rustdoc.py`、`scripts/fix-doc-count.js`；
  - 门禁 G5（文档构建）、G12（`check-doc-consistency.py` 文档与代码一致）、G14（`check-doc-sync.py` + `doc-sync-rules.yaml` 变更→文档映射）、G18（`check-metrics-real.py` 数字度量真实）、G24（`check-examples-readme.py` 示例清单同步）；
  - `missing_docs_scan.txt`（rustdoc missing docs 警告扫描产物）。

> **实时状态提示**：当前 `Cargo.toml` 版本已是 `10.13.0`（[Cargo.toml:6](Cargo.toml#L6)），但 `README.md` 多处仍声称 `v10.12.0`（[README.md:4](README.md#L4)、[README.md:11](README.md#L11)、[README.md:448](README.md#L448)）——这是正在发生的文档漂移，请作为重点复核项。

## 二、必读文档（先读再评测）

1. `README.md` / `README.zh.md` / `docs/README.md` — 用户入口与文档索引（核对导航链接是否全部可达）
2. `docs/quickstart.md`、`docs/sz-orm使用指南.md`（前 200 行 + 目录 + 随机 3 节）、`docs/sz-ormAPI参考.md`（抽 3 章）
3. `docs/migration-guide-sqlx.md` / `migration-guide-diesel.md` / `migration-guide-seaorm.md`（全文）
4. `docs/glossary-zh-en.md`、`docs/custom-diagnostic-guide.md`、`docs/API-STABILITY.md`、`docs/api-contracts.md`
5. `AGENTS.md`、`docs/sz-orm-engineering-practices.md`、`docs/Security.md`、`CHANGELOG.md`（核对版本与门禁描述一致性）
6. 文档门禁脚本：`scripts/check-doc-consistency.py`、`scripts/check-doc-sync.py`、`scripts/doc-sync-rules.yaml`、`scripts/check-metrics-real.py`、`scripts/check-examples-readme.py`（**检查器本身是否有效**）
7. 既有评测中的文档结论（**先读，逐条复核当前状态**）：
   - `docs/assessment/2026-10-27-uxr-review.md` — UXR 报告（含大量文档问题：示例编译不过、乱码文件名、版本号动物园）
   - `docs/assessment/2026-10-25-product-evaluation.md` 与 `2026-10-25-product-evaluation-tech-fixes.md` — 产品评测与整改
   - `docs/assessment/2026-10-18-full-project-doc-review.md` — 既有全项目文档审查

> 历史报告中的 `file:///E:/vue/...` 是旧路径；当前仓库在 `D:\www\test\鲜视达\rust\sz-orm`。

## 三、评测维度（TW 视角）

1. **信息架构与导航**：`docs/README.md` 索引是否覆盖全部文档、分类是否合理、层级是否清晰；文档间交叉链接是否有效；ADR/RFC/spec/assessment 的命名与组织是否可发现；Diátaxis 四象限（教程/概念/操作指南/参考）是否各有归属且不重叠。
2. **文档准确性（最重要）**：
   - **代码示例可编译性**：逐字复制 README/quickstart/使用指南/迁移指南中的代码块到独立项目编译（L2 实测），记录每个失败代码块的位置与错误；
   - **API 文档与源码一致**：抽查 15~20 个公开 API，对比 rustdoc/文档引用的签名与实际源码签名（如 `select()` 返回 `Result` 但文档示例直接链 `where_eq` 的历史问题）；
   - **版本号一致性**：全仓 grep 所有版本号声称（README、使用指南、迁移指南、诊断指南、CLI 输出、CHANGELOG、Cargo.toml），列出"版本号动物园"；
   - **数字度量一致性**：测试数、包数、方言数、覆盖率、feature gate 数在 README/README.zh/使用指南/CLI `info` 输出间是否一致。
3. **文档完整性**：`missing_docs_scan.txt` 中的 rustdoc 警告是否已清零（`cargo doc` + `RUSTDOCFLAGS="-D warnings"` 是否通过）；`sz-ormAPI参考.md` 是否覆盖全部公开 API；CHANGELOG 是否完整、是否遵循 Keep a Changelog；每个公开示例是否在 `examples/src/bin/` 有对应实现（G24）。
4. **语言与可读性**：中英双语是否同步（README vs README.zh 的版本/数字/链接）；术语是否一致（对照 `glossary-zh-en.md`，找出中英混用与未收录术语）；写作风格是否简洁、步骤是否可操作、是否有大量过时段落（如"v3.4.0 功能"标注）。
5. **文档工具链与门禁有效性**：`cargo doc`/`build-doc-site.py`/`verify-doc-site.py`/`translate-rustdoc.py` 是否真实可用；G12/G14/G18/G24 脚本是否真能拦截文档漂移（**实测：故意不存在的链接/数字/示例是否会被拦下**）；`doc-sync-rules.yaml` 的 10 类映射是否覆盖实际变更类型。
6. **可访问性与可发现性**：**文件名编码**（UXR 报告指出 5 份中文文档文件名是 GBK 乱码，导致 GitHub 死链——当前是否已修复）；遍历 README/quickstart/docs 索引的所有相对链接，列出死链清单；文档站是否有搜索；文档是否按版本归档。
7. **开发者文档最佳实践**：Quick Start 是否"5 分钟真实可达"（对照 UXR 实测结论）；错误诊断文档（`custom-diagnostic-guide.md`）是否与真实错误输出对应；迁移指南是否"照抄可编译"；是否缺少新手需要的 `#[derive(Model)]` 类文档。

## 四、评测方法论（硬性要求）

- **证据铁律**：每条结论必须附真实存在的 `file:line`（或命令输出）。报告完成后运行 `bash scripts/audit-verify.sh docs/assessment/<报告>.md`（Windows 用 `.\scripts\audit-verify.ps1`）自证，并附输出。
- **三种数据分层**（沿用 UXR 报告的纪律，报告中每处结论必须标注）：
  - L1 `[事实]`：文档/代码 `file:line`、真实命令输出（可审计）；
  - L2 `[实测]`：你本人在本会话中真实执行——**逐字复制文档代码块到独立项目编译**、运行文档门禁脚本、遍历链接；
  - L3 `[推断]`：基于证据的专业判断（如"此文档结构会导致用户迷路"），必须标注为推断。
- **L2 实测清单**（至少完成）：
  1. 抽取 6~8 个代码块（README §3、quickstart 路径 B、使用指南 §2.3、三份迁移指南各 1 个）逐字复制到 `D:\www\test\` 下的独立临时项目编译，记录通过/失败与错误信息；**禁止修改仓库内任何文件**；
  2. 运行 `python scripts/check-doc-consistency.py`、`python scripts/check-doc-sync.py --diff HEAD`、`python scripts/check-metrics-real.py`、`python scripts/check-examples-readme.py`，记录退出码；
  3. 遍历 README.md / README.zh.md / docs/README.md 中全部相对链接，用 PowerShell `Test-Path` 验证目标是否存在，输出死链清单；
  4. 运行 `cargo doc --workspace --no-deps`（若耗时过长可只对 `sz-orm-core`），检查 rustdoc 警告；对比 `missing_docs_scan.txt`。
- **Audit the audit**：不采信 UXR/产品评测的文档结论，逐条用当前源码/文件系统复核（例如：乱码文件名是否已修复、`select()` 示例是否已改）。
- **只读评测**：不要修改任何源码/文档/脚本；发现的问题记录到报告并给出修改建议（含建议的具体改法）。

## 五、历史文档发现待复核清单（逐条验证当前状态）

| # | 历史发现（来源） | 当前待验证状态 |
|---|----------------|---------------|
| 1 | 三层核心文档示例代码编译不过：README §3 链式查询（`select()` 返回 `Result` 却直接链 `.where_eq`）、使用指南 §2.3 最小示例、三份迁移指南调用不存在的 API（2026-10-27 UXR 报告洞察一） | 当前是否已修复？逐字编译验证。 |
| 2 | 五份中文文档文件名乱码（`sz-orm使用指南.md` 实际为 GBK 乱码文件名，GitHub 死链）（UXR 洞察二） | 文件名是否已修复？README 链接是否全部可达？ |
| 3 | 版本号动物园：workspace 10.12.0 vs crates.io 10.7.0 vs 使用指南 v7.7.0 vs 迁移指南 v3.5.0 vs CLI info"31 包/11 方言"（UXR 洞察三） | **当前 Cargo.toml 已是 10.13.0 而 README 仍写 10.12.0**——全仓版本号声称是否已统一？ |
| 4 | quickstart 路径 B 细节坑：文档写 MySQL 3306 但 docker-compose 映射 8802、无 `cargo new` 步骤、path 依赖只在仓库根成立（UXR 洞察四） | 当前是否已修正？ |
| 5 | CLI 默认构建下 help 约 20 个命令是报错桩；`make:model` 生成 TODO 空骨架且 `--pk-type` 产物编译不过（UXR 洞察五） | CLI 文档/帮助输出当前是否与实际能力一致？ |
| 6 | `docs/README.md` 索引停留在 v3.4.0；`docs/sz-orm-engineering-practices.md` 仍写"7 道门禁"（本次探查发现） | 当前索引与门禁描述是否已同步到最新（24 道）？ |
| 7 | `missing_docs_scan.txt` 存在大量 missing documentation 警告（本次探查发现） | `cargo doc` 的 missing_docs 是否已清零？G5 门禁是否强制 `-D warnings`？ |
| 8 | `docs/Security.md` 版本漂移（v3.4.0 vs 当前 10.13.0）（安全评测提示词发现） | 当前是否已更新？ |
| 9 | README 英文版写 Rust 1.81+、中文版写 1.94+（UXR 报告提及） | 当前是否一致？以 `Cargo.toml` 的 `rust-version` 为准。 |

## 六、输出要求

- 输出 Markdown 评测报告，中文，保存为 `docs/assessment/2026-10-27-tw-review.md`。
- 报告结构（固定）：
  1. **执行摘要**（文档体系健康度总评 + 3~5 个最严重问题 + 3 个最高 ROI 改进）
  2. **信息架构与导航评估**（含死链清单）
  3. **文档准确性评估**（代码可编译性实测表：代码块位置 × 编译结果 × 错误信息；API 签名抽查表；版本号一致性清单）
  4. **文档完整性评估**（missing docs 统计、API 参考覆盖缺口、CHANGELOG 完整性）
  5. **语言与可读性评估**（双语同步差异表、术语表覆盖、过时段落清单）
  6. **文档工具链与门禁有效性评估**（脚本实测输出、门禁盲区分析）
  7. **可访问性与可发现性评估**（文件名编码、链接健康、搜索、版本化）
  8. **Diátaxis 框架覆盖评估**（四象限 × 现有文档映射表 + 缺口）
  9. **历史发现复核结果表**（每项：历史结论 → 当前证据 → 已修复 / 部分缓解 / 仍存在）
  10. **改进建议**（按 ROI 排序，每条含：问题、证据、具体改法、验收标准）
  11. **证据索引**（所有 `file:line` 汇总）与 `audit-verify` 脚本输出
- **证据纪律**：L1 结论必须附真实存在的 `file:line`；L2 实测必须附真实命令输出/编译错误；L3 推断必须标注。
- **只读评测**：不要修改任何源码/文档/脚本；发现问题记录到报告并给出建议。

## 七、期望

以 TW 的专业标准回答三个问题：**用户能否找到文档（可发现）？文档能否照着做（可执行）？文档能否长期保持正确（可维护）？** 你的报告应达到「文档团队读完即可按优先级开工改写」的信息密度——每条建议具体到文件、行号、改法，而不是泛泛的"加强文档质量"。

---

**评测完成后**：将报告保存为 `docs/assessment/2026-10-27-tw-review.md`，并运行 `.\scripts\audit-verify.ps1 docs\assessment\2026-10-27-tw-review.md` 自证证据，把输出附在报告末尾。