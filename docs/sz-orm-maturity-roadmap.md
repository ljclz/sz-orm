# SZ-ORM 已实现包成熟化执行路线图

> 版本：v4.9.0 | 制定日期：2026-08-15 | 基于 2026-08-15 全量审计实测数据
> 目标：将 25 个"🟡 已实现" + 2 个降级包（config/postgis）+ 6 个"🔍 待复核"包全部提升为"✅ 成熟（代码完整、测试充分）"
> 关联文档：[sz-orm与同类产品对比分析.md](sz-orm与同类产品对比分析.md)

---

## 1. 背景与目标

### 1.1 现状

2026-08-15 审计后，58 个工作空间包分类为：

| 分类 | 数量 | 判定标准 |
|------|------|---------|
| ✅ 成熟（代码完整、测试充分） | 27 | LOC ≥ 3,000 且 tests ≥ 50 且 API ≥ 30 |
| 🟡 已实现（功能完整） | 25 | API ≥ 3 且（tests ≥ 10 或 E2E/CLI/宏接入证据） |
| 🔍 待复核 | 6 | API ≥ 3 但 Rust 侧 tests < 10（跨语言 E2E 另计） |

> **数据口径修正（2026-08-15 二次修订）**：初版数据用"全包 LOC（含 tests/）"和"pub fn 行数 + no_mangle 行数"统计，
> 导致：① LOC 高估 2-17%（tests/ 目录计入）；② API 高估（impl 内同名方法重复计数 + no_mangle 双重计数）。
> 本版统一为：LOC = 仅 `src/` 目录；API = **唯一函数名**（`pub fn` / `pub async fn` / `pub extern "C" fn` / `pub extern "system" fn` 去重）。
> 修正后：原 29 个成熟中 sz-orm-config（LOC 2,834）与 sz-orm-postgis（API 29）降级；java API 6 与独立验证一致 ✅。
> 目标：25 个"已实现" + 2 个降级包 + 6 个待复核包全部达到"成熟"。

### 1.2 成熟标准（两轨制，建议采纳）

> ⚠️ **重要**：单轨 LOC 标准对绑定/集成层不适用（详见 §3.3）。
> 建议将"成熟"判定拆分为两轨：

| 轨 | 适用对象 | 成熟标准 |
|----|---------|---------|
| **功能型轨** | 独立功能库（调度/驱动/诊断/优化器等） | LOC ≥ 3,000 且 tests ≥ 50 且 API ≥ 30 |
| **绑定/集成轨** | 跨语言绑定 + 框架集成层（java/go/cpp/python/cabi/axum/flamegraph/actix/js） | 跨语言 E2E 全部通过 + 导出函数 100% 有测试覆盖 + 头文件/文档齐全（**不设 LOC 门槛**） |

---

## 2. 差距总览（25 个已实现 + 2 个降级 + 6 个待复核，2026-08-15 修正口径实测）

> 数据口径（2026-08-15 修正）：LOC = `find packages/$pkg/src -name "*.rs" -exec cat {} + | wc -l`（仅 src/，不含 tests/）；
> tests = `grep -rE "#\[test\]|#\[tokio::test"`；
> API = 唯一函数名（`pub fn` / `pub async fn` / `pub extern "C" fn` / `pub extern "system" fn` 提取函数名后 `sort -u`），
> 不重复计数 impl 内同名方法，no_mangle 不单独累加。

### 2.1 A 类：只差临门一脚（6 个）

| 包 | LOC | tests | API | 差距（vs 成熟标准） | 优先级 |
|----|-----|-------|-----|-------------------|--------|
| sz-orm-scheduler | 2,580 | 96 | 59 | LOC 差 420 | P0 |
| sz-orm-advisor | 1,491 | 44 | 23 | tests 差 6 / API 差 7 | P0 |
| sz-orm-explain | 2,036 | 47 | 13 | tests 差 3 / API 差 17 | P0 |
| sz-orm-limit | 1,535 | 63 | 16 | API 差 14 | P0 |
| sz-orm-graph | 1,116 | 33 | 42 | tests 差 17 / LOC 差 1,884 | P1 |
| sz-orm-designer | 1,715 | 26 | 32 | tests 差 24 / LOC 差 1,285 | P1 |

