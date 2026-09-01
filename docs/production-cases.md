# SZ-ORM 生产案例

> 更新日期：2026-08-22
> 适用版本：v5.0.0

---

## 案例 1：sz-pay 支付系统

### 概要

| 项目 | 值 |
|------|-----|
| 项目名称 | sz-pay |
| 项目路径 | `E:\vue\test\sz-pay\server\sz-rust` |
| 技术栈 | Rust + axum + sz-orm v5.0.0 |
| 使用包数 | 6 |
| E2E 测试数 | 27 |
| 接线状态 | 全部通过（PHANTOM-1: 0，接线 4/4） |

### 使用的 sz-orm 包

| 包 | 用途 | 接线方式 | 测试数 |
|----|------|---------|--------|
| sz-orm-graph | 图数据库 HTTP API | axum router `/api/graph/*` | 3 HTTP + 2 wiring |
| sz-orm-vector | 向量搜索 HTTP API | axum router `/api/vector/*` | 4 HTTP + 3 wiring |
| sz-orm-audit | 审计日志链 | service 封装（HashChainAuditor） | 2 wiring |
| sz-orm-crypto | 密码学服务 | service 封装（AES/PBKDF2/HMAC） | 4 wiring |
| sz-orm-masking | 数据脱敏 | service 封装（DataMasker） | 5 wiring |
| sz-orm-auth | 认证授权 | service 封装（RBAC+TOTP） | 4 wiring |

### 接线代码证据

- `src/controllers/graph_controller.rs` — 3 个 axum handler（节点 CRUD + 关系查询）
- `src/controllers/vector_controller.rs` — 4 个 axum handler（向量插入 + 相似搜索 + 批量 + 元数据）
- `src/services/audit_service.rs` — HashChainAuditor 封装
- `src/services/crypto_service.rs` — 6 个密码学 API
- `src/services/masking_service.rs` — 6 个脱敏 API
- `src/services/auth_rbac_service.rs` — 5 个 RBAC+TOTP API
- `src/router.rs` — graph/vector 路由挂载

### Cargo.toml 依赖

```toml
[dependencies]
sz-orm-graph = { version = "5.0.0", features = ["neo4j"] }
sz-orm-vector = { version = "5.0.0", features = ["hnsw"] }
sz-orm-audit = "5.0.0"
sz-orm-crypto = "5.0.0"
sz-orm-masking = "5.0.0"
sz-orm-auth = { version = "5.0.0", features = ["rbac", "totp"] }
```

### 验证结果

```
cargo test --workspace -j 2 --no-fail-fast
27 passed; 0 failed
```

---

## 案例 2：sz-orm-cli 数据库迁移工具

### 概要

| 项目 | 值 |
|------|-----|
| 项目名称 | sz-orm-cli |
| 项目路径 | `cli/`（工作空间成员） |
| 技术栈 | Rust + sz-orm-core + sz-orm-sql-validator |
| 命令数 | 12+ |
| 使用场景 | 数据库迁移管理、Model 骨架生成、SQL 校验、Seeder 数据填充 |

### 功能列表

| 命令 | 用途 |
|------|------|
| `info` | 显示 ORM 概要信息 |
| `migrate` | 执行数据库迁移 |
| `migrate:status` | 查看迁移进度 |
| `migrate:rollback` | 回滚最后一个迁移 |
| `migrate:fresh` | 重建数据库（开发环境） |
| `make:migration <name>` | 生成迁移文件骨架 |
| `make:model <name>` | 生成 Model 骨架代码 |
| `make:seeder <name>` | 生成 Seeder 文件骨架 |
| `seed` | 执行 Seeder 数据填充 |
| `sql:validate <sql>` | SQL 校验（防注入） |
| `dialect list` | 列出所有支持的方言 |
| `dialect show <db>` | 显示指定方言信息 |

### 使用示例

```bash
# 生成迁移
sz-orm make:migration create_users

# 生成 Model
sz-orm make:model User --pk-type i32

# 执行迁移
sz-orm migrate --dsn sqlite://./app.db

# SQL 校验
sz-orm sql:validate "SELECT * FROM users WHERE id = 1"

# 查看方言
sz-orm dialect list
sz-orm dialect show postgres
```

### 代码证据

- `cli/src/main.rs:1-2787` — CLI 入口，12+ 子命令
- `cli/src/phantom1_wiring.rs` — 幻影交付接线验证

---

## 案例 3：多语言绑定（Python/Java/Go/C++）

### 概要

| 绑定 | 包 | FFI 技术 | 使用场景 |
|------|-----|---------|---------|
| Python | sz-orm-python | PyO3 0.20 | 数据分析、ETL 脚本 |
| Java | sz-orm-java | JNI 0.22 | 企业应用、Spring 集成 |
| Go | sz-orm-go | CGo | 微服务、云原生 |
| C++ | sz-orm-cpp | C ABI | 高性能计算、游戏 |
| C | sz-orm-cabi | C ABI | 嵌入式、系统编程 |
| JS/TS | sz-orm-js | napi-rs | Node.js 后端、全栈 |

### Python 绑定示例

```python
from sz_orm import PyPool, PyQuery

# 创建连接池
pool = PyPool("sqlite://./app.db", max_size=10)

# 参数化查询（防 SQL 注入）
results = PyQuery(pool).table("users").where_eq("status", "active").find_all()

# 事务
with pool.transaction() as tx:
    tx.table("users").where_eq("id", 1).update({"name": "updated"})
    tx.table("audit_log").insert({"action": "update", "target": "user:1"})
```

### 代码证据

| 绑定 | 入口文件 | 测试文件 |
|------|---------|---------|
| Python | `packages/sz-orm-python/src/lib.rs` | `packages/sz-orm-python/tests/` |
| Java | `packages/sz-orm-java/src/lib.rs` | `packages/sz-orm-java/tests/` |
| Go | `packages/sz-orm-go/src/lib.rs` | `packages/sz-orm-go/tests/` |
| C++ | `packages/sz-orm-cpp/src/lib.rs` | `packages/sz-orm-cpp/tests/` |
| C ABI | `packages/sz-orm-cabi/src/lib.rs` | `packages/sz-orm-cabi/tests/` |
| JS/TS | `packages/sz-orm-js/src/lib.rs` | `packages/sz-orm-js/tests/` |

---

## 案例汇总

| # | 案例 | 类型 | 包数 | 状态 |
|---|------|------|------|------|
| 1 | sz-pay | 生产系统 | 6 | ✅ 27 E2E 测试通过 |
| 2 | sz-orm-cli | 开发工具 | 2 | ✅ 12+ 命令可用 |
| 3 | 多语言绑定 | FFI 绑定 | 6 | ✅ 6 种语言覆盖 |

### 覆盖的 sz-orm 能力

- ✅ 连接池（自研 AtomicU32 + crossbeam-queue）
- ✅ 查询构建器（参数化查询，防 SQL 注入）
- ✅ 迁移管理（CLI + 代码）
- ✅ 图数据库（Neo4j 驱动 + HTTP API）
- ✅ 向量搜索（HNSW + HTTP API）
- ✅ 审计日志（HashChain）
- ✅ 密码学（AES/PBKDF2/HMAC）
- ✅ 数据脱敏（DataMasker）
- ✅ 认证授权（RBAC + TOTP）
- ✅ 多语言绑定（6 种语言）
- ✅ SQL 校验（防注入）
- ✅ 31 种方言支持