# sz-orm v2.4.0 任务分解文档

> 版本：v2.4.0
> 基线：v2.3.0（已全部完成）
> 日期：2026-08-07
> 文档定位：任务分解（可执行任务清单），对应需求规格 `spec.md`（33 条 EARS 需求）与技术设计 `design.md`
> 任务总数：7 个主任务 / 35 个子任务，覆盖全部 33 条需求（REQ-IT-001~013 / REQ-PB-001~010 / REQ-REL-001~010）

---

## 任务规划原则

1. **垂直切割**：按业务功能分组（集成测试 / 性能基准 / 发布 / 下游验证），非按技术层次分组
2. **可验收**：每个子任务标注对应需求编号与验收标准，可独立判定完成
3. **原子性**：一个子任务只做一件事，标注涉及文件路径
4. **有序性**：被依赖任务在前（基础设施 → 测试套件 → 基准套件 → 发布准备 → 发布执行 → 下游验证 → 收尾）

---

## 1. 搭建集成测试基础设施

**对应需求**：REQ-IT-001~006, 012（等价性断言工具与策略断言工具）
**依赖**：无（v2.3.0 已交付 SmartEagerLoader / EagerLoader / StrategyResolver / NestedEagerResult）
**目标**：为五方言集成测试提供等价性断言工具与测试数据构造器，是任务 2 的前置依赖

### 1.1 实现等价性断言工具 assert_eager_equivalent
- [ ] 在 `packages/sz-orm-core/tests/common/equivalence.rs` 新增 `assert_eager_equivalent(smart_results, manual_results, relation_kind)` 函数，逐行逐字段比对智能与手动加载结果集（行数、字段名、字段值），HasMany/ManyToMany 子集合按外键值无序排序后比对（避免方言默认排序差异导致假阴性）
- [ ] 断言失败时 `panic!` 输出差异明细（差异行号、字段名、期望值 vs 实际值），满足 spec §5.1.3 异常场景 2
- [ ] 测试数据使用整型/字符串避免浮点精度问题（风险 R-10 缓解）
- **对应需求**：REQ-IT-001, REQ-IT-002, REQ-IT-003
- **验收标准**：函数编译通过，对相等结果集断言通过、对差异结果集 panic 输出明细

### 1.2 实现策略选择断言工具 assert_strategy_selected
- [ ] 在 `packages/sz-orm-core/tests/common/equivalence.rs` 新增 `assert_strategy_selected(decision, expected)` 函数，断言 `StrategyDecision.strategy == expected`，不符则 panic 输出 relation_name / actual / expected / reason
- **对应需求**：REQ-IT-004, REQ-IT-005, REQ-IT-006
- **验收标准**：对 HasOne→Join、HasMany→DataLoader、ManyToMany→IntermediateTableBatch 三种决策断言通过

### 1.3 实现嵌套深度断言工具 assert_nested_depth_equal
- [ ] 在 `packages/sz-orm-core/tests/common/equivalence.rs` 新增 `assert_nested_depth_equal(smart_tree, manual_tree)` 函数，递归比对 `NestedEagerResult` 树（节点类型 Leaf/Node 一致、逐层 children 数量一致、逐字段 row 比对、max_depth 一致）
- [ ] 递归深度限 3 级避免栈溢出（design §2.4.2 算法 2）
- **对应需求**：REQ-IT-012
- **验收标准**：对相同嵌套树断言通过、对深度/节点数差异 panic 输出差异层

### 1.4 实现测试数据构造器 TestSchemaBuilder
- [ ] 在 `packages/sz-orm-core/tests/common/schema_builder.rs` 新增 `TestSchemaBuilder` 结构体，含 `new(dialect, conn)` / `build()` / `seed()` / `teardown()` 方法
- [ ] `build()` 按方言生成 DDL 建表（users/orders/profiles/roles/user_roles 五表），方言感知（MySQL/PG/SQLite/Oracle/MSSQL 各自 DDL 语法）
- [ ] `seed()` 插入测试数据：5 条 users（覆盖空关联 user3、单条关联 user4、多条关联 user1/5）+ 10 条 orders + 3 条 profiles + 3 条 roles + 6 条 user_roles，满足 spec §6.1 ≥5 主 + ≥10 关联 + 边界情况
- [ ] `teardown()` 执行 `DROP TABLE IF EXISTS` 清理（Oracle 23ai 支持），即使断言失败也清理避免残留（spec §6.1.3）
- **对应需求**：REQ-IT-001~013（数据约束支撑）
- **验收标准**：五方言下 build+seed+teardown 均成功，数据满足边界覆盖

