# sz-orm v7.2.0 发布说明

**发布日期**: 2026-09-16
**版本**: 7.2.0
**上一版本**: 7.1.0

## 四大方向交付摘要

### 1. 真实 DB 性能基准（任务组 1-3）

替代 v6.9.0 的模拟延迟模型，实现真实 DB 查询基准测试：

- **DbBackend 枚举**: SQLite/MySQL 双后端支持
- **BenchError 错误类型**: 5 变体（DbConnectFailed/QueryFailed/InvalidConnectionString/ProductionDatabaseRejected/IncomparableWorkload）
- **BenchConfig 扩展**: db_backend + db_connection 字段，serde default 兼容旧 JSON
- **BenchResult 扩展**: is_real_db 字段标注真实 DB 基准
- **real_db.rs 模块**: DatasetInitializer + SzOrmWorkload + SqlxWorkload + SeaOrmWorkload + RealDbExecutor + run_workload_real
- **CLI 参数扩展**: --db-backend/--db-connection/--simulate/--dataset-size
- **real-bench feature gate**: 启用真实 DB 基准（引入 sz-orm-core/sz-orm-sqlx/sqlx/sea-orm/tokio）
- **测试**: 22 单元测试 + 5 SQLite 集成测试 + 3 可复现性测试

使用示例:
```bash
# SQLite 真实 DB 基准
cargo run -p sz-orm-bench --features real-bench -- --db-backend sqlite --db-connection "sqlite:///tmp/test.db?mode=rwc" --measure-rounds 10

# MySQL 真实 DB 基准
cargo run -p sz-orm-bench --features real-bench -- --db-backend mysql --db-connection "mysql://root:test123@127.0.0.1:3306/sz_orm_test"

# 模拟路径（回归对比）
cargo run -p sz-orm-bench -- --simulate
```

### 2. deprecated API 清理（任务组 4-5）

评估 8 处 deprecated API 的下游迁移状态：

| API | 替代 | sz-pay | new-wxapp | 决策 |
|-----|------|--------|-----------|------|
| InMemoryAlertHook::url() | identifier() | 未迁移 | 未迁移 | 保留 |
| ModelEvaluator::evaluate() | evaluate_with_executor() | 未迁移 | 未迁移 | 保留 |
| Query (struct) | QueryBuilder<M> | 未迁移 | 未迁移 | 保留 |
| Query::select/insert/update/delete | QueryBuilder<M>::* | 未迁移 | 未迁移 | 保留 |
| MemoryFusionCache | TtlFusionCache | 已迁移 | 已迁移 | 保留（内部测试使用） |

**移除数量**: 0（全部保留，下游仍在使用或内部测试依赖）

### 3. 绑定层 API 覆盖率审计（任务组 6-7）

| 绑定 | 导出符号数 | 核心 API 数 | 覆盖率 |
|------|-----------|------------|--------|
| cabi | 36 | 2139 | 1.68% |
| java | 30 | 2139 | 1.40% |
| python | 2 | 2139 | 0.09% |
| go | 34 | 2139 | 1.59% |
| cpp | 34 | 2139 | 1.59% |
| js | 76 | 2139 | 3.55% |
| wasm | 2 | 2139 | 0.09% |

注: 核心 API 2139 个含大量内部 pub 符号，绑定层职责是用户面向 API 非全量导出。补齐需后续迭代。

### 4. 统一 API 文档站（任务组 8-9）

- **build-doc-site.py**: 聚合 70 包 rustdoc HTML + 跨包搜索索引 + 统一导航 + 敏感信息扫描
- **verify-doc-site.py**: 验证文档站完整性

使用示例:
```bash
cargo doc --workspace --no-deps
python scripts/build-doc-site.py --rustdoc-html target/doc --version 7.2.0 --git-commit $(git rev-parse HEAD) --output target/doc-site
python scripts/verify-doc-site.py --doc-site target/doc-site --expected-crates 70 --expected-version 7.2.0
```

## 新增脚本

| 脚本 | 用途 |
|------|------|
| scripts/check-deprecated-downstream.py | 扫描下游项目 deprecated API 调用 |
| scripts/check-allow-deprecated-dangling.py | 检测悬空 #[allow(deprecated)] 标注 |
| scripts/check-binding-coverage.py | 绑定层 API 覆盖率审计 |
| scripts/build-doc-site.py | 构建统一 API 文档站 |
| scripts/verify-doc-site.py | 验证文档站完整性 |

## 新增报告

| 报告 | 内容 |
|------|------|
| docs/deprecated-removal-report-v7.2.0.json | 8 API 下游迁移状态 |
| docs/binding-coverage-report-v7.2.0.json | 7 绑定覆盖率审计 |
| scripts/deprecated_apis.json | deprecated API 清单基线 |

## 门禁验证

- ✅ fmt 格式检查通过
- ✅ check 编译通过（默认 + real-bench feature）
- ✅ clippy 零警告（默认 + real-bench feature + all-targets）
- ✅ test 22 单元测试 + 5 SQLite 集成测试 + 3 可复现性测试通过
- ✅ 禁止占位实现检查通过
- ✅ 幻影交付检查通过（run_workload_real 有 CLI 调用点 main.rs:96）
- ✅ SQL 拼接检查通过（real_db.rs 无 SQL 拼接）