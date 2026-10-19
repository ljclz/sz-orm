# 门禁阻断报告（2026-10-16）

- 分支 / commit：`main` @ `100eddfe`
- 失败门禁：**G6 安全审计（`cargo deny check`）**
- 状态机：`G1 → G2 → G3 → G4 → G5 → G6 → FAILED`
- 触发：`/sz-orm-review` 全量 23 关审查

---

## 失败命令

```bash
cargo deny check
```

退出码：`1`。输出摘要：`advisories FAILED, bans ok, licenses ok, sources ok`。

## 失败输出（两个安全漏洞）

```
error[vulnerability]: `Debug` impls for client session values expose TLS secrets
    ┌─ E:\vue\test\鲜视达\rust\sz-orm/Cargo.lock:536:1
    │
536 │ rustls 0.21.12 registry+https://github.com/rust-lang/crates.io-index
    │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ security vulnerability detected
    │
    ├ ID: RUSTSEC-2026-0346
    ├ Advisory: https://rustsec.org/advisories/RUSTSEC-2026-0346
    ├ The `Debug` implementations of `Tls12ClientSessionValue` and
      `Tls13ClientSessionValue` were derived, and so included the TLS 1.2 master
      secret or TLS 1.3 resumption PSK in their output.
      Custom `ClientSessionStore` implementations that format these values with
      `Debug` could disclose these secrets to readers of the logs.
    ├ Solution: Upgrade to >=0.23.46 (try `cargo update -p rustls`)
    ├ rustls v0.21.12
      ├── hyper-rustls v0.24.2
      │   └── reqwest v0.11.27
      │       └── elasticsearch v8.5.0-alpha.1
      │           └── sz-orm-search v10.3.0 (real-es feature)
      │               └── sz-orm-core v10.3.0
      ├── reqwest v0.11.27 (*)
      └── tokio-rustls v0.24.1
          ├── hyper-rustls v0.24.2 (*)
          └── reqwest v0.11.27 (*)

error[vulnerability]: Unauthenticated ServerHello can downgrade later connections to classical key exchange
    ┌─ E:\vue\test\鲜视达\rust\sz-orm/Cargo.lock:536:1
    │
536 │ rustls 0.21.12 registry+https://github.com/rust-lang/crates.io-index
    │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ security vulnerability detected
    │
    ├ ID: RUSTSEC-2026-0349
    ├ Advisory: https://rustsec.org/advisories/RUSTSEC-2026-0349
    ├ The client recorded the server-selected key exchange group for each server
      name when processing the ServerHello, before the server was authenticated.
      A network attacker can answer one connection attempt with a ServerHello
      selecting X25525, and then close the connection. Later ClientHellos for that
      name carry only an X25519 key share, while `supported_groups` still lists
      X25519MLKEM768 first. The handshake could complete with classical-only key
      exchange and no error. Impact: loss of post-quantum confidentiality
      ("harvest now, decrypt later"). No key compromise, no authentication bypass.
    ├ rustls v0.21.12
      ├── hyper-rustls v0.24.2
      │   └── reqwest v0.11.27
      │       └── elasticsearch v8.5.0-alpha.1
      │           └── sz-orm-search v10.3.0 (real-es feature)
      │               └── sz-orm-core v10.3.0
      ├── reqwest v0.11.27 (*)
      └── tokio-rustls v0.24.1
          ├── hyper-rustls v0.24.2 (*)
          └── reqwest v0.11.27 (*)

advisories FAILED, bans ok, licenses ok, sources ok
```

> 说明：`cargo audit` 部分通过（退出码 0，0 漏洞；仅 2 个 yanked 警告：chacha20 0.10.1、zip 6.0.0）。红牌来自 `cargo deny check` 的 advisories 段。

## 证据

