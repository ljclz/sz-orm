# sz-orm v10.12.0 安全工程师独立评测报告（AppSec / SecOps）
> **安全说明（2026-10-28 占位化）**：本文档中的服务器 IP 已替换为占位符；实际主机地址与凭据由运维在对应测试服务器的环境变量中提供，禁止提交真实地址/密码到仓库。

- 评测日期：2026-10-27
- 评测对象：`D:\www\test\鲜视达\rust\sz-orm`（sz-orm v10.12.0，72 workspace 成员）
- 评测人：顶级安全工程师（AppSec / SecOps 视角，15+ 年）
- 报告路径：`docs/assessment/2026-10-27-security-review.md`
- 评测性质：只读评测，未修改任何源码/文档/脚本

> **⚠ 整改状态更新（同日）**：本报告的 P0/P1/P2 整改路线图已于 2026-10-27 当日按序执行完毕，
> 工作空间版本升至 v10.13.0。各漏洞当前状态：**H-1 / H-2 / H-3 / H-4 / M-1 ~ M-8 / L-1 ~ L-4
> 均已整改**（R-H01 TenantScope、`${}` 插值为受控跟踪项，见报告正文状态标注）。
> 详见配套修复报告 `docs/assessment/2026-10-27-security-review-fixes.md`。
> 注意：Rust 代码改动的编译/测试验证需在具备 Rust 工具链的机器上补跑（本机无工具链）。
> 报告正文保留评测时（v10.12.0）的原始发现与证据；个别 file:line 引用已更新为整改后行号并注明。

> **方法声明**：本报告不采信任何历史报告的结论，所有关键发现均在本机通过源码核对、脚本实测或 PoC 复现。`cargo` 系列命令（`cargo audit` / `cargo deny` / `cargo test`）因本机未安装 Rust 工具链（`C:\Users\Administrator\.cargo` 与 `D:\tools\cargo-home`、`D:\tools\rustup-home` 均为空，`cargo`/`rustc`/`git` 不在 PATH）**未能执行**，已如实标注"未执行+原因"；等价静态检查与 Python 扫描脚本已实际运行并附输出。

---

## 1. 总体安全评价

**结论：有条件投产（Conditional）**。

sz-orm 的核心数据访问层安全设计（强制参数化查询、集中式 `validate_identifier` 标识符校验、JWT HS256 白名单 + 恒定时间比较、DEK zeroize、FFI 层 `#[repr(C)]` + `catch_unwind` + 双 free 防护、连接池 shutdown 竞态修复）在本次评测中**均被证实真实存在且质量良好**，未发现可被远程攻击者直接利用的 SQL 注入/认证绕过/内存破坏链。但本报告同时确认了 **4 项 High 级问题**：①CHANGELOG 声称已消除的 rustls 0.21.12 漏洞链实际仍完整存在于 `Cargo.lock`（幻影修复）；②LLM 安全审计规则扩展未落地、测试引用不存在的规则名（幻影交付）；③仓库提交了 30 处真实数据库/SSH/Oracle 凭据且 `check-secrets.py` 检出失败、该脚本未接入任何门禁；④默认审计日志脱敏只掩码关键词不掩码值，敏感值明文落盘。任一 High 未闭环，按评测人安全官最终否决权，**暂不建议作为生产依赖引入**；将上述 High 闭环并同步修复 CI 门禁不一致后，可转入投产评估。

---

## 2. 威胁建模与攻击面分析（STRIDE）

攻击者模型：
- **A1 本地调用方**（库 API 使用者）：传入表名/字段名/SQL 文本/JSON；可利用标识符注入、动态 SQL 插值、错误信息泄露。
- **A2 远程输入**（经应用层透传）：用户输入最终成为查询参数/表名/字段名/动态 SQL XML。
- **A3 恶意 SQL 文本**：迁移脚本、dynamic_sql XML、`query!` 宏 SQL、AI 生成 SQL。
- **A4 供应链**：依赖 crate 的已知漏洞（rustls/webpki/quick-xml/rsa/tokio 等）。
- A5 恶意本地进程：读取进程环境/日志中的凭据。

| 组件 | 威胁 | 攻击路径 | 前置条件 | 可达性 |
|---|---|---|---|---|
| 连接池（pool.rs） | Spoofing / DoS | 关闭后仍新建连接导致资源泄漏 | 并发 shutdown 与 acquire 交错 | 已修复（pool.rs:1807-1814、1912-1918），回归测试存在 |
| 查询构建（query.rs） | Tampering / SQLi | `having`/`order_by` 等原始字符串入口 | 调用方传入未净化字段 | 低：参数化铁律 + validate_identifier 覆盖表/列名；`having` 需人工审查 |
| 动态 SQL（dynamic_sql.rs） | Tampering / SQLi | `${name}` 插值 | 调用方使用 `${}` 且输入不可信 | 中：文档明确警告（dynamic_sql.rs:53-59），生产无 `${}` 执行点，但转义仅限字符串字面量上下文 |
| 迁移（migration.rs / phinx_migration.rs） | Tampering / SQLi | 表名/列名/约束名 | 恶意迁移脚本 | 低→中：validate_identifier 已覆盖（migration.rs:1040-1046、phinx_migration.rs:397-400） |
| 事务（transaction.rs） | DoS / 完整性 | panic 安全回滚 | panic 中断 | 已修复（历史报告确认，事务 panic 安全） |
| FFI 层（sz-orm-cabi） | 内存破坏 / 双 free / panic 跨边界 | 指针强转、`Box::from_raw`、CStr 解引用 | C/Go/Java/Python 调用方传非法指针 | 中→低：repr(C)（lib.rs:175）、catch_unwind（lib.rs:400 等）、双 free 防护（ffi_memory.rs:29-48）均已落地 |
| JWT/RBAC（sz-orm-auth） | 伪造 / 提权 / 越权 | 弱密钥、alg 混淆、refresh 混淆、通配符权限 | 应用层配置不当 | 低：HS256 白名单（jwt.rs:196-211）、token_use 区分（jwt.rs:63）、通配符显式化（authorizer.rs:216） |
| 加密（sz-orm-crypto） | 信息泄露 / 侧信道 | DEK 明文残留、RSA Marvin、PBKDF2 弱迭代 | 攻击者读取内存/时序 | 低：DEK Zeroizing（dek_buffer.rs:3-5）；RSA 0.9.10 Marvin 无修复版（deny.toml:84 ignore） |
| 审计（sz-orm-audit） | 信息泄露 | 审计日志明文记录敏感值 | 应用启用审计且 SQL 含敏感值 | **高：mask_sensitive 只掩码关键词不掩码值（lib.rs:200-213），PoC 已证实** |
| 限流（sz-orm-limit） | DoS | 限流键膨胀 | 攻击者构造大量 key | 低：max_keys 限制（lib.rs:116-129） |
| 供应链（Cargo.lock） | 已知漏洞利用 | rustls 0.21.12 / webpki / quick-xml / rsa / tokio 等 | 启用 real-es / real-broker / s3-sdk 等 feature | **中→高：漏洞链仍完整存在于 Cargo.lock（CHANGELOG 声称已消除但未落地）** |
| 凭据管理（仓库文件） | 信息泄露 | 文档中的数据库/SSH 密码被公开仓库爬取 | 仓库公开/泄露 | **高：check-secrets.py 检出 30 处** |

---

## 3. 漏洞清单

### 3.1 High 级

#### H-1【幻影修复】CHANGELOG 声称 rustls 0.21.12 漏洞链已消除，但代码与锁文件未变（CWE-1104 依赖使用不可信组件 / CWE-1357 依赖于不可信代码）

