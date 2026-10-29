# sz-orm TW 评测整改报告（2026-10-27）
> **安全说明（2026-10-28 占位化）**：本文档中的服务器 IP 已替换为占位符；实际主机地址与凭据由运维在对应测试服务器的环境变量中提供，禁止提交真实地址/密码到仓库。

- **配套报告**：[2026-10-27-tw-review.md](2026-10-27-tw-review.md)（技术文档工程师评测，11 节）
- **整改日期**：2026-10-27（§九 增补于 2026-10-28）
- **整改范围**：评测报告 §10 改进建议 R1/R2/R4/R5/R6/R7/R9 的落地；R3（G25 修复）由并行会话完成；R8（CHANGELOG 版本缺口）/R10（文档站搜索与版本化）为长期项未在本轮实施
- **环境说明**：git 位于 `D:\Git\cmd\git.exe`（不在 PATH，调用时临时加入 PATH 即可）；G11/G14 已于 2026-10-28 用该路径在本机验证通过（见 §八/§十一）；文件重命名使用文件系统操作（后续提交时 git 会识别为 rename）

---

## 一、R1 · SQLx 迁移指南两处编译错误修复

**问题**：`docs/migration-guide-sqlx.md` §2.2 `query!(...).fetch_all(&pool)` 报 E0308（`&Pool` ≠ `&mut dyn Connection`）；§3.8 `conn.query_with_params(&sql, &params)?` 报 E0277（缺 `.await`）；§2.3 `User::from_rows(&rows)` 引用不存在的 API。

**整改**：
- §2.2 改为 `query!` 宏文档（`packages/sz-orm-macros/src/lib.rs:460-462`）的真实用法：`let q = sz_orm_core::query!("SELECT id, name FROM users WHERE id = ?"); let users = q.fetch_all(&mut *conn).await?;`（`PooledConnection` 未实现 `Connection` trait，需 `&mut *conn` 解引用）。
- §2.3 结果映射改为 `FromQueryResult` trait 的 `from_row`：`rows.iter().map(User::from_row).collect::<Result<Vec<User>, _>>()?`。
- §3.8 补 `.await`。

**验证**（探针 `D:\www\test\tw-review-tmp\docprobe`，独立项目）：
```
cargo check --bin sqlx22 → Finished, EXIT=0
cargo check --bin sqlx38 → Finished, EXIT=0
```

## 二、R2（部分） · README 版本号同步

**问题**：`README.md:4/11/44/136`、`README.zh.md:4/11` 仍写 v10.12.0，而 `Cargo.toml:6` 为 10.13.0。

**整改**：双语 README 版本号与版本徽章全部更新为 v10.13.0；README "23 gates" 更新为 "25 gates"（`README.md:96`）。AGENTS.md 版本已由并行会话同步为 10.13.0。

**遗留**：`docs/sz-orm使用指南.md`/`docs/sz-ormAPI参考.md`/`docs/sz-orm学习路线图.md`（v7.7.0）与 `docs/sz-orm与同类产品对比分析.md`（v8.8.0）、`docs/README.md`（v3.4.0）的深层版本统一建议由并行会话批量处理（涉及文档重写，非本次最小整改范围）。

## 三、R4 · ADR 目录乱码文件名修复 + 缺失 ADR 补写

**问题**：`docs/adr/` 下 10 个 ADR 文件名为 GBK 乱码（如 `0001-杩炴帴姹犵敤-AtomicU32-鑰岄潪-Mutex.md`），`docs/ADR与生产Bug定位规范.md` 亦乱码；且 ADR-0003/ADR-0009 文件缺失导致 docs/README.md 两处死链。

