# sz-orm Go/Java/C++ 绑定事务与模型级 API 扩充编码任务分解

> 任务编号：TASK-002
> 对应需求规格：`docs/spec/bindings_tx_model/spec.md`（REQ-BND-001 ~ REQ-BND-018）
> 对应技术设计：`docs/spec/bindings_tx_model/design.md`
> 版本基线：v4.9.0
> 日期：2026-08-19
> 目标：为 Go/Java/C++ 三个绑定补充事务 API（begin/commit/rollback）和模型级 API（insert/update/delete/find），每个绑定附 E2E 测试

---

## 1. CABI 层模型级导出实现

### 1.1 标识符校验函数
- [ ] 在 `packages/sz-orm-cabi/src/lib.rs` 新增 `fn validate_identifier(name: &str) -> Result<(), SzOrmErrorCode>`，正则白名单 `^[A-Za-z_][A-Za-z0-9_]*$`，拒绝含 `'`/`;`/`--`/空格的输入（REQ-BND-011）
- [ ] 单元测试：合法标识符通过；`"users; DROP--"` / `"ta'ble"` / `"a b"` 返回 InvalidArgument
- **依赖**：无
- **验证方法**：`cargo test -p sz-orm-cabi validate_identifier`；负向测试覆盖 SQL 注入向量
- **预估工作量**：0.5h

### 1.2 JSON 参数编解码
- [ ] 在 `packages/sz-orm-cabi/src/lib.rs` 新增 `fn parse_fields_json(json: &str) -> Result<Vec<String>, SzOrmErrorCode>` 和 `fn parse_values_json(json: &str) -> Result<Vec<JsonValue>, SzOrmErrorCode>`，使用 serde_json（REQ-BND-007）
- [ ] 单元测试：合法 JSON 解析成功；非法 JSON 返回 InvalidArgument
- **依赖**：无
- **验证方法**：`cargo test -p sz-orm-cabi parse_fields_json`
- **预估工作量**：0.5h

### 1.3 sz_orm_model_insert 导出实现
- [ ] 在 `packages/sz-orm-cabi/src/lib.rs` 新增 `#[no_mangle] unsafe extern "C" fn sz_orm_model_insert(handle, table, fields_json, values_json) -> QueryResultC`（REQ-BND-007）
- [ ] 核心逻辑：catch_unwind 包裹 → 校验 handle 非空 → validate_identifier(table) → parse_fields_json → 逐字段 validate_identifier → parse_values_json → 复用 sz-orm-core QueryBuilder 构建 `INSERT INTO <table> (<fields>) VALUES (?, ...)` → 参数化执行
- [ ] handle 可为 pool_handle 或 tx_handle（统一处理，事务内走事务连接）
- [ ] 异常映射：非法表名/字段名 → InvalidArgument；JSON 解析失败 → InvalidArgument；执行失败 → QueryFailed；panic → Panic
- **依赖**：1.1, 1.2
- **验证方法**：`grep "sz_orm_model_insert" packages/sz-orm-cabi/src/lib.rs` 命中；`cargo test -p sz-orm-cabi model_insert`
- **预估工作量**：1.5h

### 1.4 sz_orm_model_update 导出实现
- [ ] 新增 `#[no_mangle] unsafe extern "C" fn sz_orm_model_update(handle, table, set_json, where_clause, where_params_json) -> QueryResultC`（REQ-BND-007）
- [ ] 核心逻辑：校验标识符 → 解析 set_json（字段名+值）→ 构建 `UPDATE <table> SET <field>=?, ... WHERE <where_clause>` → 参数化执行
- [ ] where 条件无匹配 → rows_affected=0（非错误）
- **依赖**：1.1, 1.2
- **验证方法**：`grep "sz_orm_model_update" packages/sz-orm-cabi/src/lib.rs` 命中；`cargo test -p sz-orm-cabi model_update`
- **预估工作量**：1h

