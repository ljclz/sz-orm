# sz-orm v8.7.0 覆盖率采集报告

> 采集日期：2026-09-23 07:22:08 UTC
> 工具版本：cargo-llvm-cov 0.9.1
> 采集命令：cargo llvm-cov --workspace --exclude sz-orm-python --exclude sz-orm-java --exclude sz-orm-js --exclude sz-orm-cabi --exclude sz-orm-go --exclude sz-orm-cpp --ignore-run-fail --lcov --output-path lcov.info

---

## 覆盖率摘要

| 指标 | 值 |
|------|-----|
| 总行数 | 186258 |
| 覆盖行数 | 165009 |
| 行覆盖率 | 88.59% |
| 门限要求 | >= 60% |
| 门限结果 | PASS |

---

## 覆盖率最低的 50 个文件

| 文件 | 总行数 | 覆盖行数 | 覆盖率 |
|------|--------|----------|--------|
| cli/src/phantom1_wiring.rs | 223 | 0 | 0.00% |
| packages/sz-orm-ai-designer/src/ai_schema_designer.rs | 155 | 0 | 0.00% |
| packages/sz-orm-ai-migration/src/ai_migration_generator.rs | 42 | 0 | 0.00% |
| packages/sz-orm-auth/src/error.rs | 9 | 0 | 0.00% |
| packages/sz-orm-bench/src/main.rs | 153 | 0 | 0.00% |
| packages/sz-orm-graphql/src/resolver.rs | 8 | 0 | 0.00% |
| packages/sz-orm-lsp/src/main.rs | 36 | 0 | 0.00% |
| packages/sz-orm-postgis/src/error.rs | 18 | 0 | 0.00% |
| packages/sz-orm-search/src/error.rs | 19 | 0 | 0.00% |
| packages/sz-orm-storage/src/error.rs | 17 | 0 | 0.00% |
| packages/sz-orm-studio/src/main.rs | 31 | 0 | 0.00% |
| packages/sz-orm-timeseries/src/error.rs | 17 | 0 | 0.00% |
| packages/sz-orm-vector/src/error.rs | 18 | 0 | 0.00% |
| packages/sz-orm-websocket/src/error.rs | 14 | 0 | 0.00% |
| cli/src/main.rs | 1489 | 89 | 5.98% |
| packages/sz-orm-core/src/shadow.rs | 261 | 28 | 10.73% |
| packages/sz-orm-back/src/error.rs | 21 | 4 | 19.05% |
| packages/sz-orm-sqlx/src/any.rs | 1600 | 328 | 20.50% |
| packages/sz-orm-mig/src/error.rs | 16 | 4 | 25.00% |
| packages/sz-orm-mqtt/src/error.rs | 14 | 4 | 28.57% |
| packages/sz-orm-oracle/src/lib.rs | 1367 | 592 | 43.31% |
| packages/sz-orm-queue/src/error.rs | 11 | 5 | 45.45% |
| packages/sz-orm-mssql/src/lib.rs | 1174 | 565 | 48.13% |
| cli/src/entity_generator.rs | 199 | 100 | 50.25% |
| packages/sz-orm-studio/src/server.rs | 53 | 27 | 50.94% |
| packages/sz-orm-axum/src/validation.rs | 290 | 169 | 58.28% |
| packages/sz-orm-postgis/src/geometry.rs | 655 | 385 | 58.78% |
| packages/sz-orm-sqlx/src/unified_pool.rs | 147 | 87 | 59.18% |
| packages/sz-orm-sqlx/src/error.rs | 25 | 15 | 60.00% |
| packages/sz-orm-lsp/src/server.rs | 355 | 214 | 60.28% |
| packages/sz-orm-mig/src/transformer/mod.rs | 150 | 91 | 60.67% |
| packages/sz-orm-vector/src/lib.rs | 78 | 51 | 65.38% |
| packages/sz-orm-core/src/n1_eliminator.rs | 202 | 134 | 66.34% |
| packages/sz-orm-axum/src/lib.rs | 158 | 105 | 66.46% |
| packages/sz-orm-mig/src/migrator.rs | 460 | 307 | 66.74% |
| packages/sz-orm-core/src/nested_active_model.rs | 464 | 310 | 66.81% |
| packages/sz-orm-core/src/eager_loader.rs | 492 | 329 | 66.87% |
| packages/sz-orm-health/src/endpoint.rs | 315 | 212 | 67.30% |
| packages/sz-orm-core/src/schema_sync.rs | 712 | 485 | 68.12% |
| packages/sz-orm-core/src/value.rs | 596 | 410 | 68.79% |
| packages/sz-orm-core/src/smart_eager_loader.rs | 704 | 492 | 69.89% |
| packages/sz-orm-core/src/quick_query.rs | 321 | 227 | 70.72% |
| packages/sz-orm-core/src/cache.rs | 385 | 273 | 70.91% |
| packages/sz-orm-core/src/error.rs | 473 | 337 | 71.25% |
| packages/sz-orm-core/src/db_type.rs | 144 | 107 | 74.31% |
| packages/sz-orm-core/src/accessors.rs | 701 | 521 | 74.32% |
| packages/sz-orm-dtx/src/lib.rs | 580 | 433 | 74.66% |
| packages/sz-orm-core/src/l2_cache.rs | 1697 | 1273 | 75.01% |
| packages/sz-orm-actix/src/lib.rs | 437 | 328 | 75.06% |
| packages/sz-orm-core/src/tenant_context.rs | 251 | 189 | 75.30% |

---

## 排除包说明

以下 6 个绑定层包因服务器缺少对应语言运行时库而排除：
- sz-orm-python（需 Python 开发库）
- sz-orm-java（需 JNI）
- sz-orm-js（需 napi-rs）
- sz-orm-cabi（C ABI）
- sz-orm-go（需 Go FFI）
- sz-orm-cpp（需 C++ FFI）

---

> 本报告由 cargo-llvm-cov 实际执行生成，非占位文档。lcov.info 产物已存档。
rbase64: invalid input