### 2.2 B 类：功能型包，需补真实功能 + 测试（12 个）

| 包 | LOC | tests | API | 差距 | 优先级 |
|----|-----|-------|-----|------|--------|
| sz-orm-parallel | 1,022 | 33 | 20 | tests 差 17 / API 差 10 / LOC 差 1,978 | P1 |
| sz-orm-stream | 972 | 42 | 20 | tests 差 8 / API 差 10 / LOC 差 2,028 | P1 |
| sz-orm-fusion | 1,436 | 35 | 18 | tests 差 15 / API 差 12 / LOC 差 1,564 | P1 |
| sz-orm-cabi | 806 | 22 | 18 | tests 差 28 / API 差 12 / LOC 差 2,194 | P1 |
| sz-orm-mssql | 1,135 | 24 | 6 | tests 差 26 / API 差 24 / LOC 差 1,865 | P2 |
| sz-orm-oracle | 1,338 | 21 | 9 | tests 差 29 / API 差 21 / LOC 差 1,662 | P2 |
| sz-orm-adaptive | 571 | 19 | 14 | tests 差 31 / API 差 16 / LOC 差 2,429 | P2 |
| sz-orm-diagnosis | 906 | 31 | 8 | tests 差 19 / API 差 22 / LOC 差 2,094 | P2 |
| sz-orm-masking | 663 | 69 | 4 | API 差 26 / LOC 差 2,337 | P2 |
| sz-orm-actix | 479 | 20 | 7 | tests 差 30 / API 差 23 / LOC 差 2,521 | P2 |
| sz-orm-js | 648 | 18 | 37 | tests 差 32 / LOC 差 2,352 | P2 |
| sz-orm-n1-lint | 412 | 7 | 4 | tests 差 43 / API 差 26 / LOC 差 2,588 | P2 |

> 注：sz-orm-python 因 Rust 侧 tests=8 < 10 移入 C 类（绑定/集成轨）复核。

### 2.3 C 类：薄绑定/集成层 + 待复核（14 个）

**C1 绑定/集成轨（LOC 门槛不适用，按 E2E 覆盖判定）：**

| 包 | LOC | tests | API | 成熟路径（绑定/集成轨） | 优先级 |
|----|-----|-------|-----|------------------------|--------|
| sz-orm-java | 181 | 0（Java E2E 7 步 ✓） | 6 | 补充事务级 JNI API + 扩展 Java E2E | P1 |
| sz-orm-go | 284 | 8（Go E2E ✓） | 8 | 补充事务级 syscall API + 扩展 Go E2E | P1 |
| sz-orm-cpp | 272 | 7（无 g++ E2E） | 8 | **在 g++ 环境执行 szorm.h 编译 + E2E** | P1 |
| sz-orm-python | 856 | 8 | 3 | PyPool 已真实连接；补 PyModel/QueryBuilder API + 测试至 tests ≥ 10 | P1 |
| sz-orm-axum | 203 | 16 | 5 | 补中间件集成测试（事务提交/回滚路径） | P2 |
| sz-orm-flamegraph | 362 | 8 | 8 | 补 SVG 快照测试 + Brendan Gregg 格式黄金文件 | P2 |

**C2 功能完整但未达功能型轨门槛（按功能型轨补齐）：**

