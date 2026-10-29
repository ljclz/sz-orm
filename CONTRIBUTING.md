# 贡献指南（CONTRIBUTING）

> 感谢关注并贡献 SZ-ORM！本文档是贡献流程的**摘要**，完整规范以 [AGENTS.md](AGENTS.md) 与 [docs/sz-orm-engineering-practices.md](docs/sz-orm-engineering-practices.md) 为准。
>
> 适用版本：SZ-ORM v10.13.0（workspace 72 成员：70 lib 包 + cli + examples）| 更新日期：2026-10-29

---

## 1. ADR-0001 铁律：上游仓库不可直接修改

**严禁下游项目修改上游 sz-orm / sz-rust 仓库的任何文件。** 任何改动必须通过 PR 贡献到上游。

- 违反此原则会导致审计记录与事实不符，**直接红牌拒绝入库**。
- 门禁 11（`git diff --name-only HEAD`）会自动检查上游文件是否被越权修改。
- 本仓库自身的架构级决策记录见 `docs/adr/`（如 [docs/adr/0001-连接池用-AtomicU32-而非-Mutex.md](docs/adr/0001-连接池用-AtomicU32-而非-Mutex.md)）。

完整背景见 [docs/sz-orm-engineering-practices.md](docs/sz-orm-engineering-practices.md)。

## 2. 开发环境要求

| 依赖 | 要求 | 说明 |
|------|------|------|
| Rust | **1.81+**（workspace `rust-version = "1.81"`，Edition 2021） | 版本由根 `Cargo.toml` 的 `workspace.package.version` 统一管理（当前 10.13.0），子包一律 `version.workspace = true`，禁止手写版本号 |
| Python | **3.8+** | 门禁与扫描脚本（`scripts/check-*.py`）依赖 |
| 数据库（可选） | MySQL / PostgreSQL / Oracle / SQLite | 仅集成测试与 `db-verify` 需要；本机参考配置：MySQL 9.6、PostgreSQL 18、Oracle 23ai Free |

**凭据安全**：数据库凭据一律经环境变量注入（如 `SZ_ORM_ORACLE_PASSWORD`、`SZ_ORM_MYSQL_PASSWORD`、`DATABASE_URL` 等，命名约定见 [docs/e2e-env-config.md](docs/e2e-env-config.md)），**严禁写入仓库**——门禁 9 的 `scripts/check-secrets.py` 会扫描提交内容。

## 3. 提交前门禁（34 道）

完整 34 道门禁清单见 [AGENTS.md](AGENTS.md)「门禁清单」一节，CI 侧由 `scripts/run-gates-v10120-server.sh` 等执行增强门禁（26~34）。

**本地最小集（每次提交前必跑）**：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Windows 一键门禁（Linux/macOS 用 ./scripts/gate.sh）
./scripts/gate.ps1
```

涉及真实数据库的集成测试：`cargo test --workspace --tests -- --ignored`（未就绪的服务按 AGENTS.md「G7 集成测试本机排除清单」跳过）。

## 4. 硬约束（摘要）

以下为 AGENTS.md「AI 辅助开发 11 条硬约束」中与本仓库最相关的红线，全文见 [AGENTS.md](AGENTS.md)：

1. **禁止占位实现**：生产代码不得出现 `todo!` / `unimplemented!` / `unreachable!`（门禁 8）。
2. **SQL 必须参数化**：WHERE 条件一律用 `where_eq` / `or_where_eq` 等参数化 API；禁止字符串拼接；`where_cond` / `or_where` 已标记 deprecated；默认禁止 `SELECT *`（门禁 9 SQL 注入扫描）。
3. **unsafe 零容忍**：每个 unsafe 块必须附 `// SAFETY:` 块级注释，或函数附 `/// # Safety` 文档（双口径，见 `scripts/check-unsafe-audit.py`）。
4. **禁止幻影交付**：宣称"自动 / 强制 / 默认 / 集成"的能力必须附生产调用点 `file:line` 证据；"模块存在 + 测试通过" ≠ "已交付"（门禁 15）。
5. **禁止幻影测试**：测试必须验证行为副作用，禁止永真断言、丢弃返回结果、只验证初始值/配置值。
6. **API 兼容性**：签名变更必须同步更新所有调用方与测试。

