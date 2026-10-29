# v10.13.0 安全整改交付记录（2026-10-27 评测 P0/P1/P2 全项）
> **安全说明（2026-10-28 占位化）**：本文档中的服务器 IP 已替换为占位符；实际主机地址与凭据由运维在对应测试服务器的环境变量中提供，禁止提交真实地址/密码到仓库。

- 日期：2026-10-27
- 依据：`docs/assessment/2026-10-27-security-review.md`（AppSec/SecOps 独立评测，结论"有条件投产"）
- 执行方式：按评测报告 §9 路线图顺序（P0-1 → P0-2 → P0-3 → P0-4 → P1-5~8 → P2-9~13）
- 版本：工作空间 10.12.0 → **10.13.0**（`Cargo.toml:6`）
- 证据规则：每条结论附真实 `file:line`；可复跑命令附实际输出

> **验证边界（如实声明）**：本机无 Rust 工具链（`cargo`/`rustc`/`git` 不在 PATH，
> `D:\tools\cargo-home`、`D:\tools\rustup-home` 为空），Rust 代码改动的
> `cargo check/test/clippy` 验证需在工具链机器上补跑（清单见 §5）。
> Python 门禁脚本与逻辑模拟已在本机实际运行，输出见 §4。

---

## 1. P0 阻断项（4/4 完成）

### P0-1（H-1）rustls 0.21.12 漏洞链真实消除 ✅

| 变更 | 证据 |
|---|---|
| `ElasticsearchProvider` 底层 HTTP 客户端从 elasticsearch crate 8.5.0-alpha.1 迁移为 reqwest 0.12 直连 ES REST API（8 个端点，路径段经 `Url::path_segments_mut` 百分号编码） | `packages/sz-orm-search/src/elasticsearch_provider.rs:14`（`use reqwest::{Client, Method, RequestBuilder, StatusCode, Url}`）、`:39`（`new`）、`:58`（`request` 拼接+认证）、`:105`（`impl SearchExt`） |
| `real-es` feature 与依赖声明替换 | `packages/sz-orm-search/Cargo.toml:17`（`real-es = ["dep:reqwest"]`）、`:32`（`reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"], optional = true }`） |
| CHANGELOG v10.6.0 失实声称加修正声明（不篡改原文，保留审计记录） | `CHANGELOG.md:281`（修正声明块） |
| CHANGELOG 新增 v10.13.0 整改条目 | `CHANGELOG.md:8` |
| deny.toml rustls 相关 ignore 加"待 Cargo.lock 刷新后移除"备注 | `deny.toml:42`（RUSTSEC-2026-0049 条目注释） |

### P0-2（H-2）LLM 安全审计 14 条规则真实落地 ✅

| 变更 | 证据 |
|---|---|
| `builtin_patterns()` 从 4 条扩展至 14 条：新增 `or_1_eq_1_variant` / `union_injection_encoded` / `stacked_injection_variant` / `comment_injection_nested` / `sensitive_data_password` / `sensitive_data_secret` / `sensitive_data_token` / `privilege_escalation_drop` / `privilege_escalation_alter` / `privilege_escalation_grant`，与 `tests/llm_security_audit_expanded_test.rs` 断言一致 | `packages/sz-orm-ai/src/llm_security_audit.rs:129`（`builtin_patterns`）、`:164`（首条新规则）、`:227`（末条新规则） |
| 新增 `InjectionPatternStore::from_patterns` 构造器 | `packages/sz-orm-ai/src/llm_security_audit.rs:237` |
| 新增 `LlmSecurityAuditor::rule_only_excluding(excluded)`：内部 DDL 生成器（IndexAutoApplier/SchemaAutoApplier）生成的合法 DDL（`ALTER TABLE ADD COLUMN`、对 `token` 列建索引）不被 DDL 动词/敏感列名规则拦截，注入类规则仍生效 | `packages/sz-orm-ai/src/llm_security_audit.rs:399` |
| `SchemaAutoApplier::security_check` / `IndexAutoApplier::security_check` 改用排除构造器（防止 14 规则上线后 auto-evolver 功能回归） | `packages/sz-orm-ai/src/ai_deep/schema_design_advisor.rs`（security_check 内 LlmSecurityAuditor 调用）、`packages/sz-orm-ai/src/index_advisor.rs`（同） |
| 逻辑模拟验证：expanded 12 测试 + 主测试 5 断言 + learning 2 断言 + DDL 排除 4 断言 = 26/26 通过 | §4 输出（`zcode-rules-sim.py`） |

