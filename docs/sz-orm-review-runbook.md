# SZ-ORM 审查执行指南（sz-orm-pr-review runbook）

> **执行入口**：23 道门禁逐关执行（`scripts/` 各脚本）+ 可选 AI 评审环节
> **Skill**：`C:\Users\Administrator\.zcode\skills\sz-orm-review\SKILL.md`（ZCode 内 `/sz-orm-review`）
> **参考**：`sz-rust/docs/pr-review-执行指南.md`（2026-08-16 提交 `7c53bc2`，含 AI 评审环节）
> **适用版本**：v4.9.0（命令与门禁编号以仓库 `AGENTS.md` 为准）
> **项目根目录**：`E:\vue\test\鲜视达\rust\sz-orm`

---

## 一、用途

提交/PR 变更集的全量质量审查：**diff 扫描 → 静态检查 → 安全门禁 → 23 道门禁 → 可选 AI 评审**，生成带状态机与严重度模型的汇总报告。任一硬门禁失败即红牌停止（fail-closed）。

## 二、前置条件

| 依赖 | 说明 |
|------|------|
| cargo / python / git | Git Bash 需 `export PATH="$HOME/.cargo/bin:$PATH"`（cargo 默认不在 Git Bash PATH） |
| 外部工具 | `cargo install cargo-audit cargo-deny cargo-llvm-cov cargo-mutants`（G6/G20/G22 需要，缺失时先安装，禁止跳关） |
| 本机数据库 | G7 需要：MySQL 9.6 `root:test123@127.0.0.1:3306/sz_orm_test`、PG 18 `postgres:test123@127.0.0.1:5432/sz_orm_test` |
| AI_API_KEY（可选） | 仅 AI 评审环节需要；OpenAI 兼容 Provider（默认 CSDN `glm_for_coding`，可切快手，见第六节） |

> 无 key 也能跑完整门禁审查；AI 评审无 key 时如实记录 `missing-key`（medium），不静默跳过。

## 三、命令速查

```bash
# 0. 环境准备（Git Bash）
export PATH="$HOME/.cargo/bin:$PATH"

# 1. 快速门禁（gate.ps1，13 关，pre-push 级）
.\scripts\gate.ps1              # PowerShell 全 13 关
.\scripts\gate.ps1 -Fast        # 前 3 关（fmt/check/clippy）
.\scripts\gate.ps1 -SkipTests   # 跳过测试

# 2. 全量 23 道门禁（红牌即停，逐关执行，见第四节命令表）

# 3. 静态审查 + AI 评审（ZCode 方式，推荐）
#    对当前工作区执行完整审查并让 AI 评审 diff + 问题清单：
/sz-orm-review review --ai

# 4. 手动 AI 评审（无 ZCode 环境时，见第六节模板）
export AI_API_KEY=sk-xxx
bash scripts/ai-review-manual.sh  # 或按第六节 curl 模板
```

## 四、全量 23 道门禁与严重度模型