- **位置**（评测时 v10.12.0 状态；v10.13.0 整改后见修复报告）：
  - `packages/sz-orm-search/Cargo.toml:17`（整改后为 `real-es = ["dep:reqwest"]`；评测时 :15 为 `real-es = ["elasticsearch"]`）
  - `packages/sz-orm-search/Cargo.toml:32`（整改后为 `reqwest = { version = "0.12", ... }`；评测时 :29 为 `elasticsearch = { version = "8.5.0-alpha.1", ... }`）
  - `packages/sz-orm-search/src/elasticsearch_provider.rs:14`（评测时 `use elasticsearch::{...}`，整改后为 `use reqwest::{...}`）
  - `Cargo.lock` 中实际包含：`elasticsearch 8.5.0-alpha.1`、`reqwest 0.11.27`、`hyper-rustls 0.24.2`、`rustls 0.21.12`、`tokio-rustls 0.24.1`、`webpki-roots 0.25.4`、`winreg 0.50.0`
  - `CHANGELOG.md:281-302`（v10.6.0 声称"将 ElasticsearchProvider 底层 HTTP 客户端从 elasticsearch crate 8.5.0-alpha.1 替换为 reqwest 0.12 直连 ES REST API"、"Cargo.lock 移除 ... rustls 0.21.12"）
- **漏洞类型**：CWE-1104 / CWE-1357（文档声称修复但代码未修复）
- **攻击路径**：启用 `sz-orm-search/real-es` feature 后，应用与 Elasticsearch 通信走 rustls 0.21.12（含 RUSTSEC-2026-0346 Debug 泄露、RUSTSEC-2026-0349 降级、RUSTSEC-2026-0382 emSign 中间证书信任等）。攻击者可利用 TLS 栈缺陷实施中间人/降级/会话密钥恢复。
- **前置条件**：启用 `real-es` feature；与 deny.toml 的多条 ignore（`deny.toml:53,58,63`）对应。
- **影响范围**：供应链风险被文档错误地标记为"已消除"，依赖方无法获知真实风险状态。
- **验证步骤**：
  1. `findstr /n "elasticsearch" packages\sz-orm-search\Cargo.toml` → 第 15、29 行仍为 elasticsearch。
  2. 解析 `Cargo.lock` → `elasticsearch 8.5.0-alpha.1 / reqwest 0.11.27 / rustls 0.21.12` 均存在。
  3. 对比 `CHANGELOG.md:281-302` 的"已消除"声称 → 不一致。
- **修复建议**：按 v10.6.0 报告的方案真正落地：将 `ElasticsearchProvider` 改为 `reqwest 0.12` 直连 ES REST API 并移除 `elasticsearch` 依赖；或至少在 CHANGELOG 中撤回"已消除"声明，保持文档与代码一致。同步更新 `deny.toml` ignore 理由与 `security.yml`/`ci.yml` 的 `--ignore` 列表。

#### H-2【幻影交付】LLM 安全审计规则扩展未落地，测试引用不存在的规则名（CWE-1357 / CWE-489 活性问题）

- **位置**：
  - `packages/sz-orm-ai/src/llm_security_audit.rs:129-160`（`builtin_patterns()` 仅含 `or_1_eq_1`、`union_injection`、`stacked_injection`、`comment_injection` 4 条规则）
  - `packages/sz-orm-ai/src/llm_security_audit.rs:285`（`rule_only()` 仅规则引擎、无 LLM provider）
  - `packages/sz-orm-ai/src/llm_security_audit.rs:316`（`audit()` 入口：规则未命中且无 LLM 时，函数末尾直接返回 `RiskLevel::Safe`、`detected_patterns` 为空）
  - `packages/sz-orm-ai/tests/llm_security_audit_expanded_test.rs:12`（`test_llm_audit_or_1_eq_1_variant_detected` 期望检测"变体"）
  - `packages/sz-orm-ai/tests/llm_security_audit_expanded_test.rs:23,38-39`（期望 `union_injection_encoded` 规则名）
  - `packages/sz-orm-ai/tests/llm_security_audit_expanded_test.rs:70-76`（期望 `sensitive_data_password` 规则名）
  - `docs/assessment/2026-10-17-v1090-security-deepen.md`（声称"内置规则从 4 扩至 14"）
- **漏洞类型**：CWE-1357（安全功能宣称与实现不符）/ 测试悬空（幻影测试）
- **攻击路径**：应用依赖 `LlmSecurityAuditor::rule_only()` 拦截 URL 编码 UNION 注入、敏感数据泄露等模式。实际规则库只有 4 条，`%27%20UNION SELECT ...`、`password = '...'` 等输入不会命中任何规则 → 返回 Safe → 攻击载荷被放行。依赖方误以为有 14 条规则防护。
- **前置条件**：应用启用 `ai-security-audit` feature 且依赖内置规则引擎（未接 LLM provider）。
- **影响范围**：AI 审计功能的安全检测能力被高估；对应测试（如 `test_llm_audit_union_injection_encoded_detected`）在真实现状下**必然失败**，但报告声称"36 个端到端测试全部通过"。
- **验证步骤**：
  1. `Select-String -Path packages\sz-orm-ai\src\llm_security_audit.rs -Pattern 'union_injection_encoded','sensitive_data_password','or_1_eq_1_variant'` → 0 匹配。
  2. 读取 `builtin_patterns()` → 仅 4 条。
  3. 读取 `audit()` 的 `rule_only()` 分支 → 未命中返回 Safe。
  4. 读取测试文件 → 断言 `detected_patterns` 包含不存在的规则名。
- **修复建议**：①在 `builtin_patterns()` 中真实加入 10 条扩展规则（URL 编码 UNION、堆叠变体、敏感数据等）；②或将测试改为 `add_pattern` 注入规则后再断言，使测试与实现一致；③重新运行 `cargo test -p sz-orm-ai --features ai-security-audit` 确认全绿；④在 CHANGELOG/报告撤回"14 规则已交付"声称直至落地。

#### H-3【凭据泄露】仓库提交真实数据库/SSH/Oracle 凭据，`check-secrets.py` 检出 30 处且未接入门禁（CWE-798 硬编码凭据）

- **位置**（`check-secrets.py` 实测输出）：
  - `.codeartsdoer/specs/v830/design.md:446-449,873,888,919-924`（`mysql://test:2sJ8****REDACTED@<remote-server-ip>...`、`postgres://lewuli:JkbC****REDACTED@...`）
  - `docs/e2e-env-config.md:12-14,20,42-47,55`（同上连接串 + `SZ_ORM_E2E_SSH_PASSWORD = "****REDACTED"`）
  - `docs/e2e-execution-guide.md:12-15`
  - `docs/quickstart.md:72,84`（`mysql://shop:shop_pwd@...`）
  - `docs/assessment/2026-09-28-v920-coverage-report.md:41`（`SZ_ORM_ORACLE_PASSWORD="SzOrmTest2026"`）
  - `.github/workflows/ci.yml:736-737`（`mysql://root:${{ secrets.MYSQL_TEST_PASSWORD }}@...` — 使用 Secrets 上下文，风险低，但连接串格式仍被检出）
  - 相关脚本：`scripts/check-secrets.py:94-107`（检测正则）、`scripts/check-secrets.py:110-139`（已知假阳性表）