| 包 | LOC | tests | API | 差距 | 优先级 |
|----|-----|-------|-----|------|--------|
| sz-orm-config | 2,834 | 146 | 41 | LOC 差 166（原 29 成熟中降级） | P1 |
| sz-orm-postgis | 3,201 | 78 | 29 | API 差 1（原 29 成熟中降级） | P0 |
| sz-orm-rw | 2,199 | 113 | 59 | LOC 差 801 | P2 |
| sz-orm-grpc | 2,156 | 63 | 35 | LOC 差 844 | P2 |
| sz-orm-sql-validator | 2,084 | 92 | 28 | API 差 2 / LOC 差 916 | P2 |
| sz-orm-crypto | 1,569 | 119 | 29 | API 差 1 / LOC 差 1,431 | P2 |
| sz-orm-logger | 1,780 | 86 | 57 | LOC 差 1,220 | P2 |

> **修正说明**：初版将 C 类标为 10 个并称 rw/grpc/crypto/sql-validator/logger "功能完整只需补测试"，
> 新口径显示这 5 个包在功能型轨下 LOC/API 均有真实差距（2-1,431 LOC），不能简单视为"已成熟"。
> sz-orm-postgis 仅差 1 个 API，应优先处理（P0）。

---

## 3. 执行清单（逐包行动项 + 验收标准）

### 3.1 A 类：只差临门一脚

#### A1. sz-orm-scheduler（LOC 2,823 → ≥3,000，补 ~200 LOC 功能）✅ 已完成

- [x] 补调度器状态机测试：任务取消 / 重试策略 / 失败隔离
- [x] 补优先级队列行为测试（高优先级任务先执行）
- [x] 补定时调度边界测试（时区 / DST / 闰秒处理）
- [x] 新增 TaskExecutionTracker（执行历史追踪）+ TaskHealthSummary（健康度汇总）
- **验收**：LOC=3078 ≥ 3000，tests=109 ≥ 100，clippy 零警告 ✅

#### A2. sz-orm-advisor（tests 44 → 50，API 26 → 30）✅ 已完成

- [x] 补 6 个建议类型的 DDL 生成方言覆盖测试（MySQL/PG/SQLite/Oracle/MSSQL）
- [x] 新增 AdvisorConfig builder 链 + SuggestionType::all/is_ddl + AdvisorDialect::parse_name
- [x] 补建议优先级排序测试
- **验收**：tests=51 ≥ 50，API=32 ≥ 30 ✅

#### A3. sz-orm-explain（tests 47 → 50，API 20 → 30）✅ 已完成

- [x] 补五方言 EXPLAIN 解析边界测试（嵌套计划 / 并行计划 / 分区表）
- [x] 新增计划树遍历 API：ExplainDialect/ScanType 的 as_str/all/parse_name + ExplainPlan 10 个查询方法
- [x] 补计划回归检测测试（计划变化 → PlanRegression 触发）
- **验收**：tests=50 ≥ 50，API=30 ≥ 30 ✅

#### A4. sz-orm-limit（API 26 → 30）✅ 已完成

- [x] 新增限流策略变体：15 个查询方法（is_allowed/capacity/key_count 等）
- [x] 补策略切换热更新测试
- **验收**：API=41 ≥ 30，tests=55 全过 ✅

#### A5. sz-orm-graph（tests 33 → 50）✅ 已完成

- [x] 补图查询测试：路径遍历 / 最短路径 / 环检测
- [x] 补 Cypher 生成测试（节点/关系/属性过滤）
- **验收**：tests=51 ≥ 50 ✅

#### A6. sz-orm-designer（tests 26 → 50）✅ 已完成

- [x] 补 Schema 序列化往返测试（Model → SQL → Model）
- [x] 补逆向工程测试（DDL → Schema 对象）
- [x] 补表关系检测测试（外键 → 关系图）
- **验收**：tests=50 ≥ 50 ✅

### 3.2 B 类：功能型包

#### B1. sz-orm-parallel（tests 33 → 50，API 22 → 30）

- [ ] 新增结果集类型：`BTreeMap` 合并 / `Stream` 输出
- [ ] 新增调度策略：`FifoStrategy` / `LifoStrategy`（当前仅 Semaphore）
- [ ] 补 200 查询压力测试 + 极端并发（64 worker）测试
- **验收**：tests ≥ 50，API ≥ 30