| G# | 门禁 | 命令 | 失败严重度 |
|----|------|------|-----------|
| 1 | 格式检查 | `cargo fmt --all -- --check` | medium |
| 2 | 编译检查 | `cargo check --workspace --all-targets` | **critical**（compile-error） |
| 3 | clippy 严格模式 | `cargo clippy --workspace --all-targets -- -D warnings` | critical / lint-warning → medium |
| 4 | 单元/集成测试 | `cargo test --workspace` | **critical**（test-failure） |
| 5 | 文档构建 | `cargo doc --workspace --no-deps --all-features` | high（doc-error） |
| 6 | 安全审计 | `cargo audit`；`cargo deny check` | **critical**（vulnerability） |
| 7 | 真实服务集成 | `cargo test --workspace -- --ignored` | high（integration，含环境因素） |
| 8 | 禁止占位实现 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages cli examples` | high（placeholder） |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1`（Git Bash：`.sh`） | **critical**（sql-injection） |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` | **critical**（compile-error） |
| 11 | ADR-0001 上游未修改 | `git diff --name-only HEAD`；`scripts/check-upstream-unmodified.ps1` | **critical**（adr-violation，红牌） |
| 12 | 文档与代码一致性 | `python scripts/check-doc-consistency.py` | medium |
| 13 | 审计证据验证 | `bash scripts/audit-verify.sh <审计报告.md>` | high（evidence-invalid） |
| 14 | 文档同步更新 | `python scripts/check-doc-sync.py --diff HEAD` | medium |
| 15 | 幻影交付检查 | `python scripts/check-phantom-delivery.py` | high（phantom-delivery） |
| 16 | 语义反模式扫描 | `python scripts/check-semantic-patterns.py` | high（anti-pattern） |
| 17 | 架构一致性扫描 | `python scripts/check-architecture.py` | high（architecture） |
| 18 | 度量真实性扫描 | `python scripts/check-metrics-real.py`（`--fix` 自动修正） | medium（fake-metrics） |
| 19 | 发布一致性扫描 | `python scripts/check-publish-consistency.py` | high（publish-mismatch） |
| 20 | 变异测试杀率 | `python scripts/check-mutation-coverage.py`（杀率 <70%） | high（mutation-killrate） |
| 21 | 安全攻击测试 | 见 4.1（JWT/密码学/租户 + OWASP 85 测试 + A06） | **critical**（security-test） |
| 22 | 覆盖率门禁 | `python scripts/check-coverage.py`（行覆盖 <60%） | high（coverage） |
| 23 | 未用依赖扫描 | `python scripts/check-unused-deps.py` | low（unused-dep，警告级） |

**阻塞规则**：任何硬门禁失败 → 状态 `failed`，立即停止后续门禁（fail-closed），禁止合入；修复后从失败关重跑。

**阈值档位（2026-08-16 盘点决议，三档并存各有用途）**：

| 指标 | 门禁档（本表） | CI 档（ci.yml） | CQO 严格档（质量官） |
|------|---------------|-----------------|---------------------|
| 变异杀率 | ≥70%（G20） | — | **≥96%**（红牌） |
| 行覆盖率 | ≥60% 关键模块（G22） | ≥70% 全 workspace lib（tarpaulin） | **≥90% 核心映射逻辑**（project_rules） |

> 门禁档 = 提交/合入门槛；CI 档 = GitHub Actions 自动执行；CQO 严格档 = 关键模块变更（auth/crypto/pool/并发）或发布前执行，由 `sz-orm-qa` 智能体判定。

### 4.1 G21 安全攻击测试明细

```bash
cargo test -p sz-orm-auth --test security_attacks
cargo test -p sz-orm-crypto --test kat
cargo test -p sz-orm-core --features multi-tenant-enhanced --test security_attacks
cargo test --workspace --features owasp-pentest-suite    # OWASP A01~A10，85 测试，分布于 12 个包
scripts/owasp_a06_vulnerable_components.ps1
```

### 4.2 增强关卡（条件触发，非默认全量）

| G# | 门禁 | 触发条件 | 命令 | 失败严重度 |
|----|------|---------|------|-----------|
| 24 | 契约测试（T2） | **API 变更时必做** | `cargo test -p sz-orm-core --test contracts` | critical（api-contract） |
| 25 | 密钥/凭据预检 | **发布前必做** | `python scripts/check-secrets.py` | critical（secret-exposed） |
| 26 | 废弃 API 保留期 | 有 `#[deprecated]` 标注时 | `python scripts/check-deprecation-period.py` | high（deprecation-period） |

### 4.3 CQO 质量官工作流（sz-orm-qa 五步，G20/G21 的深化）

