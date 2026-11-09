# SzRSQL 矩阵差分评测实施批——红队独立复核报告（2026-11-09）

> 依据提示词：[2026-11-09-szrsql-matrix-redteam-review-prompt.md](2026-11-09-szrsql-matrix-redteam-review-prompt.md)
> 被复核对象：报告 `2026-11-09-szrsql-matrix-differential.md` + 实施批 7 commits（`077ddd75 → 75dccb91`）
> 复核者：独立新会话（非报告作者会话），符合第 5 环要求
> 复核环境：szrsql 6543 trust（复核中进程静默退出一次，已重启，PID 50536，二进制 `D:\cargo-target\debug\szrsql.exe` 构建于 2026-11-09 09:43）；PG18 5432；szrsql 仓库只读，复核期间 HEAD 已前进至 `96c6dc72`（报告基线 `f937e474`）

## 0. 最终裁决

**放行入库**。两个 P0 结论经独立验证均成立（UP-2 静态归因逐行核实 + UP-1 修复后二进制反向确认 + PG18 对照），实施批代码无幻影、无占位，登记基本准确。发现 2 项 P3 级证据瑕疵与 1 项登记数字漂移（均为文档层面，不阻断，见 §4）。

## 1. 逐对象复核结论

### 1.1 报告的两个 P0 —— 通过

**UP-2（IS [NOT] DISTINCT FROM 误裁列）——归因成立，行号核实**：

- 基线静态核实（`git show f937e474:crates/szrsql-optimizer/src/rule.rs`）：
  - `IsDistinctFrom` 全文出现 **0 次**（报告声称一致）；
  - 三个收集器函数体 `collect_column_refs_recursive`(L435) / `collect_expr_column_refs_grouped`(L1597) / `collect_expr_unqualified_refs`(L1768) 内 `IsDistinctFrom` 均为 0 次；
  - 同构对照成立：`IsNull` 在三个收集器中各出现 1 次（基线 L493/L1652/L1822）——「IsDistinctFrom 落入 `_ => {}` 被 ProjectionPruning 裁剪」的归因机制属实。
- 动态反向确认：运行中实例已含上游修复 `45d05bb8`（fix(optimizer): UP-2，2026-11-09 09:23，二进制 09:43 构建），我方独立复现脚本 `D:\tmp\rt_up2_indf.py` 实测：原失败查询 `SELECT id FROM rt_t WHERE v IS NOT DISTINCT FROM 10` 现返回 `[(1,)]` 正确；`IS NULL` 对照 `[(2,)]`；IndexScan 对照 `[(1,)]`——缺陷报告可直接驱动上游修复，定性正确。
- 防御式推断核实：基线 rule.rs 中 `SimilarTo`/`Substring` 均为 0 次（报告「建议上游一并排查」的同类风险推断属实，ast.rs L2905/L2930 存在这两个 Expr 变体）。

**UP-1（RR/SERIALIZABLE 非真快照）——结论成立，经修复后二进制反向确认**：

- 我方独立双会话实验 `D:\tmp\rt_up1b_raw.py`（raw BEGIN 规避驱动事务管理干扰）：当前二进制（含上游修复 `96c6dc72` fix(tx): UP-1，09:35:53 commit，二进制 09:43 构建）szrsql RR read1=100/read2=100 **快照稳定**，PG18 对照同为快照稳定——修复生效。
- 基线缺陷证据链：上游修复 commit `96c6dc72` 提交说明自述「矩阵差分（sz-orm 2026-11-09）复现：RR 事务 A 首读 v=100，B autocommit UPDATE v=200 后，A 再读 pk 返回 200」并定位三处代码路径，与本报告 §1 M4 记载完全吻合；报告双库对拍脚本 `D:\tmp\m4_isolation_v3.py` 实存。
- 注：复核中 SERIALIZABLE 探针首轮 read1=200 为我方脚本自身残留状态伪影（前序 level 的 UPDATE 未回填即测），已修正脚本后排除——该伪影不构成对报告的反例。

### 1.2 audit-verify 自证 —— 通过

`bash scripts/audit-verify.sh docs/assessment/2026-11-09-szrsql-matrix-differential.md`：**通过 2 / 失败 0 / 警告 9**。9 个警告逐条核对均为 szrsql 跨仓库路径（`src/expr.rs:2703/1230/9783`、`src/rule.rs:1768/1597/435`、`src/pgwire/session.rs:7028`、`dialect.rs:1760`、`any_driver.rs:129`），符合报告「szrsql 文件写相对路径」的规范声明。

### 1.3 实施批代码 —— 通过（附 2 项 P3 瑕疵）

**`6ecfc964` 迁移锁降级** —— 通过：