---

## 2. 实现五方言集成测试套件

**对应需求**：REQ-IT-001~013（13 条）
**依赖**：任务 1（等价性断言工具 + 测试数据构造器）
**目标**：五方言 × 三关联类型 × 三策略 = 45 个等价性验证点全部通过

### 2.1 实现 MySQL 方言集成测试
- [ ] 在 `packages/sz-orm-core/tests/smart_eager_integration_mysql.rs` 新增 7 个测试函数：`test_hasone_equivalent_mysql` / `test_hasmany_equivalent_mysql` / `test_many_to_many_equivalent_mysql` / `test_join_strategy_mysql` / `test_dataloader_strategy_mysql` / `test_intermediate_strategy_mysql` / `test_nested_depth_mysql`
- [ ] 连接 `mysql://root:test123@127.0.0.1:3306/sz_orm_test`，需真实服务测试标注 `#[ignore]`，通过 `cargo test -- --ignored` 触发
- [ ] 每测试函数执行：TestSchemaBuilder.build+seed → smart() 加载 → 手动 with() 加载 → assert_eager_equivalent → teardown
- **对应需求**：REQ-IT-007（方言覆盖）+ REQ-IT-001~006,012（等价性）
- **验收标准**：`cargo test --test smart_eager_integration_mysql -- --ignored` 全部通过

### 2.2 实现 PostgreSQL 方言集成测试
- [ ] 在 `packages/sz-orm-core/tests/smart_eager_integration_pg.rs` 新增同 2.1 的 7 个测试函数（`_pg` 后缀），连接 `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- **对应需求**：REQ-IT-008
- **验收标准**：`cargo test --test smart_eager_integration_pg -- --ignored` 全部通过

### 2.3 实现 SQLite 方言集成测试
- [ ] 在 `packages/sz-orm-core/tests/smart_eager_integration_sqlite.rs` 新增同 2.1 的 7 个测试函数（`_sqlite` 后缀），连接 `sqlite::memory:`（in-memory 无外部依赖，不标注 `#[ignore]` 默认执行）
- **对应需求**：REQ-IT-009
- **验收标准**：`cargo test --test smart_eager_integration_sqlite` 全部通过（默认执行无需 --ignored）

### 2.4 实现 Oracle 方言集成测试
- [ ] 在 `packages/sz-orm-core/tests/smart_eager_integration_oracle.rs` 新增同 2.1 的 7 个测试函数（`_oracle` 后缀），连接 `oracle://sys:test123@127.0.0.1:1521/freepdb1`（Sysdba 权限），标注 `#[ignore]`
- [ ] Oracle IN 列表 >1000 分批查询已由 eager_loader.rs 实现，验证不报错
- **对应需求**：REQ-IT-010
- **验收标准**：`cargo test --test smart_eager_integration_oracle -- --ignored` 全部通过

### 2.5 实现 MSSQL 方言集成测试
- [ ] 在 `packages/sz-orm-core/tests/smart_eager_integration_mssql.rs` 新增同 2.1 的 7 个测试函数（`_mssql` 后缀），连接远程 `sh-mssql-adrul9nm.sql.tencentcdb.com:22527`，标注 `#[ignore]`
- **对应需求**：REQ-IT-011
- **验收标准**：`cargo test --test smart_eager_integration_mssql -- --ignored` 全部通过（远程可用时）
- **风险标记**：R-01（MSSQL 远程连接不稳定），不可用时按 2.6 标注跳过

### 2.6 实现方言跳过标注机制
- [ ] 在各方言测试连接处使用 `unwrap_or_else(|e| ignore!("方言不可用: {e}"))` 模式，连接失败时标注 `#[ignore]` 并通过 `tracing::warn!` 记录跳过原因，禁止静默通过
- [ ] 验证测试报告中跳过方言明确标注原因（非静默 PASS）
- **对应需求**：REQ-IT-013
- **验收标准**：方言环境不可用时测试标注 ignore + 报告记录跳过原因，非静默通过