```bash
# ① 变异测试（严格档 <96% 红牌）
python scripts/check-mutation-coverage.py --threshold 0.96
# ② 结果集差分测试
cargo test -p sz-orm-core --test plan_cache_differential
cargo test -p sz-orm-core --test simd_differential
cargo test -p sz-orm-core --test smallstring_differential
cargo test -p sz-orm-config --test config_diff_test
# ③ 池混沌测试
cargo test -p sz-orm-core --test chaos --test chaos_pool
# ④ API 反向审查（audit-api-changes 三模式）
scripts/audit-api-changes.ps1               # 默认对比 HEAD~1
scripts/audit-api-changes.ps1 -Base main    # 对比 main 分支
scripts/audit-api-changes.ps1 -Strict       # API 变更但测试未同步 → 非零退出
scripts/verify_bindings.ps1
# ⑤ 影子流量校验（重大性能优化上线时，72h 生产旁路对比）
#    实现参照 .trae/skills/sz-orm-shadow-traffic/SKILL.md，本地无法执行时如实记录"未执行"
```

**CQO 红牌规则**（`.trae/agents/sz-orm-qa.md`，映射到严重度模型）：

| 规则 | 触发 | 严重度 |
|------|------|--------|
| 变异杀率 <96% | 严格档 | high（mutation-killrate） |
| 差分测试结果集不一致 | ②任一 | **critical**（differential-mismatch） |
| 池枯竭时 Panic | ③ | **critical**（pool-panic） |
| 公共 API 缺失 Send+Sync | ④ | high（missing-send-sync） |

### 4.4 CI-only 关卡对照表（本地无法复现，CI 红牌时按此定位）

| CI 关卡 | 位置 | 本地替代 | 说明 |
|---------|------|---------|------|
| fuzz | ci.yml `fuzz` job | 无（需 cargo-fuzz + libFuzzer） | 核心模块短时模糊测试，CI 红牌需在 CI 日志定位 |
| benchmark | ci.yml `benchmark` job | 无 | 性能回归门禁 |
| soak-smoke | ci.yml `soak-smoke` / soak.yml | `docs/soak-toolkit-guide-sz-orm.md` 有本地指南 | 长时浸泡冒烟 |
| semgrep | semgrep.yml | 无（CI 专用） | 语义静态扫描 |
| codeql | codeql.yml | 无（CI 专用） | GitHub 安全分析 |
| semver-check | semver-check.yml | 无（CI 专用） | 语义化版本检查 |
| bindings | bindings.yml | `scripts/verify_bindings.ps1` | FFI 绑定一致性（本地可近似） |
| mq-integration | ci.yml `mq-integration` job | 需消息队列环境 | 消息队列集成 |

> CI 红牌且本地 23 关全绿时：先确认关卡属于本表（fuzz/benchmark/soak/semgrep/codeql），再回 CI 日志定位，不要误判为本地回归。

## 五、状态机与环节

```
scanning → static → security → gates(23 关) → 增强(24-26，条件触发) → ai(可选) → done / failed
```

任一步骤命令失败即 `failed` 并停止（fail-closed），报告记录完整状态流转与退出码。

| 环节 | 检查内容 | 产出 |
|------|---------|------|
| diff 扫描 | `git diff --check`（空白/冲突标记）+ `git diff --name-only` 变更集清单 | 变更集 diff --stat |
| 静态 | G1 fmt / G2 check / G3 clippy / G10 feature 全组合 | 编译与格式问题 |
| 安全 | G6 audit / G8 占位 / G9 SQL 注入 / G21 安全攻击 + OWASP | 安全问题清单 |
| 门禁 | G4~G23 其余关卡（test/doc/ADR/幻影交付/语义/架构/度量/发布/变异/覆盖/未用依赖） | 逐关结果 + 严重度 |
| AI（可选） | AI 评审 diff + 已发现问题清单（见第六节） | AI 评审意见（**不参与阻塞判定**） |

## 六、AI 评审环节（微信《代码审查 Skill 实战》方法论）

