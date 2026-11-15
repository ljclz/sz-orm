# 二波三实例批（Kingbase/GBase 8s/Dameng）第 5 环红队独立复核——2026-11-14

> 复核对象：实施 commits `70db9577`（KB+GB）+ `eab60aa3`（DM）；报告 `docs/assessment/2026-11-14-wave2-three-vendor-matrix.md`。
> 复核者与实施者非同一 session。裁决：**条件 PASS**（P0=0 / P1=1 / P2=2 / P3=4）——实施批核心声称（供给纠正、三实例真连全过、KB2 delegate 行为级证据、GBase [bracket] 边界）全部独立复现成立；P1 为 GB3 测试容器依赖泄漏进默认套件（gate 4 假绿风险），须随登记批处置。

## 0. 发现清单

| 级别 | # | 发现 | 证据 | 处置要求 |
|------|---|------|------|---------|
| **P1** | R-1 | `matrix_gb3_dialect_shape_vs_8s_syntax` 未标 `#[ignore]`，函数体经 `run_gb8s_sql` 硬依赖 wsl+docker+`gb8s` 容器+容器内手工创建的 `t_probe` 表——**容器依赖泄漏进默认测试套件（gate 4 `cargo test --workspace`）**。无容器环境（含 Linux CI：`Command::new("wsl")` 直接 Err→`.expect` panic）gate 4 必红；本机绿仅因容器恰在运行 | packages/sz-orm-core/tests/integration_gbase.rs:333（`#[test]` 无 ignore，对照 :301/:315 同文件 GB1/GB2 均有 ignore）、packages/sz-orm-core/tests/integration_gbase.rs:359、packages/sz-orm-core/tests/integration_gbase.rs:360 | 登记批补 `#[ignore = "需要 GBase 8s 容器运行"]`（同文件 GB1/GB2 同款）；复跑 `cargo test -p sz-orm-core --test integration_gbase` 确认 6 passed（既有 6 单测）+ 5 ignored |
| **P2** | R-2 | GB1~GB3 依赖容器内 `t_probe` 表（szorm_test 库），但**repo 内无任何创建脚本、报告供给表（§1）未登记该前置步骤**——按报告 §1 重建容器即 GB2/GB3 断供（表不存在→217 错误≠断言期望形态） | packages/sz-orm-core/tests/integration_gbase.rs:317、packages/sz-orm-core/tests/integration_gbase.rs:323、packages/sz-orm-core/tests/integration_gbase.rs:339；红队实测 `select count(*) from t_probe;` = 1 row（表系手工预建）；全文件无 `CREATE TABLE t_probe` | 报告 §1 补登记前置 SQL（`CREATE TABLE t_probe (id INT);`），或测试内自建/自清理（推荐后者，同 KB 线 unique_table 惯例） |
| **P2** | R-3 | 报告 §4 条目 5「**KB/DM 通道** delegate 转发修复（OC 批）真连行为级验证成立」与 §5「三个新通道证据齐备」**对 DM/GB 通道证据强度过称**：OC 批 4 转发函数中，DM 通道无一在 DM8 上行为级差异化验证——upsert=None 系 trait 契约（非行为验证，packages/sz-orm-core/tests/integration_dameng.rs:421）；`build_add_foreign_key`/`render_table_options`/`build_create_table_with_options` 全仓**无任何基础方言覆写**（仅 trait 默认 packages/sz-orm-core/src/dialect.rs:238/:222/:191）→ 转发对 DM/GB 输出零差异；DM3 测试本体不调用 `run_dm_sql`（packages/sz-orm-core/tests/integration_dameng.rs:418~439 全本地断言）；GB 通道 SqlServerDialect 亦无 upsert 覆写，GB1~GB3 无一执行委托产物 DDL。**OC 批修复的行为级差异化证据仅 KB2 成立**（PostgreSqlDialect 覆写 upsert，packages/sz-orm-core/src/dialect.rs:1056；KB2 真库 EXCLUDED 冲突更新 10→99 复跑通过） | docs/assessment/2026-11-14-wave2-three-vendor-matrix.md:67、docs/assessment/2026-11-14-wave2-three-vendor-matrix.md:71 | 措辞级修订：§4.5 改为「KB 通道行为级验证成立；DM/GB 通道为生成面/边界登记（转发对该两通道输出无差异化影响——基础方言无覆写）」，或以 DM8/8s 真执行委托产物 DDL 补证（DM8 已由红队补证 FK DDL 可执行，见 §4.3-R6） |
| P3 | R-4 | DM3 docstring 声称「FK DDL 形态经 disql 验证」，但测试本体未含任何 disql 调用——验证发生在测试外（人工），证据链未入测试。红队已独立复现该声称**事实为真**（见 §4.3-R6：DM8 接受 FK DDL 且 CASCADE 级联 N=0） | packages/sz-orm-core/tests/integration_dameng.rs:415（docstring 声称）vs packages/sz-orm-core/tests/integration_dameng.rs:418（函数无 run_dm_sql） | 随登记批将 disql 验证落进 DM3（或删 docstring 声称改「形态断言」措辞） |
| P3 | R-5 | integration_kingbase.rs:274 注释「KingbaseES @15434」与实际不符——15434 是 license 过期的 kb2（xuxuclassmate）容器；本批载荷容器为 kb4@**15435** | packages/sz-orm-core/tests/integration_kingbase.rs:274 vs 红队 `docker ps`（kb4→15435，kb2→15434）与 `SZ_ORM_KB_URL` 15435 复跑 4/4 | 注释端口改 15435 |
| P3 | R-6 | packages/sz-orm-core/tests/integration_dameng.rs:326 disql 连接串硬编码 `SYSDBA/SYSDBA001`——社区镜像默认口令+本地容器（敏感度低，check-secrets EXIT=0 未拦截），但与 P0-3「凭据经 env 注入」惯例不一致；对照同文件 Oracle 线 :15-29 全 env 注入 | packages/sz-orm-core/tests/integration_dameng.rs:326 | 后续批改 `SZ_ORM_DM_PASSWORD` env 注入（低优先） |
| P3 | R-7 | 报告文件 `2026-11-14-wave2-three-vendor-matrix.md` 复核期间曾**未提交**（工作区 untracked），而 CHANGELOG.md:12~19 [Unreleased] 已登记并引用该路径——若 CHANGELOG 先行提交而报告缺席，G35 文档链接红牌。**复核收尾时实施方并行提交 `adbfbe3f`（登记批）已落盘报告 + AGENTS.md「已发布」行——R-7 已消解，仅留痕** | `git status`（复核中段 `?? docs/assessment/2026-11-14-wave2-three-vendor-matrix.md`）；CHANGELOG.md:14；`git show --stat adbfbe3f`（报告 71 行入库） | 已消解（adbfbe3f），无需处置 |

