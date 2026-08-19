# sz-orm 异常检测（Anomaly Detection）编码任务分解

> 任务编号：TASK-004
> 对应需求规格：`docs/spec/anomaly_detection/spec.md`（REQ-ANM-001 ~ REQ-ANM-016）
> 对应技术设计：`docs/spec/anomaly_detection/design.md`
> 版本基线：v4.9.0
> 日期：2026-08-19
> 目标：新增 sz-orm-anomaly 包，通过持续采集数据库运行指标，基于统计规则 + 阈值检测异常模式（突增/耗尽/偏离基线），输出异常告警

---

## 1. 新增 sz-orm-anomaly 包骨架

### 1.1 创建包结构
- [ ] 在 `packages/sz-orm-anomaly/` 新增 Cargo.toml（name = "sz-orm-anomaly"，version.workspace = true，edition.workspace = true）
- [ ] 新增 `src/lib.rs` 作为包入口，声明模块结构（collector/window/detector/alert/report/integration/config）
- [ ] 在根 Cargo.toml `[workspace] members` 添加 `packages/sz-orm-anomaly`
- [ ] 依赖声明：sz-orm-core / sz-orm-masking / serde / serde_json / tokio（feature 门控 anomaly-detection）
- [ ] 禁止 crate 级 `#![allow(dead_code)]`（session rules）
- **依赖**：无
- **验证方法**：`cargo check -p sz-orm-anomaly` 编译成功；`grep "sz-orm-anomaly" Cargo.toml` 命中
- **预估工作量**：0.5h

### 1.2 feature gate 配置
- [ ] 在 sz-orm-anomaly/Cargo.toml 定义 `[features] anomaly-detection = []`（默认不启用，REQ-ANM-015）
- [ ] 在 sz-orm-core/Cargo.toml 添加 `anomaly-detection = ["sz-orm-anomaly/anomaly-detection"]` feature gate
- [ ] 验证不启用 feature 时编译不含异常检测模块
- **依赖**：1.1
- **验证方法**：`cargo check -p sz-orm-core`（不启用 feature）成功；`cargo check -p sz-orm-core --features anomaly-detection` 成功
- **预估工作量**：0.5h

---

## 2. 指标采集模块实现

### 2.1 指标数据结构定义
- [ ] 在 `packages/sz-orm-anomaly/src/collector.rs` 定义 `SlowQueryMetric { timestamp, elapsed_ms, sql_summary }` / `ErrorMetric { timestamp, error_type, count }` / `PoolMetric { timestamp, active, idle, waiting, acquire_ms }`（REQ-ANM-001/002/003）
- [ ] 定义 `MetricType` 枚举（SLOW_QUERY/ERROR/POOL_USAGE）
- **依赖**：1.1
- **验证方法**：`cargo check -p sz-orm-anomaly` 编译成功
- **预估工作量**：0.5h

### 2.2 慢查询指标采集
- [ ] 实现 `pub fn record_slow_query(&self, elapsed_ms: u64, sql_summary: &str, timestamp: DateTime)`（REQ-ANM-001）
- [ ] SQL 摘要脱敏：调用 sz-orm-masking 将参数值替换为占位符（REQ-ANM-006）
- [ ] 异步/旁路采集：通过 channel 异步发送指标，不阻塞主路径（REQ-ANM-005）
- **依赖**：2.1
- **验证方法**：单元测试 record_slow_query 记录耗时/摘要/时间戳；脱敏测试 SQL 含 `password='xxx'` → 摘要显示 `password=?`
- **预估工作量**：1h

### 2.3 错误率指标采集
- [ ] 实现 `pub fn record_error(&self, error_type: ErrorType, timestamp: DateTime)`（REQ-ANM-002）
- [ ] 错误类型枚举：ConnectionError/SqlError/TimeoutError
- **依赖**：2.1
- **验证方法**：单元测试 record_error 记录错误类型/计数/时间戳
- **预估工作量**：0.5h

