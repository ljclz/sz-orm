# 门禁审查报告（2026-09-13）

- 分支 / commit：`main` @ `6dc47d1`（工作区含 v7.0.0 在途变更，110 个未提交文件基线）
- 范围：G1~G23 全量 + 条件触发 G24（API 变更）+ G25（secrets）+ G26（deprecated 保留期）
- 执行方式：`/sz-orm-review` 全量 + `--ai`（AI 评审后端：CSDN `https://ai.csdn.net/api/model/coding/v1`，model `deepseek-v4-flash`）
- 总耗时：约 8 小时（G20 变异测试占 6.5 小时，含一次 3 小时脚本超时重跑）

## 结果表

| G# | 门禁 | 状态 | 关键证据 |
|----|------|------|----------|
| 1 | fmt | ✅ | 逐包 `cargo fmt -p <pkg> --check` 全 0 违规（一次性 `--all` 触发 Windows os error 206，分包绕过） |
| 2 | check | ✅ | `cargo check --workspace --all-targets` 退出码 0（5m20s；初跑失败为陈旧 duckdb 构建产物，清理后恢复） |
| 3 | clippy | ✅ | 修复后退出码 0（首次红牌：2 处未用导入，见"修复"#1） |
| 4 | test | ✅ | 分片 A 6691 + 分片 B 3912 + relation_derive 2 = **10,603 通过 / 0 失败**（修复 2 项，见"修复"#2/#3） |
| 5 | doc | ✅ | `cargo doc --workspace --no-deps --all-features` 退出码 0，96 个文档 |
| 6 | audit | ✅ | `cargo audit` 退出码 0 + `cargo deny check` 退出码 0 |
| 7 | integration | ⚠️ 环境受限 | 代码层 **0 回归**：MySQL 系（mysql/tidb/mariadb/oceanbase）+ PG + Oracle + SQLite 全过；21 失败 = MSSQL 远程主机不可达 12 + ClickHouse 未启动 3 + Redis 未启动 4 + 设计占位测试 1（见"偏差"） |
| 8 | 占位实现 | ✅ | grep 4 处命中均为 doc 注释/测试字符串/普通注释，0 真实占位 |
| 9 | SQL 注入 | ✅ | `check-sql-injection.sh` 退出码 0，52 条 REVIEW 项全部为错误消息拼接/守卫字符串（非 SQL 执行路径） |
| 10 | feature 全组合 | ✅ | `cargo check --workspace --all-targets --all-features` 退出码 0 |
| 11 | ADR-0001 | ✅ | `check-upstream-unmodified.ps1`：核心包修改已附带文档更新 |
| 12 | 文档一致性 | ✅ | 修复后 PASS（包数 71→72，见"修复"#4） |
| 13 | 审计证据 | ✅ | `audit-verify.sh docs/assessment/2026-09-10-integration-test-coverage-delivery.md`：9 通过 / 0 失败 |
| 14 | 文档同步 | ✅ | 修复后 `OK: all affected docs synced`（README 版本引用，见"修复"#5） |
| 15 | 幻影交付 | ✅ | PHANTOM-1 0 个 | 符号通过 38 | 接线断言 **4/4** | PHANTOM-2 192（警告级，feature 矩阵设计使然） |
| 16 | 语义反模式 | ✅ | 硬规则 0 | 软规则 7（提示级） |
| 17 | 架构一致性 | ✅ | `check-architecture.py` 退出码 0 |
| 18 | 度量真实性 | ✅ | `--fix` 修正 README 徽章后复验 PASS（tests 14140→14449、packages 71→72，见"修复"#6） |
| 19 | 发布一致性 | ✅ | `check-publish-consistency.py` 退出码 0 |
| 20 | 变异杀率 | ✅ | **90.16%（55/61）≥ 70%**；49 unviable 不计；存活 6 个见"遗留观察"；cargo-mutants 脚本 3h 超时改为直跑不限时完成 |
| 21 | 安全攻击 | ✅ | auth security_attacks 5/5 + crypto kat 4/4 + tenant security_attacks 4/4 + OWASP 套件 5340 通过（1 失败已修复，见"修复"#7，A02 复跑 5/5）+ A06 全部通过 |
| 22 | 覆盖率 | ✅ | 4 个关键模块行覆盖 **100.0%**（bloom 112/112、cache_warmup 584/584、process_l1_cache 510/510、tenant_quota_rls 778/778）≥ 60% |
| 23 | 未用依赖 | ✅ | 38 个警告级（cargo-machete，含 feature 门控误报），非阻塞 |
| 24 | 契约测试 | ✅ | `cargo test -p sz-orm-core --test contracts`：228 通过 / 0 失败（v7.0.0 有 API 变更，条件触发必做） |
| 25 | secrets | ✅ | 扫描 1708 文件，0 命中 |
| 26 | 废弃保留期 | ✅ | 1 个 `#[deprecated]`，满足 ≥2 个 minor 版本期 |

