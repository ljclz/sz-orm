# sz-orm v4.3.0 feature gate 逐步启用计划

> 日期：2026-08-12
> 依据：M6-T3 编译验证（7 个 feature 独立编译 ✅ / 组合编译 ✅ / 5 新包 all-features ✅ / workspace 回归 ✅）
> 原则：所有新能力 feature gate 默认关闭，按 P1→P2→P3 优先级分阶段启用，每阶段独立验证，不破坏 sz-pay 生产依赖

---

# 一、7 个 feature gate 验证状态（M6-T3，2026-08-12）

| feature gate | 所属包 | 独立编译 | 组合编译 | 测试 |
|-------------|--------|---------|---------|------|
| `explain-analyzer` | sz-orm-explain + sz-orm-macros | ✅ | ✅（含 db-verify 组合） | 35 + 真库集成 |
| `query-flamegraph` | sz-orm-flamegraph | ✅ | ✅ | 8 |
| `n1-lint` | sz-orm-n1-lint + sz-orm-macros + cli | ✅ | ✅（含 core 组合） | 7 + 宏验证 + 交叉验证 4 |
| `lineage-viz` | sz-orm-audit | ✅ | ✅（含 data-lineage/data-quality） | 4 新增 + 128 包内 |
| `compile-governance` | sz-orm-core + sz-orm-macros | ✅ | ✅（含 data-validation/db-verify） | 3 + 集成 3 + 编译期强制验证 |
| `adaptive-query` | sz-orm-adaptive | ✅ | ✅ | 15 |
| `db-fusion` | sz-orm-fusion | ✅ | ✅ | 12 |

**已知环境限制**：workspace 全 feature 组合（`--all-features`）因 `rdkafka-sys` 的
cmake 构建失败（Windows 缺少 VS CMake 工具链，`0xc0000409`）无法全量编译——
为**既有环境问题**（v4.2.0 之前即存在，`sz-orm-queue --all-features` 单独验证确认），
与 v4.3.0 新增代码无关；v4.3.0 的 5 个新包 `--all-features` 编译全部通过。

---

# 二、分阶段启用计划

## 阶段一（P1，v4.3.0 发布时启用建议）

| feature | 启用理由 | 前置条件 |
|---------|---------|---------|
| `explain-analyzer` | 开发期性能问题前置拦截，编译期警告非阻断，风险最低 | 无（真库验证需 DATABASE_URL + SZ_ORM_QUERY_VERIFY=1，仅开发者本地/CI 启用） |
| `n1-lint` | 开发期 N+1 发现，标注宏 + CLI 双入口，无运行时影响 | 无（仅编译期/CLI 使用） |

**建议**：这两个 feature 纳入**开发工作流**（CI 中运行 `cargo clippy` + `sz-orm n1-lint --path=src`；
本地开发 `SZ_ORM_QUERY_VERIFY=1` + `db-verify`），不改变运行时行为。

## 阶段二（P2，v4.3.x 补丁/小版本）

| feature | 启用理由 | 前置条件 |
|---------|---------|---------|
| `compile-governance` | 合规（GDPR/等保）需求的项目可启用，PII 编译期强制零运行时开销 | 模型需先补齐 `#[pii]`/`#[mask]` 标注（编译失败会阻断，需先整改存量模型） |
| `lineage-viz` | 数据治理可视化的项目可启用，纯导出无副作用 | 无（依赖 `data-lineage`，SQL 血缘解析需 sqlparser） |
| `query-flamegraph` | 性能排查场景按需启用（输出 SVG/Brendan Gregg） | 无 |

## 阶段三（P3，实验性，按需启用）

| feature | 启用理由 | 前置条件 |
|---------|---------|---------|
| `adaptive-query` | 运行时自适应（自动分页/缓存），**缓存默认关闭**需显式开启 | 评估统计开销后启用；生产开启缓存前先验证 TTL 语义 |
| `db-fusion` | 多数据库融合 POC（实验性，转正建议见 `docs/评估/2026-08-12_db-fusion实验评估.md`） | 阶段一（TTL + 失效广播）未完成前仅限非生产实验 |

---

# 三、启用步骤（每阶段通用）

1. 在目标包启用 feature：`cargo add <pkg> --features <feature>` 或 Cargo.toml 手动配置
2. 运行该包测试：`cargo test -p <pkg> --features <feature>`
3. 运行全 workspace 门禁：`cargo test --workspace -j 2 --no-fail-fast` + `cargo clippy --workspace --all-targets -- -D warnings`
4. sz-pay 兼容性验证：sz-pay 不启用任何新 feature（crates.io 2.3.0 独立），行为不变
5. 文档同步：CHANGELOG.md 记录启用状态

---

> 文档结束。验证命令均已在 M6-T3 执行（2026-08-12），遵循 AGENTS.md 审计合规铁律。
