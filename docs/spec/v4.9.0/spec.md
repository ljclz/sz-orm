# sz-orm v4.9.0 需求规格说明书

> 版本：v4.9.0（OWASP Top 10 完整覆盖渗透测试套件）
> 基线：v4.8.0（跨语言分布式事务协调 + 低代码双向同步 + OpenAPI 反向生成 + WASM 真实数据库连接闭环，4 项需求 REQ-V48-001~004 全部通过 feature gate 隔离，已发布到 crates.io 4.8.0，2026-08-14 安全审计已修复 4 个 MEDIUM 发现：FIND-001 SQL 注入 / FIND-002 Mutex poisoning / FIND-003 命令行密码泄露 / FIND-004 SSRF）
> 日期：2026-08-14
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：14 项需求全部 P1（用户选定完整 OWASP Top 10 覆盖），按"REQ-V49-001 A01 访问控制深化 → REQ-V49-002 A02 加密失败深化 → REQ-V49-003 A03 注入深化 → REQ-V49-004 A04 不安全设计 → REQ-V49-005 A05 安全配置错误深化 → REQ-V49-006 A06 过时组件深化 → REQ-V49-007 A07 完整性失败 → REQ-V49-008 A08 日志监控失败深化 → REQ-V49-009 A09 认证失败深化 → REQ-V49-010 A10 SSRF 深化 → REQ-V49-011 XSS → REQ-V49-012 CSRF → REQ-V49-013 文件上传安全 → REQ-V49-014 业务逻辑并发竞态"序推进，14 项无强依赖可并行开发
> 需求编号约定：REQ-V49-xxx（v4.9.0 需求项，REQ-V49-001 ~ REQ-V49-014）
> 规划依据：`docs/assessment/2026-08-14-security-audit.md`（白帽审计 4 个 MEDIUM 已修复）+ `docs/spec/v4.8.0/spec.md`（v4.8.0 已完成）+ 现有安全测试代码（`packages/sz-orm-auth/tests/{security_attacks,blackhat_poc}.rs`、`packages/sz-orm-core/tests/security_attacks.rs`、`packages/sz-orm-crypto/tests/{kat,blackhat_poc}.rs`）+ OWASP Top 10 (2021) 完整覆盖矩阵
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.8.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦 OWASP Top 10 完整覆盖渗透测试（A01~A10 深化 + XSS + CSRF + 文件上传安全 + 业务逻辑并发竞态）；本版本不涉及 crates.io 发布流程变更，不新增 workspace 成员（保持 60），不新增生产运行时能力，仅新增渗透测试套件 + 安全验证工具
> 边界声明：与 v4.8.0 零重叠（见第 1.4 节），v4.8.0 是"跨语言互操作 + 全栈闭环"层，v4.9.0 是"OWASP Top 10 完整覆盖渗透测试"层；本版本所有新增代码落在既有包的测试目录（`tests/`）或既有包的安全验证模块扩展，不修改既有生产运行时逻辑

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.9.0 的 OWASP Top 10 (2021) 完整覆盖渗透测试套件：14 项需求对应 14 个攻击面，每项需求包含攻击向量定义 + 防御断言 + EARS 格式验收条件 + 验证方法（cargo test / grep / cargo audit / cargo deny）。本组件复用既有安全测试基础（`packages/sz-orm-auth/tests/security_attacks.rs` JWT 攻击向量 + `packages/sz-orm-auth/tests/blackhat_poc.rs` OAuth2/JWT/TOTP/RBAC 黑帽 PoC + `packages/sz-orm-core/tests/security_attacks.rs` 多租户越权 + `packages/sz-orm-crypto/tests/kat.rs` 密码学 KAT + `packages/sz-orm-crypto/tests/blackhat_poc.rs` HMAC/PBKDF2 黑帽 PoC + `packages/sz-orm-audit/src/lib.rs` 审计日志脱敏 + `packages/sz-orm-masking/src/lib.rs` 数据脱敏 + `packages/sz-orm-config/src/prod_ready.rs` 生产配置校验 + `deny.toml` cargo-deny 配置 + `scripts/check-sql-injection.ps1` SQL 注入扫描），补齐 OWASP Top 10 未覆盖面：A01 访问控制深化（垂直/水平越权 + IDOR + 强制浏览 + JWT claims 深度）/ A02 加密失败深化（明文传输 + 弱算法 + 硬编码密钥 + ECB + 不安全随机数）/ A03 注入深化（NoSQL + OS 命令 + 模板 + 表达式 + Header 注入）/ A04 不安全设计（业务逻辑缺陷 + 缺失限流/重试/幂等/资源释放 + 竞态）/ A05 安全配置错误深化（默认配置 + 调试模式 + 错误消息泄露 + 默认密码）/ A06 过时组件深化（CVE + 许可证 + yanked + 重复依赖 + SBOM）/ A07 完整性失败（CI/CD + 签名验证 + 反序列化 + 构建可重现性）/ A08 日志监控失败深化（日志注入 + 脱敏深化 + 告警 + 审计完整性）/ A09 认证失败深化（会话固定 + 超时 + 并发 + 凭证填充 + 弱密码 + 账户枚举）/ A10 SSRF 深化（内网探测 + 协议白名单 + DNS rebinding + 元数据端点）/ XSS（HTML 表单转义 + 反射型/存储型/DOM 型）/ CSRF（CSRF token + SameSite + Origin 验证）/ 文件上传安全（类型/大小/内容验证 + 路径遍历 + Magic bytes）/ 业务逻辑并发竞态（连接池/租户配额/缓存击穿/分布式事务/TOCTOU）。

## 1.2 核心输入

1. **v4.8.0 已验收基线**：跨语言分布式事务协调 + 低代码双向同步 + OpenAPI 反向生成 + WASM 真实数据库连接闭环，4 项能力全部通过 feature gate 隔离，已发布到 crates.io 4.8.0，作为本版本基准。
2. **2026-08-14 安全审计报告**：`docs/assessment/2026-08-14-security-audit.md`，4 个 MEDIUM 发现已修复（FIND-001 SQL 注入 `packages/sz-orm-lc/src/lib.rs:954` model.name 未验证 → 已添加表名验证 / FIND-002 Mutex poisoning `packages/sz-orm-core/src/cache_warmup_protection.rs:232` 等 186 处 `.lock().unwrap()` → 已替换 parking_lot::Mutex / FIND-003 命令行密码泄露 `packages/sz-orm-macros/src/lib.rs:1602` → 已改用环境变量/stdin / FIND-004 SSRF `packages/sz-orm-wasm/src/real_db/connection.rs:33` proxy_url 未验证 → 已添加 URL 验证）。
3. **现有安全测试覆盖矩阵**：
   - **JWT 攻击向量**（`packages/sz-orm-auth/tests/security_attacks.rs`）：伪造签名 / 过期 token / 算法混淆 / 弱 secret 猜测 / 格式攻击（5 个测试函数，117 行）。
   - **多租户越权**（`packages/sz-orm-core/tests/security_attacks.rs`，feature = "multi-tenant-enhanced"）：跨租户表名访问 / tenant_id 参数化 / 无租户上下文 / Schema 路由边界值（4 个测试函数，122 行）。
   - **密码学 KAT**（`packages/sz-orm-crypto/tests/kat.rs`）：SHA-256 NIST FIPS 180-4 / HMAC-SHA256 RFC 4231 / PBKDF2 Python 官方向量 / AES-256-GCM 往返+篡改+AAD（4 个测试函数，121 行）。
   - **黑帽 PoC（auth）**（`packages/sz-orm-auth/tests/blackhat_poc.rs`）：OAuth2 授权码可预测（C-1）/ JWT 令牌类型混淆（C-2）/ TOTP 空密钥（M-10）/ RBAC action 级越权（M-11）（4 个回归测试，210 行）。
   - **黑帽 PoC（crypto）**（`packages/sz-orm-crypto/tests/blackhat_poc.rs`）：HMAC 参数走私（H-1）/ PBKDF2 弱迭代（M-8）（2 个回归测试，92 行）。
   - **审计日志脱敏**（`packages/sz-orm-audit/src/lib.rs:118` `mask_sensitive`）：password/token/credit_card 等敏感词大小写不敏感脱敏 + 哈希链防篡改（`HashChainAuditor`）。
   - **数据脱敏**（`packages/sz-orm-masking/src/lib.rs`）：Phone/Email/IdCard/BankCard/Name/Address/IP/Imei/Plate/ApiKey 10 种脱敏规则。
   - **生产配置校验**（`packages/sz-orm-config/src/prod_ready.rs`）：敏感字段规则强制 + 密码脱敏 + 空路径拒绝。
   - **cargo-deny 配置**（`deny.toml`）：安全公告（RUSTSEC 忽略清单 11 项带 reason）+ 许可证白名单（MIT/Apache-2.0/BSD/ISC/Zlib/MPL-2.0 等，禁止 copyleft）+ 依赖来源限制（unknown-registry/unknown-git = deny）+ 重复依赖警告。
   - **SQL 注入扫描**（`scripts/check-sql-injection.ps1` + `scripts/check-sql-injection.sh`）：静态扫描 SQL 字符串拼接。
4. **OWASP Top 10 (2021) 完整覆盖矩阵**：A01 失效的访问控制 / A02 加密失败 / A03 注入 / A04 不安全设计 / A05 安全配置错误 / A06 易受攻击和过时的组件 / A07 软件和数据完整性失败 / A08 安全日志和监控失败 / A09 身份识别和认证失败 / A10 服务器端请求伪造（SSRF）+ XSS + CSRF + 文件上传安全 + 业务逻辑并发竞态。
5. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
6. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包（sz-orm-core/sqlx/config/auth/macros/queue），已升级到 v4.8.0，作为 API 兼容性验证的下游基准。
7. **既有 feature gate 体系**：v4.8.0 已有 11 个 feature（v4.7.0 的 7 个 + v4.8.0 的 4 个：`cross-lang-dtx`/`lc-bidirectional-sync`/`openapi-reverse`/`wasm-real-db`）+ 本版本新增 1 个 feature（`owasp-pentest-suite` sz-orm-core，聚合所有渗透测试的 feature gate），作为新能力 feature gate 隔离的基础。

## 1.3 核心输出

1. **A01 失效的访问控制深化渗透测试**：`packages/sz-orm-auth/tests/owasp_a01_access_control.rs`（垂直越权 + 水平越权 + IDOR + 强制浏览 + JWT claims 深度验证），复用既有 `RbacAuthorizer`（`packages/sz-orm-auth/src/authorizer.rs`）/ `JwtAuthenticator`（`packages/sz-orm-auth/src/lib.rs`）/ `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs`）。
2. **A02 加密失败深化渗透测试**：`packages/sz-orm-crypto/tests/owasp_a02_crypto_failures.rs`（明文传输检测 + 弱算法检测 + 硬编码密钥扫描 + ECB 模式检测 + 不安全随机数），复用既有 `AesGcmCrypter`/`Pbkdf2Hasher`/`HmacSigner`（`packages/sz-orm-crypto/src/lib.rs`）。
3. **A03 注入深化渗透测试**：`packages/sz-orm-core/tests/owasp_a03_injection.rs`（NoSQL + OS 命令 + 模板 + 表达式 + Header 注入），复用既有 `QueryBuilder`（`packages/sz-orm-core/src/query.rs`）/ `OpenApiInjectionGuard`（`packages/sz-orm-swagger/src/reverse/mod.rs:27`）。
4. **A04 不安全设计渗透测试**：`packages/sz-orm-core/tests/owasp_a04_insecure_design.rs`（业务逻辑缺陷 + 缺失限流/重试/幂等/资源释放 + 竞态），复用既有 `WasmDbRateLimiter`（`packages/sz-orm-wasm/src/real_db/mod.rs:26`）/ `RetryPolicy`（`packages/sz-orm-grpc/src/lib.rs:415`）/ `TenantResourceQuota`（`packages/sz-orm-core/src/tenant_quota_rls.rs`）。
5. **A05 安全配置错误深化渗透测试**：`packages/sz-orm-config/tests/owasp_a05_misconfig.rs`（默认配置 + 调试模式 + 错误消息泄露 + 默认密码），复用既有 `prod_ready.rs`（`packages/sz-orm-config/src/prod_ready.rs`）。
6. **A06 易受攻击和过时的组件深化渗透测试**：`scripts/owasp_a06_vulnerable_components.ps1`（CVE 漏洞追踪 + 许可证合规深化 + yanked crate 检测 + 重复依赖检测 + SBOM 生成），复用既有 `deny.toml` + `cargo audit` + `cargo deny check`。
7. **A07 软件和数据完整性失败渗透测试**：`packages/sz-orm-audit/tests/owasp_a07_integrity.rs`（CI/CD 管道完整性 + 签名验证 + 反序列化完整性 + 构建可重现性），复用既有 `HashChainAuditor`（`packages/sz-orm-audit/src/lib.rs`）。
8. **A08 安全日志和监控失败深化渗透测试**：`packages/sz-orm-audit/tests/owasp_a08_logging_failures.rs`（日志注入 + 脱敏深化 + 告警 + 审计完整性），复用既有 `SqlAuditor`/`mask_sensitive`（`packages/sz-orm-audit/src/lib.rs`）/ `DataMasker`（`packages/sz-orm-masking/src/lib.rs`）。
9. **A09 身份识别和认证失败深化渗透测试**：`packages/sz-orm-auth/tests/owasp_a09_auth_failures.rs`（会话固定 + 超时 + 并发 + 凭证填充 + 弱密码 + 账户枚举），复用既有 `JwtAuthenticator`/`OAuth2Server`/`MfaManager`（`packages/sz-orm-auth/src/lib.rs`）。
10. **A10 SSRF 深化渗透测试**：`packages/sz-orm-wasm/tests/owasp_a10_ssrf.rs`（内网探测 + 协议白名单 + DNS rebinding + 元数据端点），复用既有 `WasmRealDbConnection`（`packages/sz-orm-wasm/src/real_db/mod.rs:19`，FIND-004 已修复 URL 验证）。
11. **XSS 跨站脚本攻击渗透测试**：`packages/sz-orm-lc/tests/owasp_xss.rs`（HTML 表单转义 + 反射型/存储型/DOM 型），复用既有 `FormGenerator`/`FieldTypeMapping::sql_to_html_input`（`packages/sz-orm-lc/src/lib.rs:210`）。
12. **CSRF 跨站请求伪造渗透测试**：`packages/sz-orm-auth/tests/owasp_csrf.rs`（CSRF token + SameSite + Origin 验证），复用既有 `OAuth2Server`（`packages/sz-orm-auth/src/oauth2.rs`）。
13. **文件上传安全渗透测试**：`packages/sz-orm-storage/tests/owasp_file_upload.rs`（类型/大小/内容验证 + 路径遍历 + Magic bytes），复用既有 `StorageBackend` trait（`packages/sz-orm-storage/src/storage.rs:15` `put` 方法）。
14. **业务逻辑并发竞态条件渗透测试**：`packages/sz-orm-core/tests/owasp_race_conditions.rs`（连接池/租户配额/缓存击穿/分布式事务/TOCTOU），复用既有 `Pool`（`packages/sz-orm-core/src/pool.rs`）/ `QuotaEnforcer`（`packages/sz-orm-core/src/tenant_quota_rls.rs`）/ `CacheWarmupProtection`（`packages/sz-orm-core/src/cache_warmup_protection.rs`）/ `DtxManager`（`packages/sz-orm-dtx/src/lib.rs:432`）。
15. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
16. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不修改既有生产运行时逻辑**：所有渗透测试为新增测试代码（`tests/owasp_*.rs`）或新增安全验证工具（`scripts/owasp_*.ps1`），不修改既有 `src/` 下生产代码。既有 `RbacAuthorizer`/`JwtAuthenticator`/`QueryBuilder`/`Pool`/`QuotaEnforcer`/`WasmRealDbConnection`/`SqlAuditor`/`StorageBackend` 等保留不动。
2. **不破坏既有公开 API**：所有新能力通过 `owasp-pentest-suite` feature gate 隔离，既有公开 API 签名保持完全向后兼容。
3. **不新增 workspace 成员**：workspace 成员保持 60（`Cargo.toml:2`），所有新测试落在既有包的 `tests/` 目录。
4. **不降低既有测试覆盖**：v4.9.0 不得使 v4.8.0 已验收测试基线回退，仅增不减。
5. **不引入 unsafe**：所有新增测试代码无 `unsafe` 块，沿用既有 unsafe 零容忍铁律。
6. **不引入占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`，所有新增测试须真实实现攻击向量与防御断言。
7. **不引入 Breaking Change**：新能力通过 `owasp-pentest-suite` feature gate 隔离，默认关闭，既有 feature 组合行为不变。
8. **不负责修复新发现的漏洞**：本版本仅交付渗透测试套件（发现漏洞 + 断言防御），若渗透测试发现新漏洞，记录在审计报告并标记为后续版本修复，本版本不修复（除非为 P0 CRITICAL 且修复不影响 API 兼容性）。
9. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
10. **不做模糊测试（fuzzing）**：本版本交付确定性渗透测试（明确攻击向量 + 防御断言），模糊测试（`packages/sz-orm-core/tests/fuzz.rs` 已有）不在本版本范围。
11. **不做形式化验证**：本版本交付经验性渗透测试，形式化验证（`packages/sz-orm-core/tests/formal.rs` 已有）不在本版本范围。
12. **不做性能基准测试**：本版本交付安全渗透测试，性能基准测试不在本版本范围。
13. **不重复实现已有安全测试**：JWT 攻击向量 / 多租户越权 / 密码学 KAT / 黑帽 PoC 已有测试不重复，本版本补齐未覆盖面。
14. **不与 v4.8.0 任务重叠**：v4.8.0 已占用的包/模块（sz-orm-dtx cross-lang-dtx / sz-orm-lc lc-bidirectional-sync / sz-orm-swagger openapi-reverse / sz-orm-wasm wasm-real-db）本版本不触碰其新增逻辑，新增范围全部落在既有包的测试目录扩展。

---

# 2. 领域术语

**OWASP Top 10 (2021)**

: Open Web Application Security Project 发布的 Web 应用十大安全风险清单（2021 版），包含 A01 失效的访问控制 / A02 加密失败 / A03 注入 / A04 不安全设计 / A05 安全配置错误 / A06 易受攻击和过时的组件 / A07 软件和数据完整性失败 / A08 安全日志和监控失败 / A09 身份识别和认证失败 / A10 服务器端请求伪造（SSRF）。

**渗透测试（Penetration Test）**

: 以攻击者视角对系统进行安全测试，构造攻击向量验证防御是否成立。测试通过 = 防御断言成立（攻击被拒绝/检测/缓解）；测试失败 = 漏洞被证明存在。

**攻击向量（Attack Vector）**

: 攻击者利用系统缺陷的具体路径与方法，包含触发条件 + 攻击载荷 + 预期攻击效果。

**防御断言（Defense Assertion）**

: 系统应对攻击向量的预期行为（拒绝/检测/缓解/日志记录），以 `assert!` / `assert_eq!` 形式固化在测试代码中。

**EARS 格式（Easy Approach to Requirements Syntax）**

: 需求规格格式，5 种模式：Ubiquitous（系统应当 X）/ Event-driven（当 Y 发生时，系统应当 X）/ State-driven（当处于状态 S 时，系统应当 X）/ Optional（当处于状态 S 时，系统可能 X）/ Unwanted（如果 Y 发生，系统应当 X）。

**垂直越权（Vertical Privilege Escalation）**

: 低权限用户访问高权限功能/资源（如普通用户访问管理员接口），属于 A01 失效的访问控制。

**水平越权（Horizontal Privilege Escalation）**

: 同权限级别用户访问其他用户的资源（如用户 A 访问用户 B 的订单），属于 A01 失效的访问控制。

**IDOR（Insecure Direct Object Reference）**

: 通过直接引用对象 ID 访问未授权资源（如修改 URL 中 `?id=1` 为 `?id=2` 访问他人订单），属于 A01 失效的访问控制。

**强制浏览（Forced Browsing）**

: 直接访问受保护 URL 路径绕过导航限制（如直接访问 `/admin panel`），属于 A01 失效的访问控制。

**明文传输（Cleartext Transmission）**

: 敏感数据以明文形式传输（HTTP 而非 HTTPS / 未加密的数据库连接），属于 A02 加密失败。

**弱算法（Weak Algorithm）**

: 已被证明不安全的加密算法（MD5 / SHA-1 用于签名 / DES / RC4 / ECB 模式），属于 A02 加密失败。

**硬编码密钥（Hardcoded Secret）**

: 加密密钥/密码/token 以字面量形式嵌入源代码，属于 A02 加密失败。

**ECB 模式（Electronic Codebook）**

: 对称加密分组模式，相同明文产生相同密文，泄露数据模式，属于 A02 加密失败。

**不安全随机数（Insecure Random）**

: 使用 `rand::thread_rng()`（非 CSPRNG）或 `DefaultHasher` 生成安全敏感值（token/密钥/nonce），属于 A02 加密失败与 CWE-338。

**NoSQL 注入**

: NoSQL 查询中通过构造特殊操作符（`$where`/`$ne`/`$gt`）绕过条件，属于 A03 注入。

**OS 命令注入**

: 通过用户输入拼接系统命令（`Command::new("sh").arg("-c").arg(user_input)`）执行任意命令，属于 A03 注入。

**模板注入（Template Injection）**

: 模板引擎中注入模板语法（`{{7*7}}`/`${7*7}`）执行任意代码，属于 A03 注入。

**表达式注入（Expression Injection）**

: 表达式语言中注入表达式（SpEL/OGNL/EL）执行任意代码，属于 A03 注入。

**Header 注入（Header Injection）**

: 通过 CRLF 字符（`\r\n`）注入 HTTP 头部（`Set-Cookie`/`Location`），属于 A03 注入。

**业务逻辑缺陷（Business Logic Flaw）**

: 业务流程中的逻辑错误（如负数数量/跳过支付步骤/重复使用优惠码），属于 A04 不安全设计。

**缺失限流（Missing Rate Limiting）**

: 关键接口未限流，允许暴力破解/凭证填充/资源耗尽，属于 A04 不安全设计。

**缺失重试上限（Missing Retry Limit）**

: 失败重试无上限，允许攻击者触发无限重试耗尽资源，属于 A04 不安全设计。

**缺失幂等性（Missing Idempotency）**

: 重复请求产生重复副作用（重复扣款/重复下单），属于 A04 不安全设计。

**缺失资源释放（Missing Resource Release）**

: 资源（连接/文件/锁）未释放导致泄露，属于 A04 不安全设计与 A05 安全配置错误。

**竞态条件（Race Condition）**

: 并发访问共享资源时因时序问题导致数据不一致/越权/双重消费，属于 A04 不安全设计。

**TOCTOU（Time-of-Check to Time-of-Use）**

: 检查与使用之间的时间窗口被攻击者利用（如检查余额后扣款前余额被改变），属于 A04 不安全设计与竞态条件。

**默认配置（Default Configuration）**

: 使用不安全的默认配置（默认密码/默认端口/默认允许所有来源），属于 A05 安全配置错误。

**调试模式（Debug Mode）**

: 生产环境启用调试模式泄露堆栈/源码/SQL，属于 A05 安全配置错误。

**错误消息泄露（Verbose Error Message）**

: 错误消息泄露内部信息（堆栈/SQL/文件路径/版本号），属于 A05 安全配置错误。

**默认密码（Default Password）**

: 使用出厂默认密码（admin/admin / root/root），属于 A05 安全配置错误。

**CVE 漏洞追踪（CVE Tracking）**

: 追踪依赖的已知 CVE 漏洞（通过 `cargo audit` + RUSTSEC 公告），属于 A06 易受攻击和过时的组件。

**许可证合规（License Compliance）**

: 依赖许可证符合项目合规要求（禁止 GPL/AGPL copyleft），属于 A06 易受攻击和过时的组件。

**Yanked Crate**

: 已从 crates.io 撤回的 crate 版本（可能含严重缺陷），属于 A06 易受攻击和过时的组件。

**重复依赖（Duplicate Dependency）**

: 同一 crate 的多个版本共存导致二进制膨胀/版本碎片化，属于 A06 易受攻击和过时的组件。

**SBOM（Software Bill of Materials）**

: 软件物料清单，列出所有依赖及其版本/来源/许可证，属于 A06 易受攻击和过时的组件与 A07 完整性失败。

**CI/CD 管道完整性（CI/CD Pipeline Integrity）**

: CI/CD 管道防篡改（签名提交/签名构建产物/可重现构建），属于 A07 软件和数据完整性失败。

**签名验证（Signature Verification）**

: 验证软件包/依赖/构建产物的数字签名，防止篡改，属于 A07 软件和数据完整性失败。

**反序列化完整性（Deserialization Integrity）**

: 反序列化时验证数据完整性（类型/长度/签名），防止反序列化攻击，属于 A07 软件和数据完整性失败。

**构建可重现性（Build Reproducibility）**

: 相同输入产生位级相同的构建产物，防止构建管道篡改，属于 A07 软件和数据完整性失败。

**日志注入（Log Injection）**

: 攻击者通过用户输入注入换行/伪造日志条目，污染审计日志，属于 A08 安全日志和监控失败。

**日志脱敏（Log Masking）**

: 日志中敏感数据（密码/token/信用卡/PII）脱敏，属于 A08 安全日志和监控失败。

**告警（Alerting）**

: 安全事件触发告警（异常登录/越权尝试/注入检测），属于 A08 安全日志和监控失败。

**审计完整性（Audit Integrity）**

: 审计日志防篡改（哈希链/签名/追加写入），属于 A08 安全日志和监控失败。

**会话固定（Session Fixation）**

: 攻击者预设用户会话 ID，用户登录后攻击者复用该会话 ID，属于 A09 身份识别和认证失败。

**会话超时（Session Timeout）**

: 会话空闲/绝对超时后失效，属于 A09 身份识别和认证失败。

**并发会话（Concurrent Sessions）**

: 同一账户并发会话数限制，防止会话共享/凭证共享，属于 A09 身份识别和认证失败。

**凭证填充（Credential Stuffing）**

: 使用泄露的凭证字典批量尝试登录，属于 A09 身份识别和认证失败。

**弱密码（Weak Password）**

: 密码不满足复杂度要求（长度/字符集/常见密码黑名单），属于 A09 身份识别和认证失败。

**账户枚举（Account Enumeration）**

: 通过登录/注册/找回密码响应差异枚举有效账户，属于 A09 身份识别和认证失败。

**内网探测（Internal Network Probing）**

: 通过 SSRF 探测内网服务（`http://127.0.0.1:8080`/`http://169.254.169.254`），属于 A10 SSRF。