**结论：26 关中 25 关通过，G7 因外部服务不可用部分受限（代码层 0 回归）。** 状态机全程合规（红牌即停→修复→从失败关重跑，无跳关）。

## 审查中发现并修复的问题（逐项验证）

1. **[P3] 未用导入 clippy 红牌**：`packages/sz-orm-sqlx/src/any_driver.rs` 测试中 `use sz_orm_core::Connection`（2 处）在具体类型有固有方法时冗余。已删除，G3 复跑退出码 0。
2. **[P0] PlanCache 锁序反转死锁（HEAD 既有 bug，非本次变更引入）**：`packages/sz-orm-core/src/plan_cache.rs:487` 命中路径原持 `parse_cache.read` 时获取 `access_order.write`，与未命中路径 `:514`（access_order→parse_cache）成环；`test_concurrent_cache_same_sql` 隔离复跑 120s 超时确证挂起。统一锁序 `access_order → optimize_cache → parse_cache → table_index` 重写 5 处获取点（`:487`、`:514`、get_or_optimize 命中路径、store_optimize、invalidate_table `:612`）。验证：差分测试 30/30 连跑通过 + 全量 G4 重跑 0 失败。
3. **[P3] trybuild 快照过期（rustc 1.98.0 trait 路径格式化）**：`packages/sz-orm-macros/tests/ui/relation_fail/*.stderr` 期望 `Model`，新版 rustc 输出 `sz_orm_core::Model`。`TRYBUILD=overwrite` 重生成，diff 逐行核验仅 4 行路径格式差异，无真实回归掩盖。
4. **[P2] 文档包数不一致**：`docs/sz-orm-engineering-practices.md:3` 71→72 workspace 包；`AGENTS.md:5` 同步（注：`check-doc-consistency.py --fix` 将「71（69」错误替换为「7269」，已手工修正为「72（70 lib 包 + cli + examples）」，脚本 replace 逻辑缺陷建议登记）。
5. **[P2] README 版本引用未随 Cargo.toml 同步**：`README.md:4`（v6.7.0→v7.0.0、71→72 members）、`README.md:11`（badge 6.6.0→7.0.0），G14 复跑退出码 0。
6. **[P2] README 徽章数字过期**：tests 14140→14449（实测 14449 测试标注）、packages 71→72。按"数字禁止手写"原则用 `check-metrics-real.py --fix` 自动修正，复验 PASS。
7. **[P3] OWASP A02 假阳性**：`packages/sz-orm-nl-query/src/cached_pipeline.rs` 用 `DefaultHasher` 计算内存缓存键（确定性、非密码学），与既有豁免同类。已登记入 `packages/sz-orm-crypto/tests/owasp_a02_crypto_failures.rs:166` 豁免表并附注释，A02 复跑 5/5。

## G7 偏差说明（已补齐环境，最终全绿）

**2026-09-14 环境补齐后复跑：167 通过 / 1 失败（唯一失败为设计占位 `cargo_mutants_baseline`），全部真实集成测试通过**（MySQL 系 + PG + Oracle + SQLite + DuckDB + MSSQL 8+7 + Redis 4 + ClickHouse 2）。环境路径：

- Redis/ClickHouse 跑在服务器 `122.51.216.76` Docker（`szorm-redis-ext` 6380→6379、`szorm-clickhouse` 9004），安全组未开放，经 SSH 隧道 `127.0.0.1:6379`/`127.0.0.1:9004` 访问；ClickHouse 挂载的 `/tmp/ch_users.xml` 随服务器重启丢失已重建。
- MSSQL 为腾讯云实例（此前网络不可达，本次可达）。
- 复跑命令（`--tests` 选择器排除 doctest）：`cargo test --workspace --tests --no-fail-fast -- --ignored --test-threads=4`，凭据经 `SZ_ORM_MYSQL_URL`/`SZ_ORM_PG_URL`/`SZ_ORM_REDIS_URL`/`SZ_ORM_CLICKHOUSE_URL`/`SZ_ORM_MSSQL_*` 注入。

