# 新会话提示词：sz-orm MySQL 方言矩阵真连验证（v1，2026-11-11）

> **用法**：将下方「---」之间的内容整体复制到新会话，作为首个提示词发送。
> **背景**：sz-orm v10.13.4。SzRSQL 已完成 ×PG18 矩阵差分（9 项上游缺陷全修复）+ ×SQLite 三方对拍，均六环闭环。MySQL/Oracle 是**独立方言族**，无「偏离 PG18 即缺陷」的仲裁关系——本批评测对象是 **sz-orm 自己的方言层**：同一功能载荷按 `DbType::MySQL` 方言生成 SQL，在真 MySQL 上执行，验证生成被接受且结果符合 MySQL 自身语义。
> **定位**：查 sz-orm 方言缺口（生成 SQL 真库拒绝/结果错位/元数据面错配），非查数据库缺陷。已有 `integration_mysql.rs` 25 项功能集成测试（G7 战役通过）——矩阵聚焦**方言生成面**，不重复功能覆盖。
> **六环闭环**：本批按评测交付闭环链条执行（提示词→报告→实施批→复评→红队复核→登记）。

---

**角色**：你是 sz-orm 的资深 Rust 数据库适配工程师。任务：以 MySQL 方言能力面为载荷做真连矩阵验证，产出三分类清单（**sz-orm 方言缺口**[修复]/ **MySQL 环境特有**[宝塔配置类，登记]/ **文档措辞**），并修复 sz-orm 侧可修复项。

**目标仓库与环境**：
- 本仓库：`D:\www\test\鲜视达\rust\sz-orm`（v10.13.4）
- 被测：宝塔 MySQL 8.0.29 @ `127.0.0.1:8802`（本机常驻，先探活）
- 凭据：root 密码在宝塔面板安装日志 `D:/BtSoft/panel/logs/install/`（搜索"MySQL root 密码"）；经 `SZ_ORM_MYSQL_URL` 环境变量注入（格式见 `packages/sz-orm-core/tests/integration_mysql.rs` 的 `mysql_url()`），**密码不入库**（审计合规铁律）
- 方言层：`packages/sz-orm-core/src/dialect.rs`（MySqlDialect）；通道：`packages/sz-orm-sqlx`（SqlxMysql 通道）+ `sz-orm-core` 直连通道

## 环境探活（第一步）

```bash
# MySQL 探活（凭据经 env 注入后）
mysql -h 127.0.0.1 -P 8802 -u root -p -e "SELECT VERSION();"
# 库准备：sz_orm_test 库（不存在则创建，utf8mb4）
```

## 评测矩阵（MySQL 方言特有面；每载荷 = 方言层生成 + 真库执行 + 行为断言三步）

| # | 能力块 | 载荷（方言生成面） | 重点 |
|---|--------|-------------------|------|
| MY1 | 排序与 NULL | `ORDER BY` ASC/DESC 含 NULL 列；多键排序 | **MySQL ASC 时 NULL 排最前**（PG 排最后）——方言层是否原样透传并由断言锚定 MySQL 语义；sz-orm 文档是否误导 |
| MY2 | 标识符与保留字 | 反引号 quoting（方言层 `quote()`）；保留字表名列名（`` `order` ``）；Unicode 标识符；63+ 长标识符 | 生成 SQL 被真库接受；保留字漏加反引号即方言缺口 |
| MY3 | 类型与 DDL | AUTO_INCREMENT 主键回填路径（`last_insert_id()`/`LAST_INSERT_ID()`）；TEXT/BLOB/JSON/DECIMAL 类型往返；utf8mb4 中文与 emoji 往返 | 主键回填通道在真库的正确性（sz-pay 生产路径）；JSON 列生成面 |
| MY4 | UPSERT 与批量 | `INSERT .. ON DUPLICATE KEY UPDATE`（方言 build_batch_upsert 生成面）；批量 INSERT 1000+ 行参数上限（`max_allowed_packet` 面） | 生成 SQL 与真库执行结果一致；参数上限探针 |
| MY5 | 分页与窗口 | `LIMIT n OFFSET m`；CTE（`WITH`）；窗口函数（8.0 支持 `OVER`）；`GROUP_CONCAT`（separator 语义） | 8.0 新语法方言层是否透传；GROUP_CONCAT 默认长度截断（环境特有登记） |
| MY6 | 边缘语义 | 三值 NULL（`= NULL` 恒空）；除零（sql_mode 决定报错或 NULL——**宝塔默认 sql_mode 探针**）；隐式类型转换（`'5'+1`）；空串 vs NULL（MySQL 有别于 Oracle）；`0='false'` 弱类型 | sql_mode 环境差异登记为环境特有；方言层不假设 sql_mode |

**不纳入**（登记理由）：主从/备份（运维面）、性能基准（sz-orm-bench 已覆盖）、复制格式。

## 硬性要求

- **语义基准**：MySQL 8.0 官方语义 + 真库实测行为；遇方言争议以真库执行结果为准，**禁止拿 PG18 当仲裁**
- **sz-orm 方言缺口**：生成 SQL 被真库拒绝/结果错位 → 修复 `dialect.rs` MySqlDialect 并补单测 + 真连断言；**真库自身行为**（如 GROUP_CONCAT 截断、sql_mode）只登记不修
- **参数化铁律**（门禁 9）：所有测试 SQL 参数化；零 unsafe、零 todo!
- **测试产物**：新增 `packages/sz-orm-core/tests/integration_mysql_matrix.rs`（沿用 `integration_mysql.rs` 的 env 注入跳过模式，未配置 `SZ_ORM_MYSQL_URL` 自动跳过）
- **提交纪律**（硬约束 13）：每能力块立即 conventional commit；串行 cargo；`cargo test --workspace` 需 `PYTHONHOME="C:\Program Files\python"`

## 验收清单（逐项留输出）

1. `cargo check --workspace --all-targets` / clippy -D warnings / fmt 72 包全绿
2. `cargo test -p sz-orm-core --test integration_mysql_matrix -- --ignored`：矩阵真连全过（MY1~MY6 逐块留输出）
3. `cargo test -p sz-orm-core --test integration_mysql -- --ignored`：既有 25 项不回归
4. 三分类清单：每条附证据（方言生成 SQL + 真库输出 + `dialect.rs` file:line）
5. 门禁：G9（--strict）/G12/G15/G16/G33/G35/G36 全过
6. 报告 `docs/assessment/2026-11-11-mysql-dialect-matrix.md`，完成后跑 `bash scripts/audit-verify.sh`（0 失败验收线）；六环闭环登记（CHANGELOG/AGENTS/commit）

## 环境坑位速查

- git 在 `D:\Git`（不在 PATH）；`cargo fmt --all` 用 `bash scripts/check-fmt-all.sh`
- MySQL 端口 **8802**（非 3306——3306 被 Docker 代理占用）；宝塔服务名 `mysqld` 可能需 `net start` 探活
- 中文载荷经 CLI 传参会有 GBK 假编码——统一走测试文件内字符串字面量（Rust 源码 UTF-8）
- 凭据不入库：`SZ_ORM_MYSQL_URL` 运行时注入；报告引用凭据处一律写环境变量名
- 临时脚本放 `D:\tmp`（勿入仓库）；对拍/探针脚本用 `python -I`
