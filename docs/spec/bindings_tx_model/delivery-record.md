# TASK-002 交付记录：Go/Java/C++ 绑定事务与模型级 API 扩充

> 任务编号：TASK-002
> 对应需求规格：`docs/spec/bindings_tx_model/spec.md`（REQ-BND-001 ~ REQ-BND-018）
> 对应技术设计：`docs/spec/bindings_tx_model/design.md`
> 版本基线：v4.9.0
> 交付日期：2026-08-19
> 执行者：Rust 代码开发子智能体

---

## 1. 变更文件清单

| 文件 | 变更类型 | 新增行数 | 说明 |
|------|---------|---------|------|
| `packages/sz-orm-cabi/src/lib.rs` | 扩展 | +1257 | 新增 8 个模型级 C ABI 导出 + 16 个单元测试 |
| `packages/sz-orm-go/src/lib.rs` | 扩展 | +535 | 新增 13 个 Go 转发函数 + 5 个单元测试 |
| `packages/sz-orm-java/src/lib.rs` | 扩展 | +707 | 新增 12 个 JNI 入口 + 5 个单元测试 |
| `packages/sz-orm-cpp/src/lib.rs` | 扩展 | +530 | 新增 13 个 extern C 转发 + 5 个单元测试 |

**变更范围确认**：仅修改 sz-orm-cabi/go/java/cpp 四个绑定包源码，未新增 workspace 成员，未修改 sz-orm-core 源码（仅复用 `Pool`/`PooledConnection`/`Value`/`Connection::execute_with_params`/`query_with_params`）。

---

## 2. 新增 API 清单

### 2.1 CABI 层（8 个导出函数）

| API | 签名 | 代码位置 | 用途 |
|-----|------|---------|------|
| `sz_orm_model_insert` | `(handle, table, fields_json, values_json) -> QueryResultC` | `packages/sz-orm-cabi/src/lib.rs:1042` | 在 pool 上插入行 |
| `sz_orm_model_update` | `(handle, table, set_json, where_clause, where_params_json) -> QueryResultC` | `packages/sz-orm-cabi/src/lib.rs:1107` | 在 pool 上更新行 |
| `sz_orm_model_delete` | `(handle, table, where_clause, where_params_json) -> QueryResultC` | `packages/sz-orm-cabi/src/lib.rs:1182` | 在 pool 上删除行 |
| `sz_orm_model_find` | `(handle, table, where_clause, where_params_json) -> *mut c_char` | `packages/sz-orm-cabi/src/lib.rs:1252` | 在 pool 上查询行（返回 JSON） |
| `sz_orm_model_insert_tx` | `(tx_handle, table, fields_json, values_json) -> QueryResultC` | `packages/sz-orm-cabi/src/lib.rs:1318` | 在事务内插入行 |
| `sz_orm_model_update_tx` | `(tx_handle, table, set_json, where_clause, where_params_json) -> QueryResultC` | `packages/sz-orm-cabi/src/lib.rs:1383` | 在事务内更新行 |
| `sz_orm_model_delete_tx` | `(tx_handle, table, where_clause, where_params_json) -> QueryResultC` | `packages/sz-orm-cabi/src/lib.rs:1461` | 在事务内删除行 |
| `sz_orm_model_find_tx` | `(tx_handle, table, where_clause, where_params_json) -> *mut c_char` | `packages/sz-orm-cabi/src/lib.rs:1533` | 在事务内查询行 |

**辅助函数**（内部，非导出）：
- `validate_identifier` (`lib.rs:955`)：标识符校验，正则白名单 `^[A-Za-z_][A-Za-z0-9_]*$`
- `json_to_value` (`lib.rs:971`)：`serde_json::Value` → `sz_orm_core::Value` 转换
- `parse_fields_json` / `parse_values_json` / `parse_set_json`：JSON 参数编解码
- `build_insert_sql` / `build_update_sql` / `build_delete_sql` / `build_select_sql`：参数化 SQL 构建