任何审计/审查结论必须附带可验证的 `file:line` 代码证据（详见 [docs/sz-orm-engineering-practices.md](docs/sz-orm-engineering-practices.md)「审计合规铁律」），修复后必须实际运行 `cargo test` 并附输出。

## 5. PR 流程

1. **Fork** 本仓库，`git clone` 你的 fork 并配置 upstream。
2. **建分支**：`feat/<短描述>`、`fix/...`、`docs/...`、`refactor/...`。
3. **开发 + 测试**：新公共函数至少一个测试；bug 修复必须附带修复前失败的回归测试；重大变更（新包 / 破坏性 API / 新 feature gate / 架构决策）请先走 RFC 流程（模板见 [docs/rfc/template.md](docs/rfc/template.md)）。
4. **门禁全绿**：跑完第 3 节本地最小集及相关脚本门禁。
5. **提交 PR**：使用 [.github/pull_request_template.md](.github/pull_request_template.md) 模板填写变更摘要、关联 Issue、测试计划；Bug 请用 [.github/ISSUE_TEMPLATE](.github/ISSUE_TEMPLATE) 模板先行建 Issue。
6. **五维审查**（每个 PR 必做，逐项过，不能只依赖自动化门禁）：

   **正确性 → 可读性 → 架构 → 安全性 → 性能**

7. CI 全部门禁通过且至少一名维护者批准后合并。

## 6. 行为准则（简条）

- **尊重**：保持友善、耐心与包容，认真对待每一份贡献。
- **专业**：讨论对事不对人——**指错不指人**，批评代码不批评作者。
- **无骚扰**：人身攻击、贬低、骚扰性言论零容忍，维护者有权移除并处置。
- **讲证据**：技术分歧以事实与可验证证据（`file:line`、测试输出、基准数据）为裁决依据。

完整社区规范可参考 [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct)。

**安全漏洞**：请勿公开提 Issue，按 [SECURITY.md](SECURITY.md) 的负责任披露流程报告。

---

## English Summary

- **ADR-0001 (iron rule)**: Never modify upstream sz-orm / sz-rust files from downstream projects — all changes must land upstream via PR. Enforced by gate 11. See [docs/sz-orm-engineering-practices.md](docs/sz-orm-engineering-practices.md).
- **Toolchain**: Rust 1.81+ (edition 2021, workspace-managed version 10.13.0), Python 3.8+ for gate scripts. DB credentials go through environment variables (`SZ_ORM_ORACLE_PASSWORD` etc.) — never commit secrets.
- **Gates**: 34 pre-merge gates (full table in [AGENTS.md](AGENTS.md)). Local minimum: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, plus `./scripts/gate.ps1` (Windows) / `./scripts/gate.sh` (Linux/macOS).
- **Hard constraints**: no `todo!`/`unimplemented!`; parameterized SQL only (`where_eq`...); `unsafe` requires `// SAFETY:` comments; no phantom delivery (claims of "automatic/default integration" need `file:line` production call-site evidence) and no phantom tests (tests must assert behavioral side effects).
- **PR flow**: fork → branch → gates green → PR (use the template) → five-dimensional review (correctness / readability / architecture / security / performance).
- **Conduct**: be respectful and professional, criticize code not people, zero tolerance for harassment. Report security vulnerabilities privately per [SECURITY.md](SECURITY.md).

The detailed guide is written in Chinese above; for the full engineering rules read [AGENTS.md](AGENTS.md).

## 参考文档

- [AGENTS.md](AGENTS.md) —— 34 道门禁清单与硬约束全文
- [docs/sz-orm-engineering-practices.md](docs/sz-orm-engineering-practices.md) —— 工程化审查规范（ADR-0001 / 五维审查 / 审计合规铁律）
- [docs/e2e-env-config.md](docs/e2e-env-config.md) —— 集成测试环境变量约定
- [docs/rfc/](docs/rfc/) / [docs/adr/](docs/adr/) —— RFC 与架构决策记录
- [SECURITY.md](SECURITY.md) —— 安全漏洞披露流程
- [CHANGELOG.md](CHANGELOG.md) —— 版本历史

感谢你为 SZ-ORM 做出贡献！