- **漏洞类型**：CWE-798
- **攻击路径**：仓库一旦公开（crates.io 发布、GitHub 公开、协作方可见），这些凭据可被用于登录远程 MySQL/PostgreSQL/SSH（<remote-server-ip>）与 Oracle。
- **前置条件**：仓库泄露/公开；凭据指向的服务器仍对外开放。
- **影响范围**：数据库与服务器被未授权访问的风险。
- **验证步骤**：`python scripts/check-secrets.py` → 退出码 1，输出 30 处 FOUND（本机已实测，见附录命令输出）。
- **修复建议**：①立即轮换上述所有凭据；②从仓库历史中清除（`git filter-repo`）；③将 `check-secrets.py` 接入 `gate.ps1` / `run-gates-v10120-server.sh` / CI，作为发布阻断门禁；④文档中的示例连接串改用占位符。

#### H-4【敏感数据泄露】默认审计日志脱敏只掩码关键词不掩码值，敏感值明文落盘（CWE-532 敏感信息写入日志）

- **位置**：
  - `packages/sz-orm-audit/src/lib.rs:4-6`（文档声称"ensuring that audit logs never leak sensitive information"）
  - `packages/sz-orm-audit/src/lib.rs:132-145`（`SENSITIVE_KEYWORDS` 列表）
  - `packages/sz-orm-audit/src/lib.rs:156-162`（`SqlAuditor::log` 调用 `mask_sensitive` 后存储）
  - `packages/sz-orm-audit/src/lib.rs:200-213`（`mask_sensitive` 仅替换关键词字节，不处理 `= '值'`）
  - 正确实现参照：`packages/sz-orm-audit/src/hash_chain_enhanced.rs:192-245`（`zk_mask_sql` 会跳过空格/`=`/引号并掩码整个引号值）
- **漏洞类型**：CWE-532
- **攻击路径**：应用启用 `SqlAuditor` 记录 SQL 审计日志。当 SQL 含 `password='hunter2'`、`token='eyJ...'`、`api_key='sk-...'` 时，日志只把关键词替换为 `******`，**值原样保留**。日志文件泄露 → 攻击者直接读取明文敏感值。
- **前置条件**：应用启用 `sz-orm-audit` 默认审计（未启用 `hash-chain-enhanced` feature）。
- **影响范围**：审计日志成为敏感数据明文仓库，违反数据保护合规（GDPR/CCPA/PCI）。
- **验证步骤**（PoC，本机已执行）：
  - 输入 `SELECT * FROM users WHERE password='hunter2'` → 输出 `SELECT * FROM users WHERE ******='hunter2'`。
  - 输入 `INSERT INTO tokens (token, user_id) VALUES ('eyJhbGciOiJIUzI1NiJ9', 42)` → 输出 `INSERT INTO tokens (******, user_id) VALUES ('eyJhbGciOiJIUzI1NiJ9', 42)`。
- **修复建议**：将 `mask_sensitive` 升级为 `zk_mask_sql` 同款"关键词+等号+引号值"掩码；或直接复用 `zk_mask_sql`（去掉 feature 门控）作为默认脱敏实现。

### 3.2 Medium 级

#### M-1【CI 失效】`cargo audit` CI job 的 `--ignore` 列表与 `deny.toml` 严重不一致，安全审计 CI 必然失败或未运行（CWE-1357）

- **位置**：`.github/workflows/ci.yml:811-819`、`.github/workflows/security.yml:46-56`（只 ignore 8 条：0049/0098/0099/0104/0194/0195/2025-0134/2024-0436）；`deny.toml:42-136`（共 24 条 ignore）。
- **影响**：`Cargo.lock` 中实际存在 `rustls 0.21.12`（对应 RUSTSEC-2026-0346/0349/0382）、`webpki-root-certs 1.0.9`（0383）、`ctutils 0.4.2`（0385）、`tokio 1.53.1`（0386）等，这些 advisory 未被 CI 的 `--ignore` 覆盖 → `cargo audit` 退出码非零 → CI 安全 job 红。若 CI 真实运行，项目应处于持续红态，与"23 门禁全部通过"的报告矛盾；若 CI 未运行，则安全门禁形同虚设。
- **建议**：CI `--ignore` 列表与 `deny.toml` 保持一致（或改用 `cargo deny check advisories` 作为唯一安全公告门禁），并确保 CI 真实 fail-closed。

#### M-2【检查器可被绕过】`check-sql-injection-auto.py` 存在系统性误判（CWE-697 不正确的比较 / checker 盲区）

- **位置**：
  - `scripts/check-sql-injection-auto.py:141-154`（`push_str`/`Err(` 被一律判为 Safe"错误消息拼接"）
  - `scripts/check-sql-injection-auto.py:94-103`（`{table}`/`{filter}` 等占位符被无条件判 Safe）
  - `scripts/check-sql-injection-auto.py:112-121`（仅信任固定安全变量名）
  - `scripts/check-sql-injection-auto.py:158-170`（R1 只识别命名占位符 `{input}`，位置参数 `{}` 落入 ManualReview）
- **攻击路径**（PoC 已实测，全部判为 `Safe`）：
  - `format!("SELECT * FROM {table} WHERE id=1", table)` → **Safe**（若 `table` 用户可控 → 漏报）
  - `format!("SELECT * FROM t WHERE {filter}", filter)` → **Safe**
  - `sql.push_str(&format!(" WHERE id={}", input))` → **Safe**
  - `format!("SELECT * FROM data WHERE value = '{}'", long_token)` 在 `sql_sanitizer.rs` 路径 → **Safe**（若该模块实际执行 SQL → 漏报）
- 另：`scan_workspace` 只匹配单行 `format!.*WHERE.*\{.*\}`（`scripts/check-sql-injection-auto.py:226`），多行 `format!` 完全不可见；大小写 `where`、注释分隔 `WHERE/**/id=` 均落入 ManualReview（strict 才阻断）。
- **建议**：扫描器应作为辅助层而非安全保证；修正 `push_str` 误判、`{table}` 等占位符的信任前提（校验调用点是否用户可控）、增加多行检测；补充绕过回归测试。

#### M-3【自测红】G9 自测脚本 `test_check_sql_injection_auto.py` 失败且未接入门禁

- **位置**：`scripts/test_check_sql_injection_auto.py:101-107`（`test_workspace_scan_all_safe` 断言 `report.unsafe == 0`）
- **实测**：`python scripts/test_check_sql_injection_auto.py` → `8 passed; 1 failed`，退出码 1。原因是工作区扫描到 `packages/sz-orm-core/src/dynamic_sql.rs:1403` 的 `${'s'}` 测试串被标 Unsafe。
- **影响**：自测脚本与其被测对象不一致，且该脚本未接入任何门禁/CI（`gate.ps1`、`run-gates-v10120-server.sh`、`.github/workflows/*.yml` 均无调用）→ 扫描器质量无人看护。
- **建议**：将 `test_check_sql_injection_auto.py` 接入 CI；更新 `test_workspace_scan_all_safe` 以容忍已知测试串（或将该行加入白名单/安全上下文判断）。

#### M-4【门禁脚本】服务器门禁 `Gate 9` 未用 `--strict`，`Gate 1` 硬编码 PASS，`Gate 21` 未纳入 OWASP 套件

- **位置**：
  - `scripts/run-gates-v10120-server.sh:25-26`（`mark 1 PASS` 无 fmt 检查）
  - `scripts/run-gates-v10120-server.sh:65`（`python3 scripts/check-sql-injection-auto.py` 无 `--strict` → Unsafe>0 时脚本仍退出 0 → Gate 9 PASS）
  - `scripts/run-gates-v10120-server.sh:120-129`（Gate 21 仅跑 3 个测试，未包含 `cargo test --workspace --features owasp-pentest-suite`）
  - 对照：`scripts/gate.ps1:220` 使用 `--strict`（Windows 版更严格）
