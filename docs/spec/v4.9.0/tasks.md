# sz-orm v4.9.0 编码任务文档

> 版本：v4.9.0（OWASP Top 10 完整覆盖渗透测试套件）
> 基线：v4.8.0（已发布 crates.io 4.8.0，2026-08-14 安全审计 4 个 MEDIUM 已修复）
> 日期：2026-08-15
> 文档定位：编码任务清单（How to execute），对应需求规格 `spec.md`（14 项 EARS 需求 REQ-V49-001~014）+ 技术设计 `design.md`
> 任务约束：Rust 2021 Edition / 工作空间 60 包不新增 / 版本 4.9.0 / Windows MSVC（RUST_MIN_STACK=134217728, CARGO_INCREMENTAL=0）/ 测试 `cargo test --workspace -j 2 --no-fail-fast` / 禁止 todo!/unimplemented!/unreachable! / unsafe 零容忍 / 禁止 crate 级 #![allow(dead_code)] / 无 Breaking Change / feature gate `owasp-pentest-suite` 隔离 / 不修改既有生产代码，新增 tests/owasp_*.rs 和 scripts/owasp_*.ps1 / 每个任务附验证方法（cargo test / grep）/ 严禁幻影交付（每项附端到端接线测试）/ 临时文件测试后必须删除并释放进程

---

## 里程碑总览

| 里程碑 | 名称 | 任务数 | 对应需求 | 验收标志 |
|--------|------|--------|---------|---------|
| M0 | 基础设施准备 | 3 | — | feature gate 声明完成 + 既有证据验证通过 |
| M1 | A01~A05 渗透测试 | 5 | REQ-V49-001 ~ REQ-V49-005 | 5 项 OWASP 攻击面渗透测试全部通过 |
| M2 | A06~A10 渗透测试 | 5 | REQ-V49-006 ~ REQ-V49-010 | 5 项 OWASP 攻击面渗透测试全部通过 |
| M3 | 附加攻击面渗透测试 | 4 | REQ-V49-011 ~ REQ-V49-014 | XSS/CSRF/文件上传/竞态渗透测试全部通过 |
| M4 | 集成验证与门禁 | 4 | 全部 | 全套聚合测试 + 23 道门禁 + 幻影交付检查通过 |
| M5 | 文档与发布准备 | 3 | — | 文档同步 + 审计报告 + 版本号 4.9.0 |

**任务总数**：24 个主任务，覆盖 14 项需求 + 6 项基础设施/集成/文档任务

---

## M0. 基础设施准备

> 目标：声明聚合 feature gate `owasp-pentest-suite`，验证既有基础设施 file:line 证据真实存在，为后续渗透测试提供编译门控基础
> 依赖：无
> 验收标准：`cargo check --workspace --all-targets` 通过 + 各包 `owasp-pentest-suite` feature 声明存在 + 既有证据 file:line 全部验证通过

### T0.1 声明聚合 feature gate `owasp-pentest-suite`

- [ ] 在 `packages/sz-orm-core/Cargo.toml` 的 `[features]` 段新增 `owasp-pentest-suite = []` 聚合 feature 声明（空数组，仅作测试编译门控，不引入新依赖）
- [ ] 在以下 11 个包的 `Cargo.toml` `[features]` 段新增 `owasp-pentest-suite = []` 空数组声明：`sz-orm-auth` / `sz-orm-crypto` / `sz-orm-config` / `sz-orm-swagger` / `sz-orm-lc` / `sz-orm-wasm` / `sz-orm-grpc` / `sz-orm-dtx` / `sz-orm-audit` / `sz-orm-masking` / `sz-orm-storage`
- [ ] 确保默认 feature 不包含 `owasp-pentest-suite`（默认关闭，既有行为不变）

**输入**：design.md §feature gate 总览 + §2.6.3 扩展方式
**输出**：12 个 Cargo.toml 新增 `owasp-pentest-suite = []` 声明
**验证方法**：
```bash
grep -rn "owasp-pentest-suite" packages/*/Cargo.toml
# 预期：12 个包均有声明
cargo check --workspace --all-targets
# 预期：通过（默认 feature 不含 owasp-pentest-suite，行为不变）
cargo check --workspace --all-targets --all-features
# 预期：通过（feature 全组合编译，含 owasp-pentest-suite）
```
**依赖**：无

### T0.2 验证既有基础设施 file:line 证据真实存在

- [ ] 逐一验证 design.md §五 列出的 50+ 项 file:line 证据（如 `RbacAuthorizer` struct at `packages/sz-orm-auth/src/authorizer.rs:28` / `JwtEncoder::encode` at `packages/sz-orm-auth/src/jwt.rs:132` / `QueryBuilder::build_select_with_params` at `packages/sz-orm-core/src/query.rs:2029` / `HashChainAuditor::verify` at `packages/sz-orm-audit/src/lib.rs:876` / `WasmRealDbConnection::new` at `packages/sz-orm-wasm/src/real_db/connection.rs:33` / `StorageBackend::put` at `packages/sz-orm-storage/src/storage.rs:15` 等）
- [ ] 验证既有安全测试基础存在：`packages/sz-orm-auth/tests/security_attacks.rs`（117 行）/ `packages/sz-orm-auth/tests/blackhat_poc.rs`（210 行）/ `packages/sz-orm-core/tests/security_attacks.rs`（122 行）/ `packages/sz-orm-crypto/tests/kat.rs`（121 行）/ `packages/sz-orm-crypto/tests/blackhat_poc.rs`（92 行）
- [ ] 验证既有脚本存在：`scripts/check-sql-injection.ps1` + `scripts/gate.ps1` + `deny.toml`

**输入**：design.md §五 证据验证清单
**输出**：证据验证报告（所有 file:line 真实存在）
**验证方法**：
```bash
# 使用 read 工具逐项验证文件存在 + 行号在范围内
# 示例：验证 RbacAuthorizer struct at authorizer.rs:28
# 使用 read 工具读取 packages/sz-orm-auth/src/authorizer.rs offset=28 limit=1
# 预期：第 28 行包含 "struct RbacAuthorizer" 或类似定义
```
**依赖**：无

### T0.3 验证 v4.8.0 测试基线不回退

- [ ] 运行 `cargo test --workspace -j 2 --no-fail-fast`（默认 feature，不含 owasp-pentest-suite），确认 v4.8.0 已验收测试基线全部通过
- [ ] 记录基线测试通过数作为后续比对的基准

**输入**：v4.8.0 已验收基线
**输出**：基线测试通过数记录
**验证方法**：
```bash
cargo test --workspace -j 2 --no-fail-fast 2>&1 | tail -20
# 预期：全部通过，无失败
```
**依赖**：T0.1

---

## M1. A01~A05 渗透测试（OWASP Top 10 前半部分）

> 目标：交付 A01 访问控制深化 / A02 加密失败深化 / A03 注入深化 / A04 不安全设计 / A05 安全配置错误深化 5 项渗透测试
> 依赖：M0 完成
> 验收标准：5 项渗透测试全部通过 + 复用既有基础设施 + 无 todo!/unimplemented! + 无 unsafe + 临时文件清理

### T1.1 实现 A01 失效的访问控制深化渗透测试（REQ-V49-001）

