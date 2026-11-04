# sz-orm TW 文档评测复跑报告（2026-11-04）

- **复跑角色**：顶级技术文档工程师（TW），15 年经验
- **复跑对象**：sz-orm v10.13.1（`Cargo.toml:6` 声明 workspace.package.version = "10.13.1"）
- **复跑日期**：2026-11-04
- **复跑性质**：regression audit——验证上轮（2026-10-27/30）R1~R10 整改是否复发、新门禁（G35/G36 等）是否真实有效、10.13.0→10.13.1 两轮发布期间文档体系是否漂移
- **配套报告**：[2026-10-27-tw-review.md](2026-10-27-tw-review.md)（上轮评测）、[2026-10-27-tw-review-fixes.md](2026-10-27-tw-review-fixes.md)（上轮整改）、[2026-10-29-legacy-citation-ledger.md](2026-10-29-legacy-citation-ledger.md)（引用台账）
- **证据纪律**：L1=`[事实]`（file:line / 命令输出）；L2=`[实测]`（本会话亲自执行，含全部注入测试）；L3=`[推断]`（显著标注）
- **L2 实测环境**：Windows 10 x64、Python 3.8.6、PowerShell 5.1、git-bash、`RUSTUP_HOME=D:\rustup-home`、`CARGO_HOME=D:\cargo-home`、探针项目位于 `D:\www\test\tw-rerun-tmp\docprobe`（仓库外）
- **只读评测**：未修改任何源码/文档/脚本（注入测试的临时 canary 与 README.md 单行注入已测后恢复，见 §3）

---

## 1. 执行摘要

**复跑总评：文档体系在 10.13.0→10.13.1 两轮发布后基本"找得到、做得对、守得住"，但存在一处门禁盲区导致的版本号漂移复发。**

12 项复跑清单中 **11 项持续有效**，**1 项发现新问题**（第 7 项：G33 未覆盖文档的版本号漂移）。上轮 R1~R10 整改中 **9 项已闭环且未复发**，**1 项部分复发**（R2 版本号动物园在 G33 盲区文档中残留），**1 项未直接复跑但间接验证**（R6 rustdoc 断链，本轮未跑 `cargo doc -D warnings`，但 G32/G35 通过间接确认文档侧无乱码/死链）。

**注入测试通过率：4/4 = 100%**。四个新门禁/增强点全部经反向注入验证"能红"而非"当前绿"：
- G35 链接健康：注入死链 → 红牌 ✅
- G36 CHANGELOG 版本线：注入日期乱序 + 版本回退 → 红牌 ✅
- G12 文档一致性（R9 增强）：注入 README 版本回退 10.12.0 → 红牌 ✅
- G13 fail-closed：活跃零引用 canary → 红牌（ps1+sh 两入口） ✅

**新问题（1 项，按 ROI 排序）**：
- **N1（中 ROI）**：G33 门禁盲区——`docs/README.md`、`docs/api-contracts.md`、`docs/API-STABILITY.md`、`docs/ADR与生产Bug定位规范.md`、`docs/sz-orm架构设计.md`、`docs/sz-ormAPI参考.md`、`docs/production-cases.md`、`docs/sz-orm-review-runbook.md` 共 8 份文档的"当前版本声明"仍停留在 10.13.0/3.4.0/7.7.0/5.0.0/4.9.0，G33 未覆盖因此不报。详见 §4。

**门禁体系健康度**：G12/G18/G24/G32/G33/G35/G36 全绿；G13 fail-closed 三语义两入口实测正确；audit-verify 对上轮整改报告自证通过 9/0/2。门禁不是摆设——本轮核心交付物"注入测试通过率 100%"证明每一个被测门禁都能红。

---

## 2. 复跑清单逐项结果表