**整改**：
- 重命名 10 个文件为规范中文名（`0001-连接池用-AtomicU32-而非-Mutex.md` … `0011-异步运行时仅支持tokio.md`、`ADR与生产Bug定位规范.md`）。
- 补写 `docs/adr/0003-事务嵌套用-SAVEPOINT-加深度限制.md` 与 `docs/adr/0009-QueryBuilder只生成SQL不执行.md`（决策内容依据现有代码契约：`Transaction::savepoint`/`DEFAULT_MAX_NESTING_DEPTH=8` 与 QueryBuilder 纯生成 SQL 的设计）。

**验证**：README.md / README.zh.md / docs/README.md / docs/quickstart.md / docs/adr/README.md 全部相对链接 Test-Path 可达（0 死链）。

## 四、R5 · 契约文档幽灵 API 清理

**问题**：`docs/api-contracts.md:342` 把已删除的 `where_cond(s)`/`or_where(s)` 列为契约方法；`docs/API-STABILITY.md:45` 把不存在的 `#[derive(Model)]` 列为 Tier 1 稳定宏。

**整改**：
- `api-contracts.md` 改为参数化方法清单并注明 "v5.0.0 起 `where_cond`/`or_where` 已移除"。
- `API-STABILITY.md` 将 `#[derive(Model)]` 标注为"尚不存在，属规划中 API，请勿引用"。

## 五、R6 · rustdoc 断链修复（G5 转绿）

**问题**：`packages/sz-orm-core/src/simd.rs:174` 的 doc 注释中 `#[cold]` 被 rustdoc 当作断链，`RUSTDOCFLAGS="-D warnings" cargo doc` 失败。

**整改**：`#[cold]` 改为反引号包裹 `` `#[cold]` ``，避免 intra-doc 链接解析。

**验证**：
```
RUSTDOCFLAGS="-D warnings" cargo doc -p sz-orm-core --no-deps → Finished, EXIT=0
```

## 六、R7 · 过时扫描产物清理 + 门禁数量口径统一

**整改**：
- `missing_docs_scan.txt`（139 行过时警告）替换为状态说明，注明当前 cargo doc 无 missing docs 警告及重新生成命令。
- `AGENTS.md:22` 标题 "24 道门禁" → "25 道门禁"（表本身 25 行）。
- `docs/README.md:33` "10 道门禁" → "25 道门禁"。
- `README.md:96` "23 gates" → "25 gates"。

**遗留**：`docs/sz-orm-engineering-practices.md` 仍为"标准 7 道门禁 + 特殊强化门禁"的旧结构，建议后续按 AGENTS.md 25 道门禁表重写该文档。

## 七、验证汇总

| 项 | 结果 |
|----|------|
| G12 文档与代码一致性 | ✅ 退出码 0 |
| G18 度量真实性 | ✅ 退出码 0 |
| G24 示例清单同步 | ✅ 退出码 0 |
| G25 文档代码可编译性 | ✅ 退出码 0（并行会话已完成 v2 脚本修复 + 使用指南代码块修正） |
| cargo doc -D warnings（G5） | ✅ 退出码 0（simd.rs 修复后实测；后因并行 rusqlite 升级冲突暂缓复跑，见 §9.4） |
| sqlx 指南 §2.2/§3.8 独立编译 | ✅ 探针 EXIT=0 |
| 入口文档相对链接 | ✅ 0 死链 |
| docs/adr 索引链接 | ✅ 11/11 可达 |

## 八、遗留事项（已更新：二轮处理结果）

