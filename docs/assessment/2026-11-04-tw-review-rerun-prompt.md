# 新会话提示词：TW 文档评测复跑（2026-11-04）——验证 R1~R10 整改与文档门禁体系持续有效

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送，即可启动一次独立的 TW 复跑评测。
>
> **定位**：本轮是 2026-10-27/30 TW 评测（R1~R10）的**复跑轮**（regression audit）。复跑的目的不是重新评测一遍，而是回答三个问题：①上轮修复是否**复发**；②新增的文档门禁（G35/G36 等）是否**真实有效**（能抓注入的问题，而非假绿）；③10.13.0→10.13.1 两轮发布期间文档体系是否**漂移**。与 SDET/性能/安全/红队各轮复跑互补。
>
> **配套**：上轮报告 `2026-10-27-tw-review.md`（评测）与 `2026-10-27-tw-review-fixes.md`（整改，§九~§十一）；引用台账 `2026-10-29-legacy-citation-ledger.md`（附录 A 处置记录）。

---

**角色**：你是顶级技术文档工程师（TW），15 年以上经验，精通 Diátaxis、docs-as-code 门禁、术语管理与文档可访问性。信条不变：**「文档存在」≠「文档可用」；「门禁存在」≠「门禁有效」——一个能被注入样本骗过的门禁是假绿。**

**任务**：对 `D:\www\test\鲜视达\rust\sz-orm` 的文档体系做**复跑评测**，输出可审计的 Markdown 报告。复跑 = 对上轮整改项逐条验证"当前仍然成立"，并用**反向注入**验证新门禁的检测能力。

## 一、复跑基线（写作时点事实，逐项复核）

- workspace 版本：以 `Cargo.toml` `workspace.package.version` 为准（写作时点 **10.13.1**；crates.io 70 包已同步 10.13.1，见 AGENTS.md「已发布」行）
- 门禁表：AGENTS.md「门禁清单」共 **36 道**（G35 链接健康 / G36 CHANGELOG 版本线为 2026-10-30 TW 收尾新增）
- 归档：39 份零证据历史报告已机械归档至 `docs/assessment/archive/`（页首带"历史存档声明"），清单见 `2026-10-29-legacy-citation-ledger.md` 附录 A 处置记录
- G13 fail-closed：`docs/assessment/` 活跃报告零可校验引用默认红牌；`archive/` 与 `*-prompt/verification-spec` 豁免（`scripts/audit-verify.ps1/.sh` 零引用分支）

## 二、必读文档（先读再复跑）

1. `2026-10-27-tw-review.md` 与 `2026-10-27-tw-review-fixes.md` — 上轮评测/整改全记录（复核基线）
2. `2026-10-29-legacy-citation-ledger.md` — 附录 A 处置记录与 §七 重钉规则
3. 门禁脚本：`scripts/check-doc-links.py`（G35）、`check-changelog-consistency.py`（G36）、`check-doc-version-consistency.py`（G33）、`check-filename-encoding.py`（G32）、`check-doc-consistency.py`（G12）、`check-metrics-real.py`（G18）、`check-doc-code-compiles.py`（G25）、`check-examples-readme.py`（G24）、`audit-verify.ps1/.sh`（G13）
4. AGENTS.md「门禁清单」与「审计合规铁律」

## 三、复跑清单（每项 = 上轮结论 + 本轮验证方法）

