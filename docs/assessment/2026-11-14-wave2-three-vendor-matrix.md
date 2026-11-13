# 二波三实例真连验证（KB1~KB4 / GB1~GB3 / DM1~DM3）——2026-11-14

> 六环闭环批次。容器化二波剩余三实例（Kingbase/Dameng/GBase）供给纠正后真连验证——**纠正 2026-11-13 报告（`2026-11-13-oceanbase-dialect-matrix.md` §0）的「人工供给门槛」结论**：该结论系 `docker search` 直连 `index.docker.io` 被 GFW 超时的假阴性，本机 daemon 配置 mirror（1panel/1ms/daocloud）后 pull 通道可用，三家社区镜像均可自动供给。
>
> 实施批：commit `70db9577`（KB+GB）+ `eab60aa3`（DM）。

## 1. 供给事实（三厂容器与凭据）

| 实例 | 镜像 | 版本实测 | 端口/凭据 | 协议/断言通道 |
|------|------|---------|----------|--------------|
| KingbaseES | `haloking/kingbase:v8r3` | V008R003C002B0320（`chyiyaqing:v8r6`/`xuxuclassmate` 内置 license 2021 过期不可用） | 54321→15435，SYSTEM/SYSTEM | PG wire → sqlx 直连（全 Rust 断言） |
| GBase 8s | `liaosnet/gbase8s:v8.8_3503x1_x64` | V8.8 AEE 3.5.0（onstat On-Line） | 9088→19088，gbasedbt | Informix 协议，无 Rust 驱动 → 容器内 dbaccess CLI（MZ 批 DB2 CLP 同型） |
| Dameng DM8 | `sizx/dm8` | DM8 V8（内置 license 14 天，2026-11-27 到期） | 5236→52336，SYSDBA/SYSDBA001 | 自有协议，无 Rust 驱动 → 容器内 disql CLI（同型） |

DM8 容器启动要点（登记）：entrypoint **五个 env 全给**（PAGE_SIZE/CHARSET/LENGTH_IN_CHAR/CASE_SENSITIVE/SYSDBA_PWD，缺一报 `[X] value error`）+ 挂载路径 `/home/dmdba/data`（非 /opt/dmdbms/data）。

## 2. 矩阵结果

证据形态：三实例均为容器/CLI 通道（无库内行号可引），可校验锚点为**测试文件函数定义行**与套件输出；执行命令与结果数字见各载荷段。

### Kingbase（KB1~KB4）

载荷锚点：packages/sz-orm-core/tests/integration_kingbase.rs:299 起（KB1 至 KB4 定义行 299/318/374/464）。
- **KB1 引擎身份**：`version()` 含 Kingbase；委托链 quote 双引号（KingbaseDialect→PostgreSqlDialect）。
- **KB2 upsert 委托链（OC 批 delegate 修复的 KB 通道行为级）**：修复前 KingbaseDialect.build_upsert_on_conflict 落 trait 默认 None；修复后渲染 `ON CONFLICT ("id") DO UPDATE SET "v"=EXCLUDED."v"`，真库冲突更新（10→99）验证。
- **KB3 FK 动作委托链**：`build_add_foreign_key`（quote + CASCADE 白名单）→ ADD CONSTRAINT 接受 → ON DELETE CASCADE 级联行为级（删父行子行消失）。
- **KB4 类型**：NUMERIC(38,6) 20 位整数大数无损往返（PG 族通道，对照 PG 批 R2）。
- 坑位（登记）：sqlx INT4 列须 i32 解码（i64 报 mismatched）；COUNT(*) 结果 INT8 须 `::BIGINT` cast。

### GBase 8s（GB1~GB3）

载荷锚点：packages/sz-orm-core/tests/integration_gbase.rs:301 起（GB1 至 GB3 定义行 301/315/333）。
- **GB1 引擎身份**：sysmaster 直连裸 SQL（`select first 1 tabname from systables`）可执行。
- **GB2 方言 quote 形态行为级（重要引擎事实）**：GBaseDialect 委托 SqlServerDialect 的 `[bracket]` quote 在纯 8s Informix 协议实例被拒（201 syntax error），双引号 delimited identifier 被接受——**GBaseDialect→SqlServerDialect 委托仅在带 TDS 网关的企业实例上可执行，纯 Informix 社区版不可执行**（既有 TDS 用例的前提是腾讯云企业实例；登记为文档措辞/使用边界）。
- **GB3 分页语法对照**：8s 原生 `SKIP..FIRST`（SKIP 前置）分页可执行——与 SqlServerDialect 的 OFFSET..FETCH 形态差异登记。
- 坑位（登记）：四层嵌套引号（Rust→wsl bash -c→docker bash -c→su）须一次性成形，二次 replace 层错（GB2 初版教训）。