1. **深层中文文档版本统一 ✅ 已完成**：使用指南/API 参考/学习路线图头部改为"随仓库更新（v10.13.0+）"版本无关措辞；对比分析更新为 v10.13.0 并修正旧 E:\ 路径；docs/README.md 索引更新为 v10.13.0 并补版本历程行。
2. **`docs/sz-orm-engineering-practices.md` 门禁结构重写 ✅ 已完成**：新增"当前门禁总表（25 道）"章节（含 G1-G25 命令），旧 7+14 结构标注为历史演进 CI 配置说明；章节重编号 0-7；测试合计与部署清单同步更新。
3. **G14/G11 已在本机验证通过**：git 实为 `D:\Git\cmd\git.exe`（不在 PATH）。`python scripts/check-doc-sync.py --diff HEAD`（PATH 加入 D:\Git\cmd 后）触发 Rule 1（cargo-version）/Rule 3（new-pub-api），均 SYNCED → **EXIT=0**；`git diff --name-only HEAD` 列出 293 个未提交改动（均为各会话待提交文件，无 ADR-0001 违规）。G5 已在并行会话统一 rusqlite 后恢复通过（EXIT=0）。
4. 本会话产生的探针项目位于 `D:\www\test\tw-review-tmp\docprobe`（仓库外），可作为后续复验工件。
5. R8（CHANGELOG 补录 22 条 + 推断日期）与 R10（文档站全文搜索 + 版本历程注入 + 版本归档 `versions.html` + 75 个详情页）已于 2026-10-28 完整落地（见 §十/§十一）。G11/G14 也已在本机用 `D:\Git\cmd\git.exe` 验证通过（G14 EXIT=0）。

---

## 九、二轮整改增补（R9 落地 + 链接全量清零）

> 本节为遗留问题处理轮的增补记录（2026-10-28）。

### 9.0 遗留 1/2 落地（深层文档版本统一 + 门禁结构重写）

- 深层中文文档版本统一：`docs/sz-orm使用指南.md`、`docs/sz-ormAPI参考.md`、`docs/sz-orm学习路线图.md` 头部改为"随仓库更新（v10.13.0+）"版本无关措辞并更新日期；`docs/sz-orm与同类产品对比分析.md` v8.8.0→v10.13.0 且旧 `file:///E:/` 死路径改相对路径；`docs/README.md` 索引 v3.4.0→v10.13.0 并补版本历程行。
- `docs/sz-orm-engineering-practices.md` 门禁结构重写：新增"当前门禁总表（25 道）"章节，旧 7+14 结构标注为历史演进 CI 配置说明，章节重编号 0-7，测试合计（5,442→21,161+）与部署清单同步。

### 9.1 R9 · 版本漂移门禁盲区消除（G12 增强）

**问题**：G12 原只检查 AGENTS.md/engineering-practices 的版本号，README/README.zh 顶部版本漂移（10.12.0 vs 10.13.0）无门禁拦截——本轮整改前实际发生了该漂移。

**整改**：`scripts/check-doc-consistency.py` checks 表新增 4 项：README.md/README.zh.md 的顶部版本号（`(> v)(\d+\.\d+\.\d+)`）与版本徽章（`(version-)(\d+\.\d+\.\d+)`），支持 `--fix` 自动修正。

**验证**：
- 正向：`python scripts/check-doc-consistency.py` → EXIT=0
- 反向（注入漂移实测）：临时将 README.md 版本改回 10.12.0 → G12 报 `[MISMATCH] README.md 顶部版本号: 实际=10.13.0, 文档=10.12.0` + 版本徽章 MISMATCH，EXIT=12；恢复后 EXIT=0

### 9.2 G25 符号索引嵌套目录覆盖验证

[L2] `build_symbol_index()` 当前以 `rglob("src/**/*.rs")` 扫描，实测索引 22,996 个方法符号，嵌套目录文件（如 `packages/sz-orm-ai/src/autonomous/policy_engine.rs` 的 `load_from_config`、`packages/sz-orm-core/src/plugin_marketplace/plugin_marketplace_verifier.rs` 的 `load_batch`）均已收录；`where_gte`（真实不存在）正确缺席。

### 9.3 链接检查扩展至全量核心文档（80 文件 0 死链）

将链接检查从 4 份入口文档扩展到 80 份核心用户文档（根目录 *.md + docs 根 + docs/adr + docs/migration），发现并修复：
- `README.en.md`（遗留旧版）8 处死链：性能基准链接改指 `bench-comparison/BENCHMARK_REPORT.md`（真实存在），5 行指向不存在文档的表格行移除，对比分析链接修正为 `docs/sz-orm与同类产品对比分析.md`
- `docs/sz-orm-engineering-practices.md` 指向仓库外 sz-rust 的链接改为纯文本说明
- 修正本轮整改在对比分析中引入的 `../../Cargo.toml` 多一级路径错误 → `../Cargo.toml`