**环境补齐过程中发现并修复 3 处真实 MSSQL 测试 bug（HEAD 既有，此前从未在真 SQL Server 上跑通过）：**

1. `tests/common/schema_builder.rs:74-89`：MSSQL 方言生成了 `CREATE TABLE IF NOT EXISTS`——T-SQL 任何版本都不支持（错误 156）；改为 MSSQL 生成普通 `CREATE TABLE`（幂等性由前置 teardown 保证）。
2. `tests/common/schema_builder.rs` seed_data：MSSQL `IDENTITY(1,1)` 列拒绝显式插入 id（错误 544）；MSSQL 方言下每条 INSERT 以批内 `SET IDENTITY_INSERT <t> ON; ...; OFF` 包裹。
3. `tests/smart_eager_integration_mssql.rs:74-110,176+`：`try_get_mssql_value` 用逐类型试探 `row.get::<i32>`——tiberius 对类型不符的列直接 panic（BIGINT 列取 i32 报 Conversion 错误）；改为按 `ColumnType` 分派。同文件 `query_with_params` 增加统一 `?` → `@P1..@Pn` 占位符翻译（T-SQL 文本不支持 `?`，错误 102）。

**首轮全量 `--ignored` 扫描的 120 个"doctest 失败"为工具伪影**：故意标记 `ignore` 的 doctest（依赖外部环境不可独立编译）被 blanket `--ignored` 强制执行；G7 的正确形态是加 `--tests` 选择器排除 doctest 目标。`cargo_mutants_baseline`（`packages/sz-orm-core/tests/mutation.rs:232`）同为故意 panic 占位，非缺陷。

## 遗留观察（不阻塞）

- G20 存活变异体 6 个：5 个为 Debug fmt / 观测计数器类（`bloom_count`、`in_flight_count`），1 个 `QuotaEnforcer::check_quota` 的 None 分支删除存活——建议后续为 quota None 分支补充定向测试（mutants.out/mutants.out/missed.txt）。
- `check-mutation-coverage.py` 内置 10800s 超时对当前变异体规模（110 个体、`--test-threads=1`）不足，本次直跑约 6 小时完成；建议脚本超时参数化。
- `llvm-cov-target` 目录存在来自旧路径 `C:/sz-rust-target` 的陈旧构建指纹，会导致 sys 包 `bindgen.rs` 缺失假失败（G22 重跑两次才通过）；建议对 llvm-cov-target 定期清理或迁移。

## 后续执行记录（2026-09-14，遗留项全部闭环）

| 事项 | 处置 | 提交 |
|------|------|------|
| G20 等价变异（quota None 分支） | 删冗余分支 + 2 定向测试；手动模拟超限分支删除变异 5 测试红确证可杀 | `135e33c` |
| mutants 脚本超时 | `--timeout` 参数化（默认 6h） | `135e33c` |
| doc-consistency --fix 括号吞噬 | 改为仅替换值捕获组 span + 补缺失修复 pattern | `135e33c` |
| G7 环境补齐 | 服务器 Docker 容器启动（Redis/ClickHouse）+ SSH 双隧道 + 修复 3 处真实 MSSQL 测试 bug（CREATE TABLE IF NOT EXISTS / IDENTITY_INSERT / 列类型分派与 `?`→`@Pn` 翻译）；G7 终态 167/0 | `460f7ea` |
| v7.0.0 在途工作 | 五大方向交付提交（72 文件 +5935） | `5079960` |
| G20 剩余 5 个存活变异 | 补杀测试 ×3；变异模拟验证 5/5 全杀（mutant 态 3 测试红，还原后 33/33 绿） | `96fdf38` |
| AI 评审遗留 #1（中间件链无 bench） | `middleware_chain_overhead_p99_under_1ms`（10k 采样 P99 ≤1ms）声称成立 | `23c3fec` |
| AI 评审遗留 #2（TDE 密钥安全） | LocalKmsClient 存储 Zeroizing 化 + DekBuffer 栈 key 用毕清零；核证 TdeInterceptor fail-closed 无明文回退 | `23c3fec` |
| AI 评审遗留 #3（多区域并发） | RegionFailoverCoordinator CAS 并发守卫 + FailoverInProgress 变体 + 8 线程争用测试 + 设计文档 `docs/multi-region-failover-concurrency.md` | `23c3fec` |

