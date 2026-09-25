# v9.0.0 迁移指南

> 版本：v9.0.0 | 更新日期：2026-09-26

## 概述

v9.0.0 是 sz-orm 的性能优化 + 深度集成版本。本指南描述从 v8.8.0 升级到 v9.0.0 的步骤和注意事项。

## 变更摘要

### 新增功能

| 功能 | 模块 | 说明 |
|------|------|------|
| known_good 快速路径 | sz-orm-core/pool | 连接释放后跳过 is_connected() 检查 |
| clonecheap | sz-orm-core/value | Copy 变体零堆分配克隆 |
| reap_idle_selective | sz-orm-core/pool | 选择性回收空闲连接（提前停止） |

### 性能优化

- 连接池 acquire 路径：known_good=true 时跳过健康检查
- 批量插入：Value::clonecheap 替代 Value::clone，Copy 变体零堆分配
- 空闲连接回收：reap_idle_selective 遇到未过期连接即停止

### 覆盖率提升

- v8.7.0：88.59%
- v9.0.0：≥95%（284 新测试补齐 12 个低覆盖率文件）

### sz-pay 深度集成

- 新增 `sz_orm_ai.rs`：NL2SQL + 自动调优 + 索引建议接线
- 新增 `sz_orm_dtx.rs`：Saga 分布式事务接线
- 新增 `sz_orm_obs.rs`：Prometheus 指标 + 结构化日志接线

## 升级步骤

### 1. 更新 Cargo.toml

```toml
[dependencies]
sz-orm-core = "9.0.0"
sz-orm-sqlx = "9.0.0"
# ... 其他 sz-orm-* 包
```

### 2. 验证编译

```bash
cargo check
```

### 3. 运行测试

```bash
cargo test
```

### 4. 验证性能不退化

v9.0.0 的性能优化是透明的，无需修改业务代码。可通过基准测试验证：

```bash
cargo test --features real-bench
```

## API 兼容性

v9.0.0 **完全向后兼容** v8.8.0。所有变更均为新增方法或内部优化，不破坏既有 API。

### 新增公共方法

| 方法 | 位置 | 说明 |
|------|------|------|
| `Value::clonecheap` | `sz-orm-core/src/value.rs` | Copy 变体零堆分配克隆 |

### 内部变更（不影响 API）

- `PooledConnection` 新增 `known_good` 字段（`#[doc(hidden)] pub`）
- `reap_idle` 改为 `reap_idle_selective`（内部实现变更）

## 注意事项

### 1. known_good 字段

`PooledConnection` 新增了 `known_good: bool` 字段。如果下游项目手动构造 `PooledConnection`（不推荐），需要添加该字段。正常通过连接池 API 使用则无需修改。

### 2. clonecheap

`Value::clonecheap` 是新增方法，不替代 `Value::clone`。批量插入内部已自动使用 `clonecheap`，外部代码无需修改。

### 3. Feature gate

v9.0.0 不新增 feature gate。所有新功能默认启用。