**协议白名单（Protocol Whitelist）**

: SSRF 防御：仅允许 `http`/`https` 协议，拒绝 `file`/`gopher`/`dict`/`ftp`，属于 A10 SSRF。

**DNS Rebinding**

: 攻击者控制 DNS 解析，首次解析为合法 IP 通过校验，二次解析为内网 IP 完成 SSRF，属于 A10 SSRF。

**元数据端点（Metadata Endpoint）**

: 云厂商元数据服务端点（AWS `169.254.169.254`/GCP `metadata.google.internal`/Azure `169.254.169.254`），通过 SSRF 访问窃取实例凭据，属于 A10 SSRF。

**XSS（Cross-Site Scripting）**

: 跨站脚本攻击，攻击者注入恶意脚本到网页，在其他用户浏览器执行，分为反射型/存储型/DOM 型。

**CSRF（Cross-Site Request Forgery）**

: 跨站请求伪造，攻击者诱导已登录用户在不知情下发送请求，利用用户会话执行操作。

**CSRF Token**

: 防御 CSRF 的随机 token，嵌入表单/请求头，服务端验证，属于 CSRF 防御。

**SameSite Cookie**

: Cookie 的 SameSite 属性（Strict/Lax/None）限制跨站请求携带，属于 CSRF 防御。

**Origin 验证**

: 验证请求 `Origin`/`Referer` 头部是否为合法来源，属于 CSRF 防御。

**文件上传安全（File Upload Security）**

: 文件上传时验证类型/大小/内容，防止上传恶意文件（Web Shell/路径遍历/超大文件），属于 A04 不安全设计与 A05 安全配置错误。

**Magic Bytes**

: 文件头魔数字节，用于验证文件真实类型（不依赖扩展名），属于文件上传安全。

**路径遍历（Path Traversal）**

: 通过 `../` 序列访问预期目录外文件，属于 A03 注入与文件上传安全。

**v4.9.0 feature gate**

: 控制本版本新能力的 feature gate（`owasp-pentest-suite` sz-orm-core，聚合所有渗透测试），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

1. **安全测试工程师（渗透测试）**：编写并运行 OWASP Top 10 渗透测试套件，验证防御断言成立。
2. **白帽审计员**：以攻击者视角审查代码，构造攻击向量，记录发现并验证修复。
3. **黑帽 PoC 验证者**：验证已修复漏洞的回归测试（攻击行为不再成立）。
4. **DevSecOps 工程师**：将渗透测试集成到 CI/CD 管道（门禁 21 安全攻击测试），自动化执行。
5. **依赖安全工程师**：追踪依赖 CVE/许可证/yanked/重复依赖，维护 `deny.toml` 忽略清单与 SBOM。

## 3.2 外部系统

1. **OWASP Top 10 (2021) 规范**：定义十大安全风险清单，作为渗透测试覆盖矩阵基准。
2. **RUSTSEC 公告库**：Rust 生态安全公告（`https://github.com/rustsec/advisory-db`），`cargo audit` 数据源。
3. **crates.io**：Rust 包仓库，`cargo deny` 检查 yanked crate 数据源。
4. **本机数据库（MySQL/PostgreSQL/Oracle）**：渗透测试的真实数据库环境（连接信息见 1.2 节）。
5. **sz-pay 生产依赖**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.9.0 OWASP Top 10 完整覆盖渗透测试 上下文图

actor "安全测试工程师" as pentester
actor "白帽审计员" as auditor
actor "黑帽 PoC 验证者" as blackhat
actor "DevSecOps 工程师" as devsecops
actor "依赖安全工程师" as dep_engineer

package "sz-orm v4.9.0 渗透测试套件" as suite {
  component "A01 访问控制深化\n(owasp_a01_access_control.rs)" as a01
  component "A02 加密失败深化\n(owasp_a02_crypto_failures.rs)" as a02
  component "A03 注入深化\n(owasp_a03_injection.rs)" as a03
  component "A04 不安全设计\n(owasp_a04_insecure_design.rs)" as a04
  component "A05 安全配置错误深化\n(owasp_a05_misconfig.rs)" as a05
  component "A06 过时组件深化\n(owasp_a06_vulnerable_components.ps1)" as a06
  component "A07 完整性失败\n(owasp_a07_integrity.rs)" as a07
  component "A08 日志监控失败深化\n(owasp_a08_logging_failures.rs)" as a08
  component "A09 认证失败深化\n(owasp_a09_auth_failures.rs)" as a09
  component "A10 SSRF 深化\n(owasp_a10_ssrf.rs)" as a10
  component "XSS\n(owasp_xss.rs)" as xss
  component "CSRF\n(owasp_csrf.rs)" as csrf
  component "文件上传安全\n(owasp_file_upload.rs)" as upload
  component "业务逻辑并发竞态\n(owasp_race_conditions.rs)" as race
}

component "OWASP Top 10 (2021) 规范" as owasp
component "RUSTSEC 公告库" as rustsec
component "crates.io" as crates_io
database "本机数据库\n(MySQL/PG/Oracle)" as db
component "sz-pay 生产依赖" as szpay

pentester --> suite : 编写/运行渗透测试
auditor --> suite : 构造攻击向量
blackhat --> suite : 验证回归测试
devsecops --> suite : CI/CD 集成（门禁 21）
dep_engineer --> a06 : 维护 deny.toml + SBOM

suite --> owasp : 覆盖矩阵
a06 --> rustsec : cargo audit
a06 --> crates_io : cargo deny (yanked)
suite --> db : 真实 DB 渗透测试
suite --> szpay : API 兼容性验证

@enduml
```

---

# 4. DFX 约束

## 4.1 性能

1. **渗透测试套件执行时间**：全套 14 项渗透测试在 Windows MSVC + `RUST_MIN_STACK=134217728` 环境下执行时间不超过 60 秒（不含真实 DB 集成测试），单项渗透测试不超过 10 秒。
2. **真实 DB 渗透测试执行时间**：涉及真实数据库的渗透测试（A03 注入深化 / A10 SSRF 深化 / 文件上传安全 / 业务逻辑并发竞态）执行时间不超过 120 秒。
3. **cargo audit + cargo deny 执行时间**：A06 过时组件深化渗透测试（`cargo audit` + `cargo deny check`）执行时间不超过 30 秒。

## 4.2 可靠性

1. **渗透测试确定性**：所有渗透测试为确定性测试（明确攻击向量 + 防御断言），不依赖随机/时间/网络，确保 CI 可重现。
2. **渗透测试隔离性**：渗透测试不修改全局状态（环境变量/文件系统/数据库），测试间相互隔离，并行执行不干扰。
3. **本地硬盘文件清理**：渗透测试写入本地硬盘的临时文件（如文件上传测试的临时文件）在测试结束后必须及时删除并释放进程，沿用既有铁律。
4. **渗透测试不破坏生产数据**：真实 DB 渗透测试使用独立测试库（`sz_orm_test`），不触碰生产数据，测试后清理测试数据。

## 4.3 安全性

1. **渗透测试不引入真实漏洞**：渗透测试中的攻击载荷仅用于验证防御断言，不得引入真实可利用漏洞（如测试用的"恶意"SQL 须在测试沙箱内执行，不影响其他测试）。
2. **渗透测试不泄露敏感信息**：渗透测试中的"敏感"数据（密码/token/密钥）须为测试专用值（如 "test-secret-42"），不得使用真实生产凭据。
3. **渗透测试代码无 unsafe**：所有渗透测试代码无 `unsafe` 块，沿用既有 unsafe 零容忍铁律。
4. **渗透测试代码无占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`，所有攻击向量与防御断言须真实实现。

## 4.4 可维护性

1. **渗透测试可追溯**：每项渗透测试须附 OWASP 编号（A01~A10/XSS/CSRF/文件上传/竞态）+ 攻击向量描述 + 防御断言描述 + 验证方法（cargo test / grep / cargo audit）。
2. **渗透测试可扩展**：渗透测试套件结构清晰（每项 OWASP 独立测试文件），便于后续新增攻击向量。
3. **门禁前置**：所有新增渗透测试须通过 23 道门禁（特别是门禁 21 安全攻击测试 + 门禁 4 单元/集成测试 + 门禁 8 占位检查 + 门禁 15 幻影交付）。
4. **审计证据**：每项渗透测试结论须附 `file:line` 证据（测试函数位置 + 防御断言位置），沿用既有审计合规铁律。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 `owasp-pentest-suite` feature gate 隔离，默认关闭，既有公开 API 签名不变。
2. **v4.8.0 测试基线不回退**：v4.8.0 已验收测试基线须全部通过，仅增不减。
3. **sz-pay 生产依赖兼容**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包 API 不变，升级到 v4.9.0 须无 Breaking Change。
4. **五方言覆盖**：A03 注入深化 / A10 SSRF 深化 / 文件上传安全 / 业务逻辑并发竞态涉及数据库的渗透测试须覆盖 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言（或标注方言限制）。
5. **跨平台**：渗透测试须在 Windows MSVC + Linux + macOS 均可执行（PowerShell 脚本须有 Bash 等价脚本）。

---

# 5. 核心能力

## 5.1 A01 失效的访问控制深化渗透测试（REQ-V49-001，P1）

### 5.1.1 业务规则

1. **垂直越权渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a01_access_control.rs` 测试，构造垂直越权攻击向量：普通用户角色（`User::new(1, "alice").with_roles(vec!["user".to_string()])`）尝试调用管理员功能（`az.can(&user, "delete", "any_resource")` / `az.can(&user, "admin", "panel")`），断言 `RbacAuthorizer`（`packages/sz-orm-auth/src/authorizer.rs`）拒绝（返回 `false`）。
   a. 验收条件：[普通用户角色 + 调用 delete/admin 功能] → [`az.can()` 返回 false，拒绝垂直越权]；[管理员角色 + 调用 delete/admin 功能] → [`az.can()` 返回 true，正常授权]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a01_access_control`
2. **水平越权渗透测试**（EARS: Ubiquitous）
   系统应当构造水平越权攻击向量：用户 A（tenant_id=1）尝试访问用户 B（tenant_id=2）的资源，通过 `TenantContext::new(1, IsolationStrategy::SchemaIsolation)`（`packages/sz-orm-core/src/tenant_context.rs`）+ `QueryBuilder.with_tenant_id(1)` 构建查询，断言生成的 SQL 包含 `tenant_1_` 前缀（Schema 隔离），不包含 `tenant_2_`。
   a. 验收条件：[用户 A tenant_id=1 + 访问用户 B tenant_id=2 资源] → [查询 SQL 含 tenant_1_ 前缀，不含 tenant_2_，水平越权被阻止]
   验证方法：`cargo test -p sz-orm-core --features multi-tenant-enhanced,owasp-pentest-suite --test owasp_a01_access_control`
3. **IDOR 渗透测试**（EARS: Ubiquitous）
   系统应当构造 IDOR 攻击向量：用户 A（user_id=1）尝试通过修改 `?id=2` 访问用户 B（user_id=2）的订单，断言查询须附加 `user_id = $1`（参数化）条件，返回结果仅含 user_id=1 的订单。
   a. 验收条件：[用户 A user_id=1 + 查询 orders where id=2] → [查询附加 user_id=1 条件，返回空（id=2 属于用户 B），IDOR 被阻止]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a01_access_control`
4. **强制浏览渗透测试**（EARS: Ubiquitous）
   系统应当构造强制浏览攻击向量：未授权用户直接访问受保护资源（如管理员 schema 迁移表 `__sz_orm_migrations`），断言 `RbacAuthorizer` 拒绝（返回 false）或查询附加租户隔离条件。
   a. 验收条件：[未授权用户 + 直接访问 __sz_orm_migrations] → [`az.can()` 返回 false 或查询附加租户隔离，强制浏览被阻止]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a01_access_control`