### 2.2 Go 绑定（13 个转发函数）

| API | 代码位置 | 转发目标 |
|-----|---------|---------|
| `sz_orm_go_transaction_begin` | `packages/sz-orm-go/src/lib.rs:171` | `sz_orm_cabi::sz_orm_transaction_begin` |
| `sz_orm_go_transaction_execute` | `packages/sz-orm-go/src/lib.rs:185` | `sz_orm_cabi::sz_orm_transaction_execute` |
| `sz_orm_go_transaction_commit` | `packages/sz-orm-go/src/lib.rs:203` | `sz_orm_cabi::sz_orm_transaction_commit` |
| `sz_orm_go_transaction_rollback` | `packages/sz-orm-go/src/lib.rs:216` | `sz_orm_cabi::sz_orm_transaction_rollback` |
| `sz_orm_go_transaction_free` | `packages/sz-orm-go/src/lib.rs:229` | `sz_orm_cabi::sz_orm_transaction_free` |
| `sz_orm_go_model_insert` | `packages/sz-orm-go/src/lib.rs:246` | `sz_orm_cabi::sz_orm_model_insert` |
| `sz_orm_go_model_update` | `packages/sz-orm-go/src/lib.rs:267` | `sz_orm_cabi::sz_orm_model_update` |
| `sz_orm_go_model_delete` | `packages/sz-orm-go/src/lib.rs:290` | `sz_orm_cabi::sz_orm_model_delete` |
| `sz_orm_go_model_find` | `packages/sz-orm-go/src/lib.rs:312` | `sz_orm_cabi::sz_orm_model_find` |
| `sz_orm_go_model_insert_tx` | `packages/sz-orm-go/src/lib.rs:331` | `sz_orm_cabi::sz_orm_model_insert_tx` |
| `sz_orm_go_model_update_tx` | `packages/sz-orm-go/src/lib.rs:352` | `sz_orm_cabi::sz_orm_model_update_tx` |
| `sz_orm_go_model_delete_tx` | `packages/sz-orm-go/src/lib.rs:375` | `sz_orm_cabi::sz_orm_model_delete_tx` |
| `sz_orm_go_model_find_tx` | `packages/sz-orm-go/src/lib.rs:397` | `sz_orm_cabi::sz_orm_model_find_tx` |

### 2.3 Java 绑定（12 个 JNI 入口）

| API | 代码位置 | 转发目标 |
|-----|---------|---------|
| `Java_sz_1orm_1java_SzOrmPool_beginTransaction` | `packages/sz-orm-java/src/lib.rs:189` | `sz_orm_cabi::sz_orm_transaction_begin` |
| `Java_sz_1orm_1java_SzOrmPool_commitTransaction` | `packages/sz-orm-java/src/lib.rs:210` | `sz_orm_cabi::sz_orm_transaction_commit` |
| `Java_sz_1orm_1java_SzOrmPool_rollbackTransaction` | `packages/sz-orm-java/src/lib.rs:231` | `sz_orm_cabi::sz_orm_transaction_rollback` |
| `Java_sz_1orm_1java_SzOrmPool_freeTransaction` | `packages/sz-orm-java/src/lib.rs:256` | `sz_orm_cabi::sz_orm_transaction_free` |
| `Java_sz_1orm_1java_SzOrmPool_modelInsert` | `packages/sz-orm-java/src/lib.rs:279` | `sz_orm_cabi::sz_orm_model_insert` |
| `Java_sz_1orm_1java_SzOrmPool_modelUpdate` | `packages/sz-orm-java/src/lib.rs:320` | `sz_orm_cabi::sz_orm_model_update` |
| `Java_sz_1orm_1java_SzOrmPool_modelDelete` | `packages/sz-orm-java/src/lib.rs:365` | `sz_orm_cabi::sz_orm_model_delete` |
| `Java_sz_1orm_1java_SzOrmPool_modelFind` | `packages/sz-orm-java/src/lib.rs:406` | `sz_orm_cabi::sz_orm_model_find` |
| `Java_sz_1orm_1java_SzOrmPool_modelInsertTx` | `packages/sz-orm-java/src/lib.rs:456` | `sz_orm_cabi::sz_orm_model_insert_tx` |
| `Java_sz_1orm_1java_SzOrmPool_modelUpdateTx` | `packages/sz-orm-java/src/lib.rs:497` | `sz_orm_cabi::sz_orm_model_update_tx` |
| `Java_sz_1orm_1java_SzOrmPool_modelDeleteTx` | `packages/sz-orm-java/src/lib.rs:542` | `sz_orm_cabi::sz_orm_model_delete_tx` |
| `Java_sz_1orm_1java_SzOrmPool_modelFindTx` | `packages/sz-orm-java/src/lib.rs:583` | `sz_orm_cabi::sz_orm_model_find_tx` |

