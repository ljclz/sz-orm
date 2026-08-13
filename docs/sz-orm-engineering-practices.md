# SZ-ORM 工程化实践规范

> **目标项目**：SZ-ORM（鲜视达 ORM 框架，60 workspace 包，6,794+ 测试（2026-08-12 实测））
> **项目版本**：v4.7.0
> **文档用途**：锁定已有工程质量，防止后续修改引入退化
> **维护规则**：任何修改 CI/CD 或新增门禁的 PR 必须同步更新本文档
> **文档版本**：v3.1（2026-08-01）

---

## 目录

1. [核心原则（ADR-0001）](#0-核心原则adr-0001)
2. [标准 7 道门禁（已实现）](#1-标准-7-道门禁已实现)
3. [SZ-ORM 特殊强化门禁（新增）](#2-sz-orm-特殊强化门禁新增)
4. [五维审查增强](#3-五维审查增强)
5. [测试金字塔](#4-测试金字塔)
6. [CI/CD 工作流约束](#5-cicd-工作流约束)
7. [附录：SZ-ORM 教训 → 防御追溯表](#6-附录sz-orm-教训--防御追溯表)

---

## 0. 核心原则（ADR-0001）

**ADR-0001：严禁修改上游 sz-rust / sz-orm 仓库的任何文件。**

sz-orm 作为上游基础库，其代码由独立团队维护。下游项目（如 sz-pay）通过 Cargo 依赖引用 sz-orm，**不得**直接修改 `target/` 或 vendored 源码中的 sz-orm 文件。任何需要的改动必须通过以下途径：

1. **提交 PR 到 sz-orm 上游**：功能增强、Bug 修复走正常贡献流程
2. **使用 sz-orm 提供的扩展点**：trait 实现、hook、自定义 Dialect
3. **Fork + patch**：仅在紧急生产修复时，且必须同步向上游提交 PR

**违反此原则的后果**：
- 下游项目与上游版本脱节，无法享受安全补丁和功能更新
- 审计记录与事实不符（Phase 0/1/2/4 检查点均要求"上游仓库未修改"）
- 多人协作时产生难以追踪的分歧

**例外**：sz-orm 自身仓库的开发当然可以修改 sz-orm 文件。此约束针对的是**使用 sz-orm 的下游项目**。

---

## 1. 标准 7 道门禁（已实现）

以下门禁已完整实现在 CI 配置中，任何提交/PR 必须通过全部门禁。

| # | 门禁 | CI Job 名 | 命令 | 状态 |
|---|------|-----------|------|------|
| 1 | fmt 格式检查 | `lint` | `cargo fmt --all -- --check` | ✅ 已有 |
| 2 | check 编译检查 | `build`（3OS×2rust） | `cargo check --workspace --all-targets` | ✅ 已有 |
| 3 | clippy 静态分析 | `lint` | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 已有 |
| 4 | test 单元/集成测试 | `test` | `cargo test --workspace` | ✅ 已有 |
| 5 | doc 文档构建 | `build` 内包含 | `cargo doc --workspace --no-deps --all-features` | ✅ 已有（在 build job 中） |
| 6 | audit 安全审计 | `security`（security.yml） | `cargo audit` + `cargo deny check` | ✅ 已有 |
| 7 | integration 真实服务集成 | `integration`（integration.yml） | `cargo test --workspace -- --ignored` + docker services | ✅ 已有 |

### 1.1 fmt — 代码格式检查

- **CI Job**: `lint`
- **命令**: `cargo fmt --all -- --check`
- **阻断**: 格式不一致直接 CI 失败
- **本地修复**: `cargo fmt --all`

### 1.2 check — 工作空间编译验证

- **CI Job**: `build`（矩阵: ubuntu / windows / macos × stable / beta）
- **命令**: `cargo build --workspace --all-targets --verbose`
- **环境变量**: `RUSTFLAGS: "-D warnings"` — 零警告编译
- **注意**: 3 操作系统 × 2 Rust 版本共 6 种组合全部通过才放行

### 1.3 clippy — 严格静态分析

- **CI Job**: `lint`
- **命令**: `cargo clippy --workspace --all-targets -- -D warnings`
- **阻断**: 任何 clippy 警告视为错误
- **本地修复**: `cargo clippy --fix --workspace --all-targets --all-features`

### 1.4 test — 工作空间测试

- **CI Job**: `test`
- **命令**: `cargo test --workspace --verbose`
- **依赖**: `needs: [lint, build]`— 格式和编译通过后才运行
- **额外**: 同时运行 SQLite 集成测试 `cargo test --package sz-orm-core --test integration_sqlite`

### 1.5 doc — 文档构建

- **CI Job**: 内嵌在 `build` job 中
- **命令**: `cargo doc --workspace --no-deps --all-features`
- **RUSTDOCFLAGS**: `-D warnings`（在本地 gate.ps1 中设置）
- **阻断**: doc 链接断裂或 doc 警告视为错误
- **注意**: 与 build 同一 job，不在单独 job 运行

### 1.6 audit — 安全审计

- **CI Workflow**: `security.yml`（独立 workflow）
- **命令**:
  - `cargo audit` — 漏洞公告扫描（已知忽略项见 `deny.toml`）
  - `cargo deny check advisories` — 安全公告检查
  - `cargo deny check bans` — 依赖禁用与重复检测
  - `cargo deny check licenses` — 许可证合规
  - `cargo deny check sources` — 依赖来源限制
- **阻断**: 任何 `deny` 级别的检查失败阻断合入

### 1.7 integration — 真实服务集成测试

- **CI Workflow**: `integration.yml`（独立 workflow）
- **依赖服务**: MySQL 9.6 / PostgreSQL 18 / RabbitMQ 3.13 / Mosquitto 2.0 / MinIO
- **命令**: `cargo test --package <pkg> --features <feat> -- --ignored --nocapture`
- **覆盖包**: sz-orm-core（MySQL+PG）、sz-orm-sqlx（real DB）、sz-orm-mqtt（real-broker）、sz-orm-queue（rabbitmq）、sz-orm-storage（s3-sdk）、sz-orm-websocket（server）
- **触发**: push/PR + 每日定时 02:00（Asia/Shanghai） + 手动触发

### 1.8 补充：额外 CI Job

CI 配置中还包含以下扩展 Job：

| Job | 触发条件 | 说明 |
|-----|---------|------|
| `real-features-compile` | 每次 push/PR | 验证 real-* feature 编译（postgis/timeseries/search） |
| `benchmark` | push 到 main | criterion 性能基准测试（warm-up 1s, measurement 3s, 30 samples） |
| `soak-smoke` | 每次 push/PR | 10 秒 Soak 冒烟测试，验证框架不退化 |
| `coverage` | push/PR | cargo-tarpaulin 覆盖率报告上传 Codecov |
| `soak`（soak.yml） | 每周日 UTC 00:00 | 长时 6h Soak 测试（检测内存泄漏/句柄泄漏/性能退化） |

---

## 2. SZ-ORM 特殊强化门禁（新增）

以下三道门禁基于 SZ-ORM 审查报告中的血泪教训制定，必须补充到 gate.ps1 和 CI 中。

### 门禁 8：禁止占位实现检查

| 属性 | 值 |
|------|-----|
| **教训来源** | SZ-ORM V-1~V-7 共 7 个虚假/伪实现 |
| **命令** | PowerShell 脚本扫描 |
| **CI Job 名** | `check-placeholders`（新增） |
| **状态** | ✅ 已通过（0 处占位实现） |

**扫描脚本**（PowerShell）：

```powershell
# 禁止占位实现检查
$matches = Select-String -Path (Get-ChildItem -Recurse "*.rs" -Exclude "*target*").FullName -Pattern '\b(todo!|unimplemented!|unreachable!)\b'
if ($matches) {
    Write-Warning "发现占位实现，共 $($matches.Count) 处"
    $matches | ForEach-Object { Write-Host "  $($_.Path):$($_.LineNumber) — $($_.Line.Trim())" }
    exit 8
}
Write-Host "[OK] 无占位实现" -ForegroundColor Green
```

**Linux 版**（gate.sh）：

```bash
matches=$(grep -rn '\btodo!\|\bunimplemented!\|\bunreachable!' --include='*.rs' --exclude-dir=target .)
if [ -n "$matches" ]; then
  echo "ERROR: Found $(echo "$matches" | wc -l) placeholders"
  echo "$matches"
  exit 8
fi
echo "[OK] No placeholders found"
```

**说明**：
- 扫描工作空间中所有 `*.rs` 文件（排除 `target/` 目录）
- 匹配模式：`todo!()`、`unimplemented!()`、`unreachable!()`
- 不允许任何占位实现进入 main 分支
- 开发阶段允许存在，合入前必须清除

### 门禁 9：SQL 注入扫描

| 属性 | 值 |
|------|-----|
| **教训来源** | SZ-ORM C-1~C-6 共 6 个 Critical SQL 注入 |
| **命令** | PowerShell 脚本扫描 SQL 拼接模式 |
| **CI Job 名** | `check-sql-injection`（新增） |
| **状态** | ✅ 已通过（8 处漏洞已修复） |

**扫描脚本**（PowerShell）：

```powershell
# SQL 注入扫描：检测 SQL 拼接模式
$sqlPatterns = @(
    @{ Name = "format! SQL 拼接"; Pattern = 'format!\s*\(\s*"[^"]*(?:SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|WHERE)[^"]*".*\{' },
    @{ Name = "字符串插值 SQL"; Pattern = '"(?:[^"]*(?:SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|WHERE)[^"]*)\$\{?\w+\}?"' },
    @{ Name = "SQL 字符串拼接"; Pattern = '\.to_string\(\s*\)\s*\+\s*"' },
    @{ Name = "raw SQL 参数插值"; Pattern = '\.(?:execute|query|raw)\s*\(\s*format!' }
)

$foundIssues = $false
foreach ($pattern in $sqlPatterns) {
    $matches = Select-String -Path (Get-ChildItem -Recurse "*.rs" -Exclude "*target*").FullName -Pattern $pattern.Pattern
    if ($matches) {
        Write-Warning "[$($pattern.Name)] 发现 $($matches.Count) 处"
        $matches | ForEach-Object { Write-Host "  $($_.Path):$($_.LineNumber)" }
        $foundIssues = $true
    }
}

if ($foundIssues) {
    Write-Host "[FAIL] SQL 注入扫描未通过，请使用参数化查询替代拼接" -ForegroundColor Red
    exit 9
}
Write-Host "[OK] SQL 注入扫描通过" -ForegroundColor Green
```

**说明**：
- 扫描 `format!` 宏中嵌入 SQL 关键字的字符串拼接
- 扫描字符串插值 SQL（`${var}` 或 `{var}` 在 SQL 字符串中）
- 扫描 `.to_string() + "SQL"` 模式的拼接
- 扫描 `.execute()/.query()/.raw()` 传入 `format!` 的结果
- 所有 SQL 必须使用参数化查询（`?` 或 `$N` 占位符）

### 门禁 10：Feature Flag 全组合编译

| 属性 | 值 |
|------|-----|
| **教训来源** | SZ-ORM V-4 real-* feature 数月未在 CI 编译 |
| **命令** | `cargo check --workspace --all-targets --all-features` |
| **CI Job 名** | `check-all-features`（新增） |
| **状态** | ✅ 已通过（编译零错误） |

**命令**：

```bash
cargo check --workspace --all-targets --all-features
```

**说明**：
- gate.ps1 关卡 2 已包含 `--all-features`，但 CI `build` job 未使用
- 需在 CI `build` job 中将 `cargo build --workspace --all-targets` 改为包含 `--all-features`
- 确保所有 feature 组合（包括 real-*、mock-*、default）都能正确编译
- 防止 feature 隔离失败导致伪实现逃逸

### 门禁 11：上游仓库未修改检查（ADR-0001）

| 属性 | 值 |
|------|-----|
| **教训来源** | 下游项目直接修改 sz-orm 源码导致审计记录与事实不符 |
| **命令** | `git diff --name-only HEAD` + 文件完整性校验 |
| **CI Job 名** | `check-upstream-unmodified`（新增） |
| **状态** | ✅ 已通过 |

**检查脚本**（PowerShell）：

```powershell
# ADR-0001：上游仓库未修改检查
# 在下游项目中运行，确认 sz-orm / sz-rust 相关文件未被修改
$upstreamPatterns = @("sz-orm-core", "sz-orm-auth", "sz-orm-config", "sz-orm-macros", "sz-rust-core")
$modified = git diff --name-only HEAD

$violations = @()
foreach ($pattern in $upstreamPatterns) {
    foreach ($file in $modified) {
        if ($file -like "*$pattern*") {
            $violations += $file
        }
    }
}

if ($violations.Count -gt 0) {
    Write-Warning "[ADR-0001 VIOLATION] 上游仓库文件被修改："
    $violations | ForEach-Object { Write-Host "  $_" }
    Write-Host "请通过 PR 贡献到上游，而非直接修改。" -ForegroundColor Red
    exit 11
}
Write-Host "[OK] ADR-0001 通过：上游仓库未修改" -ForegroundColor Green
```

### 门禁 12：文档与代码一致性检查

| 属性 | 值 |
|------|-----|
| **教训来源** | 审计记录写"43 包"实际 41 包；版本号不一致 |
| **命令** | `python scripts/check-doc-consistency.py` |
| **CI Job 名** | `check-doc-consistency`（新增） |
| **状态** | ✅ 已通过 |

**校验内容**：
- AGENTS.md 中的版本号、包数量
- sz-orm-engineering-practices.md 中的项目版本、workspace 包数量
- 所有数据必须与实际 Cargo.toml 一致

**自动修复**：
```bash
# 自动修复文档中的不一致数据
python scripts/check-doc-consistency.py --fix
```

### 门禁 13：审计证据验证（审计合规硬约束）

| 属性 | 值 |
|------|-----|
| **教训来源** | 审计报告写"已修复"但实际未改；file:line 证据编造 |
| **命令** | `bash scripts/audit-verify.sh <审计报告.md>` |
| **CI Job 名** | `audit-verify`（新增） |
| **状态** | ✅ 已通过 |

**验证内容**：
- 审计报告中所有 `file:line` 引用是否真实存在
- 行号是否在文件实际行数范围内
- 禁止编造证据

**铁律**：
- ❌ 禁止：`已修复`、`应该没问题`、`参见其他文档`
- ✅ 必须：`[packages/sz-orm-core/src/query.rs:127] 已修复，cargo test 输出：43 passed`
- 违反本条视为审计无效，必须重新执行

### 门禁 14：文档同步更新检查

| 属性 | 值 |
|------|-----|
| **教训来源** | 代码变更未同步更新文档，导致文档与代码脱节（版本号、feature、API、连接池、方言、DSL、缓存、迁移指南等） |
| **命令** | `python scripts/check-doc-sync.py --diff HEAD` |
| **CI Job 名** | `check-doc-sync`（新增） |
| **状态** | ✅ 已实现 |

**检查逻辑**：
1. 解析 `git diff` 获取本次变更文件清单与新增行
2. 按 10 类代码变更 → 受影响文档映射规则（`scripts/doc-sync-rules.yaml`）匹配
3. 对每个受影响文档，检查是否在本次 diff 中被修改
4. 未修改则退出码 1 阻断提交；全部已修改或已跳过则退出码 0

**10 类映射规则**：

| # | 规则名 | 代码变更 | 受影响文档 |
|---|--------|---------|-----------|
| 1 | cargo-version | Cargo.toml 版本号变更 | README.md / AGENTS.md / engineering-practices.md |
| 2 | cargo-feature | Cargo.toml feature 列表变更 | engineering-practices.md（feature 矩阵） |
| 3 | new-pub-api | 新增 pub API（pub fn/struct/enum/trait/mod/const/static/type/use） | 对比分析文档 / design.md |
| 4 | pool-rs | pool.rs 变更 | 对比分析文档 / engineering-practices.md |
| 5 | dialect-rs | dialect.rs 变更 | 对比分析文档 / engineering-practices.md |
| 6 | typed-ast-rs | typed_ast.rs 变更 | 对比分析文档 / engineering-practices.md |
| 7 | l2-cache-rs | l2_cache.rs / l1_cache.rs 变更 | 对比分析文档 / engineering-practices.md |
| 8 | migration | 迁移模块变更 | engineering-practices.md / 迁移指南 |
| 9 | cargo-new-package | Cargo.toml workspace members 新增包 | AGENTS.md / engineering-practices.md |
| 10 | cargo-dependency | Cargo.toml 依赖变更 | AGENTS.md / engineering-practices.md |

**跳过标记**：在受影响文档任意位置添加 `# doc-sync-skip` 行可跳过该文档的检查（用于紧急修复或文档尚未创建的场景）。

**CI 集成**：在 `.github/workflows/ci.yml` 中新增 `check-doc-sync` job，PR 触发时执行文档同步检查，退出码非 0 阻断 PR 合入。

**单元测试**：`tests/test_check_doc_sync.py` 覆盖规则加载、diff 解析、跳过标记、退出码、端到端三种场景（未同步/已同步/跳过），36 个测试全通过。

---

## 3. 五维审查增强

### 3.1 审查维度

每次合入 PR 前必须进行五维审查，覆盖以下维度：

| 维度 | 审查要点 | SZ-ORM 对应教训 |
|------|---------|----------------|
| **正确性** | 逻辑正确、边界处理、错误处理、并发安全 | 锁 panic（13 处 expect）+ API 签名变更未同步 |
| **可读性** | 命名清晰、注释恰当、代码结构合理 | — |
| **架构** | 模块边界、依赖方向、feature 隔离、API 设计 | 名实不符（S-1~S-8）、夸大对比（D-1~D-7） |
| **安全性** | SQL 注入、unsafe 审计、输入验证、权限 | SQL 注入（C-1~C-6）+ 标识符注入（phinx_migration） |
| **性能** | 内存分配、锁竞争、序列化开销、连接池 | — |

### 3.2 AI 生成代码特有检查

对于 AI 生成的代码变更，增加以下检查项：

| 检查项 | 说明 |
|--------|------|
| `unsafe` 代码审计 | 检查所有 `unsafe` 块的安全性、不变式维护、内存安全，必须有 `// SAFETY:` 注释 |
| 所有权泄漏检查 | 检查 `Box::leak`、`ManuallyDrop`、`forget` 使用场景；检查 `Arc` 循环引用 |
| 锁使用审计 | 检查 `Mutex`/`RwLock` 范围、死锁风险、是否为 `parking_lot`（无 poison） |
| 虚假实现检测 | 检查是否有 `todo!()`、空实现、mock 实现逃逸到 main |
| API 名实一致性 | 检查函数名是否与实现行为一致（对比 S-1~S-8） |
| API 签名变更传播 | 修改返回类型（如 `T` → `Result<T, E>`）时，必须同步更新所有调用方和测试 |
| 跨平台兼容性 | 检查平台特定代码是否有条件编译保护 |
| `as` 类型转换 | 检查 `as i32` / `as u32` / `as usize` 等缩窄转换是否可能溢出 |
| SQL 注入检查 | 检查是否有 `format!` 拼接 SQL，必须使用 `where_eq` 等参数化方法 |
| 标识符注入检查 | 检查表名/列名/约束名是否经过 `validate_identifier` 校验 |

### 3.3 审查清单脚本

使用现有 `scripts/audit-api-changes.ps1` 进行 API 变更审计：

```powershell
# 对比 HEAD~1 的 API 变更
./scripts/audit-api-changes.ps1

# 对比 main 分支
./scripts/audit-api-changes.ps1 -Base main

# 严格模式（API 变更但测试未同步时退出码非零）
./scripts/audit-api-changes.ps1 -Strict
```

---

## 4. 测试金字塔

SZ-ORM 当前测试数据：

| 层级 | 数量 | 说明 |
|------|------|------|
| **T1 — 单元测试** | 4500+ | 核心模块独立测试（Value、DbType、Dialect、Model trait、QueryBuilder、Pool、Transaction、Hooks 等） |
| **T2 — 契约测试** | 300+ | 公共 API 行为契约（pool、transaction、hooks、error、phinx_migration 等） |
| **T3 — 集成测试** | 400+ | 真实数据库（MySQL 9.6 × PostgreSQL 18 × SQLite）+ RabbitMQ + MinIO |
| **T4 — 属性测试** | 100+ | Property-Based Testing（proptest）覆盖 Value 序列化/SQL 生成 |
| **T5 — Fuzz 测试** | 40+ | 模糊测试（SQL 解析、Value 反序列化、标识符注入抵抗） |
| **T6 — Soak 测试** | 12+ | 长时稳定性测试（10s 冒烟 / 6h 完整） |
| **合计** | **5,442+** | 覆盖全部 43 个 workspace 包 |

### 4.1 T1：单元测试

- 每个模块的独立功能测试，不依赖外部服务
- 使用 `#[cfg(test)] mod tests` 内联在源码中
- 覆盖率要求：核心模块 >= 90%
- 当前状态：sz-orm-core 1300+ 单元测试，覆盖 query/model/pool/transaction/hooks/error/schema_gen/sql_safety/phinx_migration 等全部模块

### 4.2 T2：契约测试

- 集中管理在 `packages/sz-orm-core/tests/contracts/`
- 每一个公共 API 行为契约对应一个测试用例
- 契约变更必须同步更新 `docs/api-contracts.md`
- 运行命令：`cargo test -p sz-orm-core --test contracts`

### 4.3 T3：集成测试

- 需要真实数据库/消息队列/对象存储服务
- 全部标注 `#[ignore]`，仅在 CI 或手动指定时运行
- MySQL 9.6 + PostgreSQL 18 + SQLite 多版本矩阵测试
- 运行命令：`cargo test --package sz-orm-core --test integration_mysql -- --ignored`

### 4.4 T4：Property-Based Testing

- 使用 `proptest` crate（版本统一管理在 workspace dependencies）
- 覆盖：Value 序列化/反序列化、SQL 生成、Dialect 输出、标识符校验
- 运行命令：`cargo test --workspace proptest`（或 `PROPTEST_CASES=10000 cargo test` 强化）

### 4.5 T5：Fuzz 测试

- 覆盖：SQL 解析器、Value 解析、动态 SQL XML 模板、标识符注入抵抗
- 工具：`cargo fuzz`（需 nightly）
- 运行命令：`cargo fuzz run <target>`

### 4.6 T6：Soak 测试

- 短时冒烟（每次 push/PR）：`cargo test --package sz-orm-core --test soak soak_smoke_10s`
- 长时完整（每周日 00:00 UTC）：`cargo test -p sz-orm-core --test soak -- --ignored --nocapture`（soak.yml，timeout 420 分钟）
- 退化检测标准：
  - RSS 增长 > 50MB → 内存泄漏
  - fd_count 增长 > 10 → 句柄泄漏
  - ops_per_sec 衰减 > 10% → 性能退化
  - p99_latency 增长 > 2x → 慢退化

---

## 5. CI/CD 工作流约束

### 5.1 本地开发流程

```mermaid
flowchart LR
    A[修改代码] --> B[cargo check]
    B --> C[运行关联测试]
    C --> D[cargo clippy]
    D --> E[gate.ps1]
    E --> F[git commit]
```

**详细步骤**：

1. **`cargo check --workspace --all-targets`** — 快速编译检查（避免完整 build）
2. **`cargo test -p <affected-package>`** — 运行受影响包的测试
3. **`cargo test -p sz-orm-core --test contracts`** — 运行契约测试（API 变更时必做）
4. **`cargo clippy --workspace --all-targets --all-features -- -D warnings`** — 严格 lint
5. **`./scripts/gate.ps1`** — 本地门禁全关卡验证（7 道关卡 + 新增 4 道）
6. **`git commit`** — 通过后提交

**紧急修复**：使用 `./scripts/gate.ps1 -Fast` 只跑前 3 关（fmt + check + clippy）

### 5.2 AI 辅助开发 11 条硬约束

以下约束适用于任何使用 AI 辅助对 SZ-ORM 进行修改的场景：

| # | 约束 | 说明 |
|---|------|------|
| 1 | **禁止占位实现** | 不允许 AI 生成 `todo!()` / `unimplemented!()` / `unreachable!()` |
| 2 | **强制参数化查询** | 不允许 AI 生成任何 SQL 字符串拼接代码；任何 WHERE 条件必须用 `where_eq`/`or_where_eq` 等参数化方法 |
| 3 | **API 兼容性** | AI 修改公共 API 时必须同步更新 `api-contracts.md` 和契约测试；签名变更（如 `T` → `Result<T,E>`）必须同步更新所有调用方 |
| 4 | **五维审查** | AI 生成代码必须通过正确性/可读性/架构/安全性/性能五维审查 |
| 5 | **unsafe 零容忍** | AI 生成 `unsafe` 代码必须单独标注并经过人工审计，必须有 `// SAFETY:` 注释 |
| 6 | **禁止 mock 逃逸** | AI 引入的 mock/伪实现必须在合入 main 前替换为真实实现 |
| 7 | **门禁前置** | AI 必须主动运行 `gate.ps1` 验证代码，不能依赖 CI 发现编译错误 |
| 8 | **跨平台意识** | AI 添加平台相关代码必须使用条件编译，不能破坏双平台编译 |
| 9 | **Feature 隔离** | AI 修改 feature-gated 代码时必须验证 feature 全组合编译 |
| 10 | **教训记忆** | AI 必须阅读本附录的防御追溯表，避免重复已犯错误 |
| 11 | **禁止幻影交付** | 宣称"自动/强制/默认/集成"的能力必须附生产调用点证据（file:line）；"模块存在 + 测试通过"≠"已交付"。交付前运行门禁 15（`python scripts/check-phantom-delivery.py`）：零调用符号（PHANTOM-1）必须接线或删除；feature gate 无启用点（PHANTOM-2）必须在文档标注"提供 X 组件（需手动接入）"。依据 2026-08-13 审计报告 `docs/assessment/2026-08-13-production-zero-call-audit.md` |

### 5.3 编译时 SQL 验证（db-verify feature）

sz-orm-macros 提供 `query!` 宏，支持连真 DB 验证：

```bash
# 启用连真 DB 验证（支持 MySQL/PostgreSQL/SQLite）
export DATABASE_URL="mysql://root:test123@127.0.0.1:3306/sz_orm_test"
export SZ_ORM_QUERY_VERIFY=1
cargo build --features sz-orm-macros/db-verify
```

- 默认仅语法校验（`validate_sql_content`）
- 启用 `db-verify` feature + `SZ_ORM_QUERY_VERIFY=1` 后，编译时连真 DB 执行 `EXPLAIN` 验证
- 支持 MySQL（`EXPLAIN`）、PostgreSQL（`EXPLAIN`）、SQLite（`EXPLAIN QUERY PLAN`）

### 5.4 部署前检查清单

部署前必须逐项确认以下检查全部通过：

- [ ] **门禁检查**
  - [ ] 10 道门禁全部通过（含增强门禁 8-11）
  - [ ] 所有 feature 组合编译通过

- [ ] **测试检查**
  - [ ] 单元测试 + 集成测试全部通过
  - [ ] Soak 冒烟测试 10s 高并发无内存增长（soak.yml 已配置）

- [ ] **审查检查**
  - [ ] 五维审查全部通过（正确性/可读性/架构/安全/性能）
  - [ ] 无残留的占位宏（todo!/unimplemented!/unreachable!）
  - [ ] 无 SQL 拼接（所有查询参数化）
  - [ ] 所有 unsafe 有 // SAFETY: 注释
  - [ ] API 签名变更已传播到所有调用方和测试

- [ ] **文档检查**
  - [ ] ADR 已记录所有重大决策
  - [ ] API 参考已更新
  - [ ] 审计记录与实际代码状态一致

---

## 6. 附录：SZ-ORM 教训 → 防御追溯表

本表将 SZ-ORM 审查报告中识别的每类问题映射到对应的防御门禁。任何后续修改必须确保不会重蹈覆辙。

| 教训类别 | 问题数 | 防御门禁 | 是否已实现 |
|---------|--------|---------|-----------|
| SQL 注入（C-1~C-6） | 6 | 门禁 9（SQL 拼接扫描）+ 五维审查（安全性）+ where_eq 参数化强制 | ✅ 已实现 |
| 标识符注入（C-2 扩展） | 3 | phinx_migration 标识符校验 + validate_identifier + 门禁 9 | ✅ 已实现 |
| 虚假/伪实现（V-1~V-7） | 7 | 门禁 8（占位检查）+ 五维审查（正确性）+ 门禁 10（feature 全组合） | ✅ 已实现 |
| 转义不一致（H-1） | 1 | 契约测试（T2）+ 各方言独立 escape 测试 | ✅ 已有 |
| 锁 panic（13 处 expect） | 13 | 五维审查（正确性）+ parking_lot 替换 + unwrap 消除 | ✅ 已修复 |
| 名实不符（S-1~S-8） | 8 | 门禁 6（API 审计）+ 契约测试（T2） | ✅ 已有 |
| 夸大对比（D-1~D-7） | 7 | 五维审查（架构维度） | ✅ 已有 |
| 幻影交付（P-1~P-3） | 3 | 门禁 15（零调用符号断言 + feature 启用矩阵）+ 文档措辞分级（"提供 X 组件" vs "自动/强制"）+ M8 接线冒烟 | ✅ 已实现 |
| 数字/性能声明无载体（D-8） | 1 | 数字自动生成（禁止手写）+ 基准代码入仓 | 🟡 待办 |
| Feature 隔离失败（V-4） | 1 | 门禁 10（feature 全组合编译） | ✅ 已实现 |
| 跨平台限制 | 1 | CI 双平台（build matrix: ubuntu + windows + macos） | ✅ 已有 |
| API 签名变更未传播 | — | 五维审查（正确性）+ AI 检查表"API 签名变更传播" | ✅ 已实现 |
| 上游仓库被修改（ADR-0001） | — | 门禁 11（上游未修改检查） | ✅ 已实现 |

### 6.1 教训详情参考

| 编号 | 类别 | 问题 | 文件 | 修复措施 |
|------|------|------|------|---------|
| C-1 | SQL 注入 | `format!` 拼接 SQL 字符串 | 多个查询文件 | 门禁 9 扫描 + 改为参数化查询 |
| C-2 | SQL 注入 | 字符串插值拼接 WHERE 条件 | 动态查询模块 | 门禁 9 扫描 + 使用 QueryBuilder |
| C-3 | SQL 注入 | ORDER BY 子句未过滤列名 | QueryBuilder | 白名单验证 |
| C-4 | SQL 注入 | GROUP BY 用户输入未转义 | 聚合查询 | 参数化 + 白名单 |
| C-5 | SQL 注入 | LIKE 查询未转义通配符 | 搜索模块 | 转义 `%` 和 `_` |
| C-6 | SQL 注入 | 表名动态拼接 | Schema Gen | 白名单 + 门禁 9 |
| C-7 | 标识符注入 | PhinxTable 表名/列名未校验 | phinx_migration.rs | validate_identifier + Result 传播 |
| C-8 | 标识符注入 | FK 约束名/动作未校验 | phinx_migration.rs | validate_fk_action + validate_identifier |
| C-9 | 标识符注入 | FK 引用表/列名未校验 | phinx_migration.rs | validate_identifier |
| V-1 | 虚假实现 | `todo!()` 留在 release 代码 | sz-orm-postgis | 门禁 8 + 五维审查 |
| V-2 | 虚假实现 | 空函数体无实现 | sz-orm-timeseries | 门禁 8 + 五维审查 |
| V-3 | 虚假实现 | `unimplemented!()` 在错误路径 | sz-orm-search | 门禁 8 + 五维审查 |
| V-4 | 虚假实现 | 伪实现数月未在 CI 编译 | real-* feature | 门禁 10（full-features） |
| V-5 | 虚假实现 | mock 实现逃逸到 main | 测试 mock | 门禁 8 + 五维审查 |
| V-6 | 虚假实现 | `unreachable!()` 触发 panic | 错误处理 | 门禁 8 + proper error handling |
| V-7 | 虚假实现 | 空 `match` 分支 | 模式匹配 | 门禁 8 + 补全分支 |
| H-1 | 转义不一致 | 不同方言 escape 行为不同 | Dialect trait | 契约测试覆盖各方言 |
| S-1 | 名实不符 | `find_all` 实际查全部但无分页 | QuickQuery | API 审计 + 契约测试 |
| S-2 | 名实不符 | `delete` 实际软删除 | ModelExt | 更名为 `soft_delete` |
| S-3 | 名实不符 | `save` 未区分 insert/update | ModelExt | 拆分为 `insert`/`update` |
| S-4 | 名实不符 | `query` 不返回查询结果 | 连接方法 | 修正返回值 |
| S-5 | 名实不符 | `batch_insert` 非事务性 | sz-orm-batch | 添加事务包装 |
| S-6 | 名实不符 | `cache.set` 返回值类型不一致 | Cache trait | 统一为 `Result<()>` |
| S-7 | 名实不符 | `connection.ping` 不检测连接状态 | Connection trait | 增加真实 ping |
| S-8 | 名实不符 | `migrate.latest` 不是最新版本 | Migration | 修正语义 |
| D-1 | 夸大对比 | 基准测试未关闭 Turbo Boost | 基准测试 | 添加环境检查 |
| D-2 | 夸大对比 | 对比时使用不同数据集 | 基准测试 | 统一数据量 |
| D-3 | 夸大对比 | warm-up 不足影响结果 | 基准测试 | criterion 强制 warm-up |
| D-4 | 夸大对比 | 只测最优路径 | 基准测试 | 增加 P50/P95/P99 |
| D-5 | 夸大对比 | 未对比竞争对手相同版本 | 基准测试 | 指定版本号 |
| D-6 | 夸大对比 | 测试环境不同 | 基准测试 | CI 固定环境 |
| D-7 | 夸大对比 | 选择性报告结果 | 基准测试 | 完整报告 |
| — | 锁 panic | 13 处 `.expect()` 在 Mutex 上 | 多个文件 | 替换为 `parking_lot` + panic 安全处理 |
| — | Feature 隔离 | real-* feature 数月未在 CI 编译 | Cargo.toml | 门禁 10 + real-features-compile job |
| — | 跨平台 | Windows 路径分隔符差异 | 文件操作 | CI 双平台矩阵 |
| — | API 签名变更未传播 | `create()` 返回类型变更但调用方未更新 | phinx_migration.rs + 测试 | 五维审查 + AI 检查表 |
| P-1 | 幻影交付 | 文档宣称"自动拦截/强制执行"但生产路径零调用（N1QueryDetector、QuotaEnforcer、CacheWarmer、cache_ttl、hooks 等 37 个符号） | entity_graph.rs、tenant_quota_rls.rs、cache_warmup_protection.rs、query.rs 等 | 门禁 15 PHANTOM-1 断言 + 文档降级（2026-08-13，见 docs/assessment/2026-08-13-production-zero-call-audit.md） |
| P-2 | 幻影交付 | feature gate 定义存在但无任何启用点，默认构建不编译（149 个 gate） | 各包 Cargo.toml | 门禁 15 PHANTOM-2 警告 + 文档标注"提供 X 组件（需手动接入）" |
| P-3 | 幻影交付 | "集成验证"里程碑只跑单包 feature 测试，不验证生产接线 | docs/spec/v4.7.0/tasks.md M8 | M8 升级为"生产入口冒烟"（启用 feature 后从 cli/example 真实调用） |
| D-8 | 夸大对比 | 性能/规模数字无载体（"13.8 亿次操作"等）且 README 内数字互相矛盾（56/43/60 成员、5,404/5,809/6,900+ 测试） | README.md:4/100/102/359/849 | 数字自动生成（从 Cargo.toml/测试统计推导）+ 基准代码入仓 + 删除无载体数字 |

---

## 附录：与其他文档的关系

- 本规范定义 **SZ-ORM 工程化的全局规范**，是 sz-orm 项目所有 crate 必须遵守的约定；
- [`AGENTS.md`](../AGENTS.md) 定义 **AI 工作指南与项目架构**，是本规范的前置阅读材料；
- [`docs/audit/`](audit/) 存放 **审计报告与基线文档**，本规范是其工程化审计结论的落地；
- [`docs/adr/`](adr/) 是 ADR 索引，包含 ADR-0001（严禁修改上游仓库）等架构决策；
- 本规范与 sz-rust 的 [`sz-rust-engineering-practices.md`](../sz-rust/docs/sz-rust-engineering-practices.md) 对齐，共享门禁 8-10 的设计理念。

---

## v3.3.0 工程化审查要点（2026-08-08）

### 8 Feature 组合矩阵

| Feature | 包 | 默认 | 依赖 |
|---------|-----|------|------|
| `multi-tenant-enhanced` | sz-orm-core | 关闭 | sz-orm-audit, sz-orm-masking |
| `dist-cache` | sz-orm-core | 关闭 | bloomfilter, rand, sz-orm-crypto |
| `graphql-n1` | sz-orm-graphql | 关闭 | parking_lot |
| `graphql-schema-gen` | sz-orm-graphql | 关闭 | sz-orm-macros |
| `graphql-complexity` | sz-orm-graphql | 关闭 | 无 |
| `ai-nl2sql-enhanced` | sz-orm-ai | 关闭 | 无 |
| `ai-index-advisor` | sz-orm-ai | 关闭 | sqlparser |
| `ai-rewrite-advisor` | sz-orm-ai | 关闭 | sqlparser |

### v3.3.0 审查清单

1. **Feature 正交性**：8 个 feature 任意组合编译通过
2. **默认零行为变更**：不启用 feature 时，既有测试全部通过
3. **API 向后兼容**：无 Breaking Change，新能力通过 feature gate 隔离
4. **安全审计**：新代码零 SQL 注入、零 unsafe、零占位实现
5. **性能不回退**：v3.2.0 性能基准不回退
6. **下游零回归**：sz-pay 5139 测试零回归

### 五方言集成测试指南

v3.3.0 四项新能力在五方言（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）下的行为一致性验证：

```bash
# MySQL
DATABASE_URL="mysql://root:test123@127.0.0.1:3306/sz_orm_test" cargo test --workspace -- --ignored

# PostgreSQL
DATABASE_URL="postgres://postgres:test123@127.0.0.1:5432/sz_orm_test" cargo test --workspace -- --ignored

# SQLite（内存）
cargo test --workspace

# Oracle / MSSQL：需配置对应数据库连接
```

### v3.3.0 新增模块

| 模块 | 文件 | 测试数 |
|------|------|--------|
| TenantContext | `packages/sz-orm-core/src/tenant_context.rs` | 8 |
| TenantSecurity | `packages/sz-orm-core/src/tenant_security.rs` | 12 |
| DistCache | `packages/sz-orm-core/src/dist_cache.rs` | 22 |
| GraphQLIR | `packages/sz-orm-graphql/src/query_ir.rs` | 16 |
| DataLoader | `packages/sz-orm-graphql/src/dataloader.rs` | 7 |
| SchemaGen | `packages/sz-orm-graphql/src/schema_gen.rs` | 10 |
| Complexity | `packages/sz-orm-graphql/src/complexity.rs` | 11 |
| IntentAnalysis | `packages/sz-orm-ai/src/intent_analysis.rs` | 12 |
| IndexAdvisor | `packages/sz-orm-ai/src/index_advisor.rs` | 9 |
| RewriteAdvisor | `packages/sz-orm-ai/src/rewrite_advisor.rs` | 11 |
| AdviceCommon | `packages/sz-orm-ai/src/advice_common.rs` | 5 |

---

## v3.4.0 工程化审查要点（2026-08-09）

### 10 Feature 组合矩阵

| Feature | 包 | 默认 | 依赖 |
|---------|-----|------|------|
| `test-coverage` | sz-orm-core | 关闭 | 无 |
| `arch-improvement` | sz-orm-core | 关闭 | 无 |
| `perf-smallstring` | sz-orm-core | 关闭 | compact_str |
| `perf-enum-dispatch` | sz-orm-core | 关闭 | 无 |
| `perf-zero-copy-l2` | sz-orm-core | 关闭 | zero-copy |
| `perf-box-str` | sz-orm-core | 关闭 | 无 |
| `type-safe-columns` | sz-orm-core | 关闭 | sz-orm-macros/type-safe-columns |
| `typed-column` | sz-orm-core | 关闭 | 无 |
| `typed-dsl` | sz-orm-core | 关闭 | 无 |
| `migration-guide` | sz-orm-core | 关闭 | 无 |

### v3.4.0 审查清单

1. **Feature 正交性**：10 个 feature 任意组合编译通过
2. **默认零行为变更**：不启用 feature 时，既有测试全部通过
3. **API 向后兼容**：无 Breaking Change，新能力通过 feature gate 隔离
4. **安全审计**：新代码零 SQL 注入、零 unsafe、零占位实现
5. **性能不回退**：v3.3.0 性能基准不回退
6. **下游零回归**：sz-pay 6 个测试套件零回归
7. **五方言集成测试**：MySQL 23 + PostgreSQL 18 + SQLite 25 + Oracle 10 + DuckDB 7 = 83 项全通过

### v3.4.0 新增模块

| 模块 | 文件 | 测试数 |
|------|------|--------|
| SqlBuffer | `packages/sz-orm-core/src/sql_buffer.rs` | 16（差分） |
| DialectKind | `packages/sz-orm-core/src/dialect.rs` | 4（基准） |
| Value::BoxedStr | `packages/sz-orm-core/src/value.rs` | 4（基准） |
| L2 zero_copy | `packages/sz-orm-core/src/l2_cache.rs` | 4（基准） |
| Column<T> | `packages/sz-orm-core/src/column.rs` | 30 |
| typed_ast ext | `packages/sz-orm-core/src/typed_ast.rs` | 30 |
| sz_pay_pattern | `examples/src/bin/sz_pay_pattern.rs` | 1（示例） |

---

> **最后更新**: 2026-08-09
> **维护人**: SZ-ORM 工程团队
> **规范版本**: v3.4
>
> **v3.4 变更摘要**（2026-08-09）：
> - 新增 v3.4.0 工程化审查要点（10 feature 组合矩阵 + 审查清单 + 五方言集成测试结果）
> - 新增 7 个模块的测试数据
> - 项目版本 v3.3.0 → v3.4.0
>
> **v3.3 变更摘要**（2026-08-08）：
> - 新增 v3.3.0 工程化审查要点（8 feature 组合矩阵 + 审查清单 + 五方言集成测试指南）
> - 新增 11 个模块的测试数据
> - 项目版本 v3.2.0 → v3.3.0
>
> **v3.1 变更摘要**（2026-08-01）：
> - 新增第 0 章：ADR-0001 核心原则（严禁修改上游仓库）
> - 新增门禁 11：上游仓库未修改检查
> - 新增 5.3 节：编译时 SQL 验证（db-verify feature）
> - AI 检查表新增"API 签名变更传播"和"标识符注入检查"两项
> - 教训追溯表新增 C-7/C-8/C-9（标识符注入）和"API 签名变更未传播"条目
> - 项目版本 v1.2.0 → v1.2.1
> - 测试数据统一为 6,794+（覆盖 52 个 workspace 包，2026-08-12 实测）