5. **JWT claims 深度验证渗透测试**（EARS: Ubiquitous）
   系统应当构造 JWT claims 篡改攻击向量：签发 `JwtClaims::new("user-1", exp).with_roles(vec!["user".to_string()])`，篡改 claims 为 `with_roles(vec!["admin".to_string()])` 但保留原签名，断言 `JwtEncoder::decode`（`packages/sz-orm-auth/src/jwt.rs`）拒绝（签名校验失败）。深化既有 `packages/sz-orm-auth/tests/security_attacks.rs:53` `attack_tampered_payload_rejected`，新增 `iss`（issuer）/`aud`（audience）/`sub`（subject）/`nbf`（not-before）claims 验证。
   a. 验收条件：[篡改 roles/iss/aud/sub/nbf claims + 保留原签名] → [`decode` 拒绝，签名校验失败]；[签发 iss="issuer-A" + 验证 iss="issuer-B"] → [拒绝，issuer 不匹配]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a01_access_control`
6. **RBAC 通配符权限深化渗透测试**（EARS: Ubiquitous）
   系统应当构造 RBAC 通配符越权攻击向量：`az.grant("operator", "read")`（action 级）不得隐式授予 `read:任意资源`（沿用既有 `packages/sz-orm-auth/tests/blackhat_poc.rs:188` M-11 修复），深化测试 `*` 通配符（`az.grant("admin", "*")` 授予所有）与 `action:*` 通配符（`az.grant("operator", "read:*")` 授予所有 read）的边界。
   a. 验收条件：[`grant("operator", "read")` + `can("read", "payments")`] → [false，action 级不授予资源]；[`grant("admin", "*")` + `can("delete", "any")`] → [true，`*` 通配符授予所有]；[`grant("operator", "read:*")` + `can("read", "posts")`] → [true，`read:*` 授予所有 read]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a01_access_control`
7. **复用既有访问控制基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `RbacAuthorizer`（`packages/sz-orm-auth/src/authorizer.rs`）/ `JwtAuthenticator`（`packages/sz-orm-auth/src/lib.rs`）/ `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs`）/ `QueryBuilder.with_tenant_id`（`packages/sz-orm-core/src/query.rs`），不重复实现访问控制逻辑。
   a. 验收条件：[A01 渗透测试] → [复用既有 RbacAuthorizer/JwtAuthenticator/TenantContext，不新建访问控制逻辑]
8. **禁止项**（EARS: Unwanted）
   如果 A01 渗透测试影响默认编译或破坏既有访问控制，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用，且既有 `RbacAuthorizer`/`JwtAuthenticator`/`TenantContext` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无 A01 渗透测试，行为与 v4.8.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title A01 失效的访问控制深化渗透测试 交互流程
actor "渗透测试" as test
participant "RbacAuthorizer\n(既有)" as az
participant "JwtAuthenticator\n(既有)" as jwt
participant "TenantContext\n(既有)" as ctx
participant "QueryBuilder\n(既有)" as qb

test -> az : 垂直越权: user.can("delete", "any")
az --> test : false (拒绝)
test -> az : 水平越权: user_A.can("read", "user_B_resource")
az --> test : false (拒绝)

test -> ctx : TenantContext::new(1, SchemaIsolation)
test -> qb : with_tenant_id(1).table("orders")
qb --> test : SQL 含 tenant_1_orders (隔离)

test -> jwt : IDOR: user_A query orders where id=2
jwt -> qb : 附加 user_id=1 条件
qb --> test : 返回空 (id=2 属于 user_B)

test -> jwt : 篡改 claims (roles=user→admin)
jwt --> test : 签名校验失败 (拒绝)
@enduml
```

### 5.1.3 异常场景

1. **访问控制配置缺失**
   a. 触发条件：`RbacAuthorizer` 未配置任何角色权限
   b. 系统行为：默认拒绝（fail-close），所有 `can()` 返回 false
   c. 用户感知：所有操作被拒绝，提示"未授权"
2. **JWT claims 缺失**
   a. 触发条件：JWT 缺少 `iss`/`aud`/`sub` 等必需 claims
   b. 系统行为：`decode` 拒绝，返回 `AuthError::InvalidToken`
   c. 用户感知：错误"invalid token: missing required claim"

## 5.2 A02 加密失败深化渗透测试（REQ-V49-002，P1）

### 5.2.1 业务规则

1. **明文传输检测渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a02_crypto_failures.rs` 测试，构造明文传输攻击向量：检测数据库连接串是否使用 `mysql://`（明文）而非 `mysqls://`（TLS）/`postgres://`（明文）而非 `postgresqls://`（TLS），断言生产配置（`packages/sz-orm-config/src/prod_ready.rs`）拒绝明文连接串。
   a. 验收条件：[配置 `mysql://root:pass@host/db`（明文）] → [生产配置校验拒绝，提示"use TLS connection mysqls://"]；[配置 `mysqls://root:pass@host/db`（TLS）] → [校验通过]
   验证方法：`cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a02_crypto_failures`
2. **弱算法检测渗透测试**（EARS: Ubiquitous）
   系统应当构造弱算法攻击向量：检测是否使用 MD5/SHA-1 用于签名（`sha1::Sha1::digest` 用于 HMAC-SHA1 是 TOTP 标准 RFC 4226/6238 允许，但用于 JWT 签名须拒绝）/DES/RC4/ECB 模式，断言 `sz-orm-crypto`（`packages/sz-orm-crypto/src/lib.rs`）仅使用 SHA-256/HMAC-SHA256/AES-256-GCM/PBKDF2-HMAC-SHA256。
   a. 验收条件：[grep "Md5::new" src/] → [无生产代码使用 MD5]；[grep "Sha1::new" src/ 排除 TOTP] → [无 JWT 签名使用 SHA-1]；[grep "Des::" / "Rc4::" / "Ecb::" src/] → [无生产代码使用 DES/RC4/ECB]
   验证方法：`cargo test -p sz-orm-crypto --features owasp-pentest-suite --test owasp_a02_crypto_failures` + `grep -rn "Md5::\|Des::\|Rc4::\|Ecb::" packages/*/src/`
3. **硬编码密钥扫描渗透测试**（EARS: Ubiquitous）
   系统应当构造硬编码密钥攻击向量：扫描生产代码（`packages/*/src/`）中硬编码的密钥/密码/token（排除测试代码 `tests/` 和文档注释），断言无硬编码密钥（沿用既有审计：12 处硬编码全部在测试代码中）。
   a. 验收条件：[grep "secret"/"password"/"token" = 字面量 in src/] → [无生产代码硬编码密钥，全部在 tests/ 或文档注释]
   验证方法：`cargo test -p sz-orm-crypto --features owasp-pentest-suite --test owasp_a02_crypto_failures` + `grep -rn "\"secret\"\|\"password\"\|\"token\"" packages/*/src/ --exclude-dir=tests`
4. **ECB 模式检测渗透测试**（EARS: Ubiquitous）
   系统应当构造 ECB 模式攻击向量：检测对称加密是否使用 ECB 模式（相同明文产生相同密文），断言 `AesGcmCrypter`（`packages/sz-orm-crypto/src/lib.rs`）使用 GCM 模式（AEAD，相同明文 + 不同 nonce 产生不同密文）。
   a. 验收条件：[加密 "plaintext" 两次 + 相同 key] → [两次密文不同（GCM 随机 nonce），非 ECB 模式]
   验证方法：`cargo test -p sz-orm-crypto --features owasp-pentest-suite --test owasp_a02_crypto_failures`
5. **不安全随机数检测渗透测试**（EARS: Ubiquitous）
   系统应当构造不安全随机数攻击向量：检测安全敏感值（token/密钥/nonce/授权码）是否使用 CSPRNG（`rand::rngs::OsRng`）而非 `rand::thread_rng()`（非 CSPRNG）/`DefaultHasher`（CWE-338），断言 `sz-orm-auth` 使用 `OsRng`（沿用既有 C-1 修复 `packages/sz-orm-auth/tests/blackhat_poc.rs:23` OAuth2 授权码 + C-2 修复 JWT 令牌家族 ID）。
   a. 验收条件：[grep "thread_rng()\|DefaultHasher::new" src/ 排除测试] → [安全敏感值不使用非 CSPRNG]；[OAuth2 授权码生成] → [使用 OsRng，时间戳枚举无法还原（沿用 C-1 回归测试）]
   验证方法：`cargo test -p sz-orm-crypto --features owasp-pentest-suite --test owasp_a02_crypto_failures` + `grep -rn "thread_rng()\|DefaultHasher::new" packages/*/src/`
6. **密钥长度验证渗透测试**（EARS: Ubiquitous）
   系统应当构造弱密钥攻击向量：检测加密密钥长度是否满足最低要求（AES-256 须 32 字节 / HMAC-SHA256 须 ≥ 32 字节 / PBKDF2 迭代须 ≥ 100_000，沿用既有 M-8 修复 `packages/sz-orm-crypto/tests/blackhat_poc.rs:68`），断言短密钥/低迭代被拒绝。
   a. 验收条件：[AES-256 密钥 16 字节] → [拒绝，提示"key must be 32 bytes"]；[PBKDF2 迭代 1000] → [拒绝，提示"iterations must be ≥ 100_000"（M-8 修复）]
   验证方法：`cargo test -p sz-orm-crypto --features owasp-pentest-suite --test owasp_a02_crypto_failures`
7. **复用既有密码学基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `AesGcmCrypter`/`Pbkdf2Hasher`/`HmacSigner`/`sha256_hex`/`hmac_sha256`（`packages/sz-orm-crypto/src/lib.rs`）+ 既有 KAT 测试（`packages/sz-orm-crypto/tests/kat.rs`）+ 既有黑帽 PoC（`packages/sz-orm-crypto/tests/blackhat_poc.rs`），不重复实现密码学逻辑。
   a. 验收条件：[A02 渗透测试] → [复用既有 AesGcmCrypter/Pbkdf2Hasher/HmacSigner + KAT/黑帽 PoC，不新建密码学逻辑]
8. **禁止项**（EARS: Unwanted）
   如果 A02 渗透测试影响默认编译或破坏既有密码学，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用，且既有 `AesGcmCrypter`/`Pbkdf2Hasher`/`HmacSigner` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无 A02 渗透测试，行为与 v4.8.0 一致]

### 5.2.2 交互流程

```plantuml
@startuml
title A02 加密失败深化渗透测试 交互流程
actor "渗透测试" as test
participant "sz-orm-config\n(既有 prod_ready)" as config
participant "AesGcmCrypter\n(既有)" as aes
participant "Pbkdf2Hasher\n(既有)" as pbkdf2
participant "OsRng\n(既有 CSPRNG)" as rng

test -> config : 明文连接串 mysql://
config --> test : 拒绝 (要求 TLS)

test -> test : grep Md5/Des/Rc4/Ecb in src/
test --> test : 无弱算法 (仅 SHA-256/AES-256-GCM)

test -> test : grep 硬编码密钥 in src/
test --> test : 无硬编码 (全部在 tests/)

test -> aes : 加密 "plaintext" 两次
aes -> rng : OsRng nonce
aes --> test : 两次密文不同 (GCM 非 ECB)

test -> pbkdf2 : 迭代 1000
pbkdf2 --> test : 拒绝 (M-8: ≥ 100_000)
@enduml
```

### 5.2.3 异常场景

1. **TLS 证书无效**
   a. 触发条件：数据库 TLS 证书过期/自签名/CA 不受信
   b. 系统行为：连接拒绝，返回 TLS 错误
   c. 用户感知：错误"TLS certificate invalid"
2. **密钥长度不足**
   a. 触发条件：加密密钥短于最低要求
   b. 系统行为：拒绝初始化，返回 `CryptoError::InvalidKeyLength`
   c. 用户感知：错误"key length insufficient"

## 5.3 A03 注入深化渗透测试（REQ-V49-003，P1）

### 5.3.1 业务规则

1. **NoSQL 注入渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a03_injection.rs` 测试，构造 NoSQL 注入攻击向量：通过 `QueryBuilder` 构造含 NoSQL 操作符的查询（`{"$ne": null}` / `{"$gt": ""}`），断言 `QueryBuilder`（`packages/sz-orm-core/src/query.rs`）参数化绑定（值只出现在 params 中，不内联到 SQL），NoSQL 操作符被拒绝或转义。
   a. 验收条件：[查询 where_eq("field", Value::String("{\"$ne\": null}"))] → [值参数化绑定，`$ne` 不被解释为操作符，作为字面量字符串处理]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a03_injection`