### Dameng（DM1~DM3）

载荷锚点：packages/sz-orm-core/tests/integration_dameng.rs:351 起（DM1 至 DM3 定义行 351/370/418）。
- **DM1 引擎身份**：disql 连通（mode normal, state is open）；Oracle 风格双引号 alias（`SELECT 1 AS "Val" FROM DUAL`）真执行；委托链 quote 双引号。
- **DM2 Oracle 类型往返**：方言 DDL 产物（`CREATE TABLE "DM_PROBE" ("ID" NUMBER(10) NOT NULL PRIMARY KEY, "NAME" VARCHAR2(100))`）真库 `executed successfully`；INSERT `affect rows 1`；**COMMIT 后**跨会话读回 dm8。
- **DM3 delegate 生成面**：Oracle 族 upsert 契约 None（trait 文档化行为——MERGE 由调用方自管，登记非缺口）；FK DDL 形态（quote + CASCADE）。
- 坑位（登记）：①disql 无分号进续行模式（`SQL> 2` 静默挂起——方言产物不带分号须补齐）；②DM8 默认非自动提交，独立会话 INSERT 后须 COMMIT；③**SQL 经 stdin 直喂 disql（零转义）**——echo 引号嵌套在 wsl 默认 shell 层会丢 `$`（V$VERSION 屡次变 V，GB 线四层嵌套教训升级版）；④`LD_LIBRARY_PATH=/home/dmdba/dmdbms/bin`（disql 动态库）。

## 3. 回归与门禁

| 项 | 结果 |
|---|------|
| `integration_dameng` 全量（DM 3 + 既有 Oracle 直测 9） | ✅ 12/12（gate.env 凭据，2026-11-14 实测） |
| `integration_kingbase` 既有 5 项 | ✅ 零破坏 |
| `integration_gbase` 既有 7 项 | ✅ 零破坏 |
| G1 fmt | ✅ 72 包 |
| G2/G3（core tests + clippy） | ✅ 0 error |
| G9 --strict | ✅ 46/46 Safe |
| G37 | ✅ 0 命中（自测 19/19） |
| G30 phantom-tests | ✅ 通过 |

## 4. 三分类清单

| # | 分类 | 内容 | 处置 |
|---|------|------|------|
| 1 | **供给结论纠正** | 三厂社区镜像可自动供给（KingbaseES V8R3 haloking/GBase 8s liaosnet/DM8 sizx）——推翻 2026-11-13 OC 报告 §0「人工门槛」 | 报告 §0 修订（本报告 §1）；DM8 内置 license 14 天（2026-11-27 到期，届时需官网试用 license 替换） |
| 2 | 引擎特有（Kingbase） | sqlx INT4→i32 解码约束；V8R3 与 PG 行为一致（EXCLUDED/CASCADE/NUMERIC） | 登记 |
| 3 | **引擎特有（GBase 8s，重要）** | GBaseDialect→SqlServerDialect 委托仅 TDS 网关企业实例可执行；纯 Informix 社区版 [bracket] 拒/双引号收/SKIP..FIRST 前置 | 登记使用边界（文档措辞级） |
| 4 | 引擎特有（Dameng） | disql 续行模式/非自动提交/stdin 零转义；Oracle 族 upsert None 契约；DM8 兼容 Oracle 风格 DDL/双引号 quote | 登记 |
| 5 | sz-orm 方言缺口 | **0 处**——KB2 upsert 为唯一行为级差异化证据（EXCLUDED 真库生效）；DM/GB 为转发到位+产物合法执行级（FK/表选项无覆写，输出与默认实现零差异） | 无需改码 |

## 5. 结论

二波三实例全部真连验证通过（KB 4 + GB 3 + DM 3 = 10 载荷），delegate 宏转发修复（OC 批）行为级差异化证据在 KB2（upsert 是 4 转发中唯一有基础方言覆写的——PG EXCLUDED vs None）；DM/GB 通道为转发到位 + 产物合法执行证据（FK/表选项无覆写、输出与默认实现零差异，GB 经 CLI 生成面）。方言矩阵系列至此覆盖：MySQL/Oracle/PG/SQLite/容器六实例/OceanBase/SzRSQL/Kingbase/GBase 8s/Dameng。
