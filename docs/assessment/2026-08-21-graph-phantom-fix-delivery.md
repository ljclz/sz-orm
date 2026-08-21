# 图数据库幻影交付修复 — 交付记录

**日期**：2026-08-21  
**版本**：v5.0.0  
**状态**：✅ 已完成  
**审计标准**：所有结论附 file:line 证据 + 测试验证输出

---

## 1. 问题背景

sz-orm-graph 包含 ~3,057 行代码和 7 个测试，但存在幻影交付：

- `execute_query`（query.rs:126）返回 `Ok(vec![])` 空结果，未执行任何图查询
- `connect`（connection.rs:79）仅设置 `connected = true`，未建立真实连接
- 零生产调用：sz-orm-core 未接入 sz-orm-graph

## 2. 修复内容

### M1: 版本统一

- [packages/sz-orm-graph/Cargo.toml](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-graph/Cargo.toml)  
  `version = "0.1.0"` → `version.workspace = true`（继承 5.0.0）

### M2: InMemoryGraphEngine + Cypher 解析器

- [packages/sz-orm-graph/src/engine.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-graph/src/engine.rs) — 新增 InMemoryGraphEngine（~280 行）  
  - `add_node` / `add_relationship` / `execute` 真实实现  
  - `match_node` / `match_count` / `match_relationship` 内部方法  
  - `query_count` AtomicU64 统计查询次数  
  - 8 个单元测试

- [packages/sz-orm-graph/src/cypher_parser.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-graph/src/cypher_parser.rs) — 新增 CypherSubsetParser（~300 行）  
  - 支持 MATCH (n:Label) RETURN n  
  - 支持 MATCH (n:Label) WHERE n.prop = $param RETURN n  
  - 支持 MATCH (a:L1)-[r:RelType]->(b:L2) RETURN a, r, b  
  - 支持 MATCH (n:Label) RETURN count(n)  
  - 拒绝 CREATE/MERGE/DELETE/SET/SELECT  
  - 9 个单元测试

### M3: GraphConnection 扩展 + execute_query 重写