2. **OS 命令注入渗透测试**（EARS: Ubiquitous）
   系统应当构造 OS 命令注入攻击向量：检测生产代码中 `std::process::Command::new(user_input)` 或 `Command::new("sh").arg("-c").arg(user_input)` 模式，断言无用户输入直接拼接命令（沿用既有 FIND-003 修复 `packages/sz-orm-macros/src/lib.rs:1602` 命令行密码改用环境变量/stdin）。
   a. 验收条件：[grep "Command::new(" src/ + arg(user_input)] → [无用户输入直接拼接命令，全部使用参数化 args 或环境变量]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a03_injection` + `grep -rn "Command::new" packages/*/src/`
3. **模板注入渗透测试**（EARS: Ubiquitous）
   系统应当构造模板注入攻击向量：检测低代码引擎（`packages/sz-orm-lc/src/lib.rs` `CrudTemplateEngine` / `FormGenerator`）生成 HTML 时是否转义模板语法（`{{7*7}}` / `${7*7}` / `<%= 7*7 %>`），断言用户输入经 HTML 转义后不执行模板语法。
   a. 验收条件：[用户输入 "{{7*7}}" + FormGenerator 生成 HTML] → [HTML 中 `{{7*7}}` 被转义为 `&amp;#123;&amp;#123;7*7&amp;#125;&amp;#125;`，不执行模板]
   验证方法：`cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_a03_injection`
4. **表达式注入渗透测试**（EARS: Ubiquitous）
   系统应当构造表达式注入攻击向量：检测 OpenAPI 反向生成（`packages/sz-orm-swagger/src/reverse/mod.rs`）是否将用户输入作为表达式求值，断言 `OpenApiInjectionGuard`（`:27`）拒绝含表达式语法的 spec（`ReverseGenError::InjectionDetected` `:36`）。
   a. 验收条件：[OpenAPI spec 含 `${7*7}` 表达式 + 反向生成] → [`OpenApiInjectionGuard` 拒绝，返回 `InjectionDetected`]
   验证方法：`cargo test -p sz-orm-swagger --features openapi-reverse,owasp-pentest-suite --test owasp_a03_injection`
5. **Header 注入渗透测试**（EARS: Ubiquitous）
   系统应当构造 Header 注入攻击向量：检测 HTTP 响应头设置是否过滤 CRLF 字符（`\r\n`），断言 `Set-Cookie`/`Location`/`Content-Disposition` 等头部值不含 CRLF（防止响应拆分/Header 注入）。
   a. 验收条件：[设置 Location 头部为 "https://evil.com\r\nSet-Cookie: evil=1"] → [CRLF 被过滤或拒绝，不产生额外头部]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a03_injection`
6. **SQL 注入深化渗透测试**（EARS: Ubiquitous）
   系统应当深化既有 SQL 注入测试（`packages/sz-orm-core/tests/security_attacks.rs` + `scripts/check-sql-injection.ps1` + FIND-001 修复 `packages/sz-orm-lc/src/lib.rs:954` model.name 验证），新增攻击向量：UNION 注入（`' UNION SELECT * FROM users--`）/ 堆叠注入（`; DROP TABLE users--`）/ 盲注（`' AND 1=1--` vs `' AND 1=2--`）/ 二阶注入（先存储恶意值，后续查询触发），断言 `QueryBuilder` 参数化绑定 + `ModelDefinition::new` 表名验证（FIND-001 修复）。
   a. 验收条件：[UNION 注入 `' UNION SELECT * FROM users--`] → [参数化绑定，注入字符串作为字面量处理]；[堆叠注入 `; DROP TABLE users--`] → [参数化绑定，不执行额外语句]；[model.name = `users" DROP TABLE users; --`] → [`ModelDefinition::new` 拒绝（FIND-001 修复），表名只允许字母/数字/下划线]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a03_injection` + `cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_a03_injection`
7. **复用既有注入防护基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `QueryBuilder`（`packages/sz-orm-core/src/query.rs`）/ `OpenApiInjectionGuard`（`packages/sz-orm-swagger/src/reverse/mod.rs:27`）/ `ModelDefinition::new` 表名验证（FIND-001 修复）/ `scripts/check-sql-injection.ps1`，不重复实现注入防护逻辑。
   a. 验收条件：[A03 渗透测试] → [复用既有 QueryBuilder/OpenApiInjectionGuard/ModelDefinition::new/check-sql-injection，不新建注入防护逻辑]
8. **禁止项**（EARS: Unwanted）
   如果 A03 渗透测试影响默认编译或破坏既有注入防护，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用，且既有 `QueryBuilder`/`OpenApiInjectionGuard`/`ModelDefinition::new` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无 A03 渗透测试，行为与 v4.8.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title A03 注入深化渗透测试 交互流程
actor "渗透测试" as test
participant "QueryBuilder\n(既有)" as qb
participant "OpenApiInjectionGuard\n(既有)" as guard
participant "ModelDefinition::new\n(既有 FIND-001 修复)" as model
participant "FormGenerator\n(既有)" as form

test -> qb : NoSQL: where_eq("field", "{\"$ne\": null}")
qb --> test : 参数化绑定 ($ne 作为字面量)

test -> test : grep Command::new(user_input) in src/
test --> test : 无 OS 命令注入 (FIND-003 修复)

test -> form : 模板注入: "{{7*7}}"
form --> test : HTML 转义 (不执行模板)

test -> guard : 表达式注入: "${7*7}" in spec
guard --> test : InjectionDetected (拒绝)

test -> qb : SQL UNION: ' UNION SELECT * FROM users--
qb --> test : 参数化绑定 (字面量)

test -> model : model.name = "users\" DROP TABLE users; --"
model --> test : 拒绝 (FIND-001: 仅字母/数字/下划线)
@enduml
```

### 5.3.3 异常场景

1. **参数化绑定失败**
   a. 触发条件：`QueryBuilder` 未能参数化某条件（如 `where_cond` 已 deprecated）
   b. 系统行为：编译期警告（`#[deprecated]`）+ 运行时拒绝
   c. 用户感知：警告"use where_eq/or_where_eq instead"
2. **表名验证失败**
   a. 触发条件：`ModelDefinition::new` 接收非法表名（含特殊字符）
   b. 系统行为：返回错误"invalid model name"（FIND-001 修复）
   c. 用户感知：错误"model name must be alphanumeric + underscore"

## 5.4 A04 不安全设计渗透测试（REQ-V49-004，P1）

### 5.4.1 业务规则

1. **业务逻辑缺陷渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a04_insecure_design.rs` 测试，构造业务逻辑缺陷攻击向量：负数数量（`quantity = -1` 导致负金额）/ 跳过支付步骤（直接调用确认接口）/ 重复使用优惠码（同一码多次使用），断言业务规则校验拒绝非法输入。
   a. 验收条件：[订单 quantity = -1] → [校验拒绝，提示"quantity must be positive"]；[未支付直接确认] → [校验拒绝，提示"payment required before confirm"]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a04_insecure_design`
2. **缺失限流渗透测试**（EARS: Ubiquitous）
   系统应当构造缺失限流攻击向量：检测关键接口（登录/密码重置/API 查询）是否配置限流，断言 `WasmDbRateLimiter`（`packages/sz-orm-wasm/src/real_db/mod.rs:26`）/ `QuotaEnforcer`（`packages/sz-orm-core/src/tenant_quota_rls.rs`）强制限流，超限拒绝（`WasmRealDbError::RateLimited`）。
   a. 验收条件：[连续 1000 次登录尝试 + 限流 100/min] → [第 101 次拒绝，返回 RateLimited]；[未配置限流] → [渗透测试标记"missing rate limiting"为发现]
   验证方法：`cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a04_insecure_design`
3. **缺失重试上限渗透测试**（EARS: Ubiquitous）
   系统应当构造缺失重试上限攻击向量：检测 `RetryPolicy`（`packages/sz-orm-grpc/src/lib.rs:415`）是否配置最大重试次数，断言无限重试被拒绝（最大重试后返回错误）。
   a. 验收条件：[RetryPolicy max_retries=3 + 连续失败 10 次] → [第 4 次后停止重试，返回错误]；[max_retries=0 + 失败] → [不重试，直接返回错误]
   验证方法：`cargo test -p sz-orm-grpc --features owasp-pentest-suite --test owasp_a04_insecure_design`
4. **缺失幂等性渗透测试**（EARS: Ubiquitous）
   系统应当构造缺失幂等性攻击向量：重复提交同一请求（相同 idempotency_key），断言 `CrossLangCompensationSerializer`（`packages/sz-orm-dtx/src/cross_lang/serializer.rs`）生成幂等键，重复调用返回缓存结果不重复执行副作用。
   a. 验收条件：[请求 idempotency_key="key-1" + 重复提交 3 次] → [第 2/3 次返回第 1 次结果，不重复执行副作用]
   验证方法：`cargo test -p sz-orm-dtx --features cross-lang-dtx,owasp-pentest-suite --test owasp_a04_insecure_design`
5. **缺失资源释放渗透测试**（EARS: Ubiquitous）
   系统应当构造缺失资源释放攻击向量：检测连接池（`Pool` `packages/sz-orm-core/src/pool.rs`）/ 文件句柄 / 锁是否在异常路径释放，断言 `Drop` 实现或显式释放（沿用既有 FIND-002 修复 Mutex poisoning 使用 parking_lot::Mutex）。
   a. 验收条件：[获取连接 + 异常 + 未显式释放] → [Drop 自动释放连接回池]；[Mutex lock + panic] → [parking_lot::Mutex 不 poisoning（FIND-002 修复）]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a04_insecure_design`
6. **竞态条件渗透测试**（EARS: State-driven）
   当并发访问共享资源时，系统应当通过 `owasp_a04_insecure_design.rs` 测试构造竞态攻击向量：并发扣减余额（`balance -= amount`）导致负余额/双重消费，断言原子操作（`AtomicU64`/`compare_exchange`）或锁保护。
   a. 验收条件：[100 并发扣减 balance=100 amount=1] → [最终 balance=0，无负余额/双重消费，原子操作保护]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a04_insecure_design`
7. **TOCTOU 渗透测试**（EARS: State-driven）
   当检查与使用之间存在时间窗口时，系统应当构造 TOCTOU 攻击向量：检查余额（`balance >= amount`）后扣款前余额被另一线程改变，断言原子 `compare_exchange` 或事务保护（`DtxManager` `packages/sz-orm-dtx/src/lib.rs:432`）。
   a. 验收条件：[线程 A 检查 balance=100 >= amount=100 + 线程 B 扣减 balance=100 → balance=0 + 线程 A 扣减] → [线程 A 扣减失败（balance=0 < amount=100），TOCTOU 被原子操作阻止]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a04_insecure_design`
8. **复用既有设计约束基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `WasmDbRateLimiter`/`RetryPolicy`/`CrossLangCompensationSerializer`/`Pool`/`QuotaEnforcer`/`DtxManager`，不重复实现设计约束逻辑。
   a. 验收条件：[A04 渗透测试] → [复用既有 WasmDbRateLimiter/RetryPolicy/CrossLangCompensationSerializer/Pool/QuotaEnforcer/DtxManager，不新建设计约束逻辑]
9. **禁止项**（EARS: Unwanted）
   如果 A04 渗透测试影响默认编译或破坏既有设计约束，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无 A04 渗透测试，行为与 v4.8.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title A04 不安全设计渗透测试 交互流程
actor "渗透测试" as test
participant "WasmDbRateLimiter\n(既有)" as limiter
participant "RetryPolicy\n(既有)" as retry
participant "Pool\n(既有)" as pool
participant "AtomicU64\n(原子操作)" as atomic

test -> test : 业务逻辑: quantity = -1
test --> test : 校验拒绝 (quantity must be positive)

test -> limiter : 1000 次登录 + 限流 100/min
limiter --> test : 第 101 次拒绝 (RateLimited)

test -> retry : max_retries=3 + 失败 10 次
retry --> test : 第 4 次停止 (返回错误)

test -> pool : 获取连接 + 异常
pool --> test : Drop 自动释放 (无泄露)

test -> atomic : 100 并发扣减 balance=100
atomic --> test : balance=0 (无负余额, 原子保护)

test -> atomic : TOCTOU: 检查后扣减前余额改变
atomic --> test : compare_exchange 失败 (TOCTOU 阻止)
@enduml
```

### 5.4.3 异常场景

1. **限流配置缺失**
   a. 触发条件：关键接口未配置限流
   b. 系统行为：渗透测试标记"missing rate limiting"为发现
   c. 用户感知：审计报告记录"interface X has no rate limiting"
2. **幂等键冲突**
   a. 触发条件：不同请求使用相同 idempotency_key
   b. 系统行为：返回第 1 次结果，不重复执行
   c. 用户感知：响应与第 1 次一致

## 5.5 A05 安全配置错误深化渗透测试（REQ-V49-005，P1）

### 5.5.1 业务规则

1. **默认配置渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a05_misconfig.rs` 测试，构造默认配置攻击向量：检测是否使用不安全默认值（默认密码/默认端口/默认允许所有来源 CORS），断言 `prod_ready.rs`（`packages/sz-orm-config/src/prod_ready.rs`）生产配置校验拒绝不安全默认值。
   a. 验收条件：[配置 password="admin"] → [生产配置校验拒绝，提示"weak default password"]；[配置 CORS allow_origins="*"] → [拒绝，提示"specify explicit origins"]
   验证方法：`cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a05_misconfig`
2. **调试模式渗透测试**（EARS: Ubiquitous）
   系统应当构造调试模式攻击向量：检测生产环境是否启用调试模式（`RUST_LOG=debug`/`cfg!(debug_assertions)`/`#[cfg(debug_assertions)]`），断言生产构建（`--release`）不启用调试代码路径。
   a. 验收条件：[`cargo build --release` + grep "debug_assertions" in src/] → [生产构建不启用调试代码路径]；[RUST_LOG=debug in 生产] → [拒绝或降级为 info]
   验证方法：`cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a05_misconfig` + `grep -rn "debug_assertions\|RUST_LOG=debug" packages/*/src/`
3. **错误消息泄露渗透测试**（EARS: Ubiquitous）
   系统应当构造错误消息泄露攻击向量：检测错误消息是否泄露内部信息（堆栈/SQL/文件路径/版本号），断言生产错误消息为用户友好消息（不泄露内部细节），沿用既有审计（93 处 `eprintln!` 全部是错误日志，未打印密码/token）。
   a. 验收条件：[触发 SQL 错误 + 生产模式] → [错误消息为"query failed"，不泄露 SQL 语句/表名/列名]；[错误日志] → [记录完整错误供调试，但不返回给用户]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a05_misconfig`
4. **默认密码渗透测试**（EARS: Ubiquitous）
   系统应当构造默认密码攻击向量：检测是否使用出厂默认密码（admin/admin / root/root / test/test123），断言 `prod_ready.rs` 拒绝默认密码（沿用既有 `packages/sz-orm-config/src/prod_ready.rs:104` "production environment must define at least one sensitive field rule"）。
   a. 验收条件：[配置 password="admin"/"root"/"test123"] → [生产配置校验拒绝，提示"weak default password"]
   验证方法：`cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a05_misconfig`
5. **不必要功能启用渗透测试**（EARS: Ubiquitous）
   系统应当构造不必要功能攻击向量：检测生产构建是否启用不必要 feature（如 `real-es`/`real-broker` 在不需要 ES/MQTT 的场景），断言 `deny.toml`（`deny.toml:16` `all-features = true`）检查所有 feature 组合的安全影响。
   a. 验收条件：[生产构建 + 启用 real-es 但不使用 ES] → [警告或拒绝，提示"unnecessary feature real-es enabled"]；[cargo deny check all-features] → [所有 feature 组合安全公告检查]
   验证方法：`cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a05_misconfig` + `cargo deny check`
6. **目录列举渗透测试**（EARS: Ubiquitous）
   系统应当构造目录列举攻击向量：检测静态文件服务是否启用目录列举（`index.html` 缺失时列出目录内容），断言目录列举关闭。
   a. 验收条件：[访问 /static/ 无 index.html] → [返回 403/404，不列出目录内容]
   验证方法：`cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a05_misconfig`
7. **复用既有配置校验基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `prod_ready.rs`（`packages/sz-orm-config/src/prod_ready.rs`）/ `deny.toml`，不重复实现配置校验逻辑。
   a. 验收条件：[A05 渗透测试] → [复用既有 prod_ready.rs/deny.toml，不新建配置校验逻辑]
8. **禁止项**（EARS: Unwanted）
   如果 A05 渗透测试影响默认编译或破坏既有配置校验，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无 A05 渗透测试，行为与 v4.8.0 一致]

### 5.5.2 交互流程

```plantuml
@startuml
title A05 安全配置错误深化渗透测试 交互流程
actor "渗透测试" as test
participant "prod_ready.rs\n(既有)" as config

test -> config : 默认密码 "admin"
config --> test : 拒绝 (weak default password)

test -> test : grep debug_assertions in src/
test --> test : 生产构建不启用调试路径

test -> test : 触发 SQL 错误
test --> test : 用户消息 "query failed" (不泄露 SQL)

test -> config : CORS allow_origins="*"
config --> test : 拒绝 (specify explicit origins)

test -> test : 访问 /static/ 无 index.html
test --> test : 403 (目录列举关闭)
@enduml
```

### 5.5.3 异常场景

1. **生产配置缺失**
   a. 触发条件：生产环境未配置必需的敏感字段规则
   b. 系统行为：`prod_ready.rs` 拒绝启动（沿用既有 `:104`）
   c. 用户感知：错误"production environment must define at least one sensitive field rule"
2. **不安全 CORS**
   a. 触发条件：CORS 配置 `allow_origins="*"` + `allow_credentials=true`
   b. 系统行为：拒绝，提示"cannot use wildcard origin with credentials"
   c. 用户感知：错误"unsafe CORS configuration"

## 5.6 A06 易受攻击和过时的组件深化渗透测试（REQ-V49-006，P1）

### 5.6.1 业务规则

1. **CVE 漏洞追踪渗透测试**（EARS: Ubiquitous）
   系统应当提供 `scripts/owasp_a06_vulnerable_components.ps1` + Bash 等价脚本，构造 CVE 漏洞追踪攻击向量：运行 `cargo audit` 检查 RUSTSEC 公告，断言无未忽略的漏洞公告（沿用既有 `deny.toml:36` 忽略清单 11 项带 reason）。
   a. 验收条件：[`cargo audit`] → [无未忽略的 RUSTSEC 公告，忽略清单 11 项均有 reason]；[新 RUSTSEC 公告] → [脚本标记为发现，须评估并添加到 ignore 或升级]
   验证方法：`cargo audit` + `pwsh scripts/owasp_a06_vulnerable_components.ps1`
2. **许可证合规深化渗透测试**（EARS: Ubiquitous）
   系统应当构造许可证合规攻击向量：运行 `cargo deny check licenses` 检查依赖许可证，断言全部在白名单（`deny.toml:99` MIT/Apache-2.0/BSD/ISC/Zlib/MPL-2.0 等），无 copyleft（GPL/AGPL/LGPL）。
   a. 验收条件：[`cargo deny check licenses`] → [全部许可证在白名单，无 copyleft]；[依赖含 GPL] → [拒绝，提示"copyleft license GPL-3.0 not allowed"]
   验证方法：`cargo deny check licenses` + `pwsh scripts/owasp_a06_vulnerable_components.ps1`
3. **Yanked Crate 检测渗透测试**（EARS: Ubiquitous）
   系统应当构造 yanked crate 攻击向量：运行 `cargo deny check` 检测 yanked crate（`deny.toml:89` `yanked = "warn"`），断言无 yanked 依赖（或警告并记录）。
   a. 验收条件：[`cargo deny check`] → [无 yanked 依赖，或警告并记录]；[依赖 yanked] → [警告"crate X version Y is yanked"]
   验证方法：`cargo deny check` + `pwsh scripts/owasp_a06_vulnerable_components.ps1`
4. **重复依赖检测渗透测试**（EARS: Ubiquitous）
   系统应当构造重复依赖攻击向量：运行 `cargo deny check bans` 检测重复依赖（`deny.toml:131` `multiple-versions = "warn"`），断言无重复依赖（或警告并记录版本碎片化）。
   a. 验收条件：[`cargo deny check bans`] → [无重复依赖，或警告并记录]；[同一 crate 两个版本] → [警告"crate X has multiple versions: A, B"]
   验证方法：`cargo deny check bans` + `pwsh scripts/owasp_a06_vulnerable_components.ps1`
5. **SBOM 生成渗透测试**（EARS: Ubiquitous）
   系统应当构造 SBOM 生成验证：运行 `cargo cyclonedx` 或等价工具生成 SBOM（CycloneDX 格式），断言 SBOM 包含所有依赖及其版本/来源/许可证。
   a. 验收条件：[`cargo cyclonedx`] → [生成 sbom.json，含所有依赖 + version + license + source]；[SBOM 缺失依赖] → [标记为发现]
   验证方法：`cargo cyclonedx` + `pwsh scripts/owasp_a06_vulnerable_components.ps1`
6. **依赖来源限制渗透测试**（EARS: Ubiquitous）
   系统应当构造依赖来源攻击向量：运行 `cargo deny check sources` 检测依赖来源（`deny.toml:148` `unknown-registry = "deny"` / `:150` `unknown-git = "deny"`），断言全部依赖来自 crates.io（无未知 registry/git 来源）。
   a. 验收条件：[`cargo deny check sources`] → [全部依赖来自 crates.io，无未知 registry/git]；[依赖来自 git] → [拒绝，提示"unknown git source"]
   验证方法：`cargo deny check sources` + `pwsh scripts/owasp_a06_vulnerable_components.ps1`
7. **复用既有依赖审计基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `deny.toml` + `cargo audit` + `cargo deny check`，不重复实现依赖审计逻辑。
   a. 验收条件：[A06 渗透测试] → [复用既有 deny.toml/cargo audit/cargo deny，不新建依赖审计逻辑]
8. **禁止项**（EARS: Unwanted）
   如果 A06 渗透测试影响默认编译或破坏既有依赖审计，则系统应当通过脚本隔离（`scripts/owasp_a06_vulnerable_components.ps1`），不修改既有 `deny.toml`。
   a. 验收条件：[`cargo build` 默认编译] → [无 A06 渗透测试，行为与 v4.8.0 一致]

### 5.6.2 交互流程

```plantuml
@startuml
title A06 易受攻击和过时的组件深化渗透测试 交互流程
actor "依赖安全工程师" as engineer
participant "cargo audit" as audit
participant "cargo deny" as deny
participant "cargo cyclonedx" as cyclonedx
participant "deny.toml\n(既有)" as config

engineer -> audit : cargo audit (RUSTSEC)
audit -> config : 查询 ignore 清单
config --> audit : 11 项忽略 (带 reason)
audit --> engineer : 无未忽略公告

engineer -> deny : cargo deny check licenses
deny -> config : 查询 allow 白名单
config --> deny : MIT/Apache/BSD/ISC/Zlib/MPL
deny --> engineer : 无 copyleft

engineer -> deny : cargo deny check (yanked/bans/sources)
deny --> engineer : 无 yanked/无重复/无未知来源

engineer -> cyclonedx : cargo cyclonedx
cyclonedx --> engineer : sbom.json (全依赖 + 版本 + 许可证)
@enduml
```

### 5.6.3 异常场景

1. **新 RUSTSEC 公告**
   a. 触发条件：`cargo audit` 发现新 RUSTSEC 公告未在 ignore 清单
   b. 系统行为：脚本标记为发现，须评估影响并添加到 ignore 或升级依赖
   c. 用户感知：审计报告记录"new RUSTSEC advisory: X, needs evaluation"
2. **许可证不合规**
   a. 触发条件：依赖含 copyleft 许可证（GPL/AGPL）
   b. 系统行为：`cargo deny check licenses` 拒绝
   c. 用户感知：错误"copyleft license X not allowed"

## 5.7 A07 软件和数据完整性失败渗透测试（REQ-V49-007，P1）

### 5.7.1 业务规则

1. **CI/CD 管道完整性渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a07_integrity.rs` 测试，构造 CI/CD 管道完整性攻击向量：检测 CI/CD 管道是否签名提交/签名构建产物/可重现构建，断言 23 道门禁（AGENTS.md）全部通过（特别是门禁 11 上游仓库未修改 + 门禁 13 审计证据验证 + 门禁 15 幻影交付检查）。
   a. 验收条件：[CI/CD 运行 23 道门禁] → [全部通过，无门禁失败]；[上游仓库修改] → [门禁 11 拒绝，提示"ADR-0001 violated"]；[审计证据虚假] → [门禁 13 拒绝，提示"audit evidence invalid"]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a07_integrity` + `pwsh scripts/gate.ps1`