- [ ] 新增 `packages/sz-orm-auth/tests/owasp_a01_access_control.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `a01_vertical_privilege_escalation_rejected` 测试：构造普通用户角色 `User::new(1, "alice").with_roles(vec!["user".to_string()])`，调用 `az.can(&user, "delete", "any_resource")` / `az.can(&user, "admin", "panel")`，断言返回 `false`（拒绝垂直越权）；管理员角色断言返回 `true`
- [ ] 实现 `a01_horizontal_privilege_isolation` 测试：构造 `TenantContext::new(1, IsolationStrategy::SchemaIsolation)` + `QueryBuilder.with_tenant_id(1).table("orders")`，断言生成的 SQL 含 `tenant_1_` 前缀，不含 `tenant_2_`（水平越权被 Schema 隔离阻止）
- [ ] 实现 `a01_insecure_direct_object_reference_blocked` 测试：构造用户 A（user_id=1）查询 orders where id=2，断言查询附加 `user_id = $1` 参数化条件，返回结果仅含 user_id=1 的订单（IDOR 被阻止）
- [ ] 实现 `a01_forced_browsing_rejected` 测试：构造未授权用户直接访问受保护资源（如 `__sz_orm_migrations`），断言 `az.can()` 返回 `false` 或查询附加租户隔离条件
- [ ] 实现 `a01_jwt_claims_tampering_rejected` 测试：签发 `JwtClaims::new("user-1", exp).with_roles(vec!["user".to_string()])`，篡改 claims 为 `with_roles(vec!["admin".to_string()])` 但保留原签名，断言 `JwtEncoder::decode` 拒绝（签名校验失败）；深化新增 `iss`/`aud`/`sub`/`nbf` claims 篡改攻击向量
- [ ] 实现 `a01_rbac_wildcard_boundary` 测试：`az.grant("operator", "read")` + `can("read", "payments")` 断言 `false`（action 级不授予资源）；`az.grant("admin", "*")` + `can("delete", "any")` 断言 `true`（`*` 通配符授予所有）；`az.grant("operator", "read:*")` + `can("read", "posts")` 断言 `true`（`read:*` 授予所有 read）
- [ ] 复用既有 `RbacAuthorizer`（`packages/sz-orm-auth/src/authorizer.rs:28`）/ `JwtEncoder`（`packages/sz-orm-auth/src/jwt.rs:122`）/ `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs:80`）/ `QueryBuilder::with_tenant_id`（`packages/sz-orm-core/src/query.rs:526`），不新建访问控制逻辑
- [ ] 深化既有 `packages/sz-orm-auth/tests/security_attacks.rs:53` `attack_tampered_payload_rejected`（新增 iss/aud/sub/nbf 篡改向量，不重复既有断言）+ 既有 `packages/sz-orm-auth/tests/blackhat_poc.rs:188` M-11（新增 `*`/`read:*` 边界）

**输入**：spec.md §5.1 + design.md §1.1.1 A01 既有基础设施
**输出**：`packages/sz-orm-auth/tests/owasp_a01_access_control.rs`（6 个测试函数）
**验证方法**：
```bash
cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a01_access_control
# 预期：6 个测试全部通过
cargo test -p sz-orm-core --features multi-tenant-enhanced,owasp-pentest-suite --test owasp_a01_access_control
# 预期：水平越权测试通过（如水平越权测试在 core 包）
grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-auth/tests/owasp_a01_access_control.rs
# 预期：无输出
```
**依赖**：T0.1, T0.2

### T1.2 实现 A02 加密失败深化渗透测试（REQ-V49-002）

- [ ] 新增 `packages/sz-orm-crypto/tests/owasp_a02_crypto_failures.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 新增 `packages/sz-orm-config/tests/owasp_a02_crypto_failures.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `a02_cleartext_transport_rejected` 测试（config 包）：构造明文连接串 `mysql://root:pass@host/db` / `postgres://root:pass@host/db`，断言 `prod_ready.rs::validate` 拒绝，提示"use TLS connection mysqls://"；构造 TLS 连接串 `mysqls://` / `postgresqls://`，断言校验通过
- [ ] 实现 `a02_weak_algorithm_absent` 测试：grep 扫描 `packages/*/src/` 中 `Md5::` / `Des::` / `Rc4::` / `Ecb::`（排除 TOTP SHA-1 RFC 4226/6238 允许场景），断言无生产代码使用弱算法
- [ ] 实现 `a02_hardcoded_secret_absent` 测试：grep 扫描 `packages/*/src/` 中硬编码密钥字面量（排除 `tests/` 和文档注释），断言无生产代码硬编码密钥
- [ ] 实现 `a02_ecb_mode_not_used` 测试：使用 `AesGcmCrypter::new(key)` 加密相同明文两次，断言两次密文不同（GCM 随机 nonce，非 ECB 模式）
- [ ] 实现 `a02_insecure_random_absent` 测试：grep 扫描 `packages/*/src/` 中 `thread_rng()` / `DefaultHasher::new`（排除测试），断言安全敏感值使用 `OsRng`（沿用 C-1 修复 `packages/sz-orm-auth/tests/blackhat_poc.rs:23`）
- [ ] 实现 `a02_weak_key_length_rejected` 测试：构造 AES-256 密钥 16 字节，断言 `AesGcmCrypter::new` 拒绝；构造 PBKDF2 迭代 1000，断言 `Pbkdf2Hasher` 拒绝（沿用 M-8 修复 `packages/sz-orm-crypto/tests/blackhat_poc.rs:68`）
- [ ] 复用既有 `AesGcmCrypter`（`packages/sz-orm-crypto/src/lib.rs:89`）/ `Pbkdf2Hasher`（`:182`）/ `HmacSigner`（`:303`）/ `prod_ready.rs`（`packages/sz-orm-config/src/prod_ready.rs:101`）+ 既有 `kat.rs` / `blackhat_poc.rs`，不新建密码学逻辑

**输入**：spec.md §5.2 + design.md §1.1.1 A02 既有基础设施
**输出**：`packages/sz-orm-crypto/tests/owasp_a02_crypto_failures.rs`（5 个测试函数）+ `packages/sz-orm-config/tests/owasp_a02_crypto_failures.rs`（1 个测试函数）
**验证方法**：
```bash
cargo test -p sz-orm-crypto --features owasp-pentest-suite --test owasp_a02_crypto_failures
cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a02_crypto_failures
# 预期：全部通过
grep -rn "Md5::\|Des::\|Rc4::\|Ecb::" packages/*/src/
# 预期：无输出（或仅 TOTP SHA-1 允许场景）
grep -rn "thread_rng()\|DefaultHasher::new" packages/*/src/
# 预期：无输出或仅测试代码
```
**依赖**：T0.1, T0.2

### T1.3 实现 A03 注入深化渗透测试（REQ-V49-003）

