# 新会话提示词：第五轮全量门禁复跑 sz-orm（v1，2026-10-31）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送，即可启动一次独立的第五轮全量门禁复跑。
>
> **与第四轮复跑（2026-10-30/31 本会话）的差异**：第四轮已闭环 G35/G36 自测缺口、G33 门禁数 36 同步、C# ABI 41/22 同步、路线图报告过时引用修正、G19 语义版本比较、GBK 编码修复，并提交了并行线评测整改与评测证据链入库（`4dfae8f`→`fa68444` 等）。本轮新增：① v10.13.1 已发布 crates.io（69 包，红队渗透测试评测加固版，`docs/assessment/2026-10-31-pentest-review.md`）；② 红队审计交叉引用批量重钉（工作区 84 个未提交文件）；③ G20 全量变异已跑通（11680 变异体，单变异体 66min 超时确认必须 CI 长跑）；④ 磁盘清理（D 盘 565G，cargo fmt --all os error 206 需分包执行）。本轮核心：处置红队审计遗留的未提交文件 + 全量门禁复跑 + 红队评测结论交叉复核。

---

**角色**：你是独立门禁审计员（SDET + 工程化治理双视角），职责是**复核而非信任**：上一轮（2026-10-30/31）声称"全部门禁绿、工作区归零"，且此后又叠加了红队渗透评测、v10.13.1 发布、81+ 份评测报告的交叉引用重钉。你的任务是用可复现的命令重新验证一切，揪出漂移、假绿与新引入的问题。

**任务**：对 `D:\www\test\鲜视达\rust\sz-orm`（v10.13.1，`Cargo.toml:6`）执行第五轮全量门禁复跑。验收标准：**每一条声称都有 `file:line` 或命令输出支撑；门禁全绿；工作区归零；所有修复仍闭环。**

## 一、系统当前状态（声称值，必须自行验证）

- 版本 v10.13.1（红队渗透测试评测加固版）：**已于 2026-11-02 前全量发布 crates.io（69 包 10.13.1，API 逐包核实；sz-orm-js/python 走独立 0.1.0 线，examples 不发布）**
- 工作区 **84 个未提交文件**（+18/-4144 净删除）：40 个 `docs/assessment`（红队审计交叉引用重钉/归档）、10 个 `docs/adr`、约 20 个 `scripts/*oracle*|mirror|container*` 辅助脚本、若干 tests——**本轮第一优先级是处置它们**（判断归属 → 验证 → 提交或明确归档）
- 门禁体系已扩至 **36 道**（G35 文档链接健康、G36 CHANGELOG 版本线一致性，2026-10-30 新增）
- `docs/assessment/` 活跃报告经红队审计重钉，39 份零证据历史报告已机械归档至 `docs/assessment/archive/`（门禁 13 对 `archive/` 豁免）

## 二、必读文档（先读再跑）

1. `AGENTS.md` — 36 道门禁表、13 条硬约束、本机数据库（MySQL 宝塔 8802 勘误）、G7 排除清单
2. `docs/assessment/2026-10-31-pentest-review.md` — 红队渗透评测（QueryBuilder column()/schema_sync 两条新 SQLi 及 P0/P1/P2 加固）
3. `docs/assessment/2026-10-28-architecture-roadmap-review.md` — 架构路线图 + 第四轮复跑执行记录（§11）
4. `docs/assessment/legacy-citation-ledger.md` — 历史引用台账（69.3% 红牌噪声归因与归档处置）
5. `CHANGELOG.md` 顶部 v10.13.1 / v10.13.0 条目
6. `docs/sz-orm-engineering-practices.md` 门禁总表

## 三、第一优先级：处置 84 个未提交文件

