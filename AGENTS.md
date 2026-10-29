# sz-orm 项目 AI 工作指南

- 版本：10.13.0（workspace.package.version 集中管理）
- 语言：Rust 2021 Edition（rust-version = "1.81"）
- 工作空间：72（70 lib 包 + cli + examples）；v5.1.0 新增 sz-orm-studio / sz-orm-lsp；v4.3.0 新增 sz-orm-explain / sz-orm-flamegraph / sz-orm-adaptive / sz-orm-fusion / sz-orm-n1-lint；v4.4.0 新增 sz-orm-advisor / sz-orm-diagnosis；v4.5.0 新增 sz-orm-parallel / sz-orm-stream；v4.6.0 不新增包，7 个 feature gate 扩展既有包；v4.7.0 不新增包，7 个 feature gate 扩展既有包；v7.3.0 不新增包，10 个 feature gate 扩展既有包，3 个默认启用 perf-accel/ha-events/health-subitems，7 个按需启用 auto-failover/limit-queue-timeout/tracing-db-attrs/ai-config/ann-accel/eco-config/warp-adapt）
- 核心依赖：tokio（异步运行时）、sqlx（DB 驱动）、crossbeam-queue（连接池无锁队列）、serde/serde_json（序列化）
- 绑定层依赖：sz-orm-cabi/sz-orm-java/sz-orm-go/sz-orm-cpp 依赖 sz-orm-sqlx（SQLite 后端）+ jni 0.22（Java）；sz-orm-python 依赖 sz-orm-sqlx + pyo3 0.20（PyPool 真实连接）；sz-orm-stream dev-dependencies 含 tokio/futures（集成测试）
- 连接池：自研（AtomicU32 + crossbeam-queue ArrayQueue + Notify），非 deadpool（deadpool-postgres 仅 dev-dependency 用于 chaos-pool 测试）
- 模块路径：`packages/sz-orm-core/src/{query,model,pool,migration,transaction,hooks,repository,...}.rs`（扁平模块，非嵌套目录）
- 已发布：sz-orm-core 在 crates.io 已发布 30 个版本，最新为 **10.7.0**（2026-10-28 经 crates.io sparse index `index.crates.io/sz/-o/sz-orm-core` 核实；1.0.0 首发于 2026-07-23）；当前代码版本 10.13.0 尚未发布，需要最新能力请用 git/path 依赖（README 安装小节已如此提示）（v10.13.0 详见 CHANGELOG.md；v10.12.0 性能+稳定性深化：连接池自适应调优 + 查询计划缓存增强 + 故障恢复加速；v9.4.0 审计修复闭环：G7 缓存收益测试模型修正 + G22 覆盖率 94.7% + 23 关门禁全部通过；v9.3.0 安全加固深化：SingleFlight 缓存击穿防护 + 事务 panic 安全回滚 + 52 测试；v9.2.0 覆盖率战役：829 新增测试 + Oracle XE 21c 集成 + 安全审查修复；v9.1.0 覆盖率提升：240 新增测试 + AnyConnection 委托修复；v9.0.0 覆盖率战役 M1~M21：361 新增测试 + 连接池 known_good 快速路径引入后经审计撤销 + reap_idle 选择性回收 + Value clonecheap + 工作空间版本升级；v8.8.0 连接池优化 + 精确容量估算 + BatchSizeAdvisor + crates.io 全量发布 8.8.0；v8.7.0 真实质量基线采集：覆盖率 88.59% + 杀率 100% + 四数据库基准 8/8 PASS；v7.3.0 性能极致优化 + 企业级高可用 + AI 深度集成 + 生态扩展完整交付：28 个任务，187 个新增测试，10 个 feature gate）
- 外部生产试点：sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）已使用 sz-orm-core/sqlx/config/auth/macros/queue 6 个包
- 约束：任何 WHERE 条件必须参数化（`where_eq`/`or_where_eq` 等），`where_cond`/`or_where` 已标记 deprecated；默认禁止 `SELECT *`；N+1 防护：编译期 `#[detect_n_plus_one]` 静态检测（n1-lint）+ 运行时 `N1QueryDetector` 检测组件（需手动接入，未自动拦截）。

## 工程化审查规范