登记观察（非缺陷）：DM3 标 `#[ignore]` 但本体不需容器（packages/sz-orm-core/tests/integration_dameng.rs:417）——与 R-1 相反方向的标注错位；移除 ignore 后可作为默认套件的 delegate 生成面覆盖。

## 1. 机械复跑（自取 EXIT 码，2026-11-14）

| 项 | 命令 | 结果 | EXIT |
|----|------|------|------|
| dialect 单测 | `cargo test -p sz-orm-core --lib dialect` | **264 passed; 0 failed**（0 ignored） | 0 |
| G37 | `python scripts/check-dialect-ddl-quoting.py` | ✅ 0 命中（12 方言 DDL/FTS 生成面保持） | 0 |
| G37 自测 | `python scripts/check-dialect-ddl-quoting.py --self-test` | self-test: **19 passed / 0 failed** | 0 |
| G9 strict | `python scripts/check-sql-injection-auto.py --strict` | Total 46 / Safe 46 / Unsafe 0 / ManualReview 0 | 0 |
| G9 自测 | `python scripts/test_check_sql_injection_auto.py` | **15 passed; 0 failed** | 0 |
| audit-verify（实施批报告） | `bash scripts/audit-verify.sh docs/assessment/2026-11-14-wave2-three-vendor-matrix.md` | ✅ 通过 3 / 失败 0 / 警告 3（弱语义，纯路径引用） | 0 |
| G1 | `bash scripts/check-fmt-all.sh` | ✅ 72 包通过 | 0 |
| G30（报告声称核对） | `python scripts/check-phantom-tests.py` | ✅ 硬规则 0 命中（软规则 861 提示 + P3 存量豁免 182） | 0 |