### P0-3（H-3）凭据处置 ✅（仓库侧；凭据轮换需运维）

| 变更 | 证据 |
|---|---|
| 文档中真实数据库/SSH/MSSQL 凭据全部替换为 `<your-password>` 占位符（e2e-env-config / e2e-execution-guide / quickstart / .codeartsdoer v830 design+spec+tasks+supplement_tasks / gate-block-report / 评测报告引文脱敏） | `docs/e2e-env-config.md:12-14`（占位符连接串）、`docs/e2e-env-config.md:36`（SSH 密码占位符 + 整改声明 :6-8） |
| `sz-orm-macros` 文档与测试夹具中的真实 MSSQL 凭据替换（`ExamplePwdOnly4Test`） | `packages/sz-orm-macros/src/lib.rs:1640`（doc）、`:3575`（test） |
| `check-secrets.py` 连接串正则补 `sqlserver|mssql|oracle` scheme（checker 盲区修复）+ 假阳性扩充 + `${{ secrets.* }}` CI 上下文过滤 | `scripts/check-secrets.py:99`（正则） |
| 接入门禁三处：gate.ps1 关卡 9 并入、run-gates-v10120-server.sh Gate 9 并入、CI（security.yml + ci.yml）新增 secret-scanning job | `scripts/gate.ps1:215-224`、`scripts/run-gates-v10120-server.sh:64-72`、`.github/workflows/security.yml:79-80`、`.github/workflows/ci.yml:842` |
| **实测**：`check-secrets.py` 从 30 findings → **0 findings，退出码 0** | §4 输出 |
| ⚠ 遗留（运维动作）：<remote-server-ip> 的 MySQL/PG/SSH 凭据与 Tencent MSSQL 凭据已进 git 历史，**必须轮换**；必要时 `git filter-repo` 清史 | 评测报告 H-3 |

### P0-4（H-4）审计日志值级脱敏 ✅

| 变更 | 证据 |
|---|---|
| `mask_sensitive` 升级：关键词后的 `= '值'` / `= "值"` 引号值一并掩码（含反斜杠转义引号处理），与 `zk_mask_sql` 值级语义对齐；`VALUES` 位置关联值为已知残留限制（文档注明 + 参数化查询下 SQL 文本不含字面量值） | `packages/sz-orm-audit/src/lib.rs:227`（`mask_sensitive`）、`:205`（SqlAuditor 方法） |
| 新增 3 个回归测试（单引号值 / 双引号值含空格 / VALUES 残留锚点） | `packages/sz-orm-audit/src/lib.rs:1255`（`test_mask_sensitive_masks_quoted_value_after_eq`）等 |
| 模块文档声称同步更新 | `packages/sz-orm-audit/src/lib.rs:4-9` |
| 既有 12 处脱敏测试断言兼容性 + 新 3 测试 + 转义边界，共 16 断言模拟全通过 | §4 输出（`zcode-mask-regress.py`） |
| PoC 对照：`password='hunter2'` 旧行为 `******='hunter2'`（值明文）→ 新行为 `******='******'` | §4 输出 |

---

## 2. P1 高优先（4/4 完成）