**每次开发必须严格遵守** [docs/sz-orm-engineering-practices.md](docs/sz-orm-engineering-practices.md)，核心要点：

### ADR-0001（铁律）

**严禁下游项目修改上游 sz-orm / sz-rust 仓库的任何文件。** 任何改动必须通过 PR 贡献到上游。违反此原则会导致审计记录与事实不符，直接红牌拒绝入库。

### 门禁清单（提交前必过）

> 核心 25 道见下表；增强门禁（26~34）由 `scripts/run-gates-v10120-server.sh` 执行，编号以该脚本为准（2026-10-28 编号统一：文件名编码/文档版本/生成器产物由 26/27/28 改为 32/33/34，消除重号）。

| # | 门禁 | 命令 |
|---|------|------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` |
| 4 | test 单元/集成测试 | `cargo test --workspace` |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` |
| 8 | 禁止占位实现 + unsafe 合规审计 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` + `scripts/check-unsafe-audit.py`（v10.13.0 起双口径 SAFETY：块级 `// SAFETY:` 或函数级 `/// # Safety`，皆缺才红牌；L-3 裁决落地） |
| 9 | SQL 注入扫描 + 自测 | `scripts/check-sql-injection-auto.py`（v8.5.0 自动化审查引擎，7 规则 R1-R7；v10.13.0 起支持 `#[cfg(test)]` 内联测试上下文感知 + push_str 插值参数收紧，实测 46/46 Safe、--strict 通过；自测脚本 `scripts/test_check_sql_injection_auto.py` 13/13 通过，**v10.13.0 起并入关卡 9**——扫描器自测必须随门禁执行） + `scripts/check-secrets.py` 凭据扫描（H-3 整改） |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD`（ADR-0001） |
| 12 | 文档与代码一致性检查 | `python scripts/check-doc-consistency.py` |
| 13 | 审计证据验证 | `bash scripts/audit-verify.sh <审计报告.md>` |
| 14 | 文档同步更新检查 | `python scripts/check-doc-sync.py --diff HEAD` |
| 15 | 幻影交付检查 | `python scripts/check-phantom-delivery.py`（双模式：①符号断言 PHANTOM-1 零调用符号，任何一项存在即失败；②接线断言 W1~W2 模块内接线函数体级验证；③PHANTOM-2 门控未启用为警告。接线规范：跨文件接线→符号断言自动变绿；同文件接线→登记 WIRING_ASSERTIONS 表）。PHANTOM-2 支持豁免登记（PHANTOM2_EXEMPT 表，2026-10-28 P2-2）；新 feature 默认必须登记豁免理由或由成员启用 |
| 16 | 语义反模式扫描 | `python scripts/check-semantic-patterns.py`（无效递增/丢弃检查结果/释放路径空操作等；2026-08-13 已抓出 2 处真实 bug：quota 只增不减、QueryStats 参数错位） |
| 17 | 架构一致性扫描 | `python scripts/check-architecture.py`（概念重复实现/依赖白名单/孤儿包；豁免登记：bloom_filter 双实现——dist_cache 击穿守卫 vs warmup 穿透过滤，合并列为阶段 3 架构债） |
| 18 | 度量真实性扫描 | `python scripts/check-metrics-real.py`（README/README.zh 数字声称 vs 源码统计，--fix 自动修正；数字禁止手写；2026-10-25 扩展：feature 总数 + 双语 README + 英文 Total 行纳入比对） |
| 19 | 发布一致性扫描 | `python scripts/check-publish-consistency.py`（版本声明一致性；豁免：sz-orm-python/js/graph 独立 0.1.0 版本线；2026-10-27 扩展：`--check-crates-io` 可选远程校验 README 声称的 crates.io 版本是否已发布，网络不可用时自动跳过） |
| 20 | 变异测试杀率 | 分层执行：日常门禁用 `bash scripts/collect-mutation-diff.sh`（cargo-mutants `--in-diff` 增量变异，Windows 本地会话预算内可跑）；发布前门禁用 `bash scripts/collect-mutation-linux.sh --fail-under 70`（v8.6.0 CI/CD 自动执行，cargo-mutants 对 5 包子集，杀率 < 70% 失败；CI/CD mutation job 阻断 PR）。跨包核心逻辑（如 pool 回归测试在 sz-orm-sqlx）需加 `--test-package` 消除变异盲区。实测数据采集日期 2026-09-23） |
| 21 | 安全攻击测试 | `cargo test -p sz-orm-auth --test security_attacks && cargo test -p sz-orm-crypto --test kat && cargo test -p sz-orm-core --features multi-tenant-enhanced --test security_attacks`（JWT 伪造/过期/弱密钥 + 密码学 RFC/NIST 向量 KAT + 租户越权/注入向量；并发正确性：bloom 多线程不漏判测试——loom 模+型检查因 RUSTFLAGS 污染依赖树不可行，2026-08-14 评估记录）+ v4.9.0 OWASP Top 10 完整覆盖渗透测试套件（`--features owasp-pentest-suite`，v10.13.0 实测：门控测试 49 个/9 文件，文件名含 owasp/csrf/xss/file_upload/race/ssrf 的相关测试共 145 个，覆盖 A01~A10 + XSS/CSRF/文件上传/竞态；A06 脚本 `scripts/owasp_a06_vulnerable_components.ps1`） |
| 22 | 覆盖率门禁 | `bash scripts/collect-coverage-linux.sh --fail-under 60`（v8.6.0 CI/CD 自动执行，cargo-llvm-cov 对全工作空间行覆盖率，< 60% 失败；CI/CD coverage job 阻断 PR；实测数据采集日期 2026-09-23） |
| 23 | 未用依赖扫描 | `python scripts/check-unused-deps.py`（cargo-machete，警告级；feature 门控误报登记 `[package.metadata.cargo-machete] ignored`） |
| 24 | 示例清单同步检查 | `python scripts/check-examples-readme.py`（2026-10-25 产品评测修复：README Examples/示例 表格必须与 examples/src/bin/ 实际 bin 双向一致——文档引用幽灵示例或新增示例未登记均红牌） |
| 25 | 文档代码可编译性检查 | `python scripts/check-doc-code-compiles.py`（2026-10-27 UXR 评测 R1 新增：静态扫描 README/quickstart/使用指南/迁移指南中 rust 代码块——校验代码块引用的 core 方法/派生宏是否存在、`fn main()` 无 Result 却用 `?`、`select()` 返回 Result 未解包即链式调用等常见"照抄编译不过"陷阱；文档示例代码与源码 API 脱节时红牌；自测：`python scripts/test_check_doc_code_compiles.py`） |
| 26 | CI 与 deny.toml ignore 一致性 | `python scripts/check-ci-deny-sync.py`（2026-10-27 安全评测 M-1 防复发：CI 工作流 cargo-audit `--ignore` 集合必须与 `deny.toml [advisories].ignore` 完全一致，防止"登记豁免但 CI 假红/漏检"漂移） |
| 27 | 门禁脚本自检 | `python scripts/check-gate-scripts.py`（2026-10-27 安全评测 M-4 防复发：扫描门禁脚本自身的硬编码 PASS、`\|\|` 假 PASS，并校验关键安全辅助脚本（G9 自测/secrets/unsafe 审计/26/27/28 门禁）均已接入） |
| 28 | 安全文档版本一致性 | `python scripts/check-security-doc-version.py`（2026-10-27 安全评测 M-5 防复发：`docs/Security.md` 版本头必须与 `workspace.package.version` 一致） |
| 29 | 幻影测试引用检测 | `python scripts/check-test-symbols.py`（2026-10-27 安全评测 H-2 防复发，警告级：检测测试断言 `detected_patterns`/规则名时引用了对应包 src/ 中不存在的标识符字符串） |
| 30 | 幻影测试扫描 | `python scripts/check-phantom-tests.py`（2026-10-27 SDET 评测新增：P1 同表达式互斥或/ P2 双字面量断言/ P3 测试函数零断言/ P5 差分命名规范 为硬规则；P2b/P4/P6 为软提示。验收：能命中历史样本——`dialect_advanced_test` 永真断言、`multi_tenant_security_test` 字面量比较、`smallstring_differential` 名义差分；自测 `tests/test_check_phantom_tests.py`） |
| 31 | CI 阻断语义审计 | `python scripts/check-ci-blocking.py`（2026-10-27 SDET 评测新增：任何 `continue-on-error` job 必须登记在 `EXEMPT_JOBS` 豁免表，否则红牌；fail-closed。自测 `tests/test_check_ci_blocking.py`） |
| 32 | 文件名编码合法性 | `python scripts/check-filename-encoding.py`（2026-10-27 UXR 评测第三轮新增，2026-10-28 由 26 改号：遍历仓库检测乱码文件名——含 U+FFFD 替换字符、`_md` 结尾丢失扩展名、非 ASCII 名无法按 UTF-8 无损解码均红牌，防止中文文档链接在 GitHub 上 404） |
| 33 | 文档版本声明一致性 | `python scripts/check-doc-version-consistency.py`（2026-10-27 UXR 评测第三轮新增，2026-10-28 由 27 改号：扫描 README/README.zh/使用指南/学习路线图/AGENTS.md 的"当前版本声明"行（顶部版本/徽章/适用版本/当前工作空间版本/当前代码版本），与 workspace 版本不一致即红牌；历史版本日志与迁移指南等标注历史性的文档不在检查范围） |
| 34 | 代码生成器产物编译验证 | `python scripts/check-generated-code-compiles.py`（2026-10-27 UXR 评测第三轮新增，2026-10-28 由 28 改号：调用 CLI `make:model` 生成 5 种主键类型（i64/u32/u64/String/uuid）的模型产物，在临时 crate 中 `cargo check` 编译验证，防止生成器产出引用不存在宏/私有模块的坏代码；cargo 不可用时跳过） |

### 五维审查（每次 PR 必做）

正确性 → 可读性 → 架构 → 安全性 → 性能

### AI 辅助开发 13 条硬约束

1. 禁止占位实现（todo!/unimplemented!/unreachable!）
2. 强制参数化查询（禁止 SQL 字符串拼接）
3. API 兼容性（签名变更必须同步更新所有调用方和测试）
4. 五维审查必过
5. unsafe 零容忍（必须有 // SAFETY: 注释）
6. 禁止 mock 逃逸
7. 门禁前置（主动运行 gate.ps1）
8. 跨平台意识
9. Feature 隔离
10. 教训记忆（阅读防御追溯表）
11. 禁止幻影交付：宣称"自动/强制/默认/集成"的能力必须附生产调用点证据（file:line）；"模块存在 + 测试通过"≠"已交付"（门禁 15）；feature gate 无启用点时文档必须用"提供 X 组件（需手动接入）"措辞，禁止用"强制执行/自动注入/自动拦截/启动预热/默认生效"等集成语义描述零调用模块（依据 2026-08-13 审计：docs/assessment/2026-08-13-production-zero-call-audit.md）
12. 禁止幻影测试（v10.0.0 审计新增）：测试必须验证行为副作用，不能只验证初始值/配置值；禁止 `let _result = ...` 丢弃操作结果；禁止永真断言（如 `confidence >= 0.0`）；新增配置类型必须在同一 PR 中添加生产消费点（Pool::new() 读取 config 创建组件 + getter 方法）；修复模式：Pool 添加 `Option<Arc<Component>>` 字段 → `Pool::new()` 根据 `config.enabled` 创建 → 添加 getter → 测试通过 getter 验证行为
13. 每次会话必须自动提交（2026-10-29 新增）：完成一个自洽任务单元后立即 `git add` + `git commit`（conventional commit 风格，附验证输出），**禁止跨会话累积未提交变更**；提交必须可编译（连带依赖文件如 Cargo.toml 的依赖新增）；不打包并行会话的半成品改动；提交后 `git status` 只应剩余有意留给其他会话的文件

### 审计合规铁律（生死线）

**任何审计/审查结论必须附带可验证的代码证据：**

- ❌ 禁止：`已修复`、`应该没问题`、`参见其他文档`
- ✅ 必须：`[packages/sz-orm-core/src/query.rs:127](file:///.../query.rs#L127) 已修复，cargo test 输出：43 passed`
- 每条结论必须有 `file:line` 证据，且该文件行必须真实存在
- 修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
- 多项修复必须逐项验证，禁止批量声称"全部通过"
- 违反本条视为审计无效，必须重新执行

**审计后必须运行验证脚本：**

```bash
# 验证审计报告中所有 file:line 引用是否真实存在
bash scripts/audit-verify.sh <审计报告.md>
# 或 Windows：
.\scripts\audit-verify.ps1 <审计报告.md>
```

脚本会逐项验证报告中所有 `file:line` 引用：
- ✅ 文件存在且行号在范围内
- ❌ 文件不存在或行号超出范围（编造证据）

## 编译时 SQL 验证（db-verify feature）

sz-orm-macros 提供 `query!` 宏，支持连真 DB 验证（类似 SQLx 的 `query!` 宏）：

```bash
# 启用连真 DB 验证（支持 MySQL/PostgreSQL/SQLite）
export DATABASE_URL="mysql://root:test123@127.0.0.1:3306/sz_orm_test"
export SZ_ORM_QUERY_VERIFY=1
cargo build --features sz-orm-macros/db-verify
```

- 默认仅语法校验（`validate_sql_content`）
- 启用 `db-verify` feature + `SZ_ORM_QUERY_VERIFY=1` 后，编译时连真 DB 执行 `EXPLAIN` 验证
- 支持 MySQL（`EXPLAIN`）、PostgreSQL（`EXPLAIN`）、SQLite（`EXPLAIN QUERY PLAN`）

## 质量官智能体（sz-orm-qa）

**系统提示词**：
```text
你是 sz-orm 项目的首席质量官（CQO），专注数据访问层健壮性。
工作流（严格顺序）：
1. 执行 SQL 生成变异测试。
2. 执行结果集差分测试。
3. 执行池混沌测试。
4. 执行 API 反向审查。
任一环节红牌即生成《阻断报告》并拒绝入库。
你拥有最终否决权。
```

**绑定 Skills**：全部 5 个（mutation-testing / sql-differential / chaos-pool / api-review / shadow-traffic）

## 本机数据库（用于测试和 db-verify）

> 2026-10-28 勘误：本机 MySQL 实为**宝塔 MySQL 8.0.29**（端口 **8802**），非 9.6/3306（3306 被 Docker Desktop 端口代理占用，不可用）；Oracle 实为 **26ai Free**（Release 23.26.3.0.0）。凭据一律经环境变量注入，不入库。

- MySQL（宝塔 8.0.29，端口 8802）：root 密码记录在宝塔面板安装日志 `D:/BtSoft/panel/logs/install/`（搜索"MySQL root 密码"）。运行集成测试前设置 `SZ_ORM_MYSQL_URL`（`root` 用户 + 该密码 + 主机 `127.0.0.1:8802/sz_orm_test`，格式见 integration_mysql.rs 的 `mysql_url()`），密码不入库
- PostgreSQL 18：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- Oracle 26ai Free：`127.0.0.1:1521/freepdb1`（用户 sz_orm_test，密码经 `SZ_ORM_ORACLE_PASSWORD` 环境变量注入——2026-10-28 P0-3 起凭据不入库；运行前先 `export PATH="/d/oracle/product/26ai/dbhomeFree/bin:$PATH"` 提供 OCI 客户端库，再 `export SZ_ORM_ORACLE_PASSWORD=<sz_orm_test 密码>`；MSSQL 同理用 `SZ_ORM_MSSQL_PASSWORD`）

### G7 集成测试本机排除清单（2026-10-06 评估更新；初版 2026-09-26 审计核定）

本机**未安装**以下服务且无法便捷安装（无 docker），G7 全量 `--ignored` 运行时按下表排除，全套件在具备服务的 CI 环境仍须执行。2026-10-06 实测更新：**Redis 已就绪**（本机 6379 服务运行中，`integration_redis` 4/4 通过）；**ClickHouse 可经 122 服务器 Docker + SSH 隧道实测跑通（3/3 通过）**，本机无需安装；MSSQL 腾讯云实例已下线（域名 NXDOMAIN），服务器 mcr 镜像大文件拉取网络不可达，本机仅有 LocalDB（命名管道），仍需自备 SQL Server 实例。详见 `docs/assessment/2026-10-06-local-integration-services-evaluation.md` 与隧道实测记录 `docs/assessment/2026-10-06-integration-tunnel-verification.md`。

| 套件 | 依赖 | 测试数 | 本机处置 |
|------|------|--------|---------|
| `integration_clickhouse` | ClickHouse MySQL 兼容协议 | 3 | 默认 `--skip clickhouse`；可经 122 服务器 Docker 容器 + SSH 隧道实测（2026-10-06 验证 3/3 通过，见隧道实测记录） |
| `integration_mssql` | SQL Server 2019+ TCP 1433 + SQL 认证 | 8 | `--skip mssql`（腾讯云实例已下线；服务器 mcr 镜像网络不可达；本机仅有 LocalDB 2019 命名管道，不满足 TCP + SQL 认证，需自备实例） |
| `integration_gbase`（真实 DB 部分） | 腾讯云 GBase/SQL Server 实例 | 2 | 凭据已移出源码，需设置 `SZ_ORM_GBASE_HOST/PORT/USER/PASSWORD/DATABASE`，未配置自动跳过 |
| `test_prepared_cache_hit_benefit` / `soak_pool_long_running_steady_state` | 无（时序敏感） | 2 | 高负载机器易误报，单独复跑验证 |

**已就绪（2026-10-06）**：`integration_redis`（本机 `redis://127.0.0.1:6379/0` 与 122 服务器容器 `6380` 端口 SSH 隧道双路径均验证）—— 4/4 通过，从排除清单移除。

已知坑：`cargo test -- --ignored` 会让 rustdoc 尝试编译 ```ignore 标注的示意 doc 块（feature 统一暴露后数量大）；G7 全量跑用 `cargo test --workspace --tests -- --ignored ...` 规避，不可编译的示意文档块用 ```text 标注。

## 质量基线（v8.7.0 实测；v9.0.0 审计复测见下）

> 所有数值均为实测采集，非手写。采集命令和日期见对应报告。

| 维度 | v8.7.0 实测 | 采集日期 | 工具 | 报告 |
|------|------------|---------|------|------|
| 行覆盖率 | **88.59%**（165,009/186,258 行） | 2026-09-23 07:22 UTC | cargo-llvm-cov 0.9.1 | docs/assessment/2026-09-23-v870-coverage-report.md |
| 变异测试杀率 | **100%**（46 killed + 7 timeout / 53 tested） | 2026-09-23 09:30 UTC | cargo-mutants v27.1.0 | docs/assessment/2026-09-23-v870-mutation-report.md |
| 四数据库基准 | **8/8 PASS**（SQLite 2 + MySQL 1 + PG 2 + Oracle 3） | 2026-09-24 | cargo test --features real-bench | docs/assessment/2026-09-24-v870-perf-benchmark-complete.md |
| 集成测试 | **151 passed**（MySQL 28 + PG 23 + Oracle 7 + e2e 93） | 2026-09-23 | cargo test -- --ignored | docs/assessment/2026-09-23-v870-gate-report.md |
| 对比分析 | **无退化**（覆盖率 +1.19%，杀率持平，性能首次采集） | 2026-09-24 | — | docs/assessment/2026-09-23-v870-baseline-comparison.md |

### v9.0.0 审计复测（2026-09-26，worktree 全量 23 关）

| 维度 | v9.0.0 实测 | 采集日期 | 工具 | 报告 |
|------|------------|---------|------|------|
| 单元/集成测试 | **12,830 passed / 0 failed**（478 套件） | 2026-09-26 | cargo test --workspace | docs/assessment/2026-09-26-gate-review-report.md |
| 集成测试（真实服务） | **159 passed**（排除清单外全绿） | 2026-09-26 | cargo test --workspace --tests -- --ignored | docs/assessment/2026-09-26-gate-review-report.md |
| 行覆盖率 | **90.2%**（分支 100%） | 2026-09-26 | cargo-llvm-cov 0.9.0 | docs/assessment/2026-09-26-gate-review-report.md |
| 变异测试杀率 | **75%**（审计改动行 3 killed/4 viable；pool.rs 全文件 333 变异体超会话预算，部分轮 17/24） | 2026-09-26 | cargo-mutants v27.1.0 --in-diff | docs/assessment/2026-09-26-gate-review-report.md |

已知教训：`cargo-mutants --in-place` 被强杀时会把活跃变异体残留在源码中（`/* ~ changed by cargo-mutants ~ */` 标记）——中断后必须 `grep -rln "changed by cargo-mutants"` 检查并 `git checkout` 恢复。