### 2.7 验证三关联类型等价性（跨方言汇总）
- [ ] 确认 HasOne 关联在五方言下 SmartEagerLoader 结果集与手动 EagerLoader 完全等价（行数/字段/值/嵌套结构逐行逐字段比对）
- [ ] 确认 HasMany 关联在五方言下结果集等价（子记录集合按外键无序比对）
- [ ] 确认 ManyToMany 关联（含中间表）在五方言下结果集等价（经中间表关联记录无序比对）
- **对应需求**：REQ-IT-001, REQ-IT-002, REQ-IT-003
- **验收标准**：五方言三关联类型共 15 个等价性验证点全部通过

### 2.8 验证三策略独立覆盖
- [ ] 验证 Join 策略：HasOne/BelongsTo 关联触发 Join 策略，`assert_strategy_selected(decision, LoadStrategy::Join)` 通过，执行结果 == 手动单次 JOIN 查询
- [ ] 验证 DataLoader 策略：HasMany 关联触发 DataLoader 策略，执行结果 == 手动逐条查询合并结果
- [ ] 验证 IntermediateTableBatch 策略：ManyToMany 有中间表触发 IntermediateTableBatch，执行结果 == 手动经中间表查询；无中间表时回退 DataLoader + `tracing::warn!` 告警（design §2.4.1 决策矩阵）
- **对应需求**：REQ-IT-004, REQ-IT-005, REQ-IT-006
- **验收标准**：三策略独立覆盖验证全部通过，回退分支告警日志存在

---

## 3. 实现性能基准套件

**对应需求**：REQ-PB-001~010（10 条）
**依赖**：任务 1（复用等价性断言思路）；v2.3.0 已交付 StrategyResolver / N1Eliminator / BenchmarkReporter
**目标**：决策延迟 ≤ 100μs、智能 vs 手动退化 ≤ 10%、N+1 消除生效，四规模全覆盖

### 3.1 实现基准数据生成/清理工具 SmartEagerBenchHarness
- [ ] 在 `bench-comparison/benches/smart_eager_harness.rs` 新增 `SmartEagerBenchHarness` 结构体，含 `new(conn)` / `setup(scale)` / `teardown(scale)` 方法
- [ ] `setup(scale)` 按规模 N ∈ {10, 100, 1000, 10000} 生成主表 N 条 + 关联表 ≈N 条，外键均匀分布（每主记录关联子记录数 ≈ N/主记录数），满足 spec §6.2 规模档位与数据分布
- [ ] 使用 SQLite in-memory 避免外部依赖与网络/DB 负载干扰（design §2.5.2.4）
- **对应需求**：REQ-PB-004, REQ-PB-005, REQ-PB-006, REQ-PB-007
- **验收标准**：四规模 setup+teardown 均成功，外键分布均匀

### 3.2 实现决策延迟基准
- [ ] 在 `bench-comparison/benches/bench_smart_eager.rs` 新增 `bench_decision_latency` 基准组，criterion 采样 ≥100 次 `StrategyResolver::resolve()` 调用，统计 P50/P95/P99/Max 墙钟耗时
- [ ] 断言 P99 ≤ 100μs，超标时基准报告标注决策延迟超标 + 实际 P99 值 + 超标幅度（spec §5.2.3 异常场景 1）
- **对应需求**：REQ-PB-001
- **验收标准**：P99 决策延迟 ≤ 100μs，报告含 P50/P95/P99/Max 统计
- **风险标记**：R-04（系统噪声致 P99 偶发超标），criterion 配置统计平滑缓解

### 3.3 实现智能 vs 手动对比基准
- [ ] 在 `bench-comparison/benches/bench_smart_eager.rs` 新增 `bench_smart_vs_manual` 基准组，四规模分别用 smart() 与手动 with() 加载相同数据集并计时
- [ ] 断言智能耗时 / 手动耗时 ≤ 1.10（退化容忍 10%），超标时报告标注性能退化 + 退化比例 + 规模维度（spec §5.2.3 异常场景 2）
- **对应需求**：REQ-PB-002
- **验收标准**：四规模智能/手动耗时比 ≤ 1.10，报告含两者均值/中位数/P99

