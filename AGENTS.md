# sz-orm 项目 AI 工作指南

- 版本：4.7.0（workspace.package.version 集中管理）
- 语言：Rust 2021 Edition（rust-version = "1.81"）
- 工作空间：60 个成员（58 个 lib 包 + cli + examples；v4.3.0 新增 sz-orm-explain / sz-orm-flamegraph / sz-orm-adaptive / sz-orm-fusion / sz-orm-n1-lint；v4.4.0 新增 sz-orm-advisor / sz-orm-diagnosis；v4.5.0 新增 sz-orm-parallel / sz-orm-stream；v4.6.0 不新增包，7 个 feature gate 扩展既有包；v4.7.0 不新增包，7 个 feature gate 扩展既有包）
- 核心依赖：tokio（异步运行时）、sqlx（DB 驱动）、crossbeam-queue（连接池无锁队列）、serde/serde_json（序列化）
- 连接池：自研（AtomicU32 + crossbeam-queue ArrayQueue + Notify），非 deadpool（deadpool-postgres 仅 dev-dependency 用于 chaos-pool 测试）
- 模块路径：`packages/sz-orm-core/src/{query,model,pool,migration,transaction,hooks,repository,...}.rs`（扁平模块，非嵌套目录）
- 已发布：sz-orm-core 1.0.0 已发布到 crates.io（2026-07-23），当前代码版本 4.7.0（v4.7.0 7 项需求全部完成，8 个里程碑 M0~M7 通过）
- 外部生产试点：sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）已使用 sz-orm-core/sqlx/config/auth/macros/queue 6 个包
- 约束：任何 WHERE 条件必须参数化（`where_eq`/`or_where_eq` 等），`where_cond`/`or_where` 已标记 deprecated；默认禁止 `SELECT *`；N+1 防护：编译期 `#[detect_n_plus_one]` 静态检测（n1-lint）+ 运行时 `N1QueryDetector` 检测组件（需手动接入，未自动拦截）。

## 工程化审查规范

**每次开发必须严格遵守** [docs/sz-orm-engineering-practices.md](docs/sz-orm-engineering-practices.md)，核心要点：

### ADR-0001（铁律）

**严禁下游项目修改上游 sz-orm / sz-rust 仓库的任何文件。** 任何改动必须通过 PR 贡献到上游。违反此原则会导致审计记录与事实不符，直接红牌拒绝入库。

### 21 道门禁（提交前必过）

| # | 门禁 | 命令 |
|---|------|------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` |
| 4 | test 单元/集成测试 | `cargo test --workspace` |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` |
| 8 | 禁止占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD`（ADR-0001） |
| 12 | 文档与代码一致性检查 | `python scripts/check-doc-consistency.py` |
| 13 | 审计证据验证 | `bash scripts/audit-verify.sh <审计报告.md>` |
| 14 | 文档同步更新检查 | `python scripts/check-doc-sync.py --diff HEAD` |
| 15 | 幻影交付检查 | `python scripts/check-phantom-delivery.py`（双模式：①符号断言 PHANTOM-1 零调用符号，任何一项存在即失败；②接线断言 W1~W2 模块内接线函数体级验证；③PHANTOM-2 门控未启用为警告。接线规范：跨文件接线→符号断言自动变绿；同文件接线→登记 WIRING_ASSERTIONS 表） |
| 16 | 语义反模式扫描 | `python scripts/check-semantic-patterns.py`（无效递增/丢弃检查结果/释放路径空操作等；2026-08-13 已抓出 2 处真实 bug：quota 只增不减、QueryStats 参数错位） |
| 17 | 架构一致性扫描 | `python scripts/check-architecture.py`（概念重复实现/依赖白名单/孤儿包；豁免登记：bloom_filter 双实现——dist_cache 击穿守卫 vs warmup 穿透过滤，合并列为阶段 3 架构债） |
| 18 | 度量真实性扫描 | `python scripts/check-metrics-real.py`（README 数字声称 vs 源码统计，--fix 自动修正；数字禁止手写） |
| 19 | 发布一致性扫描 | `python scripts/check-publish-consistency.py`（版本声明一致性；豁免：sz-orm-python/js/graph 独立 0.1.0 版本线） |
| 20 | 变异测试杀率 | `python scripts/check-mutation-coverage.py`（cargo-mutants 对关键模块子集，杀率 < 70% 失败；2026-08-14 首跑 100%：22/22 变异体被杀） |
| 21 | 安全攻击测试 | `cargo test -p sz-orm-auth --test security_attacks && cargo test -p sz-orm-crypto --test kat && cargo test -p sz-orm-core --features multi-tenant-enhanced --test security_attacks`（JWT 伪造/过期/弱密钥 + 密码学 RFC/NIST 向量 KAT + 租户越权/注入向量；并发正确性：bloom 多线程不漏判测试——loom 模型检查因 RUSTFLAGS 污染依赖树不可行，2026-08-14 评估记录） |

### 五维审查（每次 PR 必做）

正确性 → 可读性 → 架构 → 安全性 → 性能

### AI 辅助开发 11 条硬约束

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

- MySQL 9.6：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`
- PostgreSQL 18：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- Oracle 23ai Free：`127.0.0.1:1521/freepdb1`（用户 sys，密码 test123，Sysdba 权限）
