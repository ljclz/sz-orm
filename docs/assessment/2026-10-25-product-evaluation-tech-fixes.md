# sz-orm 产品评测技术解决措施

- **配套报告**：[2026-10-25-product-evaluation.md](2026-10-25-product-evaluation.md)
- **性质**：将产品评测发现的问题转化为可执行的技术整改措施（脚本/门禁/文档）
- **原则**：所有修复以"门禁自动拦截"为终态，禁止依赖人眼复查；历史版本日志中的数字（如 "v4.1.0 New Capabilities (9 feature gates)"）不属于当前度量，不比对。
- **当前盲区实测**：`check-metrics-real.py` 的 `CLAIM_PATTERNS` 仅比对测试/包/方言/成员，**无 feature 数模式**（[scripts/check-metrics-real.py:83-105](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/check-metrics-real.py#L83)），且 `scan_readme` 只扫 `README.md`（[scripts/check-metrics-real.py:108](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/check-metrics-real.py#L108)），导致 README.zh.md 停留在 v9.4.0 却不报错。

---

## M1：门禁脚本补盲区（0.5 天，最高优先级）

### 1.1 G18 增加 feature 总数比对

在 `scripts/check-metrics-real.py` 的 `CLAIM_PATTERNS` 中追加一条**精确模式**，只匹配 Overview 当前总量声称：

```python
(r"(\d[\d,]*)\s*feature gates for modular composition",
 "features", "feature 总数(Overview)", r"\g<1> feature gates for modular composition"),
```

- 实测效果：`README.md:44` "517 feature gates for modular composition" → 与实测 508 不符，被 G18 抓出并 `--fix` 自动修正。
- 注意：**不要**用宽泛的 `(\d+)\s*feature gates?` 模式，否则会误报 README.md:66/80/136/196 的历史版本日志（实测演示已确认这些是 v4.x/v3.x 历史声明，非当前总量）。

### 1.2 G18 增加 README.zh.md 扫描

将 `scan_readme` 的单文件扫描扩展为扫描 `["README.md", "README.zh.md"]`。现有 `CLAIM_PATTERNS` 已含中文模式（"工作空间成员"/"测试"/"SQL 方言"），可直接复用。

- 实测效果：`README.zh.md:4` "v9.4.0 · 72 工作空间成员 · 20424+ 测试 · 28 SQL 方言" → 测试数 20424 ≠ 21134，被 G18 抓出。
- 版本号 `v9.4.0` 不在 G18 比对范围（G18 管数字度量，版本号由 G12 管）——因此还需 M2.4 同步 README.zh 版本。

### 1.3 验证方式

```bash
python scripts/check-metrics-real.py          # 应报出 README.md:44 与 README.zh.md:4
python scripts/check-metrics-real.py --fix    # 自动修正后再次运行应 PASS
```

## M2：README 历史段落清理（0.5 天）

| # | 位置 | 问题 | 措施 |
|---|------|------|------|
| 2.1 | [README.md:13](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L13) | 版本徽章 9.4.0 过时 | 改为 10.12.0；或改用动态 shields.io 徽章（`shields.io/badge/version-...` 需静态值，无法动态读 Cargo.toml，建议仍由 G18/G12 校验） |
| 2.2 | [README.md:879](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L879) | "Total: 9905 tests" 为 v3.x 遗留，与顶部 21134 矛盾 | 删除该静态总数行，改为引用顶部质量基线表；或在 G18 增加该段落的精确匹配模式 |
| 2.3 | [README.md:445-448](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L445) | Quick Start 依赖 `sz-orm-core = "1.4"`，当前 10.12.0 | 改为 `sz-orm-core = "10.12"`（若 crates.io 已同步发布）；若 crates.io 仍滞后，则明确标注"最新能力使用 path/git 依赖"并给出版本同步的发布计划（见 M6） |
| 2.4 | [README.zh.md:4](file:///E:/vue/test/鲜视达/rust/sz-orm/README.zh.md#L4) | 中文版整体停留 v9.4.0 | 用脚本同步 README.md 顶部数据行到 README.zh.md；或将 README.zh 精简为桥页（保留中文指南 [docs/sz-orm使用指南.md](../sz-orm使用指南.md)），避免双份大文档漂移 |

## M3：示例清单自动同步（1 天）

- **问题**：README Examples 只列 8 个 bin，实际 `examples/src/bin/` 有 30+ 个（README.md:845-859）。
- **措施**：新增 `scripts/check-examples-readme.py`：
  1. 枚举 `examples/src/bin/*.rs` 文件名作为 bin 名集合；
  2. 解析 README Examples 表格中的 `--bin <name>` 提取已登记集合；
  3. 差集非空 → 报错退出（新增示例未登记）。
- **门禁化**：在 AGENTS.md 23 道门禁表追加 G24"示例清单同步检查"，或在 G12 脚本中登记该规则。

## M4：变异测试门禁分层落地（持续，架构评测建议落地）

- **日常门禁**：`cargo-mutants --in-diff`（增量变异），复用已有 [scripts/collect-mutation-diff.sh](../../scripts/collect-mutation-diff.sh)，纳入 G20 检查链——Windows 本地可在会话预算内跑完。
- **发布前门禁**：CI Linux 全量 `collect-mutation-linux.sh --fail-under 70`（对应 G20 原始定义）。
- **跨包盲区消除**：变异命令增加 `--test-package sz-orm-sqlx`（架构评测风险 3：core 的 pool 修复其回归测试在 sqlx 包中）。

## M5：快速开始闭环（3-5 天）

- **5.1** 仓库根提供 `docker-compose.yml`（MySQL + PostgreSQL，对应本机 G7 已就绪服务的版本），新用户 `docker compose up -d` 一键起库。
- **5.2** 新增 `docs/quickstart.md`：5 分钟从零跑通真库 CRUD——使用 `sz-orm make:model`（CLI 已实现，[cli/src/main.rs:45-60](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/src/main.rs#L45)）生成模型，而非手写 `impl Model`（README.md:459-477 当前门槛）。
- **5.3** README Quick Start 改为"方式 A：crates.io（若已同步）/ 方式 B：path 依赖（推荐）"并链接 docs/quickstart.md。

## M6：crates.io 发布流水线（1-2 天）

- **问题**：crates.io 仅发布到 1.0.0（AGENTS.md 2026-07-23），与当前 10.12.0 严重脱节，README 却推荐 crates.io 安装。
- **措施**：新增 `scripts/publish-crates.sh`：
  1. 按依赖拓扑排序（先 sz-orm-macros/core，再扩展包）；
  2. 逐个 `cargo publish`（失败可重试，记录已发布列表）；
  3. 发布完成后运行 G18 `--fix` 同步 README 版本引用。
- **备选策略**：若短期内不打算全量发布，则在 README 显著位置声明"最新能力走 path/git 依赖，crates.io 版本滞后"，消除用户按文档装 1.x 的信任陷阱。

---

## 优先级与工作量汇总

| 里程碑 | 问题 | 工作量 | 优先级 |
|--------|------|--------|--------|
| M1 门禁补盲区 | README 数字矛盾无法自动拦截 | 0.5 天 | P0 |
| M2 README 清理 | 版本/测试数/依赖版本/中文版滞后 | 0.5 天 | P0 |
| M6 crates.io 发布 | 发布版本严重滞后 | 1-2 天 | P0 |
| M5 快速开始闭环 | 新用户首查门槛高 | 3-5 天 | P1 |
| M3 示例清单同步 | 文档与代码脱节 | 1 天 | P1 |
| M4 变异门禁分层 | G20 本地不可全量落地 | 持续 | P2 |

**终态验收标准**：`python scripts/check-metrics-real.py` 对 README.md/README.zh.md 零告警；README 三处测试数字一致；Quick Start 按文档可 5 分钟跑通真库；新增示例未登记时门禁失败。