### 3.4 实现 N+1 消除对比基准
- [ ] 在 `bench-comparison/benches/bench_smart_eager.rs` 新增 `bench_n1_elimination` 基准组，四规模分别执行逐条查询（N+1 次）与 N1Eliminator 批量合并（1 次），对比查询次数与耗时
- [ ] 断言消除后查询次数 < 消除前，未减少时报告标注消除无效 + 前后查询次数（spec §5.2.3 异常场景 3）
- **对应需求**：REQ-PB-003
- **验收标准**：四规模消除后查询次数 < 消除前，耗时降幅可量化

### 3.5 验证四规模数据集覆盖
- [ ] 确认规模 10 / 100 / 1000 / 10000 四档均执行完整三类基准（决策延迟 + 智能 vs 手动 + N+1 消除），规模精确无近似（spec §6.2.1）
- [ ] 10000 规模独立超时阈值，超时不影响其他规模（spec §5.2.3 异常场景 4）
- **对应需求**：REQ-PB-004, REQ-PB-005, REQ-PB-006, REQ-PB-007
- **验收标准**：四规模三类基准全部执行并产出数据
- **风险标记**：R-05（10000 规模插入超时），独立超时阈值缓解

### 3.6 验证 criterion 配置合规
- [ ] 在基准代码显式设置 `sample_size(100)` / `warm_up_time(Duration::from_secs(3))` / `measurement_time(Duration::from_secs(10))`，确保不被覆盖
- [ ] 确认 `CriterionConfig` 默认值已合规（design §2.5.2.2）
- **对应需求**：REQ-PB-009
- **验收标准**：sample_size ≥ 100 ∧ warm_up ≥ 3s ∧ measurement ≥ 10s

### 3.7 实现基准报告生成与规模缺失标注
- [ ] 复用存量 `BenchmarkReporter`，新增 SmartEager 维度记录（smart_eager_decision_latency / smart_vs_manual / n1_elimination），覆盖四规模
- [ ] 生成 Markdown + CSV/JSON 格式报告，DSN 脱敏（BenchmarkReporter 已实现），含时间戳/CPU/Rust 版本/DB 版本环境信息（spec §6.2.4 可复现）
- [ ] 基准执行后检查四规模数据完整性，缺失则在报告 `missing_dimensions` 标注 + 原因，禁止静默缺失（spec §5.2.1 规则 10）
- **对应需求**：REQ-PB-008, REQ-PB-010
- **验收标准**：产出 Markdown + CSV/JSON 报告，DSN 已脱敏，缺失规模标注原因非静默

---

## 4. 实现发布前门禁与拓扑排序

**对应需求**：REQ-REL-002, REQ-REL-003, REQ-REL-007, REQ-REL-008
**依赖**：无（可与任务 2、任务 3 并行）
**目标**：发布前 10 道门禁全通过、依赖拓扑顺序可计算、版本号一致、凭证安全

### 4.1 实现依赖拓扑排序脚本
- [ ] 在 `scripts/compute_topology.ps1` 新增拓扑排序脚本，解析 workspace 43 包各 `Cargo.toml` 的 sz-orm-* 依赖（path 与 version 依赖均计入），构建 DAG，用 Kahn 算法变体拓扑排序（入度相同按包名字典序打破并列，确保唯一可复现）
- [ ] 检测循环依赖并报错（sz-orm 内部不应有循环依赖，design §2.4.3 步骤 6）
- [ ] 输出拓扑序包名列表（被依赖的包在前，stdout 每行一个包名）
- **对应需求**：REQ-REL-002
- **验收标准**：输出 43 包拓扑序，对任意包 P 其所有 sz-orm-* 依赖在 P 之前出现
- **风险标记**：R-11（循环依赖），算法检测并报错缓解

### 4.2 验证版本号一致性
- [ ] 确认 workspace 根 `Cargo.toml` 的 `workspace.package.version = "2.3.0"`，所有子包 `version.workspace = true`，无个别包版本偏差
- [ ] 确认 license = MIT、repository = `https://github.com/ljclz/sz-orm` 与 workspace 一致（spec §6.3）
- **对应需求**：REQ-REL-003
- **验收标准**：全包 version = 2.3.0 ∧ version.workspace = true