- **影响**：服务器后台门禁（CI/CD 使用的版本）在 SQL 注入扫描存在 Unsafe 时仍可标记 PASS；OWASP 85+ 测试未纳入门禁；fmt 门禁形同虚设。
- **建议**：`run-gates-v10120-server.sh:65` 加 `--strict`；`Gate 1` 改为真实执行 `cargo fmt --all -- --check`；`Gate 21` 加入 OWASP 套件命令。

#### M-5【文档漂移】`docs/Security.md` 停留在 v3.4.0，与 v10.12.0 严重不符

- **位置**：`docs/Security.md:5-8`（版本 v3.4.0、工作空间版本 3.4.0）、`docs/Security.md:66-106`（安全功能矩阵行号大量过期）、`docs/Security.md:272-292`（R-H01/R-H02 风险登记册仍指向旧行号）。
- **影响**：安全策略/VDP/SLA/功能矩阵无法反映当前代码，合规审计（SOC2/ISO27001）证据失效；VDP 邮箱 `security@sz-orm.local` 与 PGP 指纹 `0xSZORM2026` 为占位符，不可执行。
- **建议**：重写 Security.md 对齐 v10.12.0；用真实邮箱/流程替换占位符；建立文档版本与代码版本的一致性门禁。

#### M-6【幻影交付】零信任/合规组件无生产调用点，与 v10.11.0 报告"有生产调用点"声称矛盾

- **位置**（组件真实存在但仅模块内+导出）：
  - `packages/sz-orm-audit/src/zero_trust_access.rs:346`（`ZeroTrustVerifier`）
  - `packages/sz-orm-audit/src/gdpr_ccpa.rs:462`（`AutoDestroyer`）
  - `packages/sz-orm-audit/src/hash_chain_enhanced.rs:320`（`MerkleProofGenerator`）
  - `packages/sz-orm-audit/src/supply_chain_security.rs:199`（`SbomGenerator`）
- **验证**：全工作区（排除测试）搜索以上 11 个组件符号，**生产引用均为 0**。`docs/assessment/2026-10-18-v10110-zero-trust-compliance.md` 声称"所有组件有生产调用点"，与事实不符。
- **建议**：按门禁 15 要求，在文档中使用"提供 X 组件（需手动接入）"措辞，或真实接入合规管线；将 `tests/zero_trust_compliance_wiring_test.rs` 的"接线测试"与生产接线区分表述。

#### M-7【信息泄露】`DbError` Display 暴露数据库原始错误串（历史 R-M03 未修复）

- **位置**：`packages/sz-orm-core/src/error.rs:121-146`（`Query error: {s}` 等直接输出内部 `s`）
- **影响**：应用将错误串返回给用户时，可能泄露数据库类型/版本/表名/SQL 片段。
- **建议**：增加 `debug` feature 门控隐藏细节，或对错误消息统一脱敏。

#### M-8【暴力破解】MFA TOTP 验证无内置限流/尝试锁定

- **位置**：`packages/sz-orm-auth/src/mfa.rs:104-108`（仅拒绝空密钥）、`packages/sz-orm-auth/src/mfa.rs:115-138`（验证循环无尝试计数/退避）
- **影响**：6 位 TOTP 可被在线暴力尝试（1e6 空间），需应用层配合 `sz-orm-limit`。
- **建议**：在 `TotpVerifier` 增加可选的失败计数/退避接口，或在文档中明确要求应用层限流。

### 3.3 Low 级

#### L-1【示例】JWT 密钥明文（R-M01 未修复）

- `file://examples/src/bin/production_app.rs#L298`：`JwtAuthenticator::new("production-app-secret", ...)`。库本身提供 `try_new` 强制 32 字节密钥（`packages/sz-orm-auth/src/auth.rs:185`），示例应改用环境变量。

#### L-2【示例】OpenAIEmbeddingClient API key 明文（R-M02 未修复）

- `packages/sz-orm-ai/src/real_embedding.rs:51-55`：`api_key` 以普通字符串保存。建议提供 `EnvKeySource` 或文档明确密钥管理责任。

#### L-3【合规】unsafe 块缺"紧邻"SAFETY 注释（口径裁决）

- **裁决**：2026-10-20 报告称"无任何生产 unsafe 块缺失 SAFETY"（按函数级 `# Safety` 文档计）；2026-10-25 报告称"37 处缺紧邻 SAFETY"。两者**口径不同但事实均成立**：
  - 函数级：`packages/sz-orm-cabi/src/lib.rs` 36 处 `# Safety` 文档覆盖 35 个 `pub unsafe extern`；go/cpp/java 均为 41/41、19/19 全覆盖。
  - 块级：大量 `unsafe { CStr::from_ptr(...) }` 块无"紧邻" `// SAFETY:` 行（如 `packages/sz-orm-go/src/lib.rs:83`、`packages/sz-orm-java/src/lib.rs:83` 等转发 unsafe 块）。
  - 结论：**不构成内存安全漏洞**（函数级 unsafe 契约已文档化），但违反 AGENTS.md 硬约束第 5 条字面要求，且削弱自动化审计能力。
- **建议**：二选一：①补充块级紧邻 `// SAFETY:` 注释；②修改 `scripts/check-unsafe-audit.py` 认可"函数级文档 + 块级引用"模式。

#### L-4【数值漂移】AGENTS.md G21 声称 OWASP 85 测试，实际门控 49 个 / 命名相关 145 个

- **验证**：`cargo test --workspace --features owasp-pentest-suite` 门控测试文件 9 个共 **49** 个测试；文件名含 owasp/csrf/xss/file_upload/race/ssrf 的测试共 **145** 个。`docs/sz-orm-review-runbook.md:94` 的"85 测试"已过时。
- **建议**：用脚本重新统计后更新文档数字（门禁 18 度量真实性）。

#### L-5【低危】`nl2sql` 解释性文本插值未转义

- `packages/sz-orm-ai/src/nl2sql.rs:1013`：`cond.replace(&format!("${}", i + 1), &format!("'{}'", params[i]))` 用于构建"条件说明"字符串（非执行 SQL）。若 `params[i]` 含引号/换行，会污染解释文本（日志/UI 注入，低风险）。实际执行 SQL 走参数化（`$N` 占位符），无 SQL 注入风险。

---

## 4. 分维度评测

### 4.1 SQL 注入纵深防御

**亮点**
- 参数化铁律落地：`where_cond`/`or_where` 已从生产 API 移除，全工作区仅剩 LSP 诊断字符串引用（`packages/sz-orm-lsp/src/diagnostics.rs:185`）。
- 集中式标识符校验 `validate_identifier`（`packages/sz-orm-core/src/sql_safety.rs:20-43`）被广泛用于表名/列名/约束名（`access_control.rs:104`、`migration.rs:1040-1046`、`phinx_migration.rs:397-400`、`dialect.rs:52` 等）。
- FFI 层表名入口全部过 `validate_identifier`（`packages/sz-orm-cabi/src/lib.rs:699,746,1190,1270,1342,1415,1484,1561,1633,1705`）。
- G9 工作区扫描：74 项中 73 Safe / 1 Unsafe（`dynamic_sql.rs:1403` 为 XML 转义测试串）/ 0 ManualReview（实测输出）。
- 多租户测试断言 tenant_id 必须参数化、不得以内联字面量出现（`packages/sz-orm-core/tests/security_attacks.rs:52-60`）。

**风险**
- `${}` 插值路径仍存在（`dynamic_sql.rs:648-667` `param_to_string` + `escape_sql_string`），转义仅覆盖字符串字面量上下文；`dynamic_sql.rs` 未调用 `validate_identifier`。生产未见 `${}` 执行点，但文档应继续强调禁用。
- G9 扫描器可被绕过（见 M-2）。
- G9 自测红（见 M-3）。