报告 §3 门禁表声称（G1 72 包 / G9 46 Safe / G37 0 命中自测 19 / G30 通过）与红队复跑一致。

## 2. 真连复跑（三实例容器均在运行，2026-11-14）

| 套件 | 命令要点 | 结果 | EXIT |
|------|---------|------|------|
| Kingbase 矩阵 | `SZ_ORM_KB_URL=postgres://SYSTEM:<pw>@127.0.0.1:15435/test cargo test -p sz-orm-core --test integration_kingbase matrix_kb -- --include-ignored` | **4 passed**（matrix_kb1_engine_identity / kb2_delegate_upsert_forwarded / kb3_delegate_fk_forwarded / kb4_numeric_and_types） | 0 |
| Kingbase 既有回归 | `cargo test -p sz-orm-core --test integration_kingbase`（默认套件） | **5 passed; 7 ignored**（既有 5 单测零破坏，与报告 §3 一致） | 0 |
| GBase 矩阵 | `cargo test -p sz-orm-core --test integration_gbase matrix_gb -- --include-ignored` | **3 passed**（gb1_engine_identity_bare_sql / gb2_quote_forms_on_pure_8s / gb3_dialect_shape_vs_8s_syntax） | 0 |
| GBase 默认套件（R-1 取证） | `cargo test -p sz-orm-core --test integration_gbase`（无 --include-ignored） | **7 passed; 4 ignored——其中 `matrix_gb3_dialect_shape_vs_8s_syntax ... ok` 在默认套件真执行**（0.66s 含 docker exec 往返），证实容器依赖泄漏 | 0 |
| Dameng 矩阵 | `cargo test -p sz-orm-core --test integration_dameng matrix_dm -- --include-ignored` | **3 passed**（dm1_engine_identity / dm2_oracle_types_roundtrip / dm3_delegate_surface） | 0 |
| Dameng 全量（Oracle 回归） | `source /d/tools/gate.env`（SZ_ORM_ORACLE_PASSWORD）+ PATH OCI 后 `cargo test -p sz-orm-core --test integration_dameng -- --include-ignored` | **12 passed; 0 failed**（DM 3 + 既有 Oracle 直测 3 + 方言单测 6，与报告 §3「12/12」一致） | 0 |

锚点行核实（audit-verify 机检口径）：KB1~KB4 = packages/sz-orm-core/tests/integration_kingbase.rs:299/:318/:374/:464；GB1~GB3 = packages/sz-orm-core/tests/integration_gbase.rs:301/:315/:333；DM1~DM3 = packages/sz-orm-core/tests/integration_dameng.rs:351/:370/:418——与报告 §2 载荷锚点声明逐一相符。

## 3. 关键声称对抗审查

### 3.1 R-A「GBaseDialect→SqlServerDialect 委托仅 TDS 网关企业实例可执行；纯 8s [bracket] 拒/双引号收」——**成立（红队容器内独立复现，非借用测试 helper）**

红队直接 `wsl docker exec gb8s bash -c "su - gbasedbt -c 'echo ... | dbaccess szorm_test'"` 四层引号手工成形：