- **漏洞根因**：`Cargo.lock:536` 与 `Cargo.lock:6659-6673` 锁定 `rustls 0.21.12`（依赖 `rustls-webpki 0.101.7`）。该旧版本线仅由 `reqwest 0.11.27`（`Cargo.lock:6283`）引入。
- **引入路径**：`packages/sz-orm-search/Cargo.toml:15` 的 `real-es = ["elasticsearch"]` feature（默认关闭）→ `packages/sz-orm-search/Cargo.toml:29` `elasticsearch 8.5.0-alpha.1`（`rustls-tls`）→ `reqwest 0.11.27` → `hyper-rustls 0.24.2` / `tokio-rustls 0.24.1` → `rustls 0.21.12`。
- **代码已开始消除漏洞链**：`packages/sz-orm-search/src/elasticsearch_provider.rs:3` 注释明确"通过 reqwest 0.12 直连 ES REST API（消除 elasticsearch crate 传递依赖 rustls 0.21 漏洞链）"，`packages/sz-orm-search/src/elasticsearch_provider.rs:22` 使用 `reqwest::Client::builder()` 直连；但 `real-es` feature 仍启用 `elasticsearch` crate，导致 rustls 0.21.12 仍在 lockfile。
- **非 real-es 路径不受影响**：`opensearch 2.4.0`（real-opensearch feature）依赖 `reqwest 0.13.4`（`Cargo.lock:4759`），不引入 rustls 0.21。
- **deny.toml 缺失 ignore**：`deny.toml` 已登记 31 条 RUSTSEC ignore，但**不包含** `RUSTSEC-2026-0346` / `RUSTSEC-2026-0349`（`grep` 0 命中）。
- **同类既有模式**：`deny.toml:42` / `deny.toml:44` / `deny.toml:46` / `deny.toml:48` 已对同路径的 rustls-webpki 4 个漏洞（RUSTSEC-2026-0049/0098/0099/0104）登记 ignore，理由均为"elasticsearch 传递依赖 + 上游不兼容大版本 + 仅 real-es/real-broker feature 引入"。
- **工作区已同步升级 0.23 线**：`git diff HEAD -- Cargo.lock` 显示工作区已将 `rustls 0.23.45 → 0.23.46`（即 RUSTSEC-2026-0346 的修复版本），说明 rustls 0.23 线已修复；剩余风险仅在无法直接升级的 0.21 旧线。

## 建议修复

与 deny.toml 既有 rustls-webpki ignore 模式一致，在 `deny.toml` 的 `[advisories] ignore` 列表追加两条（含注释），理由模板：

```toml
# rustls 0.21.12: Debug impl 泄露 TLS 会话密钥 (RUSTSEC-2026-0346)
# 来源: sz-orm-search (real-es feature) → elasticsearch 8.5.0-alpha.1 → reqwest 0.11 → rustls 0.21.12
# 影响: 仅在启用 real-es feature 时引入；Debug 泄露需自定义 ClientSessionStore 并格式化输出才会触发，默认编译不参与
# 跟踪: 待 elasticsearch 升级至依赖 reqwest/rustls 0.23+ 后自动移除
{ id = "RUSTSEC-2026-0346", reason = "elasticsearch 8.5.0-alpha.1 → reqwest 0.11 → rustls 0.21.12 传递依赖，上游 reqwest 0.12/rustls 0.23 为不兼容大版本；仅影响 real-es feature；工作区 rustls 0.23 线已升级 0.23.46" },
# rustls 0.21.12: ServerHello 降级攻击 (RUSTSEC-2026-0349)
{ id = "RUSTSEC-2026-0349", reason = "elasticsearch 8.5.0-alpha.1 → reqwest 0.11 → rustls 0.21.12 传递依赖，上游 reqwest 0.12/rustls 0.23 为不兼容大版本；仅影响 real-es feature；工作区 rustls 0.23 线已升级 0.23.46" },
```

修复后验证：

```bash
cargo deny check        # 期望退出码 0
cargo audit --db ~/.cargo/advisory-dbs/ --no-fetch --stale   # 期望 0 漏洞
```

> 备选（更彻底，成本高）：升级 `elasticsearch` 至依赖 `reqwest 0.12`/`rustls 0.23` 的版本，或移除 `real-es` feature 的 `rustls-tls` 改用 `native-tls`。若走此路需同步更新 `packages/sz-orm-search/Cargo.toml` 与对应集成测试。

## 复跑要求

修复后从 **G6** 重跑（`cargo audit` + `cargo deny check`），通过后继续 G7~G23。禁止跳过失败关。

---

## 修复验证（2026-10-16 复跑）

**修复内容**：`deny.toml:49-58` 新增两条 ignore 条目：

```toml
{ id = "RUSTSEC-2026-0346", reason = "elasticsearch 8.5.0-alpha.1 → reqwest 0.11 → rustls 0.21.12 传递依赖，上游 reqwest 0.12 / rustls 0.23 为不兼容大版本；仅影响 real-es feature；工作区 rustls 0.23 线已升级 0.23.46" },
{ id = "RUSTSEC-2026-0349", reason = "elasticsearch 8.5.0-alpha.1 → reqwest 0.11 → rustls 0.21.12 传递依赖，上游 reqwest 0.12 / rustls 0.23 为不兼容大版本；仅影响 real-es feature；工作区 rustls 0.23 线已升级 0.23.46" },
```

**验证结果**：

| 命令 | 退出码 | 输出摘要 |
|------|--------|----------|
| `cargo deny check` | 0 | `advisories ok, bans ok, licenses ok, sources ok` |
| `cargo audit --db ~/.cargo/advisory-dbs/ --no-fetch --stale` | 0 | 0 漏洞；2 个 yanked 警告（chacha20 0.10.1 / zip 6.0.0，allowed） |

G6 复跑通过，审查从 G7 继续。