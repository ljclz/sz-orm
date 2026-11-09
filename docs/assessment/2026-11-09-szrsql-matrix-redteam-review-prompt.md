# 新会话提示词：SzRSQL 矩阵差分评测实施批——红队独立复核（v1，2026-11-09）

> **用法**：将下方「---」之间的内容整体复制到**新的独立会话**（非报告作者会话）作为首个提示词发送。
> **六环定位**：本批评测已完成第 1~4 环（实施提示词→报告→实施批→复评），本提示词启动**第 5 环红队独立复核**。复核者与实施者不得为同一会话——本会话即执行第 5 环。
> **背景文档**：报告 `docs/assessment/2026-11-09-szrsql-matrix-differential.md`；反馈提示词 `docs/assessment/2026-11-09-szrsql-matrix-bugs-feedback-prompt.md`；实施批 commits `ac36909b → 75dccb91` 共 7 个。

---

**角色**：你是 sz-orm 项目的**红队复核工程师**，与矩阵差分评测实施者（另一会话）相互独立。你的任务不是重做评测，而是**证伪**：对实施批的每项声称找反例、找夸大、找幻影、找测试盲区。质量官（sz-orm-qa）标准：任一环节红牌即生成《阻断报告》并拒绝入库。

**被复核对象**（逐项给出复核结论）：

1. **报告**：`docs/assessment/2026-11-09-szrsql-matrix-differential.md`
   - 复核其 9 项 szrsql 上游缺陷的分级是否恰当（P0×2/P2×2/P3×5）——重点验证两个 P0：
     - UP-1 RR/SERIALIZABLE 非真快照：报告引用双会话实验，自行复现（szrsql 6543 trust 已在跑；复现脚本思路见报告 §1 M4）；
     - UP-2 IS [NOT] DISTINCT FROM 误裁列：报告定位到 `crates/szrsql-optimizer/src/rule.rs:435/1597/1768` 三个收集器缺 arm（szrsql 基线 `f937e474`）——**只读核实**行号与内容是否属实、归因（ProjectionPruning 裁列）是否成立；
   - 复核「宁窄勿宽」声明：报告 §5 的不可声明清单是否有遗漏（例如：DISCARD 失败→连接重建的性能税是否被轻描淡写；20 万行流式 25× 性能差距的定性是否合理）；
   - 跑 `bash scripts/audit-verify.sh docs/assessment/2026-11-09-szrsql-matrix-differential.md`，检查 9 个警告是否均为 szrsql 跨仓库路径跳过（报告规范：szrsql 文件写完整路径不加行号——报告实际写法是否与规范一致）。

2. **实施批代码**（7 commits）：
   - `6ecfc964` 迁移锁降级：复核 `packages/sz-orm-core/src/migration.rs` 新 Err 分支——降级为 `Ok(None)` 是否引入新风险（多实例并发 migrate 丢锁窗口、告警日志是否足以让运维感知）；新测试 `r7_lock_query_error_degrades_to_unlocked_migration` 是否真验证行为（断言 DDL 执行 + 不释放未持有锁）还是幻影断言；
   - `ac36909b` + `22978cb2` + `4b6bf7eb` 矩阵测试：复核 `packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs` 15 项 + `integration_szrsql_kill_recover.rs` 1 项——
     - 运行 `cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored`（需 szrsql 6543，未启动则跳过并如实记录）验证 13 passed + 2 failed（2 个失败必须恰为 `#[ignore="szrsql 上游..."]` 留档项）；
     - 用 `python scripts/check-phantom-tests.py` 视角人工审查：3 个留档失败项是否「真失败于上游缺陷本身」（错误信息须与报告记录一致），而非测试自身写错导致失败；
     - 断言质量抽查：随机抽 5 项，验证断言的是行为副作用而非初始值/配置值；
   - `75dccb91` 安全收口：复核 `.exe` 扩展名校验是否充分（`SZRSQL_BIN` 仍可指向恶意同名 .exe——评估该风险在测试场景下的可接受性并给出结论）。

3. **登记**：CHANGELOG.md Unreleased 节与 AGENTS.md「已发布」行的描述是否与实际交付一致（数字、文件名、定性）；有无夸大（如「全功能矩阵」措辞 vs 实际 M1~M9 中 M5/M6 有未覆盖子项的记录是否保留）。

**硬性要求**：
- 独立验证优先于阅读报告：每个 P0 结论先自己复现/核实，再对照报告；
- szrsql 仓库（`D:\www\test\鲜视达\rust\szrsql`）**只读**，基线 `f937e474`（HEAD 若已前进，记录新 commit 并注明差异可能影响复现）；
- 环境：szrsql 6543 trust（未启动：`D:/rust-target/debug/szrsql.exe --port 6543 --auth-mode trust --connection-idle-timeout 0 --data-dir D:/tmp/szrsql-matrix &`）；PG18 5432；git 在 `D:\Git`；`cargo fmt --all` 用 `bash scripts/check-fmt-all.sh`；临时脚本放 `D:\tmp`；
- 每条结论附 `file:line` 或命令输出证据（审计合规铁律）；「通过/红牌」分级，不混建议为缺陷。

**输出**：
1. 《红队复核报告》落盘 `docs/assessment/2026-11-09-szrsql-matrix-redteam-review.md`：逐对象复核结论（通过/红牌 + 证据）→ 发现的真实缺陷清单（若有）→ 最终裁决（放行入库 / 阻断 + 阻断理由）；
2. 报告完成后跑 `bash scripts/audit-verify.sh docs/assessment/2026-11-09-szrsql-matrix-redteam-review.md`（0 失败为验收线）；
3. `git commit`（conventional commit，附验证输出）——若裁决放行，即完成第 5 环；若阻断，生成整改提示词进入返工循环。

**环境坑位速查**：差分对拍中文须经 Python subprocess stdin UTF-8（bash heredoc 会产生 PG 0xb1 假编码错误）；szrsql 进程可能静默退出（netstat :6543 检查）；`psql` 在 `D:\PostgreSQL\18\bin`；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`。