- [ ] 新增 `packages/sz-orm-core/tests/owasp_a03_injection.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（NoSQL + OS 命令 + Header + SQL 深化）
- [ ] 新增 `packages/sz-orm-swagger/tests/owasp_a03_injection.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（表达式注入）
- [ ] 新增 `packages/sz-orm-lc/tests/owasp_a03_injection.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（模板注入 + 表名验证）
- [ ] 实现 `a03_nosql_operator_parameterized` 测试（core 包）：构造 `QueryBuilder::where_eq("field", Value::String("{\"$ne\": null}"))`，调用 `build_select_with_params()`，断言值参数化绑定（`$ne` 作为字面量字符串，不被解释为操作符）
- [ ] 实现 `a03_os_command_injection_absent` 测试（core 包）：grep 扫描 `packages/*/src/` 中 `Command::new(user_input)` / `Command::new("sh").arg("-c").arg(user_input)` 模式，断言无用户输入直接拼接命令（沿用 FIND-003 修复）
- [ ] 实现 `a03_template_injection_escaped` 测试（lc 包）：构造用户输入 `{{7*7}}` / `${7*7}` / `<%= 7*7 %>`，调用 `FormGenerator` / `CrudTemplateEngine` 生成 HTML，断言模板语法被 HTML 转义，不执行模板
- [ ] 实现 `a03_expression_injection_rejected` 测试（swagger 包）：构造 OpenAPI spec 含 `${7*7}` 表达式，调用 `OpenApiInjectionGuard::check`，断言返回 `ReverseGenError::InjectionDetected`
- [ ] 实现 `a03_header_injection_crlf_filtered` 测试（core 包）：构造 Location 头部值 `https://evil.com\r\nSet-Cookie: evil=1`，断言 CRLF 字符被过滤或拒绝，不产生额外头部
- [ ] 实现 `a03_sql_injection_union_parameterized` / `a03_sql_injection_stacked_rejected` / `a03_sql_injection_blind_parameterized` / `a03_sql_injection_second_order_blocked` 测试（core 包）：构造 UNION 注入 `' UNION SELECT * FROM users--` / 堆叠注入 `; DROP TABLE users--` / 盲注 `' AND 1=1--` vs `' AND 1=2--` / 二阶注入，断言 `QueryBuilder::build_select_with_params` 参数化绑定
- [ ] 实现 `a03_model_name_validation_finds_001` 测试（lc 包）：构造恶意表名 `users" DROP TABLE users; --`，调用 `ModelDefinition::validate_identifier`，断言拒绝（FIND-001 修复，仅允许字母/数字/下划线）
- [ ] 复用既有 `QueryBuilder`（`packages/sz-orm-core/src/query.rs:36`）/ `OpenApiInjectionGuard`（`packages/sz-orm-swagger/src/reverse/injection_guard.rs:25`）/ `ModelDefinition::validate_identifier`（`packages/sz-orm-lc/src/lib.rs:41`）/ `FormGenerator`（`packages/sz-orm-lc/src/lib.rs:678`）+ 既有 `scripts/check-sql-injection.ps1`，不新建注入防护逻辑

**输入**：spec.md §5.3 + design.md §1.1.1 A03 既有基础设施
**输出**：3 个测试文件（core 6 个测试 + swagger 1 个测试 + lc 2 个测试）
**验证方法**：
```bash
cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a03_injection
cargo test -p sz-orm-swagger --features openapi-reverse,owasp-pentest-suite --test owasp_a03_injection
cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_a03_injection
# 预期：全部通过
grep -rn "Command::new" packages/*/src/
# 预期：审查无 user_input 拼接
```
**依赖**：T0.1, T0.2

### T1.4 实现 A04 不安全设计渗透测试（REQ-V49-004）

- [ ] 新增 `packages/sz-orm-core/tests/owasp_a04_insecure_design.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（业务逻辑缺陷 + 资源释放 + 竞态 + TOCTOU）
- [ ] 新增 `packages/sz-orm-wasm/tests/owasp_a04_insecure_design.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（缺失限流）
- [ ] 新增 `packages/sz-orm-grpc/tests/owasp_a04_insecure_design.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（缺失重试上限）
- [ ] 新增 `packages/sz-orm-dtx/tests/owasp_a04_insecure_design.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（缺失幂等性）
- [ ] 实现 `a04_negative_quantity_rejected` / `a04_skip_payment_rejected` 测试（core 包）：构造负数数量 `quantity = -1` / 跳过支付直接确认，断言业务规则校验拒绝
- [ ] 实现 `a04_missing_rate_limiting_enforced` 测试（wasm 包）：构造 1000 次登录尝试 + 限流 100/min，调用 `WasmDbRateLimiter`，断言第 101 次拒绝（`WasmRealDbError::RateLimited`）
- [ ] 实现 `a04_missing_retry_limit_enforced` 测试（grpc 包）：构造 `RetryPolicy` max_retries=3 + 连续失败 10 次，断言第 4 次后停止重试；max_retries=0 + 失败，断言不重试直接返回错误
- [ ] 实现 `a04_missing_idempotency_enforced` 测试（dtx 包）：构造相同 idempotency_key="key-1" 重复提交 3 次，调用 `CrossLangCompensationSerializer`，断言第 2/3 次返回第 1 次结果，不重复执行副作用
- [ ] 实现 `a04_missing_resource_release_drop` 测试（core 包）：构造获取连接 + 异常 + 未显式释放，断言 `Drop` 自动释放连接回池；构造 Mutex lock + panic，断言 `parking_lot::Mutex` 不 poisoning（FIND-002 修复）
- [ ] 实现 `a04_race_condition_atomic_protected` 测试（core 包）：构造 100 并发扣减 balance=100 amount=1，使用 `AtomicU64::compare_exchange`，断言最终 balance=0，无负余额/双重消费
- [ ] 实现 `a04_toctou_compare_exchange_blocks` 测试（core 包）：构造线程 A 检查 balance=100 >= amount=100 + 线程 B 扣减 balance=100 → balance=0 + 线程 A 扣减，断言线程 A `compare_exchange` 失败（TOCTOU 被原子操作阻止）
- [ ] 复用既有 `WasmDbRateLimiter`（`packages/sz-orm-wasm/src/real_db/rate_limiter.rs:11`）/ `RetryPolicy`（`packages/sz-orm-grpc/src/lib.rs:415`）/ `CrossLangCompensationSerializer`（`packages/sz-orm-dtx/src/cross_lang/serializer.rs:23`）/ `Pool`（`packages/sz-orm-core/src/pool.rs:749`）/ `QuotaEnforcer`（`packages/sz-orm-core/src/tenant_quota_rls.rs:167`）/ `DtxManager`（`packages/sz-orm-dtx/src/lib.rs:432`），不新建设计约束逻辑

**输入**：spec.md §5.4 + design.md §1.1.1 A04 既有基础设施
**输出**：4 个测试文件（core 4 个测试 + wasm 1 个测试 + grpc 1 个测试 + dtx 1 个测试）
**验证方法**：
```bash
cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a04_insecure_design
cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a04_insecure_design
cargo test -p sz-orm-grpc --features owasp-pentest-suite --test owasp_a04_insecure_design
cargo test -p sz-orm-dtx --features cross-lang-dtx,owasp-pentest-suite --test owasp_a04_insecure_design
# 预期：全部通过
```
**依赖**：T0.1, T0.2

### T1.5 实现 A05 安全配置错误深化渗透测试（REQ-V49-005）

- [ ] 新增 `packages/sz-orm-config/tests/owasp_a05_misconfig.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 新增 `packages/sz-orm-core/tests/owasp_a05_misconfig.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（错误消息泄露部分）
- [ ] 实现 `a05_default_password_rejected` 测试（config 包）：构造配置 password="admin" / "root" / "test123"，调用 `prod_ready.rs::validate`，断言拒绝，提示"weak default password"
- [ ] 实现 `a05_debug_mode_not_in_release` 测试（config 包）：grep 扫描 `packages/*/src/` 中 `debug_assertions` / `RUST_LOG=debug`，断言生产构建（`--release`）不启用调试代码路径
- [ ] 实现 `a05_error_message_no_leak` 测试（core 包）：构造触发 SQL 错误场景，断言生产错误消息为"query failed"等用户友好消息，不泄露 SQL 语句/表名/列名；错误日志记录完整错误供调试但不返回给用户
- [ ] 实现 `a05_cors_wildcard_rejected` 测试（config 包）：构造 CORS `allow_origins="*"` + `allow_credentials=true`，断言 `prod_ready.rs::validate` 拒绝，提示"specify explicit origins"
- [ ] 实现 `a05_unnecessary_feature_warned` 测试（config 包）：构造生产构建启用 `real-es` 但不使用 ES 场景，断言警告或拒绝；运行 `cargo deny check` 验证所有 feature 组合安全公告检查
- [ ] 实现 `a05_directory_listing_disabled` 测试（config 包）：构造访问 `/static/` 无 index.html 场景，断言返回 403/404，不列出目录内容
- [ ] 复用既有 `prod_ready.rs`（`packages/sz-orm-config/src/prod_ready.rs:101`）/ `deny.toml`，不新建配置校验逻辑

**输入**：spec.md §5.5 + design.md §1.1.1 A05 既有基础设施
**输出**：2 个测试文件（config 5 个测试 + core 1 个测试）
**验证方法**：
```bash
cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a05_misconfig
cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a05_misconfig
# 预期：全部通过
grep -rn "debug_assertions\|RUST_LOG=debug" packages/*/src/
# 预期：审查（生产构建不启用调试路径）
```
**依赖**：T0.1, T0.2

---

## M2. A06~A10 渗透测试（OWASP Top 10 后半部分）

> 目标：交付 A06 过时组件深化 / A07 完整性失败 / A08 日志监控失败深化 / A09 认证失败深化 / A10 SSRF 深化 5 项渗透测试
> 依赖：M0 完成
> 验收标准：5 项渗透测试全部通过 + A06 脚本跨平台（PowerShell + Bash 等价）+ 复用既有基础设施

### T2.1 实现 A06 易受攻击和过时的组件深化渗透测试（REQ-V49-006）

- [ ] 新增 `scripts/owasp_a06_vulnerable_components.ps1`（PowerShell 脚本）
- [ ] 新增 `scripts/owasp_a06_vulnerable_components.sh`（Bash 等价脚本）
- [ ] 实现 `Invoke-CveAudit` 函数（ps1）/ `invoke_cve_audit` 函数（sh）：运行 `cargo audit`，断言无未忽略的 RUSTSEC 公告（沿用既有 `deny.toml:36` 忽略清单 11 项带 reason）
- [ ] 实现 `Invoke-LicenseCheck` 函数：运行 `cargo deny check licenses`，断言全部许可证在白名单（MIT/Apache-2.0/BSD/ISC/Zlib/MPL-2.0 等），无 copyleft（GPL/AGPL/LGPL）
- [ ] 实现 `Invoke-YankedCheck` 函数：运行 `cargo deny check`，断言无 yanked 依赖（或警告并记录）
- [ ] 实现 `Invoke-DuplicateCheck` 函数：运行 `cargo deny check bans`，断言无重复依赖（或警告并记录版本碎片化）
- [ ] 实现 `Invoke-SbomGeneration` 函数：运行 `cargo cyclonedx`，断言生成 sbom.json 含所有依赖 + version + license + source；若 `cargo cyclonedx` 未安装则跳过 SBOM 部分并警告（不阻塞 CVE/许可证/yanked/重复/来源检查）
- [ ] 实现 `Invoke-SourceCheck` 函数：运行 `cargo deny check sources`，断言全部依赖来自 crates.io（无未知 registry/git 来源）
- [ ] 确保脚本不修改既有 `deny.toml`（脚本隔离，仅读取 `deny.toml` 配置）
- [ ] 确保 PowerShell 与 Bash 脚本逻辑等价（跨平台）

**输入**：spec.md §5.6 + design.md §1.1.1 A06 既有基础设施
**输出**：`scripts/owasp_a06_vulnerable_components.ps1` + `scripts/owasp_a06_vulnerable_components.sh`（6 个函数）
**验证方法**：
```bash
cargo audit
# 预期：无未忽略公告
cargo deny check licenses
# 预期：无 copyleft
cargo deny check
# 预期：无 yanked
cargo deny check bans
# 预期：无重复依赖
cargo deny check sources
# 预期：全部来自 crates.io
pwsh scripts/owasp_a06_vulnerable_components.ps1
# 或 bash scripts/owasp_a06_vulnerable_components.sh
# 预期：脚本输出审计报告，全部断言通过
```
**依赖**：T0.1, T0.2

### T2.2 实现 A07 软件和数据完整性失败渗透测试（REQ-V49-007）

- [ ] 新增 `packages/sz-orm-audit/tests/owasp_a07_integrity.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `a07_cicd_pipeline_23_gates_pass` 测试：运行 `scripts/gate.ps1`（23 道门禁），断言全部通过（特别是门禁 11 上游仓库未修改 + 门禁 13 审计证据验证 + 门禁 15 幻影交付检查）
- [ ] 实现 `a07_hash_chain_tamper_detected` 测试：构造审计日志哈希链，篡改第 5 条日志，调用 `HashChainAuditor::verify`，断言返回失败（检测到篡改）
- [ ] 实现 `a07_deserialization_no_proto_pollution` 测试：构造恶意 JSON `{"__proto__": {"admin": true}}`，调用 `serde_json::from_str`，断言反序列化为普通结构，`__proto__` 不被特殊处理，无原型污染；构造超长字段，断言长度校验拒绝
- [ ] 实现 `a07_build_reproducibility` 测试：运行 `cargo build --release` 两次，比较产物哈希，断言哈希相同（可重现）或记录不可重现原因（嵌入时间戳/路径/随机）
- [ ] 实现 `a07_dependency_integrity_sources` 测试：运行 `cargo deny check sources`，断言全部依赖来自 crates.io，无未知来源
- [ ] 实现 `a07_hash_chain_delete_detected` / `a07_hash_chain_reorder_detected` / `a07_hash_chain_replay_detected` 测试：构造删除中间日志 / 逆序日志 / 重放日志攻击向量，调用 `HashChainAuditor::verify`，断言全部检测到篡改
- [ ] 复用既有 `HashChainAuditor`（`packages/sz-orm-audit/src/lib.rs:792`）/ `scripts/gate.ps1` / `cargo deny check sources`，不新建完整性逻辑