| 项 | 变更 | 证据 |
|---|---|---|
| M-1 CI ignore 对齐 | `security.yml` 与 `ci.yml` 的 cargo-audit `--ignore` 从 8 条补齐至与 `deny.toml` 一致的 24 条 | `.github/workflows/security.yml`（cargo-audit step）、`.github/workflows/ci.yml`（同） |
| M-2 G9 扫描器加固 | ①错误消息规则去掉 `push_str` 豁免；②`push_str`+WHERE 仅在无插值或全白名单变量时判 Safe，否则 ManualReview（新增 `_extract_format_args` 括号配对解析）；③`#[cfg(test)]` 之后的行按测试上下文判定 | `scripts/check-sql-injection-auto.py:256`（push_str 规则）、`:316`（cfg(test) 感知）、`_extract_format_args`/`_split_top_level` 辅助函数 |
| M-3 G9 自测修复 | 自测从 8 passed/1 failed → **13 passed/0 failed**（新增 3 个 push_str 单测 + 1 个 cfg(test) 模块测试；workspace 扫描断言恢复通过：46/46 Safe、--strict 退出码 0） | `scripts/test_check_sql_injection_auto.py:101`（manual_review 单测）、`:131`（cfg(test) 单测）、`:173`（workspace 断言）；§4 输出 |
| M-4 门禁脚本 | Gate 1 真实 `cargo fmt --all -- --check`（不再硬编码 PASS）；Gate 9 加 `--strict` 并入 secrets；Gate 21 纳入 owasp-pentest-suite（12 个定义该 feature 的包逐一执行） | `scripts/run-gates-v10120-server.sh:25-27`（Gate 1）、`:64-72`（Gate 9）、`:124-141`（Gate 21） |

---

## 3. P2 加固项（5/5 完成）

| 项 | 变更 | 证据 |
|---|---|---|
| M-7 DbError 信息泄露 | 新增 `DbError::redacted_summary()`：24 变体全映射为生产安全摘要，`Contextual` 递归；Display 保留细节供日志 | `packages/sz-orm-core/src/error.rs:296`（方法）、`:806`（测试：断言不含内部 SQL 片段） |
| M-8 MFA 防爆破 | 新增 `TotpAttemptGuard`（连续失败 N 次后退避，成功清零，key 隔离）+ `TotpVerifier::verify_with_guard`（退避期内正确码也拒绝）+ 3 个测试 | `packages/sz-orm-auth/src/mfa.rs:213`（struct）、`:243`（is_blocked）、`:151`（verify_with_guard）、`:455/:466`（测试） |
| L-1/L-2 示例密钥 | 示例 JWT 密钥改环境变量（`SZ_ORM_EXAMPLE_JWT_SECRET`）；`OpenAIEmbeddingClient` 新增 `from_env()` 推荐入口并标注 `new` 的泄露面 | `examples/src/bin/production_app.rs:297-298`、`packages/sz-orm-ai/src/real_embedding.rs:74`（from_env） |
| L-3 SAFETY 口径裁决落地 | `check-unsafe-audit.py` 认可双口径（块级 `// SAFETY:` / 函数级 `/// # Safety`），双口径皆缺才失败；实测 17 文件全有块级覆盖、0 失败 | `scripts/check-unsafe-audit.py`（口径注释与 `_FN_LEVEL_SAFETY`）；§4 输出 |
| L-4 测试数字校正 | AGENTS.md G9 "66/66 Safe" → 实测口径；G21 "85 个测试" → 门控 49/命名相关 145（实测）；runbook 两处同步 | `AGENTS.md:34`（G9）、`AGENTS.md:46`（G21）、`docs/sz-orm-review-runbook.md:72`、`:94` |

**文档与交付记录**：

