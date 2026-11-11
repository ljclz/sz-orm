# 新会话提示词：sz-orm Oracle 方言矩阵真连验证（v1，2026-11-11）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：sz-orm v10.13.4。SzRSQL（×PG18 + ×SQLite）矩阵差分已六环闭环。本批为 Oracle 方言层真连矩阵验证——评测对象是 **sz-orm 自己的 OracleDialect 生成面**：载荷按 `DbType::Oracle` 生成 SQL，在真 Oracle 26ai Free 上执行，验证生成被接受且结果符合 Oracle 自身语义。
> **与 MySQL 批的关系**：同模板、可同会话先后执行（建议 MySQL 批先行——服务就绪度更高）；Oracle 特有面（空串=NULL、RETURNING INTO、标识符大写折叠）单独成块。
> **六环闭环**：按评测交付闭环链条执行。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：以 Oracle 方言能力面为载荷做真连矩阵验证，产出三分类清单（**sz-orm 方言缺口**[修复]/ **Oracle 环境特有**[26ai Free 配置类，登记]/ **文档措辞**），并修复 sz-orm 侧可修复项。

**目标仓库与环境**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4）
- 被测：Oracle 26ai Free（23ai 家族）@ `127.0.0.1:1521/freepdb1`，用户 `sz_orm_test`（已授 DBA）
- 凭据：`SZ_ORM_ORACLE_PASSWORD` 环境变量注入（**不入库**；缺失则测试 assert 拒跑——见 `integration_oracle.rs:oracle_password()`）；OCI 客户端库需 `export PATH="/d/oracle/product/26ai/dbhomeFree/bin:$PATH"`
- 连接通道：`sz-orm-core/tests/integration_oracle.rs` 用 `oracle` crate 直连（15 项 `#[test]`）；`packages/sz-orm-sqlx` 走 `sz-orm-oracle` 通道（`OracleConnectionFactory`/`OraclePoolHandle`，见 `unified_pool.rs:39`）
- 既有覆盖：`integration_oracle.rs` 15 项（方言/转义/事务/批量插入基线）——矩阵聚焦**方言生成面**增量

## 环境探活（第一步）

```bash
export PATH="/d/oracle/product/26ai/dbhomeFree/bin:$PATH"
export SZ_ORM_ORACLE_PASSWORD=<sz_orm_test 密码>
# 探活（sqlplus 必须 cmd //c + .sql 脚本文件，命令行直传有编码坑）
sqlplus -L sz_orm_test/<pw>@127.0.0.1:1521/freepdb1 @D:/tmp/oracle_probe.sql
# 或直接跑既有测试验证通道：cargo test -p sz-orm-core --test integration_oracle -- --ignored
```

## 评测矩阵（Oracle 方言特有面；每载荷 = 方言层生成 + 真库执行 + 行为断言三步）