**输入**：spec.md §5.7 + design.md §1.1.1 A07 既有基础设施
**输出**：`packages/sz-orm-audit/tests/owasp_a07_integrity.rs`（7 个测试函数）
**验证方法**：
```bash
cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a07_integrity
# 预期：7 个测试全部通过
pwsh scripts/gate.ps1
# 预期：23 道门禁全部通过
```
**依赖**：T0.1, T0.2

### T2.3 实现 A08 安全日志和监控失败深化渗透测试（REQ-V49-008）

- [ ] 新增 `packages/sz-orm-audit/tests/owasp_a08_logging_failures.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 新增 `packages/sz-orm-masking/tests/owasp_a08_logging_failures.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `a08_log_injection_newline_escaped` 测试（audit 包）：构造用户输入 `"user\n[INFO] fake log"`，调用 `SqlAuditor::log`，断言换行符被转义为 `\\n`，不产生伪造日志条目
- [ ] 实现 `a08_mask_sensitive_password_token` 测试（audit 包）：构造 SQL `SELECT * FROM users WHERE PASSWORD='secret' AND Token='abc'`，调用 `mask_sensitive`，断言脱敏为 `******='******'`
- [ ] 实现 `a08_mask_sensitive_case_insensitive` 测试（audit 包）：构造 SQL 含 `Password` / `TOKEN` / `Credit_Card`（大小写混合），断言全部脱敏
- [ ] 实现 `a08_mask_sensitive_boundary_substring` 测试（audit 包）：构造 SQL 含 `passwordless`（子串边界），断言不脱敏（`passwordless` 是完整标识符，非敏感词）
- [ ] 实现 `a08_data_masker_phone_email_idcard` 测试（masking 包）：构造手机号 "13812345678" + Phone 规则，断言脱敏为 "138****5678"；身份证 "110101199001012345" + IdCard 规则，断言脱敏为 "1101**********2345"；API key 短于 8 字符，断言脱敏为 "***"
- [ ] 实现 `a08_alerting_brute_force` 测试（audit 包）：构造 5 次登录失败，断言审计日志记录 5 次失败 + 触发"brute force"告警
- [ ] 实现 `a08_audit_integrity_append_only` 测试（audit 包）：构造追加写入新日志（哈希链延伸成功）+ 修改历史日志（`verify()` 失败）+ 删除历史日志（`verify()` 失败）
- [ ] 实现 `a08_missing_monitoring_detected` 测试（audit 包）：构造关键操作列表（登录/越权/注入/限流/审计篡改），检查审计日志覆盖，断言无监控盲区；某操作无审计则标记为发现
- [ ] 复用既有 `SqlAuditor`（`packages/sz-orm-audit/src/lib.rs:54`）/ `mask_sensitive`（`:118`）/ `HashChainAuditor`（`:792`）/ `DataMasker`（`packages/sz-orm-masking/src/lib.rs:36`），不新建日志监控逻辑

**输入**：spec.md §5.8 + design.md §1.1.1 A08 既有基础设施
**输出**：2 个测试文件（audit 7 个测试 + masking 1 个测试）
**验证方法**：
```bash
cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a08_logging_failures
cargo test -p sz-orm-masking --features owasp-pentest-suite --test owasp_a08_logging_failures
# 预期：全部通过
```
**依赖**：T0.1, T0.2

### T2.4 实现 A09 身份识别和认证失败深化渗透测试（REQ-V49-009）

- [ ] 新增 `packages/sz-orm-auth/tests/owasp_a09_auth_failures.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `a09_session_fixation_new_token` 测试：构造攻击者预设 session_id="fixed-id"，用户登录后调用 `JwtAuthenticator` 签发新 token，断言新 session_id ≠ "fixed-id"（会话固定被阻止）
- [ ] 实现 `a09_session_timeout_expired_rejected` 测试：构造 token exp=now+3600，等待超时后调用 `JwtEncoder::decode`，断言拒绝（沿用既有 `packages/sz-orm-auth/tests/security_attacks.rs:41` `attack_expired_token_rejected`）
- [ ] 实现 `a09_concurrent_sessions_limited` 测试：构造同一账户并发登录 10 次 + 限制 5，断言第 6 次拒绝或踢出最早会话
- [ ] 实现 `a09_credential_stuffing_blocked` 测试：构造 1000 对泄露凭证字典批量尝试登录 + 限流 100/min + 锁定 5 次，断言限流拒绝 + 账户锁定 + 告警
- [ ] 实现 `a09_weak_password_rejected` 测试：构造密码 "123456" / "password" / "admin" / "qwerty"（常见弱密码）/ 长度 < 8 / 无字符多样性，断言密码复杂度校验拒绝
- [ ] 实现 `a09_account_enumeration_unified_response` 测试：构造登录不存在的用户 + 登录存在用户但密码错误，断言响应统一为"invalid credentials"（不区分用户不存在与密码错误）
- [ ] 实现 `a09_mfa_bypass_replay_brute_blocked` 测试：构造跳过 MFA 直接访问 / MFA 重放 / MFA 暴力（6 位 TOTP 100 万次枚举），调用 `MfaManager` / `TotpVerifier`，断言强制 MFA + 限流 + 时间窗口（沿用 M-10 修复 `packages/sz-orm-auth/tests/blackhat_poc.rs:150`）
- [ ] 实现 `a09_oauth2_redirect_uri_state_pkce` 测试：构造 redirect_uri="https://evil.com"（开放重定向）/ state 缺失/不匹配/重放 / PKCE 缺失，调用 `OAuth2Server`，断言强制验证（沿用 C-1 修复 `packages/sz-orm-auth/tests/blackhat_poc.rs:23`）
- [ ] 复用既有 `JwtAuthenticator`（`packages/sz-orm-auth/src/auth.rs:150`）/ `OAuth2Server`（`packages/sz-orm-auth/src/oauth2.rs:130`）/ `MfaManager`（`packages/sz-orm-auth/src/mfa.rs:180`）/ `TotpVerifier`（`packages/sz-orm-auth/src/mfa.rs:108`），不新建认证逻辑

**输入**：spec.md §5.9 + design.md §1.1.1 A09 既有基础设施
**输出**：`packages/sz-orm-auth/tests/owasp_a09_auth_failures.rs`（8 个测试函数）
**验证方法**：
```bash
cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures
# 预期：8 个测试全部通过
```
**依赖**：T0.1, T0.2

### T2.5 实现 A10 SSRF 深化渗透测试（REQ-V49-010）

- [ ] 新增 `packages/sz-orm-wasm/tests/owasp_a10_ssrf.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `a10_internal_network_probing_rejected` 测试：构造 `WasmRealDbConnection::new("http://127.0.0.1:8080", ...)` / `http://localhost:6379` / `http://192.168.1.1:22`，断言拒绝内网地址，提示"internal address not allowed"
- [ ] 实现 `a10_protocol_whitelist_enforced` 测试：构造 `WasmRealDbConnection::new("file:///etc/passwd", ...)` / `gopher://...` / `dict://...` / `ftp://...`，断言拒绝，提示"only http/https allowed"
- [ ] 实现 `a10_dns_rebinding_blocked` 测试：构造 DNS rebinding 场景（首次解析 1.2.3.4 通过校验，二次解析 127.0.0.1 完成 SSRF），断言防御：IP 锁定（pin IP）或二次解析校验拒绝
- [ ] 实现 `a10_metadata_endpoint_rejected` 测试：构造 `http://169.254.169.254/latest/meta-data/iam/security-credentials/`（AWS）/ `http://metadata.google.internal/computeMetadata/v1/`（GCP）/ `http://169.254.169.254/metadata/instance`（Azure），断言拒绝云元数据端点
- [ ] 实现 `a10_ipv6_internal_rejected` 测试：构造 `http://[::1]:8080` / `http://[fe80::1]:8080`（IPv6 内网），断言拒绝
- [ ] 实现 `a10_decimal_ip_rejected` 测试：构造 `http://2130706433/`（十进制 IP = 127.0.0.1），断言拒绝
- [ ] 实现 `a10_octal_ip_rejected` 测试：构造 `http://0177.0.0.1/`（八进制 IP = 127.0.0.1），断言拒绝
- [ ] 复用既有 FIND-004 修复 `WasmRealDbConnection::new`（`packages/sz-orm-wasm/src/real_db/connection.rs:33` URL 验证），不新建 SSRF 防御逻辑
- [ ] **设计说明**：若 FIND-004 修复未在 `new` 中生效（当前代码 `:33` 未验证 URL），渗透测试会标记为发现，记录在审计报告（符合 spec.md §1.4.8"不负责修复新发现的漏洞"），不阻塞渗透测试交付