### 1.5 sz_orm_model_delete 导出实现
- [ ] 新增 `#[no_mangle] unsafe extern "C" fn sz_orm_model_delete(handle, table, where_clause, where_params_json) -> QueryResultC`（REQ-BND-007）
- [ ] 核心逻辑：校验标识符 → 构建 `DELETE FROM <table> WHERE <where_clause>` → 参数化执行
- **依赖**：1.1, 1.2
- **验证方法**：`grep "sz_orm_model_delete" packages/sz-orm-cabi/src/lib.rs` 命中；`cargo test -p sz-orm-cabi model_delete`
- **预估工作量**：0.5h

### 1.6 sz_orm_model_find 导出实现
- [ ] 新增 `#[no_mangle] unsafe extern "C" fn sz_orm_model_find(handle, table, where_clause, where_params_json) -> *mut c_char`（REQ-BND-007）
- [ ] 核心逻辑：校验标识符 → 构建 `SELECT * FROM <table> WHERE <where_clause>` → 参数化查询 → 结果集转 JSON 行数组字符串（调用方用 sz_orm_string_free 释放）
- [ ] 无匹配行 → 返回空数组 `[]`（非 null）
- **依赖**：1.1, 1.2
- **验证方法**：`grep "sz_orm_model_find" packages/sz-orm-cabi/src/lib.rs` 命中；`cargo test -p sz-orm-cabi model_find`
- **预估工作量**：1h

### 1.7 CABI 层单元测试
- [ ] 在 `packages/sz-orm-cabi/tests/` 新增模型级 API 单元测试：insert→find 往返、update→find 生效、delete→find 返回空（REQ-BND-008/009/010）
- [ ] 负向测试：非法表名/字段名返回 InvalidArgument（REQ-BND-011）；无效句柄返回 InvalidArgument
- [ ] 事务内模型操作测试：begin→model_insert(tx)→rollback→find 返回空（REQ-BND-014）
- [ ] 参数化查询验证：grep 源码无 format!/push_str 拼接 SQL 值（REQ-BND-012）
- **依赖**：1.3, 1.4, 1.5, 1.6
- **验证方法**：`cargo test -p sz-orm-cabi` 全通过；`grep -rn "format!\|push_str" packages/sz-orm-cabi/src/lib.rs` 无 SQL 值拼接
- **预估工作量**：2h

---

## 2. Go 绑定事务与模型 API 实现

### 2.1 Go 事务 API 转发
- [ ] 在 `packages/sz-orm-go/src/lib.rs` 新增 `sz_orm_go_transaction_begin/commit/rollback` 转发至 sz-orm-cabi（REQ-BND-006）
- [ ] 在 `packages/sz-orm-go/go/szorm/` 新增 `Tx` struct + `BeginTx() (*Tx, error)` / `Commit() error` / `Rollback() error` 方法（REQ-BND-006）
- [ ] Tx 析构（defer）调用 rollback（若仍活跃，REQ-BND-004）
- **依赖**：1.7
- **验证方法**：`grep "BeginTx\|Commit\|Rollback" packages/sz-orm-go/go/szorm/*.go` 命中
- **预估工作量**：1h

### 2.2 Go 模型 API 转发
- [ ] 在 `packages/sz-orm-go/src/lib.rs` 新增 `sz_orm_go_model_insert/update/delete/find` 转发至 sz-orm-cabi（REQ-BND-013）
- [ ] 在 `packages/sz-orm-go/go/szorm/` 新增 `ModelInsert(table, fields, values) (int64, error)` / `ModelUpdate` / `ModelDelete` / `ModelFind` 方法（REQ-BND-013）
- [ ] fields/values 序列化为 JSON 传递至 CABI
- **依赖**：1.7
- **验证方法**：`grep "ModelInsert\|ModelUpdate\|ModelDelete\|ModelFind" packages/sz-orm-go/go/szorm/*.go` 命中
- **预估工作量**：1.5h

