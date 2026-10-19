# v10.1.0 幻影交付与幻影测试审计报告

> 审计日期：2026-10-14
> 审计范围：v10.1.0 新增的全部代码和测试
> 审计依据：AGENTS.md 第 11 条（禁止幻影交付）+ 第 12 条（禁止幻影测试）
> **修复状态：全部已修复（2026-10-14）**

---

## 审计结论：存在幻影交付和幻影测试

### 一、幻影交付（4 个配置类型无生产消费点）

**判定标准**：新增配置类型必须在 Pool::new() 中被读取并创建组件 + 有 getter 方法（AGENTS.md 第 12 条）。

| # | 配置类型 | 定义位置 | Pool::new() 读取 | getter | 文档措辞 | 判定 |
|---|---------|---------|-----------------|--------|---------|------|
| 1 | `ObservabilityLoopConfig` | `pool.rs:864` | ❌ 未读取 | ❌ 无 | "可观测性自动闭环" | **幻影交付** |
| 2 | `ComplianceAutoCheckConfig` | `pool.rs:866` | ❌ 未读取 | ❌ 无 | "安全合规自动化" | **幻影交付** |
| 3 | `DiagnosticReportConfig` | `pool.rs:868` | ❌ 未读取 | ❌ 无 | "自动生成诊断报告" | **幻影交付** |
| 4 | `self_check_on_start` / `self_check_modules` | `pool.rs:870-872` | ❌ 未读取 | ❌ 无 | "自检" | **幻影交付** |

**对比：非幻影的配置类型**

| 配置类型 | Pool::new() 读取 | getter | 判定 |
|---------|-----------------|--------|------|
| `PerformanceLinkageConfig` | ✅ `pool.rs:1802` 读取 `.enabled` | ✅ `pool.rs:3021` `linkage_metrics()` | **非幻影** |

**根因**：Pool::with_factory()（pool.rs:1792）只读取了 `config.performance_linkage.enabled`，未读取 `config.observability_loop`、`config.compliance_auto_check`、`config.diagnostic_report`、`config.self_check_on_start`。这 4 个配置存储在 PoolConfig 中但无任何生产代码消费。

**文档措辞问题**：交付记录和规格文档使用了"自动闭环"、"自动检查"、"自动生成"等集成语义措辞，但实际需用户手动创建引擎实例，违反 AGENTS.md 第 11 条："禁止用'自动注入/自动拦截/默认生效'等集成语义描述零调用模块"。

---

### 二、幻影测试（8 个测试只验证初始值/配置值）

**判定标准**：测试必须验证行为副作用，不能只验证初始值/配置值（AGENTS.md 第 12 条）。

| # | 测试函数 | 文件:行号 | 验证内容 | 判定 |
|---|---------|---------|---------|------|
| 1 | `test_linkage_config_default_disabled` | `performance_linkage.rs:108` | 仅验证默认值 false | **幻影测试** |
| 2 | `test_observability_loop_config_default_disabled` | `performance_linkage.rs:129` | 仅验证默认值 false | **幻影测试** |
| 3 | `test_compliance_auto_check_config_default_disabled` | `performance_linkage.rs:138` | 仅验证默认值 false | **幻影测试** |
| 4 | `test_security_preset_default_is_none` | `performance_linkage.rs:145` | 仅验证默认值 None | **幻影测试** |
| 5 | `test_diagnostic_report_config_default_disabled` | `diagnostic_report.rs:108` | 仅验证默认值 false | **幻影测试** |
| 6 | `test_loop_config_default_disabled` | `observability_loop.rs:238` | 仅验证默认值 false | **幻影测试** |
| 7 | `test_compliance_config_default_disabled` | `compliance_auto_checker.rs:199` | 仅验证默认值 false | **幻影测试** |
| 8 | `test_v1010_pool_config_new_fields_default_e2e` | `wiring_e2e.rs:2358` | 仅验证 6 个字段默认值 | **幻影测试** |

---

### 三、不构成幻影的项（通过检查）

| 检查项 | 结果 | 证据 |
|--------|------|------|
| `let _result = ...` 丢弃结果 | ✅ 0 处 | 全量 grep 无匹配 |
| 永真断言（`>= 0.0` / `assert!(true)`） | ✅ 0 处 | 全量 grep 无匹配 |
| `PerformanceLinkageConfig` 生产消费点 | ✅ 有 | `pool.rs:1802` 读取 + `pool.rs:3021` getter + `pool.rs:3029/3038` record 方法 |
| `UnifiedHotUpdater` 生产消费点 | ✅ 有 | `pool.rs:3050` `Pool::apply_hot_update()` |
| `LinkageMetrics` 行为测试 | ✅ 有 | `record_hit/record_skip` 后验证计数变化 |
| `ObservabilityLoop` 行为测试 | ✅ 有 | `push_event` 后验证 `trigger_count` 变化 |
| `ComplianceAutoChecker` 行为测试 | ✅ 有 | `run_check` 后验证 `violations` 和 `compliance_score` 变化 |
| `DiagnosticReportGenerator` 行为测试 | ✅ 有 | `compute_health_score` 验证计算结果 |
| `UnifiedHotUpdater` 行为测试 | ✅ 有 | `validate` 验证接受/拒绝不同输入 |
| 并发正确性测试 | ✅ 有 | 100 线程并发后验证计数正确性 |

---

### 四、修复建议

#### 4.1 幻影交付修复（4 项）

每项需添加 Pool 生产消费点：

1. **ObservabilityLoopConfig**：Pool::new() 读取 `config.observability_loop`，当 enabled 时创建 `Arc<ObservabilityLoop>` 并存储在 Pool 中 + 添加 `pool.observability_loop()` getter
2. **ComplianceAutoCheckConfig**：Pool::new() 读取 `config.compliance_auto_check`，当 enabled 时创建 `ComplianceAutoChecker` + 添加 getter
3. **DiagnosticReportConfig**：Pool::new() 读取 `config.diagnostic_report`，当 enabled 时创建 `DiagnosticReportGenerator` + 添加 getter
4. **self_check_on_start**：Pool::new() 读取 `config.self_check_on_start`，当 true 时执行 `SystemSelfChecker::run()` + 添加 getter

或者：将文档措辞改为"提供 X 组件（需手动接入）"，不使用"自动"等集成语义。

#### 4.2 幻影测试修复（8 项）

每个只验证默认值的测试需补充行为副作用验证：

1. `test_linkage_config_default_disabled` → 补充：设置 enabled=true 后验证 Pool 创建 linkage_metrics
2. `test_observability_loop_config_default_disabled` → 补充：enabled=true 时 push_event 后 trigger_count 递增
3. `test_compliance_auto_check_config_default_disabled` → 补充：auto_check_enabled=true 时 run_check 后有结果
4. `test_security_preset_default_is_none` → 补充：apply_security_preset(ComplianceFirst) 后配置变化
5. `test_diagnostic_report_config_default_disabled` → 补充：enabled=true 时 compute_health_score 返回有效值
6. `test_loop_config_default_disabled` → 补充：enabled=true 时 push_event 后 analysis_count 递增
7. `test_compliance_config_default_disabled` → 补充：run_check 后 compliance_score 变化
8. `test_v1010_pool_config_new_fields_default_e2e` → 补充：设置 enabled=true 创建 Pool 后通过 getter 验证组件存在