**输入**：spec.md §5.10 + design.md §1.1.1 A10 既有基础设施
**输出**：`packages/sz-orm-wasm/tests/owasp_a10_ssrf.rs`（7 个测试函数）
**验证方法**：
```bash
cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a10_ssrf
# 预期：7 个测试全部通过（或部分标记为发现，记录审计报告）
```
**依赖**：T0.1, T0.2

---

## M3. 附加攻击面渗透测试（XSS + CSRF + 文件上传 + 竞态）

> 目标：交付 XSS / CSRF / 文件上传安全 / 业务逻辑并发竞态 4 项渗透测试
> 依赖：M0 完成
> 验收标准：4 项渗透测试全部通过 + 临时文件清理 + 并发测试确定性（无 flaky）

### T3.1 实现 XSS 跨站脚本攻击渗透测试（REQ-V49-011）

- [ ] 新增 `packages/sz-orm-lc/tests/owasp_xss.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `xss_html_form_escaping` 测试：构造用户输入 `<script>alert('xss')</script>` / `<img onerror=alert(1)>` / `<svg onload=alert(1)>`，调用 `FormGenerator` 生成 HTML，断言 HTML 转义（`<` → `&lt;` / `>` → `&gt;` / `"` → `&quot;` / `'` → `&#x27;` / `&` → `&amp;`）
- [ ] 实现 `xss_reflected_escaped` 测试：构造 URL 参数 `?name=<script>alert(1)</script>` 反射到页面，断言反射前 HTML 转义
- [ ] 实现 `xss_stored_escaped_on_render` 测试：构造输入 `<script>alert(1)</script>` 存入 DB + 读取渲染，断言渲染时 HTML 转义（存储原值，渲染转义）
- [ ] 实现 `xss_dom_safe_api` 测试：构造用户输入经 `innerHTML` / `document.write` / `eval` 注入 DOM，断言使用 `textContent` / `createElement` 安全 API（或转义后赋值 `innerHTML`）
- [ ] 实现 `xss_html_input_type_safe` 测试：构造字段类型 VARCHAR + `FieldTypeMapping::sql_to_html_input`，断言生成 `<input type="text">`，value 转义；构造字段类型 TEXT + 用户输入含 `">`，断言 value 转义，不逃逸 input 属性
- [ ] 复用既有 `FormGenerator`（`packages/sz-orm-lc/src/lib.rs:678`）/ `CrudTemplateEngine`（`:871`）/ `FieldTypeMapping::sql_to_html_input`（`:298`），不新建 HTML 生成逻辑

**输入**：spec.md §5.11 + design.md §1.1.1 XSS 既有基础设施
**输出**：`packages/sz-orm-lc/tests/owasp_xss.rs`（5 个测试函数）
**验证方法**：
```bash
cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_xss
# 预期：5 个测试全部通过
```
**依赖**：T0.1, T0.2

### T3.2 实现 CSRF 跨站请求伪造渗透测试（REQ-V49-012）

- [ ] 新增 `packages/sz-orm-auth/tests/owasp_csrf.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `csrf_token_missing_rejected` / `csrf_token_mismatch_rejected` / `csrf_token_expired_rejected` 测试：构造请求无 CSRF token / 错误 token / 过期 token，调用 `OAuth2Server`，断言拒绝，提示"missing CSRF token" / "CSRF token mismatch" / "CSRF token expired"
- [ ] 实现 `csrf_samesite_cookie_enforced` 测试：构造 Cookie 无 SameSite 属性，断言标记为发现"missing SameSite attribute"；构造 SameSite=None，断言拒绝或警告，提示"use SameSite=Strict or Lax"
- [ ] 实现 `csrf_origin_validation` 测试：构造请求 Origin="https://evil.com"（跨站），断言拒绝，提示"origin not allowed"；构造 Origin="https://legit.example.com"，断言通过
- [ ] 实现 `csrf_oauth2_state_csrf_defense` 测试：构造 OAuth2 授权请求 state 缺失/不匹配/重放，调用 `OAuth2Server`，断言强制 state 验证（沿用 C-1 修复，state 须 OsRng 随机 + 单次使用 + 绑定会话）
- [ ] 实现 `csrf_login_csrf_new_session` 测试：构造攻击者凭证诱导受害者提交（登录 CSRF），断言登录后签发新 session_id（不复用，沿用 A09 会话固定防御）+ CSRF token
- [ ] 复用既有 `OAuth2Server`（`packages/sz-orm-auth/src/oauth2.rs:130`）/ `JwtAuthenticator`（`packages/sz-orm-auth/src/auth.rs:150`），不新建 CSRF 防御逻辑

**输入**：spec.md §5.12 + design.md §1.1.1 CSRF 既有基础设施
**输出**：`packages/sz-orm-auth/tests/owasp_csrf.rs`（6 个测试函数）
**验证方法**：
```bash
cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_csrf
# 预期：6 个测试全部通过
```
**依赖**：T0.1, T0.2

### T3.3 实现文件上传安全渗透测试（REQ-V49-013）

- [ ] 新增 `packages/sz-orm-storage/tests/owasp_file_upload.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离
- [ ] 实现 `file_upload_type_whitelist_enforced` 测试：构造上传 `.php` / `.jsp` / `.exe` / `.sh` / `.html` / `.svg`（可执行/含脚本）/ 双扩展名 `evil.php.jpg` / 大小写 `evil.PHP`，调用 `StorageBackend::put`，断言类型白名单拒绝
- [ ] 实现 `file_upload_size_limit_enforced` 测试：构造上传 10GB 超大文件 + 限制 100MB / 0 字节文件 / 负 Content-Length，断言大小限制拒绝
- [ ] 实现 `file_upload_content_magic_bytes` 测试：构造上传 `evil.jpg` 内容为 `<?php system($_GET['cmd']); ?>` / `evil.png` 内容为 `<script>alert(1)</script>`，断言 Magic bytes 不匹配（JPEG `FF D8 FF` / PNG `89 50 4E 47`），拒绝
- [ ] 实现 `file_upload_path_traversal_sanitized` 测试：构造文件名 `../../../etc/passwd` / `..\\..\\windows\\system32` / 绝对路径 `/etc/passwd`，调用 `SandboxedFs::normalize`，断言净化（移除 `../` / `..\\` / 绝对路径）或拒绝
- [ ] 实现 `file_upload_magic_bytes_match` 测试：构造上传 `.jpg` + Magic bytes `FF D8 FF`，断言通过；构造 `.jpg` + Magic bytes `50 4B 03 04`（ZIP），断言拒绝，提示"Magic bytes mismatch: expected JPEG, got ZIP"
- [ ] 实现 `file_upload_null_byte_defense` 测试：构造文件名 `evil.jpg\0.php`（Null byte 截断），断言 Null byte 防御，拒绝或净化为 `evil.jpg`
- [ ] 实现 `file_upload_temp_file_cleanup` 测试：构造上传完成/失败场景，检查临时目录，断言临时文件已删除，无残留（使用 `tempfile::TempDir` Drop 自动清理 + 显式 `std::fs::remove_file`）
- [ ] 复用既有 `StorageBackend::put`（`packages/sz-orm-storage/src/storage.rs:15`）/ `SandboxedFs::normalize`（`packages/sz-orm-wasm/src/advanced.rs:450`），不新建存储逻辑

**输入**：spec.md §5.13 + design.md §1.1.1 文件上传既有基础设施
**输出**：`packages/sz-orm-storage/tests/owasp_file_upload.rs`（7 个测试函数）
**验证方法**：
```bash
cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload
# 预期：7 个测试全部通过 + 临时文件已删除
# 验证临时文件清理：测试后检查临时目录无残留
```
**依赖**：T0.1, T0.2

### T3.4 实现业务逻辑并发竞态条件渗透测试（REQ-V49-014）

