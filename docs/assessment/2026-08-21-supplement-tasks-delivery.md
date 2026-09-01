# sz-orm 补充任务交付记录

**日期**：2026-08-21
**版本**：5.0.0
**任务范围**：3 个补充任务（白帽测试 + 性能测试 + sz-pay 接线）
**依据**：spec.md（22 条 EARS 需求）+ design.md（1079 行）+ tasks.md（645 行）

---

## 任务 1：白帽安全测试交付

### 文件清单
- 新增：`packages/sz-orm-core/tests/whitehat_security_validation.rs`

### file:line 证据
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:1]` 模块文档注释（白帽测试视角说明）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:15]` Order 模型定义
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:22]` Order 实现 Model trait（含 tenant_field）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:37]` mysql_builder 辅助函数
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:46]` test_whitehat_parameterized_query_effective（WH-02）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:67]` test_whitehat_type_safe_column_accepts_registered（WH-03）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:80]` test_whitehat_type_safe_column_rejects_unregistered（WH-03）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:97]` test_whitehat_tenant_boundary_isolated（WH-04）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:113]` test_whitehat_tenant_boundary_without_tenant（WH-04）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:127]` test_whitehat_input_validation_rejects_invalid（WH-05）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:150]` test_whitehat_default_config_safe（WH-06）
- `[packages/sz-orm-core/tests/whitehat_security_validation.rs:185]` test_whitehat_boundary_extreme_inputs（WH-07）

### cargo test 输出
```
running 8 tests
test test_whitehat_tenant_boundary_without_tenant ... ok
test test_whitehat_boundary_extreme_inputs ... ok
test test_whitehat_input_validation_rejects_invalid ... ok
test test_whitehat_tenant_boundary_isolated ... ok
test test_whitehat_type_safe_column_accepts_registered ... ok
test test_whitehat_type_safe_column_rejects_unregistered ... ok
test test_whitehat_default_config_safe ... ok
test test_whitehat_parameterized_query_effective ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 验证命令
```bash
cargo test -p sz-orm-core --test whitehat_security_validation
# 结果：8 passed; 0 failed
```

---

## 任务 2：图数据库性能测试交付

### 文件清单
- 新增：`packages/sz-orm-graph/tests/in_memory_performance.rs`

### file:line 证据
- `[packages/sz-orm-graph/tests/in_memory_performance.rs:1]` 模块文档注释（基于 InMemoryGraphEngine，不依赖 Neo4j）
- `[packages/sz-orm-graph/tests/in_memory_performance.rs:10]` 常量定义（NODE_COUNT=1000, QUERY_COUNT=100, P95_LIMIT_MS=500）
- `[packages/sz-orm-graph/tests/in_memory_performance.rs:23]` test_in_memory_1000_node_query_p95（PERF-03/04）
- `[packages/sz-orm-graph/tests/in_memory_performance.rs:60]` test_in_memory_query_count_increments（PERF-04）
- `[packages/sz-orm-graph/tests/in_memory_performance.rs:75]` test_in_memory_real_node_returned（PERF-04）

### cargo test 输出
```
running 3 tests
test test_in_memory_query_count_increments ... ok
test test_in_memory_real_node_returned ... ok
P95 延迟: 0ms (限制: 500ms)
test test_in_memory_1000_node_query_p95 ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 验证命令
```bash
cargo test -p sz-orm-graph --test in_memory_performance -- --nocapture
# 结果：3 passed; 0 failed; P95=0ms ≤ 500ms
```

### 资源清理
- InMemoryGraphEngine 为栈分配，测试结束自动释放
- 无临时文件、无进程残留

---

## 任务 3：sz-pay 生产接线交付

### 文件清单
- 修改：`E:\vue\test\sz-pay\server\sz-rust\Cargo.toml`（新增 sz-orm-graph path 依赖 + graph feature + v50-all 追加 graph）
- 新增：`E:\vue\test\sz-pay\server\sz-rust\src\services\graph_service.rs`（graph 查询服务，3 个函数）
- 修改：`E:\vue\test\sz-pay\server\sz-rust\src\services\mod.rs`（新增 graph_service 模块声明）
- 新增：`E:\vue\test\sz-pay\server\sz-rust\tests\graph_wiring_e2e.rs`（端到端接线测试，2 个测试）

### file:line 证据
- `[E:\vue\test\sz-pay\server\sz-rust\Cargo.toml:30]` sz-orm-graph path 依赖（optional = true）
- `[E:\vue\test\sz-pay\server\sz-rust\Cargo.toml:155]` graph = ["dep:sz-orm-graph"] feature 定义
- `[E:\vue\test\sz-pay\server\sz-rust\Cargo.toml:164]` v50-all 包含 "graph"
- `[E:\vue\test\sz-pay\server\sz-rust\src\services\graph_service.rs:1]` 模块文档注释
- `[E:\vue\test\sz-pay\server\sz-rust\src\services\graph_service.rs:14]` OnceLock 全局引擎
- `[E:\vue\test\sz-pay\server\sz-rust\src\services\graph_service.rs:27]` add_person_node 函数（真实调用 engine.add_node）
- `[E:\vue\test\sz-pay\server\sz-rust\src\services\graph_service.rs:38]` query_person 函数（真实调用 engine.execute）
- `[E:\vue\test\sz-pay\server\sz-rust\src\services\graph_service.rs:44]` get_query_count 函数（真实调用 engine.query_count）
- `[E:\vue\test\sz-pay\server\sz-rust\src\services\mod.rs:107]` graph_service 模块声明
- `[E:\vue\test\sz-pay\server\sz-rust\tests\graph_wiring_e2e.rs:1]` 端到端测试文档注释
- `[E:\vue\test\sz-pay\server\sz-rust\tests\graph_wiring_e2e.rs:14]` test_graph_wiring_query_count_increments
- `[E:\vue\test\sz-pay\server\sz-rust\tests\graph_wiring_e2e.rs:25]` test_graph_wiring_real_node_returned

### cargo test 输出
```
running 2 tests
test test_graph_wiring_real_node_returned ... ok
test test_graph_wiring_query_count_increments ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 验证命令
```bash
cargo check -p sz-pay-server                    # 默认编译通过（graph feature 关闭）
cargo check -p sz-pay-server --features graph   # graph feature 启用后编译通过
cargo test -p sz-pay-server --test graph_wiring_e2e --features graph
# 结果：2 passed; 0 failed
```