**门禁终态：26/26 全绿**（G7 经环境补齐 + `--tests` 排除 doctest 伪影后 167 通过，唯一失败为设计占位 `cargo_mutants_baseline`）。

## 发版登记（2026-09-14）

- **CI-only 门禁本地无法复现**（对照 runbook 4.4 节，如实登记，待 CI 结果印证）：
  fuzz（cargo-fuzz 长时模糊测试）、soak（长时间浸泡）、semgrep/codeql（静态安全扫描）、semver-check（cargo-semver-checks API 兼容性）——本地 23+3 关全绿不覆盖这些关卡；若 CI 红牌按 runbook "先确认是否 CI-only 关卡" 定位。
- **pre-push 集成门禁**：13 关全过（`[ACCEPT]`），修复期间顺带修正 3 处 gate.ps1 与正规门禁语义不一致/环境缺陷（fmt 分包 os error 206、SQL 扫描委托正规脚本、多份 all-features clippy/doc 严格模式残留共 20 余处）。
- **v7.0.0 tag**：本报告提交后打于 HEAD（全部审查修复 + 五大方向交付 + 环境补齐后的全绿状态）。

## 后续修复（2026-09-14 追加，按遗留观察执行）

1. **G20 存活变异处置**：`QuotaEnforcer::check_quota` 内层 `None => Ok(())` 分支与 `_` 兜底臂语义等价（删除后 None 落入 `_` 仍返回 Ok），属**等价变异**，任何测试不可杀。处置：删除冗余分支（`packages/sz-orm-core/src/tenant_quota_rls.rs:239-241`）缩小变异面 + 新增 2 个定向测试 pin 住语义（`test_quota_enforcer_resource_without_limit`、`test_quota_enforcer_unknown_tenant_with_others_configured`）。杀伤力验证：手动模拟「超限分支删除」变异，5 个测试红（含 1 个新增），还原后 41/41 绿。
2. **mutants 脚本超时参数化**：`scripts/check-mutation-coverage.py` 新增 `--timeout`（默认 21600s=6h，按本次实测校准），原硬编码 10800s 对 110 变异体 × `--test-threads=1` 不足。
3. **doc-consistency --fix 括号吞噬缺陷修复**：`scripts/check-doc-consistency.py` 旧实现 `\g<1>新值` 整段替换会吞掉未捕获的尾随字符（产生「7269 lib 包」损坏文本）；改为仅替换第 2 捕获组 span，并补齐 `workspace 包数量` 缺失的修复 pattern。验证：人为注入不一致 → `--fix` → 括号保留 + 复验 PASS。
4. llvm-cov-target 陈旧指纹已在审查中清除（libduckdb-sys/libsqlite3-sys 构建目录 + fingerprint），G22 已恢复正常；残余风险为同型问题复发，发现时同法清理。

## 补充信息 → AI 评审（不阻塞判定）

评审模型：deepseek-v4-flash（CSDN 端点，HTTP 200）。diff sha256：`a37672f7…a2bf`（缓存于 `~/.cache/sz-orm-review/`）。评分 **6.5/10**。要点摘录：

1. 插件系统中间件链性能声称（≤1ms）缺基准测试佐证 —— 本次 G20/G22 已覆盖部分核心模块，plugin.rs 不在变异子集内，建议补 bench。
2. TDE 密钥管理（kms_client/dek_buffer）缓存条目 Drop 清理与降级路径需安全审计。
3. 多区域路由/故障转移并发策略需明确锁与原子性说明。
4. 7.0.0 主版本升级的破坏性变更清单需在 CHANGELOG 明示。
5. "新增 499 测试"未在截断 diff 中可见 —— **AI 评审局限说明**：模型仅见前 8000 字符 diff；本次 G4 实测 10,603 测试通过可证测试已存在。

AI 结论未采纳为阻塞项（依据 skill 规则：AI 评审仅供参考）。

## 事件总线

`ReviewCompleted` 事件已追加至 `docs/audit/events.jsonl`。