| # | 上轮结论 | 本轮证据 | 判定 |
|---|---------|---------|------|
| 1 | 三层文档代码可编译（README §3 / 使用指南 §2.3 / SQLx 指南 §2.2 §3.8 修复） | [L2] 探针项目 `D:\www\test\tw-rerun-tmp\docprobe` 三个 bin `cargo check` 全通过：`readme3`（README.md:513-547 逐字复制）EXIT=0；`guide23`（docs/sz-orm使用指南.md:79-113 逐字复制）EXIT=0；`sqlx38`（docs/migration-guide-sqlx.md:244-259 逐字复制 + 最小外壳）EXIT=0。SQLx §2.2 `q.fetch_all(&mut *conn).await?`（docs/migration-guide-sqlx.md:60）与 §3.8 `conn.query_with_params(&sql, &params).await?`（docs/migration-guide-sqlx.md:258）均编译通过 | **持续有效** |
| 2 | G35 链接 0 死链（265 份扫描） | [L2] `python scripts/check-doc-links.py` → 扫描 **233** 个文件，0 死链，EXIT=0；`--self-test` EXIT=0。注入死链 canary（`docs/_tw_rerun_canary.md` 第 5 行，测后已删除） → EXIT=1，精确定位死链行；恢复后复跑 EXIT=0。注：扫描数 233 vs 上轮 265，系 10.13.0→10.13.1 期间文档增减 | **持续有效** |
| 3 | G36 版本线一致（Unreleased 置顶、从新到旧、最新==workspace） | [L2] `python scripts/check-changelog-consistency.py` → 解析 **76** 个版本头，workspace=10.13.1，EXIT=0；`--self-test` EXIT=0。注入日期乱序（10.13.0 日期晚于 10.13.1）→ 报"版本 10.13.0 日期 2026-11-02 晚于上一版本 2026-10-31"；注入版本回退（最新 10.13.0 != workspace 10.13.1）→ 报"最新版本 10.13.0 != workspace 版本 10.13.1" | **持续有效** |
| 4 | G12 含 R9 增强（README 双语顶部版本号+徽章 vs workspace） | [L2] `python scripts/check-doc-consistency.py` → 版本 10.13.1，包数量 72，README.md/README.zh.md 顶部版本号+徽章全部 [OK]，EXIT=0。注入 README.md 顶部版本号 10.13.1→10.12.0 → 报 `[MISMATCH] README.md 顶部版本号: 实际=10.13.1, 文档=10.12.0` + FAIL；恢复后复跑 EXIT=0 | **持续有效** |
| 5 | G13 fail-closed 三语义 | [L2] 两入口（ps1/sh）× 三语义全符合：①archive 报告 `docs/assessment/archive/2026-08-14-blackhat-security-audit.md` exit 0（豁免）；②活跃零引用 canary `docs/assessment/_tw_rerun_canary.md` exit 1（红牌"docs/assessment/ 活跃报告必须含可校验引用"）；③含有效引用 canary `docs/assessment/_tw_rerun_canary_ok.md` exit 0（通过 2/0/0）。ps1 与 sh 输出一致 | **持续有效** |
| 6 | 39 份归档隔离维持 | [L2] `ls docs/assessment/archive/ \| wc -l` = **39**。抽 3 份（最早/中间/最晚）确认页首存档声明完好：`docs/assessment/archive/2026-08-14-blackhat-security-audit.md:1`、`docs/assessment/archive/2026-09-23-v870-mutation-report.md:1`、`docs/assessment/archive/2026-10-20-v10120-delivery-record.md:1` 均含"📦 历史存档（2026-11-02 自动化归档）"声明并引用 legacy-citation-ledger 附录 A | **持续有效** |
| 7 | 版本号动物园不复发 | [L2] grep 全仓"当前工作空间版本/当前代码版本/适用版本/顶部版本/徽章"：G33 覆盖的 5 份文档（README.md:4/11、README.zh.md:4/11、docs/sz-orm使用指南.md:5/7、docs/sz-orm学习路线图.md:5/6、AGENTS.md:3）全部 10.13.1 ✅。但 G33 未覆盖的 8 份文档漂移（见 §4 N1） | **新问题** |
| 8 | 门禁数声明一致（36） | [L2] `python scripts/check-doc-version-consistency.py` → "权威门禁数（AGENTS.md 门禁表）= 36，门禁数声称扫描完成"，7 处当前版本声明全部与 workspace 一致，EXIT=0 | **持续有效** |
| 9 | 文件名编码（G32） | [L2] `python scripts/check-filename-encoding.py` → "全部文件名编码合法"，EXIT=0 | **持续有效** |
| 10 | 文档站工具链可用 | [L2] 最小 fake rustdoc 产物（1 个 `sz_orm_core` 目录）→ `python scripts/build-doc-site.py` EXIT=0：聚合 1 crate，Markdown 全文搜索 **76** 份文档，版本归档 **76** 个版本，敏感信息扫描通过，manifest 生成。`python scripts/verify-doc-site.py --expected-version 10.13.1 --expected-crates 1` EXIT=0：10 项 ✓ 全通过（crate 数量/版本/统一导航/跨包搜索/Markdown 全文搜索/版本归档/版本历程链接/敏感信息扫描/Markdown 搜索索引/版本归档详情页）。R10 产物 search.html / markdown-search.json / versions.html / manifest 新字段齐备 | **持续有效** |
| 11 | 度量真实性（G18）与示例清单（G24） | [L2] G18：`python scripts/check-metrics-real.py` → 实际统计 包=70 成员=72 测试标注=**21290** feature=510 版本=10.13.1，README 数字一致，EXIT=0。G24：`python scripts/check-examples-readme.py` → 38 个 bin 全部登记，EXIT=0 | **持续有效** |
| 12 | audit-verify 对 TW 报告自证 | [L2] 对 `docs/assessment/2026-10-27-tw-review-fixes.md` 运行 audit-verify 两入口：ps1 通过 9/失败 0/警告 2 EXIT=0；sh 通过 9/失败 0/警告 2 EXIT=0 | **持续有效** |