| 项 | 证据 |
|---|---|
| Security.md 对齐 v10.13.0（M-5）：版本头、支持版本表、VDP 渠道修正（移除 `security@sz-orm.local`/`0xSZORM2026` 占位符）、§2.1 历史基线声明、§2.3 不安全默认行为状态更新、§5.1 R-H01/R-H02 状态复核、§12 变更历史 | `docs/Security.md:5-14`（头）、`docs/Security.md:36-49`（VDP） |
| 零信任组件"需手动接入"措辞修正（M-6）：v10.11.0 交付记录顶部加修正声明（11 个顶层组件零生产调用点，与 v10.9.0 已接线组件区分） | `docs/assessment/2026-10-18-v10110-zero-trust-compliance.md:3-11` |
| 评测报告加整改状态横幅 + 引用行号更新 | `docs/assessment/2026-10-27-security-review.md:10-13` |

---

## 4. 本机实测输出汇总（2026-10-27）

```
$ python scripts/check-sql-injection-auto.py --strict        → 退出码 0
=== SQL Injection Automated Review (Gate 9) ===
Total: 46 | Safe: 46 | Unsafe: 0 | ManualReview: 0
Gate 9 PASSED: all 46 items judged Safe (automated)

$ python scripts/test_check_sql_injection_auto.py            → 退出码 0
  PASS: test_r3_question_placeholder ... (8 既有)
  PASS: test_push_str_with_user_input_manual_review   (新增：原误判 Safe → ManualReview)
  PASS: test_push_str_with_whitelisted_var_safe
  PASS: test_push_str_without_interpolation_safe
  PASS: test_cfg_test_module_lines_skipped
  PASS: test_workspace_scan_all_safe
13 passed; 0 failed; 13 total

$ python scripts/check-secrets.py                            → 退出码 0（整改前 30 findings）
Scanned 2919 files ...
OK: no secrets detected

$ python scripts/check-unsafe-audit.py                       → 退出码 0
含 unsafe 关键字的 src 文件数: 17
真实 unsafe 且 SAFETY 双口径皆缺的文件数: 0
仅函数级 # Safety 文档覆盖的文件数: 0
结果: ✅ 通过

$ <LLM 14 规则逻辑模拟>    → 26/26 PASS（expanded 12 + 主测试 5 + learning 2 + DDL 排除 4 + 顺序 1 + 计数 2）
$ <mask_sensitive 值级脱敏回归模拟> → 16/16 PASS（新增 3 + 既有 12 + 转义边界 1）
$ <G9 绕过 PoC 复测>：
  push_str(&format!(" WHERE id={}", input))   旧行为 [Safe] → 新行为 [ManualReview] ✅
  剩余 Safe 项均为文档化豁免：{table}/{filter} 结构占位符（R2）、安全模块测试向量（R4）、
  真实参数化查询（R3）
$ .\scripts\audit-verify.ps1 docs\assessment\2026-10-27-security-review.md
  通过: 154 | 失败: 0 | 警告: 0 → 审计证据验证通过
```

---

## 5. 工具链安装与本机 cargo 验证（2026-10-27 已执行，非补跑）

> 用户指示"安装最新版"后，本机已通过 TUNA 镜像安装 **Rust 1.99.1（2026-10-15 发布，最新 stable）**：
> - 下载 `rustup-init.exe`（`https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe`，12.7MB）
> - 发现系统环境 `CARGO_HOME=D:\cargo-home` / `RUSTUP_HOME=D:\rustup-home`，但 `D:\cargo-home\bin` 下代理二进制为 **0 字节**（原安装损坏）；`D:\rustup-home\toolchains` 工具链完好
> - 用 `rustup-init -y --default-toolchain stable --profile minimal` 修复代理（`D:\cargo-home\bin` 的 cargo/rustc 恢复可用）
> - 配置 TUNA crates 镜像（`D:\cargo-home\config.toml`：`sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/`）
> - MSVC 链接器：`D:\BuildTools\VC\Auxiliary\Build\vcvars64.bat` + MSVC 14.44 可用

