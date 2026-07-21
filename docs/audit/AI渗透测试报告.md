# SZ-ORM AI 渗透测试报告

> 测试日期：2026-07-21
> 测试方法：静态代码安全扫描（AI 辅助）
> 扫描范围：packages/ 全部 39 个 workspace 成员 (~160+ Rust 源文件)

---

## 一、总体统计

| 分类 | Critical | High | Medium | Low | 合计 |
|------|----------|------|--------|-----|------|
| SQL 注入 | 2 | 4 | 3 | 0 | 9 |
| unsafe | 0 | 0 | 0 | 0 | 0 |
| 整数溢出 | 0 | 0 | 0 | 3 | 3 |
| 资源泄漏 | 0 | 0 | 3 | 1 | 4 |
| 路径遍历 | 1 | 0 | 2 | 0 | 3 |
| Panic 路径 | 0 | 0 | 3 | 2 | 5 |
| 信息泄露 | 0 | 0 | 0 | 2 | 2 |
| **总计** | **3** | **4** | **11** | **8** | **26** |

---

## 二、积极发现

1. **项目完全使用 Safe Rust**，无任何生产 `unsafe` 代码。
2. `sql_safety.rs` 提供了完善的 `validate_identifier()`、`validate_fk_action()`、`validate_id_value()` 防御机制。
3. `Dialect::quote_checked()` 方法已定义，且有完整的单元测试覆盖。
4. `nl2sql.rs` 的 `validate_select_only()` 和 `validate_no_injection()` 提供了基础安全层。

---

## 三、Critical 问题

### C-1: dialect.rs quote() 全未使用 quote_checked()

- **文件**: packages/sz-orm-core/src/dialect.rs
- **影响范围**: 约 80+ 处 `self.quote()` 调用（build_create_table / build_alter_table / build_drop_table）
- **风险**: 若调用方传入不可信标识符（表名/列名），可执行 SQL 注入
- **修复建议**: 所有 DDL 方法改用 `self.quote_checked()` 替代 `self.quote()`
- **已有防护**: `quote_checked()` 已定义并有测试，但生产代码未使用

### C-2: local.rs 路径遍历

- **文件**: packages/sz-orm-storage/src/local.rs:17-19
- **方法**: `full_path()` — `PathBuf::from(&self.base_path).join(key)`
- **风险**: `key` 不受限制，可传入 `../../etc/passwd` 读写任意文件
- **修复建议**: 对 `key` 做路径规范化，拒绝 `..` 和路径分隔符

### C-3: dynamic_sql.rs ${name} 字符串插值

- **文件**: packages/sz-orm-core/src/dynamic_sql.rs:481-493
- **风险**: `${name}` 直接嵌入 SQL，`escape_sql_string()` 转义不完整
- **修复建议**: 废弃 `${name}` 或强制走白名单校验

---

## 四、High 问题

### H-1: QueryBuilder where_cond() / or_where() / having() 裸字符串

- **文件**: packages/sz-orm-core/src/query.rs:114-123, 183-186
- **风险**: 直接拼接用户输入到 SQL，不做转义
- **建议**: 提供参数化 API 或对裸字符串做转义

### H-2: QueryBuilder select() 无校验

- **文件**: packages/sz-orm-core/src/query.rs:91-93
- **风险**: 调用方可能误用 `select()` 替代 `select_quoted()`
- **建议**: 标记 `select()` 为 deprecated

### H-3: SimpleNl2SqlEngine 表/列名未引用

- **文件**: packages/sz-orm-ai/src/nl2sql.rs:607-710
- **风险**: NL 提取的表名列名直接 format! 拼接
- **建议**: 提取后先经 validate_identifier() 校验

### H-4: OpenAINl2SqlEngine LLM 生成 SQL 的安全依赖

- **文件**: packages/sz-orm-ai/src/nl2sql.rs:940-1023
- **风险**: LLM 输出不可控，黑名单易绕过
- **建议**: 增加 SQL parser AST 验证

---

## 五、优先修复建议

| 优先级 | 问题 | 工作量 |
|--------|------|--------|
| P0 | dialect.rs quote() → quote_checked() | ~80 处替换 |
| P0 | local.rs 路径遍历 | 1 个文件 |
| P1 | dynamic_sql.rs ${name} 插值校验 | 1 个函数 |
| P1 | query.rs where_cond/or_where/having 增强 | 3 个方法 |
| P2 | nl2sql.rs 表名列名校验 | 2 个引擎 |
| P2 | entity_graph/observer RwLock unwrap | 2 个文件 |
| P3 | 信息泄露 sanitize | 各模块微调 |