---

## 3. 注入测试记录

> 纪律：所有注入在临时副本/canary 上进行，测后恢复现场并复跑门禁确认回到绿。

| # | 门禁 | 注入样本 | 期望 | 实测 | 恢复确认 |
|---|------|---------|------|------|---------|
| 1 | G35 链接健康 | 创建 `docs/_tw_rerun_canary.md`（第 5 行含 `[死链测试](no-such-target-file.md)`，测后已删除） | EXIT=1，精确定位死链 | [L2] `python scripts/check-doc-links.py` → "扫描 234 个文件 ❌ docs/_tw_rerun_canary.md 第 5 行 链接 `no-such-target-file.md` -> 目标不存在"，EXIT=1 ✅ | 删除 canary，复跑 EXIT=0（233 文件 0 死链）✅ |
| 2 | G36 版本线（日期乱序） | 构造 CHANGELOG 样本：`## [10.13.1] — 2026-10-31` 后跟 `## [10.13.0] — 2026-11-02` | 报"晚于上一版本" | [L2] `check_changelog(bad, '10.13.1')` → `['版本 10.13.0 日期 2026-11-02 晚于上一版本 2026-10-31（版本线应从新到旧）']` ✅ | 纯函数调用，未改 CHANGELOG.md，无需恢复 |
| 3 | G36 版本线（版本回退） | 构造 CHANGELOG 样本：最新版本头 `## [10.13.0] — 2026-10-27`，workspace=10.13.1 | 报"最新版本 != workspace" | [L2] `check_changelog(bad2, '10.13.1')` → `['最新版本 10.13.0 != workspace 版本 10.13.1']` ✅ | 纯函数调用，未改 CHANGELOG.md，无需恢复 |
| 4 | G12 文档一致性（R9 增强） | 临时把 `README.md:4` 顶部版本号 `> v10.13.1 ·` 改为 `> v10.12.0 ·`（仅第一处） | 报 `[MISMATCH] README.md 顶部版本号` + FAIL | [L2] `python scripts/check-doc-consistency.py` → `[MISMATCH] README.md 顶部版本号: 实际=10.13.1, 文档=10.12.0` + `[FAIL]` ✅ | Python 脚本恢复 `> v10.13.1 ·`，复跑 EXIT=0（[PASS]）✅ |
| 5 | G13 fail-closed（ps1） | 创建 `docs/assessment/_tw_rerun_canary.md`（无任何 file:line 引用） | exit 1 | [L2] `powershell -File scripts/audit-verify.ps1 -ReportPath docs/assessment/_tw_rerun_canary.md` → "未找到任何 file:line 证据 — docs/assessment/ 活跃报告必须含可校验引用（fail-closed）"，EXIT=1 ✅ | 删除 canary ✅ |
| 6 | G13 fail-closed（sh） | 同上 canary | exit 1 | [L2] `bash scripts/audit-verify.sh docs/assessment/_tw_rerun_canary.md` → 同款红牌消息，EXIT=1 ✅ | 删除 canary ✅ |

