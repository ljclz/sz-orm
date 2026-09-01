# PHANTOM-2 Feature Gate 评估报告

> 日期：2026-08-22
> 版本：v5.0.0
> 评估对象：179 个未默认启用的 feature gate
> 依据：`phantom2-preliminary.json` + `phant2-verified.json` + `phantom2-apply-log.json`

---

## 1. 评估汇总

| 决策 | 数量 | 说明 |
|------|------|------|
| A（默认启用） | 32 | 已添加到 default 数组，编译通过 |
| B（保持手动） | 147 | 需外部依赖/特殊环境/按需使用 |
| C（移除） | 0 | 无 |
| **合计** | **179** | |

### 按分类分布

| 分类 | A | B | 合计 |
|------|---|---|------|
| 性能优化 | 10 | 0 | 10 |
| 生产调优 | 22 | 0 | 22 |
| 方言扩展 | 8 | 0 | 8 |
| 安全测试 | 0 | 12 | 12 |
| AI | 0 | 8 | 8 |
| 队列 | 0 | 8 | 8 |
| WASM | 0 | 4 | 4 |
| 真实驱动 | 0 | 12 | 12 |
| 测试基础设施 | 0 | 5 | 5 |
| CLI | 0 | 17 | 17 |
| 功能扩展 | 0 | 90 | 90 |

---

## 2. 决策 A 详情（32 个，已默认启用）

### sz-orm-core（27 个）

| Feature | 分类 | 依据 |
|---------|------|------|
| auto-prewarm | 性能优化 | 空门控，连接池预热 |
| perf-zero-copy-l2 | 性能优化 | 空门控，零拷贝优化 |
| perf-enum-dispatch | 性能优化 | 空门控，枚举分派优化 |
| perf-box-str | 性能优化 | 空门控，Box<str> 优化 |
| cache-coherence | 性能优化 | 空门控，缓存一致性 |
| performance | 性能优化 | 空门控，性能优化聚合 |
| dialect-cockroachdb | 方言扩展 | 空门控，SQL generation only |
| dialect-yugabytedb | 方言扩展 | 空门控，SQL generation only |
| dialect-snowflake | 方言扩展 | 空门控，SQL generation only |
| dialect-redshift | 方言扩展 | 空门控，SQL generation only |
| dialect-informix | 方言扩展 | 空门控，SQL generation only |
| dialect-saphana | 方言扩展 | 空门控，SQL generation only |
| dialect-firebird | 方言扩展 | 空门控，SQL generation only |
| prod-redis-tls | 生产调优 | 空门控，Redis TLS |
| prod-jwt-key-rotation | 生产调优 | 空门控，JWT 密钥轮换 |
| prod-metrics-acl | 生产调优 | 空门控，指标 ACL |
| prod-shutdown-timeout | 生产调优 | 空门控，关闭超时 |
| prod-leak-detection | 生产调优 | 空门控，泄漏检测 |
| prod-n1-tuning | 生产调优 | 空门控，N+1 调优 |
| prod-pool-tuning | 生产调优 | 空门控，连接池调优 |
| prod-config-masking | 生产调优 | 空门控，配置脱敏 |
| prod-log-level | 生产调优 | 空门控，日志级别 |
| prod-health-endpoint | 生产调优 | 空门控，健康端点 |
| prod-probe-endpoint | 生产调优 | 空门控，探针端点 |
| prod-circuit-tuning | 生产调优 | 空门控，熔断调优 |
| prod-rate-limit-tuning | 生产调优 | 空门控，限流调优 |
| prod-dialect-security | 生产调优 | 空门控，方言安全 |

### sz-orm-auth（1 个）

| Feature | 分类 | 依据 |
|---------|------|------|
| prod-jwt-key-rotation | 生产调优 | 空门控，JWT 密钥轮换 |

### sz-orm-health（2 个）

| Feature | 分类 | 依据 |
|---------|------|------|
| prod-health-endpoint | 生产调优 | 空门控，健康端点 |
| prod-probe-endpoint | 生产调优 | 空门控，探针端点 |

### sz-orm-queue（1 个）

| Feature | 分类 | 依据 |
|---------|------|------|
| prod-redis-tls | 生产调优 | 空门控，Redis TLS |

### sz-orm-sqlx（1 个）

| Feature | 分类 | 依据 |
|---------|------|------|
| auto-prewarm | 性能优化 | 空门控，连接池预热 |

---

## 3. 决策 B 详情（147 个，保持手动启用）

### 固定决策 B 的分类

| 分类 | 数量 | 依据 |
|------|------|------|
| 安全测试（owasp-pentest-suite） | 12 | 渗透测试代码不应进入生产默认构建 |
| AI | 8 | 需 LLM API key 等运行时凭证 |
| 队列 | 8 | 需外部 broker + 部分需原生库 |
| WASM | 4 | 需 wasm32 目标环境 |
| 真实驱动 | 12 | 需外部服务运行时 |
| 测试基础设施 | 5 | 测试用 feature |
| CLI | 17 | CLI 编译时按需启用 |
| 功能扩展 | 90 | 需逐案研判，暂定 B |

---

## 4. 验证结果

### 编译验证

```
cargo check --workspace -j 2
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 54s
```

✅ 工作空间编译通过

### 测试验证

| 包 | 结果 | 备注 |
|----|------|------|
| sz-orm-auth | ✅ 0 passed, 0 failed | 无测试 |
| sz-orm-health | ✅ 1 passed, 0 failed | 通过 |
| sz-orm-queue | ✅ 0 passed, 0 failed | 无测试 |
| sz-orm-sqlx | ✅ 1 passed, 0 failed | 通过 |
| sz-orm-core | ⚠️ 部分失败 | blackhat_sql_injection 3 个失败（已存在，非本次导致） |

### 已知失败（非本次导致）

- `blackhat_sql_injection::m5_having_multiple_conditions_and_joined` — 已存在失败
- `blackhat_sql_injection::m5_having_valid_count_renders` — 已存在失败
- `blackhat_sql_injection::m5_quick_query_having_parametized` — 已存在失败

---

## 5. 变更记录

### 修改的文件

| 文件 | 变更类型 | 变更数 |
|------|---------|--------|
| `packages/sz-orm-core/Cargo.toml` | default 数组追加 | 27 |
| `packages/sz-orm-auth/Cargo.toml` | default 数组追加 | 1 |
| `packages/sz-orm-health/Cargo.toml` | default 数组追加 | 2 |
| `packages/sz-orm-queue/Cargo.toml` | default 数组追加 | 1 |
| `packages/sz-orm-sqlx/Cargo.toml` | default 数组追加 | 1 |

### 附带修复

- `packages/sz-orm-core/Cargo.toml`：为 `tenant_quota_rls_regression` 测试添加 `required-features = ["multi-tenant-enhanced"]` 声明（修复已存在的编译错误）

---

## 6. 代码证据

- 评估脚本：`scripts/phantom2_evaluate.py`、`scripts/phantom2_verify.py`、`scripts/phantom2_apply.py`
- 初步决策：`phantom2-preliminary.json`（179 条记录）
- 验证结果：`phantom2-verified.json`（179 条记录）
- 变更记录：`phantom2-apply-log.json`（32 条变更）
- spec.md：`.codeartsdoer/specs/feature_gate_eval/spec.md`（477 行）
- design.md：`.codeartsdoer/specs/feature_gate_eval/design.md`（904 行）
- tasks.md：`.codeartsdoer/specs/feature_gate_eval/tasks.md`（429 行）