| # | 能力块 | 载荷（方言生成面） | 重点 |
|---|--------|-------------------|------|
| OR1 | 空串与 NULL | `INSERT ''` 后 `IS NULL` 判定；`'' IS NULL`；CONCAT 空串传播 | **Oracle 空串即 NULL**（与 MySQL/PG 相反）——方言层生成不假设空串语义，断言锚定 Oracle 行为 |
| OR2 | 标识符大小写折叠 | 未加引号标识符 → 真库存大写；小写加双引号标识符往返；30/128 字节标识符长度（12.2+ 为 128） | sz-orm 生成的混合大小写标识符在真库的可见性（元数据探测 `ALL_TAB_COLUMNS` 大写匹配面） |
| OR3 | 主键回填 | IDENTITY 列（12.1+）`GENERATED ALWAYS/BY DEFAULT`；序列 + 触发器旧路径；**`INSERT .. RETURNING id INTO ?`**（sz-orm Oracle 主键回填通道） | RETURNING INTO 绑定参数往返在 rust-oracle 通道的正确性（sz-pay 类生产路径前置） |
| OR4 | 分页与排序 | `OFFSET n ROWS FETCH NEXT m ROWS ONLY`（12c+）vs 方言是否还生成 ROWNUM 旧式；NULLS FIRST/LAST（Oracle **支持**该语法，与 MySQL 不同）；ORDER BY 含 NULL | 方言分页生成面现代化检查；NULLS 语法透传 |
| OR5 | 类型往返 | NUMBER(p,s) 精度（38 位）；VARCHAR2(4000)/CLOB 边界；DATE/TIMESTAMP 时区；RAW/BLOB | NUMBER 大整数/高精度往返不丢位；CLOB 边界写入 |
| OR6 | 边缘语义 | 三值 NULL（`= NULL` 恒空）；除零（ORA-01476 报错，无 sql_mode 开关）；隐式转换（VARCHAR2 数字比较）；DUAL 表（`SELECT 1 FROM DUAL`——方言层无 FROM 查询生成面）；事务内 DDL（Oracle DDL 隐式提交！与 MySQL/PG 重大差异） | **DDL 隐式提交**对 sz-orm 事务管理器的影响探针（migration/schema_sync 场景）；方言层 SELECT 无 FROM 是否自动补 DUAL |

**不纳入**（登记理由）：RAC/DataGuard（运维面）、AWR/性能诊断、PL/SQL 存储过程体生成（sz-orm 不生成 PL/SQL）。

## 硬性要求

- **语义基准**：Oracle 26ai（23ai 家族）官方语义 + 真库实测；禁止拿 PG18/MySQL 当仲裁
- **sz-orm 方言缺口**：生成 SQL 被真库拒绝（ORA-xxxx）/结果错位 → 修复 `dialect.rs` OracleDialect 并补单测 + 真连断言；真库自身行为（DDL 隐式提交等）只登记 + 在事务管理器文档标注
- **参数化铁律**（门禁 9）：所有测试 SQL 参数化绑定；零 unsafe、零 todo!
- **测试产物**：新增 `packages/sz-orm-core/tests/integration_oracle_matrix.rs`（沿用 `integration_oracle.rs` 的 env 注入 + assert 拒跑模式）
- **提交纪律**（硬约束 13）：每能力块立即 conventional commit；串行 cargo；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / clippy -D warnings / fmt 72 包全绿
2. `cargo test -p sz-orm-core --test integration_oracle_matrix -- --ignored`：矩阵真连全过（OR1~OR6 逐块留输出）
3. `cargo test -p sz-orm-core --test integration_oracle -- --ignored`：既有 15 项不回归
4. 三分类清单：每条附证据（方言生成 SQL + 真库 ORA 输出 + `dialect.rs` file:line）
5. 门禁：G9（--strict）/G12/G15/G16/G33/G35/G36 全过
6. 报告 `docs/assessment/2026-11-11-oracle-dialect-matrix.md`，完成后跑 `bash scripts/audit-verify.sh`（0 失败验收线）；六环闭环登记（CHANGELOG/AGENTS/commit）

## 环境坑位速查

- git 在 `D:\Git`（不在 PATH）；`cargo fmt --all` 用 `bash scripts/check-fmt-all.sh`
- **OCI PATH 必设**（未设则链接/加载失败）；sqlplus 命令行直传中文/特殊字符有编码坑——用 `.sql` 脚本文件 + `cmd //c`
- 26ai Free 的 PDB：连接串必须带服务名 `freepdb1`（CDB root 无业务表）
- 凭据不入库：`SZ_ORM_ORACLE_PASSWORD` 运行时注入；缺失时测试应 assert 拒跑而非 panic 泄漏
- DDL 隐式提交：测试内建表/清理与事务断言的交错要小心——DDL 会提前 COMMIT 整个事务
- 临时脚本放 `D:\tmp`（勿入仓库）；报告引用凭据处一律写环境变量名