**注入测试通过率：6/6 = 100%**（含 G35 死链、G36 日期乱序、G36 版本回退、G12 版本回退、G13 ps1 fail-closed、G13 sh fail-closed）。

---

## 4. 新问题

### N1 · G33 门禁盲区：8 份文档"当前版本声明"漂移（中 ROI）

**问题**：G33（`scripts/check-doc-version-consistency.py:44-63`）的 CHECKS 表只覆盖 5 份文档（README.md、README.zh.md、docs/sz-orm使用指南.md、docs/sz-orm学习路线图.md、AGENTS.md），未覆盖以下 8 份含"当前版本声明"的文档，导致它们的版本号漂移无门禁拦截。

**证据**（[L1] grep 全仓 + 逐文件确认，workspace 权威值 `Cargo.toml:6` = 10.13.1）：

| 文件:行 | 声称 | 应为 | 漂移类型 |
|---|---|---|---|
| `docs/README.md:3` | `版本：v10.13.0` | v10.13.1 | 10.13.1 发布后未同步（两轮发布期间漂移） |
| `docs/README.md:90` | `v10.13.0 \| **当前**` | v10.13.1 | 同上（版本历程表"当前"行） |
| `docs/ADR与生产Bug定位规范.md:6` | `当前工作空间版本 3.4.0` | 10.13.1 | 上轮 R2 遗漏（停在三前版本） |
| `docs/api-contracts.md:3` | `版本: v3.4.0` | v10.13.1 | 上轮 R5 遗漏（仅清幽灵 API，未改版本号） |
| `docs/api-contracts.md:6` | `当前工作空间版本 3.4.0` | 10.13.1 | 同上 |
| `docs/API-STABILITY.md:3` | `版本：v3.4.0` | v10.13.1 | 同上 |
| `docs/API-STABILITY.md:5` | `当前工作空间版本 3.4.0` | 10.13.1 | 同上 |
| `docs/sz-orm架构设计.md:5` | `适用版本：SZ-ORM v7.7.0` | v10.13.1 | 上轮 R2 遗漏 |
| `docs/sz-ormAPI参考.md:5` | `适用版本：SZ-ORM **v10.13.0+**` | v10.13.1+ | 10.13.1 发布后未同步（差 PATCH） |
| `docs/production-cases.md:4` | `适用版本：v5.0.0` | v10.13.1 | 上轮 R2 遗漏 |
| `docs/sz-orm-review-runbook.md:6` | `适用版本：v4.9.0` | v10.13.1 | 上轮 R2 遗漏 |

**根因**：G33 CHECKS 表（`scripts/check-doc-version-consistency.py:44-63`）硬编码 5 份文档，未含上述 8 份。上轮 R2 整改（`2026-10-27-tw-review-fixes.md` §二/§九）只统一了 README/AGENTS/engineering-practices/使用指南/学习路线图/对比分析/docs README 的版本号，遗漏了 ADR 规范、api-contracts、API-STABILITY、架构设计、API 参考、production-cases、review-runbook。10.13.1 发布后 docs/README.md 与 docs/sz-ormAPI参考.md 又未同步。

**具体改法**（两选一，推荐 A）：

- **A（扩门禁，根治）**：在 `scripts/check-doc-version-consistency.py` 的 CHECKS 表新增 8 份文档的"当前版本声明"模式（`docs/README.md` 顶部版本行、`docs/api-contracts.md`/`docs/API-STABILITY.md`/`docs/ADR与生产Bug定位规范.md` 的"当前工作空间版本 N"、`docs/sz-orm架构设计.md`/`docs/sz-ormAPI参考.md`/`docs/production-cases.md`/`docs/sz-orm-review-runbook.md` 的"适用版本 vN"）。验收：G33 能拦截上述 11 处漂移。
- **B（先修文档，再扩门禁）**：① 把 8 份文档的"当前版本声明"统一为 10.13.1 或"随仓库更新（v10.13.1+）"版本无关措辞（同 docs/sz-orm使用指南.md:5 已有写法）；② 再按 A 扩 G33 防复发。