### 4.3 配置凭证安全
- [ ] 确认 crates.io token 通过环境变量 `CARGO_REGISTRY_TOKEN` 或 `cargo login` 传入，发布脚本不硬编码 token
- [ ] 验证 token 不出现在 git 跟踪文件中：`git log --all -p` 审计无 token 字面量
- **对应需求**：REQ-REL-007
- **验收标准**：token 不入版本控制，通过环境变量传入
- **风险标记**：R-12（token 泄露），环境变量传入缓解

### 4.4 执行发布前门禁全通过检查
- [ ] 复用 `scripts/gate.ps1`（AGENTS.md 已定义），执行 10 道门禁：fmt / check / clippy / test / doc / audit / integration（--ignored）/ 占位检查 / SQL 注入扫描 / Feature 全组合
- [ ] 任一门禁 FAIL → 阻断发布，输出失败门禁名称与详情（spec §5.3.3 异常场景 1）
- [ ] 确认无占位实现（`grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` 无结果）、无 unsafe（零容忍）、clippy 零警告
- **对应需求**：REQ-REL-008
- **验收标准**：10 道门禁全部 PASS，任一 FAIL 阻断发布

---

## 5. 执行 crates.io 逐包发布

**对应需求**：REQ-REL-001, REQ-REL-009, REQ-REL-010
**依赖**：任务 2（集成测试通过）+ 任务 3（基准通过）+ 任务 4（门禁通过 + 拓扑序 + 凭证）
**目标**：43 包按拓扑序全部发布到 crates.io，版本 2.3.0，失败即中止

### 5.1 实现逐包发布脚本
- [ ] 在 `scripts/publish_crates_io.ps1` 新增发布脚本，参数 `-WorkspaceRoot` + `-DryRun`，流程：门禁检查 → 检查 token → 计算拓扑序 → 逐包 `cargo publish -p <pkg>` → sz-pay 验证
- [ ] 任一包发布失败立即 `exit 1` 中止后续发布，输出失败包名 + 已发布包列表 + 错误详情（REQ-REL-009 禁止部分发布）
- [ ] 支持 `-DryRun` 模式仅打印 `cargo publish` 命令不实际执行
- **对应需求**：REQ-REL-001, REQ-REL-009
- **验收标准**：脚本按拓扑序逐包发布，失败即中止，输出可追溯
- **风险标记**：R-06（发布中途失败），失败即中止策略缓解；R-07（版本已存在），发布前检查缓解

### 5.2 执行 43 包发布
- [ ] 设置 `CARGO_REGISTRY_TOKEN` 环境变量，执行 `scripts/publish_crates_io.ps1 -WorkspaceRoot "E:\vue\test\鲜视达\rust\sz-orm"`
- [ ] 确认 43 包（41 lib + cli + examples，cli/examples 若 publish=false 则跳过并记录）全部在 crates.io 上可见且版本号 2.3.0
- [ ] 发布前检查各包 2.3.0 版本未已存在（`cargo search` 或 crates.io API），已存在则跳过该包并记录（R-07 缓解）
- **对应需求**：REQ-REL-001
- **验收标准**：43 包在 crates.io 可见 ∧ 版本号 == 2.3.0

### 5.3 验证禁止覆盖上游业务代码
- [ ] 确认发布过程仅修改版本号/发布元数据，执行 `git diff --name-only HEAD` 检查 sz-orm 仓库变更文件，无业务代码变更（ADR-0001 铁律）
- [ ] 确认发布脚本不修改 sz-orm 仓库业务代码，仅修改 sz-pay 自身 Cargo.toml（任务 6）
- **对应需求**：REQ-REL-010
- **验收标准**：git diff 仅含版本号/发布元数据变更，无业务代码变更

---

## 6. sz-pay 下游验证

**对应需求**：REQ-REL-004, REQ-REL-005, REQ-REL-006
**依赖**：任务 5（43 包已发布到 crates.io）
**目标**：sz-pay 从 crates.io 拉取 v2.3.0 构建成功，移除 patch 段后零回归

### 6.1 升级 sz-pay 依赖版本
- [ ] 在 `scripts/verify_sz_pay.ps1` 新增 sz-pay 验证脚本，将 `E:\vue\test\sz-pay\server\sz-rust\Cargo.toml` 中 7 个 sz-orm-* 依赖（core/sqlx/config/auth/macros/queue/scheduler）版本从 2.1.0 升级到 2.3.0
- [ ] 使用文本替换修改版本号（非 PowerShell 重定向，遵守用户约束），不修改 sz-pay 业务代码
- **对应需求**：REQ-REL-004
- **验收标准**：7 个依赖版本号变为 2.3.0，`cargo build` 成功无编译错误