2. **签名验证渗透测试**（EARS: Ubiquitous）
   系统应当构造签名验证攻击向量：检测软件包/依赖/构建产物是否签名验证，断言 `HashChainAuditor`（`packages/sz-orm-audit/src/lib.rs`）哈希链防篡改（SHA-256 哈希链，篡改任意日志条目使 `verify()` 失败）。
   a. 验收条件：[审计日志哈希链 + 篡改第 5 条日志] → [`HashChainAuditor::verify()` 失败，检测到篡改]；[正常追加日志] → [`verify()` 通过]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a07_integrity`
3. **反序列化完整性渗透测试**（EARS: Ubiquitous）
   系统应当构造反序列化完整性攻击向量：检测 `serde_json::from_str` 反序列化是否验证数据完整性（类型/长度/签名），断言反序列化后的数据使用方式安全（沿用既有审计：134 处 serde_json 反序列化，serde_json 不执行代码，风险在于反序列化后的数据使用方式）。
   a. 验收条件：[恶意 JSON `{"__proto__": {"admin": true}}` + 反序列化] → [反序列化为普通结构，`__proto__` 不被特殊处理，无原型污染]；[超长字段 + 反序列化] → [长度校验拒绝，提示"field too long"]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a07_integrity`
4. **构建可重现性渗透测试**（EARS: Ubiquitous）
   系统应当构造构建可重现性攻击向量：检测相同输入是否产生位级相同的构建产物（`cargo build --release` 两次 + 比较产物哈希），断言构建可重现（或记录不可重现原因：嵌入时间戳/路径/随机）。
   a. 验收条件：[`cargo build --release` 两次 + 比较产物哈希] → [哈希相同（可重现），或记录不可重现原因]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a07_integrity`
5. **依赖完整性渗透测试**（EARS: Ubiquitous）
   系统应当构造依赖完整性攻击向量：检测依赖是否来自可信来源（crates.io）+ 未被篡改（`cargo deny check sources` `deny.toml:148` `unknown-registry = "deny"`），断言全部依赖来自 crates.io（沿用 A06 依赖来源限制）。
   a. 验收条件：[`cargo deny check sources`] → [全部依赖来自 crates.io，无未知来源]；[依赖被篡改] → [哈希不匹配，拒绝]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a07_integrity` + `cargo deny check sources`
6. **审计日志哈希链深化渗透测试**（EARS: Ubiquitous）
   系统应当深化既有 `HashChainAuditor`（`packages/sz-orm-audit/src/lib.rs`）测试，构造攻击向量：删除中间日志条目 / 逆序日志 / 重放日志，断言 `verify()` 检测所有篡改。
   a. 验收条件：[删除第 5 条日志] → [`verify()` 失败]；[逆序日志] → [`verify()` 失败]；[重放日志] → [`verify()` 失败或检测重复]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a07_integrity`
7. **复用既有完整性基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `HashChainAuditor`（`packages/sz-orm-audit/src/lib.rs`）/ 23 道门禁（AGENTS.md）/ `cargo deny check sources`，不重复实现完整性逻辑。
   a. 验收条件：[A07 渗透测试] → [复用既有 HashChainAuditor/23 道门禁/cargo deny，不新建完整性逻辑]
8. **禁止项**（EARS: Unwanted）
   如果 A07 渗透测试影响默认编译或破坏既有完整性，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无 A07 渗透测试，行为与 v4.8.0 一致]

### 5.7.2 交互流程

```plantuml
@startuml
title A07 软件和数据完整性失败渗透测试 交互流程
actor "渗透测试" as test
participant "23 道门禁\n(既有 gate.ps1)" as gate
participant "HashChainAuditor\n(既有)" as chain
participant "serde_json\n(既有)" as serde

test -> gate : 运行 23 道门禁
gate --> test : 全部通过 (含门禁 11/13/15)

test -> chain : 哈希链 + 篡改第 5 条
chain --> test : verify() 失败 (检测篡改)

test -> serde : 恶意 JSON __proto__
serde --> test : 普通结构 (无原型污染)

test -> test : cargo build --release 两次
test --> test : 产物哈希相同 (可重现)

test -> chain : 删除/逆序/重放日志
chain --> test : verify() 失败 (检测所有篡改)
@enduml
```

### 5.7.3 异常场景

1. **门禁失败**
   a. 触发条件：23 道门禁中任意一道失败
   b. 系统行为：CI/CD 拒绝合并，提示失败门禁
   c. 用户感知：错误"gate X failed: ..."
2. **哈希链篡改**
   a. 触发条件：审计日志被篡改（删除/修改/重放）
   b. 系统行为：`HashChainAuditor::verify()` 失败
   c. 用户感知：错误"audit log integrity violated"

## 5.8 A08 安全日志和监控失败深化渗透测试（REQ-V49-008，P1）

### 5.8.1 业务规则

1. **日志注入渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a08_logging_failures.rs` 测试，构造日志注入攻击向量：用户输入含换行符（`"user\n2026-08-14 [INFO] admin logged in"`）注入日志，断言 `SqlAuditor::log`（`packages/sz-orm-audit/src/lib.rs:67`）转义/过滤换行符，不产生伪造日志条目。
   a. 验收条件：[用户输入 `"user\n[INFO] fake log"` + 审计日志] → [换行符被转义为 `\\n`，不产生伪造日志条目]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a08_logging_failures`
2. **日志脱敏深化渗透测试**（EARS: Ubiquitous）
   系统应当深化既有 `mask_sensitive`（`packages/sz-orm-audit/src/lib.rs:118`）测试，构造攻击向量：SQL 含 `password`/`token`/`credit_card`/`secret`/`api_key` 等敏感词（大小写混合/作为子串/作为标识符部分），断言全部脱敏为 `******`，不泄露敏感值。深化既有 `packages/sz-orm-audit/src/lib.rs:1002` `test_mask_sensitive_password` + `:1011` `test_mask_sensitive_case_insensitive`。
   a. 验收条件：[SQL `SELECT * FROM users WHERE PASSWORD='secret' AND Token='abc'`] → [脱敏为 `SELECT * FROM users WHERE ******='******' AND ******='******'`]；[SQL 含 `passwordless`（子串）] → [不脱敏（边界检查，`passwordless` 是完整标识符）]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a08_logging_failures`
3. **数据脱敏深化渗透测试**（EARS: Ubiquitous）
   系统应当深化既有 `DataMasker`（`packages/sz-orm-masking/src/lib.rs`）测试，构造攻击向量：PII 数据（手机号/邮箱/身份证/银行卡/姓名/地址/IP/IMEI/车牌/API key）脱敏，断言脱敏后不泄露原始数据且保持格式（沿用既有 `packages/sz-orm-masking/src/lib.rs:296` IdCard 测试 + `:317` BankCard 测试 + `:361` Address 测试）。
   a. 验收条件：[手机号 "13812345678" + Phone 规则] → [脱敏为 "138****5678"]；[身份证 "110101199001012345" + IdCard 规则] → [脱敏为 "1101**********2345"]；[API key 短于 8 字符] → [脱敏为 "***"（太短不泄露结构）]
   验证方法：`cargo test -p sz-orm-masking --features owasp-pentest-suite --test owasp_a08_logging_failures`
4. **告警渗透测试**（EARS: Event-driven）
   当安全事件发生时，系统应当通过 `owasp_a08_logging_failures.rs` 测试构造告警攻击向量：异常登录（5 次失败）/ 越权尝试（10 次）/ 注入检测（SQL/NoSQL）/ 限流触发，断言安全事件记录审计日志 + 触发告警（可配 webhook/log/metric）。
   a. 验收条件：[5 次登录失败] → [审计日志记录 5 次失败 + 触发"brute force"告警]；[10 次越权尝试] → [审计日志记录 + 触发"privilege escalation"告警]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a08_logging_failures`
5. **审计完整性渗透测试**（EARS: Ubiquitous）
   系统应当构造审计完整性攻击向量：审计日志追加写入不可篡改（`HashChainAuditor` 哈希链 + 追加写入），断言修改/删除历史日志被检测（沿用 A07 哈希链深化）。
   a. 验收条件：[追加写入新日志] → [成功，哈希链延伸]；[修改历史日志] → [`verify()` 失败]；[删除历史日志] → [`verify()` 失败]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a08_logging_failures`
6. **缺失监控检测渗透测试**（EARS: Ubiquitous）
   系统应当构造缺失监控攻击向量：检测关键操作（登录/越权/注入/限流/审计篡改）是否有审计日志 + 告警，断言无监控盲区。
   a. 验收条件：[关键操作列表 + 审计日志覆盖检查] → [全部关键操作有审计日志，无监控盲区]；[某操作无审计] → [标记为发现"missing monitoring for X"]
   验证方法：`cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a08_logging_failures`
7. **复用既有日志监控基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `SqlAuditor`/`mask_sensitive`/`HashChainAuditor`（`packages/sz-orm-audit/src/lib.rs`）/ `DataMasker`（`packages/sz-orm-masking/src/lib.rs`），不重复实现日志监控逻辑。
   a. 验收条件：[A08 渗透测试] → [复用既有 SqlAuditor/mask_sensitive/HashChainAuditor/DataMasker，不新建日志监控逻辑]
8. **禁止项**（EARS: Unwanted）
   如果 A08 渗透测试影响默认编译或破坏既有日志监控，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无 A08 渗透测试，行为与 v4.8.0 一致]

### 5.8.2 交互流程

```plantuml
@startuml
title A08 安全日志和监控失败深化渗透测试 交互流程
actor "渗透测试" as test
participant "SqlAuditor\n(既有)" as auditor
participant "mask_sensitive\n(既有)" as mask
participant "DataMasker\n(既有)" as data_mask
participant "HashChainAuditor\n(既有)" as chain

test -> auditor : 日志注入: "user\n[INFO] fake"
auditor --> test : 换行转义 (无伪造条目)

test -> mask : SQL 含 PASSWORD='secret'
mask --> test : 脱敏 ******='******'

test -> data_mask : 手机号 "13812345678"
data_mask --> test : "138****5678"

test -> auditor : 5 次登录失败
auditor -> chain : 记录 + 哈希链延伸
chain --> test : 触发 "brute force" 告警

test -> chain : 修改历史日志
chain --> test : verify() 失败 (篡改检测)
@enduml
```

### 5.8.3 异常场景

1. **审计日志写入失败**
   a. 触发条件：审计日志存储满/权限不足/IO 错误
   b. 系统行为：降级到内存缓冲 + 告警，不丢失日志
   c. 用户感知：告警"audit log write failed, buffering in memory"
2. **告警通道不可达**
   a. 触发条件：告警 webhook 不可达
   b. 系统行为：重试 + 降级到日志记录，不丢失告警
   c. 用户感知：告警"alert channel unreachable, retrying"

## 5.9 A09 身份识别和认证失败深化渗透测试（REQ-V49-009，P1）

### 5.9.1 业务规则

1. **会话固定渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a09_auth_failures.rs` 测试，构造会话固定攻击向量：攻击者预设 session_id="fixed-id"，用户登录后断言签发新 session_id（不复用攻击者预设的），`JwtAuthenticator`（`packages/sz-orm-auth/src/lib.rs`）登录后签发新 token。
   a. 验收条件：[攻击者预设 session_id="fixed" + 用户登录] → [登录后 session_id ≠ "fixed"，签发新 token，会话固定被阻止]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures`
2. **会话超时渗透测试**（EARS: State-driven）
   当会话空闲/绝对超时后，系统应当通过 `owasp_a09_auth_failures.rs` 测试构造超时攻击向量：签发 exp=now+3600 的 token，等待超时后断言 `JwtEncoder::decode` 拒绝（沿用既有 `packages/sz-orm-auth/tests/security_attacks.rs:41` `attack_expired_token_rejected`）。
   a. 验收条件：[token exp=now+3600 + 等待 3601 秒] → [`decode` 拒绝，会话超时失效]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures`
3. **并发会话渗透测试**（EARS: Ubiquitous）
   系统应当构造并发会话攻击向量：同一账户并发登录 10 次，断言并发会话数限制（可配，默认 5），超限拒绝新会话或踢出旧会话。
   a. 验收条件：[同一账户并发登录 10 次 + 限制 5] → [第 6 次拒绝或踢出最早会话，并发会话限制生效]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures`
4. **凭证填充渗透测试**（EARS: Ubiquitous）
   系统应当构造凭证填充攻击向量：使用泄露凭证字典（1000 对 username/password）批量尝试登录，断言限流（A04）+ 账户锁定（5 次失败锁定 15 分钟）+ 告警（A08）阻止凭证填充。
   a. 验收条件：[1000 对凭证 + 限流 100/min + 锁定 5 次] → [限流拒绝 + 锁定 + 告警，凭证填充被阻止]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures`
5. **弱密码渗透测试**（EARS: Ubiquitous）
   系统应当构造弱密码攻击向量：密码 "123456"/"password"/"admin"/"qwerty"（常见弱密码）/ 长度 < 8 / 无字符多样性，断言密码复杂度校验拒绝弱密码。
   a. 验收条件：[密码 "123456"] → [拒绝，提示"weak password: too common"]；[密码长度 6] → [拒绝，提示"password must be ≥ 8 chars"]；[密码 "abcdefgh"（无数字/特殊字符）] → [拒绝，提示"password must contain digits and special chars"]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures`
6. **账户枚举渗透测试**（EARS: Ubiquitous）
   系统应当构造账户枚举攻击向量：登录/注册/找回密码响应差异泄露账户是否存在（"user not found" vs "wrong password"），断言响应统一为"invalid credentials"（不区分用户不存在与密码错误）。
   a. 验收条件：[登录不存在的用户] → [响应"invalid credentials"，与密码错误相同]；[注册已存在的用户] → [响应"registration failed"（不泄露具体原因）]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures`
7. **MFA 绕过渗透测试**（EARS: Ubiquitous）
   系统应当构造 MFA 绕过攻击向量：跳过 MFA 直接访问受保护资源 / MFA 重放 / MFA 暴力（6 位 TOTP 100 万次枚举），断言 `MfaManager`（`packages/sz-orm-auth/src/mfa.rs`）强制 MFA + 限流 + 时间窗口（沿用既有 M-10 修复 `packages/sz-orm-auth/tests/blackhat_poc.rs:150` TOTP 空密钥拒绝）。
   a. 验收条件：[跳过 MFA 直接访问] → [拒绝，提示"MFA required"]；[MFA 重放] → [拒绝，提示"code already used"]；[MFA 暴力 100 万次] → [限流拒绝]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures`
8. **OAuth2 深化渗透测试**（EARS: Ubiquitous）
   系统应当深化既有 OAuth2 测试（`packages/sz-orm-auth/tests/blackhat_poc.rs:23` C-1 授权码可预测修复），构造攻击向量：redirect_uri 验证（开放重定向）/ state 验证（CSRF）/ PKCE（code_verifier），断言 `OAuth2Server`（`packages/sz-orm-auth/src/oauth2.rs`）强制验证。
   a. 验收条件：[redirect_uri="https://evil.com"] → [拒绝，提示"redirect_uri not registered"]；[state 缺失/不匹配] → [拒绝，提示"state validation failed"]；[PKCE 缺失] → [拒绝或降级，提示"PKCE required for public clients"]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures`
9. **复用既有认证基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `JwtAuthenticator`/`OAuth2Server`/`MfaManager`/`TotpVerifier`（`packages/sz-orm-auth/src/`），不重复实现认证逻辑。
   a. 验收条件：[A09 渗透测试] → [复用既有 JwtAuthenticator/OAuth2Server/MfaManager/TotpVerifier，不新建认证逻辑]
10. **禁止项**（EARS: Unwanted）
    如果 A09 渗透测试影响默认编译或破坏既有认证，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
    a. 验收条件：[`cargo build` 默认编译] → [无 A09 渗透测试，行为与 v4.8.0 一致]

### 5.9.2 交互流程

```plantuml
@startuml
title A09 身份识别和认证失败深化渗透测试 交互流程
actor "渗透测试" as test
participant "JwtAuthenticator\n(既有)" as jwt
participant "OAuth2Server\n(既有)" as oauth
participant "MfaManager\n(既有)" as mfa

test -> jwt : 会话固定: 预设 session_id
jwt --> test : 登录后签发新 token (不复用)

test -> jwt : 会话超时: exp=now+3600
jwt --> test : 超时后 decode 拒绝

test -> jwt : 并发会话 10 次 + 限制 5
jwt --> test : 第 6 次拒绝/踢出旧会话

test -> jwt : 凭证填充 1000 对
jwt --> test : 限流+锁定+告警 (阻止)

test -> jwt : 弱密码 "123456"
jwt --> test : 拒绝 (复杂度校验)

test -> jwt : 账户枚举: 不存在用户
jwt --> test : "invalid credentials" (不泄露)

test -> mfa : MFA 绕过/重放/暴力
mfa --> test : 强制 MFA+限流+时间窗口 (阻止)

test -> oauth : redirect_uri="https://evil.com"
oauth --> test : 拒绝 (未注册)
@enduml
```

### 5.9.3 异常场景

1. **账户锁定**
   a. 触发条件：5 次登录失败
   b. 系统行为：账户锁定 15 分钟，记录告警
   c. 用户感知：错误"account locked, retry after 15 minutes"
2. **MFA 时间窗口**
   a. 触发条件：TOTP 验证码超出时间窗口（±30 秒）
   b. 系统行为：拒绝，提示"code expired"
   c. 用户感知：错误"MFA code expired, request new one"

## 5.10 A10 SSRF 深化渗透测试（REQ-V49-010，P1）

### 5.10.1 业务规则

1. **内网探测渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_a10_ssrf.rs` 测试，构造内网探测攻击向量：`WasmRealDbConnection::new("http://127.0.0.1:8080", ...)` / `http://localhost:6379`（Redis）/ `http://192.168.1.1:22`（SSH），断言 FIND-004 修复的 URL 验证（`packages/sz-orm-wasm/src/real_db/connection.rs:33`）拒绝内网地址（可配内网白名单）。
   a. 验收条件：[proxy_url="http://127.0.0.1:8080"] → [拒绝，提示"internal address not allowed"]；[proxy_url="http://192.168.1.1:22"] → [拒绝]
   验证方法：`cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a10_ssrf`
2. **协议白名单渗透测试**（EARS: Ubiquitous）
   系统应当构造协议白名单攻击向量：`WasmRealDbConnection::new("file:///etc/passwd", ...)` / `gopher://...` / `dict://...` / `ftp://...`，断言 FIND-004 修复仅允许 `http`/`https` 协议（`packages/sz-orm-wasm/src/real_db/connection.rs` URL scheme 验证）。
   a. 验收条件：[proxy_url="file:///etc/passwd"] → [拒绝，提示"only http/https allowed"]；[proxy_url="gopher://..."] → [拒绝]
   验证方法：`cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a10_ssrf`
3. **DNS Rebinding 渗透测试**（EARS: Ubiquitous）
   系统应当构造 DNS rebinding 攻击向量：攻击者控制 DNS，首次解析为 `1.2.3.4`（合法 IP 通过校验），二次解析为 `127.0.0.1`（内网 IP 完成 SSRF），断言防御：解析后锁定 IP（pin IP）或二次解析校验。
   a. 验收条件：[DNS rebinding 首次 1.2.3.4 + 二次 127.0.0.1] → [IP 锁定或二次校验拒绝，DNS rebinding 被阻止]
   验证方法：`cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a10_ssrf`