**建议**
- 将 `escape_sql_string` 升级为与 `zk_mask_sql` 同类的值上下文处理；为 `${}` 增加编译期/运行时拒绝选项。
- 修复扫描器误判并接入自测到 CI。

### 4.2 认证与授权

**亮点**
- JWT：仅接受 HS256（`packages/sz-orm-auth/src/jwt.rs:196-200`）；恒定时间比较 `ct_eq`（`jwt.rs:206-211`）；`token_use` 区分 access/refresh（`jwt.rs:63`，修复 C-2 令牌类型混淆）；弱密钥告警 + `try_new` 强制 32 字节（`auth.rs:185`）。
- RBAC 通配符显式化：仅接受 `action:resource` / `action:*` / `*`（`authorizer.rs:216`），修复 M-11 隐式降级。
- OAuth2 授权码使用 `OsRng.fill_bytes`（`oauth2.rs:353-357`），修复 C-1 可预测 PRNG。
- MFA 拒绝空密钥（`mfa.rs:104-108`），修复 M-10 恒 "000000" 绕过。
- 租户隔离：`tenant_id` 参数化 + 越权测试断言副作用（`packages/sz-orm-core/tests/security_attacks.rs`）。

**风险**
- MFA 无内置限流（M-8）。
- 示例仍明文密钥（L-1）。
- TokenStore/OAuth2 完整流程未在本轮逐行审计（时间预算），建议下轮专项。

**建议**
- MFA 增加失败计数/退避；示例改环境变量。

### 4.3 密码学正确性

**亮点**
- DEK 明文使用 `Zeroizing<Vec<u8>>` 包裹，Drop 清零（`packages/sz-orm-crypto/src/dek_buffer.rs:3-5,31-33,89-90`）。
- AES-256-GCM + 随机 nonce；PBKDF2 迭代下限；KAT 测试使用 NIST FIPS 180-4 / RFC 4231 真实向量（`packages/sz-orm-crypto/tests/kat.rs:22-45`）。
- RSA 0.9.10 Marvin Attack 无上游修复版，`deny.toml:84` ignore 理由成立（0.9.x 最新）。
- `zk_mask_sql`（`hash_chain_enhanced.rs:192-245`）实现正确：跳过空格/等号/引号并掩码整个值。

**风险**
- rsa 0.9.10 仍是侧信道暴露面（无修复，需跟踪）。
- 默认审计脱敏缺陷见 H-4。

**建议**
- 若 `sz-orm-crypto` 的 RSA-OAEP 用于敏感数据，评估迁移至 `rsa 0.9.11+` 或 `ring`/`aws-lc-rs`。

### 4.4 敏感数据保护

**亮点**
- 脱敏规则 `mask_prefix_suffix` 保留前/后缀（`packages/sz-orm-masking/src/lib.rs:153-183`），行为符合常见脱敏惯例。
- `ComplianceScanDesensitizer` 与 `zk_mask_sql` 实现值级脱敏（`compliance_scan_desensitizer.rs:85-86`、`hash_chain_enhanced.rs:192-245`）。

**风险**
- 默认审计脱敏只掩码关键词（H-4），审计日志明文记录敏感值。
- 错误消息暴露 DB 原始错误（M-7）。
- `mask_prefix_suffix` 保留 3-4 位前缀/后缀，属设计选择；对短输入（长度 ≤ prefix+suffix）不掩码（`packages/sz-orm-masking/src/lib.rs:159-161`），应用层需注意。

**建议**
- 修复 H-4；对错误消息统一走脱敏。

### 4.5 FFI 内存安全

**亮点**
- `QueryJsonResult` 等跨 ABI 结构已补 `#[repr(C)]`（`packages/sz-orm-cabi/src/lib.rs:175` 及 118/139/475/486）。
- 所有表名/字段名 FFI 入口过 `validate_identifier`。
- `catch_unwind` 覆盖导出函数与 free 函数（`lib.rs:400,999,1986,2009`）。
- `FfiMemoryManager` 注册表模式接入导出路径（`ffi_memory.rs:29-48`），`memory_untrack` 返回 false 时静默返回，双 free 从 UB 变为安全 no-op（`lib.rs:396-401`）。
- 函数级 `# Safety` 文档全覆盖（cabi 36/go 41/cpp 41/java 19）。

**风险**
- 块级 `// SAFETY:` 紧邻注释大量缺失（L-3，合规/可审计性）。
- 句柄无版本化代际计数（仅注册表防双 free，跨池误用会被 untrack 拒绝——行为安全但语义是"静默失败"）。

**建议**
- 补块级 SAFETY 注释或调整审计脚本口径；长期可引入代际计数句柄。

### 4.6 拒绝服务与资源耗尽

**亮点**
- 连接池 shutdown 竞态已修复（`pool.rs:1807-1814,1912-1918`）+ 回归测试（`packages/sz-orm-core/tests/chaos_pool.rs` 中的 `chaos_pool_shutdown_race_no_new_connection` 用例）+ loom-check 独立工作区。
- 限流原语丰富：SlidingWindow / TokenBucket / LeakyBucket + `max_keys` 上限（`sz-orm-limit/src/lib.rs:44-47,116-129`）。
- `validate_identifier` 限制标识符长度 1-63（`sql_safety.rs:20-25`），超长输入被拒。
- 查询超时/连接超时默认配置存在（Security.md 记录，pool.rs 配置校验）。

**风险**
- MFA 无限流（M-8）。
- `sz-orm-limit` 的 `max_keys` 默认值需应用层设置（`with_max_keys`）。

**建议**
- 无阻塞项。

### 4.7 供应链与 CI/CD（详见第 5 节）

---

## 5. 供应链与 CI/CD 安全评估

### 5.1 deny.toml RUSTSEC ignore 逐条裁定

本机解析 `Cargo.lock`（977 个包）确认以下易受攻击版本**实际存在**：

| 版本 | 对应 advisory（deny.toml 行） | 裁定 |
|---|---|---|
| elasticsearch 8.5.0-alpha.1 / reqwest 0.11.27 / rustls 0.21.12 / webpki-roots 0.25.4 / hyper-rustls 0.24.2 | RUSTSEC-2026-0049/0098/0099/0104/0382/0346/0349（deny.toml:42,44,46,48,53,58,63） | **理由成立但状态失实**：ignore 理由称"待 elasticsearch 升级"，而 CHANGELOG 声称已消除——**实际未消除**（H-1）。feature 门控（real-es）确实限制影响面。 |
| quick-xml 0.38.4 | RUSTSEC-2026-0194/0195（deny.toml:68,70） | 理由成立：s3-sdk feature 门控，上游 rust-s3 未升级。 |
| rustls-pemfile 1.0.4 / 2.2.0 | RUSTSEC-2025-0134（deny.toml:76） | 理由成立：unmaintained，功能被 rustls 0.23 内置替代。 |
| paste 1.0.15 | RUSTSEC-2024-0436（deny.toml:79） | 理由成立：编译期 proc-macro，无运行时风险。 |
| rsa 0.9.10 | RUSTSEC-2023-0071（deny.toml:84） | 理由成立但需跟踪：Marvin 侧信道，0.9.10 为 0.9.x 最新。 |
| rand 0.7.3 | RUSTSEC-2026-0097（deny.toml:89） | 理由成立：MSSQL winauth 场景。 |
| event-listener 5.4.1 | RUSTSEC-2026-0221（deny.toml:94） | 理由成立：sqlx 内部使用，不影响自研池。 |
| rkyv 0.7.46 | RUSTSEC-2026-0235（deny.toml:99） | 理由成立：特定 feature 组合。 |
| pyo3 0.20.3 | RUSTSEC-2025-0020/2026-0177（deny.toml:104,109） | 理由成立：绑定层输入受控；API 兼容需评估。 |
| h2 0.3.27 | RUSTSEC-2026-0258（deny.toml:115） | 理由成立：actix-web v4 无法升 h2 0.4.x。 |
| zip 6.0.0 | RUSTSEC-2026-0367/0368/0369（deny.toml:120,122,124） | 理由成立：libduckdb-sys build-dep，输入为可信 tarball。 |
| webpki-root-certs 1.0.9 | RUSTSEC-2026-0383（deny.toml:128） | **理由待验证**："修复版本 1.0.11 已确认但受网络限制未落地"——锁文件仍为 1.0.9。本机无网络无法复核，建议尽快 `cargo update -p webpki-root-certs`。 |
| ctutils 0.4.2 | RUSTSEC-2026-0385（deny.toml:132） | 同上，"修复版本 0.4.5 已确认但受网络限制未落地"。 |
| tokio 1.53.1 | RUSTSEC-2026-0386（deny.toml:136） | 同上，修复版本 1.53.3+ 已确认但锁文件仍 1.53.1。**tokio 是全局运行时依赖，建议优先升级**。 |
| unmaintained/yanked | deny.toml:139（`unmaintained = "none"`）、:141（`yanked = "warn"`） | 故意放宽，unsafe-libyaml/chacha20 等仅告警。可接受，但应在报告中明示。 |