### 6.2 移除 [patch.crates-io] 本地覆盖
- [ ] 删除 sz-pay `Cargo.toml` 中 `[patch.crates-io]` 段（7 个 sz-orm-* 本地路径覆盖行），使依赖来源变为 crates.io 正式制品
- [ ] 执行 `cargo build --manifest-path <sz-pay Cargo.toml>` 验证构建成功，依赖来源为 crates.io 而非本地路径
- [ ] 构建失败时保留 patch 段不删除（git checkout 恢复），输出编译错误详情（spec §5.3.3 异常场景 4）
- **对应需求**：REQ-REL-005
- **验收标准**：移除 patch 段后 cargo build 成功，依赖来源为 crates.io
- **风险标记**：R-08（移除 patch 后构建失败），保留 patch 回退机制缓解

### 6.3 执行 sz-pay 回归验证
- [ ] 执行 `cargo test --manifest-path <sz-pay Cargo.toml>` 回归测试，确认业务行为与本地覆盖期间一致（零回归）
- [ ] 回归失败时输出失败测试详情，标记发布验证未完成，保留 patch 段（spec §5.3.3 异常场景 5）
- **对应需求**：REQ-REL-006
- **验收标准**：sz-pay 回归测试全部通过，业务行为不变（零回归）
- **风险标记**：R-09（回归失败），v2.4.0 无 Breaking Change + 不改 SmartEagerLoader 业务功能保证零回归

---

## 7. 集成验证与文档收尾

**对应需求**：AC-ALL-1~5（总体验收标准）
**依赖**：任务 2 + 任务 3 + 任务 5 + 任务 6（全部功能任务完成）
**目标**：全 workspace 质量门禁通过、API 兼容、文档更新、需求追溯核对

### 7.1 全 workspace 测试通过
- [ ] 执行 `cargo test --workspace` 确认全部通过，零失败、零忽略（除明确标注 `#[ignore]` 的 soak/jepsen 长时测试）
- [ ] 执行 `cargo test --workspace -- --ignored` 确认五方言集成测试（任务 2）全部通过
- **对应需求**：AC-ALL-2
- **验收标准**：cargo test --workspace 全部通过

### 7.2 clippy 零警告与格式检查
- [ ] 执行 `cargo clippy --workspace --all-targets -- -D warnings` 确认零警告
- [ ] 执行 `cargo fmt --all -- --check` 确认格式合规
- **对应需求**：AC-ALL-3
- **验收标准**：clippy 零警告，fmt 格式合规

### 7.3 API 向后兼容验证
- [ ] 确认 v2.4.0 无 Breaking Change，v2.3.0 公开 API（SmartEagerLoader / EagerLoader / StrategyResolver / N1Eliminator 等签名）全部保持不变
- [ ] 确认新增接口均为测试/基准/脚本内部接口，不进入 sz-orm-core 公开 API（design §2.2.1）
- **对应需求**：AC-ALL-1
- **验收标准**：无 Breaking Change，v2.3.0 公开 API 全部保持不变

### 7.4 更新 CHANGELOG.md
- [ ] 在 `CHANGELOG.md` 新增 v2.4.0 变更记录：SmartEagerLoader 五方言集成测试套件、性能基准套件、crates.io v2.3.0 发布、sz-pay 下游验证
- **对应需求**：AC-ALL-4
- **验收标准**：CHANGELOG.md 含 v2.4.0 变更记录

### 7.5 需求追溯矩阵核对
- [ ] 核对 spec.md 第 7 章需求追溯矩阵，确认 33 条需求（REQ-IT-001~013 / REQ-PB-001~010 / REQ-REL-001~010）全部映射到任务且验收通过
- [ ] 确认 spec §9 验收标准总览中 AC-IT-1~6 / AC-PB-1~6 / AC-REL-1~8 / AC-ALL-1~5 全部满足
- **对应需求**：AC-ALL-5
- **验收标准**：33 条需求全部满足，验收标准总览全部勾选

---

## 里程碑规划