### 2.4 连接池指标采集
- [ ] 实现 `pub fn record_pool_usage(&self, active: u32, idle: u32, waiting: u32, acquire_ms: u64, timestamp: DateTime)`（REQ-ANM-003）
- [ ] 数据来源于 sz-orm-core PoolMetrics（需扩展瞬时值采集或新增瞬时指标）
- **依赖**：2.1
- **验证方法**：单元测试 record_pool_usage 记录活跃/空闲/等待/耗时
- **预估工作量**：0.5h

### 2.5 滑动窗口实现
- [ ] 在 `packages/sz-orm-anomaly/src/window.rs` 实现 `SlidingWindow`，使用 VecDeque 存储三类指标（REQ-ANM-004）
- [ ] 按时间戳淘汰旧数据：写入时检查头部是否超过窗口大小（默认 30 分钟），超过则 pop_front
- [ ] 内存上限 10 MB（DFX 4.1.3），超限时降采样丢弃最旧数据
- **依赖**：2.1
- **验证方法**：内存测试（采集超 30min）旧数据丢弃，内存不增长；`cargo test -p sz-orm-anomaly sliding_window`
- **预估工作量**：1h

---

## 3. 异常检测模块实现

### 3.1 基线计算（Welford 在线算法）
- [ ] 在 `packages/sz-orm-anomaly/src/detector.rs` 实现 `BaselineCalculator`，使用 Welford 算法在线计算均值/标准差（REQ-ANM-007）
- [ ] 仅维护 count/mean/M2 三个状态量，单次更新 O(1)
- [ ] 基线样本不足（< 100）时返回 None，检测回退绝对阈值
- **依赖**：2.5
- **验证方法**：单元测试 Welford 算法均值/标准差正确；样本不足返回 None
- **预估工作量**：1h

### 3.2 慢查询突增检测
- [ ] 实现 `fn check_slow_query_spike(&self) -> Option<Alert>`（REQ-ANM-007）
- [ ] 核心逻辑：计算滑动窗口内慢查询计数 → 与基线均值 + 3σ 比较 → 超过则输出 SLOW_QUERY_SPIKE 告警
- [ ] 基线样本不足时仅用绝对阈值（慢查询耗时阈值默认 100ms）
- **依赖**：3.1
- **验证方法**：集成测试（模拟慢查询突增）输出 SLOW_QUERY_SPIKE 告警，含当前计数/基线/阈值
- **预估工作量**：1h

### 3.3 错误率突增检测
- [ ] 实现 `fn check_error_rate_spike(&self) -> Option<Alert>`（REQ-ANM-008）
- [ ] 核心逻辑：计算滑动窗口内错误率 → 与基线均值 + 3σ 或绝对阈值（默认 5%）比较 → 超过则输出 ERROR_RATE_SPIKE 告警
- **依赖**：3.1
- **验证方法**：集成测试（模拟错误率突增至 6%）输出 ERROR_RATE_SPIKE 告警
- **预估工作量**：1h

### 3.4 连接池耗尽检测
- [ ] 实现 `fn check_pool_exhaustion(&self) -> Option<Alert>`（REQ-ANM-009）
- [ ] 核心逻辑：检查活跃数=上限 且 等待数>阈值（默认 10）或等待耗时>阈值（默认 1s）→ 输出 POOL_EXHAUSTION 告警
- **依赖**：2.5
- **验证方法**：集成测试（活跃=上限 且 等待>10）输出 POOL_EXHAUSTION 告警
- **预估工作量**：1h

### 3.5 偏离基线检测（Optional）
- [ ] 实现 `fn check_baseline_drift(&self) -> Option<Alert>`（REQ-ANM-010，Optional）
- [ ] 核心逻辑：检测平均耗时连续 N 窗口上升 → 输出 BASELINE_DRIFT 告警
- **依赖**：3.1
- **验证方法**：集成测试（模拟平均耗时连续 3 窗口上升）输出 BASELINE_DRIFT 告警
- **预估工作量**：1h

### 3.6 严重级别判定
- [ ] 实现 `fn judge_severity(metric_value, threshold) -> Severity`（REQ-ANM-011）
- [ ] 规则：超阈值 1.5x → WARN，超 3x → CRITICAL，其他 → INFO
- **依赖**：3.2
- **验证方法**：单元测试超阈值 3x 返回 CRITICAL；超 1.5x 返回 WARN
- **预估工作量**：0.5h