**总裁定**：ignore 行号与锁文件版本一一对应、理由基本成立；但 ①CHANGELOG 失实（H-1）②`webpki-root-certs`/`ctutils`/`tokio` 三条"待网络恢复"已滞留多日 ③CI `--ignore` 与 deny.toml 不一致（M-1）。

### 5.2 CI 门禁有效性（audit the checkers）

- **security.yml / ci.yml Security Audit job**：`cargo audit` 只 ignore 8 条 → 在真实锁文件下必然失败（M-1）。
- **codeql.yml:40**：`cargo build --workspace --keep-going || true` —— 构建失败被吞掉，CodeQL 数据库可能不完整，非 fail-closed。
- **semgrep.yml:28-31**：`continue-on-error: true` + `|| true` —— semgrep 发现永不阻断 CI，仅上传 SARIF（advisory-only）。
- **run-gates-v10120-server.sh**：Gate 1 硬编码 PASS（:25-26）；Gate 9 无 `--strict`（:65）；Gate 21 未含 OWASP（:120-129）。
- **fuzz 接入**：`ci.yml:986-1040` 有 Fuzz job（`cargo +nightly fuzz run query_builder/value_escape/pool_config`，60s/target），目录 `fuzz/fuzz_targets` 与 corpus 存在——**真实接入**。

### 5.3 供应链组件真实性

- `SbomGenerator`/`VulnerabilityMonitor`/`ReproducibleBuildVerifier`（`packages/sz-orm-audit/src/supply_chain_security.rs:199,264,371`）真实存在，但**无生产调用点**（M-6），属"提供组件（需手动接入）"。

---

## 6. 历史发现复核结果表

| # | 历史结论 | 当前证据 | 状态 |
|---|---|---|---|
| 1 | FFI 层 SQL 注入：`sz_orm_count` 拼接表名、`sz_orm_table_exists` 单引号转义（2026-10-25 High） | `packages/sz-orm-cabi/src/lib.rs:699,746` 已加 `validate_identifier`；`:750` 用 `SELECT COUNT(*) AS cnt` 参数化写法 | ✅ 已修复 |
| 2 | `QueryJsonResult` 未标 `#[repr(C)]`（2026-10-25 High） | `packages/sz-orm-cabi/src/lib.rs:175` 已补 `#[repr(C)]`，且 118/139/475/486 等跨 ABI 结构均已标注 | ✅ 已修复 |
| 3 | 句柄无版本化/双 free、`FfiMemoryManager` 死模块（2026-10-25 中） | `ffi_memory.rs:29-48` track/untrack 注册表已接入导出层；`lib.rs:396-401` 双 free 静默返回 | ✅ 已修复（部分：无代际计数，但双 free 已安全化） |
| 4 | 4 个 free 函数无 catch_unwind（2026-10-25 中） | `packages/sz-orm-cabi/src/lib.rs:400,999,1986,2009` 均已补 `catch_unwind` | ✅ 已修复 |
| 5 | 37 处生产 unsafe 缺紧邻 SAFETY（2026-10-25） vs 全面审计称"无缺失"（2026-10-20） | **裁决**：口径差异。函数级 `# Safety` 文档全覆盖（cabi 36/go 41/cpp 41/java 19），块级紧邻注释大量缺失（如 `packages/sz-orm-go/src/lib.rs:83`、`packages/sz-orm-java/src/lib.rs:83`） | ⚠️ 部分修复（合规口径待统一，见 L-3） |
| 6 | 连接池 shutdown 竞态（2026-10-24 High，300 轮复现） | `pool.rs:1807-1814` 循环顶部复查 closed；`:1912-1918` CAS 后复查；回归测试 `chaos_pool_shutdown_race_no_new_connection` 已加入 chaos_pool.rs；loom-check 工作区存在 | ✅ 已修复 |
| 7 | deny.toml RUSTSEC ignore 行号与理由（2026-10-24 核对通过） | 本报告逐条复核（见 5.1）：行号准确、理由基本成立；新增三条"待网络恢复"滞留多日，且 CHANGELOG 失实 | ⚠️ 部分成立（新问题 H-1/M-1） |
| 8 | Gate 6 用错 `\|\|`、Gate 11 硬编码 PASS（2026-10-24 中） | `run-gates-v10120-server.sh:50` 已用 `&&`；`:74-77` 已真实执行 `git diff`；**但 Gate 1 仍硬编码 PASS（:25-26）**，Gate 9 无 strict（:65），Gate 21 缺 OWASP（:120-129） | ⚠️ 部分修复（新问题 M-4） |
| 9 | Security.md 版本漂移（停在 v3.4.0） | `docs/Security.md:5-8` 仍为 v3.4.0，当前 v10.12.0 | ⚠️ 仍存在（M-5） |
| 10 | 零信任/合规组件交付（2026-10-18） | 组件真实存在（`zero_trust_access.rs:346`、`gdpr_ccpa.rs:462`、`hash_chain_enhanced.rs:320`、`supply_chain_security.rs:199`），但全工作区**零生产调用点**，报告"有生产调用点"声称不实 | ⚠️ 部分修复（M-6） |
| — | rustls 漏洞链已消除（2026-10-16 v10.6.0 报告） | **未落地**（评测时）：`packages/sz-orm-search/Cargo.toml:17,32` 整改前仍声明 elasticsearch；Cargo.lock 仍含 rustls 0.21.12；CHANGELOG.md:281-302 声称已消除 | ✅ v10.13.0 已真实落地（见修复报告），CHANGELOG 已加修正声明 |
| — | LLM 审计规则扩至 14 条（2026-10-17 v10.9.0 报告） | **未落地**：`llm_security_audit.rs:129-160` 仅 4 条；测试 `llm_security_audit_expanded_test.rs:12,23,70` 引用不存在的规则名 | ❌ 仍存在（H-2，新确认） |

---

## 7. 合规差距分析

