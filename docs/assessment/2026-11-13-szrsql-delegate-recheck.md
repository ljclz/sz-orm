# SzRSQL 委托链补验（SZ1~SZ3）——2026-11-13

> 六环闭环批次。定位：**OC 批（OceanBase 方言矩阵，commit 8230561d）delegate 宏转发缺口修复**（`packages/sz-orm-core/src/dialect.rs:2147-2172`）影响面中 SzRSQL 通道的真连补验——M1~M9 既有矩阵载荷未覆盖 upsert/表选项/FK 三个新转发 API 在委托方言上的行为。
>
> **szrsql 线此前的评测覆盖**（均已闭环，见各自报告）：SzRSQL 集成（2026-11-08）/ M1~M9 矩阵差分（2026-11-09，六环闭环含红队）/ SzRSQL 一等公民四项（A1~A4）/ szrsql 线转交三项（2026-11-13）。本批不重复上述载荷。

## 1. 实例与探活

- 二进制：`D:/cargo-target/release/szrsql.exe`（szrsql 源码仓 HEAD 编译坏——`check_option`/`view_where` 缺字段，用既有 release 二进制，PG 批已登记事实）
- 启动：`szrsql.exe --port 6543 --auth-mode trust --btree-shard-count 1 --data-dir D:/tmp/szrsql_matrix_6543`（后台，netstat 确认 LISTEN）
- 连接：`SZ_ORM_SZRSQL_URL="postgres://postgres@127.0.0.1:6543/postgres"`（trust 模式免密）
- 探活：`integration_szrsql` 8/8 + `integration_szrsql_matrix` 15/15 既有项全过（2026-11-13 实测）

## 2. 交付物

| 项 | 内容 | 证据 |
|---|------|------|
| 测试 | `packages/sz-orm-sqlx/tests/integration_szrsql_matrix.rs` 追加 `matrix_sz_*` 3 测试（SZ1~SZ3） | `cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --include-ignored` → **18 passed; 0 failed**（2026-11-13 实测） |

## 3. 矩阵结果逐载荷

### SZ1 upsert 委托链（行为级）
- 生成面：`SzrsqlDialect.build_upsert_on_conflict(["id"], ["v"], ..)` 修复前落 trait 默认返回 **None**（`build_batch_upsert_with_params` 对 SzRSQL 不可用）；修复后委托 PG 渲染 `ON CONFLICT ("id") DO UPDATE SET "v"=EXCLUDED."v"`。
- 真库执行：upsert 插入（v=10）→ 冲突再插（v=99）→ 读回 `v=99`（EXCLUDED 通道冲突更新生效）。

### SZ2 表选项委托链（契约级）
- 修复前：委托方言调用 `build_create_table_with_options` 与 `build_create_table` 等价是「巧合正确」（trait 默认 render_table_options 空串恰好匹配 PG 无表选项语义）。
- 修复后：经转发走 PG 的 `render_table_options`——PG 族契约「表选项忽略、产物与 base 一致」经断言固化（注入 MySQL 语法的回归面被反向断言封死）。
- 真库执行：产物 `CREATE TABLE ...` 可执行。

### SZ3 FK 动作委托链（行为级）
- 生成面：委托 PG 的 `build_add_foreign_key`（约束名/列名 quote + ON DELETE CASCADE 白名单校验通过）。
- 真库执行：`ADD CONSTRAINT` FK 被接受；行为级 **ON DELETE CASCADE 级联生效**（删父行 → 子行消失）。

## 4. 三分类清单

| # | 分类 | 内容 | 处置 |
|---|------|------|------|
| 1 | sz-orm 方言缺口 | 与 OC 批同一处（delegate 宏 4 转发缺漏）——本批为该修复的 SzRSQL 通道补验，无新缺口 | 已随 OC 批修复；本批 3 载荷全部真连通过即为闭环证据 |
| 2 | SzRSQL 引擎特有 | PG 族契约「表选项忽略」在 SzRSQL 上成立（SZ2 反向断言）；FK CASCADE 行为级一致（SZ3） | 登记 |
| 3 | 文档措辞 | 无新增 | — |

## 5. 门禁记录（2026-11-13 实测）

| 门禁 | 结果 |
|------|------|
| 矩阵 `integration_szrsql_matrix -- --include-ignored` | ✅ **18/18**（SZ 3 + M1~M9 既有 15） |
| 回归 `integration_szrsql -- --include-ignored` | ✅ 8/8 |
| G1 fmt | ✅ 72 包（sz-orm-sqlx 追加载荷后 cargo fmt 修复） |
| G3 clippy（sz-orm-sqlx --tests） | ✅ 0 error |

## 6. 结论

OC 批 delegate 宏转发修复在 SzRSQL 通道真连补验完成：upsert EXCLUDED 渲染 + 冲突更新行为、PG 表选项忽略契约、FK CASCADE 级联行为全部真库通过。矩阵 18/18、基础套件 8/8 零回归。szrsql 线与 OC 批（OceanBase）同源于同一修复的两侧验证。