1. `select [id] from t_probe;` → `201: A syntax error has occurred.`（`Near character position 8`）——[bracket] 被纯 8s 拒绝；
2. `select "id" from t_probe;` → `1 row(s) retrieved.`——双引号 delimited identifier 被接受；
3. `onstat -` → `On-Line -- Up 02:46:47`；`onstat -V` → `GBase8sV8.8_AEE_3.5.0_3X1_ea855d`——版本与报告 §1「V8.8 AEE 3.5.0」相符。

报告 §2-GB2 与 §4 条目 3 的引擎事实登记成立。使用边界（委托仅 TDS 网关企业实例可执行）属实：GBaseDialect→SqlServerDialect 委托定义在 packages/sz-orm-core/src/dialect.rs:2205，其 quote 产物 `[id]` 在纯 Informix 协议实例不可执行。

### 3.2 R-B「KB2 修复前 Kingbase upsert 恒 None」——**成立（对照 OC 批父提交宏体）**

注意口径：KB2 声称的「修复」是 OC 批 8230561d（delegate 宏 4 转发），本批 70db9577 仅加测试——`70db9577^` 已含修复，真正前态是 `8230561d^`。红队取 `git show 8230561d^:packages/sz-orm-core/src/dialect.rs` 宏体（macro_rules! delegate_dialect_to 全体，源文件 2041~2148 行）逐行核：**宏体 28 个转发方法中无 `build_upsert_on_conflict`/`build_add_foreign_key`/`build_create_table_with_options`/`render_table_options`**——修复前委托方言落 trait 默认（upsert=None，packages/sz-orm-core/src/dialect.rs:295）。当前 HEAD 宏体 4 转发在位（packages/sz-orm-core/src/dialect.rs:2144 注释起，:2147 upsert 转发），且 KingbaseDialect→PostgreSqlDialect（packages/sz-orm-core/src/dialect.rs:2187）、DamengDialect→OracleDialect（:2199）、GBaseDialect→SqlServerDialect（:2205）。**KB2 为 OC 批修复的真实行为级证据**：PG 覆写 build_upsert_on_conflict（packages/sz-orm-core/src/dialect.rs:1056）→ 转发后 Kingbase 从 None 变为真渲染，真库 EXCLUDED 冲突更新复跑通过。DM/GB 通道无对应差异化（见 R-3）。

### 3.3 R-C「DM8 五 env 全给」——**成立（entrypoint 源码 + 运行时 env 双重核对）**

`docker run --rm --entrypoint bash sizx/dm8 -c "cat /usr/local/bin/docker-entrypoint.sh"` 全文取证：`db_init()` 执行 `dminit PATH=/home/dmdba/data PAGE_SIZE=${PAGE_SIZE} CHARSET=${CHARSET} LENGTH_IN_CHAR=${LENGTH_IN_CHAR} CASE_SENSITIVE=${CASE_SENSITIVE} SYSDBA_PWD=${SYSDBA_PWD}`——恰五个 env，且 PATH 挂载点为 `/home/dmdba/data`（非 /opt/dmdbms/data），与报告 §1 登记逐字相符。运行时核对 `docker inspect dm8-szorm`：`PAGE_SIZE=16 / CHARSET=1 / LENGTH_IN_CHAR=1 / CASE_SENSITIVE=0 / SYSDBA_PWD=SYSDBA001` 五项齐备；Mounts 为 volume `dm8data6`→`/home/dmdba/data`。附带红队补证（R-4/R-3 相关）：以 Oracle 形态 FK DDL（`ALTER TABLE "DM_RT_CHILD" ADD CONSTRAINT "fk_DM_RT_CHILD_pid" FOREIGN KEY ("PID") REFERENCES "DM_RT_PARENT" ("ID") ON DELETE CASCADE`，即 trait 默认经 Oracle quote 的产物）经 disql stdin 探针——DM8 返回 `executed successfully`，插父/子行后删父行、COMMIT，`SELECT COUNT(*)` 读回 **N=0**——DM8 兼容该 DDL 形态且 CASCADE 级联行为级成立（注：该 DDL 形态在 OC 批前后对 DM 无差异，OracleDialect 未覆写 build_add_foreign_key，见 R-3）。

