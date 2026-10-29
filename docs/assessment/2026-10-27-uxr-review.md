# sz-orm 用户体验（UX/DX）定性研究报告

- **研究角色**：顶级用户研究员（UXR），15 年定性研究经验，专注开发者体验（DX）
- **研究对象**：sz-orm v10.12.0（纯 Rust 异步 ORM workspace，72 成员）
- **研究日期**：2026-10-27
- **研究性质**：定性用户体验评测（启发式评估 / 认知走查 / 人物画像 / 用户旅程 / 模拟访谈 / 可用性测试），非代码审计
- **配套报告**：`2026-10-25-product-evaluation.md`（产品经理视角）、`2026-10-27-security-review-prompt.md`（AppSec 视角）
- **证据纪律**：L1=`[事实]`（文档/代码 file:line、命令输出，可审计）；L2=`[实测]`（研究者本会话亲自执行的真实输出与耗时）；L3=`[模拟]`（模拟合成数据，用于说明研究方法与可能痛点，**不代表真实用户意见**）

---

## 1. 执行摘要

### 五个最痛的用户洞察

**洞察一：三层核心文档的示例代码都编译不过——对开发者用户这是信任级伤害。**
README Quick Start §3 的链式查询代码（[README.md:488-496](../../README.md#L488)）调用 `.select(...)` 后直接链 `.where_eq(...)`，而 `select()` 实际返回 `Result<Self, DbError>`（[事实] [packages/sz-orm-core/src/query.rs:773](../../packages/sz-orm-core/src/query.rs#L773)）。研究者将 README 代码逐字复制到独立项目中实测：`error[E0599]: no method named 'where_eq' found for enum 'Result<T, E>'`（[实测] §7 任务 T-README）。使用指南 §2.3 的"最小示例"同病（`fn main()` 无 Result 返回却在函数体内用 `?`，[事实] 使用指南 zhdoc 副本 L97-108）。更严重的是三份迁移指南：`User::query(&pool)`、`find_all()`、`pool.begin()`、`Migrator::run(&pool)`、`#[derive(ActiveModel)]`、`User::from_rows()` 等调用链在当前代码库中**不存在**（[事实] migration-guide 抽查，见 §5-H4）——迁移者照抄必在第一步编译失败，而迁移者恰恰是最需要文档准确的用户。

**洞察二：五份核心中文文档的文件名本身是乱码——GitHub 与一切 UTF-8 环境下都是死链。**
README 与 quickstart 链接的 `docs/sz-orm使用指南.md`、`docs/sz-ormAPI参考.md`、`docs/sz-orm学习路线图.md`、`docs/sz-orm架构设计.md`、`docs/sz-orm与同类产品对比分析.md`，在磁盘上的真实文件名是 UTF-8→GBK 乱码（如 `sz-ormAPI鍙傝€_md`，其中 3 份连 `.md` 扩展名都变成了 `_md`；[事实] 文件名 Unicode 码点直接取证，见 §5-H1）。这意味着：中文 README 顶部最显眼的两个导航链接（使用指南、API 参考）在 GitHub 上 404；中文文档资产整体处于"存在但不可达"状态。中文用户是本项目宣称的重要目标人群（ThinkORM 迁移者、国产库团队），却拿到的是体验最差的入口。

**洞察三：版本号"动物园"贯穿所有用户触点，用户无法建立"我现在该用什么"的确定感。**
同一仓库里同时存在：workspace 10.12.0（[事实] Cargo.toml:6）、crates.io 实际最高 10.7.0（2026-10-16 发布，[事实] crates.io API 查询）、使用指南 v7.7.0、学习路线图 v7.7.0+、custom-diagnostic-guide 教用户装 "3.7.0"、三份迁移指南 v3.5.0、选型指南头部 v3.4.0（§6 又自称 v10.9.0）、README.zh 概览段落 v4.3.0/56 成员、README 英文版 Rust 1.81+ vs 中文版 1.94+、CLI `info` 自报"31 个包/11 种方言"（[实测]）vs README "72 包/28 方言"。README 备选安装行注释 `sz-orm-core = "10.12"`（[事实] README.md:453）——crates.io 上**不存在 10.12**，用户取消注释即失败。

**洞察四：Quick Start 的"5 分钟"承诺约七成真实——代码是真的，但细节坑让首试成功率打折。**
[实测] 路径 B 的步骤 4 代码在独立项目中冷编译 1m04s 即通过，SQLite 同构路径端到端跑出 `rows = 1`；路径 A 示例在本机编译 47.21s 并完整跑通 CRUD+事务（§7 任务 T1/T2）。这是历史评测 M5 整改的真实成果。但四处细节坑仍在：①文档说 MySQL 在 3306，docker-compose 实际映射 8802（[事实] quickstart.md:30 vs docker-compose.yml:42），照做连接拒绝；②quickstart 全文无 `cargo new`，path 依赖写法只在"把项目建在仓库根下"时成立（[事实] quickstart.md:39-40）；③"无 Docker"分支不交代建库步骤（[事实] quickstart.md:33）；④"路径 A 60 秒"对普通开发机不成立（本例高配机 47-65s 且依赖缓存部分复用，普通 4-8 核笔记本冷编译预计 5-15 分钟）。

**洞察五：工具链是"半成品+自我陈述失真"的组合，制造沉默的信任流失。**
CLI 核心命令真实可用（86 个测试，[事实] cli/tests），但默认构建下 help 里宣传的约 20 个命令（designer/lsp/ai/backup/n1-lint…）全是报错桩（[事实] cli/Cargo.toml:53 `default=[]` + main.rs:2240 等）；`sz-orm info` 自报"31 个包/11 种方言/L4 金融级"（[实测]），与 README 的 72/28 直接冲突；`make:model` 生成 TODO 空骨架，用户每加一个业务字段要手改 5 处（struct/columns/fillable/get_column_value/from_value，[事实] cli/src/main.rs:822-884），且 `--pk-type i32/u32/u64` 的产物引用不存在的 `Value::as_i32/as_u32/as_u64`、无法通过编译（[事实] main.rs:781-812 vs value.rs:462-539）。`sz-orm-designer` 后端真实（索引建议器/迁移生成器/169 测试）但 Web 前端是 68 行只读展示页；`sz-orm-studio` 无数据库连接、无前端，属演示级（[事实] server.rs:58）。

### 三个最值得做的改进（ROI 排序见 §11）

1. **建立"文档代码可编译"门禁**：把 README/quickstart/迁移指南的所有代码块抽进编译测试（no_run 集成测试或 doctest）。当前三层文档的示例代码失效没有任何门禁能拦截——这是全项目质量体系最讽刺的盲区：G12/G18 校验数字，却不校验代码。
2. **中文资产止血**：修复 5 份乱码文件名；以英文 README 为源同步中文版数字段落并纳入 G18；术语表统计自动生成。
3. **Model 定义降门槛**：提供 `#[derive(Model)]` 宏（designer 的 code_gen 已经在引用一个不存在的宏——说明这个需求早已被产品自己承认），让 make:model 生成"开箱可编译"的完整模型。

---

## 2. 研究方法与数据可信度说明

| 方法 | 执行情况 | 数据层级 |
|------|---------|---------|
| 文档事实核查（双语 README/quickstart/docker-compose/使用指南/术语表/诊断指南/3+1 份迁移指南/历史评测 2 份） | 完成（研究者 + 3 个核查代理独立取证交叉验证） | L1 |
| 启发式评估（Nielsen 10 + DX 专项 5 项） | 完成，每条附 L1 证据 | L1 |
| 认知走查（quickstart 路径 B，新手画像视角） | 完成（§6） | L1+L2 |
| Dogfooding 实测 | 路径 A（两次）、路径 B（独立项目冷编译+运行）、CLI 7 组命令、README §3 编译验证、crates.io 查询、本机端口/凭据核查 | L2 |
| 人物画像建模 | 完成（§3） | L1+L3 |
| 用户旅程地图 + 情绪曲线 | 完成（§4），情绪值基于 L2 实测挫折点 + L3 合成 | L2+L3 |
| 可用性测试（5 任务） | L2 部分由研究者亲自执行；L3 部分为模拟被测者，两者分列（§7） | L2+L3 |
| 模拟用户访谈（4 画像） | **模拟合成**，提纲见附录 A（§8） | L3 |
| 模拟焦点小组（5 人） | **模拟合成**（§9） | L3 |
| 痛点优先级矩阵 | 完成（§10），严重度/频率由 L1/L2 证据推导 | L1+L2 |

**L2 实测环境**：Windows 10 x64（高核数工作站）、cargo 1.99.1、rustc 1.99.1、仓库冷启动（无 target/ 缓存）、全局 CARGO_HOME 走 rsproxy 镜像。注意两点环境因素：①评测期间存在并行会话操作同一仓库（00:34 首测时 `sz-orm-storage` 缺失被 git 跟踪的 `tests/owasp_file_upload.rs`，00:43 由并行会话恢复——详见 §7-T1 的完整时间线，此事件不判为产品缺陷但暴露了 workspace 脆弱性）；②本机 3306 端口 MySQL 的 root 凭据与 AGENTS.md 记载不符、5432 PostgreSQL 未监听，故路径 B 的 MySQL 运行时验证止步于"服务端返回 Access denied"（协议握手已通），SQLite 同构路径完成了端到端验证。实测临时项目与产物在 `D:\www\test\uxr-tmp\`（仓库外，未触碰仓库任何文件）。

---

## 3. 人物画像（Persona）

> 构建依据：README 定位声明（[事实] README.md:16/94）、历史评测目标用户矩阵（[事实] 2026-10-25-product-evaluation.md §二）、学习路线图的明确读者画像（[事实] zhdoc-4 副本 L3）。Persona 本身是研究合成物（L3），但其痛点假设全部绑定 L1/L2 证据。

### P1 · 老王 —— Rust 资深后端工程师
- **背景**：8 年后端，Rust 4 年，用惯 SQLx 直写 SQL，新项目想找个带池/事务/迁移的ORM省 boilerplate。
- **目标**：30 分钟内评估 sz-orm 是否比"SQLx + 手写池"更划算；重点看连接池实现与事务 API。
- **文档习惯**：只读 README 快速开始 + 直接翻源码；对营销话术免疫，对"数字矛盾"高度敏感——数字矛盾会被解读为"作者对自己项目都失控"。
- **痛点假设（有证据）**：README 内部 17 vs 28 方言、9 vs 38 示例自相矛盾（[事实] §5-H2/H3）；`sql_string!` 等编译期校验有吸引力，但 Core API 文档代码编译不过（[事实] §5-H4）会直接让他转向 SQLx 官方文档。
- **情绪基线**：怀疑 → 需要可运行证据才升级为信任。

### P2 · 李工 —— PHP/Laravel（ThinkORM）迁移者
- **背景**：10 年 PHP/ThinkPHP，团队要转 Rust 技术栈；熟悉 `Db::name('user')->where(...)->select()` 链式风格。
- **目标**：找到心智模型最接近 ThinkORM 的 Rust ORM，两周内带团队迁移一个内部系统。
- **文档习惯**：读中文文档为主；术语表和中文指南是主入口；英文文档基本不看。
- **痛点假设（有证据）**：中文入口全是死链（乱码文件名，[事实] §5-H1）；使用指南版本停在自己不认识的 7.7.0 而仓库是 10.12.0；`sz-orm学习路线图.md` 质量很高（[事实] zhdoc-4：ThinkPHP→Rust 概念对照表）却因为文件名乱码在 GitHub 上不可达；迁移后 API 与 ThinkORM 的差异（手写 `impl Model`+`ModelExt` ≈140 行，[事实] model_definition.rs:23-153）远超预期。
- **情绪基线**：期待 → 中文体验越走越差。

### P3 · 陈工 —— .NET/Java 团队评估者（EF Core/Dapper/MyBatis 背景）
- **背景**：15 年企业开发，评估把一个 Java/MyBatis 中台迁到 Rust；习惯 Entity 一键映射与 ResultMap。
- **目标**：两周出评估报告：学习成本、类型安全、性能证据、迁移工具链。
- **文档习惯**：先看 README 数字表与竞品对比，再跑示例，最后看迁移指南。
- **痛点假设（有证据）**：result_map 模块是卖点（[事实] README.md:715），但三份迁移指南的"迁移后代码"一半调用链不存在（[事实] §5-H4）——他会在评估的第一周就得出"文档跟不上代码"的结论；类型映射表 `Decimal → f64 近似` 与代码 `Value::Decimal(String)` 的防精度丢失设计方向相反（[事实] diesel L204 vs value.rs:51-52），这种"文档反向误导"对企业评估是一票否决级。
- **情绪基线**：中立评审 → 因文档反向误导而否决。

### P4 · 周工 —— 追求工程质量规范的技术负责人
- **背景**：带 10 人团队，看中 sz-orm 的 24 道门禁、覆盖率、变异测试叙事；想引入并复用其工程体系。
- **目标**：确认质量体系真实可复现，评估采纳风险（单作者、版本策略）。
- **文档习惯**：读 AGENTS.md/工程实践文档/审计报告；交叉核对声称与证据。
- **痛点假设（有证据）**：README.md:96 自称 "23 gates" 而 AGENTS.md 是 24 道（[事实]）；覆盖率 88.59%（v8.7 实测）/94.4%（v10.12 评价）/94.7%（AGENTS）多个口径并存；crates.io 10.7.0 vs 仓库 10.12.0 的发布滞后与 README "10.12" 备选行的矛盾（[事实] §5-H6）会让他判定"发布流程不可信"；但 CI 工作流 11 个 yml、CONTRIBUTING/SECURITY 齐全是加分信号（[事实] .github/workflows/ 实存 11 个工作流）。
- **情绪基线**：欣赏体系 → 卡在"声称与证据的最后一公里"。

### P5 · 小林 —— Rust 初学者（在校/转行）
- **背景**：写过半年 Rust，第一次接数据库项目；跟着教程一步步来。
- **目标**：照 quickstart 跑通第一个查询，获得正反馈。
- **文档习惯**：完全照抄文档命令与代码；遇到编译错误只会整段贴进搜索引擎/AI。
- **痛点假设（有证据）**：README §3 代码照抄即编译失败且报错对新手晦涩（[实测] E0599 全文见 §7）；quickstart 没有 `cargo new` 一步，初学者不知道该把 Cargo.toml 放哪（[事实] quickstart.md:35-44）；路径 A"60 秒"在他的笔记本上大概率是 5-15 分钟，等待中会怀疑自己操作错了。
- **情绪基线**：期待 → 第一次编译失败即放弃概率高。

---

## 4. 用户旅程地图与情绪曲线

> 情绪分值 -2（挫败/放弃）~ +2（惊喜/愿推荐）。标 ⭐ 的阶段有 L2 实测支撑；未标注的由 L1 证据 + L3 合成推导。

| 阶段 | 用户行为 | 接触点 | 情绪 | 关键证据与挫折点 |
|------|---------|--------|------|----------------|
| 1. 发现 | 搜索/朋友推荐 → 打开 GitHub | README 首屏 | **+1** | 首屏数字漂亮（72 包/21134 测试/28 方言，[事实] README.md:4）；"诚实声明"和"生产就绪定位"罕见地坦白单作者与内部试点限制（README.md:94/96），加信任分 |
| 2. 读文档 | 从 README 找 quickstart 与中文指南 | 文档链接 | **-1 ⭐** | 中文导航链接（使用指南/API 参考）是死链——文件名乱码（[事实] §5-H1）；README 内部数字已经开始打架（17 vs 28 方言、9 vs 38 示例） |
| 3. 首次运行（路径 A） | `cargo run -p sz-orm-examples --bin real_db_crud` | 终端 | **0 ⭐** | 成功时输出友好、全流程叙事清晰（[实测] 47.21s 编译 + 完整 CRUD 打印）；但"60 秒"预期被现实打破（普通机器数倍于此），且首试曾因 workspace 瞬时损坏直接失败（[实测] §7-T1 时间线） |
| 4. 路径 B 连真库 | docker compose → 建项目 → 步骤 4 代码 | 终端 | **-1 ⭐** | 代码可编译（1m04s），但 3306 vs 8802 端口矛盾必坑 Docker 用户；无 cargo new 指引；"无 Docker"分支没建库步骤；本机还撞上 AGENTS.md 凭据失效（[实测]） |
| 5. 写第一个 Model | 手写 impl Model / 或 CLI make:model | 编辑器+CLI | **-2** | 无 derive 宏；手写全量 ≈140 行样板（[事实] model_definition.rs）；make:model 生成 TODO 空壳、每字段 5 处手改、i32/u32/u64 产物编译不过（[事实]/[实测] §7-T3）——旅程最低谷 |
| 6. 查询与执行 | 链式 QueryBuilder + conn.query | 编辑器 | **-1** | API 设计本身顺手（ThinkORM 风格），但 README/使用指南 §2.3 的示例代码编译不过（[实测] E0599），新手在此大量流失；`build_select()` 返回 `(String, Vec<Value>)` 二元组这种非典型设计文档未做心智解释 |
| 7. 集成测试/门禁 | cargo test / 参考工程实践 | CI | **+1** | 门禁体系文档齐全、故障排除表实用（PL001/DB003 等错误码+动作，[事实] 使用指南 §七）；对质量敏感用户是正反馈 |
| 8. 迁移现有项目 | 读迁移指南逐步搬代码 | 迁移指南 | **-2** | "迁移后代码"大面积引用不存在的 API（[事实] §5-H4）；类型映射表反向误导（Decimal）；错误处理映射零覆盖——第二最低谷 |
| 9. 上生产 | 评估 crates.io/发版/升级策略 | crates.io + CHANGELOG | **-1** | crates.io 10.7.0 vs 仓库 10.12.0，README 备选行指向不存在的 "10.12"（[事实]）；单作者风险诚实披露但无缓解方案 |

**情绪曲线示意**：

```
+2 |
+1 | ●(发现)                                ●(门禁/测试)
 0 |        ●(路径A成功边缘)        ●(读文档-1)
-1 |             ●(读文档)  ●(路径B)        ●(上生产)
-2 |                          ●(写Model)  ●(迁移现有项目)
   +---------------------------------------------------
     发现  读文档 路径A 路径B 写Model 查询 门禁 迁移 生产
```

**两个设计性结论**：①旅程的两个最低谷（写 Model、迁移）都发生在"用户已经付出沉没成本之后"——这是流失率最高的位置，修复杠杆最大；②全程唯一的 +2 潜力点（"第一次 SELECT 成功"）被路径 B 的端口矛盾和凭据噪音吃掉了，而它本是零成本修复项。

---

## 5. 启发式评估（Nielsen 10 + DX 专项）

> 每条附 L1 证据。评级：✅ 良好 / ⚠️ 有缺陷 / ❌ 严重。

### Nielsen 10

**H1 可视性与系统状态匹配（Visibility of system status）⚠️**
- [事实] `sz-orm info` 输出 "工作空间： 31 个包 / 数据库方言： 11 种 / 生产等级： L4 金融级"（[实测] 原样输出），与 README.md:44/123 "72 members / 28 dialects" 冲突——工具自我陈述与文档互斥，用户无法判断哪个是现状。dialect list 同样只列 11 种（[实测]）。
- [事实] CLI `--version` 输出 10.12.0 与 workspace 一致（[实测]），这一点是自洽的。
- [事实] sql:validate 对 `DROP TABLE users` 输出 "✓ SQL 校验通过"（[实测]）——校验器只认 SELECT/INSERT/UPDATE/DELETE 四类（[事实] sz-orm-sql-validator/src/lib.rs:157），未知语句静默放行，"✓"传递了虚假的安全状态。

**H2 系统与现实世界匹配（Match between system and real world）❌**
- [事实] 五份核心中文文档磁盘文件名为 UTF-8→GBK 乱码：`sz-ormAPI鍙傝€_md`（API 参考）、`sz-orm浣跨敤鎸囧崡.md`（使用指南）、`sz-orm涓庡悓绫讳骇鍝佸规瘮鍒嗘瀇md`（对比分析）、`sz-orm瀛︿範璺绾垮沖md`（学习路线图）、`sz-orm鏋舵瀯璁捐.md`（架构设计）；取证方式为文件名 Unicode 码点直读（如 `sz-ormAPI` 后码点 37721,20637,8364 = "鍙傝€"，而正常应为 "参考" = 21442,32771）。README.md:16、README.zh.md:16、quickstart.md:98-99、README.md:1045-1052 的对应链接在 GitHub/任何 UTF-8 检出上全部失效。另有 `docs/评估` 目录名同样乱码。
- [事实] 术语表译法互串：击穿→cache penetration、雪崩→cache stampede（[事实] glossary-zh-en.md:57-58；业内惯例多相反——penetration 对应"穿透"）；且表尾自称 75 条而实测 89 条（[事实] glossary:128 vs 全表清点）。

**H3 用户控制与自由（User control and freedom）✅**
- [事实] 迁移系统提供 up/down/rollback/reset/refresh 全套回退路径（[事实] README.md:650-655）；CLI migrate:status/rollback/fresh 默认可用（[事实] cli/src/main.rs:407-577）；连接池提供 release/status/reap_idle/close_all 手动控制（README.md:608-611）。

**H4 一致性与标准（Consistency and standards）❌**
- [事实] README.md:355 "17 SQL dialects" vs README.md:4/123 "28"（源码真相：DbType 28 变体，[事实] db_type.rs 全枚举清点）；README.md:428 目录树注释 "9 runnable examples" vs :850 "38"（实测 bin 目录 38 个文件）；README.md:430 "11 ADRs" vs :1051 "9 条 ADR"；README.md:96 "23 gates" vs AGENTS.md 24 道门禁表；README.md:1048 "39 包架构概览"（72 成员的今天）；README.md:125 ~639,200 LOC vs README.zh.md:103 ~139,000 LOC；README.md:128 Rust 1.81.0+ vs README.zh.md:106 1.94.0+（Cargo.toml:8 实为 1.81）。
- [事实] 同一文档内两个版本号：query_builder_selection_guide.md 头部 v3.4.0、§6 标题自称 v10.9.0 深化（L3 vs L147）。
- [事实] sz-pay 依赖包数三种口径：AGENTS.md 6 个、README.md:94 7 个（含 scheduler）、学习路线图 6 核心+16 optional。

**H5 错误预防（Error prevention）⚠️**
- [事实] `sql_string!` 编译期 SQL 校验 + 注入模式拦截（[事实] README.md:523-530，宏真实存在 sz-orm-macros/src/lib.rs:182）是全项目最好的错误预防设计。
- ⚠️ 但 `where_cond`/`or_where` 的废弃-移除旅程文档滞后：迁移指南 sqlx L214 仍称"已 deprecated"，实际 v5.0.0 已移除（[事实] query.rs:23）——按文档写代码会直接失败。
- ⚠️ make:model 允许用户传入 `--pk-type i32/u32/u64` 却生成引用不存在方法的代码（[事实] main.rs:781-812 vs value.rs:462-539）——工具没有预防自己生成坏代码。

**H6 识别而非回忆（Recognition rather than recall）✅**
- [事实] 链式 API 命名高度 ThinkORM 惯例化（where_eq/or_where_eq/where_in/page/join_inner，[事实] README.md:558-576），对目标用户群迁移成本低。
- [事实] 方言表带默认端口/真实连接标注（README.md:534-549），查找友好。

**H7 灵活与效率（Flexibility and efficiency of use）✅**
- [事实] 21 个高级模块 + 508 feature gate 的模块化组合（README.md:44/692-718）、`quick_query` 支持 Db::name() 无 Model 快查（README.md:713）——专家路径存在。

**H8 美学与极简（Aesthetic and minimalist design）⚠️**
- [事实] 英文 README 1057 行，含大量历史版本日志（v1.3/v1.4/v1.5/v2.0/v3.3/v3.4 段落仍占正文，README.md:134-351/962-1000）——新用户需要滚过 300+ 行过时内容才能读完核心章节。中文 README 同样保留 v1.3-v4.1 全部历史段落。
- [事实] 使用指南 3126 行、章节编号体系断裂（"一~九"后插入"三.5/三.6"再跳"10/11/12/13"，且"三.6"内小节编号与正文 §3.6.1 冲突；[事实] 代理核查 + 研究者抽验）。

**H9 帮助用户识别、诊断并从错误中恢复（Help users recognize and recover from errors）✅（文档侧）/ ⚠️（工具侧）**
- ✅ [事实] 使用指南 §七故障排除表是全文档质量最高的一节：13 行"症状/原因/处理"，错误码具体到 PL001/PL002/PL005/DB003/DB014 并给出可执行动作（[事实] zhdoc-1 副本 L2198-2214）；custom-diagnostic-guide 的 5 类编译期诊断在源码中真实存在（[事实] sz-orm-macros/src/diagnostic.rs:45-57）。
- ⚠️ [事实] 诊断文档承诺"包含错误位置"（D:7）但全文没有一份真实 rustc 输出示例；且教用户从 crates.io 装 `sz-orm-macros = "3.7.0"`（D:13）——版本不存在（crates.io 最高 10.7.0）。
- ⚠️ [事实] README §3 代码编译失败的 rustc 报错（E0599）虽给出了 expect 修复建议，但用户先得自己发现"文档错了"这个前提——错误恢复成本被转嫁给用户。

**H10 帮助与文档（Help and documentation）⚠️**
- [事实] 文档体量惊人（使用指南 3126 行/139,817 字节、38 个示例、术语表、学习路线图、3 份迁移指南），覆盖广度是真实优势。
- ❌ 但如 H2/H4 所示：中文入口不可达、代码不可编译、版本失真——"量大"没有转化成"可用"。

### DX 专项五项

**DX-1 可发现性（Discoverability）⚠️**
- [事实] quickstart.md 存在且被 README 首屏引用（README.md:441-442），但中文 README 快速开始章节的链接同样指向乱码文件名的文档；docs/README.md 未在各 README 顶部出现，38 个示例只有表格无"哪个适合我"的场景导引（[事实] README.md:850-891 平铺列表）。

**DX-2 可复现性（Reproducibility）⚠️（代码真）/ ❌（文档数字假）**
- ✅ [实测] 路径 A 示例完整跑通（含事务提交/回滚断言）；路径 B 步骤 4 代码独立项目冷编译 1m04s 通过；SQLite 同构 `rows = 1`。
- ❌ [实测] README §3 代码 E0599；[事实] 迁移指南调用链约半数不存在；[事实] docker-compose MySQL 端口 8802 vs 文档 3306。

**DX-3 错误信息质量（Error message quality）✅**
- [实测] 连接 MySQL 凭据错误时输出 `Error: QueryError("Access denied for user 'root'@'localhost' (using password: YES)")`——底层错误透传清晰，可定位。
- [实测] cargo 对 README 代码的 E0599 附带"consider using Result::expect"修复建议——框架错误面良好，问题在文档不在编译器。

**DX-4 文档可信度（Documentation trustworthiness）❌**
- 汇总 H2/H4/H10：数字矛盾 8+ 处、版本号 6 套口径、代码三层失效、中文入口死链。对"文档说谎零容忍"的开发者人群（研究前提），当前状态会触发最快路径的不信任。

**DX-5 工具链真实可用性（Toolchain readiness）⚠️**
- CLI 核心可用（[事实] 86 测试；[实测] 7 组命令全部有合理输出），但 help 宣传 30+ 命令而默认构建约 20 个是报错桩（[事实] cli/Cargo.toml:53）；designer 前端只读（[事实] web_ui.rs:121-188，页面自述"Use the API endpoints to add tables"）；studio 演示级（[事实] server.rs:58 无数据源）。另：designer 的 code_gen 产出 `#[derive(sz_orm_core::Model)]` 引用不存在的宏（[事实] code_gen.rs:49 vs sz-orm-macros/src/lib.rs:2668-3056 全 derive 清单）——"产品自己都不用自己的代码生成产物"。

---

## 6. 认知走查：Quick Start 路径 B 逐步走查

> 走查视角：P5 小林（Rust 初学者）。走查方法：每步回答四问——用户此刻的目标？文档提供的行动？用户能否识别正确操作？系统反馈是否清晰？环境：本机 Docker 不可用（走"无 Docker"分支）、凭据按本机实际调整（文档允许，quickstart.md:33）。

| # | 步骤 | 文档提供 | 小林能否识别正确操作 | 反馈质量 | 认知断点 |
|---|------|---------|---------------------|---------|---------|
| 1 | 启动数据库 | "已安装 Docker 时：`docker compose up -d mysql postgres`"；无 Docker 则"使用本机已有的实例，连接串按实际修改"（quickstart.md:24-33） | ⚠️ 半能 | 端口/凭据表清楚 | **断点 B1**：无 Docker 分支没有任何"如何建库/建用户"指引（本机 MySQL 连接成功但 `sz_orm_test` 库是否存在无从得知）；**断点 B2**：Docker 用户必踩 3306 vs 8802 端口矛盾（[事实] docker-compose.yml:42），连接拒绝时无任何提示指向原因 |
| 2 | 添加依赖 | 一个 Cargo.toml 片段（path 依赖，quickstart.md:37-42） | ❌ **不能** | 无 | **断点 B3（最重）**：文档从未出现 `cargo new`；path 写法 `path = "packages/sz-orm-core"` 只在"新项目建在仓库根目录内"时成立。小林要么把 Cargo.toml 建错地方，要么在仓库根直接覆盖性困惑。[实测] 研究者在仓库外建项目时被迫把 path 改成绝对路径——这一步文档完全没有覆盖 |
| 3 | 生成 Model | `cargo install --path cli` + `sz-orm make:model User --pk-type i64`（quickstart.md:48-60） | ⚠️ 半能 | CLI 输出"已生成 Model: ./src/models\user.rs"清晰（[实测]） | **断点 B4**：在仓库根执行 make:model 会把文件生成进仓库 `./src/models`（仓库根没有 src/，用户更困惑）；安装 CLI 需编译 ~1-3 分钟，文档无预期管理 |
| 4 | 连接并查询 | 完整 main.rs（rust,no_run 标注）+ 连接串（quickstart.md:64-84） | ✅ 能 | 编译错误时 rustc 报错可读；[实测] 该代码独立项目冷编译 1m04s 通过、SQLite 变体 `rows = 1` | **断点 B5**：代码块的依赖（`MySqlPoolHandle` 在 sz-orm-sqlx）与步骤 2 一致 ✓；但步骤 2 依赖的路径问题（B3）会让这一步根本走不到 |
| 5 | 运行完整 CRUD 示例 | `cargo run -p sz-orm-examples --bin real_db_crud`（quickstart.md:90） | ⚠️ 半能 | 示例输出叙事完整（[实测]） | **断点 B6**：此命令必须在克隆的仓库内执行——但步骤 2-4 已把用户带到"自己的项目"语境里，文档没有说明两个工作目录的切换；[实测] 首次执行该命令时曾因 workspace 瞬时损坏失败（§7-T1），对毫不知情的新手 = "这个库是坏的" |

**走查结论**：路径 B 的核心代码是可信的（这是相对 2026-10-25 评测的实质进步），但"环境与项目脚手架"的四个断点（B1/B3/B4/B6）分布在用户必经之路上，且全部属于**零代码成本的文档修复**。按认知走查的保守估计：初学者一次通过率 < 50%，平均卡点 2-3 处。

---

## 7. 可用性测试结果

### 7.1 任务定义与完成标准

| 任务 | 描述 | 完成标准 | 观察指标 |
|------|------|---------|---------|
| T1 | 零依赖路径跑通第一个查询（quickstart 路径 A） | 终端出现"端到端示例完成" | 完成率、耗时、错误次数 |
| T2 | 独立项目连数据库出查询（quickstart 路径 B 步骤 2+4） | 打印 `rows = 1` | 完成率、耗时、需要文档外知识的次数 |
| T3 | 用 CLI 生成 Model 并加入 1 个业务字段（name:String） | 生成文件通过 `cargo check` | 手改点数量、能否编译 |
| T4 | 按 SeaORM 迁移指南迁移一个"条件查询+分页" | 迁移代码通过 `cargo check` | 引用不存在 API 的次数 |
| T5 | 诊断一次连接失败（错误端口场景） | 用户能从错误信息定位到端口问题 | 定位耗时、是否需要读源码 |
| T-README | README §3 代码逐字复制编译 | 通过 `cargo check` | 是否通过 |

### 7.2 L2 实测结果（研究者亲自执行）

| 任务 | 结果 | 关键数据 |
|------|------|---------|
| T1 | ✅ 完成（第二次尝试） | **首次尝试 00:34 失败**：`error: packages\sz-orm-storage\Cargo.toml: can't find owasp_file_upload test ...`（workspace 所有 cargo 命令在 manifest 解析阶段即失败）；00:43 并行会话恢复缺失文件（文件创建时间取证 00:43:23，且 `.git/index` 中该路径被跟踪）；01:00 重跑成功：编译 47.21s（target 含此前 CLI 构建的复用），CRUD+事务全流程输出正确、断言通过。**研究判定：瞬时环境竞态，不判产品缺陷；但"一个成员缺失一个被声明的测试文件 = 全 workspace 所有 cargo 命令瘫痪"的脆弱性值得记录** |
| T2 | ✅ 完成（含调整） | 独立项目冷编译 sz-orm-core+sz-orm-sqlx+tokio：**1m04s**（cargo 自报，高核机）；MySQL 路径：连接池建立→服务端返回 Access denied（[实测] 环境凭据失效，但证明池→驱动→TCP→握手链路正常）；SQLite 同构代码 **`rows = 1`**（00:55:20）。需要文档外知识 2 次：①path 依赖改绝对路径（B3）；②换 SQLite/调整凭据属环境问题 |
| T3 | ✅ 完成（i64 路径）/ ❌ i32/u32/u64 路径必然失败 | `sz-orm make:model User --pk-type i64` 生成 78 行文件（[实测] 全文含 TODO 占位）；加 1 个业务字段需手改 **5 处**（struct/columns/fillable/get_column_value/from_value）；[事实] `--pk-type i32/u32/u64` 产物调用不存在的 `Value::as_i32/as_u32/as_u64`（main.rs:781-812 vs value.rs:462-539），cargo check 必失败 |
| T4 | ❌ 未完成（按文档） | 迁移指南 seaorm L77-81 的 `User::query(&pool).where_eq(...).find_all()`：`Model::query` 入口不存在（[事实] model.rs:37-110 无 query 方法）、`find_all` 仅存在于同步 Repository trait（repository.rs:352-355）——**照文档无法迁移**；研究者需自行从 tests/type_safe_columns.rs:171-177 反推真实链路 `QueryBuilder::new(dialect)...build_select() -> (String,Vec<Value>)` + `query_with_params` |
| T5 | ⚠️ 部分完成 | 构造连接 3306（文档说）→ Docker 实际 8802：错误输出仅 "Connection refused" 类信息，文档/错误信息均无"检查 compose 端口映射"的提示；研究者靠读 docker-compose.yml 定位（新手预计卡死或上网搜） |
| T-README | ❌ 失败 | rustc 原样报错：`error[E0599]: no method named 'where_eq' found for enum 'Result<T, E>' in the current scope`，指向 query.rs:814 并建议 `.expect(...)`；同一错误在使用指南 §2.3 以 `?` 形式复现（fn main 非 Result） |

### 7.3 L3 模拟被测结果（**模拟合成数据，不代表真实用户**）

> 以下为模拟 P2（李工，PHP 背景）与 P5（小林，初学者）执行同样任务的合成记录，用于呈现"不同画像在相同任务上的预期分化"。**声明：本节全部为研究者合成的模拟数据，无任何真实被测者。**

| 任务 | 模拟 P5 小林（初学者） | 模拟 P2 李工（PHP 迁移者） |
|------|----------------------|--------------------------|
| T1 | 预期：60 秒承诺 vs 实际等待 5-15 分钟，等待中产生"我是不是错了"的怀疑；若遇 manifest 错误则直接放弃（无法自助） | 能成功，但对"克隆 72 包仓库才能跑 SQLite"的性价比不满（“跑个内存库为什么要我下整个 workspace”） |
| T2 | 预期在 B3（cargo new 缺失）卡死：把 Cargo.toml 放错位置或不敢动手 | 能绕过 path 问题（改相对路径 `../sz-orm/packages/...`），但对"教程假设我懂 Cargo workspace"不满 |
| T3 | 预期在"每字段 5 处手改"处出错（漏改 from_value 导致静默丢字段，且无任何告警）——这种错误**编译器不报、运行时丢数据** | 对比 Laravel 迁移/Model 自动化，给出差评；make:model 与 ThinkPHP `php think make:model` 的落差明显 |
| T4 | 按文档写 → 编译失败 → 搜索报错 → 无结果 → 放弃 | 编译失败后可能意识到"文档过时"，但两周迁移窗口内不会投入逆向源码，评估结论转向 SeaORM |
| T5 | 无法定位（compose 端口矛盾超出其知识范围） | 能定位（有 Docker 经验），但会记录为"文档质量差"证据 |

---

## 8. 模拟用户访谈关键发现

> **⚠️ 以下访谈内容全部为模拟合成数据（LLM 生成），用于说明研究方法与假设痛点，不代表任何真实用户意见。** 提纲见附录 A。引用语为合成人物角色的虚构发言。

### 访谈 1 · P1 老王（Rust 资深，模拟）
> 引语（模拟）：“我第一分钟就把 README 关了——17 种方言和 28 种方言出现在同一个页面，我不知道该信哪个。连接池的无锁设计我是真的想看，但你的 Quick Start 代码连编译都不过，我为什么要赌剩下的 70 个包？”

- 关键发现（模拟归纳 + L1 佐证）：资深用户以"内部矛盾"为快速淘汰依据；他们不需要更全的文档，需要**更少的、全部为真的文档**。佐证：README 内部 4 组数字矛盾（§5-H4）。

### 访谈 2 · P2 李工（ThinkORM 迁移者，模拟）
> 引语（模拟）：“中文 README 顶上的'使用指南'点进去是 404，我当时以为是网络问题，刷新了三次。后来在文件列表里看到一个名字全是乱码的 md，才反应过来是文件名坏了。这个项目对中文用户的诚意，在第一秒就掉了一半。”

- 关键发现（模拟）：中文入口死链的杀伤集中在"最想用中文文档的人群"；学习路线图（PHP 视角）质量高但同样不可达（[事实] zhdoc-4 存在于乱码文件名下）。

### 访谈 3 · P3 陈工（企业评估者，模拟）
> 引语（模拟）：“迁移指南告诉我 Decimal 会近似成 f64——这对支付场景是致命的，我直接把 sz-orm 从候选名单划掉了。两周后我才知道代码里 Value::Decimal 是 String 存储防精度丢失的，但评估早就结束了。文档错的代价不是返工，是根本没有下一周。”

- 关键发现（模拟 + L1 佐证）：diesel L204/seaorm L224 的 `Decimal → f64 近似` 与 value.rs:51-52 的设计意图相反——**文档反向误导直接改变商业决策**，是最昂贵的文档 bug 类型。

### 访谈 4 · P4 周工（技术负责人，模拟）
> 引语（模拟）：“24 道门禁我全信，直到我发现门禁保住了 21134 个测试的数字，却没保住 README 上一段 10 行的示例代码能编译。质量体系守住了它盯的地方，没盯的地方塌得更快——因为大家都以为'有门禁'等于'全都有门禁'。”

- 关键发现（模拟 + L1/L2 佐证）：门禁体系产生"安全光环外溢"，用户对未覆盖区域（文档代码块、CLI info 数据、designer 前端）的信任度反而被系统性高估。

---

## 9. 焦点小组纪要（模拟）

> **⚠️ 模拟合成数据。** 主题：「从现有 ORM 迁移到 sz-orm 的障碍」。模拟参与者：P1 老王、P2 李工、P3 陈工、P4 周工、P5 小林。主持人按附录 A 提纲推进。

**议题 1：迁移的第一道坎**
- 共识：**都卡在"把依赖加进项目"这一步**——path 依赖写法只对仓库内成立（[事实] quickstart.md:39-40）、crates.io 版本与仓库脱节（[事实] 10.7.0 vs 10.12.0）、README 备选行 "10.12" 是不存在的版本（[事实] README.md:453）。
- 分歧：老王/陈工认为"path 依赖优先"对生产项目根本不可接受（fork 风险），要求尽快全量发布 crates.io；李工/小林无所谓渠道，只要文档步骤能走通。

**议题 2：Model 定义**
- 共识：手写 `impl Model + ModelExt`（每字段 5 处）是最大代码层障碍；derive 宏是刚需。
- 分歧：陈工指出 designer 的 code_gen 已经在生成 `#[derive(Model)]`（[事实] code_gen.rs:49）——"产品其实已经承认需要这个宏，只是宏不存在"；老王反问"生成器和宏都不对，你们内部到底哪个是真实现？"

**议题 3：文档信任**
- 共识：数字矛盾（17/28、9/38、639K/139K、1.81/1.94）+ 版本号动物园（10.12/10.7/7.7/3.7/3.4）+ 中文死链，三者叠加使"README 顶部的漂亮数字"失去效力。
- 分歧：周工认为门禁体系仍是差异化亮点值得投入；老王认为"门禁 24 道却漏掉示例代码编译"恰恰说明体系是表演性的——**必须先用门禁守住文档代码，门禁叙事才成立**。

**议题 4：留下来的理由**
- 共识：三件事被点名为"留下来的理由"：①路径 A 示例的真实性与输出质量（[实测] 全流程叙事清晰）；②使用指南故障排除表的工程含量（[事实] §七）；③诚实声明文化（单作者/内部试点/死 API 勘误，[事实] README.md:94/96/334）。
- 分歧：无——这是全场唯一全票共识。

---

## 10. 痛点优先级矩阵

> 严重度（1-5，对用户目标/信任的破坏）× 发生频率（1-5，遇到该问题的用户比例×触发次数）。定位依据 L1/L2 证据；频率含"会主动接触该触点的用户占比"估计（标注为研究判断）。

```
频率
 5 |  [P3] 版本号动物园          [P1] 文档示例代码不可编译
 4 |  [P4] Model 每字段5处手改   [P2] 中文文档死链(乱码文件名)
   |  [P5] 迁移指南API大面积失真
 3 |  [P6] 3306/8802端口矛盾     [P7] CLI info/dialect 数据失真
   |  [P8] README历史段落冗长
 2 |  [P9] make:model 坏pk-type  [P10] 无Docker分支缺建库指引
 1 |  [P11] 术语表译法互串
   +--------------------------------------------
      1     2     3     4     5   → 严重度
```

| # | 痛点 | 严重度 | 频率 | 象限 | 关键证据 |
|---|------|-------|------|------|---------|
| P1 | 三层文档示例代码不可编译 | 5 | 5 | **立即修** | [实测] E0599；[事实] 迁移指南 §5-H4；使用指南 §2.3 |
| P2 | 中文文档死链（乱码文件名） | 5 | 4 | **立即修** | [事实] 码点取证 §5-H1 |
| P3 | 版本号/数字动物园 | 4 | 5 | **立即修** | [事实] §5-H4 全清单 |
| P4 | Model 定义门槛（无 derive；每字段 5 处） | 4 | 4 | **规划** | [事实] model_definition.rs / cli main.rs:822-884 |
| P5 | 迁移指南"迁移后代码"系统性失真 | 5 | 3（仅迁移者，但迁移者是核心目标人群→权重高） | **立即修** | [事实] §5-H4；Decimal 反向误导 |
| P6 | quickstart 3306 vs compose 8802 | 3 | 3（Docker 用户 100% 触发） | 规划 | [事实] quickstart.md:30/72 vs docker-compose.yml:42 |
| P7 | CLI 自我陈述失真（31 包/11 方言/L4） | 3 | 3 | 规划 | [实测] info 输出 |
| P8 | README 历史段落冗长（300+ 行过时日志） | 2 | 3 | 排期 | [事实] README.md:134-351 |
| P9 | make:model i32/u32/u64 产物编译失败 | 3 | 2 | 规划 | [事实] main.rs:781-812 |
| P10 | 无 Docker 分支缺建库指引 | 2 | 2 | 排期 | [事实] quickstart.md:33 |
| P11 | 术语表 75/89 失真 + 击穿/雪崩译法互串 | 1 | 2 | 排期 | [事实] glossary:128/57-58 |

---

## 11. 改进建议（按 ROI 排序）

> ROI =（用户价值 × 影响面）÷ 实施成本。每条附证据锚点与可验收标准。

**R1 · 建立"文档代码可编译"门禁（成本：中；价值：极高——一次修复三层文档）**
- 做法：新增脚本把 README.md/README.zh.md/quickstart.md/3 份迁移指南/使用指南 §2 中的 rust 代码块抽入临时 crate（`no_run` 编译校验；依赖 path 指向 workspace），纳入门禁 12（文档一致性）或新增 G25。
- 证据锚点：[实测] README §3 E0599；[事实] 迁移指南不存在 API 清单（§5-H4）。
- 验收标准：`cargo run -p sz-orm-examples` 之外，新增 `python scripts/check-doc-code-compiles.py` 全绿；README §3 与使用指南 §2.3 修复后可独立编译；迁移指南每个"迁移后"代码块通过 cargo check。

**R2 · 中文资产止血包（成本：低；价值：高）**
- 做法：①修复 5 份中文文档 + 评估目录的乱码文件名（改名为规范中文名，git mv 保留历史）；②以 README.md 数字段落为源，同步 README.zh.md 概览/LOC/Rust 版本/方言数段落，并把 README.zh.md 纳入 G18 扫描（M1.2 已建立该机制，扩展模式即可）；③术语表统计行自动生成。
- 证据锚点：[事实] §5-H1/H2/H4。
- 验收标准：GitHub 上从 README.zh.md 出发的所有链接 200；G18 对双语 README 零告警；术语表统计=实际条目数。

**R3 · Quick Start 微创手术（成本：极低；价值：高）**
- 做法：①docker-compose MySQL 端口改回 3306:3306（或 quickstart 全文改 8802，二选一，以 compose 让位为佳——3306 是用户肌肉记忆）；②quickstart 增加"步骤 0：`cargo new my_app`（或在克隆仓库内操作）"的明确指令与两种场景说明；③"无 Docker"分支补 `CREATE DATABASE` 指引；④"60 秒"改为"60 秒起库（编译时间依机器另计 1-10 分钟）"。
- 证据锚点：[事实] quickstart.md:30/33/35-44 vs docker-compose.yml:42；[实测] §6 断点 B1/B3。
- 验收标准：按新文档从零（含 cargo new）到 `rows = 1` 无需任何文档外知识；Docker 用户首连成功率 100%（端口一致）。

**R4 · CLI 自洽化（成本：低；价值：中）**
- 做法：①`info` 的包数/方言数从 workspace 实际数据动态读取或构建期生成（当前 31/11 与 README 72/28 冲突）；②help 输出把默认可用命令与 feature-gated 命令分组显示（`(需 --features xxx)` 后缀）；③make:model 修 i32/u32/u64 访问器（用 `v.as_i64()` + 转换，或在 Value 上补齐 as_i32/as_u32/as_u64——后者同时是公共 API 增强）；④make:model 支持 `--fields name:String,age:i32` 一类字段参数，生成"开箱可编译"的完整四件套。
- 证据锚点：[实测] info 输出；[事实] cli/Cargo.toml:53、main.rs:781-812、value.rs:462-539。
- 验收标准：`sz-orm info` 与 README 数字一致；`make:model --pk-type u32` 产物通过 cargo check；help 无"默认构建必失败"的命令误导。

**R5 · `#[derive(Model)]` 宏（成本：高；价值：极高——直接对标 SeaORM 体验）**
- 做法：在 sz-orm-macros 增加 Model derive（生成 impl Model + ModelExt 全四件套），make:model 与 designer code_gen 改为产出 derive 用法。历史评测路线图第 5 条已建议（[事实] 2026-10-25-product-evaluation.md §八-5）；designer code_gen.rs:49 已在引用不存在的宏——需求自证。
- 验收标准：Quick Start §2 的 Model 定义缩至 `#[derive(Model)] #[orm(table = "users")]` 3 行以内；README §2 代码量减少 ≥80%；派生模型通过编译并与手写版行为等价（差分测试）。

**R6 · 迁移指南重写（成本：中；价值：高——保护核心转化人群）**
- 做法：以 `QueryBuilder::new(dialect)...build_select() -> (String,Vec<Value>)` + `query_with_params` + `conn.begin_transaction()`（pool.rs:180）为唯一真实链路重写三份指南的"迁移后"代码；补错误处理映射节（sqlx::Error/DbErr/diesel::Error → DbError/TxError/QueryError）；修正 Decimal 行（String 保精度）；删除/替换对已移除 API 的引用（where_cond）；每份指南末尾附一个可 clone 可 run 的迁移样例小项目（放 examples/ 或独立目录）。
- 验收标准：R1 门禁下三份指南代码块全绿；"迁移者 3 日内完成一个 CRUD 模块迁移"成为可演示路径。

**R7 · 发布一致性收口（成本：低；价值：中高——信任修复）**
- 做法：①README 备选行改为真实存在的 crates.io 版本（当前 10.7.0）或改为"以 crates.io 实际版本为准"的动态措辞；②把 crates.io max_version 检查纳入 G19（check-publish-consistency）远程校验；③sz-pay 包数口径统一（6/7/10 三种说法择一为权威）。
- 证据锚点：[事实] README.md:453、crates.io API（10.7.0/2026-10-16）、AGENTS.md vs README.md:94。
- 验收标准：README 中不存在 crates.io 上不存在的版本号；G19 输出含 crates.io 实际版本比对记录。

---

## 12. 历史发现复核结果表（2026-10-25 产品评测 → 当前状态）

| # | 历史摩擦点 | 当前证据 | 判定 |
|---|-----------|---------|------|
| 1 | Quick Start 依赖版本号混乱（"1.4" vs 10.12.0） | README.md:446-456 已改为 path 依赖优先 + 注释掉的 crates.io 备选；quickstart.md:44 有滞后声明 ✅。但备选行写 "10.12"——crates.io 实际最高 10.7.0（2026-10-16），取消注释仍会失败 | **部分缓解** |
| 2 | 文档内部数字矛盾（9905 vs 21134、badge 9.4.0） | 旧矛盾已清零（Tests 章节无 9905 总数行；badge 10.12.0，[事实] README.md:8/11）✅。但暴露新矛盾：17 vs 28 方言（README.md:355 vs :123）、9 vs 38 示例（:428 vs :850）、11 vs 9 ADR（:430 vs :1051）、23 vs 24 门禁（:96 vs AGENTS.md）、639K vs 139K LOC（:125 vs README.zh.md:103）、1.81 vs 1.94（:128 vs README.zh.md:106）、39 包架构概览（:1048） | **部分缓解（旧账清零，新账出现）** |
| 3 | README.zh 整体停留 v9.4.0 | 顶部数据行已同步 v10.12.0（README.zh.md:4）✅；但概览段落仍 v4.3.0/56 成员（:44）、Rust 1.94.0+（:106）、~139,000 LOC（:103）、缺 v5.1.0+ 全部新版章节 | **部分缓解** |
| 4 | 示例目录与文档脱节（8 vs 30+） | Examples 表已列 38 个（README.md:850-891），实测 bin 目录 38 个文件 ✅（G24 门禁生效的成果）；遗留：目录树注释仍写 "9 runnable examples"（README.md:428/README.zh.md:406） | **基本解决（遗留 1 处）** |
| 5 | Model 定义门槛高（手写 impl，无 derive） | quickstart.md:46-60 已提供 CLI make:model ✅；[实测] 生成 TODO 空骨架、每字段 5 处手改、i32/u32/u64 产物不可编译；derive(Model) 仍不存在（sz-orm-macros 全 derive 清单无 Model，[事实] lib.rs:2668-3056） | **部分缓解** |
| 6 | 缺"5 分钟跑通真库"教程 | quickstart.md + docker-compose.yml 已落地 ✅（M5 执行）；[实测] 代码真实可编译、路径 A 可跑通；但 3306/8802 端口矛盾、无 cargo new、无建库指引、60 秒声称不实——**文档存在但按文档走不通**（本研究新发现 4 处断点） | **形式落地，实际打折扣** |
| 7 | crates.io 仅 1.0.0，与仓库脱节 | 实测 crates.io sz-orm-core 最高 **10.7.0**（30 个版本，最新 2026-10-16 发布）——从 1.0.0 到 10.7.0 是实质进步（M6 执行）；仓库 10.12.0 仍领先 5 个 minor；README 备选行版本号仍错 | **部分缓解（大幅改善）** |

**复核总评**：2026-10-25 的 M1/M2/M3/M5/M6 都有可验证的落地痕迹（门禁脚本、双语顶部行同步、38 示例表格、quickstart+compose、crates.io 批量发布），历史评测的方向判断正确、整改真实发生了。但整改停在"把声称的数字改对"，没有延伸到"把声称的代码改对"（P1/P5）和"把中文资产修通"（P2）——下一轮整改的主战场应该从"数字一致性"转向"代码一致性与中文可达性"。

---

## 13. 附录

### 附录 A · 模拟访谈提纲（半结构化，45-60 分钟；供真实研究复用）

1. **暖场与背景**（5'）：你现在用什么 ORM/数据访问层？团队规模与技术栈？最近一次换 ORM 的原因？
2. **发现路径**（5'）：你是怎么知道 sz-orm 的？打开 README 后的前 3 分钟你做了什么？（回放式追问）
3. **首次上手**（15'）：请现场按 quickstart 走一遍（think-aloud）。追问：哪里犹豫过？哪里和预期不符？遇到错误时你做了什么？
4. **Model 与查询体验**（10'）：现场给生成的 Model 加一个字段并查询它。追问：与你在用框架的对比；哪些 API 名字让你猜错了含义？
5. **迁移/评估视角**（10'）：看一份迁移指南后，迁移你现有项目中最常用的一条查询。追问：缺什么信息？哪里不信任文档？
6. **信任与采纳**（5'）：单作者项目、crates.io 滞后、门禁体系——分别改变了你的哪些判断？什么信号会让你决定上/不上？
7. **收尾**（5'）：如果只能改三件事，你改哪三件？

### 附录 B · 可用性测试脚本（5 任务版）

- 环境：干净 Windows/macOS 开发机、已装 Rust（工具链版本记录在案）、无预装数据库（Docker 组）/有本机 MySQL（无 Docker 组）。
- 流程：每任务前只给任务卡（不给文档页码），全程 think-aloud 录屏；卡住 5 分钟未自救成功记为"需协助"；结束后做 5 分制 SEQ（任务易用性）+ 短访谈。
- 观察记录表列：任务 / 开始-结束时间 / 错误路径数 / 文档回看次数 / 卡点描述（对应 §6 断点编号）/ 自救动作 / 完成（Y/N/协助）。
- 样本建议：8-12 人，覆盖 P1/P2/P5 画像为主（P3/P4 用访谈覆盖即可）；中文用户 ≥ 1/3（检验中文资产）。

### 附录 C · 任务完成标准（复测基线）

| 任务 | 通过线 | 优秀线 |
|------|-------|-------|
| T1 路径 A | 按 README/quickstart 命令一次成功；输出含"端到端示例完成" | 耗时 ≤ 文档承诺（需文档改为真实预期） |
| T2 路径 B | 从 cargo new 到 rows=1 无文档外知识 | ≤ 文档总承诺时间（5 分钟） |
| T3 make:model+字段 | 产物 cargo check 通过；字段出现在 columns/fillable/get_column_value/from_value | 手改点 ≤ 2 处（R5 落地后应为 0-1 处） |
| T4 迁移查询 | 按指南写出的代码一次编译通过 | 无需打开 sz-orm 源码 |
| T5 错误诊断 | 从错误输出/文档 3 分钟内定位端口问题 | 文档或错误信息直接给出提示 |

### 附录 D · 本次 L2 实测原始数据索引

| 项 | 位置 |
|----|------|
| 路径 B 实测项目（MySQL/SQLite/readme_check/pg_check） | `D:\www\test\uxr-tmp\szorm-try\` |
| CLI 实测目录（make:model/make:migration 产物） | `D:\www\test\uxr-tmp\cli-lab\` |
| 中文文档 ASCII 名副本（乱码文件名取证用） | `D:\www\test\uxr-tmp\zhdoc-1~4.md` |
| 关键时间线 | 00:34 路径 A 首测失败（manifest）→ 00:43 并行会话恢复 owasp_file_upload.rs（文件时间戳取证）→ 00:52-00:55 路径 B 编译 1m04s + `rows = 1` → 00:58 CLI 编译安装 + info/dialect/make:model/sql:validate → 01:00 路径 A 重跑成功（47.21s） |

### 附录 E · 研究局限

1. 单研究者（研究者本人执行全部 L2），认知走查与启发式评估存在研究者偏差；L3 数据为合成，不能替代真实用户研究。
2. 实测机器为高核数工作站，编译耗时结论不能线性外推到普通笔记本；网络走 rsproxy 镜像，crates.io 直连体验未测。
3. 本机 PostgreSQL 未运行（5432 关闭），quickstart 的 PG 路径仅完成代码编译验证，未完成运行时验证；MySQL 运行时验证止步于凭据错误（协议握手已通）。
4. 评测期间存在并行会话修改同一仓库（00:34-00:43 的 workspace 瞬时损坏与恢复），瞬时性结论以时间线记录为准。
5. 21134+ 测试、94.4% 覆盖率、508 feature gates 等数字声称因成本原因未做全量复跑验证，仅核查了其内部一致性。

---

*报告生成：ZCode UXR 定性评测（2026-10-27）。证据抓取窗口：00:30-01:05（本机时间）。所有 L1 证据行号以当日仓库状态为准。*