- [ ] 新增 `packages/sz-orm-core/tests/owasp_race_conditions.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（连接池 + 配额 + 缓存击穿 + TOCTOU + 双重消费 + 死锁）
- [ ] 新增 `packages/sz-orm-dtx/tests/owasp_race_conditions.rs`，文件头添加 `#![cfg(feature = "owasp-pentest-suite")]` 隔离（分布式事务竞态）
- [ ] 实现 `race_connection_pool_no_deadlock` 测试（core 包）：构造 100 并发获取连接 + 池大小 10，使用 `std::thread::scope` + `Barrier` 同步，调用 `Pool`，断言最多 10 个并发连接，无死锁/无重复发放/无泄露
- [ ] 实现 `race_tenant_quota_no_overcommit` 测试（core 包）：构造 100 并发 `QuotaEnforcer::check_quota` + 配额 10，断言最多 10 个通过，无超配；构造 Mutex panic，断言 `parking_lot::Mutex` 不 poisoning（FIND-002 修复）
- [ ] 实现 `race_cache_breakdown_singleflight` 测试（core 包）：构造 100 并发查询同一未命中 key，调用 `CacheWarmupProtection`，断言仅 1 个查询打到 DB（BloomFilter + singleflight），其余等待/拒绝
- [ ] 实现 `race_dtx_state_machine_consistent` 测试（dtx 包）：构造并发 `DtxManager::commit` + `DtxManager::rollback` 同一事务，断言仅一个成功，另一个拒绝"invalid state transition"，事务状态机一致
- [ ] 实现 `race_toctou_compare_exchange_blocks` 测试（core 包）：构造线程 A 检查 balance=100 >= amount=100 + 线程 B 扣减 balance=100 → balance=0 + 线程 A 扣减，使用 `AtomicU64::compare_exchange`，断言线程 A 扣减失败（TOCTOU 被原子操作阻止）
- [ ] 实现 `race_double_spend_idempotency` 测试（core 包）：构造 100 并发使用同一优惠码 + 幂等键，使用 `AtomicU64::compare_exchange`，断言仅 1 次成功，其余拒绝"already consumed"，无双重消费
- [ ] 实现 `race_deadlock_lock_ordering` 测试（core 包）：构造线程 A 锁 1→2 + 线程 B 锁 2→1，断言锁顺序一致（全部 1→2）或超时释放，无死锁
- [ ] 复用既有 `Pool`（`packages/sz-orm-core/src/pool.rs:749`）/ `QuotaEnforcer::check_quota`（`packages/sz-orm-core/src/tenant_quota_rls.rs:229`）/ `CacheWarmupProtection`（`packages/sz-orm-core/src/cache_warmup_protection.rs:223`）/ `DtxManager::commit/rollback`（`packages/sz-orm-dtx/src/lib.rs:476/484`）/ `parking_lot::Mutex`（FIND-002 修复），不新建并发逻辑
- [ ] 确保并发测试确定性（使用 `std::thread::scope` + `Barrier` 同步，不依赖真实时间，无 flaky）

**输入**：spec.md §5.14 + design.md §1.1.1 竞态既有基础设施
**输出**：2 个测试文件（core 6 个测试 + dtx 1 个测试）
**验证方法**：
```bash
cargo test -p sz-orm-core --features tenant-quota-rls-enhanced,cache-warmup-protection,owasp-pentest-suite --test owasp_race_conditions
cargo test -p sz-orm-dtx --features owasp-pentest-suite --test owasp_race_conditions
# 预期：全部通过（确定性并发，无 flaky）
```
**依赖**：T0.1, T0.2

---

## M4. 集成验证与门禁

> 目标：全套渗透测试聚合运行 + 23 道门禁扩展 + 占位实现检查 + 幻影交付检查 + sz-pay API 兼容性验证
> 依赖：M1 + M2 + M3 完成
> 验收标准：全套聚合测试通过 + 23 道门禁全通过 + 无占位实现 + 无幻影交付 + sz-pay 兼容

### T4.1 全套渗透测试聚合运行

- [ ] 运行全套 14 项渗透测试聚合命令：`cargo test --workspace --features owasp-pentest-suite -j 2 --no-fail-fast --test "owasp_*"`（Windows MSVC 环境，`RUST_MIN_STACK=134217728` + `CARGO_INCREMENTAL=0`）
- [ ] 验证全套渗透测试执行时间不超过 60 秒（不含真实 DB 集成测试，spec.md §4.1.1）
- [ ] 验证 v4.8.0 测试基线不回退：运行 `cargo test --workspace -j 2 --no-fail-fast`（默认 feature），确认基线测试全部通过
- [ ] 验证 feature 全组合编译：`cargo check --workspace --all-targets --all-features`（含 owasp-pentest-suite + 既有 feature 组合）

**输入**：M1 + M2 + M3 全部渗透测试
**输出**：全套测试通过报告
**验证方法**：
```bash
# Windows MSVC 环境
$env:RUST_MIN_STACK=134217728
$env:CARGO_INCREMENTAL=0
cargo test --workspace --features owasp-pentest-suite -j 2 --no-fail-fast --test "owasp_*"
# 预期：全部通过
cargo test --workspace -j 2 --no-fail-fast
# 预期：v4.8.0 基线全部通过（不回退）
cargo check --workspace --all-targets --all-features
# 预期：feature 全组合编译通过
```
**依赖**：T1.1, T1.2, T1.3, T1.4, T1.5, T2.1, T2.2, T2.3, T2.4, T2.5, T3.1, T3.2, T3.3, T3.4

### T4.2 23 道门禁扩展验证

- [ ] 运行 `pwsh scripts/gate.ps1`（23 道门禁），验证全部通过
- [ ] 特别验证门禁 4（单元/集成测试）：`cargo test --workspace` 通过
- [ ] 特别验证门禁 8（占位实现检查）：`grep -rn 'todo!\|unimplemented!\|unreachable!' packages/*/tests/owasp_*.rs` 无输出
- [ ] 特别验证门禁 10（Feature 全组合编译）：`cargo check --workspace --all-targets --all-features` 通过
- [ ] 特别验证门禁 15（幻影交付检查）：`python scripts/check-phantom-delivery.py` 通过（每项渗透测试真实调用既有防御基础设施 + 真实执行攻击向量 + 真实断言）
- [ ] 特别验证门禁 21（安全攻击测试）：扩展含 owasp-pentest-suite 渗透测试 + A06 脚本

**输入**：全套渗透测试 + 既有门禁脚本
**输出**：23 道门禁全通过报告
**验证方法**：
```bash
pwsh scripts/gate.ps1
# 预期：23 道门禁全部通过
grep -rn 'todo!\|unimplemented!\|unreachable!' packages/*/tests/owasp_*.rs
# 预期：无输出
python scripts/check-phantom-delivery.py
# 预期：通过
```
**依赖**：T4.1

### T4.3 占位实现与 unsafe 检查

- [ ] 扫描所有新增 `tests/owasp_*.rs` 文件，确认无 `todo!` / `unimplemented!` / `unreachable!` 占位实现
- [ ] 扫描所有新增 `tests/owasp_*.rs` 文件，确认无 `unsafe` 块（unsafe 零容忍铁律）
- [ ] 扫描所有新增 `tests/owasp_*.rs` 文件，确认无 crate 级 `#![allow(dead_code)]`
- [ ] 扫描所有新增 `tests/owasp_*.rs` 文件，确认无 `unwrap()` 在生产路径（测试中 `unwrap()` 允许，但须在断言上下文）

**输入**：全部新增测试文件
**输出**：扫描报告（无占位/无 unsafe/无 dead_code allow）
**验证方法**：
```bash
grep -rn 'todo!\|unimplemented!\|unreachable!' packages/*/tests/owasp_*.rs
# 预期：无输出
grep -rn 'unsafe' packages/*/tests/owasp_*.rs
# 预期：无输出
grep -rn '#!\[allow(dead_code)\]' packages/*/tests/owasp_*.rs
# 预期：无输出
```
**依赖**：T4.1

### T4.4 sz-pay API 兼容性验证

- [ ] 验证 sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包：sz-orm-core/sqlx/config/auth/macros/queue）API 不变：`cargo check -p sz-orm-core --lib`（默认 feature，不含 owasp-pentest-suite）通过
- [ ] 验证既有公开 API 签名不变：对比 v4.8.0 与 v4.9.0 的公开 API（`cargo doc --workspace --no-deps`），确认无 Breaking Change
- [ ] 验证默认 feature 行为不变：`cargo build --workspace`（默认 feature）产物与 v4.8.0 一致

**输入**：v4.8.0 基线 + v4.9.0 新增
**输出**：API 兼容性验证报告
**验证方法**：
```bash
cargo check -p sz-orm-core --lib
# 预期：通过（默认 feature，API 不变）
cargo doc --workspace --no-deps
# 预期：文档构建成功，无 Breaking Change
cargo build --workspace
# 预期：默认 feature 构建成功
```
**依赖**：T4.1

---

## M5. 文档与发布准备

