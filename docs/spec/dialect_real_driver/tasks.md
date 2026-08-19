# sz-orm Informix/SAP HANA/Firebird 真实驱动集成编码任务分解

> 任务编号：TASK-003
> 对应需求规格：`docs/spec/dialect_real_driver/spec.md`（REQ-DIA-001 ~ REQ-DIA-014）
> 对应技术设计：`docs/spec/dialect_real_driver/design.md`
> 版本基线：v4.9.0
> 日期：2026-08-19
> 目标：评估 Informix/SAP HANA/Firebird 三方言在 Rust 生态的驱动 crate 可用性，对每方言做出"集成真实驱动"或"标注 SQL generation only"决策，附客观证据

---

## 1. 驱动可用性调研

### 1.1 Informix 驱动 crate 调研
- [ ] 调用 crates.io API `GET https://crates.io/api/v1/crates?q=informix` 搜索候选 crate（REQ-DIA-001）
- [ ] 对每个候选 crate 采集 9 项字段：名称/crates.io URL/最新版本/最后更新时间/下载量/是否 async/是否连接池/CI 状态/维护状态（REQ-DIA-002）
- [ ] 调用 GitHub API 查询 repo `archived` 标志 + 最近提交时间（> 1 年 → DEPRECATED，> 6 月 → 需进一步评估）
- [ ] 禁止凭 crate README 自述判定，必须基于 crates.io 下载量/GitHub 提交/CI 客观证据（REQ-DIA-003）
- [ ] 记录调研结果到 `docs/spec/dialect_real_driver/driver-survey.md` Informix 章节
- **依赖**：无
- **验证方法**：driver-survey.md 含 Informix 章节；每 crate 含 9 项字段；附 crates.io URL + GitHub URL 客观证据
- **预估工作量**：1.5h

### 1.2 SAP HANA 驱动 crate 调研
- [ ] 调用 crates.io API `GET https://crates.io/api/v1/crates?q=hana` 搜索候选 crate（REQ-DIA-001）
- [ ] 采集 9 项字段 + GitHub 维护状态（同 1.1）
- [ ] 记录到 driver-survey.md SAP HANA 章节
- **依赖**：无
- **验证方法**：driver-survey.md 含 SAP HANA 章节；每 crate 含 9 项字段
- **预估工作量**：1.5h

### 1.3 Firebird 驱动 crate 调研
- [ ] 调用 crates.io API `GET https://crates.io/api/v1/crates?q=firebird` 搜索候选 crate（REQ-DIA-001）
- [ ] 采集 9 项字段 + GitHub 维护状态（同 1.1）
- [ ] 记录到 driver-survey.md Firebird 章节
- **依赖**：无
- **验证方法**：driver-survey.md 含 Firebird 章节；每 crate 含 9 项字段
- **预估工作量**：1.5h

---

## 2. 集成可行性评估与决策

### 2.1 Informix 可行性评估与决策
- [ ] 评估候选 crate：async 支持（依赖 tokio/async-std）+ 连接池支持（自带或配合 bb8/deadpool）+ 类型映射覆盖（Informix SERIAL/ROW）+ `cargo check` 编译兼容 + `cargo audit` RUSTSEC 漏洞（REQ-DIA-005/008/009）
- [ ] 二选一决策：有可行 crate → INTEGRATED；无候选/全废弃/不支持 async/有漏洞 → SQL_GENERATION_ONLY（REQ-DIA-004）
- [ ] 记录决策 + 客观依据到 driver-survey.md
- **依赖**：1.1
- **验证方法**：driver-survey.md Informix 章节含 Decision 字段（INTEGRATED 或 SQL_GENERATION_ONLY）+ 依据
- **预估工作量**：1h

### 2.2 SAP HANA 可行性评估与决策
- [ ] 评估候选 crate：async + 连接池 + 类型映射（HANA NVARCHAR/CE 函数）+ 编译兼容 + 漏洞检查
- [ ] 二选一决策 + 记录依据
- **依赖**：1.2
- **验证方法**：driver-survey.md SAP HANA 章节含 Decision + 依据
- **预估工作量**：1h

### 2.3 Firebird 可行性评估与决策
- [ ] 评估候选 crate：async + 连接池 + 类型映射（Firebird GENERATOR/SEQUENCE/BLOB）+ 编译兼容 + 漏洞检查
- [ ] 二选一决策 + 记录依据
- **依赖**：1.3
- **验证方法**：driver-survey.md Firebird 章节含 Decision + 依据
- **预估工作量**：1h

---

## 3. 真实驱动集成实施（仅对决策为 INTEGRATED 的方言）

### 3.1 添加驱动 crate 依赖（feature 门控）
- [ ] 对每个 INTEGRATED 方言，在 `packages/sz-orm-sqlx/Cargo.toml`（或新模块）添加驱动 crate 依赖，通过 feature 门控（如 `dialect-informix-driver`），默认不启用（REQ-DIA-007）
- [ ] 验证 `cargo check`（不启用 feature）编译成功，无三方言驱动依赖
- **依赖**：2.1, 2.2, 2.3
- **验证方法**：`cargo check -p sz-orm-sqlx` 成功；`cargo check -p sz-orm-sqlx --features dialect-informix-driver` 成功（若 Informix 集成）
- **预估工作量**：1h