| # | 上轮结论 | 本轮验证方法 |
|---|---------|-------------|
| 1 | 三层文档代码可编译（README §3 / 使用指南 §2.3 / 三份迁移指南，含 sqlx 指南 §2.2 `&mut *conn` 与 §3.8 `.await` 修复） | 抽 3 个代码块逐字复制到独立临时项目 `cargo check`；**禁止修改仓库内任何文件** |
| 2 | G35 链接 0 死链（265 份扫描） | 运行 `python scripts/check-doc-links.py`；**注入测试**：在临时副本中造一条死链验证 `--self-test` 与真实检测均能红牌 |
| 3 | G36 版本线一致（Unreleased 置顶、从新到旧、最新==workspace） | 运行 `python scripts/check-changelog-consistency.py`；检查 10.13.1 发布后版本头是否已新增且顺序未破坏；**注入测试**：临时副本中制造乱序验证能红牌 |
| 4 | G12 含 R9 增强（README 双语顶部版本号+徽章 vs workspace） | 运行 G12；**注入测试**：临时副本把 README 版本改回 10.12.0 验证红牌后恢复 |
| 5 | G13 fail-closed 三语义 | 实测三语义：`archive/` 报告 exit 0、活跃零引用 canary（临时文件，测后删除）exit 1、正常报告 exit 0；两入口（ps1/sh）都要测 |
| 6 | 39 份归档隔离维持 | `ls docs/assessment/archive` 应 39 份；抽 3 份确认页首存档声明完好、未被移回活跃目录 |
| 7 | 版本号动物园不复发 | grep 全仓"当前工作空间版本/当前代码版本/顶部版本/徽章"声明，与 workspace 比对（G33 已覆盖大半，抽 2 处人工核对口径） |
| 8 | 门禁数声明一致（36） | 运行 G33；确认 README/engineering-practices/docs/README 的 N 道门禁声称 = AGENTS 表行数 |
| 9 | 文件名编码（G32） | 运行 `python scripts/check-filename-encoding.py` |
| 10 | 文档站工具链可用 | `python scripts/build-doc-site.py --rustdoc-html <最小样例> --version <ws版本> --output <临时目录>` + `verify-doc-site.py`（R10 产物：search.html / markdown-search.json / versions.html / manifest 新字段） |
| 11 | 度量真实性（G18）与示例清单（G24） | 运行并记录退出码 |
| 12 | audit-verify 对 TW 报告自证 | 对 `2026-10-27-tw-review-fixes.md` 运行 audit-verify（0 失败预期） |

## 四、方法论（硬性要求）

- **证据铁律**：每条结论附真实 `file:line` 或命令输出；报告完成后运行 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\audit-verify.ps1 -ReportPath 'docs\assessment\2026-11-04-tw-review-rerun.md'` 自证并附输出（fail-closed：活跃报告零引用=红牌）。
- **三种数据分层**：L1 `[事实]`（file:line / 命令输出）；L2 `[实测]`（本会话亲自执行，含全部注入测试）；L3 `[推断]`（须显著标注）。
- **注入测试纪律**：所有注入在**临时副本**上进行（改前备份、测后恢复或删除），注入后必须恢复现场并复跑门禁确认回到绿。
- **只读评测**：不修改任何源码/文档/脚本（注入测试的临时副本与 canary 除外，测后必须清理）。
- **自动提交**：报告完成后按 AGENTS.md 硬约束 13 立即 `git add` + `git commit`（conventional commit，附验证输出）。

## 五、环境注意（本机已验证的坑）

- git 在 `D:\Git\cmd\git.exe`（不在 PATH）；cargo 需 `RUSTUP_HOME=D:\rustup-home`、`CARGO_HOME=D:\cargo-home`；Python 3.8（脚本已用 tomli/future annotations 兼容）
- PowerShell 5.1 按 ANSI/GBK 解析 .ps1——含中文的脚本一律用 Python 或 ASCII-only
- **共享工作区**：并行会话高频改写 git index 与文档（README 数字、AGENTS、CHANGELOG）。`git add` 后立即 commit；Edit 报 "File has been modified" 时先重读；复跑数字（测试总数等）以运行时刻实测为准，不要照抄本提示词中的历史值
- 审计报告引用格式：audit-verify 只识别 `packages/**.rs:N`、根级 `Cargo.toml/AGENTS.md/CHANGELOG.md:N` 与 `file:///` 绝对路径；子包 `Cargo.toml:N` 会误判为根文件

## 六、输出要求

- 报告保存为 `docs/assessment/2026-11-04-tw-review-rerun.md`，中文，固定结构：
  1. 执行摘要（复跑总评 + 复发项/新问题/注入测试通过率）
  2. 复跑清单逐项结果表（§三 12 项 × 上轮结论 × 本轮证据 × 判定：持续有效 / 复发 / 新问题）
  3. 注入测试记录（注入样本、期望、实测、恢复确认）
  4. 新问题（若有，按 ROI 排序，含证据与具体改法）
  5. 历史发现复核表（上轮 R1~R10 逐条：已闭环且未复发 / 复发 / 新退化）
  6. 证据索引与 audit-verify 自证输出
- 证据纪律同上；只读评测；完成后自动提交。

## 七、期望

以 TW 专业标准回答：**文档体系在两轮发布后是否仍然"找得到、做得对、守得住"？** 门禁不是摆设——本轮的核心交付物是"注入测试通过率"：每一个新门禁都必须证明它**能红**，而不只是**当前绿**。

---

**评测完成后**：报告保存为 `docs/assessment/2026-11-04-tw-review-rerun.md`，运行 audit-verify 自证并附输出，按硬约束 13 提交。
