# 自定义编译期诊断指南

> 版本：v3.7.0 | 稳定性：stable | feature gate：`custom-diagnostic`

## 1. 概述

`custom-diagnostic` feature 提供自定义编译期诊断信息，当 typed-dsl 类型约束失败时生成比 Rust 默认更清晰的错误信息，包含错误位置、期望类型、实际类型和修复建议。

## 2. 启用方式

```toml
[dependencies]
sz-orm-macros = { version = "3.7.0", features = ["custom-diagnostic"] }
```

或同时启用 `typed-dsl`（向后兼容）：

```toml
[dependencies]
sz-orm-macros = { version = "3.7.0", features = ["typed-dsl", "custom-diagnostic"] }
```

## 3. 诊断场景

| 场景 | 常量 | 说明 |
|------|------|------|
| Eq 类型不匹配 | `TYPE_MISMATCH_EQ` | 列的 RustType 与比较值类型不匹配 |
| 非 Boolean 逻辑 | `NON_BOOLEAN_LOGIC` | And/Or 操作数不是 Bool 类型 |
| 跨表列引用 | `CROSS_TABLE_REFERENCE` | 列不属于当前查询的表 |
| 无效类型转换 | `INVALID_CAST` | Cast 源类型不可转换为目标类型 |
| 非 JSON 列 | `NON_JSON_COLUMN` | JSON 操作符要求列为 Json 类型 |

## 4. 使用示例

### 4.1 diagnostic_error! 宏

```rust
diagnostic_error!("类型不匹配", "请使用 Cast 显式转换");
// 编译期输出：
// error: 类型不匹配
//   help: 请使用 Cast 显式转换
```

### 4.2 #[type_check] 属性宏

```rust
#[type_check]
fn my_query() {
    let expr = ColId.eq("hello"); // i64 列与 String 比较
    // 编译期将生成自定义诊断信息
}
```

## 5. 迁移指南

从 `typed-dsl` 到 `custom-diagnostic`：
1. 在 Cargo.toml 中添加 `custom-diagnostic` feature
2. 既有 `typed-dsl` 代码无需修改（向后兼容）
3. 可独立使用 `custom-diagnostic` 而不启用 `typed-dsl`

## 6. 稳定性

- **v3.6.0**：随 `typed-dsl` 引入，无独立 feature gate
- **v3.7.0**：stable，独立 `custom-diagnostic` feature gate，测试覆盖 ≥10 用例