主流程收集完 diff + 问题清单后，交给 LLM 做最终评审——对齐 pr-reviewer 的 `ai_reviewer` 设计：**diff（截断 8000 字符）+ 已发现问题清单 → 3-5 个最重要问题（性能/安全/可维护性/并发）+ 具体修改建议 + 1-10 评分，只输出 Markdown**。

### 6.1 方式 A：ZCode 内评审（推荐）

直接让 ZCode 会话执行：`/sz-orm-review review --ai`，agent（即 AI）对 diff + 门禁问题清单输出评审意见，写入报告"补充信息 → AI 评审"章节。

### 6.2 方式 B：手动调用 OpenAI 兼容端点（无 ZCode 环境）

```bash
export AI_API_KEY=sk-xxx
# 默认端点（CSDN，套餐计费，底层 glm-5.2，上限 200k token；勿填 glm-5.1/glm-5.2 以免按普通模型计费）
export AI_BASE_URL=https://ai.csdn.net/api/model/v1
export AI_MODEL=glm_for_coding
# 已验证备用 Provider（快手）：
#   AI_BASE_URL=https://wanqing.streamlakeapi.com/api/gateway/coding/v1
#   AI_MODEL=KAT-Coder-Pro-V2.5

# 生成 diff（截断 8000 字符）
git diff main...HEAD | head -c 8000 > /tmp/pr-diff.txt
git diff --name-only main...HEAD > /tmp/pr-files.txt

# 组装 Prompt 并调用
python - <<'PY'
import json, os, urllib.request
diff = open("/tmp/pr-diff.txt", encoding="utf-8").read()
prompt = f"""你是一个资深 Rust 工程师，请评审以下 PR diff。

要求：
1. 列出 3-5 个最重要的潜在问题（性能 / 安全 / 可维护性 / 并发）
2. 给出具体的修改建议（带代码示例）
3. 整体评分 1-10

PR diff:
```
{diff}
```
只输出 Markdown，不要闲聊。"""
body = json.dumps({"model": os.environ["AI_MODEL"],
                   "messages": [{"role": "user", "content": prompt}],
                   "temperature": 0.2}).encode()
req = urllib.request.Request(os.environ["AI_BASE_URL"] + "/chat/completions",
                             data=body, headers={"Authorization": "Bearer " + os.environ["AI_API_KEY"],
                                                 "Content-Type": "application/json"})
print(json.load(urllib.request.urlopen(req))["choices"][0]["message"]["content"])
PY
```

- **输出位置**：报告"补充信息 → AI 评审"章节
- **阻塞规则**：AI 结论**不影响阻塞判定**（仅供参考，防止 LLM 误判阻塞合入）
- **无 key / 请求失败 / 解析失败**：如实记录 `missing-key` / `ai-request-failed`（medium），不静默

### 6.3 性能优化（微信《Skill 性能优化》第 7 篇方法论）

三招：**并发去等待、缓存去重复、限流不打死**（文章基准 0.033 → 3333 QPS）。应用到本流水线（**注意：门禁顺序不可并发，fail-closed 语义优先**）：

| 第 7 篇措施 | 本流水线应用 |
|------------|-------------|
| 并发 asyncio.gather（互不依赖子任务并行） | ① diff 扫描完成后即**后台启动 AI 评审**，与门禁逐关执行并行，最后合并问题清单；② G6 内部 cargo audit 与 cargo deny 并行；③ G4/G7 调高 cargo/test 并行度（--test-threads 按本机核数） |
| 缓存 3 层（L1 内存 LRU / L2 文件 hash / L3 Redis） | AI 评审结果按 **diff sha256 hash** 缓存到 `~/.cache/sz-orm-review/`（L2 级，重启不丢）；同 diff 二次运行直接复用，CI 重复跑省 LLM 调用与时间；cargo 增量编译天然是 L1 |
| 限流令牌桶 + 队列 + 超时 | AI 调用加超时（wait_for 30s，超时降级记录 `ai-timeout`/medium，不挂死审查）+ 令牌桶（约 10 req/s，防 CSDN/快手端点 429）；单会话串行本就不需要队列 |
| 监控 p50/p95/p99 + 命中率 | 报告结果表已含每关耗时；多次运行后统计关卡耗时 p50/p95 可定位瓶颈关；缓存命中/超时次数记入报告"补充信息" |

