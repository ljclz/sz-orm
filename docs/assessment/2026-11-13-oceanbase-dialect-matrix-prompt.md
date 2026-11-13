# 新会话提示词：sz-orm 方言矩阵二波补批——OceanBase 真连验证（OC1~OC6，v1，2026-11-13）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：sz-orm v10.13.4。方言矩阵一/二波已闭环：MySQL(MY)/Oracle(OR)/PG(PG)/SQLite(SQ)/容器六实例(MZ-*，MSSQL/MariaDB/TiDB/ClickHouse/DB2/openGauss)。本批为容器化二波登记（`2026-11-13-container-dialect-matrix.md` §B9）的首个可自动供给实例——**OceanBase CE 4.4.2.1**（MySQL 兼容族，委托 MySqlDialect）。
> **供给事实（本机已就绪）**：
> - 镜像 `oceanbase/oceanbase-ce:latest`（2.05GB，4.4.2.1）已拉取；实例容器 `ob-final2` 运行中，宿主端口 **12881**
> - 连接：`mysql://root%40szorm:Szorm2026@127.0.0.1:12881/sz_orm_test`（注意 `root@szorm` 是**租户用户**，URL 里 `@` 要编码为 `%40`）；`ob_compatibility_mode=MYSQL`（已实测）
> - **容器特性（重要坑位）**：daemon 模式下 entrypoint boot 后约 2 分钟容器退出（exit 255，非 OOM），`--restart=always` 已配置自动拉起；每次拉起后 observer 需要 **约 2~2.5 分钟**才可连（`ERROR 8001 Server is initializing` → 可连）。跑测试前用「守门探活」：循环 `mysql SELECT 1` 直到成功再触发测试套件（一次性成功窗口约 2 分钟，integration_oceanbase 8 项约 0.7s 可全过）
> - 可选更稳路径：`docker commit ob-final2 ob-szorm-v2:latest` 已有快照镜像（含 szorm 租户密码 + sz_orm_test 库），新容器从快照起只需等 observer 可连
> - python 侧无 mysql 客户端；容器内有 obclient：`docker exec ob-final2 obclient -h127.1 -uroot@szorm -pSzorm2026 -P2881 -e "..."`
>
> **六环闭环**：按评测交付闭环链条执行；Kingbase/Dameng/GBase 经官网评估（金仓 docker tar 需验证码表单、达梦 eco 站无公开 docker 入口、GBase 8s 开发版需登录）均为**人工供给门槛**，不在本批自动供给范围——报告 §供给边界 登记即可，勿花会话预算尝试绕过。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：以 OceanBase（MySQL 兼容族）方言能力面为载荷做真连矩阵验证，产出三分类清单（**sz-orm 方言缺口**[修复]/ **OceanBase 引擎特有**[登记]/ **文档措辞**），并修复 sz-orm 侧可修复项。

**目标仓库与环境**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4）
- 被测：OceanBase CE 4.4.2.1（MySQL 模式，ob-final2@12881，连接见上）
- 方言层：`packages/sz-orm-core/src/dialect.rs`（OceanBaseDialect = delegate_dialect_to! → MySqlDialect，`dialect.rs:2154`）
- 通道：既有 `integration_oceanbase.rs` 8 项（真实 DB 5 + ignored 3）不重复
- 参考：`integration_mariadb.rs` 的 `matrix_mz_*` 追加模式（同族兼容矩阵先例）

## 评测矩阵（OceanBase 特有面；每载荷 = 方言层生成 + 真库执行 + 行为断言三步）

