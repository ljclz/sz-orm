# sz-orm v5.0.0 迁移指南

> 从 v4.9.1 升级到 v5.0.0
> 日期：2026-08-20

## Breaking Change 总览

| # | 变更 | 影响范围 | 严重度 |
|---|------|----------|--------|
| 1 | `build_select()` 返回类型 `String` → `(String, Vec<Value>)` | 编译期 | 高 |
| 2 | `build_insert()` 返回类型 `String` → `(String, Vec<Value>)` | 编译期 | 高 |
| 3 | `build_update()` 返回类型 `String` → `(String, Vec<Value>)` | 编译期 | 高 |
| 4 | `build_delete()` 返回类型 `String` → `(String, Vec<Value>)` | 编译期 | 高 |
| 5 | Feature `perf-zero-copy-l2` 移除，合并到 `zero-copy` | 编译期 | 中 |
| 6 | Feature `process-l1-cache` 移除，合并到 `l1-cache` | 编译期 | 中 |
| 7 | Feature `cache-warmup-protection` 移除，合并到 `l1-cache` | 编译期 | 中 |
| 8 | 性能 feature 默认启用（simd/l1-cache/plan-cache/zero-copy） | 编译时间 | 低 |
| 9 | `where_cond` 文档残留清理 | 无 | 低 |
| 10 | 内部依赖版本号统一到 5.0.0 | 编译期 | 低 |
| 11 | workspace.dependencies 版本号更新 | 编译期 | 低 |

## 1. build_* 返回类型变更

### 旧 API（v4.9.1）

```rust
let sql: String = query.build_select();
let sql: String = query.build_insert(&data);
let sql: String = query.build_update(&data);
let sql: String = query.build_delete();
```

### 新 API（v5.0.0）

```rust
let (sql, params): (String, Vec<Value>) = query.build_select();
let (sql, params): (String, Vec<Value>) = query.build_insert(&data);
let (sql, params): (String, Vec<Value>) = query.build_update(&data);
let (sql, params): (String, Vec<Value>) = query.build_delete();
```

### 迁移步骤

1. 将 `let sql = query.build_select()` 改为 `let (sql, params) = query.build_select()`
2. 如需纯 SQL 字符串（参数内联渲染），使用 `query.sql()` / `query.sql_insert(&data)` / `query.sql_update(&data)` / `query.sql_delete()`
3. `sql_*` 方法标记为 `#[deprecated]`，仅供日志/调试使用

### 自动化工具

```bash
# 扫描需迁移的调用点
cargo run -p sz-orm-cli -- migrate lint --check-build-select

# 自动修复
cargo run -p sz-orm-cli -- migrate fix --build-select
```

## 2. Feature Gate 变更

### 移除的 Feature

| 旧 Feature | 替代 Feature | 迁移方式 |
|------------|-------------|----------|
| `perf-zero-copy-l2` | `zero-copy` | Cargo.toml 中替换 |
| `process-l1-cache` | `l1-cache` | Cargo.toml 中替换 |
| `cache-warmup-protection` | `l1-cache` | Cargo.toml 中替换 |

### 迁移步骤

```toml
# 旧
sz-orm-core = { version = "4.9.1", features = ["cache-warmup-protection"] }

# 新
sz-orm-core = { version = "5.0.0", features = ["l1-cache"] }
```

### 默认启用的 Feature

v5.0.0 默认启用 `simd` / `l1-cache` / `plan-cache` / `zero-copy`，无需手动指定。

如需禁用（不推荐）：

```toml
sz-orm-core = { version = "5.0.0", default-features = false, features = ["redis"] }
```

## 3. sz-pay 升级示例

```toml
# 旧（v4.9.1）
sz-orm-core = "4.9.1"

# 新（v5.0.0）
sz-orm-core = "5.0.0"
```

```bash
cargo update -p sz-orm-core
cargo update -p sz-orm-macros
cargo test --workspace
```

## 4. FAQ

### Q: build_select() 为什么改为返回元组？

A: v4.9.1 有 `build_select()` 返回纯 SQL 和 `build_select_with_params()` 返回 `(String, Vec<Value>)` 两套 API。v5.0.0 统一为参数化查询，杜绝 SQL 注入风险。纯 SQL 需求用 `sql()` 方法。

### Q: 性能 feature 默认启用会影响编译时间吗？

A: 会增加约 10-20% 编译时间。如需快速编译，可用 `default-features = false`。

### Q: sz-pay 需要改代码吗？

A: sz-pay 不直接调用 `build_*` API（通过 facade 层间接使用），只需更新 Cargo.toml 版本号。