### 真实调用验证
- `graph_service.rs:27` `engine().write()` → `eng.add_node(node)` — 真实调用 InMemoryGraphEngine::add_node
- `graph_service.rs:39` `engine().read()` → `eng.execute(query)` — 真实调用 InMemoryGraphEngine::execute
- `graph_service.rs:46` `engine().read()` → `eng.query_count()` — 真实调用 InMemoryGraphEngine::query_count

---

## 门禁验证结果表

| 门禁 | 结果 | 验证命令 |
|------|------|---------|
| fmt 格式检查 | ✅ 通过 | `cargo fmt --all -- --check` |
| sz-orm-graph clippy | ✅ 通过 | `cargo clippy -p sz-orm-graph --tests -- -D warnings` |
| 白帽测试 | ✅ 8 passed | `cargo test -p sz-orm-core --test whitehat_security_validation` |
| 性能测试 | ✅ 3 passed, P95=0ms | `cargo test -p sz-orm-graph --test in_memory_performance` |
| sz-pay 默认编译 | ✅ 通过 | `cargo check -p sz-pay-server` |
| sz-pay graph 编译 | ✅ 通过 | `cargo check -p sz-pay-server --features graph` |
| sz-pay E2E 测试 | ✅ 2 passed | `cargo test -p sz-pay-server --test graph_wiring_e2e --features graph` |
| 占位实现扫描 | ✅ 无 | 无 todo!/unimplemented!/unreachable! |
| crate 级 dead_code | ✅ 无 | 无 #![allow(dead_code)] |

---

## 已知限制

1. **本机无 Docker**：性能测试基于 InMemoryGraphEngine（不依赖外部 Neo4j），原有 `performance.rs` 和 `neo4j_integration.rs` 的 `#[ignore]` 测试保留
2. **sz-orm-graph 未发布到 crates.io 5.0.0**：sz-pay 使用 path 依赖（`../../../鲜视达/rust/sz-orm/packages/sz-orm-graph`），不通过 crates.io
3. **graph feature 默认关闭**：sz-pay 的 graph feature 默认不启用，不影响现有编译路径和测试
4. **graph_service 使用独立全局引擎**：不通过 sz-orm-core 的 graph_adapter（crates.io 上的 sz-orm-core 5.0.0 无 graph feature），直接使用 sz-orm-graph 的 InMemoryGraphEngine

---

## 文件变更清单

### 新增文件

| 任务 | 文件路径 | 说明 |
|------|---------|------|
| T1 | `packages/sz-orm-core/tests/whitehat_security_validation.rs` | 白帽测试，8 个测试 |
| T2 | `packages/sz-orm-graph/tests/in_memory_performance.rs` | 性能测试，3 个测试 |
| T3 | `E:\vue\test\sz-pay\server\sz-rust\src\services\graph_service.rs` | graph 查询服务，3 个函数 |
| T3 | `E:\vue\test\sz-pay\server\sz-rust\tests\graph_wiring_e2e.rs` | 端到端接线测试，2 个测试 |
| T4 | `docs/assessment/2026-08-21-supplement-tasks-delivery.md` | 本交付记录 |

### 修改文件

| 任务 | 文件路径 | 修改内容 |
|------|---------|---------|
| T3 | `E:\vue\test\sz-pay\server\sz-rust\Cargo.toml` | 新增 sz-orm-graph path 依赖 + graph feature + v50-all 追加 graph |
| T3 | `E:\vue\test\sz-pay\server\sz-rust\src\services\mod.rs` | 新增 `#[cfg(feature = "graph")] pub mod graph_service;` |

### 保留文件

| 任务 | 文件路径 | 保留原因 |
|------|---------|---------|
| T2 | `packages/sz-orm-graph/tests/performance.rs` | 保留原有 #[ignore] + #[cfg(feature = "integration")] 测试 |
| T2 | `packages/sz-orm-graph/tests/neo4j_integration.rs` | 保留原有 6 个 #[ignore] 测试 |