4. **元数据端点渗透测试**（EARS: Ubiquitous）
   系统应当构造元数据端点攻击向量：`http://169.254.169.254/latest/meta-data/iam/security-credentials/`（AWS）/ `http://metadata.google.internal/computeMetadata/v1/`（GCP）/ `http://169.254.169.254/metadata/instance`（Azure），断言拒绝云元数据端点。
   a. 验收条件：[proxy_url="http://169.254.169.254/latest/meta-data/..."] → [拒绝，提示"metadata endpoint not allowed"]
   验证方法：`cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a10_ssrf`
5. **SSRF 防御深化渗透测试**（EARS: Ubiquitous）
   系统应当深化既有 FIND-004 修复（`packages/sz-orm-wasm/src/real_db/connection.rs:33` URL 验证），构造攻击向量：IPv6 内网（`[::1]`/`[fe80::1]`）/ 十进制 IP（`http://2130706433/` = `127.0.0.1`）/ 八进制 IP（`http://0177.0.0.1/`）/ DNS 短域名，断言全部拒绝。
   a. 验收条件：[proxy_url="http://[::1]:8080"] → [拒绝]；[proxy_url="http://2130706433/"] → [拒绝（十进制 IP 解析为 127.0.0.1）]；[proxy_url="http://0177.0.0.1/"] → [拒绝（八进制 IP）]
   验证方法：`cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a10_ssrf`
6. **复用既有 SSRF 防御基础设施**（EARS: Ubiquitous）
   系统应当复用既有 FIND-004 修复（`WasmRealDbConnection::new` URL 验证 `packages/sz-orm-wasm/src/real_db/connection.rs:33`），不重复实现 SSRF 防御逻辑。
   a. 验收条件：[A10 渗透测试] → [复用既有 FIND-004 修复 URL 验证，不新建 SSRF 防御逻辑]
7. **禁止项**（EARS: Unwanted）
   如果 A10 渗透测试影响默认编译或破坏既有 SSRF 防御，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无 A10 渗透测试，行为与 v4.8.0 一致]

### 5.10.2 交互流程

```plantuml
@startuml
title A10 SSRF 深化渗透测试 交互流程
actor "渗透测试" as test
participant "WasmRealDbConnection::new\n(既有 FIND-004 修复)" as conn

test -> conn : proxy_url="http://127.0.0.1:8080"
conn --> test : 拒绝 (内网地址)

test -> conn : proxy_url="file:///etc/passwd"
conn --> test : 拒绝 (仅 http/https)

test -> conn : DNS rebinding 首次 1.2.3.4
conn --> test : IP 锁定 (二次解析拒绝)

test -> conn : proxy_url="http://169.254.169.254/..."
conn --> test : 拒绝 (元数据端点)

test -> conn : proxy_url="http://2130706433/"
conn --> test : 拒绝 (十进制 IP → 127.0.0.1)
@enduml
```

### 5.10.3 异常场景

1. **URL 解析失败**
   a. 触发条件：proxy_url 不是合法 URL
   b. 系统行为：返回 `WasmRealDbError::InvalidUrl`（FIND-004 修复）
   c. 用户感知：错误"invalid URL"
2. **协议不允许**
   a. 触发条件：proxy_url 协议非 http/https
   b. 系统行为：返回 `WasmRealDbError::InvalidUrl`
   c. 用户感知：错误"only http/https allowed"

## 5.11 XSS 跨站脚本攻击渗透测试（REQ-V49-011，P1）

### 5.11.1 业务规则

1. **HTML 表单转义渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_xss.rs` 测试，构造 XSS 攻击向量：低代码引擎 `FormGenerator`（`packages/sz-orm-lc/src/lib.rs`）/ `CrudTemplateEngine` 生成 HTML 表单时，用户输入含 `<script>alert('xss')</script>` / `<img onerror=alert(1)>` / `<svg onload=alert(1)>`，断言 HTML 转义（`<` → `&lt;` / `>` → `&gt;` / `"` → `&quot;` / `'` → `&#x27;` / `&` → `&amp;`）。
   a. 验收条件：[用户输入 `<script>alert('xss')</script>` + FormGenerator 生成 HTML] → [HTML 中 `<script>` 被转义为 `&lt;script&gt;`，不执行脚本]
   验证方法：`cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_xss`
2. **反射型 XSS 渗透测试**（EARS: Ubiquitous）
   系统应当构造反射型 XSS 攻击向量：用户输入经 URL 参数（`?name=<script>alert(1)</script>`）反射到页面，断言反射前 HTML 转义。
   a. 验收条件：[URL 参数 `name=<script>alert(1)</script>` + 反射到页面] → [转义为 `&lt;script&gt;`，不执行]
   验证方法：`cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_xss`
3. **存储型 XSS 渗透测试**（EARS: Ubiquitous）
   系统应当构造存储型 XSS 攻击向量：用户输入存入数据库后读取并渲染到页面，断言渲染时 HTML 转义（存储原值，渲染转义）。
   a. 验收条件：[输入 `<script>alert(1)</script>` 存入 DB + 读取渲染] → [渲染时转义，不执行]
   验证方法：`cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_xss`
4. **DOM 型 XSS 渗透测试**（EARS: Ubiquitous）
   系统应当构造 DOM 型 XSS 攻击向量：用户输入经 `innerHTML` / `document.write` / `eval` 注入 DOM，断言使用 `textContent` / `createElement` 安全 API（或转义后赋值 `innerHTML`）。
   a. 验收条件：[用户输入 + `innerHTML` 赋值] → [转义后赋值，或使用 `textContent`，不执行脚本]
   验证方法：`cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_xss`
5. **HTML input 类型安全渗透测试**（EARS: Ubiquitous）
   系统应当构造 HTML input 类型安全攻击向量：`FieldTypeMapping::sql_to_html_input`（`packages/sz-orm-lc/src/lib.rs:210`）生成的 input 类型须安全（`text`/`number`/`email`/`date` 等，不生成 `file` 除非显式配置），断言 input `value` 属性转义。
   a. 验收条件：[字段类型 VARCHAR + `sql_to_html_input`] → [生成 `<input type="text">`，value 转义]；[字段类型 TEXT + 用户输入含 `">` + 生成 input] → [value 转义，不逃逸 input 属性]
   验证方法：`cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_xss`
6. **复用既有低代码基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `FormGenerator`/`CrudTemplateEngine`/`FieldTypeMapping::sql_to_html_input`（`packages/sz-orm-lc/src/lib.rs`），不重复实现 HTML 生成逻辑。
   a. 验收条件：[XSS 渗透测试] → [复用既有 FormGenerator/CrudTemplateEngine/FieldTypeMapping，不新建 HTML 生成逻辑]
7. **禁止项**（EARS: Unwanted）
   如果 XSS 渗透测试影响默认编译或破坏既有低代码引擎，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无 XSS 渗透测试，行为与 v4.8.0 一致]

### 5.11.2 交互流程

```plantuml
@startuml
title XSS 跨站脚本攻击渗透测试 交互流程
actor "渗透测试" as test
participant "FormGenerator\n(既有)" as form
participant "FieldTypeMapping\n(既有)" as mapping

test -> form : 输入 "<script>alert('xss')</script>"
form --> test : HTML 转义 &lt;script&gt; (不执行)

test -> form : 反射型 XSS: URL 参数
form --> test : 反射前转义 (不执行)

test -> form : 存储型 XSS: 存入 DB + 读取
form --> test : 渲染时转义 (不执行)

test -> form : DOM 型 XSS: innerHTML
form --> test : 转义或用 textContent (不执行)

test -> mapping : sql_to_html_input(VARCHAR)
mapping --> test : <input type="text"> value 转义
@enduml
```

### 5.11.3 异常场景

1. **转义遗漏**
   a. 触发条件：某字段未经 HTML 转义直接渲染
   b. 系统行为：渗透测试标记为发现"XSS vulnerability in field X"
   c. 用户感知：审计报告记录"field X not escaped, XSS risk"
2. **不安全 DOM API**
   a. 触发条件：使用 `innerHTML`/`document.write`/`eval` 处理用户输入
   b. 系统行为：渗透测试标记为发现"unsafe DOM API usage"
   c. 用户感知：审计报告记录"use textContent/createElement instead"

## 5.12 CSRF 跨站请求伪造渗透测试（REQ-V49-012，P1）

### 5.12.1 业务规则

1. **CSRF token 渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_csrf.rs` 测试，构造 CSRF 攻击向量：攻击者诱导已登录用户发送请求（无 CSRF token / 错误 token / 过期 token），断言 `OAuth2Server`（`packages/sz-orm-auth/src/oauth2.rs`）强制 CSRF token 验证（state 参数，沿用既有 C-1 修复 `packages/sz-orm-auth/tests/blackhat_poc.rs:23` OAuth2 授权码 OsRng）。
   a. 验收条件：[请求无 CSRF token] → [拒绝，提示"missing CSRF token"]；[CSRF token 不匹配] → [拒绝，提示"CSRF token mismatch"]；[CSRF token 过期] → [拒绝，提示"CSRF token expired"]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_csrf`
2. **SameSite Cookie 渗透测试**（EARS: Ubiquitous）
   系统应当构造 SameSite Cookie 攻击向量：Cookie 未设置 SameSite 属性（默认 None，允许跨站）或 SameSite=None，断言生产 Cookie 设置 SameSite=Strict 或 SameSite=Lax。
   a. 验收条件：[Cookie 无 SameSite] → [渗透测试标记为发现"missing SameSite attribute"]；[Cookie SameSite=None] → [拒绝或警告，提示"use SameSite=Strict or Lax"]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_csrf`
3. **Origin 验证渗透测试**（EARS: Ubiquitous）
   系统应当构造 Origin 验证攻击向量：请求 `Origin`/`Referer` 头部为 `https://evil.com`（跨站），断言验证 `Origin`/`Referer` 为合法来源（白名单匹配）。
   a. 验收条件：[请求 Origin="https://evil.com"] → [拒绝，提示"origin not allowed"]；[请求 Origin="https://legit.example.com"] → [通过]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_csrf`
4. **OAuth2 state 参数 CSRF 防御渗透测试**（EARS: Ubiquitous）
   系统应当构造 OAuth2 state 参数 CSRF 攻击向量：OAuth2 授权请求 `state` 参数缺失/不匹配/重放，断言 `OAuth2Server` 强制 state 验证（沿用既有 C-1 修复，state 须为 OsRng 随机 + 单次使用 + 绑定会话）。
   a. 验收条件：[OAuth2 授权请求无 state] → [拒绝，提示"state required"]；[state 不匹配] → [拒绝]；[state 重放] → [拒绝，提示"state already used"]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_csrf`
5. **登录 CSRF 渗透测试**（EARS: Ubiquitous）
   系统应当构造登录 CSRF 攻击向量：攻击者诱导受害者提交攻击者的凭证（登录 CSRF），断言防御：登录后签发新 session_id（不复用，沿用 A09 会话固定防御）+ CSRF token。
   a. 验收条件：[攻击者凭证 + 受害者提交] → [登录后签发新 session_id，攻击者无法复用]
   验证方法：`cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_csrf`
6. **复用既有 OAuth2 基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `OAuth2Server`（`packages/sz-orm-auth/src/oauth2.rs`）/ `JwtAuthenticator`（`packages/sz-orm-auth/src/lib.rs`），不重复实现 CSRF 防御逻辑。
   a. 验收条件：[CSRF 渗透测试] → [复用既有 OAuth2Server/JwtAuthenticator，不新建 CSRF 防御逻辑]
7. **禁止项**（EARS: Unwanted）
   如果 CSRF 渗透测试影响默认编译或破坏既有 OAuth2，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无 CSRF 渗透测试，行为与 v4.8.0 一致]

### 5.12.2 交互流程

```plantuml
@startuml
title CSRF 跨站请求伪造渗透测试 交互流程
actor "渗透测试" as test
participant "OAuth2Server\n(既有)" as oauth

test -> oauth : 请求无 CSRF token
oauth --> test : 拒绝 (missing CSRF token)

test -> oauth : CSRF token 不匹配
oauth --> test : 拒绝 (CSRF token mismatch)

test -> test : Cookie 无 SameSite
test --> test : 标记发现 (missing SameSite)

test -> oauth : Origin="https://evil.com"
oauth --> test : 拒绝 (origin not allowed)

test -> oauth : OAuth2 state 缺失/不匹配/重放
oauth --> test : 拒绝 (state validation failed)

test -> oauth : 登录 CSRF: 攻击者凭证
oauth --> test : 签发新 session_id (攻击者无法复用)
@enduml
```

### 5.12.3 异常场景

1. **CSRF token 缺失**
   a. 触发条件：请求未携带 CSRF token
   b. 系统行为：拒绝，返回 403
   c. 用户感知：错误"missing CSRF token"
2. **Origin 不允许**
   a. 触发条件：请求 Origin 不在白名单
   b. 系统行为：拒绝，返回 403
   c. 用户感知：错误"origin not allowed"

## 5.13 文件上传安全渗透测试（REQ-V49-013，P1）

### 5.13.1 业务规则

1. **文件类型验证渗透测试**（EARS: Ubiquitous）
   系统应当提供 `owasp_file_upload.rs` 测试，构造文件类型攻击向量：上传 `.php`/`.jsp`/`.exe`/`.sh`（可执行）/ `.html`/`.svg`（含脚本）/ 双扩展名 `evil.php.jpg` / 大小写 `evil.PHP`，断言 `StorageBackend::put`（`packages/sz-orm-storage/src/storage.rs:15`）验证文件类型白名单（仅允许配置的安全类型如 `.jpg`/`.png`/`.pdf`/`.txt`）。
   a. 验收条件：[上传 `evil.php`] → [拒绝，提示"file type not allowed"]；[上传 `evil.php.jpg`] → [拒绝（双扩展名检测）]；[上传 `evil.PHP`] → [拒绝（大小写不敏感）]
   验证方法：`cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload`
2. **文件大小限制渗透测试**（EARS: Ubiquitous）
   系统应当构造文件大小攻击向量：上传超大文件（10GB）/ 0 字节文件 / 负 Content-Length，断言文件大小限制（可配，默认 100MB）+ 0 字节拒绝 + 负值拒绝。
   a. 验收条件：[上传 10GB + 限制 100MB] → [拒绝，提示"file too large"]；[上传 0 字节] → [拒绝，提示"empty file"]；[Content-Length=-1] → [拒绝]
   验证方法：`cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload`
3. **内容验证渗透测试**（EARS: Ubiquitous）
   系统应当构造内容验证攻击向量：上传 `.jpg` 文件但内容为 PHP 代码 / `.png` 文件但内容为 HTML 脚本，断言 Magic bytes 验证（文件头魔数字节，不依赖扩展名）。
   a. 验收条件：[上传 `evil.jpg` 内容为 `<?php system($_GET['cmd']); ?>`] → [Magic bytes 不匹配 JPEG（`FF D8 FF`），拒绝]；[上传 `evil.png` 内容为 `<script>alert(1)</script>`] → [Magic bytes 不匹配 PNG（`89 50 4E 47`），拒绝]
   验证方法：`cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload`
4. **路径遍历渗透测试**（EARS: Ubiquitous）
   系统应当构造路径遍历攻击向量：上传文件名 `../../../etc/passwd` / `..\\..\\windows\\system32` / 绝对路径 `/etc/passwd`，断言文件名净化（移除 `../` / `..\\` / 绝对路径，沿用既有 `packages/sz-orm-wasm/src/advanced.rs` SandboxedFs 路径遍历防护）。
   a. 验收条件：[文件名 `../../../etc/passwd`] → [净化为 `passwd` 或拒绝，不遍历目录]；[文件名 `/etc/passwd`] → [拒绝绝对路径]
   验证方法：`cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload`
5. **Magic bytes 验证渗透测试**（EARS: Ubiquitous）
   系统应当构造 Magic bytes 攻击向量：常见文件类型 Magic bytes（JPEG `FF D8 FF` / PNG `89 50 4E 47` / GIF `47 49 46` / PDF `25 50 44 46` / ZIP `50 4B 03 04`），断言上传文件 Magic bytes 匹配扩展名。
   a. 验收条件：[上传 `.jpg` + Magic bytes `FF D8 FF`] → [通过]；[上传 `.jpg` + Magic bytes `50 4B 03 04`（ZIP）] → [拒绝，提示"Magic bytes mismatch: expected JPEG, got ZIP"]
   验证方法：`cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload`
6. **文件名净化渗透测试**（EARS: Ubiquitous）
   系统应当构造文件名净化攻击向量：文件名含特殊字符（空格/中文/Unicode/控制字符/Null byte `evil.jpg\0.php`），断言文件名净化（移除特殊字符/Null byte 截断防御）。
   a. 验收条件：[文件名 `evil.jpg\0.php`] → [Null byte 截断防御，拒绝或净化为 `evil.jpg`]
   验证方法：`cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload`
7. **临时文件清理渗透测试**（EARS: Ubiquitous）
   系统应当构造临时文件清理攻击向量：上传过程中产生的临时文件须在上传完成/失败后删除（沿用既有铁律：测试写入本地硬盘的临时文件在测试结束后必须及时删除并释放进程）。
   a. 验收条件：[上传完成/失败 + 检查临时目录] → [临时文件已删除，无残留]
   验证方法：`cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload`
8. **复用既有存储基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `StorageBackend` trait（`packages/sz-orm-storage/src/storage.rs:15` `put` 方法）/ `SandboxedFs`（`packages/sz-orm-wasm/src/advanced.rs` 路径遍历防护），不重复实现存储逻辑。
   a. 验收条件：[文件上传渗透测试] → [复用既有 StorageBackend/SandboxedFs，不新建存储逻辑]
9. **禁止项**（EARS: Unwanted）
   如果文件上传渗透测试影响默认编译或破坏既有存储，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无文件上传渗透测试，行为与 v4.8.0 一致]

### 5.13.2 交互流程

```plantuml
@startuml
title 文件上传安全渗透测试 交互流程
actor "渗透测试" as test
participant "StorageBackend::put\n(既有)" as storage

test -> storage : 上传 evil.php
storage --> test : 拒绝 (类型白名单)

test -> storage : 上传 10GB + 限制 100MB
storage --> test : 拒绝 (文件过大)

test -> storage : 上传 evil.jpg 内容为 PHP
storage --> test : Magic bytes 不匹配 (拒绝)

test -> storage : 文件名 ../../../etc/passwd
storage --> test : 净化/拒绝 (路径遍历)

test -> storage : 文件名 evil.jpg\0.php
storage --> test : Null byte 防御 (拒绝/净化)

test -> storage : 上传完成
storage --> test : 临时文件已删除 (无残留)
@enduml
```

### 5.13.3 异常场景

1. **文件类型不允许**
   a. 触发条件：上传文件类型不在白名单
   b. 系统行为：拒绝，返回错误
   c. 用户感知：错误"file type not allowed"
2. **Magic bytes 不匹配**
   a. 触发条件：文件扩展名与内容 Magic bytes 不匹配
   b. 系统行为：拒绝，返回错误
   c. 用户感知：错误"Magic bytes mismatch"

## 5.14 业务逻辑并发竞态条件渗透测试（REQ-V49-014，P1）

