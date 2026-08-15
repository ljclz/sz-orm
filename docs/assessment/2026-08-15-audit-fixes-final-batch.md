# 安全审计修复记录——末批三项（M-14 / M-5 / M-6）

- **日期**：2026-08-15
- **范围**：`2026-08-14-whitehat-security-audit.md` 剩余 2 项（M-5/M-6 合一、M-14），此前 30 项已在前序批次修复
- **结果**：33/33 全部修复完毕（Critical 2 + High 6 + Medium 16 + Low 部分 + Info 部分中列入修复清单的全部项）

---

## M-14：双向同步语义重定义（sz-orm-lc）

**审计发现**：Merge 策略 = OrmWins 别名（resolved_fields 死代码）；单向同步绕过冲突门禁（破坏性覆盖无人工确认）。

**修复内容**：

1. **Merge 逐字段语义**（不再等于 OrmWins）：
   - `TypeMismatch` 无法自动合并 → 一律挂起人工确认 `[packages/sz-orm-lc/src/bidirectional_sync.rs:389](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-lc/src/bidirectional_sync.rs#L389)`
   - `ConstraintMismatch` 取保守并集（nullable=false 胜、unique=true 胜、primary_key=true 胜）`[bidirectional_sync.rs:396](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-lc/src/bidirectional_sync.rs#L396)`
   - `RelationMismatch`/`BidirectionalChange` 归入 unresolved，不再以错误类型塞入字段映射
2. **冲突门禁提升到函数级**：`sync()` 在方向分支前统一 detect → resolve，单向同步（OrmToLc/LcToOrm）同样过门禁，Manual 默认策略下类型变更挂起 `[bidirectional_sync.rs:635](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-lc/src/bidirectional_sync.rs#L635)`
3. **resolved_fields 真正消费**：`apply_resolution` 按字段名覆盖同步产物（双向并集 / 单向胜者模型）`[bidirectional_sync.rs:721](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-lc/src/bidirectional_sync.rs#L721)`
4. 配套：冲突值升级为完整描述格式（含类型），`parse_constraint_value`/`field_from_value` 解析（兼容旧格式回退）`[bidirectional_sync.rs:447](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-lc/src/bidirectional_sync.rs#L447)`、`[bidirectional_sync.rs:469](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-lc/src/bidirectional_sync.rs#L469)`

**验证**：`cargo test -p sz-orm-lc` → 51 passed, 0 failed（新增 8 项回归：Merge TypeMismatch 挂起、Merge 约束并集、OrmWins/LcWins 约束侧、RelationMismatch unresolved、单向 Manual 挂起、单向 OrmWins 放行、双向/单向 Merge 约束并集落地）

---

## M-5：`having()` 参数化（sz-orm-core，破坏性签名变更）

**审计发现**：`QueryBuilder::having()` 接受原始字符串直接拼入 HAVING（全库唯一残余注入面）。

**修复内容**：

1. 新增 `AggExpr`（函数名白名单 COUNT/SUM/AVG/MIN/MAX + 列名标识符校验）与 `HavingOp` 枚举 `[query.rs:119](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L119)`、`[query.rs:157](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L157)`
2. `having()` 签名从 `impl Into<String>` 改为 `(AggExpr, HavingOp, Value) -> Result<Self, DbError>`，构建期校验 `[query.rs:1102](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L1102)`
3. 两处渲染点参数化：无参数版内联方言转义值 `[query.rs:1546](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L1546)`；参数版 `?` 占位 + params `[query.rs:2251](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L2251)`
4. 迁移全部 6 个调用点：quick_query wrapper/测试、query_contract 契约测试、lib.rs 文档、having doctest；QuickQuery 同步参数化

**验证**：`cargo test -p sz-orm-core --test blackhat_sql_injection` → 12 passed（M-5 7 项 + M-6 5 项），含注入列名 → Err、恶意值 → `?` 绑定参数反转断言

---

## M-6：`select()` 默认安全（sz-orm-core，破坏性签名变更）

**审计发现**：`QueryBuilder::select()` 列名不校验不引用（ORDER BY/GROUP BY 均 quote，唯独 SELECT 裸拼）。

**修复内容**：

1. `select()` 改为 `Result<Self, DbError>`：每个列名经 `sql_safety::validate_identifier` 校验并 quote，构建期拦截注入 `[query.rs:720](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L720)`
2. 新增 `select_expr()` 逃生口（复杂表达式 `*`/`COUNT(*) as cnt`/`users.id`，标注仅可信来源）`[query.rs:735](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L735)`
3. `select_quoted()` 委托给 `select()` 保持兼容 `[query.rs:743](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L743)`
4. 迁移全部调用点：quick_query/select_types wrapper、contracts 契约测试（`*` → select_expr）、e2e_keyset（`COUNT(*) as cnt` → select_expr）、fuzz、param_binding、benches、examples/quick_start（`users.id` → select_expr）

**验证**：同上 blackhat 12 passed；`cargo test -p sz-orm-core` 全量无失败；`cargo check --workspace --all-targets` 通过

---

## 门禁验证

| 门禁 | 命令 | 结果 |
|------|------|------|
| 1 fmt | `cargo fmt --all -- --check` | ✅ |
| 2 check | `cargo check --workspace --all-targets` | ✅ |
| 3 clippy | `cargo clippy -p sz-orm-core -p sz-orm-lc --all-targets -- -D warnings` | ✅ |
| 4 test | `cargo test -p sz-orm-core -p sz-orm-lc` | ✅ 全绿（含 doctest 60） |
| 8 占位实现 | grep todo!/unimplemented!/unreachable! 修改文件 | ✅ 零命中 |
| 9 SQL 注入扫描 | `scripts/check-sql-injection.ps1` | ✅ 36 项全为既有文件（测试逃逸用例/其他包），本次修改文件零命中 |

**注意**：M-5/M-6 为破坏性 API 变更。外部试点 sz-pay 使用已发布的 sz-orm-core 1.0.0（crates.io），当前不受影响；若升级本地版且调用 `select()`/`having()`，需按新签名迁移（见 CHANGELOG 建议）。