### 5.1 整改验证命令执行结果（全部 PASS）

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo check -p sz-orm-search --features real-es` | ✅ rc=0（H-1 迁移编译通过） |
| 2 | `cargo test -p sz-orm-search --features real-es` | ✅ rc=0 |
| 3 | `cargo test -p sz-orm-ai --features ai-security-audit --test llm_security_audit_expanded_test` | ✅ rc=0（H-2 12 测试全绿） |
| 4 | `cargo test -p sz-orm-ai --features ai-security-audit --test llm_security_audit_test` | ✅ rc=0（66 passed） |
| 5 | `cargo test -p sz-orm-ai --features ai-security-audit --test injection_pattern_learning_test` | ✅ rc=0（13 passed；修复 `or_1_eq_1_variant` 误匹配 "xor 1=1" 后通过） |
| 6 | `cargo test -p sz-orm-audit --lib` | ✅ rc=0（77 passed，含 mask_sensitive 值级脱敏新测试） |
| 7 | `cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a08_logging_failures` | ✅ rc=0（7 passed） |
| 8 | `cargo test -p sz-orm-core --lib test_db_error_redacted_summary_hides_internal_details` | ✅ rc=0（M-7） |
| 9 | `cargo test -p sz-orm-auth --lib test_attempt_guard_blocks_after_max_failures` / `test_verify_with_guard_blocks_brute_force` | ✅ rc=0（M-8） |
| 10 | `cargo check -p sz-orm-macros --features db-verify` | ✅ rc=0（H-3 DSN 夹具编译通过） |
| 11 | `cargo test -p sz-orm-auth --test security_attacks` | ✅ rc=0（5 passed） |
| 12 | `cargo test -p sz-orm-crypto --test kat` | ✅ rc=0（KAT） |
| 13 | `cargo test -p sz-orm-core --features multi-tenant-enhanced --test security_attacks` | ✅ rc=0（4 passed，含 tenant_id 参数化断言） |
| 14 | `cargo check -p sz-orm-back`（zstd 0.14 升级后） | ✅ rc=0（G6） |
| 15 | `cargo-audit audit --db <本地 advisory-db> --no-fetch --no-yanked`（CI 风格 21 条 ignore，最终版） | ✅ rc=0（**0 vulnerabilities**，4 allowed warnings） |
| 16 | `cargo check -p sz-orm-core`（StrictTenantScope 新增后） | ✅ rc=0（编译通过） |
| 17 | `cargo test -p sz-orm-core --lib t26h_strict_tenant_scope*` | ✅ rc=0（**2 passed**：with_tenant / without_tenant） |
| 18 | `cargo-audit audit`（actix-http 升级后，21 条 ignore） | ✅ rc=0（0 vulnerabilities） |

> 说明：`cargo deny check advisories` 因 GitHub advisory-db 拉取超时（网络受限，与 deny.toml 历史记录一致）
> 未能本机执行；已用 `cargo-audit` + 本地 advisory-db（经 ghproxy 下载）等价验证 G6。
> `cargo fmt --check`（Gate 1）脚本已改为真实执行，格式化检查需在完整工作区上补跑一次。

### 5.2 G6 供应链验证发现与处理

`cargo-audit`（升级前）发现 **11 个漏洞**，处置如下：

| 公告 | 处置 | 结果 |
|---|---|---|
| RUSTSEC-2026-0385（ctutils 0.4.2） | `cargo update` → ctutils **0.4.6** | ✅ 消除，deny.toml 移除 ignore |
| RUSTSEC-2026-0386（tokio 1.53.1） | `cargo update` → tokio **1.54.1** | ✅ 消除，deny.toml 移除 ignore |
| RUSTSEC-2026-0383（webpki-root-certs 1.0.9） | `cargo update` → **1.0.11** | ✅ 消除，deny.toml 移除 ignore |
| RUSTSEC-2026-0381（webpki-roots 1.0.9，**新发现未登记**） | `cargo update` → **1.0.11** | ✅ 消除 |
| RUSTSEC-2026-0391/0403/0410（zstd 0.13.3 / zstd-safe 7.2.1 / zstd-sys 2.0.12） | `packages/sz-orm-back/Cargo.toml` zstd 0.13→0.14 + `cargo update` → zstd **0.14.3** / zstd-safe **8.2.0** / zstd-sys **2.1.2** | ✅ sz-orm-back 编译通过；锁文件干净 |
| RUSTSEC-2026-0391/0398/0403（zstd 0.13.3 / zstd-safe 7.3.0，actix-http 3.13.5 传递链） | **第二轮升级**：`cargo update -p actix-http` → actix-http **3.20.0**（依赖 zstd ^0.14 干净线），zstd 0.13.3 / zstd-safe 7.3.0 从锁文件**移除** | ✅ 全部消除，deny.toml 新增的 3 条 ignore 已删除 |
| RUSTSEC-2026-0382（rustls-webpki 0.102.8/0.103.15）、0367/0368/0369（zip 6.0.0） | 既有 deny.toml ignore（上游 major 升级不可行） | ✅ 保留（仅剩这 5 条） |

**运维动作（非代码）**：
1. ~~立即轮换~~ **状态更新（2026-10-28）**：相关服务器（<remote-server-ip> / <legacy-server-ip>）已由运维**重装系统**，仓库中记载的 MySQL/PostgreSQL/SSH 凭据已全部失效，**无需再轮换**；仓库内所有真实主机 IP 已批量替换为占位符（`<remote-server-ip>` / `<legacy-server-ip>`，涉及 24 个文件），`deploy-122.sh` 改为经 `SZORM_SERVER_122` 环境变量配置主机地址。crates.io token 若曾进入历史仍建议按需轮换。
2. 视需要用 `git filter-repo` 清理历史（会改写 SHA，需协调所有协作者）——服务器重装后风险已消除，此项降级为可选。
3. 凭据后续经密钥管理下发（环境变量 / Vault）。

### 5.3 补充证据索引（工具链安装与 G6 验证相关）

file://packages/sz-orm-back/Cargo.toml#L27
CHANGELOG.md:39
packages/sz-orm-core/src/hooks.rs:541
packages/sz-orm-core/src/hooks.rs:1667
packages/sz-orm-core/src/hooks.rs:1681
CHANGELOG.md:47

---

## 6. 遗留跟踪项（未在本轮闭环）

| 项 | 状态 | 去向 |
|---|---|---|
| R-H01 TenantScope None 放行 | ✅ **已闭环**（v10.13.0）：`StrictTenantScope` 提供默认拒绝变体（None 时返回 `1 = 0` 恒假条件） | `packages/sz-orm-core/src/hooks.rs:541`；Security.md §5.1 已更新 |
| `${}` 插值运行时拒绝开关 | 受控（文档警告 + G9 拦截 + 生产无执行点） | Security.md §5.1 长期跟踪 |
| `{table}`/`{filter}` 占位符无条件信任（G9 R2） | 文档化豁免（改动会造成海量 ManualReview 噪声） | 后续版本引入调用点分析后收紧 |
| audit `INSERT ... VALUES ('字面量')` 位置关联值 | 已知残留限制（与 zk_mask_sql 一致；参数化查询下无字面量） | lib.rs 模块文档 + 回归锚点测试已注明 |
| `Cargo.lock` 刷新 | ✅ 已执行（2026-10-27）：rustls 0.21.12 链移除、tokio 1.54.1、webpki-roots 1.0.11、zstd 0.14.3 | 见 §5.2 |
| 文档残留真实服务器 IP | ✅ **已闭环（2026-10-28）**：`<remote-server-ip>` / `<legacy-server-ip>` 在 24 个文件中的全部出现已替换为占位符；`deploy-122.sh` 主机地址改环境变量配置；服务器已重装，相关凭据失效 | 见 §5.2 运维动作 1 |

---

## 7. 门禁沉淀（防回归，2026-10-27 安全评测提炼）

将本会话发现的问题转化为自动化门禁，防止后续开发回归：

| 新门禁 | 脚本 | 防范的会话问题 | 级别 | 接入位置 |
|---|---|---|---|---|
| Gate 26 | `scripts/check-ci-deny-sync.py` | M-1：CI `--ignore` 与 deny.toml 漂移 | 阻断 | `run-gates-v10120-server.sh`、`gate.ps1` |
| Gate 27 | `scripts/check-gate-scripts.py` | M-4：门禁脚本硬编码 PASS / `\|\|` 假 PASS / 关键脚本未接入 | 阻断 | 同上 |
| Gate 28 | `scripts/check-security-doc-version.py` | M-5：Security.md 版本漂移 | 阻断 | 同上 |
| Gate 29 | `scripts/check-test-symbols.py` | H-2：测试引用不存在的规则名/模式名 | 警告级 | 同上 |
| 辅助 | `scripts/check-lockfile-ignore-relevance.py` | M-1：deny.toml 过时 ignore 滞留 | 警告级 | 发布前辅助（需本地 advisory-db，`--db`/`SZ_ORM_ADVISORY_DB`） |
| Gate 8 扩展 | `check-unsafe-audit.py` 并入 | L-3：unsafe SAFETY 审计口径 | 阻断 | Gate 8（占位实现+unsafe） |
| Gate 9 扩展 | `test_check_sql_injection_auto.py` 并入 | M-3：扫描器自测必须随门禁执行 | 阻断 | Gate 9（扫描+secrets+自测） |

实测结果（2026-10-27）：
- `check-ci-deny-sync.py` → ✅ PASS（21 == 21）
- `check-gate-scripts.py` → ✅ PASS（无硬编码 PASS、无 `||` 假 PASS、关键脚本均已接入）
- `check-security-doc-version.py` → ✅ PASS（10.13.0 == 10.13.0）
- `check-test-symbols.py` → ✅ PASS（聚焦 `detected_patterns` 规则名上下文后零误报）
- `check-lockfile-ignore-relevance.py` → ✅ PASS（无过时 ignore；本地 advisory-db 存在）

AGENTS.md 门禁表已更新（新增 26-29 行，扩展 8/9 行描述）；`docs/sz-orm-review-runbook.md` 门禁表同步。

---

## 附录：audit-verify 自证输出

运行命令：`powershell -NoProfile -ExecutionPolicy Bypass -Command "& .\scripts\audit-verify.ps1 docs\assessment\2026-10-27-security-review-fixes.md"`

```
==========================================
  审计证据验证: docs\assessment\2026-10-27-security-review-fixes.md