| # | 能力块 | 载荷（方言生成面） | 重点 |
|---|--------|-------------------|------|
| OC1 | 引擎身份契约 | `@@version_comment`（OceanBase_CE 4.4.2.1）+ `ob_compatibility_mode=MYSQL`；方言 `db_type()=OceanBase`、`is_mysql_family()=true`、DialectKind 映射 MySQL（`dialect.rs:5672`） | 委托选择的真库身份证明；version 字符串前缀 5.7.25 与 MySQL 9.6 的差异对 build_pagination 等无影响（纯文本协议面） |
| OC2 | DDL 生成面 | build_create_table（含表选项 build_create_table_with_options——MY7 B1 API 在 OB 上的行为：ENGINE/CHARSET 是否被接受/忽略）；build_alter_table 三臂（AddIndex/DropIndex/FK 提示串）；quote 往返（保留字表列名 + 混合大小写） | OB 对 MySQL DDL 方言的接受度；表选项报错还是静默（差异登记）；G2/G37 同型 quote 回归 |
| OC3 | Upsert 家族 | build_upsert_on_conflict（ON DUPLICATE KEY UPDATE 渲染）+ VALUES() 引用真库行为；INSERT IGNORE（build_insert_or_ignore_prefix）；build_batch_upsert_with_params 参数化路径 | OB MySQL 模式 ON DUPLICATE KEY 语义与 MySQL 8.0.29 对拍（MY 批基线）；IGNORE 的 warning 计数行为 |
| OC4 | 类型与值面 | Value 往返（I64/U64 边界/F64/Decimal/DateTime/Bytes）；OB NUMBER(p,s) vs MySQL DECIMAL 精度行为（38 位大数——对照 PG 批 R2 通道结论）；UNIQUE 冲突错误码 1062 映射 | 弱类型差异：OB MySQL 模式下 ZERO DATE/严格模式默认值；错误码与 MySqlDialect 假设一致性 |
| OC5 | 分布式特有 | OB 特有：`SHOW VARIABLES` 大量子集（ob_ 前缀）；分区表 DDL（PARTITION BY HASH——方言生成面不支持则登记引擎特有）；隐式主键/自增（AUTO_INCREMENT 分配模式）；事务隔离 READ COMMITTED 默认值 | 分布式数据库与单机 MySQL 的行为边界——sz-orm 方言生成面**不涉及**的部分登记为引擎特有，不要求方言支持 |
| OC6 | SQL 注入防护 | 参数化路径真连验证（where_eq 绑定 + 注入向量惰性——SDET ROI-6 同型）；QueryBuilder 生成分页 LIMIT/OFFSET 真库执行 | 门禁 9 行为级补强；OB 错误信息回显形态与 MySQL 差异登记 |

**不纳入**（登记理由）：OB 特有租户管理/资源池（DBA 运维面）、OCP 管理台、OBProxy、LS/分区迁移内核行为。

## 硬性要求

- **语义基准**：OceanBase 官方文档（4.4 CE MySQL 模式）+ 真库实测；MySQL 8.0.29（宝塔@8802）可作同族对照，但**禁止拿 PG/Oracle 当仲裁**
- **守门探活纪律**：任何 cargo test 前先确认容器可连（循环探活脚本见背景）；测试失败先看容器 Status 再判测试问题——**禁止把容器拉起窗口期的 PoolTimedOut 记为方言缺陷**
- **sz-orm 方言缺口**：修复 `dialect.rs`（OceanBase 委托族）+ 单测 + 真连断言；MySqlDialect 同型面顺带回归（MY 批基线不破坏）
- **参数化铁律**（门禁 9）；零 unsafe、零 todo!
- **测试产物**：追加 `matrix_oc_*` 到 `packages/sz-orm-core/tests/integration_oceanbase.rs`（MZ 批同型——载荷追加既有文件，不新建文件）；env `SZ_ORM_MYSQL_URL` 注入，未配置自动跳过（沿用既有 mysql_url() 模式）
- **提交纪律**（硬约束 13）；串行 cargo；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / clippy -D warnings / fmt 72 包全绿（`bash scripts/check-fmt-all.sh`）
2. `SZ_ORM_MYSQL_URL=<ob url> cargo test -p sz-orm-core --test integration_oceanbase -- --include-ignored`：OC 载荷 + 既有 8 项全过（守门探活后跑）
3. 回归：`cargo test -p sz-orm-core --test integration_mysql`（宝塔 8802，MY 批基线）不回归
4. 三分类清单：每条附证据（生成 SQL + 真库输出 + `dialect.rs` file:line **全路径**——红队 P2-1 教训：短路径会被 audit-verify 机检跳过）
5. 门禁：G9（--strict + 自测 15/15）/G12/G14/G15/G16/G33/G35/G36/G37 全过
6. 报告 `docs/assessment/2026-11-13-oceanbase-dialect-matrix.md`，audit-verify 全路径引用 0 失败；六环登记（CHANGELOG/AGENTS/commit）；第 5 环红队由独立 Agent 复核（background-task 启新 session）

## 环境坑位速查

- git 在 `D:\Git`；`cargo fmt --all` 用 `bash scripts/check-fmt-all.sh`（os error 206 规避）
- **容器死亡循环**：`ob-final2` ~2min 退出是 daemon+entrypoint 已知行为（restart=always 兜底）；跑套件用守门探活；报告须登记该环境事实（非 sz-orm 缺陷）
- URL 编码：`root@szorm` → `root%40szorm`（sqlx/mysql crate 双侧）
- docker 命令须 `wsl bash -c '...'` 单引号包裹（双引号内 `$` 被宿主 bash 吞）
- G37 门禁（dialect DDL/FTS 裸标识符检测）：`python scripts/check-dialect-ddl-quoting.py --self-test`
- 临时脚本放 `D:\tmp`；`python -I` 运行；写文件用 Write 工具（Bash heredoc 写文件被 hook 拒绝）
- 引用格式（audit-verify）：**全路径 + 真实行号**，引用行不能是空行（红队 P2-1 实测教训）