### 6.4 Choreography 事件联动（第 8 篇方法论）

审查终态 publish 事件到 `docs/audit/events.jsonl`（JSON Lines 追加，每行 `{ts, event, branch, commit, range, state, gates, report}`）：

| 事件 | 触发 | 订阅者（现状/未来） |
|------|------|-------------------|
| `ReviewCompleted` | 全绿（DONE） | **发布门禁**：`publish-all.ps1` / `publish_crates_io.ps1` 发布前检查最近一次 ReviewCompleted 才允许发布；季度审计；关卡耗时 p50/p95 统计 |
| `GateFailed` | 红牌即停（FAILED） | 阻断报告（现状）、通知（未来） |
| `ReviewCanceled` | 用户中断（CANCELED） | 审计日志（记录已跑关卡，不算失败） |

事件失败静默，不影响审查主流程（钩子是增强，不是主流程）。sz-orm 有完整发布链路（publish-all.ps1 / publish_npm.ps1 / publish_crates_io.ps1），事件联动的首要落地场景就是**发布前门禁**。

## 七、报告说明

报告生成到 `docs/assessment/<日期>-gate-review-report.md`（全绿）或 `docs/assessment/<日期>-gate-block-report.md`（红牌），结构对齐 sz-rust 四段式：

```
# 审查报告（日期，branch，range）
## 状态机        ← 状态流转 + 最终状态 + 阻塞阈值
## 问题清单      ← severity/file/rule/message 逐条（critical/high/medium/low 计数）
## 补充信息      ← 变更集 diff --stat + AI 评审（如有）
## 结论          ← ✅ 通过 / ❌ 阻塞（≥ 阈值问题）/ ❌ 失败（流程中断）
```

**证据铁律**：结论必须附真实存在的 `file:line`（可用 `scripts/audit-verify.sh` 自证）；禁止"已修复/应该没问题"措辞；修复后重跑相关测试并附输出（`cargo test` → `N passed`）。

## 八、常见问题

| 现象 | 原因与处理 |
|------|-----------|
| `cargo: command not found`（Git Bash） | 先 `export PATH="$HOME/.cargo/bin:$PATH"` |
| G7 失败 | 本机 MySQL/PG 未启动；启动后复跑 |
| G20/G22 工具缺失 | `cargo install cargo-mutants / cargo-llvm-cov`；禁止跳关 |
| 报告出现 `missing-key`（medium） | `AI_API_KEY` 未设置或拼写错误；export 后重跑 AI 环节 |
| 报告出现 compile-error（critical） | 工作区存在编译失败（可能是并行开发未完成代码）；修复后重跑 |
| 并行会话期间 | **不要跑门禁**：cargo target 锁互相阻塞，fmt 等修复会踩到他人未提交文件；等对方提交后再执行 |
| **内存不足崩溃**（mmap rmeta failed / STATUS_STACK_OVERFLOW） | 32GB 机器 + 20 核默认并行会爆内存：**全量编译必须 `-j 4`**（2026-08-16 实测） |
| **E0786 invalid metadata / E0463 can't find crate** | target 缓存损坏（多为变异/覆盖率工具异常退出残留）：`cargo clean` 后重跑（2026-08-16 清理 240.6 GiB） |
| **G5/G10 rdkafka-sys 编译失败** | 本机 Windows 环境问题（`--all-features` 触发 sz-orm-mqtt 的 rdkafka）：CI Linux 正常；按环境豁免登记（见第十节） |
| **G7 仅 integration_mssql 失败** | 本机无 SQL Server：按环境豁免登记；其余 DB 集成不受影响 |
| **mutants 源码残留** | cargo-mutants `--in-place` 异常退出会把变异代码留在工作区：跑完检查 `git status`，发现核心文件被改立即 `git checkout -- <文件>`；`mutants.out` 残留目录手动清理 |
| **A06 SBOM 未生成** | cargo-cyclonedx 已装但 0.5.9 静默失败（exit 2 无输出）：待查版本兼容 |

