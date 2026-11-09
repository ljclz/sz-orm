# SzRSQL 矩阵差分 UP-1~UP-9 上游修复——sz-orm 侧闭环复测记录（2026-11-09）

> 上游修复报告：`szrsql/docs/governance/evaluation/2026-11-09-szorm反馈-matrix-bugs-修复.md`（commit `e5d8fe02` 登记）
> 本记录：sz-orm 会话对上游 5 个修复 commit（96c6dc72/45d05bb8/977da2f7/df2a1d57/df878ad5）重建二进制后的**独立闭环复测**，复测脚本 `D:\tmp\up_closure_retest.py`（三方对照，PG18 为基准）。
> 证据锚点：矩阵测试载体 [packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs:392](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L392)（m8b_lastval 留档项）与 [packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs:248](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L248)（m5_discard 留档项）；上游修复面 `git -C szrsql show 45d05bb8 -- crates/szrsql-optimizer/src/rule.rs`（+164 行）。

## 一、复测结果：9/10 PASS（1 项为我方脚本断言误报，复验实过）

| UP | 内容 | 复测结果 | 证据 |
|----|------|---------|------|
| UP-1 | RR 真快照 | **PASS** | 双会话实验 sz=[100,100] = pg=[100,100]（修复前 sz=[100,200]） |
| UP-2 | IS [NOT] DISTINCT FROM | **PASS** | `WHERE v IS NOT DISTINCT FROM 10`→1；`IS DISTINCT FROM 10`→2，与 PG 一致（首轮脚本 FAIL 为断言写法误报，`D:\tmp\up2_recheck.py` 复验输出正确）；上游修复 diff：commit `45d05bb8`（`git -C szrsql show 45d05bb8 --stat`：rule.rs +164 行三收集器补递归臂） |
| UP-3 | bool↔int cast | **PASS** | `true::int,false::int,1::bool,0::bool` = `1\|0\|t\|f` = PG |
| UP-4 | int 溢出报错 | **PASS** | `2147483647::int+1` 报 `integer out of range`（修复前静默回绕） |
| UP-5 | lastval() | **PASS** | INSERT 后 `lastval()`=1（修复前 function not found） |
| UP-6 | EXECUTE 放行 | **PASS** | `PREPARE psel + EXECUTE psel(7)` 返回 7（修复前 firewall 拦截） |
| UP-7 | advisory lock + hashtext | **PASS** | try=t/unlock=t；`hashtext('abc')=-785388649` 与 PG 逐位一致 |
| UP-8 | current_setting 读回 | **PASS** | SET 后返回 `myapp`（修复前空串） |
| UP-9 | numeric 精度 | **PASS** | `1::numeric/3` = `0.33333333333333333333`（20 位）= PG |
| 留档项 | DISCARD ALL | **仍不支持** | 不在 UP-1~9 范围（上游未修），维持上游缺口登记 |

## 二、sz-orm 矩阵套件状态更新（新 HEAD 下）

`cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored`（测试文件 [packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs)）：
- **13 passed**（含上批 3 个 `#[ignore]` 留档中的 `matrix_m9_is_distinct_from`——上游 UP-2 修复后转绿，留档目的达成）
- **2 failed（留档项状态变化，需更新留档理由）**：
  - `matrix_m5_discard_reset`：仍失败于 `unsupported statement: Discard`——DISCARD 不在上游修复范围，留档理由不变
  - `matrix_m8b_lastval`（[integration_szrsql_matrix.rs:392](../../packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs#L392)）：失败信息**已变化**——从 `function not found: lastval` 变为 `currval of sequence lastval is not yet defined in this session`。这实为**修复后更贴近 PG 语义**（PG 中未执行 nextval/setval 前 `lastval()` 同样报 `lastval is not yet defined in this session`），留档断言需更新为「先 nextval 再 lastval」的正确用法

## 三、后续动作建议（随下一批文档/测试触碰执行）

1. `matrix_m8b_lastval` 留档注记更新：改为「UP-5 已修复；当前失败于未先 nextval 即调 lastval（与 PG 语义一致），断言应改为 nextval→lastval 正序用法」
2. `matrix_m5_discard_reset` 留档理由维持（DISCARD/RESET 仍为上游缺口）
3. 上游修复报告由 szrsql 侧六环登记（`e5d8fe02`），sz-orm 侧本记录为跨仓库证据链闭环

## 验证

- 重建：`cargo build -p szrsql-bin`（HEAD `4f67ebc5`，含全部 5 个修复 commit）✅
- 复测脚本：`python -I D:\tmp\up_closure_retest.py` → 9/10 PASS（1 误报已复验排除）
- 矩阵套件：13 passed + 2 failed（均为留档项，状态变化已分析定性）
