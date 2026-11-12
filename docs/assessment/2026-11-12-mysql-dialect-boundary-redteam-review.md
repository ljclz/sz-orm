# MySQL 方言边界能力补齐批红队复核回执（环 5）

> **复核对象**：commit 5161cdcf（B1/B2/B3）+ 27be39db（B4 + 矩阵 MY7~MY10）+ 报告 `docs/assessment/2026-11-12-mysql-dialect-boundary.md`
> **复核者**：独立 Agent session（非报告作者），main 只读，零写入
> **结论**：**放行**。阻断级 0，建议级 5（全部采纳，处置见文末）

## 逐项判定

| # | 清单项 | 判定 | 关键证据 |
|---|--------|------|---------|
| 1 | 零破坏声明 | PASS | 5161cdcf numstat dialect.rs +303/-0；27be39db query.rs +199/-2、矩阵 +359/-1——query.rs -2 逐行查明为行为保持改写（format 前缀化：空 CTE 时 `render_cte_prefix` 返回空串输出字节级不变；params 声明改 CTE 参数 pre-seed：空 CTE 等价空 Vec） |
| 2 | B1 fail-closed | PASS（附建议级 1） | Engine/Charset/Collate 三分支全跑 `validate_identifier`（dialect.rs:534/539/545）；空格/反引号/分号/注释注入形态全部拒收，`catch_unwind` 单测断言 |
| 3 | B2 fail-closed | PASS | on_delete/on_update 双分支过 `validate_fk_action`；大小写混写规范化 "set null"→`SET NULL`；None 零子句；约束名 quote 与 G2 修复同口径 |
| 4 | B4 逃生口与参数顺序 | PASS（附建议级 3/5） | 名称校验真执行（push_cte→validate_identifier_with_max）；任一递归→整条 WITH RECURSIVE；params pre-seed 于主查询任何参数入列点之前（:2907）；sql() 内联路径独立内联无污染；多 CTE 前缀渲染序与 flat_map 参数序同源对齐；clone_for_count 保留 CTE |
| 5 | 机械验证 | PASS | 矩阵 **11 passed**（MY1~MY10）+ lib **3279 passed** + audit-verify **15/0 失败**，三数字与报告一致复现 |
| 6 | S4 裁定 | PASS（附建议级 4） | IN 含 NULL 恒不命中为 ANSI 正确语义，裁定成立；**空切片 `IN ()` 边界**查明：无守卫、MySQL 1064 fail-visible、既有测试断言为既定行为——语义化属破坏性变更，要求登记（已落实 §3） |
| 7 | 报告质量 | PASS | audit-verify 全路径引用；六环表环 5 ⏳ 为落盘时点事实；三分类与建议分离成立 |

## 真实缺陷清单

**阻断级：无。**

**建议级 5 项（处置）**：

1. B1 panic 契约未写入 rustdoc → **已落实**：`build_create_table_with_options` / `render_table_options` 补 `# Panics` 段（环 6 commit）
2. 报告测试计数不实（初稿 12 → 实际新增 11：lib 7 + 矩阵 4；commit 消息「单测 8 个」系笔误不改历史） → **已更正**报告 §5.3 并注明
3. `inline_condition_values` 不感知字符串字面量（CTE SQL 字面量内 `?` 会错位内联）→ **已落实**：`with_cte` doc 增「已知限制」段；参数化路径不受影响
4. where_in 空切片 `IN ()` 边界未登记 → **已落实**：报告 §3 补登记（fail-visible + 既有测试断言既定行为 + 语义化属破坏性变更留后续）
5. nits（报告 "## 5." 重号、audit-verify 警告 6→7 漂移、build_select_with_params 容量估算未含 CTE 前缀、双参数 CTE 组合无单测）→ 重号已修、警告数已同步；容量估算与双参数 CTE 单测登记为后续小项（风险低：对齐系结构性）

## 放行结论

**放行（PASS）**。四项能力实现真实、fail-closed 语义经对抗构造验证成立、零破坏声明经 numstat 逐行查明、真连行为级验证（CASCADE 级联删/组织树深度 [1,2,3,2]/参数顺序绑定证明/information_schema 元数据核验）全部可复现。
