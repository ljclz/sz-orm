# ADR-0003: 事务嵌套用 SAVEPOINT + 深度限制

- **状态**: Accepted
- **日期**: 2026-07-19
- **相关代码**: `packages/sz-orm-core/src/transaction.rs` (L47-L55, L91-L108)
- **修复编号**: H-8

## 背景

嵌套事务通过 `SAVEPOINT` 实现。如果没有深度限制，递归调用可无限创建保存点，导致：
1. 数据库保存点栈溢出
2. 连接资源被长时间占用
3. 潜在的 DoS 风险

此外，保存点名称直接拼入 SQL（`SAVEPOINT <name>`），如果不校验名称，存在注入风险。

## 决策

1. 在 `TransactOptions` 中加入 `max_nesting_depth: u32`，默认 8
2. 每次 `savepoint()` 时检查当前深度，超过限制返回 `TxError`
3. 对保存点名称执行 `validate_savepoint_name()` 白名单校验

```rust
pub const DEFAULT_MAX_NESTING_DEPTH: u32 = 8;

fn validate_savepoint_name(name: &str) -> Result<(), TxError> {
    // 非空，只能包含 ASCII 字母/数字/下划线，不能以数字开头
}
```

## 后果

**正面：**
- 防止递归事务耗尽数据库资源
- 保存点名称注入风险消除

**负面：**
- 深度 8 对绝大多数业务场景足够，但极少数深层递归场景需调高 `with_max_nesting_depth()`
- 深度限制是软限制（应用层），数据库自身可能有更严格的硬限制

**注意事项：**
- 设为 0 表示禁用嵌套事务（首次 `savepoint()` 即报错）
- 保存点名称由 `savepoint_counter` 自动生成（`sp_1`, `sp_2`, ...），用户通常不直接指定