| 里程碑 | 名称 | 包含任务 | 交付物 | 对应需求 |
|--------|------|---------|--------|---------|
| M1 | 集成测试基础设施 | 任务 1 | equivalence.rs + schema_builder.rs | REQ-IT-001~006,012 |
| M2 | 五方言集成测试套件 | 任务 1 + 任务 2 | 5 个集成测试文件 + 45 验证点通过 | REQ-IT-001~013 |
| M3 | 性能基准套件 | 任务 1 + 任务 3 | bench_smart_eager.rs + harness + 报告 | REQ-PB-001~010 |
| M4 | 发布前准备 | 任务 4 | 拓扑排序脚本 + 门禁通过 + 凭证配置 | REQ-REL-002,003,007,008 |
| M5 | crates.io 发布 | 任务 2 + 任务 3 + 任务 4 + 任务 5 | 43 包发布到 crates.io v2.3.0 | REQ-REL-001,009,010 |
| M6 | sz-pay 下游验证 | 任务 5 + 任务 6 | sz-pay 依赖升级 + patch 移除 + 零回归 | REQ-REL-004,005,006 |
| M7 | v2.4.0 交付完成 | 任务 6 + 任务 7 | 全门禁通过 + CHANGELOG + 追溯核对 | AC-ALL-1~5 |

**里程碑时序**：M1 → (M2 ∥ M3) → M4 → M5 → M6 → M7
- M1 为基础设施，M2 与 M3 可并行（均依赖 M1），M4 可与 M2/M3 并行（无依赖），M5 需 M2+M3+M4 全完成，M6 需 M5，M7 需 M6

---

## 任务依赖图

```
                        ┌─────────────────────────┐
                        │  任务 1：集成测试基础设施  │
                        │  (equivalence + schema)  │
                        └────────────┬────────────┘
                                     │
                    ┌────────────────┼────────────────┐
                    ▼                                  ▼
       ┌────────────────────┐              ┌────────────────────┐
       │ 任务 2：五方言集成   │              │ 任务 3：性能基准    │
       │ 测试套件 (5 文件)   │              │ 套件 (3 类基准)    │
       └──────────┬─────────┘              └──────────┬─────────┘
                  │                                   │
                  │                                   │
                  │     ┌─────────────────────┐      │
                  │     │ 任务 4：发布前门禁   │      │
                  │     │ + 拓扑排序 + 凭证    │      │
                  │     └──────────┬──────────┘      │
                  │                │                 │
                  └────────────────┼─────────────────┘
                                   ▼
                      ┌─────────────────────┐
                      │ 任务 5：crates.io   │
                      │ 逐包发布 (43 包)    │
                      └──────────┬──────────┘
                                 ▼
                      ┌─────────────────────┐
                      │ 任务 6：sz-pay     │
                      │ 下游验证           │
                      └──────────┬──────────┘
                                 ▼
                      ┌─────────────────────┐
                      │ 任务 7：集成验证    │
                      │ + 文档收尾          │
                      └─────────────────────┘
```

**依赖关系明细**：
- 任务 1 → 任务 2（等价性断言工具 + 数据构造器）
- 任务 1 → 任务 3（复用断言思路 + 数据构造）
- 任务 2 → 任务 5（发布前集成测试需通过）
- 任务 3 → 任务 5（发布前基准需通过）
- 任务 4 → 任务 5（门禁 + 拓扑序 + 凭证为发布前置）
- 任务 5 → 任务 6（43 包已发布后 sz-pay 才能拉取 v2.3.0）
- 任务 6 → 任务 7（下游验证后做最终收尾）
- 任务 2 → 任务 7，任务 3 → 任务 7（全量测试验证）

---

## 风险标记汇总