- [packages/sz-orm-graph/src/connection.rs:83](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-graph/src/connection.rs#L83)  
  - `connect` 重写：`memory://` → 初始化 InMemoryGraphEngine；`neo4j://`/`bolt://` → DriverError  
  - 新增 `engine` / `engine_mut` / `add_node` / `add_relationship` 方法  
  - `disconnect` 释放引擎

- [packages/sz-orm-graph/src/query.rs:116](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-graph/src/query.rs#L116)  
  - `execute_query` 重写：接入 CypherValidator + 引擎派发（替换 `Ok(vec![])` stub）

- [packages/sz-orm-graph/src/validator.rs:51](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-graph/src/validator.rs#L51)  
  - `check_parameterization` 扩展：支持检测双引号字符串字面量（原先仅检测单引号）

### M4: sz-orm-graph 端到端测试

- [packages/sz-orm-graph/tests/in_memory_e2e.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-graph/tests/in_memory_e2e.rs) — 17 个 E2E 测试  
  - 连接测试：memory:// 成功、neo4j://bolt:// 拒绝、无效 scheme 拒绝、空 DSN 拒绝  
  - 查询测试：真实返回节点、空结果真实执行、参数化校验、SQL 透传拒绝、未连接拒绝、空 Cypher 拒绝  
  - WHERE 参数查询、count 聚合、关系查询、数据一致性  
  - 连接池测试

### M5: sz-orm-core graph feature + graph_adapter

- [packages/sz-orm-core/Cargo.toml](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml)  
  - 新增 `graph = ["dep:sz-orm-graph"]` feature  
  - 新增 `sz-orm-graph` optional 依赖  
  - 新增 `[[test]] graph_adapter_e2e` 登记

- [packages/sz-orm-core/src/lib.rs:501](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L501)  
  - 新增 `#[cfg(feature = "graph")] pub mod graph_adapter;`

- [packages/sz-orm-core/src/graph_adapter.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/graph_adapter.rs) — 新增适配层  
  - 全局引擎：`OnceLock<parking_lot::RwLock<InMemoryGraphEngine>>`  
  - `graph_query`：读锁 + engine.execute  
  - `graph_add_node` / `graph_add_relationship`：写锁 + engine.add_*  
  - `graph_query_count`：查询计数（测试验证用）  
  - 2 个单元测试

- [packages/sz-orm-core/tests/graph_adapter_e2e.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/graph_adapter_e2e.rs) — 4 个 E2E 测试  
  - 添加节点 + 查询验证  
  - WHERE 参数查询  
  - 关系查询  
  - count 聚合

## 3. 门禁验证结果

| # | 门禁 | 结果 |
|---|------|------|
| 1 | `cargo fmt --all -- --check` | ✅ 通过 |
| 2 | `cargo check -p sz-orm-core` | ✅ 通过 |
| 3 | `cargo clippy -p sz-orm-graph -- -D warnings` | ✅ 通过 |
| 3 | `cargo clippy -p sz-orm-core --features graph -- -D warnings` | ✅ 通过 |
| 4 | `cargo test -p sz-orm-graph` | ✅ 151 lib + 17 e2e 通过 |
| 4 | `cargo test -p sz-orm-core --features graph --lib` | ✅ 1826 通过 |
| 4 | `cargo test -p sz-orm-core --features graph --test graph_adapter_e2e` | ✅ 4 通过 |
| 4 | `cargo test -p sz-orm-core --lib`（默认 feature 不退化） | ✅ 1824 通过 |
| 8 | `grep 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-graph/src` | ✅ 无输出 |
| 8 | `grep 'Ok(vec!\[\])' packages/sz-orm-graph/src/query.rs` | ✅ 无输出 |

## 4. 生产可达性证据

### 4.1 sz-orm-graph 内部真实执行

- `InMemoryGraphEngine::execute`（engine.rs:61）真实执行 Cypher 查询，返回 `Vec<GraphResult>`
- `InMemoryGraphEngine::query_count`（engine.rs:55）AtomicU64 递增，证明查询真实执行
- `execute_query`（query.rs:116）调用 `engine.execute(query)`，不再返回 `Ok(vec![])`

### 4.2 sz-orm-core 生产入口

- `graph_adapter::graph_query`（graph_adapter.rs:29）：全局引擎读锁 + execute
- `graph_adapter::graph_add_node`（graph_adapter.rs:37）：全局引擎写锁 + add_node
- `graph_adapter::graph_add_relationship`（graph_adapter.rs:45）：全局引擎写锁 + add_relationship
- 启用方式：`cargo build --features graph`

### 4.3 端到端验证

```
sz-orm-core --features graph --test graph_adapter_e2e:
  test_graph_adapter_add_node_and_query ... ok
  test_graph_adapter_where_param_query ... ok
  test_graph_adapter_relationship_query ... ok
  test_graph_adapter_count_aggregation ... ok
  test result: ok. 4 passed; 0 failed
```

## 5. 修改文件清单

| 文件 | 操作 |
|------|------|
| packages/sz-orm-graph/Cargo.toml | 修改（版本统一） |
| packages/sz-orm-graph/src/lib.rs | 修改（新增模块导出 + execute_query re-export） |
| packages/sz-orm-graph/src/engine.rs | 新增 |
| packages/sz-orm-graph/src/cypher_parser.rs | 新增 |
| packages/sz-orm-graph/src/connection.rs | 修改（engine 字段 + connect 重写 + 便捷方法） |
| packages/sz-orm-graph/src/query.rs | 修改（execute_query 重写） |
| packages/sz-orm-graph/src/validator.rs | 修改（双引号字面量检测） |
| packages/sz-orm-graph/tests/in_memory_e2e.rs | 新增 |
| packages/sz-orm-core/Cargo.toml | 修改（graph feature + 依赖 + test 登记） |
| packages/sz-orm-core/src/lib.rs | 修改（graph_adapter 模块声明） |
| packages/sz-orm-core/src/graph_adapter.rs | 新增 |
| packages/sz-orm-core/tests/graph_adapter_e2e.rs | 新增 |