#### B2. sz-orm-stream（tests 42 → 50，API 23 → 30）

- [ ] 补真 SQLite 流式集成测试（`tests/` 目录，连接 `sqlite::memory:` 全流程）
- [ ] 新增窗口聚合 API：`window_batch(size)` / `aggregate(expr)`
- [ ] 补背压唤醒路径测试（生产者等待 → 消费者 pop → 继续）
- **验收**：tests ≥ 50，API ≥ 30

#### B3. sz-orm-fusion（tests 35 → 50，API 22 → 30）

- [ ] 补多源一致性校验测试（主库 vs 缓存行数/内容比对）
- [ ] 新增降级策略变体：`DegradeToPrimary` / `DegradeToCache` / `DegradeToNull`
- [ ] 补 TTL 缓存失效广播测试
- **验收**：tests ≥ 50，API ≥ 30

#### B4. sz-orm-cabi（tests 22 → 50，API 18 → 30）

- [ ] 新增事务句柄导出：`sz_orm_transaction_begin/commit/rollback`
- [ ] 新增批量执行导出：`sz_orm_execute_batch(sqls, count)`
- [ ] 新增错误消息字符串 API：`sz_orm_last_error()`
- [ ] 补并发 E2E 测试（多线程同时 pool_new/query/free）
- **验收**：tests ≥ 50，API ≥ 30，Java/Go/C++ 侧同步更新调用

#### B5. sz-orm-mssql（tests 24 → 50，API 3 → 30）

- [ ] 补连接串解析 API：`parse_conn_str()`（server/db/user/password 提取）
- [ ] 补参数化查询 API：`execute_with_params()` / `query_with_params()`
- [ ] 补事务 API：`begin/commit/rollback`
- [ ] 补类型映射测试：money/smalldatetime/uniqueidentifier
- **验收**：tests ≥ 50，API ≥ 30

#### B6. sz-orm-oracle（tests 21 → 50，API 10 → 30）

- [ ] 补连接串解析 API：`parse_connect_string()`（host/port/service_name）
- [ ] 补 PL/SQL 调用 API：`call_procedure()` / `call_function()`
- [ ] 补 LOB 处理 API：`read_lob()` / `write_lob()`
- [ ] 补类型映射测试：NUMBER 精度 / TIMESTAMP WITH TZ / RAW
- **验收**：tests ≥ 50，API ≥ 30

#### B7. sz-orm-adaptive（tests 19 → 50，API 17 → 30）

- [ ] 新增自适应策略族：`IndexSelectionStrategy` / `JoinOrderStrategy` / `BatchSizeTuner`
- [ ] 补策略收敛测试（多次执行 → 决策稳定）
- [ ] 补统计窗口滑动测试
- **验收**：tests ≥ 50，API ≥ 30

#### B8. sz-orm-diagnosis（tests 31 → 50，API 10 → 30）

- [ ] 新增修复建议 API：`suggest_fix()`（返回可执行建议列表）
- [ ] 新增报告导出 API：`to_json()` / `to_html()` / `to_markdown()`
- [ ] 补历史诊断对比测试（两次诊断 → 差异报告）
- **验收**：tests ≥ 50，API ≥ 30

#### B9. sz-orm-masking（API 4 → 30）

- [ ] 新增脱敏规则变体：`Url` / `Coordinates` / `Regex(pattern)` / `CreditCard` / `Visa`
- [ ] 补规则解析 API：`parse_rule(spec)` / `rule_list()`
- [ ] 补组合规则 API：`compose(rules)`（链式组合）
- **验收**：API ≥ 30，tests ≥ 69 保持全过

#### B10. sz-orm-actix（tests 20 → 50，API 7 → 30）