### 5.14.1 业务规则

1. **连接池竞态渗透测试**（EARS: State-driven）
   当并发获取/释放连接时，系统应当通过 `owasp_race_conditions.rs` 测试构造连接池竞态攻击向量：100 并发获取连接 + 池大小 10，断言 `Pool`（`packages/sz-orm-core/src/pool.rs`）原子操作（`AtomicU32` + `crossbeam-queue ArrayQueue` + `Notify`）无死锁/无重复发放/无泄露。
   a. 验收条件：[100 并发获取 + 池大小 10] → [最多 10 个并发连接，其余等待，无死锁/无重复/无泄露]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_race_conditions`
2. **租户配额竞态渗透测试**（EARS: State-driven）
   当并发检查/更新租户配额时，系统应当构造租户配额竞态攻击向量：100 并发 `QuotaEnforcer::check_quota`（`packages/sz-orm-core/src/tenant_quota_rls.rs:229`）+ 配额 10，断言原子操作无超配（无超过 10 个并发连接），沿用既有 FIND-002 修复（parking_lot::Mutex 不 poisoning）。
   a. 验收条件：[100 并发 check_quota + 配额 10] → [最多 10 个通过，其余拒绝，无超配]；[Mutex panic] → [parking_lot::Mutex 不 poisoning（FIND-002 修复）]
   验证方法：`cargo test -p sz-orm-core --features tenant-quota-rls-enhanced,owasp-pentest-suite --test owasp_race_conditions`
3. **缓存击穿竞态渗透测试**（EARS: State-driven）
   当并发查询缓存未命中时，系统应当构造缓存击穿竞态攻击向量：100 并发查询同一 key + 缓存未命中，断言 `CacheWarmupProtection`（`packages/sz-orm-core/src/cache_warmup_protection.rs`）BloomFilter + 单飞（singleflight）防止全部打到 DB，沿用既有 FIND-002 修复（parking_lot::Mutex 不 poisoning）。
   a. 验收条件：[100 并发查询未命中 key + BloomFilter + singleflight] → [仅 1 个查询打到 DB，其余等待/拒绝，缓存击穿被阻止]
   验证方法：`cargo test -p sz-orm-core --features cache-warmup-protection,owasp-pentest-suite --test owasp_race_conditions`
4. **分布式事务竞态渗透测试**（EARS: State-driven）
   当并发提交/回滚分布式事务时，系统应当构造分布式事务竞态攻击向量：并发 `DtxManager::commit`（`packages/sz-orm-dtx/src/lib.rs:432`）+ `DtxManager::rollback` 同一事务，断言事务状态机拒绝非法转换（已 Committed 不可 Rollback / 已 Rolledback 不可 Commit）。
   a. 验收条件：[并发 commit + rollback 同一事务] → [仅一个成功，另一个拒绝"invalid state transition"，事务状态机一致]
   验证方法：`cargo test -p sz-orm-dtx --features owasp-pentest-suite --test owasp_race_conditions`
5. **TOCTOU 渗透测试**（EARS: State-driven）
   当检查与使用之间存在时间窗口时，系统应当构造 TOCTOU 攻击向量：检查余额（`balance >= amount`）后扣款前余额被另一线程改变，断言原子 `compare_exchange` 或事务保护（沿用 A04 TOCTOU 渗透测试，此处深化并发场景）。
   a. 验收条件：[线程 A 检查 balance=100 >= amount=100 + 线程 B 扣减 balance=100 → balance=0 + 线程 A 扣减] → [线程 A 扣减失败（balance=0 < amount=100），TOCTOU 被原子操作阻止]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_race_conditions`
6. **双重消费渗透测试**（EARS: State-driven）
   当并发消费同一资源时，系统应当构造双重消费攻击向量：100 并发使用同一优惠码/同一余额/同一库存，断言幂等键 + 原子操作防止双重消费（沿用 A04 幂等性渗透测试，此处深化并发场景）。
   a. 验收条件：[100 并发使用同一优惠码 + 幂等键] → [仅 1 次成功，其余拒绝"already consumed"，无双重消费]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_race_conditions`
7. **死锁检测渗透测试**（EARS: State-driven）
   当并发获取多个锁时，系统应当构造死锁攻击向量：线程 A 持有锁 1 等待锁 2 + 线程 B 持有锁 2 等待锁 1，断言锁获取顺序一致（lock ordering）或超时机制防止死锁。
   a. 验收条件：[线程 A 锁 1→2 + 线程 B 锁 2→1] → [锁顺序一致（全部 1→2）或超时释放，无死锁]
   验证方法：`cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_race_conditions`
8. **复用既有并发基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `Pool`（`packages/sz-orm-core/src/pool.rs`）/ `QuotaEnforcer`（`packages/sz-orm-core/src/tenant_quota_rls.rs`）/ `CacheWarmupProtection`（`packages/sz-orm-core/src/cache_warmup_protection.rs`）/ `DtxManager`（`packages/sz-orm-dtx/src/lib.rs:432`）/ `parking_lot::Mutex`（FIND-002 修复），不重复实现并发逻辑。
   a. 验收条件：[竞态渗透测试] → [复用既有 Pool/QuotaEnforcer/CacheWarmupProtection/DtxManager/parking_lot，不新建并发逻辑]
9. **禁止项**（EARS: Unwanted）
   如果竞态渗透测试影响默认编译或破坏既有并发，则系统应当通过 `owasp-pentest-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无竞态渗透测试，行为与 v4.8.0 一致]

### 5.14.2 交互流程

```plantuml
@startuml
title 业务逻辑并发竞态条件渗透测试 交互流程
actor "渗透测试" as test
participant "Pool\n(既有)" as pool
participant "QuotaEnforcer\n(既有)" as quota
participant "CacheWarmupProtection\n(既有)" as cache
participant "DtxManager\n(既有)" as dtx
participant "AtomicU64\n(原子操作)" as atomic

test -> pool : 100 并发获取 + 池大小 10
pool --> test : 最多 10 并发 (无死锁/重复/泄露)

test -> quota : 100 并发 check_quota + 配额 10
quota --> test : 最多 10 通过 (无超配)

test -> cache : 100 并发查询未命中 key
cache --> test : 仅 1 个打 DB (BloomFilter+singleflight)

test -> dtx : 并发 commit + rollback 同一事务
dtx --> test : 仅一个成功 (状态机一致)

test -> atomic : TOCTOU: 检查后扣减前余额改变
atomic --> test : compare_exchange 失败 (TOCTOU 阻止)

test -> atomic : 100 并发使用同一优惠码
atomic --> test : 仅 1 次成功 (无双重消费)

test -> test : 线程 A 锁 1→2 + 线程 B 锁 2→1
test --> test : 锁顺序一致/超时 (无死锁)
@enduml
```

### 5.14.3 异常场景

1. **连接池耗尽**
   a. 触发条件：并发获取连接超过池大小 + 等待超时
   b. 系统行为：返回 `PoolError::AcquireTimeout`
   c. 用户感知：错误"connection pool exhausted, timeout"
2. **配额超限**
   a. 触发条件：并发检查配额超过限制
   b. 系统行为：返回 `QuotaError::Exceeded`
   c. 用户感知：错误"quota exceeded"
3. **死锁**
   a. 触发条件：并发获取多个锁形成循环等待
   b. 系统行为：锁顺序一致或超时释放
   c. 用户感知：错误"deadlock detected" 或超时

---

# 6. 数据约束

## 6.1 渗透测试用例（PentestCase）

1. **owasp_id**：OWASP 编号，枚举值 A01~A10/XSS/CSRF/FILE_UPLOAD/RACE
2. **attack_vector**：攻击向量描述，非空字符串
3. **defense_assertion**：防御断言描述，非空字符串
4. **ears_format**：EARS 格式，枚举值 Ubiquitous/Event-driven/State-driven/Optional/Unwanted
5. **verification_method**：验证方法，枚举值 cargo_test/grep/cargo_audit/cargo_deny/script
6. **priority**：优先级，枚举值 P0/P1/P2（本版本全部 P1）
7. **feature_gate**：所属 feature gate，`owasp-pentest-suite`
8. **test_file**：测试文件路径，非空字符串（如 `packages/sz-orm-auth/tests/owasp_a01_access_control.rs`）

## 6.2 攻击载荷（AttackPayload）

1. **payload_type**：载荷类型，枚举值 SQL/NoSQL/OS_COMMAND/TEMPLATE/EXPRESSION/HEADER/JWT/OAUTH/CSRF/XSS/FILE/SSRF/RACE/LOGIC
2. **payload_value**：载荷值，字符串（如 `' UNION SELECT * FROM users--` / `<script>alert(1)</script>` / `http://127.0.0.1:8080`）
3. **expected_result**：预期结果，枚举值 Rejected/Accepted/Detected/Logged/Alerted
4. **actual_result**：实际结果，枚举值同 expected_result（测试运行后填充）
5. **pass**：是否通过，布尔值（actual_result == expected_result）

## 6.3 防御断言（DefenseAssertion）

1. **assertion_type**：断言类型，枚举值 Rejected/Accepted/Equal/NotEqual/Contains/NotContains
2. **assertion_target**：断言目标，非空字符串（如 `RbacAuthorizer::can()` / `JwtEncoder::decode()` / `QueryBuilder::build_select_with_params()`）
3. **assertion_value**：断言值，字符串（如 `false` / `Err(AuthError::InvalidToken)` / SQL 不含 `42` 字面量）
4. **evidence**：证据，`file:line` 格式（如 `packages/sz-orm-auth/tests/owasp_a01_access_control.rs:42`）

## 6.4 依赖审计记录（DependencyAuditRecord）

1. **advisory_id**：公告 ID，非空字符串（如 `RUSTSEC-2026-0049`）
2. **advisory_type**：公告类型，枚举值 Vulnerability/Unmaintained/Yanked/Notice
3. **affected_crate**：受影响 crate，非空字符串
4. **affected_version**：受影响版本，非空字符串
5. **ignore_reason**：忽略原因，非空字符串（如 `rumqttc 0.25 传递依赖，上游 0.103 为不兼容大版本`）
6. **feature_scope**：影响 feature 范围，字符串（如 `real-broker / real-es`）
7. **tracking**：跟踪状态，枚举值 PendingUpgrade/AutoRemove/WontFix

## 6.5 文件上传安全配置（FileUploadSecurityConfig）

1. **allowed_types**：允许的文件类型白名单，列表（如 `["jpg", "png", "pdf", "txt"]`）
2. **max_size**：最大文件大小，u64 字节（默认 100MB）
3. **magic_bytes_check**：是否启用 Magic bytes 验证，布尔值（默认 true）
4. **path_traversal_check**：是否启用路径遍历检查，布尔值（默认 true）
5. **null_byte_check**：是否启用 Null byte 检查，布尔值（默认 true）
6. **temp_file_cleanup**：是否启用临时文件清理，布尔值（默认 true）

---

# 7. 需求追溯矩阵

| 需求 ID | 优先级 | 需求名称 | 验收条件数 | feature gate | 复用既有代码 |
|---------|--------|---------|-----------|-------------|-------------|
| REQ-V49-001 | P1 | A01 失效的访问控制深化渗透测试 | 8 | `owasp-pentest-suite`（新增） | `RbacAuthorizer` `packages/sz-orm-auth/src/authorizer.rs` / `JwtAuthenticator` `packages/sz-orm-auth/src/lib.rs` / `TenantContext` `packages/sz-orm-core/src/tenant_context.rs` / `QueryBuilder.with_tenant_id` `packages/sz-orm-core/src/query.rs` + 既有 `packages/sz-orm-auth/tests/{security_attacks,blackhat_poc}.rs` + `packages/sz-orm-core/tests/security_attacks.rs` |
| REQ-V49-002 | P1 | A02 加密失败深化渗透测试 | 8 | `owasp-pentest-suite`（新增） | `AesGcmCrypter`/`Pbkdf2Hasher`/`HmacSigner`/`sha256_hex`/`hmac_sha256` `packages/sz-orm-crypto/src/lib.rs` / `prod_ready.rs` `packages/sz-orm-config/src/prod_ready.rs` + 既有 `packages/sz-orm-crypto/tests/{kat,blackhat_poc}.rs` |
| REQ-V49-003 | P1 | A03 注入深化渗透测试 | 8 | `owasp-pentest-suite`（新增） | `QueryBuilder` `packages/sz-orm-core/src/query.rs` / `OpenApiInjectionGuard` `packages/sz-orm-swagger/src/reverse/mod.rs:27` / `ModelDefinition::new` 表名验证（FIND-001 修复）`packages/sz-orm-lc/src/lib.rs` / `FormGenerator`/`CrudTemplateEngine` `packages/sz-orm-lc/src/lib.rs` + `scripts/check-sql-injection.ps1` |
| REQ-V49-004 | P1 | A04 不安全设计渗透测试 | 9 | `owasp-pentest-suite`（新增） | `WasmDbRateLimiter` `packages/sz-orm-wasm/src/real_db/mod.rs:26` / `RetryPolicy` `packages/sz-orm-grpc/src/lib.rs:415` / `CrossLangCompensationSerializer` `packages/sz-orm-dtx/src/cross_lang/serializer.rs` / `Pool` `packages/sz-orm-core/src/pool.rs` / `QuotaEnforcer` `packages/sz-orm-core/src/tenant_quota_rls.rs` / `DtxManager` `packages/sz-orm-dtx/src/lib.rs:432` |
| REQ-V49-005 | P1 | A05 安全配置错误深化渗透测试 | 8 | `owasp-pentest-suite`（新增） | `prod_ready.rs` `packages/sz-orm-config/src/prod_ready.rs` / `deny.toml` |
| REQ-V49-006 | P1 | A06 易受攻击和过时的组件深化渗透测试 | 8 | `owasp-pentest-suite`（新增） | `deny.toml` + `cargo audit` + `cargo deny check` + `cargo cyclonedx` |
| REQ-V49-007 | P1 | A07 软件和数据完整性失败渗透测试 | 8 | `owasp-pentest-suite`（新增） | `HashChainAuditor` `packages/sz-orm-audit/src/lib.rs` / 23 道门禁 `AGENTS.md` / `cargo deny check sources` |
| REQ-V49-008 | P1 | A08 安全日志和监控失败深化渗透测试 | 8 | `owasp-pentest-suite`（新增） | `SqlAuditor`/`mask_sensitive`/`HashChainAuditor` `packages/sz-orm-audit/src/lib.rs` / `DataMasker` `packages/sz-orm-masking/src/lib.rs` |
| REQ-V49-009 | P1 | A09 身份识别和认证失败深化渗透测试 | 10 | `owasp-pentest-suite`（新增） | `JwtAuthenticator`/`OAuth2Server`/`MfaManager`/`TotpVerifier` `packages/sz-orm-auth/src/` + 既有 `packages/sz-orm-auth/tests/{security_attacks,blackhat_poc}.rs` |
| REQ-V49-010 | P1 | A10 SSRF 深化渗透测试 | 7 | `owasp-pentest-suite`（新增） | `WasmRealDbConnection::new` URL 验证（FIND-004 修复）`packages/sz-orm-wasm/src/real_db/connection.rs:33` |
| REQ-V49-011 | P1 | XSS 跨站脚本攻击渗透测试 | 7 | `owasp-pentest-suite`（新增） | `FormGenerator`/`CrudTemplateEngine`/`FieldTypeMapping::sql_to_html_input` `packages/sz-orm-lc/src/lib.rs` |
| REQ-V49-012 | P1 | CSRF 跨站请求伪造渗透测试 | 7 | `owasp-pentest-suite`（新增） | `OAuth2Server` `packages/sz-orm-auth/src/oauth2.rs` / `JwtAuthenticator` `packages/sz-orm-auth/src/lib.rs` |
| REQ-V49-013 | P1 | 文件上传安全渗透测试 | 9 | `owasp-pentest-suite`（新增） | `StorageBackend` trait `packages/sz-orm-storage/src/storage.rs:15` / `SandboxedFs` `packages/sz-orm-wasm/src/advanced.rs` |
| REQ-V49-014 | P1 | 业务逻辑并发竞态条件渗透测试 | 9 | `owasp-pentest-suite`（新增） | `Pool` `packages/sz-orm-core/src/pool.rs` / `QuotaEnforcer` `packages/sz-orm-core/src/tenant_quota_rls.rs` / `CacheWarmupProtection` `packages/sz-orm-core/src/cache_warmup_protection.rs` / `DtxManager` `packages/sz-orm-dtx/src/lib.rs:432` / `parking_lot::Mutex`（FIND-002 修复） |

---

# 8. 验收标准总览

## 8.1 REQ-V49-001 A01 失效的访问控制深化渗透测试（P1）

1. 垂直越权渗透测试（普通用户调用管理员功能 → `az.can()` 返回 false）
2. 水平越权渗透测试（用户 A 访问用户 B 资源 → Schema 隔离 `tenant_1_` 前缀）
3. IDOR 渗透测试（修改 `?id=2` → 附加 `user_id=1` 条件，返回空）
4. 强制浏览渗透测试（直接访问受保护资源 → 拒绝）
5. JWT claims 深度验证（篡改 roles/iss/aud/sub/nbf → 签名校验失败）
6. RBAC 通配符权限深化（`grant("operator", "read")` 不授予资源 / `*` 授予所有 / `read:*` 授予所有 read）
7. 复用既有访问控制基础设施，不重复实现
8. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.2 REQ-V49-002 A02 加密失败深化渗透测试（P1）

1. 明文传输检测（`mysql://` → 拒绝，要求 `mysqls://`）
2. 弱算法检测（grep MD5/DES/RC4/ECB → 无生产代码使用）
3. 硬编码密钥扫描（grep src/ → 无硬编码，全部在 tests/）
4. ECB 模式检测（加密两次 → 密文不同，GCM 非 ECB）
5. 不安全随机数检测（grep thread_rng/DefaultHasher → 安全敏感值使用 OsRng）
6. 密钥长度验证（AES-256 32 字节 / PBKDF2 ≥ 100_000，M-8 修复）
7. 复用既有密码学基础设施，不重复实现
8. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.3 REQ-V49-003 A03 注入深化渗透测试（P1）

1. NoSQL 注入（`$ne`/`$gt` 操作符 → 参数化绑定，作为字面量）
2. OS 命令注入（grep `Command::new(user_input)` → 无，FIND-003 修复）
3. 模板注入（`{{7*7}}` → HTML 转义，不执行模板）
4. 表达式注入（`${7*7}` → `OpenApiInjectionGuard` 拒绝）
5. Header 注入（CRLF → 过滤或拒绝）
6. SQL 注入深化（UNION/堆叠/盲注/二阶 → 参数化 + FIND-001 表名验证）
7. 复用既有注入防护基础设施，不重复实现
8. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.4 REQ-V49-004 A04 不安全设计渗透测试（P1）