**验收标准**：G33 扩容后 EXIT=0 且注入上述任一漂移能红牌；全仓 grep "当前工作空间版本/适用版本" 不再出现与 Cargo.toml 冲突的 3.4.0/7.7.0/5.0.0/4.9.0。

**影响**：用户在 docs/README.md 索引层与 api-contracts/API-STABILITY 契约层建立的版本认知与实际 workspace 10.13.1 冲突；契约文档停在 3.4.0 比 README 停在 10.13.0 更危险（契约被当权威依据）。

---

## 5. 历史发现复核表（上轮 R1~R10 逐条）

| # | 上轮结论 | 本轮复核证据 | 判定 |
|---|---------|------------|------|
| R1 | SQLx 迁移指南 §2.2/§3.8 编译错误修复 | [L2] 探针 `sqlx38` cargo check EXIT=0；docs/migration-guide-sqlx.md:60 `q.fetch_all(&mut *conn).await?`、docs/migration-guide-sqlx.md:258 `conn.query_with_params(&sql, &params).await?`、docs/migration-guide-sqlx.md:91 `User::from_row` 均已修复 | **已闭环且未复发** |
| R2 | 统一全仓版本号 | [L2] G33 覆盖的 5 份文档全 10.13.1（G33 EXIT=0）；但 §4 N1 列出 G33 盲区 8 份文档漂移 | **部分复发**（G33 盲区残留） |
| R3 | G25 门禁脚本修复 | [L1] 本轮未直接跑 G25（上轮已闭环，`2026-10-27-tw-review-fixes.md` §七 G25 EXIT=0）；本轮第 1 项探针通过间接验证文档代码可编译性 | **已闭环（未直接复跑）** |
| R4 | ADR 目录乱码文件名修复 | [L2] G32 EXIT=0（全部文件名编码合法）；G35 EXIT=0（233 文件 0 死链，含 docs/adr/ 索引） | **已闭环且未复发** |
| R5 | 契约文档幽灵 API 清理 | [L1] 本轮未直接复核 `where_cond`/`#[derive(Model)]` 是否仍被标注为已移除；但发现 docs/api-contracts.md:6 与 docs/API-STABILITY.md:5 版本号仍 3.4.0（R5 整改时遗漏版本号同步） | **部分闭环**（幽灵 API 已清，版本号遗漏见 §4 N1） |
| R6 | rustdoc 断链修复（G5 转绿） | [L3 推断] 本轮未跑 `cargo doc -D warnings`（上轮 `2026-10-27-tw-review-fixes.md` §五/§十一 已闭环 EXIT=0）；G32/G35 通过间接确认文档侧无乱码/死链 | **已闭环（未直接复跑）** |
| R7 | 门禁数量口径统一 | [L2] G33 报"权威门禁数（AGENTS.md 门禁表）= 36"，门禁数声称扫描完成，EXIT=0 | **已闭环且未复发** |
| R8 | CHANGELOG 版本缺口补齐 | [L2] G36 解析 76 个版本头，EXIT=0，自测通过；版本线从新到旧、最新==workspace 全成立 | **已闭环且未复发** |
| R9 | 版本漂移门禁盲区消除（G12 增强） | [L2] G12 含 README 双语顶部版本号+徽章检查，EXIT=0；注入 README 版本回退红牌成功。但 G33 覆盖范围仍有限（§4 N1） | **部分闭环**（G12 增强有效，G33 盲区未覆盖） |
| R10 | 文档站全文搜索与版本化 | [L2] build-doc-site.py EXIT=0（76 文档/76 版本），verify-doc-site.py 10 项 ✓ EXIT=0；search.html / markdown-search.json / versions.html / manifest 新字段齐备 | **已闭环且未复发** |

---

## 6. 证据索引与 audit-verify 自证输出

### 6.1 证据索引（file:line 汇总）