### 3.7 统一检测入口
- [ ] 实现 `pub fn detect_anomalies(&self) -> Vec<Alert>`，调用三类检测 + 严重级别判定（REQ-ANM-007/008/009）
- **依赖**：3.2, 3.3, 3.4, 3.5, 3.6
- **验证方法**：集成测试 detect_anomalies 返回异常列表
- **预估工作量**：0.5h

---

## 4. 告警输出与去重模块实现

### 4.1 告警事件结构
- [ ] 在 `packages/sz-orm-anomaly/src/alert.rs` 定义 `Alert` struct：anomaly_type / severity / timestamp / metric_value / threshold / baseline / suggestion / sql_summary（REQ-ANM-006 数据约束）
- [ ] 定义 `AnomalyType` 枚举（SLOW_QUERY_SPIKE/ERROR_RATE_SPIKE/POOL_EXHAUSTION/BASELINE_DRIFT）
- [ ] 定义 `Severity` 枚举（INFO/WARN/CRITICAL）
- **依赖**：1.1
- **验证方法**：`cargo check -p sz-orm-anomaly` 编译成功
- **预估工作量**：0.5h

### 4.2 告警去重（冷却期）
- [ ] 实现 `AlertDedup`：HashMap<AnomalyType, DateTime> 记录每种异常最后告警时间（REQ-ANM-014）
- [ ] 新告警检查冷却期（默认 5 分钟），期内不重复告警（更新计数）
- **依赖**：4.1
- **验证方法**：单元测试同类型异常 5 分钟内第二次不重复告警
- **预估工作量**：0.5h

### 4.3 告警订阅 API
- [ ] 实现 `pub fn subscribe_alerts(&self, callback: Arc<dyn Fn(Alert) + Send + Sync>) -> SubscriptionId`（REQ-ANM-003 告警订阅）
- [ ] 异常发生时通知订阅回调
- [ ] 回调 panic 捕获隔离（catch_unwind），不影响检测模块
- **依赖**：4.1
- **验证方法**：单元测试注册回调后异常发生时回调被调用；回调 panic 不影响检测
- **预估工作量**：1h

### 4.4 告警输出集成
- [ ] 实现 `fn emit_alert(&self, alert: Alert)`：去重检查 → 通知订阅者 → 导出 Prometheus 指标 → CRITICAL 影响健康度
- **依赖**：4.2, 4.3
- **验证方法**：集成测试 emit_alert 全链路触发
- **预估工作量**：0.5h

---

## 5. 报告导出模块实现

### 5.1 JSON 报告导出
- [ ] 在 `packages/sz-orm-anomaly/src/report.rs` 实现 `pub fn export_report_json(&self, period: TimeRange) -> String`（REQ-ANM-003 报告导出）
- [ ] JSON 含：检测时间段/异常列表/统计摘要（总异常数/各类型计数/各严重级别计数）
- **依赖**：4.1
- **验证方法**：单元测试 export_report_json 输出合法 JSON，含全部字段
- **预估工作量**：0.5h

### 5.2 Markdown 报告导出
- [ ] 实现 `pub fn export_report_markdown(&self, period: TimeRange) -> String`（REQ-ANM-003 报告导出）
- [ ] Markdown 含表格 + 异常列表
- **依赖**：4.1
- **验证方法**：单元测试 export_report_markdown 输出合法 Markdown 表格
- **预估工作量**：0.5h

---

## 6. 集成模块实现

### 6.1 Prometheus 指标导出
- [ ] 在 `packages/sz-orm-anomaly/src/integration.rs` 实现 `fn export_prometheus_metrics(&self) -> String`（REQ-ANM-003 Prometheus 集成，Optional）
- [ ] 导出 anomaly_count / anomaly_last_timestamp 指标
- [ ] 若 sz-orm-observability 未接入则跳过
- **依赖**：4.1
- **验证方法**：单元测试 export_prometheus_metrics 输出 Prometheus 格式含 anomaly_count
- **预估工作量**：0.5h