### 2.4 C++ 绑定（13 个 extern C 转发）

| API | 代码位置 | 转发目标 |
|-----|---------|---------|
| `sz_orm_cpp_transaction_begin` | `packages/sz-orm-cpp/src/lib.rs:163` | `sz_orm_cabi::sz_orm_transaction_begin` |
| `sz_orm_cpp_transaction_execute` | `packages/sz-orm-cpp/src/lib.rs:176` | `sz_orm_cabi::sz_orm_transaction_execute` |
| `sz_orm_cpp_transaction_commit` | `packages/sz-orm-cpp/src/lib.rs:194` | `sz_orm_cabi::sz_orm_transaction_commit` |
| `sz_orm_cpp_transaction_rollback` | `packages/sz-orm-cpp/src/lib.rs:207` | `sz_orm_cabi::sz_orm_transaction_rollback` |
| `sz_orm_cpp_transaction_free` | `packages/sz-orm-cpp/src/lib.rs:220` | `sz_orm_cabi::sz_orm_transaction_free` |
| `sz_orm_cpp_model_insert` | `packages/sz-orm-cpp/src/lib.rs:237` | `sz_orm_cabi::sz_orm_model_insert` |
| `sz_orm_cpp_model_update` | `packages/sz-orm-cpp/src/lib.rs:258` | `sz_orm_cabi::sz_orm_model_update` |
| `sz_orm_cpp_model_delete` | `packages/sz-orm-cpp/src/lib.rs:281` | `sz_orm_cabi::sz_orm_model_delete` |
| `sz_orm_cpp_model_find` | `packages/sz-orm-cpp/src/lib.rs:303` | `sz_orm_cabi::sz_orm_model_find` |
| `sz_orm_cpp_model_insert_tx` | `packages/sz-orm-cpp/src/lib.rs:322` | `sz_orm_cabi::sz_orm_model_insert_tx` |
| `sz_orm_cpp_model_update_tx` | `packages/sz-orm-cpp/src/lib.rs:343` | `sz_orm_cabi::sz_orm_model_update_tx` |
| `sz_orm_cpp_model_delete_tx` | `packages/sz-orm-cpp/src/lib.rs:366` | `sz_orm_cabi::sz_orm_model_delete_tx` |
| `sz_orm_cpp_model_find_tx` | `packages/sz-orm-cpp/src/lib.rs:388` | `sz_orm_cabi::sz_orm_model_find_tx` |

**新增 API 总计**：CABI 8 + Go 13 + Java 12 + C++ 13 = 46 个

---

## 3. 测试结果

### 3.1 sz-orm-cabi（69 个测试通过）