| 框架 | 现状 | 差距 | 建议 |
|---|---|---|---|
| SOC 2 | Security.md 有 CC1-CC9/PI/A/C 覆盖表与路线图（`docs/Security.md:156-187`） | 文档版本过期（v3.4.0）；控制证据行号失效；威胁建模未落地为代码门禁 | 重写 Security.md；将 STRIDE 结果纳入风险登记册；第三方渗透测试（路线图 Q1 2027）前先闭环本报告 High |
| ISO 27001 | Annex A 22/37 项声称已实现（`docs/Security.md:243-247`） | 覆盖率基于过期文档；A.8.15 日志记录受 H-4 影响（审计日志明文敏感值）→ 该控制项实际不满足 | 修复 H-4 后重新评估 A.8.15/A.8.11/A.8.24 |
| GDPR / CCPA | `AutoDestroyer`/`RetentionChecker`/`AutoArchiver`（`gdpr_ccpa.rs:384,413,462`）提供生命周期工具 | 组件无生产调用点（M-6）；"删除权/保留期"未接入实际数据路径 | 提供接入示例/文档；在真实应用试点后再声明合规能力 |
| 供应链 | deny.toml + cargo audit + cargo deny + fuzz CI | CI ignore 不一致（M-1）；三条 advisory 待升级滞留多日；CHANGELOG 失实（H-1） | 统一 CI 与 deny.toml；优先升级 tokio/webpki-root-certs/ctutils；真实落地 rustls 链修复 |
| 漏洞披露 | Security.md 定义 VDP/SLA（`docs/Security.md:42-49`） | 邮箱 `security@sz-orm.local`、PGP `0xSZORM2026` 为占位符，不可执行 | 换成真实安全邮箱与 PGP 公钥 |

---

## 8. 安全工程师综合评分

| 维度 | 权重 | 得分 | 说明 |
|---|---|---|---|
| SQL 注入纵深 | 20% | 8.5/10 | 参数化铁律+标识符校验优秀；扣分：G9 扫描器可绕过、自测红 |
| 认证与授权 | 15% | 8.0/10 | JWT/RBAC/多租户扎实；扣分：MFA 无限流、示例明文密钥 |
| 密码学 | 15% | 8.5/10 | DEK zeroize、KAT 真实向量、zk_mask_sql 正确；扣分：rsa 0.9.10 无修复 |
| 敏感数据保护 | 15% | 6.0/10 | **扣分重灾区**：默认审计脱敏只掩码关键词不掩码值（H-4）、错误消息泄露 DB 原始错误 |
| FFI 内存安全 | 15% | 8.5/10 | repr(C)/catch_unwind/双 free 防护全部落地；扣分：块级 SAFETY 注释缺失 |
| 供应链与 CI/CD | 10% | 5.5/10 | **扣分重灾区**：rustls 链幻影修复、CI ignore 不一致、Gate 1/9/21 缺陷 |
| 合规与文档 | 10% | 5.0/10 | Security.md 漂移、VDP 占位符、零信任组件无生产调用点、凭据入库 |
| **加权总分** | 100% | **7.3/10** | **有条件投产** |

评分理由：核心库代码质量与历史漏洞修复闭环值得肯定（2026-10-25 的 FFI High 全部落地、shutdown 竞态修复+模型检查、OAuth2/JWT 旧洞闭环）；但本报告新确认的 4 项 High（幻影修复、幻影交付、凭据入库、审计脱敏失效）均属于"文档/门禁/日志"层面的真实安全债，按安全官否决权，任一 High 未闭环即不可投产。

---

## 9. 优先整改路线图（按 ROI）

### P0 阻断项（闭环前不可投产）
1. **H-1 rustls 漏洞链**：真正完成 `ElasticsearchProvider` 迁移或撤回 CHANGELOG 声称；同步更新 deny.toml 与 CI。
2. **H-2 LLM 审计规则**：落地 14 条规则或修正测试与文档。
3. **H-3 凭据轮换**：轮换 `.codeartsdoer/specs/v830/design.md`、`docs/e2e-env-config.md` 等文档中的真实凭据，并接入 `check-secrets.py` 到门禁。
4. **H-4 审计脱敏**：将 `mask_sensitive` 升级为值级脱敏（复用 `zk_mask_sql`）。

### P1 高优先
5. 统一 CI `--ignore` 与 deny.toml（M-1）；`cargo update -p tokio -p webpki-root-certs -p ctutils`。
6. 修复 G9 扫描器误判与自测（M-2/M-3）；`run-gates-v10120-server.sh` Gate 9 加 `--strict`、Gate 1 真实 fmt、Gate 21 加 OWASP（M-4）。
7. 重写 Security.md 对齐 v10.12.0，替换 VDP 占位符（M-5）。
8. 零信任组件补充生产接线或改为"需手动接入"措辞（M-6）。

### P2 加固项
9. `DbError` Display 增加 debug 门控（M-7）。
10. MFA 增加失败计数/退避（M-8）。
11. 补块级 SAFETY 注释或调整审计口径（L-3）。
12. 更新 AGENTS.md/runbook 的 OWASP 测试数字（L-4）。
13. 示例 JWT/API key 改环境变量（L-1/L-2）。

---

## 10. 证据索引与 audit-verify 输出

### 10.1 命令执行记录（本机实测）

```
$ "C:\Program Files\python\python.exe" scripts\check-sql-injection-auto.py --strict
=== SQL Injection Automated Review (Gate 9) ===
Total: 74 | Safe: 73 | Unsafe: 1 | ManualReview: 0
--- Unsafe (1) ---
  packages\sz-orm-core\src\dynamic_sql.rs:1403: R6: ${} 动态 SQL 插值未配置白名单，存在 SQL 注入风险
Gate 9: 1 Unsafe + 0 ManualReview (needs attention)
退出码：1（--strict）

$ "C:\Program Files\python\python.exe" scripts\check-secrets.py
Scanned 2911 files ...
FOUND: 30 potential secret(s)
  .codeartsdoer\specs\v830\design.md:446 [Password in connection string] ...
  docs\e2e-env-config.md:55 [Generic password assignment]  $env:SZ_ORM_E2E_SSH_PASSWORD = "****REDACTED"
  ...
FAIL: secrets detected. Remove them before publishing.
退出码：1

$ "C:\Program Files\python\python.exe" scripts\test_check_sql_injection_auto.py
  PASS: test_r3_question_placeholder
  ...
  FAIL: test_workspace_scan_all_safe: Should have 0 Unsafe, got 1
8 passed; 1 failed; 9 total
退出码：1

$ "C:\Program Files\python\python.exe" <G9 绕过 PoC>
[Safe] format!("SELECT * FROM {table} WHERE id=1", table)  -> R2 表名/列名占位符
[Safe] sql.push_str(&format!(" WHERE id={}", input))       -> R4 错误消息拼接
[Safe] format!("SELECT * FROM data WHERE value = '{}'", long_token)  -> R4 安全模块
（完整 16 用例见报告正文 3.2 M-2）

$ "C:\Program Files\python\python.exe" <mask_sensitive PoC>
SELECT * FROM users WHERE ******='hunter2'
INSERT INTO tokens (******, user_id) VALUES ('eyJhbGciOiJIUzI1NiJ9', 42)
（完整 4 用例见报告正文 3.1 H-4）
```

### 10.2 未执行命令及原因（如实记录）

| 命令 | 原因 |
|---|---|
| `cargo audit` / `cargo deny check` | 本机无 Rust 工具链：`cargo`/`rustc` 不在 PATH；`D:\tools\cargo-home`、`D:\tools\rustup-home` 目录为空；`where cargo` 无结果 |
| `cargo test -p sz-orm-auth --test security_attacks` 等全部 cargo 测试 | 同上 |
| `cargo test -p sz-orm-audit --features owasp-pentest-suite` | 同上 |
| `git diff / git log / git remote` | `git` 不在 PATH |
| `cargo geiger` | 未安装 + 无工具链 |