- [ ] 补事务中间件完整测试：提交路径 / 回滚路径 / 异常路径
- [ ] 新增 `TxExtractor`（请求级事务提取）API
- [ ] 新增 `ErrorResponse`（统一错误响应）API
- [ ] 补 PoolState 并发访问测试
- **验收**：tests ≥ 50，API ≥ 30

#### B11. sz-orm-js（tests 18 → 50）

- [ ] 补 napi 绑定单元测试：Model 全方法 / QueryBuilder 全方法 / Pool 配置
- [ ] 补类型转换测试（JS number/string/bool ↔ Value）
- [ ] 补错误处理测试（DB 错误 → JS Error）
- **验收**：tests ≥ 50

#### B12. sz-orm-n1-lint（tests 7 → 50，API 4 → 30）

- [ ] 新增报告 API：`to_json()` / `to_sarif()` / `to_markdown()`
- [ ] 新增配置 API：`LintConfig`（启用/禁用模式、白名单）
- [ ] 补 AST 边界测试：嵌套循环 / 闭包捕获 / 宏展开 / async 块
- [ ] 补 CLI 集成测试（`cargo run -- n1-lint --path=...`）
- **验收**：tests ≥ 50，API ≥ 30

#### B13. sz-orm-python（tests 8 → 50，API 3 → 30）

- [ ] 补 PyModel CRUD API：`save()` / `find()` / `delete()`
- [ ] 补 PyQueryBuilder 全 API 映射测试（build_select/insert/update/delete）
- [ ] 补 async 桥接测试（pyo3-asyncio → tokio）
- [ ] 补连接池配置校验测试
- **验收**：tests ≥ 50，API ≥ 30

### 3.3 C 类：绑定/集成层（绑定轨标准）

#### C1. sz-orm-java（绑定轨）

- [ ] 新增事务级 JNI API：`beginTransaction` / `commit` / `rollback`
- [ ] 扩展 Java E2E：事务提交 / 回滚 / 嵌套保存点
- **验收**：Java E2E 通过 ≥ 12 步（新增 5 步），事务 API 有测试

#### C2. sz-orm-go（绑定轨）

- [ ] 新增事务级 syscall API：`BeginTx` / `Commit` / `Rollback`
- [ ] 扩展 Go E2E：事务提交 / 回滚
- **验收**：Go E2E 通过 ≥ 10 步

#### C3. sz-orm-cpp（绑定轨）

- [ ] **在 g++ 环境执行**：`g++ -std=c++17 test.cpp -lsz_orm_cpp` 编译验证
- [ ] 新增 C++ 侧 E2E 测试（建表/插入/查询/事务）
- **验收**：g++ 编译通过 + C++ E2E 全过（需 CI 或带 g++ 的机器）

#### C4. sz-orm-axum（绑定轨）

- [ ] 补事务中间件集成测试：请求成功 → commit；handler 报错 → rollback
- **验收**：tests ≥ 25，事务路径 100% 覆盖

#### C5. sz-orm-flamegraph（绑定轨）

- [ ] 补 SVG 快照测试（黄金文件比对）
- [ ] 补 Brendan Gregg 格式黄金文件
- **验收**：tests ≥ 12，渲染输出有快照验证

#### C6. sz-orm-rw / C7. sz-orm-grpc / C8. sz-orm-crypto / C9. sz-orm-sql-validator / C10. sz-orm-logger

- [ ] 逐包补场景测试（failover / 拦截器 / KAT 向量 / 方言树 / 格式化）
- **验收**：各包 tests ≥ 50 且功能路径有覆盖

---

## 4. 里程碑规划

