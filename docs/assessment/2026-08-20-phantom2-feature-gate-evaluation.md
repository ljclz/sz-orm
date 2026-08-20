# PHANTOM-2 Feature Gate 逐个评估报告

> 评估日期：2026-08-20
> 评估范围：sz-orm-core 全部 70 个 feature gate
> 评估方法：`scripts/eval_feature_gates.py`，扫描全部 .rs 文件中 `cfg(feature = "...")` 引用
> 代码证据：[Cargo.toml:18](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L18)

---

## 1. 评估总览

| 分类 | 数量 | 占比 | 说明 |
|------|------|------|------|
| ✅ 有 cfg 引用（生产接线） | **60** | 85.7% | `#[cfg(feature = "...")]` 在源码中有实际使用 |
| 🔵 组合别名（无需自身引用） | **3** | 4.3% | `default`、`performance`、`testing` — 组合其他 feature |
| ❌ 无引用（幻影 feature） | **7** | 10.0% | 定义了但源码中无任何 `cfg(feature = "...")` 使用 |
| **合计** | **70** | 100% | |

---

## 2. 7 个幻影 Feature 逐个评估

| # | Feature | 定义 | 评估 | 处置建议 |
|---|---------|------|------|----------|
| 1 | `arch-improvement` | `= []` | 规划占位，从未实现任何代码 | **移除** — 无代码、无依赖、无文档引用 |
| 2 | `doc-completion` | `= []` | 规划占位，从未实现任何代码 | **移除** — 同上 |
| 3 | `migration-guide` | `= []` | 规划占位，从未实现任何代码 | **移除** — 同上 |
| 4 | `test-coverage` | `= []` | 规划占位，从未实现任何代码 | **移除** — 同上 |
| 5 | `typed-column` | `= []` | 与 `type-safe-columns`（9 refs）重复，已废弃 | **移除** — 被 `type-safe-columns` 替代 |
| 6 | `prod-health-endpoint` | `= []`，被 `prod-ready` 引用 | `prod-ready` 组合中声明但无对应代码 gate | **保留** — `prod-ready` 组合的一部分，未来可能启用 |
| 7 | `prod-shutdown-timeout` | `= []`，被 `prod-ready` 引用 | 同上 | **保留** — 同上 |

### 处置汇总

- **移除 5 个**：`arch-improvement`、`doc-completion`、`migration-guide`、`test-coverage`、`typed-column`
- **保留 2 个**：`prod-health-endpoint`、`prod-shutdown-timeout`（`prod-ready` 组合成员，保留语义完整性）

---

## 3. 60 个有引用 Feature 分类

### 3.1 高频引用（≥10 refs）— 核心功能

| Feature | refs | 用途 |
|---------|------|------|
| `db-verify` | 52 | 编译期 SQL 验证 |
| `anomaly-detection` | 21 | 异常检测 |
| `owasp-pentest-suite` | 19 | OWASP 渗透测试 |
| `circuit-breaker` | 17 | 熔断器 |
| `tenant-quota-rls-enhanced` | 13 | 租户 RLS 增强 |
| `multi-tenant-enhanced` | 12 | 多租户增强 |
| `typed-dsl` | 12 | 类型化 DSL |
| `perf-box-str` | 11 | Box<str> 性能优化 |
| `zero-copy` | 11 | 零拷贝列式 |
| `rate-limit` | 11 | 限流 |
| `auto-prewarm` | 10 | 连接池预热 |
| `type-safe-columns` | 9 | 类型安全列 |
| `plan-cache` | 9 | 查询计划缓存 |
| `dialect-firebird` | 9 | Firebird 方言 |
| `dialect-informix` | 9 | Informix 方言 |
| `dialect-redshift` | 9 | Redshift 方言 |
| `dialect-saphana` | 9 | SAP HANA 方言 |
| `dialect-snowflake` | 9 | Snowflake 方言 |

### 3.2 中频引用（3~9 refs）— 扩展功能

| Feature | refs | 用途 |
|---------|------|------|
| `e2e-real-db` | 8 | 真实 DB 集成测试 |
| `prod-n1-tuning` | 8 | N+1 调优 |
| `dialect-cockroachdb` | 8 | CockroachDB 方言 |
| `dialect-yugabytedb` | 8 | YugabyteDB 方言 |
| `prod-metrics-acl` | 6 | 指标 ACL |
| `prod-rate-limit-tuning` | 6 | 限流调优 |
| `data-validation` | 5 | 数据验证 |
| `prod-jwt-key-rotation` | 5 | JWT 密钥轮换 |
| `l1-cache` | 4 | L1 缓存 |
| `perf-enum-dispatch` | 4 | enum dispatch 优化 |
| `redis` | 4 | Redis 支持 |
| `prod-redis-tls` | 4 | Redis TLS |
| `simd` | 3 | SIMD（已修复为 HashSet 优化） |
| `qb-migration-tool` | 3 | QueryBuilder 迁移工具 |
| `prod-circuit-tuning` | 3 | 熔断调优 |
| `n1-lint` | 3 | N+1 静态检测 |
| `sql-verify-proc` | 2 | SQL 验证过程 |

### 3.3 低频引用（1~2 refs）— 边缘功能

`benchmark-suite`、`cache-coherence`、`cache-warmup-protection`、`compile-governance`、`connection-level-tenant`、`data-seeding`、`dist-cache`、`forward-compat-sandbox`、`migration-branch`、`migration-dry-run`、`perf-smallstring`、`perf-zero-copy-l2`、`process-l1-cache`、`prod-config-masking`、`prod-dialect-security`、`prod-leak-detection`、`prod-log-level`、`prod-pool-tuning`、`prod-probe-endpoint`、`prod-ready`、`schema-diff-viz`、`streaming-export`、`typed-relation`、`validate-on-write`、`zero-downtime-rollback`

---

## 4. 3 个组合别名

| Feature | 组合内容 | 说明 |
|---------|----------|------|
| `default` | `[]` | 无默认启用的 feature |
| `testing` | `["tokio/full"]` | 测试用，启用 tokio 全功能 |
| `performance` | `["simd", "l1-cache", "plan-cache", "zero-copy"]` | 性能优化组合（已实测验证） |

---

## 5. 验证命令

```bash
# 运行 feature gate 评估脚本
python scripts/eval_feature_gates.py

# 验证特定 feature 是否有引用
grep -rn 'cfg(feature = "typed-column")' packages/ --include='*.rs'
```

---

## 6. 总结

sz-orm-core 的 70 个 feature gate 中：
- **60 个（85.7%）有生产代码引用** — 非幻影，已接线
- **3 个组合别名** — 无需自身引用，合理
- **5 个纯幻影** — `arch-improvement`、`doc-completion`、`migration-guide`、`test-coverage`、`typed-column`，建议移除
- **2 个 `prod-ready` 组合成员** — `prod-health-endpoint`、`prod-shutdown-timeout`，保留语义完整性

**PHANTOM-2 状态：7 个无引用 feature 中 5 个建议移除，2 个保留。**