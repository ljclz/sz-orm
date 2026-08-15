# sz-orm 白帽安全代码审计报告

- **审计日期**：2026-08-14
- **审计范围**：sz-orm workspace 全部 60 个包
- **审计方法**：code-reviewer skill 5 步流程（参数评估→风险发现→深度分析→置信度评估→生成结果）
- **风险类别**：内存安全 / 注入 / 并发 / 资源泄露 / 信息泄露
- **筛选标准**：severity ≥ MEDIUM 且 confidence ≥ 7

## 审计摘要

| 指标 | 值 |
|------|-----|
| 扫描文件数 | ~500+ .rs 文件 |
| 发现总数 | 4 |
| HIGH | 0 |
| MEDIUM | 4 |
| LOW（已过滤） | ~15 |
| unsafe 块（已审查） | 8（全部有 SAFETY 注释或在测试中） |
| 硬编码凭证（已审查） | 12（全部在测试代码中） |

## 发现列表

### FIND-001: SQL 注入（代码生成时）— sz-orm-lc model.name 未验证

- **severity**: MEDIUM
- **confidence**: 7/10
- **类别**: injection
- **位置**:
  - `packages/sz-orm-lc/src/lib.rs:954` — `format!("DELETE FROM \"{}\" WHERE \"id\" = $1;", model.name)`
  - `packages/sz-orm-lc/src/lib.rs:959` — `format!("SELECT COUNT(*) AS total FROM \"{}\";", model.name)`
  - `packages/sz-orm-lc/src/lib.rs:944` — `format!("UPDATE \"{}\" SET {} WHERE \"id\" = {};", model.name, ...)`
  - `packages/sz-orm-lc/src/lib.rs:888` — `generate_insert` 同样使用 `model.name`
  - `packages/sz-orm-lc/src/lib.rs:904` — `generate_select_by_id` 同样使用 `model.name`
  - `packages/sz-orm-lc/src/lib.rs:918` — `generate_select_all` 同样使用 `model.name`

**描述**：

`ModelDefinition` 结构体（`packages/sz-orm-lc/src/lib.rs:35`）的 `name` 字段为 `pub name: String`，通过 `ModelDefinition::new(name: &str)`（line 43）构造时**不进行任何验证或转义**。该 name 随后被直接拼接到 SQL 语句中：

```rust
pub fn generate_delete(model: &ModelDefinition) -> String {
    format!("DELETE FROM \"{}\" WHERE \"id\" = $1;", model.name)  // line 954
}
```

`ModelDefinition` 标注了 `#[derive(Deserialize)]`（line 34），可从 JSON 反序列化。如果用户能控制模型定义 JSON（例如通过低代码引擎的 API 接口），且 `model.name` 包含恶意内容如 `users" DROP TABLE users; --`，生成的 SQL 将为：

```sql
DELETE FROM "users" DROP TABLE users; --" WHERE "id" = $1;
```

生成的 SQL 被嵌入到 Rust 源代码字符串中（`generate_rust_repository`，line 991），通过 `sqlx::query({insert_sql:?})` 执行。虽然 `{:?}` Debug 格式化会转义 `"`，但 SQL 语句本身在运行时会被数据库解析执行。

**攻击路径**：
1. 攻击者提交恶意模型定义 JSON → `serde_json::from_str` 反序列化为 `ModelDefinition`
2. `CrudTemplateEngine::generate_delete(model)` 生成恶意 SQL
3. 生成的 Rust 代码被编译执行，恶意 SQL 在运行时执行

**修复建议**：
在 `ModelDefinition::new` 中添加表名验证，只允许字母、数字、下划线：

```rust
pub fn new(name: &str) -> Self {
    Self::validate_name(name).expect("invalid model name");
    Self { name: name.to_string(), ... }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 63 {
        return Err("model name must be 1-63 chars".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("model name must be alphanumeric + underscore".into());
    }
    Ok(())
}
```

---

### FIND-002: Mutex poisoning — 生产代码中大量 `.lock().unwrap()`

