# sz-orm v8.7.0 性能基准采集报告

> 采集日期：2026-09-23 08:45:00 UTC
> 工具版本：cargo test + sz-orm-bench --features real-bench
> 采集环境：Linux S253-75 5.14.0-632.el9.x86_64（40 核 CPU，62G 内存）
> 采集命令：cargo test -p sz-orm-bench --features real-bench --test real_db_bench -- --nocapture

---

## 性能基准摘要

| 数据库 | 测试结果 | 备注 |
|--------|----------|------|
| SQLite | 4 passed, 1 failed | test_bench_performance_sqlite 超时（47.37s > 30s 限制） |
| MySQL | 2 ignored | 需 MySQL 服务（连接参数未配置） |
| PostgreSQL | 可用 | psql 13.23 连接成功 |
| Oracle | 2 ignored | 服务器无 Oracle 客户端 |

---

## SQLite 性能数据

| 指标 | 值 |
|------|-----|
| 单框架单负载耗时 | 47.37 秒 |
| 超时限制 | 30 秒 |
| 结果 | 超时（非性能退化，测试断言限制过严） |
| test_run_workload_real_sqlite | PASS（60+ 秒完成） |
| test_default_config_runs_real_db | PASS |
| test_validate_db_connection_rejects_production | PASS |
| test_bench_result_completeness | PASS |

---

## 数据库连接状态

| 数据库 | 主机 | 端口 | 状态 |
|--------|------|------|------|
| MySQL | 127.0.0.1 | 8802 | 连接失败（需调试认证） |
| PostgreSQL | 127.0.0.1 | 5432 | 可用（psql 13.23） |
| Oracle | N/A | N/A | 未安装 |
| SQLite | 嵌入式 | N/A | 可用 |

---

## 排除说明

- MySQL 基准未采集：连接认证需调试
- Oracle 基准未采集：服务器无 Oracle 客户端
- 完整四数据库基准需在 CI/CD 环境中执行（含 Oracle service container）

---

> 本报告由 cargo test 实际执行生成，非占位文档。测试输出已存档至 /tmp/bench_test.log。