### 3.2 实现 connect/query/execute 桥接
- [ ] 对每个 INTEGRATED 方言，在 sz-orm-sqlx 或新模块实现 `async fn connect(conn_str) -> Result<Connection>` 桥接，复用 sz-orm-core 连接池（REQ-DIA-005）
- [ ] 实现 `query(sql, params) -> Row` / `execute(sql, params) -> u64` 桥接，结果集转为 sz-orm-core Row
- [ ] 所有 SQL 参数化（AGENTS.md 约束）
- **依赖**：3.1
- **验证方法**：`grep "async fn connect" packages/sz-orm-sqlx/src/` 命中；`grep -rn "format!\|push_str" packages/sz-orm-sqlx/src/` 无 SQL 值拼接
- **预估工作量**：2h

### 3.3 实现事务桥接
- [ ] 对每个 INTEGRATED 方言，实现 `begin/commit/rollback` 事务桥接（REQ-DIA-005）
- [ ] 复用 sz-orm-core Transaction 能力
- **依赖**：3.2
- **验证方法**：`grep "fn begin\|fn commit\|fn rollback" packages/sz-orm-sqlx/src/` 命中
- **预估工作量**：1h

### 3.4 实现类型映射
- [ ] Informix：SERIAL → i64，ROW → Vec<i64>（REQ-DIA-008）
- [ ] SAP HANA：NVARCHAR → String，CE 函数返回值 → f64
- [ ] Firebird：GENERATOR → i64，SEQUENCE → i64，BLOB → Vec<u8>
- [ ] 类型映射单元测试：方言特有类型正确往返
- **依赖**：3.2
- **验证方法**：`cargo test -p sz-orm-sqlx type_map` 全通过
- **预估工作量**：1.5h

### 3.5 E2E 测试（连接真实数据库）
- [ ] 对每个 INTEGRATED 方言，新增 E2E 测试：连接真实 DB → 建表 → insert → find → update → delete → 事务提交/回滚往返（REQ-DIA-006）
- [ ] 若数据库服务器未启动则标记"需数据库服务器"，跳过但不失败（REQ-DIA-006 异常场景）
- [ ] 测试后清理临时表/数据
- **依赖**：3.3, 3.4
- **验证方法**：`cargo test -p sz-orm-sqlx --features dialect-informix-driver -- --ignored` 全通过（若 DB 启动）；跳过时标记 SKIP
- **预估工作量**：2h

### 3.6 驱动安全审计
- [ ] 对每个 INTEGRATED 方言的驱动 crate 执行 `cargo audit` 检查 RUSTSEC 漏洞（REQ-DIA-009）
- [ ] 若发现漏洞则拒绝集成，改决策为 SQL_GENERATION_ONLY，更新 driver-survey.md
- **依赖**：3.1
- **验证方法**：`cargo audit` 退出码 0；driver-survey.md 决策与审计结果一致
- **预估工作量**：0.5h

---

## 4. SQL generation only 标注实施（仅对决策为 SQL_GENERATION_ONLY 的方言）

### 4.1 db_type.rs 代码注释标注
- [ ] 对每个 SQL_GENERATION_ONLY 方言，在 `packages/sz-orm-core/src/db_type.rs` 该方言枚举变体上添加注释 `// SQL generation only: 仅 SQL 生成，无真实驱动连接`（REQ-DIA-010）
- **依赖**：2.1, 2.2, 2.3
- **验证方法**：`grep "SQL generation only" packages/sz-orm-core/src/db_type.rs` 命中（标注方言数）
- **预估工作量**：0.3h

### 4.2 dialect.rs 代码注释标注
- [ ] 对每个 SQL_GENERATION_ONLY 方言，在 `packages/sz-orm-core/src/dialect.rs` 该方言 SQL 生成分支添加注释 `// SQL generation only`（REQ-DIA-011）
- **依赖**：2.1, 2.2, 2.3
- **验证方法**：`grep "SQL generation only" packages/sz-orm-core/src/dialect.rs` 命中
- **预估工作量**：0.3h

### 4.3 文档标注
- [ ] 在 `docs/sz-orm与同类产品对比分析.md` 2.3 节标注三方言"SQL generation only"（REQ-DIA-012）
- [ ] 在 `README.md` 方言列表标注三方言"SQL generation only"
- **依赖**：2.1, 2.2, 2.3
- **验证方法**：`grep "SQL generation only" docs/sz-orm与同类产品对比分析.md README.md` 命中
- **预估工作量**：0.5h

### 4.4 标注一致性校验
- [ ] grep 三处标注（db_type.rs + dialect.rs + 文档/README），措辞一致（REQ-DIA-013）
- [ ] 若某处遗漏则补齐
- **依赖**：4.1, 4.2, 4.3
- **验证方法**：三处标注措辞完全一致；无遗漏
- **预估工作量**：0.3h