1. 业务逻辑缺陷（负数数量/跳过支付/重复优惠码 → 校验拒绝）
2. 缺失限流（1000 次登录 + 限流 100/min → RateLimited）
3. 缺失重试上限（max_retries=3 + 失败 10 次 → 第 4 次停止）
4. 缺失幂等性（idempotency_key 重复 → 返回第 1 次结果）
5. 缺失资源释放（异常路径 → Drop 自动释放，FIND-002 parking_lot 不 poisoning）
6. 竞态条件（100 并发扣减 → 原子操作保护，无负余额）
7. TOCTOU（检查后扣减前余额改变 → compare_exchange 失败）
8. 复用既有设计约束基础设施，不重复实现
9. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.5 REQ-V49-005 A05 安全配置错误深化渗透测试（P1）

1. 默认配置（password="admin" → 拒绝 / CORS `*` → 拒绝）
2. 调试模式（`--release` → 不启用调试路径）
3. 错误消息泄露（SQL 错误 → 用户消息"query failed"，不泄露 SQL）
4. 默认密码（admin/root/test123 → 拒绝）
5. 不必要功能启用（real-es 不使用 → 警告/拒绝）
6. 目录列举（/static/ 无 index.html → 403）
7. 复用既有配置校验基础设施，不重复实现
8. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.6 REQ-V49-006 A06 易受攻击和过时的组件深化渗透测试（P1）

1. CVE 漏洞追踪（`cargo audit` → 无未忽略公告，11 项忽略带 reason）
2. 许可证合规深化（`cargo deny check licenses` → 无 copyleft）
3. Yanked Crate 检测（`cargo deny check` → 无 yanked）
4. 重复依赖检测（`cargo deny check bans` → 无重复）
5. SBOM 生成（`cargo cyclonedx` → sbom.json 含全依赖）
6. 依赖来源限制（`cargo deny check sources` → 全部 crates.io）
7. 复用既有依赖审计基础设施，不重复实现
8. 脚本隔离（`scripts/owasp_a06_vulnerable_components.ps1`），不修改既有 `deny.toml`

## 8.7 REQ-V49-007 A07 软件和数据完整性失败渗透测试（P1）

1. CI/CD 管道完整性（23 道门禁全部通过）
2. 签名验证（哈希链篡改 → `verify()` 失败）
3. 反序列化完整性（`__proto__` → 普通结构，无原型污染）
4. 构建可重现性（`cargo build --release` 两次 → 产物哈希相同）
5. 依赖完整性（`cargo deny check sources` → 全部 crates.io）
6. 审计日志哈希链深化（删除/逆序/重放 → `verify()` 失败）
7. 复用既有完整性基础设施，不重复实现
8. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.8 REQ-V49-008 A08 安全日志和监控失败深化渗透测试（P1）

1. 日志注入（换行符 → 转义，无伪造条目）
2. 日志脱敏深化（PASSWORD/TOKEN/credit_card → `******`，大小写不敏感）
3. 数据脱敏深化（手机号/邮箱/身份证/银行卡 → 格式保持脱敏）
4. 告警（5 次登录失败 → 审计日志 + 告警）
5. 审计完整性（追加写入 → 哈希链延伸；修改/删除 → `verify()` 失败）
6. 缺失监控检测（关键操作 → 全部有审计日志，无盲区）
7. 复用既有日志监控基础设施，不重复实现
8. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.9 REQ-V49-009 A09 身份识别和认证失败深化渗透测试（P1）

1. 会话固定（预设 session_id → 登录后签发新 token）
2. 会话超时（exp=now+3600 + 等待 3601 秒 → 拒绝）
3. 并发会话（10 次 + 限制 5 → 第 6 次拒绝/踢出）
4. 凭证填充（1000 对 + 限流 + 锁定 + 告警 → 阻止）
5. 弱密码（"123456"/长度<8/无多样性 → 拒绝）
6. 账户枚举（不存在用户 → "invalid credentials"，不区分）
7. MFA 绕过（跳过/重放/暴力 → 强制 MFA + 限流 + 时间窗口，M-10 修复）
8. OAuth2 深化（redirect_uri/state/PKCE → 验证）
9. 复用既有认证基础设施，不重复实现
10. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.10 REQ-V49-010 A10 SSRF 深化渗透测试（P1）

1. 内网探测（`http://127.0.0.1:8080` → 拒绝）
2. 协议白名单（`file:///etc/passwd` → 拒绝，仅 http/https）
3. DNS Rebinding（首次 1.2.3.4 + 二次 127.0.0.1 → IP 锁定/二次校验）
4. 元数据端点（`169.254.169.254` → 拒绝）
5. SSRF 防御深化（IPv6/十进制 IP/八进制 IP → 拒绝）
6. 复用既有 FIND-004 修复 URL 验证，不重复实现
7. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.11 REQ-V49-011 XSS 跨站脚本攻击渗透测试（P1）

1. HTML 表单转义（`<script>` → `&lt;script&gt;`）
2. 反射型 XSS（URL 参数反射 → 转义）
3. 存储型 XSS（存入 DB + 读取渲染 → 转义）
4. DOM 型 XSS（innerHTML → textContent/转义）
5. HTML input 类型安全（`sql_to_html_input` → 安全类型 + value 转义）
6. 复用既有低代码基础设施，不重复实现
7. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.12 REQ-V49-012 CSRF 跨站请求伪造渗透测试（P1）

1. CSRF token（无 token/错误/过期 → 拒绝）
2. SameSite Cookie（无 SameSite → 标记发现 / SameSite=None → 拒绝）
3. Origin 验证（`https://evil.com` → 拒绝）
4. OAuth2 state 参数 CSRF 防御（缺失/不匹配/重放 → 拒绝）
5. 登录 CSRF（攻击者凭证 → 签发新 session_id）
6. 复用既有 OAuth2 基础设施，不重复实现
7. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.13 REQ-V49-013 文件上传安全渗透测试（P1）

1. 文件类型验证（`.php`/双扩展名/大小写 → 拒绝）
2. 文件大小限制（10GB + 100MB 限制 → 拒绝 / 0 字节 → 拒绝）
3. 内容验证（`.jpg` 内容为 PHP → Magic bytes 不匹配，拒绝）
4. 路径遍历（`../../../etc/passwd` → 净化/拒绝）
5. Magic bytes 验证（`.jpg` + `FF D8 FF` → 通过 / `.jpg` + ZIP → 拒绝）
6. 文件名净化（`evil.jpg\0.php` → Null byte 防御）
7. 临时文件清理（上传完成/失败 → 临时文件已删除）
8. 复用既有存储基础设施，不重复实现
9. `owasp-pentest-suite` feature gate 隔离，默认关闭

## 8.14 REQ-V49-014 业务逻辑并发竞态条件渗透测试（P1）

1. 连接池竞态（100 并发 + 池 10 → 最多 10 并发，无死锁/重复/泄露）
2. 租户配额竞态（100 并发 + 配额 10 → 最多 10 通过，无超配，FIND-002 修复）
3. 缓存击穿竞态（100 并发未命中 → 仅 1 个打 DB，BloomFilter + singleflight）
4. 分布式事务竞态（并发 commit + rollback → 仅一个成功，状态机一致）
5. TOCTOU（检查后扣减前余额改变 → compare_exchange 失败）
6. 双重消费（100 并发同一优惠码 → 仅 1 次成功，无双重消费）
7. 死锁检测（锁 1→2 + 锁 2→1 → 锁顺序一致/超时，无死锁）
8. 复用既有并发基础设施，不重复实现
9. `owasp-pentest-suite` feature gate 隔离，默认关闭

---

# 9. feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 | 状态 |
|-------------|--------|---------|------|---------|------|
| `owasp-pentest-suite` | sz-orm-core（新增，聚合） | OWASP Top 10 完整覆盖渗透测试套件（A01~A10 深化 + XSS + CSRF + 文件上传安全 + 业务逻辑并发竞态，14 项需求） | 关闭 | REQ-V49-001 ~ REQ-V49-014 | 新增 feature，聚合所有渗透测试 |

> 说明：`owasp-pentest-suite` 为聚合 feature gate，控制所有 14 项渗透测试。各渗透测试文件（`tests/owasp_*.rs`）通过 `#[cfg(feature = "owasp-pentest-suite")]` 隔离，默认不编译。部分渗透测试依赖既有 feature（如 A01 依赖 `multi-tenant-enhanced` / A03 依赖 `openapi-reverse` / A04 依赖 `cross-lang-dtx` / A10 依赖 `wasm-real-db` / 竞态依赖 `tenant-quota-rls-enhanced` + `cache-warmup-protection`），运行时须同时启用对应 feature。

---

# 10. 与 v4.8.0 的关系

## 10.1 零重叠声明

v4.9.0 与 v4.8.0 零重叠：

| v4.8.0 能力（跨语言互操作 + 全栈闭环层） | v4.9.0 能力（OWASP Top 10 完整覆盖渗透测试层） | 关系 |
|-------------------------------|-------------------------|------|
| 跨语言分布式事务协调（`sz-orm-dtx` cross-lang-dtx） | A04 不安全设计渗透测试（复用 `CrossLangCompensationSerializer` 幂等性）+ 竞态渗透测试（复用 `DtxManager`） | 零重叠，v4.9.0 仅复用不修改 |
| 低代码双向同步（`sz-orm-lc` lc-bidirectional-sync） | XSS 渗透测试（复用 `FormGenerator`/`FieldTypeMapping`）+ A03 模板注入（复用 `CrudTemplateEngine`） | 零重叠，v4.9.0 仅复用不修改 |
| OpenAPI 反向生成（`sz-orm-swagger` openapi-reverse） | A03 表达式注入（复用 `OpenApiInjectionGuard`） | 零重叠，v4.9.0 仅复用不修改 |
| WASM 真实数据库连接闭环（`sz-orm-wasm` wasm-real-db） | A10 SSRF 深化（复用 FIND-004 修复 URL 验证）+ A04 缺失限流（复用 `WasmDbRateLimiter`） | 零重叠，v4.9.0 仅复用不修改 |

## 10.2 依赖关系

```
v4.8.0 已验收基线（11 个 feature gate + 测试基线，已发布 crates.io 4.8.0）
  │
  ├─ 2026-08-14 安全审计 4 个 MEDIUM 已修复（FIND-001 SQL 注入 / FIND-002 Mutex poisoning / FIND-003 命令行密码 / FIND-004 SSRF）
  │
  └─ owasp-pentest-suite（新增聚合 feature）──→ REQ-V49-001 ~ REQ-V49-014 OWASP Top 10 完整覆盖渗透测试
       ├─ REQ-V49-001 A01 访问控制深化（复用 RbacAuthorizer/JwtAuthenticator/TenantContext）
       ├─ REQ-V49-002 A02 加密失败深化（复用 AesGcmCrypter/Pbkdf2Hasher/HmacSigner/prod_ready）
       ├─ REQ-V49-003 A03 注入深化（复用 QueryBuilder/OpenApiInjectionGuard/ModelDefinition::new/FormGenerator）
       ├─ REQ-V49-004 A04 不安全设计（复用 WasmDbRateLimiter/RetryPolicy/CrossLangCompensationSerializer/Pool/QuotaEnforcer/DtxManager）
       ├─ REQ-V49-005 A05 安全配置错误深化（复用 prod_ready.rs/deny.toml）
       ├─ REQ-V49-006 A06 过时组件深化（复用 deny.toml/cargo audit/cargo deny/cargo cyclonedx）
       ├─ REQ-V49-007 A07 完整性失败（复用 HashChainAuditor/23 道门禁/cargo deny sources）
       ├─ REQ-V49-008 A08 日志监控失败深化（复用 SqlAuditor/mask_sensitive/HashChainAuditor/DataMasker）
       ├─ REQ-V49-009 A09 认证失败深化（复用 JwtAuthenticator/OAuth2Server/MfaManager/TotpVerifier）
       ├─ REQ-V49-010 A10 SSRF 深化（复用 FIND-004 修复 WasmRealDbConnection::new URL 验证）
       ├─ REQ-V49-011 XSS（复用 FormGenerator/CrudTemplateEngine/FieldTypeMapping）
       ├─ REQ-V49-012 CSRF（复用 OAuth2Server/JwtAuthenticator）
       ├─ REQ-V49-013 文件上传安全（复用 StorageBackend/SandboxedFs）
       └─ REQ-V49-014 业务逻辑并发竞态（复用 Pool/QuotaEnforcer/CacheWarmupProtection/DtxManager/parking_lot）

v4.9.0 14 项需求相互独立，可并行开发：
  ├─ 所有需求仅新增测试代码（tests/owasp_*.rs）或脚本（scripts/owasp_*.ps1），不修改既有生产运行时逻辑
  ├─ 所有需求通过 owasp-pentest-suite feature gate 隔离，默认关闭
  └─ 所有需求复用既有安全测试基础 + 既有基础设施，不重复实现
```

## 10.3 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-auth` | REQ-V49-001 / 009 / 012 | A01 访问控制深化 + A09 认证失败深化 + CSRF 渗透测试（新增 `tests/owasp_a01_access_control.rs` / `tests/owasp_a09_auth_failures.rs` / `tests/owasp_csrf.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-crypto` | REQ-V49-002 | A02 加密失败深化渗透测试（新增 `tests/owasp_a02_crypto_failures.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-core` | REQ-V49-003 / 004 / 005 / 014 | A03 注入深化 + A04 不安全设计 + A05 安全配置错误深化 + 业务逻辑并发竞态（新增 `tests/owasp_a03_injection.rs` / `tests/owasp_a04_insecure_design.rs` / `tests/owasp_a05_misconfig.rs` / `tests/owasp_race_conditions.rs`，`owasp-pentest-suite` feature 聚合） |
| `sz-orm-config` | REQ-V49-002 / 005 | A02 明文传输检测 + A05 安全配置错误深化（新增 `tests/owasp_a02_crypto_failures.rs` / `tests/owasp_a05_misconfig.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-swagger` | REQ-V49-003 | A03 表达式注入渗透测试（新增 `tests/owasp_a03_injection.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-lc` | REQ-V49-003 / 011 | A03 模板注入 + XSS 渗透测试（新增 `tests/owasp_a03_injection.rs` / `tests/owasp_xss.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-wasm` | REQ-V49-004 / 010 | A04 缺失限流 + A10 SSRF 深化（新增 `tests/owasp_a04_insecure_design.rs` / `tests/owasp_a10_ssrf.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-grpc` | REQ-V49-004 | A04 缺失重试上限（新增 `tests/owasp_a04_insecure_design.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-dtx` | REQ-V49-004 / 014 | A04 幂等性 + 竞态渗透测试（新增 `tests/owasp_a04_insecure_design.rs` / `tests/owasp_race_conditions.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-audit` | REQ-V49-007 / 008 | A07 完整性失败 + A08 日志监控失败深化（新增 `tests/owasp_a07_integrity.rs` / `tests/owasp_a08_logging_failures.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-masking` | REQ-V49-008 | A08 数据脱敏深化（新增 `tests/owasp_a08_logging_failures.rs`，`owasp-pentest-suite` feature） |
| `sz-orm-storage` | REQ-V49-013 | 文件上传安全渗透测试（新增 `tests/owasp_file_upload.rs`，`owasp-pentest-suite` feature） |
| `scripts/` | REQ-V49-006 | A06 过时组件深化（新增 `scripts/owasp_a06_vulnerable_components.ps1` + `.sh`） |

## 10.4 不新增包声明

v4.9.0 不新增 workspace 成员，workspace 保持 60 成员（`Cargo.toml:2`）。所有新能力落在既有 12 个包的测试目录扩展（sz-orm-auth / sz-orm-crypto / sz-orm-core / sz-orm-config / sz-orm-swagger / sz-orm-lc / sz-orm-wasm / sz-orm-grpc / sz-orm-dtx / sz-orm-audit / sz-orm-masking / sz-orm-storage）+ 1 个脚本（`scripts/owasp_a06_vulnerable_components.ps1`）。

## 10.5 OWASP Top 10 (2021) 完整覆盖矩阵

| OWASP 项 | v4.8.0 及之前已覆盖 | v4.9.0 补充覆盖 | 对应需求 |
|----------|-------------------|---------------|---------|
| A01 失效的访问控制 | RBAC action 越权（M-11）/ 跨租户越权 / JWT claims | 垂直越权 + 水平越权 + IDOR + 强制浏览 + JWT claims 深度 + RBAC 通配符深化 | REQ-V49-001 |
| A02 加密失败 | HMAC 参数走私（H-1）/ PBKDF2 弱迭代（M-8）/ AES-256-GCM / JWT / TOTP 空密钥（M-10） | 明文传输 + 弱算法 + 硬编码密钥 + ECB + 不安全随机数 + 密钥长度 | REQ-V49-002 |
| A03 注入 | SQL 注入（参数化 + FIND-001 表名验证）/ JWT 注入 | NoSQL + OS 命令 + 模板 + 表达式 + Header + SQL 深化（UNION/堆叠/盲注/二阶） | REQ-V49-003 |
| A04 不安全设计 | OAuth2 授权码可预测（C-1）/ JWT 令牌类型混淆（C-2） | 业务逻辑缺陷 + 缺失限流/重试/幂等/资源释放 + 竞态 + TOCTOU | REQ-V49-004 |
| A05 安全配置错误 | 生产配置校验 + 密码脱敏 | 默认配置 + 调试模式 + 错误消息泄露 + 默认密码 + 不必要功能 + 目录列举 | REQ-V49-005 |
| A06 易受攻击和过时的组件 | cargo audit + cargo deny（11 项忽略带 reason） | CVE 追踪深化 + 许可证深化 + yanked + 重复依赖 + SBOM + 来源限制 | REQ-V49-006 |
| A07 软件和数据完整性失败 | 审计日志哈希链（HashChainAuditor） | CI/CD 管道 + 签名验证 + 反序列化 + 构建可重现性 + 依赖完整性 + 哈希链深化 | REQ-V49-007 |
| A08 安全日志和监控失败 | 审计日志脱敏 + 数据脱敏（10 种规则） | 日志注入 + 脱敏深化 + 告警 + 审计完整性 + 缺失监控检测 | REQ-V49-008 |
| A09 身份识别和认证失败 | JWT 伪造/过期/篡改/弱密钥 + OAuth2 + MFA + TOTP | 会话固定 + 超时 + 并发 + 凭证填充 + 弱密码 + 账户枚举 + MFA 绕过 + OAuth2 深化 | REQ-V49-009 |
| A10 SSRF | WASM proxy_url 验证（FIND-004 已修复） | 内网探测 + 协议白名单 + DNS rebinding + 元数据端点 + IPv6/十进制/八进制 IP | REQ-V49-010 |
| XSS | — | HTML 表单转义 + 反射型/存储型/DOM 型 + input 类型安全 | REQ-V49-011 |
| CSRF | — | CSRF token + SameSite + Origin + OAuth2 state + 登录 CSRF | REQ-V49-012 |
| 文件上传安全 | — | 类型/大小/内容验证 + 路径遍历 + Magic bytes + 文件名净化 + 临时文件清理 | REQ-V49-013 |
| 业务逻辑并发竞态 | — | 连接池/租户配额/缓存击穿/分布式事务竞态 + TOCTOU + 双重消费 + 死锁 | REQ-V49-014 |