> 目标：更新文档（AGENTS.md 门禁 21 + README.md）+ 生成审计报告 + 版本号更新到 4.9.0
> 依赖：M4 完成
> 验收标准：文档与代码一致 + 审计报告附 file:line 证据 + 版本号 4.9.0

### T5.1 更新 AGENTS.md 门禁 21 + README.md

- [ ] 更新 `AGENTS.md` 门禁 21（安全攻击测试）：扩展含 `owasp-pentest-suite` 渗透测试 + A06 脚本调用
- [ ] 更新 `AGENTS.md` feature gate 总览：新增 `owasp-pentest-suite`（sz-orm-core 聚合，默认关闭）
- [ ] 更新 `README.md`：新增 v4.9.0 OWASP Top 10 完整覆盖渗透测试套件说明 + 启用方式（`--features owasp-pentest-suite`）
- [ ] 更新 `docs/sz-orm-engineering-practices.md`：新增 v4.9.0 渗透测试实践说明
- [ ] 验证文档与代码一致性：`python scripts/check-doc-consistency.py` 通过 + `python scripts/check-doc-sync.py --diff HEAD` 通过

**输入**：M1 + M2 + M3 + M4 全部成果
**输出**：AGENTS.md + README.md + 工程实践文档更新
**验证方法**：
```bash
python scripts/check-doc-consistency.py
# 预期：通过
python scripts/check-doc-sync.py --diff HEAD
# 预期：通过
grep -n "owasp-pentest-suite" AGENTS.md
# 预期：有声明
grep -n "v4.9.0" README.md
# 预期：有说明
```
**依赖**：T4.1, T4.2

### T5.2 生成审计报告

- [ ] 生成 `docs/assessment/2026-08-15-v4.9.0-owasp-pentest-report.md` 审计报告，内容包括：
  - 14 项 OWASP Top 10 渗透测试覆盖矩阵
  - 每项渗透测试的攻击向量 + 防御断言 + 验证结果 + file:line 证据
  - 复用的既有安全测试基础清单
  - 新发现的漏洞（若有，如 FIND-004 修复未生效）记录为后续版本修复
- [ ] 运行审计证据验证脚本：`bash scripts/audit-verify.sh docs/assessment/2026-08-15-v4.9.0-owasp-pentest-report.md`（或 `.\scripts\audit-verify.ps1`），验证所有 file:line 引用真实存在
- [ ] 运行度量真实性扫描：`python scripts/check-metrics-real.py`（README 数字声称 vs 源码统计）

**输入**：M1 + M2 + M3 + M4 全部成果
**输出**：`docs/assessment/2026-08-15-v4.9.0-owasp-pentest-report.md`
**验证方法**：
```bash
bash scripts/audit-verify.sh docs/assessment/2026-08-15-v4.9.0-owasp-pentest-report.md
# 预期：所有 file:line 引用真实存在
python scripts/check-metrics-real.py
# 预期：通过
```
**依赖**：T4.1, T4.2

### T5.3 版本号更新到 4.9.0

- [ ] 更新 `Cargo.toml`（workspace 根）`[workspace.package] version = "4.9.0"`
- [ ] 更新所有 60 个成员包的 `Cargo.toml` version = "4.9.0"（从 workspace 继承）
- [ ] 运行发布一致性扫描：`python scripts/check-publish-consistency.py`（版本声明一致性）
- [ ] 验证 `cargo check --workspace --all-targets` 通过（版本号更新后编译通过）
- [ ] 验证 `cargo test --workspace -j 2 --no-fail-fast` 通过（版本号更新后测试通过）

**输入**：M4 完成
**输出**：版本号 4.9.0 全量更新
**验证方法**：
```bash
grep -n 'version = "4.9.0"' Cargo.toml
# 预期：workspace.package version = "4.9.0"
python scripts/check-publish-consistency.py
# 预期：通过
cargo check --workspace --all-targets
# 预期：通过
cargo test --workspace -j 2 --no-fail-fast
# 预期：通过
```
**依赖**：T4.1

---

## 里程碑验收标准

### M0 验收标准

1. ✅ 12 个包的 `Cargo.toml` 新增 `owasp-pentest-suite = []` feature 声明
2. ✅ `cargo check --workspace --all-targets` 通过（默认 feature 不含 owasp-pentest-suite）
3. ✅ `cargo check --workspace --all-targets --all-features` 通过（feature 全组合编译）
4. ✅ design.md §五 50+ 项 file:line 证据全部验证真实存在
5. ✅ v4.8.0 测试基线全部通过（不回退）

### M1 验收标准

1. ✅ A01~A05 共 5 项渗透测试全部通过（`cargo test` 全绿）
2. ✅ 每项渗透测试复用既有基础设施（不新建生产逻辑）
3. ✅ 无 `todo!` / `unimplemented!` / `unreachable!` 占位实现
4. ✅ 无 `unsafe` 块
5. ✅ 临时文件清理（文件上传测试后无残留）

### M2 验收标准

1. ✅ A06~A10 共 5 项渗透测试全部通过（A06 脚本 + A07~A10 cargo test）
2. ✅ A06 脚本跨平台（PowerShell + Bash 等价）
3. ✅ A06 脚本不修改既有 `deny.toml`
4. ✅ 每项渗透测试复用既有基础设施
5. ✅ 无占位实现 + 无 unsafe

### M3 验收标准

1. ✅ XSS / CSRF / 文件上传 / 竞态共 4 项渗透测试全部通过
2. ✅ 文件上传临时文件清理（测试后无残留）
3. ✅ 竞态测试确定性（使用 `std::thread::scope` + `Barrier`，无 flaky）
4. ✅ 每项渗透测试复用既有基础设施
5. ✅ 无占位实现 + 无 unsafe

### M4 验收标准

1. ✅ 全套 14 项渗透测试聚合运行通过（`cargo test --workspace --features owasp-pentest-suite -j 2 --no-fail-fast --test "owasp_*"`）
2. ✅ 全套执行时间 ≤ 60 秒（不含真实 DB 集成测试）
3. ✅ 23 道门禁全部通过（`pwsh scripts/gate.ps1`）
4. ✅ 无占位实现（grep 无输出）+ 无 unsafe（grep 无输出）+ 无 crate 级 `#![allow(dead_code)]`
5. ✅ 幻影交付检查通过（`python scripts/check-phantom-delivery.py`）
6. ✅ sz-pay API 兼容性验证通过（无 Breaking Change）
7. ✅ v4.8.0 测试基线不回退

### M5 验收标准

1. ✅ AGENTS.md 门禁 21 扩展含 owasp-pentest-suite + A06 脚本
2. ✅ README.md 新增 v4.9.0 渗透测试套件说明
3. ✅ 文档与代码一致性检查通过（`check-doc-consistency.py` + `check-doc-sync.py`）
4. ✅ 审计报告生成 + 所有 file:line 证据真实存在（`audit-verify.sh` 通过）
5. ✅ 版本号 4.9.0 全量更新（workspace + 60 个成员包）
6. ✅ 发布一致性扫描通过（`check-publish-consistency.py`）
7. ✅ 版本号更新后 `cargo check` + `cargo test` 通过

---

## 风险与缓解

| 风险 ID | 风险描述 | 影响 | 概率 | 缓解措施 | 负责任务 |
|---------|---------|------|------|---------|---------|
| R1 | FIND-001 修复未在 `ModelDefinition::validate_identifier` 生效 | A03 表名验证渗透测试失败（漏洞被证明存在） | 中 | 渗透测试 A03 `a03_model_name_validation_finds_001` 验证 `ModelDefinition::validate_identifier("users\" DROP TABLE users; --")` 是否拒绝。若未拒绝，标记为发现记录在审计报告（符合 spec.md §1.4.8），不阻塞渗透测试交付 | T1.3 |
| R2 | FIND-004 修复未在 `WasmRealDbConnection::new` 生效 | A10 SSRF 渗透测试失败（内网/非法协议未拒绝） | 中 | 渗透测试 A10 各攻击向量验证 `new` 是否拒绝。若未拒绝，标记为发现记录在审计报告，不阻塞交付 | T2.5 |
| R3 | 真实 DB 渗透测试 OOM（Windows MSVC） | A03/A10/上传/竞态测试因内存不足失败 | 低 | 使用 `RUST_MIN_STACK=134217728` + `CARGO_INCREMENTAL=0` + `cargo test -j 2 --no-fail-fast`（spec.md §4.1） | T4.1 |
| R4 | 临时文件残留 | 文件上传渗透测试临时文件未清理 | 低 | 测试结束显式 `std::fs::remove_file` + `tempfile::TempDir`（Drop 自动清理），沿用既有铁律 | T3.3 |
| R5 | cargo cyclonedx 未安装 | A06 SBOM 生成失败 | 低 | 脚本检测 `cargo cyclonedx` 是否可用，不可用时跳过 SBOM 部分并警告（不阻塞 CVE/许可证/yanked/重复/来源检查） | T2.1 |
| R6 | 并发测试不稳定（flaky） | 竞态渗透测试偶发失败 | 低 | 使用确定性并发（`std::thread::scope` + `Barrier` 同步），不依赖真实时间；原子操作断言无 flaky | T3.4 |
| R7 | grep 误报（测试代码含 "secret"） | A02 硬编码密钥扫描误报 | 低 | grep 排除 `tests/` 目录 + 文档注释，仅扫描 `src/`（spec.md §5.2.1.3） | T1.2 |
| R8 | feature 组合编译冲突 | `owasp-pentest-suite` + 既有 feature 组合编译失败 | 低 | 聚合 feature 仅作测试编译门控（空数组声明），不引入新依赖；门禁 10 Feature 全组合编译验证 | T0.1, T4.1 |
| R9 | 渗透测试破坏既有测试基线 | v4.8.0 测试回退 | 低 | 所有新增为独立测试文件（`tests/owasp_*.rs`），不修改既有测试；门禁 4 验证 | T4.1 |
| R10 | 跨平台脚本不一致 | PowerShell 与 Bash 脚本行为差异 | 低 | A06 脚本双实现（`.ps1` + `.sh`），逻辑等价；门禁 8 跨平台意识 | T2.1 |
| R11 | 幻影交付（测试存在但未真实调用既有基础设施） | 渗透测试通过但未验证防御 | 中 | 每项渗透测试须真实调用既有防御基础设施 + 真实执行攻击向量 + 真实断言；门禁 15 `python scripts/check-phantom-delivery.py` 验证 | T4.2 |
| R12 | API Breaking Change | sz-pay 生产依赖破坏 | 低 | 所有新能力通过 `owasp-pentest-suite` feature gate 隔离，默认关闭；T4.4 验证 API 兼容性 | T4.4 |