- 逐目录判定归属：`docs/assessment` 40 个（红队审计交叉引用重钉——抽 3 份验证引用目标真实存在）、`docs/adr` 10 个（同性质）、`scripts/*oracle*|mirror|container*`（红队/Oracle 集成辅助脚本——判断是"应入库的工具"还是"一次性实验残留"，后者删除并登记理由）
- 验证手段：`git diff --stat` 分类 + 抽样 diff + 跑 `python scripts/check-doc-links.py`（重钉后的引用可能引入死链）+ `bash scripts/audit-verify.sh` 抽验 3 份被重钉的报告
- 验证通过后分组提交（conventional commit，禁止打包无关半成品）；`nul`/`0`/`tempcargo-home2/` 类垃圾直接删除

## 四、全量门禁复跑清单（硬性，全部留输出）

1. `cargo check --workspace --all-targets` / `cargo clippy --workspace --all-targets -- -D warnings`
2. `PYTHONHOME="C:/Program Files/python" cargo test --workspace --lib`（**串行执行，禁止并行 cargo**——第四轮教训：3 个 cargo 进程争锁导致 3549 过 1 failed 的偶发假红）
3. `cargo fmt`：Windows 报 os error 206，必须 `bash scripts/check-fmt-all.sh` 逐包
4. `cargo audit`（CI 口径：`--ignore` 列表从 `deny.toml` 提取，应 0 vulnerabilities；v10.13.1 发布后核对 deny.toml 与 CI ignore 是否仍一致：`python scripts/check-ci-deny-sync.py`）
5. **36 道 Python 门禁逐个跑**（G8-G36 全量，命令见 AGENTS.md 门禁表；重点新增 G35 `check-doc-links.py`、G36 `check-changelog-consistency.py`）
6. `python -m pytest tests/`：应 ≥160 passed
7. C# ABI 门禁 `python scripts/check-csharp-abi-surface.py --root .`：Rust 41 / 头文件 41 / C# 导入 22+（若 SzOrm.cs 有新增封装同步更新）
8. **C# E2E 实机**：`export PATH="/c/Users/Administrator/.dotnet:$PATH" && export CARGO_TARGET_DIR="D:\\rust-target" && dotnet run --project packages/sz-orm-cabi/csharp/SzOrmTest/SzOrmTest.csproj`（dotnet 9.0.100 在 `C:/Users/Administrator/.dotnet`；**先 `cargo build -p sz-orm-cabi`**）。v10.13.1 红队加固后若 cabi 有变更，重点看事务/批量/统计用例
9. 真库集成（本机）：MySQL 用 `export SZ_ORM_MYSQL_URL="mysql://shop:<密码>@127.0.0.1:8802/shop"`（密码在 `D:/www/test/鲜视达/服务器信息.md`，**不入库**）；PG `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`；跑 `integration_mysql -- --ignored`（25+）与 `integration_pg -- --ignored owasp`
10. `bash scripts/audit-verify.sh docs/assessment/2026-10-31-pentest-review.md` + 抽验 3 份被重钉的报告（0 失败为验收线；警告为弱语义校验不阻断）

## 五、上轮修复核对清单（逐条验证仍闭环，不采信"已修复"字样）