**最终状态**：`Checked 80 files; total broken links: 0`

### 9.4 G5 复验状态说明

simd.rs 修复后 `RUSTDOCFLAGS="-D warnings" cargo doc -p sz-orm-core --no-deps` 已实测通过（EXIT=0，见 §五）。二轮整改末尾复跑时出现 `failed to select a version for 'libsqlite3-sys'` 依赖冲突：经查系并行会话在途升级——`packages/sz-orm-core/Cargo.toml`（约 L325）声明 `rusqlite = { version = "0.32", features = ["bundled"] }`（2026-10-28 00:53 修改），与 `packages/sz-orm-studio/Cargo.toml`（L28）的 `rusqlite = "0.31"` 因 `links = "sqlite3"` 冲突，**与本轮文档/脚本改动无关**，待并行会话统一 rusqlite 版本后 G5 即可复跑。侧证：探针项目（独立依赖图，path 依赖同一份 sz-orm-core）在升级后的依赖下 `cargo check --bin sqlx38` 仍通过（EXIT=0），说明冲突仅存在于 workspace 内 core/studio 之间。

---

## 十、长期项落地（R8 + R10，2026-10-28）

### 10.1 R8 · CHANGELOG 中间版本缺口补齐（54 → 76 个版本条目）

- **补录 22 个版本**，每条标注证据来源：有精确日期的 15 个（7.2.0/7.3.0/7.5.0/7.6.0/8.4.0/8.6.0/8.7.0/8.8.0/9.5.0/9.6.0/9.7.0/9.9.0/10.1.0/6.2.0/6.3.0，依据 `docs/assessment/` 对应交付记录）；7 个无独立交付记录的版本（3.5.0/3.7.0/4.6.0/4.7.0/4.9.0/5.1.0/8.5.0，依据 AGENTS.md 版本线、迁移指南头部、custom-diagnostic-guide 明文记载），日期已按相邻版本时间线推断为区间（如 8.5.0 → 2026-09-22~09-23）。
- **不编造**：仍无交付证据的版本（1.3.0/1.4.0、3.1.0、7.1.0、7.7.0、8.3.0、10.0.0、10.4.0、10.5.0）保持缺失，CHANGELOG 头部补录声明中明示。
- [L2] 验证：版本头总数 76，倒序时间线连续（`Select-String '^## \['` 计数）。

### 10.2 R10 · 文档站全文搜索与版本化

**`scripts/build-doc-site.py` 新增**：
- `build_markdown_search_index()`：索引 72 份核心用户文档（根 README×3 + CHANGELOG + docs 根 + adr + migration）的标题/章节标题/首段摘要 → `markdown-search.json`
- `write_markdown_search_page()`：纯静态 `search.html`（原生 JS，零第三方依赖，标题×10 / 章节×3 / 摘要×1 评分排序，结果链接回仓库源文件）
- `inject_version_history()`：首页注入版本历程横幅（链接 CHANGELOG 与 docs/README.md 版本历程表）
- `write_version_archive()`：历史版本归档——`versions.html` 版本索引页 + `versions/<ver>.html` 详情页（解析 CHANGELOG 76 个版本头）
- `--repo-root` 参数 + Python 3.8 兼容（`from __future__ import annotations`，与 check-doc-consistency.py 同款方案）
- manifest 新增 `has_markdown_search` / `markdown_doc_count` / `has_version_archive` / `version_archive_count` / `version_history_linked` 字段

**`scripts/verify-doc-site.py` 同步**：新增 Markdown 搜索索引/搜索页、版本归档存在性与非空校验。