### 2.3 Go E2E 测试扩展
- [ ] 扩展 `packages/sz-orm-go/go/szorm/szorm_test.go` 至 ≥ 10 步（REQ-BND-016）
- [ ] 新增事务 E2E：建表→BeginTx→ModelInsert(tx)→Commit→ModelFind 返回数据；BeginTx→ModelInsert(tx)→Rollback→ModelFind 返回空
- [ ] 新增模型 CRUD E2E：ModelInsert→ModelFind→ModelUpdate→ModelFind→ModelDelete→ModelFind 往返
- [ ] 测试使用 SQLite 内存库，测试后清理临时文件
- **依赖**：2.1, 2.2
- **验证方法**：`go test ./packages/sz-orm-go/go/szorm/` ≥ 10 步全通过；无残留临时数据库文件
- **预估工作量**：2h

---

## 3. Java 绑定事务与模型 API 实现

### 3.1 Java 事务 JNI 入口
- [ ] 在 `packages/sz-orm-java/src/lib.rs` 新增 `Java_sz_1orm_1java_SzOrmPool_beginTransaction/commit/rollback` JNI 入口，转发至 sz-orm-cabi（REQ-BND-006）
- [ ] JNI 入口用 EnvUnowned.with_env 包裹，错误通过 ThrowRuntimeEx 抛出
- [ ] 句柄为 jlong（0 表示失败）
- **依赖**：1.7
- **验证方法**：`grep "beginTransaction\|commit\|rollback" packages/sz-orm-java/src/lib.rs` 命中
- **预估工作量**：1h

### 3.2 Java 模型 JNI 入口
- [ ] 在 `packages/sz-orm-java/src/lib.rs` 新增 `Java_sz_1orm_1java_SzOrmPool_modelInsert/Update/Delete/Find` JNI 入口（REQ-BND-013）
- [ ] 参数：表名/字段 JSON/值 JSON/where 条件，返回 QueryResult 或 JSON 字符串
- **依赖**：1.7
- **验证方法**：`grep "modelInsert\|modelUpdate\|modelDelete\|modelFind" packages/sz-orm-java/src/lib.rs` 命中
- **预估工作量**：1.5h

### 3.3 Java wrapper 类扩展
- [ ] 在 `packages/sz-orm-java/java-test/sz_orm_java/` 新增/扩展 `SzOrmPool.java`：`beginTransaction() / commit() / rollback() / modelInsert() / modelUpdate() / modelDelete() / modelFind()` 方法（REQ-BND-006/013）
- [ ] 新增 `SzOrmTx` 类封装事务句柄，finalize() 调用 rollback（若仍活跃，REQ-BND-004）
- **依赖**：3.1, 3.2
- **验证方法**：`grep "beginTransaction\|modelInsert" packages/sz-orm-java/java-test/sz_orm_java/*.java` 命中
- **预估工作量**：1h

### 3.4 Java E2E 测试扩展
- [ ] 扩展 `packages/sz-orm-java/java-test/sz_orm_java/SzOrmPoolTest.java` 至 ≥ 12 步（既有 7 步 + 事务 3 步 + 模型 CRUD 2 步，REQ-BND-015）
- [ ] 新增事务 E2E：beginTransaction→modelInsert(tx)→commit→modelFind 返回数据；beginTransaction→modelInsert(tx)→rollback→modelFind 返回空
- [ ] 新增模型 CRUD E2E：modelInsert→modelFind→modelUpdate→modelFind→modelDelete→modelFind 往返
- [ ] 测试使用 SQLite 内存库，测试后清理
- **依赖**：3.3
- **验证方法**：`javac + java SzOrmPoolTest` ≥ 12 步全通过；无残留临时文件
- **预估工作量**：2h

---

## 4. C++ 绑定事务与模型 API 实现