==========================================
  （逐条验证省略，共 34 条：Cargo.toml:6 / elasticsearch_provider.rs:14 /
   Cargo.toml:17 / CHANGELOG.md:281 / CHANGELOG.md:8 / deny.toml:42 /
   llm_security_audit.rs:129,237,399 / sz-orm-macros lib.rs:1640 /
   sz-orm-audit lib.rs:227,1255,4 / error.rs:296 / mfa.rs:213 /
   real_embedding.rs:74 / AGENTS.md:34,46 / sz-orm-back Cargo.toml:27 /
   CHANGELOG.md:39 / hooks.rs:541,1667,1681 / CHANGELOG.md:47）
==========================================
  验证结果
==========================================
  ✔ 通过: 34
  ✘ 失败: 0
  ⚠ 警告: 0
==========================================
✔ 审计证据验证通过
```

> 说明：本报告引用均经 audit-verify 按物理行 + 空行校验；`packages/sz-orm-back/Cargo.toml`
> 以 `file://` 相对路径引用以确保指向正确文件。2026-10-27 本机安装 Rust 1.99.1 后，
> §5 的 15 条 cargo 验证命令全部执行并通过（详见 §5.1/§5.2）；`cargo deny check advisories`
> 因 GitHub advisory-db 拉取超时未执行（原因如实记录），等价 `cargo-audit` 验证已覆盖。
> 配套评测报告 `2026-10-27-security-review.md` 同样通过（154 通过 / 0 失败 / 0 警告）。