```
cargo test -p sz-orm-cabi -j 2 --no-fail-fast
test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

既有 53 个测试全通过（无回退），新增 16 个测试：
- `test_validate_identifier_legal`：合法标识符校验
- `test_validate_identifier_illegal`：非法标识符校验（SQL 注入向量）
- `test_model_insert_then_find_roundtrip`：insert→find 往返
- `test_model_update_then_find`：update→find 生效
- `test_model_delete_then_find_empty`：delete→find 返回空
- `test_model_insert_illegal_table_returns_error`：非法表名返回 InvalidArgument
- `test_model_insert_illegal_field_returns_error`：非法字段名返回 InvalidArgument
- `test_model_insert_null_handle_returns_error`：null 句柄返回 InvalidArgument
- `test_model_insert_fields_values_mismatch_returns_error`：字段/值长度不匹配
- `test_model_find_no_match_returns_empty_array`：无匹配返回 `[]`
- `test_model_find_all_no_where_clause`：无 where 子句查询全部
- `test_model_insert_tx_rollback_then_find_empty`：事务内 insert→rollback→find 空
- `test_model_insert_tx_commit_then_find`：事务内 insert→commit→find 数据可见
- `test_model_update_tx_and_delete_tx`：事务内 update/delete
- `test_model_find_tx_in_transaction`：事务内 find 看到未提交数据
- `test_model_insert_tx_null_handle_returns_error`：事务内 null 句柄

### 3.2 sz-orm-go（17 个测试通过）

```
cargo test -p sz-orm-go -j 2 --no-fail-fast
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

既有 12 个测试全通过（无回退），新增 5 个测试：
- `test_go_model_insert_find_roundtrip`：Go insert→find 往返
- `test_go_model_update_delete_find`：Go update/delete/find CRUD
- `test_go_model_insert_illegal_table`：Go 非法表名返回错误
- `test_go_transaction_model_rollback`：Go 事务回滚
- `test_go_transaction_model_commit`：Go 事务提交

### 3.3 sz-orm-java（11 个测试通过）

```
cargo test -p sz-orm-java -j 2 --no-fail-fast
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

既有 6 个测试全通过（无回退），新增 5 个测试：
- `test_java_model_insert_find_roundtrip`：Java insert→find 往返
- `test_java_model_update_delete_find`：Java update/delete/find CRUD
- `test_java_model_insert_illegal_table`：Java 非法表名返回错误
- `test_java_transaction_model_rollback`：Java 事务回滚
- `test_java_transaction_model_commit`：Java 事务提交

### 3.4 sz-orm-cpp（16 个测试通过）

```
cargo test -p sz-orm-cpp -j 2 --no-fail-fast
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

既有 11 个测试全通过（无回退），新增 5 个测试：
- `test_cpp_model_insert_find_roundtrip`：C++ insert→find 往返
- `test_cpp_model_update_delete_find`：C++ update/delete/find CRUD
- `test_cpp_model_insert_illegal_table`：C++ 非法表名返回错误
- `test_cpp_transaction_model_rollback`：C++ 事务回滚
- `test_cpp_transaction_model_commit`：C++ 事务提交

---

## 4. 安全验证

### 4.1 SQL 注入防护（REQ-BND-011/012）

- **标识符校验**：`validate_identifier` (`packages/sz-orm-cabi/src/lib.rs:955`) 实现白名单 `^[A-Za-z_][A-Za-z0-9_]*$`，拒绝含 `'`/`;`/`--`/空格的输入
- **参数化查询**：所有 SQL 值通过 `Connection::execute_with_params` / `Connection::query_with_params` 参数绑定（`?` 占位符），无 SQL 值拼接
- **format! 用途确认**：新增代码中 `format!` 仅用于拼接表名（已校验）、字段名（已校验）和 `?` 占位符，不拼接 SQL 值
- **负向测试覆盖**：三绑定各自调用 `model_insert("users; DROP--", ...)` 返回 `InvalidArgument`（`test_*_model_insert_illegal_table`）

### 4.2 panic 不跨 FFI 边界