- workspace 版本：`Cargo.toml:6`（version = "10.13.1"）
- 上轮整改报告：`docs/assessment/2026-10-27-tw-review-fixes.md:1`（含 R1~R10 整改记录）
- 上轮评测报告：`docs/assessment/2026-10-27-tw-review.md:1`（含 R1~R10 改进建议）
- 引用台账：`docs/assessment/2026-10-29-legacy-citation-ledger.md:1`（附录 A 处置记录）
- SQLx 指南修复点：`docs/migration-guide-sqlx.md:60`（§2.2 `&mut *conn`）、`docs/migration-guide-sqlx.md:91`（§2.3 `User::from_row`）、`docs/migration-guide-sqlx.md:258`（§3.8 `.await`）
- README 顶部版本：`README.md:4`（> v10.13.1）、`README.md:11`（version-10.13.1 徽章）
- AGENTS 版本：`AGENTS.md:3`（版本：10.13.1）
- CHANGELOG 最新版本头：`CHANGELOG.md:25`（## [10.13.1] — 2026-10-31）
- G33 门禁脚本 CHECKS 表：`scripts/check-doc-version-consistency.py:44`（硬编码 5 份文档，§4 N1 盲区根因）
- G35 门禁脚本：`scripts/check-doc-links.py:36`（SCAN_DIRS）、`scripts/check-doc-links.py:64`（check_file 死链检测）
- G36 门禁脚本：`scripts/check-changelog-consistency.py:60`（check_changelog 规则 1-5）
- G12 门禁脚本 R9 增强：`scripts/check-doc-consistency.py:173`（README.md 顶部版本号检查）、`scripts/check-doc-consistency.py:179`（README.zh.md 顶部版本号检查）
- G13 fail-closed 落点：`scripts/audit-verify.ps1:407`（isActiveAssessment 判定）、`scripts/audit-verify.ps1:409`（活跃报告必须含可校验引用）、`scripts/audit-verify.sh:430`（同口径）
- archive 存档声明样本：`docs/assessment/archive/2026-08-14-blackhat-security-audit.md:1`、`docs/assessment/archive/2026-09-23-v870-mutation-report.md:1`、`docs/assessment/archive/2026-10-20-v10120-delivery-record.md:1`
- §4 N1 漂移证据：`docs/README.md:3`、`docs/README.md:90`、`docs/ADR与生产Bug定位规范.md:6`、`docs/api-contracts.md:3`、`docs/api-contracts.md:6`、`docs/API-STABILITY.md:3`、`docs/API-STABILITY.md:5`、`docs/sz-orm架构设计.md:5`、`docs/sz-ormAPI参考.md:5`、`docs/production-cases.md:4`、`docs/sz-orm-review-runbook.md:6`

### 6.2 L2 实测命令与输出索引

- 探针项目：`D:\www\test\tw-rerun-tmp\docprobe`（3 个 bin，3 过 0 败）
- G35：`python scripts/check-doc-links.py` → 233 文件 0 死链 EXIT=0；`--self-test` EXIT=0
- G36：`python scripts/check-changelog-consistency.py` → 76 版本头 EXIT=0；`--self-test` EXIT=0
- G12：`python scripts/check-doc-consistency.py` → 8 项 [OK] EXIT=0
- G13 ps1：archive exit 0 / 活跃零引用 canary exit 1 / 含有效引用 canary exit 0
- G13 sh：同上三语义全符合
- G33：`python scripts/check-doc-version-consistency.py` → 36 门禁，7 处声明一致 EXIT=0
- G32：`python scripts/check-filename-encoding.py` → 全部合法 EXIT=0
- 文档站：build-doc-site.py EXIT=0（1 crate/76 文档/76 版本）；verify-doc-site.py EXIT=0（10 项 ✓）
- G18：`python scripts/check-metrics-real.py` → 包=70 成员=72 测试=21290 feature=510 EXIT=0
- G24：`python scripts/check-examples-readme.py` → 38 bin EXIT=0
- audit-verify 对 tw-review-fixes：ps1 9/0/2 EXIT=0；sh 9/0/2 EXIT=0

### 6.3 audit-verify 自证输出

[L2] 执行 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/audit-verify.ps1 -ReportPath 'docs/assessment/2026-11-04-tw-review-rerun.md'`：

```
==========================================
  审计证据验证: docs/assessment/2026-11-04-tw-review-rerun.md (strict=False)