| 风险 ID | 关联任务 | 风险描述 | 严重度 | 缓解措施 |
|---------|---------|---------|--------|---------|
| R-01 | 任务 2.5 | MSSQL 远程连接不稳定 | 中 | 标注 #[ignore] + 记录跳过原因，四方言优先保障 |
| R-02 | 任务 1.4 | Oracle Sysdba 清理不彻底 | 中 | DROP TABLE IF EXISTS + 独立 schema 隔离 |
| R-03 | 任务 2.7 | 五方言排序差异致等价性假阴性 | 高 | 无序集合比对算法（按外键排序后比对） |
| R-04 | 任务 3.2 | 决策延迟 P99 偶发超 100μs | 中 | criterion sample_size=100 + warm_up=3s 统计平滑 |
| R-05 | 任务 3.5 | 10000 规模插入超时 | 低 | 独立超时阈值，不影响其他规模 |
| R-06 | 任务 5.1 | 发布中途某包失败 | 高 | 失败即中止，已发布包保持，失败包修复后重发 |
| R-07 | 任务 5.2 | 2.3.0 版本已存在 | 中 | 发布前检查，已存在则跳过 |
| R-08 | 任务 6.2 | sz-pay 移除 patch 后构建失败 | 高 | 保留 patch 段回退，排查 API 兼容性 |
| R-09 | 任务 6.3 | sz-pay 回归业务行为变化 | 高 | v2.4.0 无 Breaking Change 保证零回归，失败输出详情 |
| R-10 | 任务 1.1 | 浮点数等价性精度问题 | 低 | 测试数据用整型/字符串，必要时 epsilon 比对 |
| R-11 | 任务 4.1 | 拓扑排序遇循环依赖 | 高 | 算法检测循环并报错（sz-orm 内部不应有循环） |
| R-12 | 任务 4.3 | token 泄露到 git | 高 | 仅环境变量传入，发布脚本不含 token 字面量 |

**高严重度风险优先级**：R-03 > R-06 > R-08 = R-09 > R-11 = R-12

---

## 需求覆盖核对表

| 需求编号 | 对应任务 | 覆盖状态 |
|---------|---------|---------|
| REQ-IT-001 | 任务 1.1 + 任务 2.7 | ☐ |
| REQ-IT-002 | 任务 1.1 + 任务 2.7 | ☐ |
| REQ-IT-003 | 任务 1.1 + 任务 2.7 | ☐ |
| REQ-IT-004 | 任务 1.2 + 任务 2.8 | ☐ |
| REQ-IT-005 | 任务 1.2 + 任务 2.8 | ☐ |
| REQ-IT-006 | 任务 1.2 + 任务 2.8 | ☐ |
| REQ-IT-007 | 任务 2.1 | ☐ |
| REQ-IT-008 | 任务 2.2 | ☐ |
| REQ-IT-009 | 任务 2.3 | ☐ |
| REQ-IT-010 | 任务 2.4 | ☐ |
| REQ-IT-011 | 任务 2.5 | ☐ |
| REQ-IT-012 | 任务 1.3 + 任务 2.1~2.5 | ☐ |
| REQ-IT-013 | 任务 2.6 | ☐ |
| REQ-PB-001 | 任务 3.2 | ☐ |
| REQ-PB-002 | 任务 3.3 | ☐ |
| REQ-PB-003 | 任务 3.4 | ☐ |
| REQ-PB-004 | 任务 3.1 + 任务 3.5 | ☐ |
| REQ-PB-005 | 任务 3.1 + 任务 3.5 | ☐ |
| REQ-PB-006 | 任务 3.1 + 任务 3.5 | ☐ |
| REQ-PB-007 | 任务 3.1 + 任务 3.5 | ☐ |
| REQ-PB-008 | 任务 3.7 | ☐ |
| REQ-PB-009 | 任务 3.6 | ☐ |
| REQ-PB-010 | 任务 3.7 | ☐ |
| REQ-REL-001 | 任务 5.1 + 任务 5.2 | ☐ |
| REQ-REL-002 | 任务 4.1 | ☐ |
| REQ-REL-003 | 任务 4.2 | ☐ |
| REQ-REL-004 | 任务 6.1 | ☐ |
| REQ-REL-005 | 任务 6.2 | ☐ |
| REQ-REL-006 | 任务 6.3 | ☐ |
| REQ-REL-007 | 任务 4.3 | ☐ |
| REQ-REL-008 | 任务 4.4 | ☐ |
| REQ-REL-009 | 任务 5.1 | ☐ |
| REQ-REL-010 | 任务 5.3 | ☐ |

> 覆盖状态：☐ 待完成 / ✅ 已完成（任务执行时更新）

---

> **文档结束**
> 本文档为任务分解（可执行任务清单），对应需求规格 `spec.md`（33 条 EARS 需求）与技术设计 `design.md`。
> 7 个主任务 / 35 个子任务，按依赖关系排序，覆盖全部 33 条需求。