## 4. 报告可信度核对

| 声称 | 红队核对 | 结论 |
|------|---------|------|
| 供给表（报告 §1 三行） | `docker ps`：dm8-szorm=sizx/dm8@52336 / kb4=haloking/kingbase:v8r3@15435 / gb8s=liaosnet/gbase8s@19088——镜像、端口逐项相符；另 kb2=xuxuclassmate@15434、kb-szorm=chyiyaqing@15432 在跑（与「license 2021 过期不可用」叙述吻合，过期性红队未独立复核——非载荷关键路径） | 相符 |
| Kingbase 版本 V008R003C002B0320 | `docker exec kb4 ksql --version` → `ksql (Kingbase) V008R003C002B0320` | 相符 |
| DM8 license 14 天至 2026-11-27 | `docker logs dm8-szorm` → `file dm.key not found, use default license!` + `License will expire in 14 day(s) on 2026-11-27`；`create dm database success. 2026-11-13 23:14:31` | 相符 |
| 供给结论纠正（OC 批 §0 假阴性） | 镜像均在本地 `docker images`（haloking/chyiyaqing/xuxuclassmate/liaosnet/sizx + 1ms/1panel mirror 前缀的 sizx/dm8 双份）——mirror pull 通道可用性由镜像存在性佐证 | 成立 |
| 报告 §2 三实例「无库内行号可引，锚点=测试函数定义行」 | audit-verify 3 通过 0 失败（3 警告为弱语义提示） | 口径成立 |

## 5. 登记一致性（commit vs diff）

- `70db9577`：name-only = integration_kingbase.rs（+233）/ integration_gbase.rs（+74）/ integration_dameng.rs（+118，新建载荷），**425 insertions / 0 deletions，无越批文件**（无 src/、无 Cargo.toml）——message「KB1~KB4 + GB1~GB3 全过 + DM1~DM3 载荷就绪」与 diff 一致（DM 当时未起服，仅载荷，message 亦如实标注「sizx/dm8 mirror layer 损坏反复重试中」）。
- `eab60aa3`：name-only = integration_dameng.rs（+35/-11），无越批文件——message「DM1~DM3 真连全过 + 五 env 要点 + 套件全量 12/12」与 diff 及红队复跑一致。
- 六环第 6 环状态：CHANGELOG.md:12 起已登记（[Unreleased] Added 段，70db9577+eab60aa3 双 hash 在位）；报告文件未提交（R-7）；AGENTS.md「已发布」行待登记批更新。

## 6. audit-verify 自证

```
$ bash scripts/audit-verify.sh docs/assessment/2026-11-14-wave2-three-vendor-redteam-review.md
==========================================
  审计证据验证: docs/assessment/2026-11-14-wave2-three-vendor-redteam-review.md
  内容口径: 工作区
==========================================
  ✅ 通过: 29
  ❌ 失败: 0
  ⚠️  警告: 12（弱语义提示，均为纯路径/锚点引用，全部行号实证在位）
==========================================
✅ 审计证据验证通过
AV_SELF_EXIT=0
```

## 7. 裁决

**条件 PASS。** 三实例真连全过、供给结论纠正、GBase [bracket] 引擎边界、KB2 行为级证据均独立复现成立，无 P0。放行条件（随第 6 环登记批一并处置）：

1. **R-1（P1，放行前置）**：`matrix_gb3` 补 `#[ignore]`，恢复 gate 4 环境无关性；
2. **R-2（P2）**：报告 §1 登记 t_probe 前置 SQL（或测试内自建自清理）；
3. **R-3（P2）**：报告 §4.5/§5 措辞修订为「KB 通道行为级；DM/GB 通道生成面/边界登记」；
4. R-4~R-7（P3）：择机处置，不阻断。

处置完成后按第 6 环收口（CHANGELOG [Unreleased] 增补红队回执 hash + AGENTS.md「已发布」行 + 报告/本回执同批提交）。