### 4.1 C++ 事务 extern C 导出
- [ ] 在 `packages/sz-orm-cpp/src/lib.rs` 新增 `#[no_mangle] extern "C" fn sz_orm_cpp_transaction_begin/commit/rollback` 转发至 sz-orm-cabi（REQ-BND-006）
- [ ] 签名：`void* sz_orm_cpp_transaction_begin(void* poolHandle)` / `int sz_orm_cpp_transaction_commit(void* txHandle)` / `int sz_orm_cpp_transaction_rollback(void* txHandle)`
- **依赖**：1.7
- **验证方法**：`grep "sz_orm_cpp_transaction" packages/sz-orm-cpp/src/lib.rs` 命中
- **预估工作量**：0.5h

### 4.2 C++ 模型 extern C 导出
- [ ] 在 `packages/sz-orm-cpp/src/lib.rs` 新增 `sz_orm_cpp_model_insert/update/delete/find` 转发至 sz-orm-cabi（REQ-BND-013）
- **依赖**：1.7
- **验证方法**：`grep "sz_orm_cpp_model" packages/sz-orm-cpp/src/lib.rs` 命中
- **预估工作量**：0.5h

### 4.3 szorm.h 头文件扩展
- [ ] 在 `packages/sz-orm-cpp/cpp/szorm.h` 新增 `Transaction` RAII 类：构造调用 transaction_begin，析构调用 rollback（若仍活跃，REQ-BND-004）
- [ ] 新增 `Pool` 类方法：`modelInsert/modelUpdate/modelDelete/modelFind`（REQ-BND-013）
- [ ] 移动语义（禁止拷贝），与既有 Pool RAII 一致
- **依赖**：4.1, 4.2
- **验证方法**：`grep "class Transaction\|modelInsert" packages/sz-orm-cpp/cpp/szorm.h` 命中
- **预估工作量**：1h

### 4.4 C++ E2E 测试新增
- [ ] 在 `packages/sz-orm-cpp/cpp/` 新增 C++ E2E 测试文件（如 `e2e_test.cpp`），含：建表/插入/查询/事务提交/事务回滚/模型 CRUD 往返（REQ-BND-017）
- [ ] 编译命令：`g++ -I. e2e_test.cpp -L. -lsz_orm_cpp -o e2e_test && ./e2e_test`
- [ ] 测试使用 SQLite 内存库，测试后清理
- [ ] 若系统无 g++ 则标记"需 g++ 环境"跳过（REQ-BND-017 异常场景）
- **依赖**：4.3
- **验证方法**：`g++` 编译成功 + 执行 E2E 全通过；无残留临时文件
- **预估工作量**：2h

---

## 5. 既有测试不回退验证

### 5.1 既有测试回归
- [ ] 执行 `cargo test -p sz-orm-cabi` 确认既有 22 测试全通过（REQ-BND-018）
- [ ] 执行 `cargo test -p sz-orm-go` 确认既有 8 测试全通过（REQ-BND-018）
- [ ] 执行 `cargo test -p sz-orm-java` 确认既有测试全通过（REQ-BND-018）
- [ ] 执行 `cargo test -p sz-orm-cpp` 确认既有 7 测试全通过（REQ-BND-018）
- **依赖**：2.3, 3.4, 4.4
- **验证方法**：cargo test 各包退出码 0；测试计数 ≥ 既有数
- **预估工作量**：1h

### 5.2 既有 API 签名不变验证
- [ ] `git diff` 确认 pool_new/ping/query/execute/version 签名未变更（DFX 4.5.1）
- [ ] 确认 sz-orm-cabi 既有事务导出（sz_orm_transaction_begin/commit/rollback/execute/free）签名不变
- **依赖**：5.1
- **验证方法**：`git diff` 仅含新增函数，无既有函数签名修改
- **预估工作量**：0.5h

---

## 6. 安全与参数化验证