- **severity**: MEDIUM
- **confidence**: 8/10
- **类别**: concurrency
- **位置**（生产代码中代表性的 15 处，完整 186 处）：
  - `packages/sz-orm-core/src/cache_warmup_protection.rs:232` — `self.bloom.lock().unwrap().add(key)?`
  - `packages/sz-orm-core/src/cache_warmup_protection.rs:242` — `self.bloom.lock().unwrap().might_contain(&bloom_key)`
  - `packages/sz-orm-core/src/cache_warmup_protection.rs:252` — `let bloom = self.bloom.lock().unwrap()`
  - `packages/sz-orm-core/src/tenant_quota_rls.rs:187,206,211,223,251,266,279,418,430,441,579,597`
  - `packages/sz-orm-core/src/connection_tenant.rs:174,179,189,195,203,209`
  - `packages/sz-orm-core/src/l1_cache.rs:521,531`
  - `packages/sz-orm-core/src/rollback_zero_downtime.rs:532,541`
  - `packages/sz-orm-queue/src/delayed_priority.rs:243,269,291,328,334,365,369,500,534`
  - `packages/sz-orm-queue/src/cdc/capturer.rs:521,522`
  - `packages/sz-orm-queue/src/dlx.rs:339,347`
  - `packages/sz-orm-storage/src/multicloud_cost_forecast.rs:142,162,220,336,346,380,386,399,400`
  - `packages/sz-orm-observability/src/anomaly_remediation_rca.rs:194,199,226,253,259,282,283,350,398,416`
  - `packages/sz-orm-fusion/src/ttl_cache.rs:142`

**描述**：

Rust 的 `std::sync::Mutex` 在持有锁的线程 panic 时会被 "poisoned"。此后任何 `.lock().unwrap()` 调用都会 panic，导致级联失败。在生产代码中，如果 BloomFilter 操作、配额更新、缓存写入等任何一处 panic，整个服务会因 Mutex poisoning 而崩溃，且无法自动恢复。

**影响**：
- 缓存击穿保护失效 → 缓存穿透
- 租户配额管理失效 → 配额绕过
- 延迟队列失效 → 消息丢失
- CDC 捕获器失效 → 数据不一致

**修复建议**：
将生产代码中的 `std::sync::Mutex` 替换为 `parking_lot::Mutex`（不会 poisoning），或使用 `.lock().unwrap_or_else(|e| e.into_inner())` 模式忽略 poisoning：

```rust
// 方案 A：替换为 parking_lot::Mutex（推荐）
use parking_lot::Mutex;
// .lock() 直接返回 guard，不会 panic

// 方案 B：忽略 poisoning
let guard = self.bloom.lock().unwrap_or_else(|e| e.into_inner());
```

---

### FIND-003: 命令行密码泄露 — sz-orm-macros 编译期验证

- **severity**: MEDIUM
- **confidence**: 7/10
- **类别**: information
- **位置**：
  - `packages/sz-orm-macros/src/lib.rs:1602` — `std::process::Command::new("sqlplus").args(["-S", "-L", &conn_str])`
  - `packages/sz-orm-macros/src/lib.rs:1640` — `std::process::Command::new("sqlcmd").args(["-S", ..., "-U", &parsed.user, "-P", &parsed.password, ...])`
  - `packages/sz-orm-macros/src/lib.rs:1587` — `conn_str = format!("{}/{}@{}:{}/{}", parsed.user, parsed.password, ...)`

**描述**：

在 `db-verify` feature 启用时，编译期 SQL 验证通过 `sqlplus` 和 `sqlcmd` 命令行工具连接数据库。密码通过命令行参数传递（`-P pass` 或连接串 `user/pass@host`），会出现在进程命令行中，可被同一机器上的其他用户通过 `ps` 命令或 `/proc` 文件系统看到。

**影响**：
- 密码泄露给同机器其他用户
- 仅在编译时（`db-verify` feature 启用），不影响运行时
- 但 CI/CD 环境中可能被日志捕获

**修复建议**：
通过环境变量或 stdin 传递密码，避免命令行参数：

