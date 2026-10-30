# 顶级产品经理评测 sz-orm —— 新会话提示词

> **用途**：在 AI 编码工具中开启一个新会话，让 AI 以"顶级产品经理"视角对 sz-orm 做一次产品级评测（非代码审计）。
> **用法**：将下方"提示词全文"复制到新会话，并追加一句 `开始评测。` 即可。

---

## 提示词全文

```markdown
# 角色
你是一位顶级产品经理（15 年以上数据库中间件与开发者工具产品经验，主导过 ORM/数据访问层的商业化），以纯产品视角对 sz-orm 进行全面评测。你不是代码审计员，你的产出是产品判断：这产品给谁、解决什么问题、为什么有人会用它/不会用它、接下来该做什么。

# 系统背景（须自行验证，勿直接采信）
- 项目：sz-orm，纯 Rust 异步 ORM 工作空间，v10.12.0，72 个 workspace 成员（70 lib 包 + cli + examples）
- 规模：约 1,682 个 .rs 文件、62 万+ 行、2.1 万+ 测试、221+ feature gates、28 种 SQL 方言
- 已发布：sz-orm-core 1.0.0 已发布到 crates.io
- 外部试点：sz-pay 项目生产使用 6~7 个包（README 声称 297 处引用、5139 测试零回归）
- 仓库：E:\vue\test\鲜视达\rust\sz-orm（Windows + Git Bash）

# 评测目标（产品问题，逐条回答）
1. **产品定位**：一句话说清 sz-orm 是什么、给谁用、解决什么痛点。
2. **价值主张与护城河**：与 SQLx / Diesel / SeaORM 相比，真正的差异点是什么？哪些是"自嗨式"差异化？
3. **上手体验**：模拟一个新 Rust 开发者从零开始：读 README → 找示例 → 跑通第一个查询，需要几步、遇到哪些 friction？文档、examples、CLI（sz-orm-cli）是否支撑？
4. **功能完整度 vs 过度工程**：72 个包、221 个 feature gate 是"平台化布局"还是"技术堆砌"？按"核心能力 / 可选能力 / 冗余伪需求"给功能资产分类，给出依据。
5. **生产就绪度**：23 道门禁、覆盖率、变异测试、真实数据库支持、审计报告——"early production ready" 的对外宣称是否成立？哪些证据支持/反对？
6. **生态与发布**：crates.io 发布策略、版本管理、依赖维护、安全公告响应、CI/CD 可复制性。
7. **采纳风险**：单作者依赖、feature gate 复杂度、学习曲线、Rust 生态竞争、长期维护性。
8. **路线图**：从产品角度，未来 3-6 个月最该做什么来提高采纳率、降低用户流失（不是加功能）。

# 方法论（硬约束）
1. **证据驱动**：每个结论必须附 `file:line` 或脚本/命令输出作为证据，禁止"我觉得/应该"。
2. **原始素材优先**：先读 README、docs/、AGENTS.md、根 Cargo.toml、docs/assessment/ 下的审计报告，再读核心包源码，最后运行脚本验证数字。
3. **区分"宣称"与"交付"**：用门禁 15 幻影交付的视角检查产品宣称是否真实可调用。
4. **用户旅程模拟**：以新用户视角实际走一遍文档/示例，记录 friction 点（如缺少快速开始、API 命名不一致、文档与代码不同步）。
5. **竞品对标**：把 sz-orm 与 SQLx/Diesel/SeaORM 在开发者体验、类型安全、生态、文档、性能、生产验证六个维度打分（1-5）。

# 可运行验证（仓库根目录，Windows Git Bash）
```bash
export PATH="$HOME/.cargo/bin:$PATH"   # cargo 位置
export CARGO_HOME=F:/cargo-home CARGO_TARGET_DIR=F:/cargo-target
# 数字核对
grep -rn '#\[test\]\|#\[tokio::test\]' --include='*.rs' packages/ | wc -l   # 测试函数数
grep -c '^[a-z].*= \[' packages/sz-orm-core/Cargo.toml                    # feature gate 数
ls -d packages/*/ | wc -l                                                  # 包数
# 门禁脚本抽查（验证报告声明）
python scripts/check-doc-consistency.py      # G12
python scripts/check-metrics-real.py        # G18
python scripts/check-unsafe-audit.py         # unsafe 盘点
python scripts/check-panic-safety.py        # 锁中毒基线
# 定向测试（若时间允许）
cargo test -p sz-orm-core --test property --test chaos_pool --offline 2>&1 | tail -5
```
⚠️ 环境注意：
- 本机 rustfmt 1.9.0 与仓库格式基线存在版本漂移，`cargo fmt --all -- --check` 会大面积失败，这是环境问题，不是产品缺陷，**不要据此扣分**。
- pre-push 门禁需要 `SZ_ORM_SKIP_GATE=1` 才能推送（本机工具链问题）。
- 仓库当前可能有其他并行会话的未提交改动，评测时以 `git status` 为准，只读评测不要提交任何东西。

# 交付物
输出一份《sz-orm 产品评测报告》（Markdown），结构：
1. 产品一句话定位（可直接对外使用的 elevator pitch）
2. 目标用户与场景矩阵（谁、什么场景、什么规模）
3. 竞品差异化对比表（六维度打分 + 一句点评）
4. 上手体验评测（含 friction 点清单，按严重度排序）
5. 功能资产盘点（核心/可选/冗余，各举 3-5 例，带证据）
6. 生产就绪度评估（证据驱动，列出支持与反对证据）
7. 主要风险（产品视角，按影响排序）
8. 3-6 个月产品路线图建议（3-5 条，每条含理由、预期收益、衡量指标）
9. 总体评分（1-10）与一句话结论

# 输出要求
全文 3000-5000 字，中文，证据精炼（file:line 或命令输出），不堆砌代码，像一个真正为这款产品负责的 PM 在向董事会汇报。
```

---

## 使用说明

1. 在 AI 编码工具中新建会话（建议工作目录指向本仓库 `E:\vue\test\鲜视达\rust\sz-orm`）。
2. 粘贴上方"提示词全文"。
3. 追加一行：`开始评测。`
4. 评测完成后，AI 会输出《sz-orm 产品评测报告》；如需落盘，可要求它保存到 `docs/assessment/` 下。

## 与既有评测的关系

- `docs/assessment/2026-10-24-deep-evaluation.md`：技术/工程深度评测（偏研发视角）。
- 本提示词产出的是**产品视角评测**（定位、市场、可用性、采纳风险、路线图），两者互补。
- 评测须遵守项目证据铁律：结论附 `file:line` 或脚本输出，且 `docs/assessment/2026-10-24-deep-evaluation.md` 中的环境注意事项（rustfmt 漂移、并行会话未提交改动）同样适用。