### 6.1 SQL 注入防护验证
- [ ] 负向测试：三绑定各自调用 model_insert("users; DROP--", ...) 返回错误（REQ-BND-011）
- [ ] 负向测试：字段名含 `'` 或 `;` 返回 InvalidArgument
- **依赖**：2.3, 3.4, 4.4
- **验证方法**：负向测试用例全通过；错误码 = InvalidArgument
- **预估工作量**：0.5h

### 6.2 参数化查询验证
- [ ] grep 全部新增源码，确认无 format!/push_str 拼接 SQL 值（REQ-BND-012）
- [ ] 确认所有 SQL 值通过绑定参数传递
- **依赖**：1.7
- **验证方法**：`grep -rn "format!\|push_str" packages/sz-orm-cabi/src/lib.rs packages/sz-orm-go/src/ packages/sz-orm-java/src/ packages/sz-orm-cpp/src/` 无 SQL 值拼接
- **预估工作量**：0.3h

---

## 7. 交付记录与文档

### 7.1 生成交付记录
- [ ] 生成 `docs/spec/bindings_tx_model/delivery-record.md`，含：新增 API 清单（CABI 4 + Go 7 + Java 7 + C++ 7 = 25 个）、E2E 测试结果（Java ≥12 / Go ≥10 / C++ 全通过）、三绑定验证证据（file:line）、既有测试不回退证据（REQ-BND-018）
- **依赖**：5.1, 6.1, 6.2
- **验证方法**：delivery-record.md 存在且内容完整；含 file:line 证据
- **预估工作量**：0.5h

### 7.2 临时文件清理验证
- [ ] 扫描 E2E 测试产生的临时 SQLite 文件，确认全部删除（session rules）
- [ ] 扫描 C++ E2E 编译产物（e2e_test 可执行文件），确认清理
- **依赖**：5.1
- **验证方法**：`ls packages/sz-orm-go/go/szorm/*.tmp` 无残留；`ls packages/sz-orm-cpp/cpp/e2e_test` 不存在
- **预估工作量**：0.2h

---

## 8. 审查与确认

### 8.1 五维审查
- [ ] 正确性：三绑定事务提交/回滚行为一致；模型 CRUD 往返正确
- [ ] 可读性：新增代码精简，无冗余（session rules）
- [ ] 架构：复用 sz-orm-core QueryBuilder，未重复实现 SQL 生成
- [ ] 安全性：标识符校验 + 参数化查询 + panic 不跨 FFI 边界
- [ ] 性能：FFI 调用 < 1ms；事务往返 < 10ms（SQLite 内存库）
- **依赖**：7.1, 7.2
- **验证方法**：审查清单逐项确认，附 file:line 证据
- **预估工作量**：0.5h

### 8.2 变更范围确认
- [ ] 确认仅修改 sz-orm-cabi/go/java/cpp 包 + szorm.h + Java wrapper + E2E 测试文件
- [ ] 确认未新增 workspace 成员
- [ ] 确认未修改 sz-orm-core 源码（仅复用 QueryBuilder）
- **依赖**：8.1
- **验证方法**：`git diff --name-only` 仅含上述文件
- **预估工作量**：0.2h

---

## 任务依赖关系

```
1.1 → 1.3 → 1.7 → 2.1 → 2.3 → 5.1 → 5.2 → 6.1 → 7.1 → 8.1 → 8.2
1.2 → 1.3
1.1 → 1.4 → 1.7
1.1 → 1.5 → 1.7
1.1 → 1.6 → 1.7
1.7 → 2.2 → 2.3
1.7 → 3.1 → 3.3 → 3.4 → 5.1
1.7 → 3.2 → 3.3
1.7 → 4.1 → 4.3 → 4.4 → 5.1
1.7 → 4.2 → 4.3
1.7 → 6.2
5.1 → 7.2
```

## 任务统计

- 主任务：8 组
- 子任务：24 个
- 需求覆盖：REQ-BND-001 ~ REQ-BND-018 全部 18 项
- 预估总工作量：约 24h