### 6.2 健康度集成
- [ ] 实现 `fn impact_health(&self, alert: &Alert)`（REQ-ANM-003 健康度集成，Optional）
- [ ] CRITICAL 异常降低健康度
- [ ] 若 sz-orm-health 未接入则跳过
- **依赖**：4.1
- **验证方法**：单元测试 CRITICAL 异常降低健康度
- **预估工作量**：0.5h

---

## 7. 配置模块实现

### 7.1 阈值配置结构
- [ ] 在 `packages/sz-orm-anomaly/src/config.rs` 定义 `AnomalyConfig`：slow_query_threshold_ms / slow_query_sigma / error_rate_threshold / pool_wait_count_threshold / pool_wait_time_threshold_ms / window_size / alert_cooldown / min_baseline_samples（REQ-ANM-006 检测配置）
- [ ] 默认值：100ms / 3 / 5% / 10 / 1s / 30min / 5min / 100
- **依赖**：1.1
- **验证方法**：`cargo check -p sz-orm-anomaly` 编译成功
- **预估工作量**：0.3h

### 7.2 配置热更新
- [ ] 实现 `pub fn update_config(&self, new_config: AnomalyConfig)`（DFX 4.4.3 热更新）
- [ ] 使用 Arc<RwLock<AnomalyConfig>> 实现运行时热更新，不重启
- [ ] 阈值配置非法（负数/无效值）时使用默认阈值，记录配置错误
- **依赖**：7.1
- **验证方法**：单元测试 update_config 运行时生效；非法配置回退默认值
- **预估工作量**：0.5h

---

## 8. 测试验证

### 8.1 单元测试
- [ ] 阈值判定逻辑单元测试：突增/耗尽/偏离基线判定正确（REQ-ANM-016）
- [ ] 严重级别判定单元测试：INFO/WARN/CRITICAL 正确
- [ ] 告警去重单元测试：冷却期内不重复
- [ ] SQL 脱敏单元测试：参数值替换为占位符
- [ ] 滑动窗口单元测试：旧数据丢弃、内存不增长
- **依赖**：3.7, 4.4, 7.2
- **验证方法**：`cargo test -p sz-orm-anomaly --lib` 全通过
- **预估工作量**：2h

### 8.2 集成测试
- [ ] 模拟慢查询突增场景：注入大量慢查询指标 → detect_anomalies 返回 SLOW_QUERY_SPIKE 告警（REQ-ANM-007）
- [ ] 模拟错误率突增场景：注入大量错误指标 → 返回 ERROR_RATE_SPIKE 告警（REQ-ANM-008）
- [ ] 模拟连接池耗尽场景：注入活跃=上限+等待>阈值指标 → 返回 POOL_EXHAUSTION 告警（REQ-ANM-009）
- [ ] 告警订阅集成测试：注册回调 → 触发异常 → 回调被调用
- [ ] 报告导出集成测试：export_report_json/markdown 输出完整
- **依赖**：8.1
- **验证方法**：`cargo test -p sz-orm-anomaly --test integration` 全通过
- **预估工作量**：2h

### 8.3 负向测试（误报/漏报控制）
- [ ] 误报控制测试：正常指标不触发告警，误报率 < 5%（REQ-ANM-012）
- [ ] 漏报控制测试：模拟真实异常必须触发告警，否则测试失败（REQ-ANM-013）
- [ ] 基线样本不足测试：样本 < 100 时仅用绝对阈值，不误判
- **依赖**：8.2
- **验证方法**：`cargo test -p sz-orm-anomaly --test negative` 全通过
- **预估工作量**：1h

### 8.4 性能测试
- [ ] 指标采集耗时测试：单次 record_slow_query < 100μs（REQ-ANM-005）
- [ ] 检测判定耗时测试：单次 detect_anomalies < 1ms
- [ ] 内存占用测试：滑动窗口 30 分钟数据 < 10 MB
- **依赖**：8.2
- **验证方法**：`cargo test -p sz-orm-anomaly --test perf -- --ignored` 全通过
- **预估工作量**：1h