- 所有新增导出函数均用 `std::panic::catch_unwind` 包裹，panic 转换为 `SzOrmErrorCode::Panic`
- 证据：`packages/sz-orm-cabi/src/lib.rs:1059/1124/1196/1265/1335/1400/1472/1543`（catch_unwind 调用点）

### 4.3 禁止项验证

- 无 `todo!`/`unimplemented!`/`unreachable!`（grep 确认）
- 无 crate 级 `#![allow(dead_code)]`（grep 确认）
- 既有 API 签名不变（`git diff` 仅含新增函数，无既有函数签名修改）

---

## 5. 五维审查

| 维度 | 结论 | 证据 |
|------|------|------|
| 正确性 | 三绑定事务提交/回滚行为一致；模型 CRUD 往返正确 | `test_*_transaction_model_commit/rollback` 全通过 |
| 可读性 | 新增代码精简，无冗余；辅助函数职责单一 | `validate_identifier`/`json_to_value`/`build_*_sql` 分离 |
| 架构 | 复用 sz-orm-core `Connection::execute_with_params`/`query_with_params`，未重复实现 SQL 执行 | `packages/sz-orm-cabi/src/lib.rs:1071/1149/1216/1286` |
| 安全性 | 标识符校验 + 参数化查询 + panic 不跨 FFI 边界 | 见 §4 |
| 性能 | FFI 转发零开销；参数化查询复用 prepared statement | Go/Java/C++ 转发函数均为直接调用 cabi |

---

## 6. 需求覆盖

| 需求 ID | 描述 | 覆盖证据 |
|---------|------|---------|
| REQ-BND-006 | 三绑定事务 API 转发 | Go:171-229 / Java:189-256 / C++:163-220 |
| REQ-BND-007 | CABI 模型级导出 | CABI:1042-1533（8 个导出） |
| REQ-BND-008 | insert→find 往返 | `test_*_model_insert_find_roundtrip` |
| REQ-BND-009 | update→find 生效 | `test_*_model_update_delete_find` |
| REQ-BND-010 | delete→find 返回空 | `test_*_model_update_delete_find` |
| REQ-BND-011 | 非法表名/字段名返回错误 | `test_*_model_insert_illegal_table` |
| REQ-BND-012 | 参数化查询 | `execute_with_params`/`query_with_params` 调用 |
| REQ-BND-013 | 三绑定模型 API 转发 | Go:246-397 / Java:279-624 / C++:237-388 |
| REQ-BND-014 | 事务内模型操作 | `test_*_transaction_model_rollback/commit` |
| REQ-BND-018 | 既有测试不回退 | 既有 82 测试全通过（cabi 53 + go 12 + java 6 + cpp 11） |

---

## 7. 验证命令

```bash
# 编译环境
$env:RUST_MIN_STACK="134217728"
$env:CARGO_INCREMENTAL="0"
$env:PATH = "C:\Users\Administrator\.cargo\bin;" + $env:PATH

# 测试
cargo test -p sz-orm-cabi -j 2 --no-fail-fast   # 69 passed
cargo test -p sz-orm-go -j 2 --no-fail-fast     # 17 passed
cargo test -p sz-orm-java -j 2 --no-fail-fast   # 11 passed
cargo test -p sz-orm-cpp -j 2 --no-fail-fast    # 16 passed

# 格式检查
cargo fmt -p sz-orm-cabi -p sz-orm-go -p sz-orm-java -p sz-orm-cpp -- --check

# grep 确认导出函数
grep "fn sz_orm_model_" packages/sz-orm-cabi/src/lib.rs
grep "fn sz_orm_go_model_\|fn sz_orm_go_transaction_" packages/sz-orm-go/src/lib.rs
grep "fn Java_sz_1orm_1java_SzOrmPool_model\|beginTransaction" packages/sz-orm-java/src/lib.rs
grep "fn sz_orm_cpp_model_\|fn sz_orm_cpp_transaction_" packages/sz-orm-cpp/src/lib.rs
```