---

## 任务依赖关系图

```
M0（基础设施准备）
├── T0.1 声明 feature gate
├── T0.2 验证既有证据
└── T0.3 验证基线不回退（依赖 T0.1）

M1（A01~A05 渗透测试，依赖 M0）
├── T1.1 A01 访问控制深化（依赖 T0.1, T0.2）
├── T1.2 A02 加密失败深化（依赖 T0.1, T0.2）
├── T1.3 A03 注入深化（依赖 T0.1, T0.2）
├── T1.4 A04 不安全设计（依赖 T0.1, T0.2）
└── T1.5 A05 安全配置错误深化（依赖 T0.1, T0.2）

M2（A06~A10 渗透测试，依赖 M0）
├── T2.1 A06 过时组件深化（依赖 T0.1, T0.2）
├── T2.2 A07 完整性失败（依赖 T0.1, T0.2）
├── T2.3 A08 日志监控失败深化（依赖 T0.1, T0.2）
├── T2.4 A09 认证失败深化（依赖 T0.1, T0.2）
└── T2.5 A10 SSRF 深化（依赖 T0.1, T0.2）

M3（附加攻击面，依赖 M0）
├── T3.1 XSS（依赖 T0.1, T0.2）
├── T3.2 CSRF（依赖 T0.1, T0.2）
├── T3.3 文件上传安全（依赖 T0.1, T0.2）
└── T3.4 业务逻辑并发竞态（依赖 T0.1, T0.2）

M4（集成验证与门禁，依赖 M1 + M2 + M3）
├── T4.1 全套聚合运行（依赖 T1.1~T1.5, T2.1~T2.5, T3.1~T3.4）
├── T4.2 23 道门禁扩展（依赖 T4.1）
├── T4.3 占位实现与 unsafe 检查（依赖 T4.1）
└── T4.4 sz-pay API 兼容性验证（依赖 T4.1）

M5（文档与发布准备，依赖 M4）
├── T5.1 更新 AGENTS.md + README.md（依赖 T4.1, T4.2）
├── T5.2 生成审计报告（依赖 T4.1, T4.2）
└── T5.3 版本号更新到 4.9.0（依赖 T4.1）
```

**并行开发说明**：
- M1 内 5 项任务（T1.1~T1.5）相互独立，可并行开发
- M2 内 5 项任务（T2.1~T2.5）相互独立，可并行开发
- M3 内 4 项任务（T3.1~T3.4）相互独立，可并行开发
- M1 + M2 + M3 三个里程碑可并行开发（均仅依赖 M0）
- M4 必须在 M1 + M2 + M3 全部完成后执行
- M5 必须在 M4 完成后执行

---

## 需求覆盖追溯矩阵

| 需求 ID | 对应任务 | 验收条件数 | 测试文件 | 验证命令 |
|---------|---------|-----------|---------|---------|
| REQ-V49-001 A01 访问控制深化 | T1.1 | 8 | `tests/owasp_a01_access_control.rs` | `cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a01_access_control` |
| REQ-V49-002 A02 加密失败深化 | T1.2 | 8 | `tests/owasp_a02_crypto_failures.rs`（crypto + config） | `cargo test -p sz-orm-crypto --features owasp-pentest-suite --test owasp_a02_crypto_failures` + `cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a02_crypto_failures` |
| REQ-V49-003 A03 注入深化 | T1.3 | 8 | `tests/owasp_a03_injection.rs`（core + swagger + lc） | `cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a03_injection` + `cargo test -p sz-orm-swagger --features openapi-reverse,owasp-pentest-suite --test owasp_a03_injection` + `cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_a03_injection` |
| REQ-V49-004 A04 不安全设计 | T1.4 | 9 | `tests/owasp_a04_insecure_design.rs`（core + wasm + grpc + dtx） | `cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a04_insecure_design` + `cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a04_insecure_design` + `cargo test -p sz-orm-grpc --features owasp-pentest-suite --test owasp_a04_insecure_design` + `cargo test -p sz-orm-dtx --features cross-lang-dtx,owasp-pentest-suite --test owasp_a04_insecure_design` |
| REQ-V49-005 A05 安全配置错误深化 | T1.5 | 8 | `tests/owasp_a05_misconfig.rs`（config + core） | `cargo test -p sz-orm-config --features owasp-pentest-suite --test owasp_a05_misconfig` + `cargo test -p sz-orm-core --features owasp-pentest-suite --test owasp_a05_misconfig` |
| REQ-V49-006 A06 过时组件深化 | T2.1 | 8 | `scripts/owasp_a06_vulnerable_components.{ps1,sh}` | `pwsh scripts/owasp_a06_vulnerable_components.ps1` + `cargo audit` + `cargo deny check` |
| REQ-V49-007 A07 完整性失败 | T2.2 | 8 | `tests/owasp_a07_integrity.rs` | `cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a07_integrity` |
| REQ-V49-008 A08 日志监控失败深化 | T2.3 | 8 | `tests/owasp_a08_logging_failures.rs`（audit + masking） | `cargo test -p sz-orm-audit --features owasp-pentest-suite --test owasp_a08_logging_failures` + `cargo test -p sz-orm-masking --features owasp-pentest-suite --test owasp_a08_logging_failures` |
| REQ-V49-009 A09 认证失败深化 | T2.4 | 10 | `tests/owasp_a09_auth_failures.rs` | `cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_a09_auth_failures` |
| REQ-V49-010 A10 SSRF 深化 | T2.5 | 7 | `tests/owasp_a10_ssrf.rs` | `cargo test -p sz-orm-wasm --features wasm-real-db,owasp-pentest-suite --test owasp_a10_ssrf` |
| REQ-V49-011 XSS | T3.1 | 7 | `tests/owasp_xss.rs` | `cargo test -p sz-orm-lc --features owasp-pentest-suite --test owasp_xss` |
| REQ-V49-012 CSRF | T3.2 | 7 | `tests/owasp_csrf.rs` | `cargo test -p sz-orm-auth --features owasp-pentest-suite --test owasp_csrf` |
| REQ-V49-013 文件上传安全 | T3.3 | 9 | `tests/owasp_file_upload.rs` | `cargo test -p sz-orm-storage --features owasp-pentest-suite --test owasp_file_upload` |
| REQ-V49-014 业务逻辑并发竞态 | T3.4 | 9 | `tests/owasp_race_conditions.rs`（core + dtx） | `cargo test -p sz-orm-core --features tenant-quota-rls-enhanced,cache-warmup-protection,owasp-pentest-suite --test owasp_race_conditions` + `cargo test -p sz-orm-dtx --features owasp-pentest-suite --test owasp_race_conditions` |

**覆盖完整性**：14 项需求全部映射到 14 个任务（T1.1~T1.5 + T2.1~T2.5 + T3.1~T3.4），无遗漏。

---

> 文档结束。本任务文档将 spec.md 14 项 OWASP Top 10 完整覆盖渗透测试需求 + design.md 技术设计转化为 24 个可执行任务（M0~M5 六个里程碑），每个任务附验证方法（cargo test / grep），所有任务遵循"不修改既有生产代码、新增 tests/owasp_*.rs 和 scripts/owasp_*.ps1、feature gate owasp-pentest-suite 隔离、复用既有基础设施、无占位实现、无 unsafe、临时文件清理"约束。