- 代码核实（[packages/sz-orm-core/src/migration.rs:679](../../packages/sz-orm-core/src/migration.rs#L679)）：Err 分支 → `tracing::warn!`（含 key + error 字段，运维可检索）→ `Ok(None)`；下游 `release_migration_lock`（migration.rs:712）对 `None` 直接 return，不会执行未持有的锁释放——语义闭环正确。
- 降级风险评估：多实例并发 migrate 确实失去串行化保护，但这是「后端不支持锁」的既有事实（rows 为空分支早已同语义降级），warn 消息明确写出「多实例并发需部署侧串行化」，与既有约定一致，可接受。
- 测试 `r7_lock_query_error_degrades_to_unlocked_migration`（migration.rs:2459）**真验证行为非幻影**：FailingLockConn 模拟 `pg_try_advisory_lock` 缺函数 → 断言 DDL 真实执行（`CREATE TABLE t_r7` 出现在日志）+ 断言不尝试 `pg_advisory_unlock`。实测：`cargo test -p sz-orm-core --lib r7_lock` = **4 passed, 0 failed**。

**`ac36909b` + `22978cb2` + `4b6bf7eb` 矩阵测试** —— 通过：

- 实跑 `cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored`（SZ_ORM_SZRSQL_URL 注入 6543）：**13 passed + 2 failed**，与报告 §6 记录形态一致。
- 2 个失败均为 `#[ignore="szrsql 上游…"]` 留档项，且**失败于上游缺陷本身**而非测试自身写错：
  - `matrix_m5_discard_reset` → `unsupported statement: Discard { object_type: ALL }`（与报告 §1 M5 逐字一致）；
  - `matrix_m8b_lastval` → `function not found: lastval`（与报告 §2 UP-5 一致）。
  - 第三个留档项 `matrix_m9_is_distinct_from` 本次 **passed**——因运行中实例已含上游 UP-2 修复，属预期良性漂移（修复后留档项转绿正是留档的目的），非幻影。
- 断言质量抽查 5 项（m1 group/having 行数、m2 ILIKE 命中数/DISTINCT ON 分组数、m5b 循环租用断言 + active≤2 收敛断言、m4 快照一致性断言、kill_recover 基线/kill 后失败/重启后恢复三段断言）：全部断言行为副作用（查询结果行数、池容量收敛、连接失败恢复），无初始值/配置值断言，无 `let _ =` 丢弃。`python scripts/check-phantom-tests.py` 全量：**硬规则 0 命中**（软规则 833 条为存量 P3 豁免），通过。
- M6b 20 万行流式为 `D:\tmp\m6b_stream4.py` 脚本实验（非仓库测试），报告 §1 M6 如实标注实验形态（单会话 21 次 FETCH 10000），脚本文件实存——不构成幻影，但见 §4 漂移项 3。

**`75dccb91` 安全收口** —— 通过：

- 核实（integration_szrsql_kill_recover.rs:21-28）：`.exe` 扩展名 + `is_file()` 双校验，参数全部 `Command::arg` 列表传递（无 shell、无拼接）。
- 残余风险评估：`SZRSQL_BIN` 仍可指向恶意同名 .exe——但该环境变量由本机操作者设置、测试仅本地执行、命令行参数固定不可注入命令结构，攻击面为「本机环境变量已被攻击者控制的场景」（此时已无护栏意义）。**风险可接受，结论：通过。**

### 1.4 登记 —— 通过（附 1 项数字漂移）

- CHANGELOG.md Unreleased：M1~M9 / 14 项矩阵测试 / 3 留档 / P0×2+P2×2+P3×5 / 迁移锁降级——与报告 §2/§4/§6 一致。
- AGENTS.md 已发布行（AGENTS.md:10）：「14 项矩阵测试（11 真连过 + 3 上游缺陷留档）」与报告一致。
- 「全功能矩阵」措辞：报告 §1 M5/M6 明确保留了未覆盖子项记录（kill-recover 初判未执行 → 已补测闭环；20 万行流式补测），M7「多租户/N+1 为 sz-orm 侧组件不回归」的限定语保留——无夸大。

## 2. 发现的真实缺陷清单

**无红牌级缺陷。** 以下 3 项为 P3 级文档瑕疵（不影响任何技术结论，随下次文档触碰时顺手修正即可）：

| # | 级别 | 瑕疵 | 证据 |
|---|------|------|------|
| R-1 | P3 | 报告 §1 UP-2 定位线索引用 `session.rs:7028/7471`，基线实际为 **7049/7495**（`ProjectionPruning::apply` 调用点）——行号漂移，调用点本身属实 | `git show f937e474:crates/szrsql-protocol/src/pgwire/session.rs | grep -n ProjectionPruning` = 7049/7495 |
| R-2 | P3 | 报告引用 `expr.rs:9783`（标注 precompile 递归），基线该行为 Decimal/Float 比较分支；precompile 实际在 `10176`（`precompile_column_refs`），`IsDistinctFrom` arm 在 `10292`——行号错位但「precompile 已递归处理 IsDistinctFrom」的技术结论仍成立 | `git show f937e474:crates/szrsql-sql/src/expr.rs | sed -n '9783p;10176p;10292p'` |
| R-3 | P3 | 矩阵测试数漂移：CHANGELOG/AGENTS 写「14 项」，实施批补测 M5b 后文件实际为 **15 项**（另有 kill_recover 1 项单独计）——报告 §5「矩阵 14 项」同漂移 | `grep -c "async fn matrix" integration_szrsql_matrix.rs` = 15 |

## 3. 复核过程记录（独立验证优先）

1. 先复现后读报告：UP-2/UP-1 均先跑独立脚本（`D:\tmp\rt_up2_indf.py`、`rt_up1_isolation.py`、`rt_up1b_raw.py`），再对照报告记载。
2. szrsql 仓库全程只读（仅 `git show` / `grep`）；复核期间其 HEAD 自 `f937e474` 前进至 `96c6dc72`（UP-2/UP-1 修复由上游并行会话落地），已按提示词要求记录并利用该状态做反向确认。
3. szrsql 进程复核中途静默退出一次（提示词坑位预言命中），`netstat :6543` 发现后重启，重启日志 `D:\tmp\szrsql_restart.log` 留档。
4. 运行中二进制归属核实：PID 50536 → `D:\cargo-target\debug\szrsql.exe`（构建 2026-11-09 09:43:06，晚于两个修复 commit），确认反向验证有效。

## 4. 最终裁决

**放行入库**（第 5 环完成）。报告结论、实施批代码、登记均通过红队复核；R-1/R-2/R-3 三项 P3 文档瑕疵登记在案，不阻断，建议在下一批文档变更时一并修正（报告行号引用 + 测试计数 14→15）。

验证输出（本报告完成后自证）见文末 audit-verify 记录。