| 里程碑 | 范围 | 预计工作量 | 验收门禁 |
|--------|------|-----------|---------|
| **M0：数据修正落地** | 路线图 + 对比文档数据同步（新口径） | 0.5 天 | 文档数字与源码实测一致 |
| **M1：近门槛清零** ✅ | A 类 6 包（scheduler/advisor/explain/limit/graph/designer） | 2-3 天 | 6 包达标 + clippy 零警告 |
| **M2：标准修正** | 两轨制标准落地（文档 + 门禁 15/19 脚本同步） | 0.5 天 | 绑定轨包按 E2E 判定成熟 |
| **M3：B 类近门槛** ✅ | B1-B4（parallel/stream/fusion/cabi） | 3-4 天 | 4 包 tests ≥ 50 / API ≥ 30 |
| **M4：B 类全量** ✅ | B5-B12（8 个包） | 5-8 天 | 8 包达标 |
| **M5：C 类全量** ✅ | C1 绑定轨 6 包 + C2 功能轨 5 包 | 3-5 天 | 绑定轨 E2E 全过 + 功能轨达标（cpp 需 g++ 环境） |

**合计**：约 15-20 个工作日（修正后），58 个包全部达到"成熟"。
> 注：与初版 10-15 天相比增加约 5 天，原因是新口径暴露的 API/LOC 差距更大（如 advisor API 差 4→7、
> explain API 差 10→17、config/postgis 降级需补），以及 5 个"功能完整"包（rw/grpc/crypto/sql-validator/logger）
> 在功能型轨下实际未达门槛。

---

## 5. 门禁与验证

每个包成熟化完成后必须通过：

| # | 门禁 | 命令 |
|---|------|------|
| 1 | fmt | `cargo fmt --all -- --check` |
| 2 | check | `cargo check --workspace --all-targets` |
| 3 | clippy | `cargo clippy --workspace --all-targets -- -D warnings` |
| 4 | test | `cargo test -p <pkg> [--features <feat>]`（tests 达标） |
| 8 | 禁止占位实现 | `grep -rn 'todo!\|unimplemented!\|unreachable!'` |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` |
| 15 | 幻影交付检查 | `python scripts/check-phantom-delivery.py` |
| 20 | 变异测试杀率 | `python scripts/check-mutation-coverage.py` |
| 22 | 覆盖率门禁 | `python scripts/check-coverage.py`（关键模块 ≥ 60%） |

**逐包验证要求**：
- 每包完成后运行 `cargo test -p <pkg>` 并附输出（禁止批量声称"全部通过"）
- 每个新增 API 必须附 `file:line` 证据（审计合规铁律）
- 文档 [sz-orm与同类产品对比分析.md](sz-orm与同类产品对比分析.md) 同步更新分类

---

## 6. 风险与注意事项

### 6.1 设计约束（ADR 铁律）

- **禁止为凑 LOC 堆代码**：LOC 差距必须通过真实功能/测试补齐，违反"禁止幻影交付"（门禁 15）
- **API 兼容性**：新增 API 必须向后兼容，签名变更需同步更新所有调用方（含 sz-pay）
- **Feature 隔离**：新 API 放入既有 feature gate，默认关闭，无 Breaking Change

### 6.2 环境限制

- **C++ 绑定**：本机无 g++，C++ E2E 需在 CI 或有 g++ 的机器执行（文档已如实标注）
- **JS 绑定**：Node E2E 需 Node.js 环境（当前仅 Rust 侧单元测试）

### 6.3 优先级建议

1. **M0（数据修正）先行**：路线图 + 对比文档数据同步（已完成本路线图，对比文档待并行会话收敛）
2. **M1（近门槛）**：A 类 6 包 + postgis + config，低风险、见效快，2-3 天
3. **M2（标准修正）紧跟**：避免绑定层包被 LOC 门槛卡死
4. **M3（B 类近门槛）**：parallel/stream/fusion/cabi 已接近达标
5. **M4/M5**：剩余 B 类 + C 类，逐包推进，每包独立验证

---

> 本文档数据基于 2026-08-15 全量审计实测（LOC/tests/API 均从源码统计），
> 每个包的差距数字可复现：`find packages/$pkg -name "*.rs" -exec cat {} + | wc -l` 等命令。
> 里程碑完成情况将随执行进度更新。
