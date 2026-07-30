# API 稳定性与废弃策略

> 版本：v1.2.0 · 生效日期：2026-07-29 · 适用范围：SZ-ORM 全部公共 API

## 1. 语义化版本 (SemVer) 承诺

SZ-ORM 严格遵循 [Semantic Versioning 2.0.0](https://semver.org/lang/zh-CN/)：

| 版本号变更 | 触发条件 | 向后兼容 |
|-----------|---------|---------|
| **MAJOR** (x.0.0) | 破坏性 API 变更（删除/重命名公共类型、修改函数签名、改变行为语义） | ❌ 不兼容 |
| **MINOR** (1.x.0) | 新增功能、新增类型、新增 trait 方法（有默认实现） | ✅ 兼容 |
| **PATCH** (1.0.x) | Bug 修复、性能优化、文档改进 | ✅ 兼容 |

### 1.1 什么是"公共 API"

以下被视为公共 API，受稳定性承诺约束：

- 所有 `pub` 结构体、枚举、trait、函数、常量、类型别名
- 所有 `pub` 宏（`macro_rules!` 和 proc-macro）
- Cargo.toml 中 `[features]` 定义的特性开关
- 公共类型的 `derive` 宏派生行为（`Debug`、`Clone`、`Serialize` 等）

以下**不**受稳定性承诺约束：

- `pub(crate)` 和私有项
- `#[doc(hidden)]` 标注的内部 API
- 测试代码（`tests/`、`benches/` 目录）
- `examples/` 目录中的示例代码

## 2. API 稳定性分层

SZ-ORM 将公共 API 按稳定性分为三层：

### Tier 1 — 稳定 (Stable)

- **承诺**：在当前 MAJOR 版本内不引入破坏性变更
- **变更条件**：仅在 MAJOR 版本升级时可能变更
- **覆盖范围**：
  - `sz-orm-core`：`Model` trait、`QueryBuilder`、`Value`、`DbError`、`Pool`、`PoolConfig`、`Connection` trait（已有方法）
  - `sz-orm-sqlx`：`SqlitePoolHandle`、`MySqlPoolHandle`、`PgPoolHandle` 及对应 `ConnectionFactory`
  - `sz-orm-macros`：`typed_query!`、`schema!`、`sql_string!`
  - CLI 命令行接口（`sz-orm-cli`）

### Tier 2 — 实验性 (Experimental)

- **承诺**：可能在 MINOR 版本中变更，但不会在不通知的情况下删除
- **变更条件**：MINOR 版本升级时可能调整签名或行为，会提前在 CHANGELOG 中标注
- **覆盖范围**：
  - `sz-orm-dtx`：分布式事务 API（TCC/Saga 模式仍在演进）
  - `sz-orm-ai`：NL2SQL API（依赖 LLM 能力，接口可能调整）
  - `sz-orm-sharding`：分库分表路由 API
  - `sz-orm-search`：全文检索 API
  - `Connection` trait 的新增方法（`execute_with_params`、`query_with_params` 等）
  - 各扩展包中标注 `#[doc = "Experimental"]` 的 API

### Tier 3 — 内部不稳定 (Internal)

- **承诺**：无稳定性承诺，可能在任何版本中变更或删除
- **覆盖范围**：
  - 所有标注 `#[doc(hidden)]` 的项
  - 标注 `// INTERNAL` 或 `// DO NOT USE` 的代码
  - 编译器内部辅助类型和 trait

## 3. 废弃策略

### 3.1 废弃流程

当一个公共 API 需要被废弃时，遵循以下流程：

```
[Deprecation Notice] → [保留 N 个 MINOR 版本] → [MAJOR 版本移除]
```

### 3.2 具体规则

1. **废弃通知**：在 API 上添加 `#[deprecated]` 属性，并标注替代方案：
   ```rust
   #[deprecated(
       since = "1.3.0",
       note = "使用 `new_method` 替代，此方法将在 2.0.0 中移除"
   )]
   pub fn old_method(&self) -> Result<()> { ... }
   ```

2. **保留期**：废弃的 API 至少保留 **2 个 MINOR 版本**。例如：
   - 在 `1.3.0` 中废弃 → 在 `1.5.0` 中仍可用 → 在 `2.0.0` 中移除

3. **文档标注**：CHANGELOG 中以 `**DEPRECATED**` 前缀记录每次废弃

4. **编译警告**：使用废弃 API 会产生 `deprecated` 编译警告（非错误），建议用户尽快迁移

### 3.3 废弃例外

以下情况可以跳过保留期，在下一个 PATCH 版本中直接移除：

- 安全漏洞修复（CVE 相关）
- 数据正确性问题（可能导致数据损坏）
- 编译器要求（Rust edition 升级导致的不兼容）

## 4. 破坏性变更处理

### 4.1 MAJOR 版本升级条件

只有在以下情况才会升级 MAJOR 版本：

1. 删除或重命名 Tier 1 稳定 API
2. 修改 Tier 1 API 的函数签名（参数类型、返回类型、参数顺序）
3. 改变 Tier 1 API 的行为语义（如事务隔离级别默认值变更）
4. 修改 `Value` 枚举的变体（新增变体是 MINOR，删除/重命名是 MAJOR）
5. 修改 `Model` trait 的必要方法签名

### 4.2 升级指南

每次 MAJOR 版本升级时，将提供：

1. **迁移指南**：`docs/migration-vN-to-v(N+1).md`
2. **变更清单**：CHANGELOG 中列出所有破坏性变更
3. **自动化迁移工具**（如可能）：CLI 子命令辅助迁移

## 5. 版本支持策略

| 版本类型 | 支持周期 | 安全补丁 |
|---------|---------|---------|
| 最新 MAJOR 版本 | 完整支持 | ✅ |
| 前一个 MAJOR 版本 | 过渡期 6 个月 | ✅ 仅安全补丁 |
| 更早的 MAJOR 版本 | 不支持 | ❌ |

## 6. Cargo feature 稳定性

- 已发布的 `[features]` 开关视为 Tier 1 稳定 API
- 新增 feature 是 MINOR 变更
- 删除 feature 是 MAJOR 变更（需先经过废弃流程）
- feature 的默认启用状态变更需要至少 1 个 MINOR 版本的过渡期

## 7. 变更记录

| 日期 | 版本 | 变更内容 |
|------|------|---------|
| 2026-07-28 | v1.0.0 | 首次发布 API 稳定性承诺 |
| 2026-07-29 | v1.2.0 | 版本号同步至工作空间当前版本；内容不变 |