---

## 5. SQL 生成层保留验证

### 5.1 既有 SQL 生成测试回归
- [ ] 执行 `cargo test -p sz-orm-core --features dialect-informix,dialect-saphana,dialect-firebird` 确认三方言 SQL 生成测试全通过（REQ-DIA-014）
- [ ] 确认既有 25 种其他方言测试不受影响（REQ-DIA-014）
- **依赖**：3.5, 4.4
- **验证方法**：cargo test 退出码 0；测试计数 ≥ 既有数
- **预估工作量**：1h

### 5.2 SQL 生成层不变验证
- [ ] `git diff` 确认 dialect.rs 仅新增注释，未修改 SQL 生成逻辑（REQ-DIA-014）
- [ ] `git diff` 确认 db_type.rs 仅新增注释，未修改枚举变体
- **依赖**：5.1
- **验证方法**：`git diff` 仅含注释新增行，无逻辑变更
- **预估工作量**：0.3h

---

## 6. feature 门控验证

### 6.1 默认不启用验证
- [ ] 执行 `cargo check -p sz-orm-core` 确认不启用 feature 时编译成功，无三方言驱动依赖（REQ-DIA-007）
- [ ] 执行 `cargo check -p sz-orm-sqlx` 确认不启用 feature 时编译成功
- **依赖**：3.1, 4.4
- **验证方法**：cargo check 退出码 0；`grep` target/ 无三方言驱动符号
- **预估工作量**：0.5h

### 6.2 feature 启用验证（若集成）
- [ ] 对每个 INTEGRATED 方言，执行 `cargo check -p sz-orm-sqlx --features dialect-<name>-driver` 确认编译成功
- **依赖**：3.1
- **验证方法**：cargo check 各 feature 退出码 0
- **预估工作量**：0.5h

---

## 7. 交付记录与文档

### 7.1 生成交付记录
- [ ] 生成 `docs/spec/dialect_real_driver/delivery-record.md`，含：三方言决策结果（INTEGRATED/SQL_GENERATION_ONLY）+ 调研证据（crates.io/GitHub URL）+（若集成）E2E 结果 +（若标注）标注位置清单（file:line）（REQ-DIA-014）
- **依赖**：5.1, 6.1
- **验证方法**：delivery-record.md 存在且内容完整；含 file:line 证据
- **预估工作量**：0.5h

### 7.2 更新对比分析文档
- [ ] 更新 `docs/sz-orm与同类产品对比分析.md` 2.3 节：三方言从"仅 SQL 生成层，无真实数据库驱动连接"更新为决策结果（集成 → "已集成真实驱动 <crate-name>"；标注 → "SQL generation only"）
- **依赖**：4.3, 3.5
- **验证方法**：grep 对比文档含三方言决策结果
- **预估工作量**：0.3h

---

## 8. 审查与确认

### 8.1 五维审查
- [ ] 正确性：三方言决策基于客观证据；集成方案 E2E 通过；标注方案三处一致
- [ ] 可读性：driver-survey.md 结构清晰，每 crate 9 项字段完整
- [ ] 架构：集成方案复用既有连接池 + Connection trait；标注方案不修改 SQL 生成逻辑
- [ ] 安全性：驱动 crate 无 RUSTSEC 漏洞；SQL 参数化
- [ ] 性能：（若集成）查询往返 < 50ms；连接池复用率 ≥ 95%
- **依赖**：7.1, 7.2
- **验证方法**：审查清单逐项确认，附 file:line 证据
- **预估工作量**：0.5h

### 8.2 变更范围确认
- [ ] 确认仅修改 driver-survey.md + delivery-record.md + 对比分析文档 + README + db_type.rs/dialect.rs 注释 +（若集成）sz-orm-sqlx 驱动适配层
- [ ] 确认未新增 workspace 成员
- [ ] 确认未修改既有 25 种其他方言行为
- **依赖**：8.1
- **验证方法**：`git diff --name-only` 仅含上述文件
- **预估工作量**：0.2h

---

## 任务依赖关系

```
1.1 → 2.1 → 3.1 → 3.2 → 3.3 → 3.5 → 5.1 → 5.2 → 7.1 → 8.1 → 8.2
1.2 → 2.2 → 3.1
1.3 → 2.3 → 3.1
2.1 → 4.1 → 4.4 → 5.1
2.2 → 4.2 → 4.4
2.3 → 4.3 → 4.4
3.2 → 3.4 → 3.5
3.1 → 3.6
4.4 → 6.1 → 6.2
3.5 → 7.2
4.3 → 7.2
```

## 任务统计

- 主任务：8 组
- 子任务：22 个
- 需求覆盖：REQ-DIA-001 ~ REQ-DIA-014 全部 14 项
- 预估总工作量：约 18h（含调研 + 集成 + E2E）
- 注：集成任务（3.x）仅对决策为 INTEGRATED 的方言执行；标注任务（4.x）仅对决策为 SQL_GENERATION_ONLY 的方言执行