## 十、执行记录与决策

### 2026-08-16 全量审查（main @ bcc1f42）

结果：**18/23 通过**，4 个异常全部为环境/依赖问题，无代码质量问题（详见 `docs/assessment/2026-08-16-full-gate-review-report.md`）。

| 异常 | 性质 | 处置 |
|------|------|------|
| G5/G10 rdkafka-sys | 本机 Windows 环境 | **环境豁免**（CI Linux 验证通过） |
| G6 licenses（xxhash-rust BSL-1.0） | bcc1f42 新引入，真实问题 | **✅ 已解决（2026-08-16 同日）**：按团队决策替换为 twox-hash（XXH64 算法输出一致，改动 2 处代码 + 2 处 Cargo.toml，1669 lib 测试 0 失败） |
| G6 advisories ×14 | 生态现状（feature-gated/dev-only） | **✅ 已解决**：deny.toml 补齐 2 个 pyo3 豁免 + 新建 `.cargo/audit.toml` 登记全部 14 个（参照 0049 先例）；重跑 cargo audit exit 0、cargo deny 四项全 ok |
| G7 MSSQL 集成 | 本机无 SQL Server | **环境豁免** |

### 环境豁免登记规范（2026-08-16 起执行）

环境类失败（非代码问题）豁免条件：① 提供根因证据（如 rdkafka-sys 日志 / 无 MSSQL 服务）；② CI 对应关卡通过作为交叉验证；③ 在阻断报告中显式标注"环境豁免"并登记到本节。代码类失败（G6 类依赖/许可证问题）必须修复或团队决策后登记，不适用环境豁免。

### 本机环境基线（2026-08-16 实测）

- 全量编译必须 `-j 4`（32GB 内存 / 20 核）
- 已安装工具：cargo-audit / cargo-deny / cargo-llvm-cov / cargo-mutants / cargo-cyclonedx
- 数据库在跑：MySQL 9.6（3306）、PG 18（5432）；**无 SQL Server**
- 已知环境死穴：rdkafka-sys（`--all-features` 时触发）、cargo-cyclonedx 0.5.9 静默失败

## 九、与其他门禁的关系

| 门禁 | 触发时机 | 与本指南的关系 |
|------|---------|---------------|
| `scripts/gate.ps1`（13 关） | pre-push / 提交前 | 快速集成门禁；本指南 23 关是其超集 |
| CQO 质量官工作流（五步） | 关键模块变更 | G20/G21 的深化（变异/差分/混沌/API 审查/影子流量） |
| sz-orm-review skill（ZCode） | 任意时刻 | 自动化执行本指南：`/sz-orm-review`（全量）/ `fast` / `gates N-M` / `report <file>` |
| sz-rust pr-review.sh | sz-rust 仓库 | 同方法论的另一实现（`e:\vue\test\鲜视达\rust\sz-rust\scripts\audit\pr-review.sh`），仅可读参照，禁止修改上游 |

**建议流程**：开发中靠 gate.ps1 -Fast 快查；提交后、合入前跑全量 23 关 + AI 评审；API 变更加跑 G24 契约测试、发布前加跑 G25 secrets；关键模块（auth/crypto/pool/并发）变更再跑 CQO 五步（严格档）。

**编译时 SQL 验证（db-verify）**：`query!` 宏支持连真 DB 验证，`export SZ_ORM_QUERY_VERIFY=1` + `cargo build --features sz-orm-macros/db-verify`（详见 AGENTS.md），涉及 SQL 宏改动时可作为 G9 之外的补充验证。