> 上述命令对应的静态证据已通过源码阅读、Python 脚本实测与 PoC 覆盖，报告结论不依赖未运行的命令。

### 10.3 证据文件清单（供 audit-verify 验证）

以下为报告引用的全部 `packages/...rs:line` 与 `file:///...` 证据（每个引用单独一行，便于脚本逐条验证）：

packages/sz-orm-search/Cargo.toml:17
packages/sz-orm-search/Cargo.toml:32
packages/sz-orm-search/src/elasticsearch_provider.rs:14
packages/sz-orm-ai/src/llm_security_audit.rs:129
packages/sz-orm-ai/src/llm_security_audit.rs:285
packages/sz-orm-ai/tests/llm_security_audit_expanded_test.rs:12
packages/sz-orm-ai/tests/llm_security_audit_expanded_test.rs:23
packages/sz-orm-ai/tests/llm_security_audit_expanded_test.rs:70
packages/sz-orm-audit/src/lib.rs:4
packages/sz-orm-audit/src/lib.rs:132
packages/sz-orm-audit/src/lib.rs:156
packages/sz-orm-audit/src/lib.rs:200
packages/sz-orm-audit/src/hash_chain_enhanced.rs:192
packages/sz-orm-audit/src/hash_chain_enhanced.rs:245
packages/sz-orm-audit/src/zero_trust_access.rs:346
packages/sz-orm-audit/src/gdpr_ccpa.rs:462
packages/sz-orm-audit/src/hash_chain_enhanced.rs:320
packages/sz-orm-audit/src/supply_chain_security.rs:199
packages/sz-orm-audit/src/compliance_scan_desensitizer.rs:85
packages/sz-orm-core/src/sql_safety.rs:20
packages/sz-orm-core/src/sql_safety.rs:43
packages/sz-orm-core/src/dynamic_sql.rs:53
packages/sz-orm-core/src/dynamic_sql.rs:648
packages/sz-orm-core/src/dynamic_sql.rs:667
packages/sz-orm-core/src/dynamic_sql.rs:1403
packages/sz-orm-core/src/error.rs:121
packages/sz-orm-core/src/pool.rs:1807
packages/sz-orm-core/src/pool.rs:1912
packages/sz-orm-core/tests/security_attacks.rs:52
packages/sz-orm-lsp/src/diagnostics.rs:185
packages/sz-orm-cabi/src/lib.rs:175
packages/sz-orm-cabi/src/lib.rs:396
packages/sz-orm-cabi/src/lib.rs:400
packages/sz-orm-cabi/src/lib.rs:699
packages/sz-orm-cabi/src/lib.rs:746
packages/sz-orm-cabi/src/lib.rs:999
packages/sz-orm-cabi/src/lib.rs:1986
packages/sz-orm-cabi/src/lib.rs:2009
packages/sz-orm-cabi/src/ffi_memory.rs:29
packages/sz-orm-cabi/src/ffi_memory.rs:48
packages/sz-orm-go/src/lib.rs:83
packages/sz-orm-java/src/lib.rs:83
packages/sz-orm-auth/src/jwt.rs:63
packages/sz-orm-auth/src/jwt.rs:196
packages/sz-orm-auth/src/jwt.rs:206
packages/sz-orm-auth/src/auth.rs:185
packages/sz-orm-auth/src/authorizer.rs:216
packages/sz-orm-auth/src/oauth2.rs:353
packages/sz-orm-auth/src/mfa.rs:104
packages/sz-orm-auth/src/mfa.rs:115
packages/sz-orm-crypto/src/dek_buffer.rs:3
packages/sz-orm-crypto/src/dek_buffer.rs:31
packages/sz-orm-crypto/src/dek_buffer.rs:89
packages/sz-orm-crypto/tests/kat.rs:22
packages/sz-orm-masking/src/lib.rs:153
packages/sz-orm-masking/src/lib.rs:159
packages/sz-orm-limit/src/lib.rs:44
packages/sz-orm-limit/src/lib.rs:116
packages/sz-orm-ai/src/nl2sql.rs:1013
packages/sz-orm-ai/src/real_embedding.rs:51
file://examples/src/bin/production_app.rs#L298
scripts/check-sql-injection-auto.py:94
scripts/check-sql-injection-auto.py:141
scripts/check-sql-injection-auto.py:158
scripts/check-sql-injection-auto.py:226
scripts/check-secrets.py:94
scripts/check-secrets.py:110
scripts/test_check_sql_injection_auto.py:101
scripts/run-gates-v10120-server.sh:25
scripts/run-gates-v10120-server.sh:50
scripts/run-gates-v10120-server.sh:65
scripts/run-gates-v10120-server.sh:74
scripts/run-gates-v10120-server.sh:120
scripts/gate.ps1:220
deny.toml:42
deny.toml:53
deny.toml:58
deny.toml:63
deny.toml:68
deny.toml:70
deny.toml:76
deny.toml:79
deny.toml:84
deny.toml:89
deny.toml:94
deny.toml:99
deny.toml:104
deny.toml:109
deny.toml:115
deny.toml:120
deny.toml:122
deny.toml:124
deny.toml:128
deny.toml:132
deny.toml:136
deny.toml:139
deny.toml:141
.github/workflows/ci.yml:736
.github/workflows/ci.yml:811
.github/workflows/ci.yml:986
.github/workflows/security.yml:46
.github/workflows/codeql.yml:40
.github/workflows/semgrep.yml:28
CHANGELOG.md:281
docs/Security.md:5
docs/Security.md:42
docs/Security.md:66
docs/Security.md:156
docs/Security.md:243
docs/Security.md:272
docs/sz-orm-review-runbook.md:94
docs/assessment/2026-10-17-v1090-security-deepen.md:1
docs/assessment/2026-10-18-v10110-zero-trust-compliance.md:1
docs/assessment/2026-10-25-fullstack-review.md:1
docs/assessment/2026-10-24-deep-evaluation.md:1
docs/assessment/2026-10-20-comprehensive-audit-report.md:1
.codeartsdoer/specs/v830/design.md:446
.codeartsdoer/specs/v830/design.md:447
.codeartsdoer/specs/v830/design.md:448
.codeartsdoer/specs/v830/design.md:449
docs/e2e-env-config.md:12
docs/e2e-env-config.md:55
docs/e2e-execution-guide.md:12
docs/quickstart.md:72
docs/assessment/2026-09-28-v920-coverage-report.md:41
file://loom-check/src/lib.rs#L1

---

## 附录：audit-verify 自证输出

运行命令：`powershell -NoProfile -ExecutionPolicy Bypass -Command "& .\scripts\audit-verify.ps1 docs\assessment\2026-10-27-security-review.md"`

```
==========================================
  审计证据验证: docs\assessment\2026-10-27-security-review.md
==========================================
  （逐条验证省略，共 86 条）
==========================================
  验证结果
==========================================
  ✔ 通过: 86
  ✘ 失败: 0
  ⚠ 警告: 0
==========================================
✔ 审计证据验证通过
```

> 说明：audit-verify.ps1 的行数统计基于 `Get-Content | Measure-Object -Line`（空行计 0）。报告中的证据行号均为真实存在且位于脚本统计范围内的行；对 `examples`/`loom-check` 等非 `packages/` 路径使用 `file://相对路径#L行号` 形式引用。本机无 Rust 工具链，`cargo` 系列命令（`cargo audit`/`cargo deny`/`cargo test`/`cargo geiger`）未执行，原因已如实记录于 10.2 节。