| # | 上轮修复 | 本轮核对方式 |
|---|---------|-------------|
| 1 | G35/G36 门禁自测（`TestGate35_36SelfTest`） | `pytest tests/test_gate_scripts.py -q` 全过 |
| 2 | G33 权威门禁数断言 36 | `count_agents_gates() == 36` 且 README/practices/docs-README 无 34/25 残留 |
| 3 | C# ABI 数字 41/22 双语 README 同步 | `check-csharp-abi-surface.py` exit 0 + grep "37 个 C ABI"零命中 |
| 4 | G19 语义版本比较（`_ver_key`） | `--check-crates-io` 重跑：10.7 < 10.13.0 应为警告非红牌 |
| 5 | GBK 编码修复（3 门禁脚本 UTF-8 重配置） | GBK 控制台（默认 cmd）直接运行 3 脚本不抛 UnicodeEncodeError |
| 6 | ProdReadyChecker 配置驱动真实验证 | `cargo test -p sz-orm-core --features prod-ready --lib prod_ready_check` 14 passed |
| 7 | 可观测性接线（metrics_hook + MetricsConn） | `cargo test -p sz-orm-sqlx --test metrics_wiring_test` 通过 |
| 8 | 连接池 Drop 同步快路径 | `cargo test -p sz-orm-core --test chaos_pool` 15+ passed |
| 9 | 凭据不入库（SzOrmTest2026 移除 + LEGACY_AUDIT_PATHS） | `check-secrets.py` 0 命中 + `grep -rn "SzOrmTest2026" packages/ AGENTS.md` 零命中 |
| 10 | audit-verify v4（.ps1 符号断言）与 v3（.sh 弱语义）双入口 | 各自 `--self-test`/自测通过；抽查 1 份活跃报告 0 失败 |

## 六、红队评测交叉复核（本轮新增维度）

红队渗透评测（2026-10-31）声称 P0/P1/P2 全项加固（提交 `5309785f`）。抽验至少 3 条：

1. `QueryBuilder column()` SQLi 加固：找到对应修复 `file:line` + 回归测试名，运行该测试
2. `schema_sync` DDL 加固：同上
3. 429 限流/`already exists` 假失败处理（发布脚本 `dc9c415`）：阅读 `scripts/publish-crates.sh` 相关段并核对 v10.13.1 发布记录

同时验证红队报告自身过 audit-verify（`bash scripts/audit-verify.sh docs/assessment/2026-10-31-pentest-review.md`）。

## 七、输出要求

1. 报告保存为 `docs/assessment/2026-10-31-gates-rerun-report.md`，结构：决策摘要（三行内）→ 门禁结果总表（36 道逐条）→ 遗留处置记录 → 上轮核对表 → 新发现问题（如有）→ 证据索引
2. **证据纪律**：L1 结论必须附真实 `file:line`；L2 必须附本会话真实命令输出；L3 推断必须标注。报告完成后运行 `bash scripts/audit-verify.sh docs/assessment/2026-10-31-gates-rerun-report.md` 自证（0 失败为验收线）并附输出
3. **只读评测 + 收尾提交并行**：发现的并行线遗留按第四轮模式分组提交（验证 → 归属 → conventional commit）；评测本身不修改源码
4. 提交纪律（AGENTS 硬约束 13）：完成一个自洽任务单元立即提交，禁止跨会话累积；提交必须可编译
5. 结尾必须回答：**「如果明天要把 sz-orm 交给一个陌生团队接手生产开发，这份代码库的交接风险清单是什么？」**——按风险从高到低列 3-5 条，每条附证据

## 八、环境坑位速查（第四轮实测教训）

- **并行 cargo 争锁**：绝不并行运行多个 cargo 命令；发现 cargo 进程卡死（`Get-Process cargo`）先 `Stop-Process` 再单发重跑
- **PYTHONHOME**：`sz-orm-python` 测试需 `PYTHONHOME="C:/Program Files/python"`（非标准安装位置，pyo3 找不到 encodings）
- **fmt**：`cargo fmt --all` 在 Windows 报 os error 206，用 `bash scripts/check-fmt-all.sh`
- **MySQL**：宝塔 8.0.29 端口 **8802**（非 3306）；root 密码在宝塔安装日志 `D:/BtSoft/panel/logs/install/`，shop 用户密码在 `D:/www/test/鲜视达/服务器信息.md`
- **dotnet**：`C:/Users/Administrator/.dotnet`，PATH 用 MSYS 格式 `/c/Users/Administrator/.dotnet`
- **git**：本机 git 在 `D:\Git`（部分环境不在 PATH）
- **D 盘空间**：2026-10-31 已清理至 565G 可用；cargo home 凭据可能被清理脚本删除，发布前需重新 `cargo login`