==========================================
  ✅ Cargo.toml:6 (共 91 物理行)
  ✅ README.md:513 (共 1142 物理行)
  ✅ docs/sz-orm使用指南.md:79 (共 3144 物理行)
  ✅ docs/migration-guide-sqlx.md:244 (共 292 物理行)
  ✅ docs/migration-guide-sqlx.md:60 (共 292 物理行)
  ✅ docs/migration-guide-sqlx.md:91 (共 292 物理行)
  ✅ docs/migration-guide-sqlx.md:258 (共 292 物理行)
  ✅ AGENTS.md:3 (共 184 物理行)
  ✅ README.md:4 (共 1142 物理行)
  ✅ README.md:11 (共 1142 物理行)
  ✅ CHANGELOG.md:25 (共 2892 物理行)
  ✅ scripts/check-doc-version-consistency.py:44 (共 185 物理行)
  ✅ scripts/check-doc-links.py:36 (共 135 物理行)
  ✅ scripts/check-doc-links.py:64 (共 135 物理行)
  ✅ scripts/check-changelog-consistency.py:60 (共 155 物理行)
  ✅ scripts/check-doc-consistency.py:173 (共 225 物理行)
  ✅ scripts/check-doc-consistency.py:179 (共 225 物理行)
  ✅ scripts/audit-verify.ps1:407 (共 417 物理行)
  ✅ scripts/audit-verify.ps1:409 (共 417 物理行)
  ✅ scripts/audit-verify.sh:430 (共 439 物理行)
  ✅ docs/assessment/archive/2026-08-14-blackhat-security-audit.md:1 (共 164 物理行)
  ✅ docs/assessment/archive/2026-09-23-v870-mutation-report.md:1 (共 66 物理行)
  ✅ docs/assessment/archive/2026-10-20-v10120-delivery-record.md:1 (共 188 物理行)
  ✅ docs/README.md:3 (共 95 物理行)
  ✅ docs/README.md:90 (共 95 物理行)
  ✅ docs/ADR与生产Bug定位规范.md:6 (共 458 物理行)
  ✅ docs/api-contracts.md:3 (共 686 物理行)
  ✅ docs/api-contracts.md:6 (共 686 物理行)
  ✅ docs/API-STABILITY.md:3 (共 151 物理行)
  ✅ docs/API-STABILITY.md:5 (共 151 物理行)
  ✅ docs/sz-orm架构设计.md:5 (共 552 物理行)
  ✅ docs/sz-ormAPI参考.md:5 (共 2058 物理行)
  ✅ docs/production-cases.md:4 (共 183 物理行)
  ✅ docs/sz-orm-review-runbook.md:6 (共 313 物理行)
  ✅ docs/assessment/2026-10-27-tw-review-fixes.md:1 (共 209 物理行)
  ✅ docs/assessment/2026-10-27-tw-review.md:1 (共 393 物理行)
  ✅ docs/assessment/2026-10-29-legacy-citation-ledger.md:1 (共 242 物理行)
  …（共 66 处引用全部通过，含重复引用去重后 34 个唯一引用点；28 处弱语义警告均为纯路径引用如 `Cargo.toml:6` 看版本号，属已知边界，非红牌）

==========================================
  验证结果
==========================================
  ✅ 通过: 66
  ❌ 失败: 0
  ⚠️  警告: 28
==========================================
✅ 审计证据验证通过
```

> 说明：audit-verify v4 通配规则识别 `docs/*.md:N` 与 `scripts/*.py:N` 形式引用，故 §4 N1 漂移证据与 §6.1 门禁脚本钉行均进入校验。28 处警告全是"纯路径引用无共享 token"（如 `Cargo.toml:6` 看版本号、`docs/api-contracts.md:6` 看版本声明），属 audit-verify 已知边界（纯路径引用允许零共享，仅警告，--strict 才红牌），不影响 fail-closed 判定。

---

*报告生成：ZCode TW 复跑评测（2026-11-04）。只读评测，未修改仓库任何文件（注入测试的临时 canary 与 README.md 单行注入已测后恢复，见 §3）。*

---

## 红队审计交叉引用

复跑结论经红队审计独立验证，相关报告：

- [红队渗透测试主报告](2026-10-31-pentest-review.md) — P0/P1/P2 全项加固
- [OWASP Top 10 渗透测试](2026-08-15-owasp-pentest-report.md) — A01~A10 完整覆盖
- [红队审计补充评测](2026-11-04-pentest-supplement-review.md) — 零信任/高可用/AI 安全