**端到端实测**（最小 fake rustdoc 产物）：
```
python scripts/build-doc-site.py → Markdown 全文搜索: 72 份文档；版本归档: 75 个版本；BUILD_EXIT=0
python scripts/verify-doc-site.py → 10 项 ✓ 全部通过（含版本归档）；VERIFY_EXIT=0
模拟查询实测：连接池→18 命中（ADR-0001/0008 居前）、迁移指南→12 命中、savepoint→3 命中
```

**附带发现（已闭环）**：构建时敏感信息扫描首次抓到 `docs/e2e-execution-guide.md` 中的真实服务器 IP（密码已占位，IP 未占位）。该发现验证了 R10 新产物被敏感信息门禁正确覆盖；随后已对全仓库 docs 真实 IP 统一占位化（见 §十一）。

### 10.3 ADR-0003/0009 权威版恢复（并行会话协作修正）

并行会话从 git 历史恢复了 ADR-0003（2026-07-19，含修复编号 H-8 与 `validate_savepoint_name` 设计）与 ADR-0009（2026-07-22，含 ThinkPHP 差异分析与 Bug 定位提示）的**原始内容**（以"-旧版"后缀共存）。本轮已将原始权威内容恢复为主文件名并移除后缀副本——此前整改报告 §三 中"补写"的简化版本被原始历史记录取代（原始内容详实度远高于补写版）。

---

## 十一、收尾清零轮（2026-10-28）

### 11.1 G5 恢复通过

并行会话统一 rusqlite 版本后，`RUSTDOCFLAGS="-D warnings" cargo doc -p sz-orm-core --no-deps` 复跑通过：`Finished` + `EXIT=0`。

### 11.2 docs/评估 乱码文件名清零

并行会话已将 `docs/评估/` 大部分文件改名规范；本轮用可逆 GBK→UTF-8 转码（gb18030 兼容 PUA 字符）批量还原剩余 7 个乱码文件：
- `2026-08-11_文档真实性评估_v4.md`、`2026-08-11_生产就绪评估.md`、`2026-08-11_生产就绪评估_v4.md`、`2026-08-12_db-fusion实验评估.md`、`2026-08-12_v4.2.0_基线评估.md`、`2026-09-20_深度成熟度评估_基于源码.md`、`2026-08-11_突破方向可行性评估_基于实际代码.md`
- 经验：NTFS 上 UTF-8→GBK 乱码可逆还原；`gbk` 对 PUA 字符失败时改用 `gb18030`。

### 11.3 全仓库 docs 真实服务器 IP 占位化（P0-3 收尾）

对 `docs/e2e-*`、`docs/soak-toolkit-*`、`docs/assessment/`（含本报告）与 `docs/delivery/` 全部 .md 统一替换两台测试服务器的真实主机 IP → `<remote-server-ip>` / `<legacy-server-ip>`；[L2] 复扫残留数为 0。此举补齐 P0-3"凭据占位"未覆盖的主机地址字段。

### 11.4 CHANGELOG 推断日期消除"待考"

7 个无独立交付记录的补录版本日期改为基于相邻版本时间线的推断区间（如 8.5.0 → 2026-09-22~09-23），头部补录声明同步措辞。

### 11.5 文档站版本归档（R10 完整落地）

`build-doc-site.py` 新增 `write_version_archive()`：`versions.html` 版本索引页 + `versions/<ver>.html` 详情页（解析 CHANGELOG 76 个版本头，实测生成 75 个版本详情页）；`verify-doc-site.py` 同步校验。端到端：BUILD_EXIT=0、VERIFY_EXIT=0。

### 11.6 收尾后最终门禁状态

[L2] G12=0 / G18=0 / G24=0 / G25=0 / G5=0；80 份核心文档链接 0 死链；docs 全目录真实 IP 残留 0；audit-verify 对整改报告通过。

---

*整改报告生成：2026-10-27。所有改动均经门禁/编译/链接验证。*