---

## 9. feature 门控与兼容性验证

### 9.1 feature 门控验证
- [ ] 执行 `cargo check -p sz-orm-core`（不启用 anomaly-detection）确认编译成功，无检测模块依赖（REQ-ANM-015）
- [ ] 执行 `cargo check -p sz-orm-core --features anomaly-detection` 确认编译成功
- **依赖**：8.4
- **验证方法**：cargo check 两种模式均退出码 0
- **预估工作量**：0.5h

### 9.2 既有 API 不变验证
- [ ] 执行 `cargo test -p sz-orm-diagnosis` 确认既有测试全通过（DFX 4.5.1）
- [ ] 执行 `cargo test -p sz-orm-observability` 确认既有测试全通过
- [ ] 执行 `cargo test -p sz-orm-health` 确认既有测试全通过
- [ ] `git diff` 确认既有包 API 签名未变更
- **依赖**：8.4
- **验证方法**：cargo test 各包退出码 0；git diff 无既有包签名变更
- **预估工作量**：0.5h

---

## 10. 交付记录与文档

### 10.1 生成交付记录
- [ ] 生成 `docs/spec/anomaly_detection/delivery-record.md`，含：新增 API 清单（record_slow_query/record_error/record_pool_usage/detect_anomalies/subscribe_alerts/export_report_json/markdown/update_config）、测试结果（单元+集成+负向+性能）、集成点证据（Prometheus/健康度，file:line）、feature gate 启用方式（`--features anomaly-detection`）（REQ-ANM-016）
- **依赖**：9.1, 9.2
- **验证方法**：delivery-record.md 存在且内容完整；含 file:line 证据
- **预估工作量**：0.5h

### 10.2 更新对比分析文档
- [ ] 更新 `docs/sz-orm与同类产品对比分析.md`：新增异常检测能力项
- **依赖**：9.1
- **验证方法**：grep 对比文档含"异常检测"
- **预估工作量**：0.3h

---

## 11. 审查与确认

### 11.1 五维审查
- [ ] 正确性：三类检测准确，误报 < 5%，漏报为零
- [ ] 可读性：代码精简，模块划分清晰
- [ ] 架构：独立包 + feature gate 隔离 + 复用既有基础设施（masking/observability/health）
- [ ] 安全性：SQL 摘要脱敏；阈值配置不泄露日志；回调 panic 隔离
- [ ] 性能：采集 < 100μs，判定 < 1ms，内存 < 10MB
- **依赖**：10.1, 10.2
- **验证方法**：审查清单逐项确认，附 file:line 证据
- **预估工作量**：0.5h

### 11.2 变更范围确认
- [ ] 确认新增 sz-orm-anomaly 包 + sz-orm-core Cargo.toml feature gate + 交付记录文档
- [ ] 确认未修改既有 sz-orm-diagnosis/observability/health 包 API
- [ ] 确认 sz-pay 生产依赖不受影响（sz-pay 不启用 anomaly-detection feature）
- **依赖**：11.1
- **验证方法**：`git diff --name-only` 仅含上述文件
- **预估工作量**：0.2h

---

## 任务依赖关系

```
1.1 → 1.2 → 2.1 → 2.2 → 2.5 → 3.1 → 3.2 → 3.6 → 3.7 → 4.4 → 8.1 → 8.2 → 8.3 → 8.4 → 9.1 → 9.2 → 10.1 → 11.1 → 11.2
2.1 → 2.3 → 2.5
2.1 → 2.4 → 2.5
3.1 → 3.3 → 3.7
3.1 → 3.5 → 3.7
2.5 → 3.4 → 3.7
2.1 → 4.1 → 4.2 → 4.4
4.1 → 4.3 → 4.4
4.1 → 5.1 → 5.2
4.1 → 6.1 → 6.2
1.1 → 7.1 → 7.2
8.3 → 8.4
9.1 → 10.2
```

## 任务统计

- 主任务：11 组
- 子任务：38 个
- 需求覆盖：REQ-ANM-001 ~ REQ-ANM-016 全部 16 项
- 预估总工作量：约 24h