```rust
// sqlcmd 支持环境变量 SQLCMDPASSWORD
std::env::set_var("SQLCMDPASSWORD", &parsed.password);
let out = std::process::Command::new("sqlcmd")
    .args(["-S", &format!("{},{}", parsed.host, parsed.port), "-U", &parsed.user, "-d", &parsed.database, "-Q", &query])
    .output()?;

// sqlplus 通过 stdin 传递连接串
let mut child = std::process::Command::new("sqlplus")
    .args(["-S", "-L", "/nolog"])
    .stdin(Stdio::piped()).spawn()?;
child.stdin.take().unwrap().write_all(format!("connect {}/{}@{}:{}/{}\n", user, pass, host, port, service).as_bytes())?;
```

---

### FIND-004: SSRF — sz-orm-wasm proxy_url 未验证

- **severity**: MEDIUM
- **confidence**: 7/10
- **类别**: injection / SSRF
- **位置**：
  - `packages/sz-orm-wasm/src/real_db/connection.rs:33` — `pub fn new(proxy_url: &str, ...) -> Self`
  - `packages/sz-orm-wasm/src/real_db/connection.rs:134` — `reqwest::Client::new().post(&self.proxy_url)`

**描述**：

`WasmRealDbConnection::new(proxy_url: &str, ...)` 接受任意字符串作为代理 URL，不验证 URL 协议（`http://` / `https://`）或目标地址。`send_request_http` 方法直接使用 `reqwest::Client::new().post(&self.proxy_url)` 发送 HTTP 请求到该 URL。

如果 `proxy_url` 来自用户输入或配置文件且未经验证，攻击者可以：
- 指向内网服务（`http://127.0.0.1:8080`）进行 SSRF
- 指向 `file://` 协议读取本地文件（如果 reqwest 支持）
- 指向恶意服务器窃取请求体中的数据

**修复建议**：
在 `new()` 中验证 URL：

```rust
pub fn new(proxy_url: &str, ...) -> Result<Self, WasmRealDbError> {
    let url = url::Url::parse(proxy_url).map_err(|_| WasmRealDbError::InvalidUrl)?;
    match url.scheme() {
        "http" | "https" => {},
        _ => return Err(WasmRealDbError::InvalidUrl),
    }
    Ok(Self { proxy_url: proxy_url.to_string(), ... })
}
```

---

## 已审查但未报告的项（confidence < 7 或 severity LOW）

| 项 | 位置 | 原因 |
|----|------|------|
| unsafe Waker::from_raw | `sz-orm-wasm/src/real_db/connection.rs:318` | 测试代码，标准 dummy waker 模式 |
| unsafe FFI alloc/dealloc | `sz-orm-cabi/src/ffi_memory.rs:32,52` | 有 SAFETY 注释，layout 匹配 |
| unsafe 测试代码 | `sz-orm-cpp/lib.rs:60`, `sz-orm-java/lib.rs:74`, `sz-orm-go/lib.rs:60` | 测试代码 |
| 硬编码 "secret"/"valid-token" | 12 处 | 全部在测试代码或文档注释中 |
| eprintln! 敏感信息 | 93 处 | 全部是错误日志，未打印密码/token |
| 环境变量密码 | 12 处 | 全部在测试代码中，有默认值回退 |
| SQL format! 拼接 | 305 处 | 大部分是表名拼接（受控标识符）或测试代码 |
| serde_json 反序列化 | 134 处 | serde_json 不执行代码，风险在于反序列化后的数据使用方式 |
| reqwest HTTP 请求 | 61 处 | 大部分是配置的 API 端点，非用户控制 |
| 路径遍历 | `sz-orm-wasm/src/advanced.rs` | SandboxedFs 有防护，测试验证了路径遍历被阻止 |

## 修复计划

| 发现 | 修复方案 | 影响范围 |
|------|---------|---------|
| FIND-001 | `ModelDefinition::new` 添加表名验证 | `sz-orm-lc` |
| FIND-002 | 生产代码 `.lock().unwrap()` 替换为 `parking_lot::Mutex` 或忽略 poisoning | `sz-orm-core`, `sz-orm-queue`, `sz-orm-storage`, `sz-orm-observability`, `sz-orm-fusion` |
| FIND-003 | 编译期验证改用环境变量/stdin 传递密码 | `sz-orm-macros` |
| FIND-004 | `WasmRealDbConnection::new` 添加 URL 验证 | `sz-orm-wasm` |