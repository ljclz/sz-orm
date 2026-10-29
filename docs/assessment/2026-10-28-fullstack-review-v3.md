# sz-orm v10.13.0 全栈工程师独立评测（v3）

> 评测对象：`D:\www\test\鲜视达\rust\sz-orm`（branch `main`，workspace 版本 10.13.0）
> 评测日期：2026-10-28；评测方式：**只读**（评测阶段未修改任何源码/脚本/文档/配置，未提交、未推送）
> 安全声明：本次评测**未读取、未打印任何疑似密钥内容**。对疑似凭据文件仅通过「是否被 git 跟踪」「是否被 `.gitignore` 覆盖」「是否命中私钥头特征（只输出文件名）」三种方式判定风险，报告只给出路径与风险类型。
> **本文档包含两个阶段的记录，请勿混读**：第一~九节是**评测当时**（只读、整改前）的证据快照，数字与结论保持原样不回改；**第十节**记录 2026-10-28 按第七节 ROI 清单执行的整改、复测命令与退出码，并逐条标注"原结论 → 现状"。任一数字只有配上它所属的阶段才是可采信的。
>
> **行号漂移声明（2026-10-28 整改阶段核验）**：整改改动了被引用的源文件，因此本报告 `file:line` 分两类，读证据时必须区分——
> ① **仍然成立的现行引用**（指向整改后仍存在的事实行）：`packages/sz-orm-cabi/src/lib.rs` 的导出位置因新增 DSN 守卫整体 **+7 行**，已统一换算为现行行号（如 `query_result_free` `:436`→`:443`、`pool_stats` `:549`→`:556`、`execute_batch` `:606`→`:613`、`transaction_begin` `:822`→`:829`、`pool_free` `:310`→`:317`、`sz_orm_ping` `:339`→`:346`、runtime worker 配置 `:232`→`:239`）；`packages/sz-orm-core/src/pool.rs`、`ffi_memory.rs` 等未改动文件的行号未变。
> ② **只对整改前版本有效的历史引用**（对应代码已被本轮改掉，行号处已不是所述内容）：`packages/sz-orm-cabi/csharp/SzOrm.cs:64`、`:82`（`LPStr`）与 `:135`、`:166`（`PtrToStringAnsi`）——现为 `LPUTF8Str`/`PtrToStringUTF8`，复核命令 `grep -c 'UnmanagedType\.LPStr\b' = 0`、`grep -c PtrToStringAnsi = 0`，现行位置见 §10.3；`scripts/check-secrets.py:62`（`SCAN_EXTENSIONS`）、`:81`（敏感名规则）、`:202`（放行分支）——修复后位置分别为 `:82`–`102`、`:119`–`121`、`:239`–`245`，见 §10.1。这两类历史引用保留原行号**不是为了证明今天的代码**，而是为了证明"评测当时确实看到了那个内容"；其结论的现状一律以第十节为准。

---

## 一、总体结论

**加权总分 6.5 / 10（"技术上值得尊敬，证据上不可采信"）。**

sz-orm 的**技术骨架**（自研连接池、C ABI 设计、28 方言抽象、门禁脚本群）达到了一个成熟项目的形状；但**证据体系**与**交付面**存在系统性虚化：密钥被 git 跟踪而扫描器报「无密钥」、G3 clippy 实际红而文档称已清零、G24/G25 只写在文档里从未接线、证据校验器只验"行存在"不验"行内容"、C# 绑定零 CI 却被 README 标注「E2E tested」。

**三条决定性理由：**

1. **凭据泄露 P0 + 扫描器假绿。** 仓库有 7 个被 git 跟踪的文件命中私钥头特征（含 `tmp_ssh_key2` 与多个 `*.js` 传输脚本），且均未被 `.gitignore` 覆盖；而 `scripts/check-secrets.py` 扫描 2975 个文件后输出 `OK: no secrets detected`（exit 0）。扫描器不扫 `.js`/无扩展名文件，导致"安全门禁通过"这一结论在任何评审中都不成立——这是本次评测**最不可接受**的发现。（整改后状态见 §10.1：扫描器修复后命中数实为 **9 个**，已取消跟踪并入库 `.gitignore`；**密钥轮换与 git 历史清除仍未执行**。）
2. **门禁体系"红灯 + 空转"并存。** 实测 `cargo clippy -p sz-orm-core -p sz-orm-cabi --all-targets -- -D warnings` **exit 1**（`v65_prepared_cache_benefit.rs:10`）；AGENTS 登记的门禁 24/25 在 `run-gates-v10120-server.sh`、`gate.ps1`、CI 三处**均未接线**；门禁编号 26/27/28 重复出现两套；`audit-verify` 只校验"文件存在 + 行号在范围内 + 该行非空"，本次用**两条纯虚构断言**实测通过（exit 0）。"全门禁通过"无法作为结论使用。
3. **FFI/C# 交付未闭环。** `cargo test -p sz-orm-cabi` 在本机**无法完成**：`build.rs:14` 的 cbindgen 触发 `cargo metadata --all-features`，离线直接 exit 101，联网则卡在 `Downloading crates ...` 超过 15 分钟（本机实测 `cargo metadata --all-features` 150 秒超时仍无进展）；C# 侧 7 个 P/Invoke 导出对应 C ABI 的 37 个导出，中文数据走 `LPStr`/`PtrToStringAnsi`（与 Rust UTF-8 语义不一致），且 `.github/workflows/` 中**不存在任何 dotnet/C# job**。（整改后状态见 §10.2/§10.3：离线可构建已修复；C# 编码、终结器竞态、CI job 已处理；本机评审计数有误——实测 9 个 `DllImport` / 6 个公开方法，未覆盖的是事务/参数化 CRUD/批量/统计；**C# 实机 E2E 本机仍无 .NET SDK，未运行**。）

一句话：**可以作为架构参考与内部试点，不宜作为生产依赖接入（尤其 C# 场景）。**

---

## 二、评分表（维度 × 权重 × 得分）

| # | 维度 | 权重 | 得分 | 核心理由（一句） |
|---|------|------|------|------------------|
| 1 | Rust 核心质量 / 内存安全 | 20% | **7.5** | core 库测试 3228 全过、unsafe 全部有 SAFETY 覆盖；但 G3 在 `--all-targets` 下实测红灯，与"已清零"文档矛盾 |
| 2 | 异步并发 | 15% | **8.0** | 连接池 CAS + 条件唤醒 + shutdown 竞态修复有回归测试，背压控制器实现干净；`notify` 许可合并语义未文档化 |
| 3 | 性能 | 15% | **6.5** | 有真实基准代码与性能门禁设计，但本次无任何可复现基准数据，产物未入库 |
| 4 | FFI / ABI + C# | 20% | **5.5** | ABI 设计（repr(C)/37 导出/panic 边界/内存注册表）优秀；但构建需联网、C# 编码与 API 面缺失、零 CI |
| 5 | SQL 方言广度 / 正确性 | 10% | **7.0** | 28 种 DbType、21 个真实驱动、方言分发穷尽 match；7 个方言默认不编译，308 个 feature gate 无启用点 |
| 6 | 工程质量体系 | 10% | **5.5** | 31 道门禁设计用心，但存在红灯、未接线、编号重复、校验器语义空转、扫描器盲区 |
| 7 | 产品化 / 可维护性 | 10% | **5.0** | 双语文档与示例齐全；但 crates.io 落后 6 个版本、README.en 缺绑定章节、AGENTS 版本陈旧、72 包/508 features 负担重 |

**加权计算：**

```
7.5×0.20 + 8.0×0.15 + 6.5×0.15 + 5.5×0.20 + 7.0×0.10 + 5.5×0.10 + 5.0×0.10
= 1.500 + 1.200 + 0.975 + 1.100 + 0.700 + 0.550 + 0.500
= 6.525 ≈ 6.5 / 10
```

---

## 三、分维度详评

### 3.1 Rust 核心质量 / 内存安全（20%，7.5 分）

**亮点（有证据）**

- 核心连接池实现是"真代码"而非包装：`packages/sz-orm-core/src/pool.rs:1980` [must-contain: pub async fn acquire] 的 `acquire` 是完整的状态机，包含 CAS 计数（`packages/sz-orm-core/src/pool.rs:2118` [must-contain: AtomicU32]）、CAS 之后的 closed 二次检查（`packages/sz-orm-core/src/pool.rs:2140` [must-contain: 收窄]）、等待循环顶部的关池唤醒（`packages/sz-orm-core/src/pool.rs:2037` [must-contain: closed.load]）、释放路径的条件 `notify_one`（`packages/sz-orm-core/src/pool.rs:2371` [must-contain: notify_one]）与 `shutdown`（`packages/sz-orm-core/src/pool.rs:2742` [must-contain: pub async fn shutdown]）。shutdown 竞态还有专门回归测试：`packages/sz-orm-core/tests/chaos_pool.rs:819`。
- 实测 `CARGO_NET_OFFLINE=true cargo test -p sz-orm-core --lib` → **exit 0，3228 passed / 0 failed**（见 §8.1）。
- unsafe 治理是本次评测**少数完全达标**的项：`python scripts/check-unsafe-audit.py` → exit 0，17 个含 unsafe 关键字文件、**0 个缺 SAFETY 覆盖**；cabi 内 `// SAFETY:` 注释 316 处对应 278 个 unsafe 块（注释多于块，说明覆盖到语句级）。
- 工具链钉子真实存在：`rust-toolchain.toml:13` 为 `channel = "1.99.1"`；`async-trait` 已升至 0.1.92（`Cargo.lock:892`）。

**风险**

- **P1｜G3 实际红灯，与文档矛盾。** 实测 `cargo clippy -p sz-orm-core -p sz-orm-cabi --all-targets -- -D warnings` → **exit 1**，唯一错误在 `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:10`（`clippy::doc_lazy_continuation`，`//!` 续行缩进）。历史报告曾称"G3 红灯清零 exit 0"，说明**测试目标被排除在当次验证之外**（库目标能过、`--all-targets` 不过）。
- **P2｜MSRV 与门禁工具链双轨。** 下游承诺 `rust-version = "1.81"`，实际用 1.99.1 跑门禁；`async-trait 0.1.92` 这类"靠升级依赖消除新 lint"的修复对 MSRV 用户不可见，存在"CI 绿 / 用户红"的漂移风险。
- **P3｜规模与维护成本。** 1755 个 `.rs` 文件、72 个 workspace 成员、508 个 feature——对单一 ORM 而言属于超大工作面，任何跨包改动都要付出索引成本。

**建议**

1. 修 `v65_prepared_cache_benefit.rs:10` 的 doc 缩进，并把 G3 的验收口径固定为 `--all-targets`（否则"清零"永远可能是假绿）。
2. 在 CI 增加 `--all-targets` + MSRV 1.81 双跑，避免工具链升级掩盖问题。

### 3.2 异步并发（15%，8.0 分）

**亮点（有证据）**

- 连接池用 `AtomicU32` CAS + `crossbeam_queue::ArrayQueue` + `tokio::Notify`，没有用 `Mutex` 包裹计数（`packages/sz-orm-core/src/pool.rs:2118` [must-contain: AtomicU32] 注释明确说明 v0.2.1 用 CAS 替换了 `Mutex<u32>`）。
- 关池竞态处理到位：`closed` 置位后唤醒等待者、`acquire` 回到循环顶部重查（`packages/sz-orm-core/src/pool.rs:2037` [must-contain: closed.load]），CAS 成功后再次检查 `closed` 以收窄窗口（`packages/sz-orm-core/src/pool.rs:2140` [must-contain: 收窄]）。
- 流式背压是独立组件而非文档口号：`packages/sz-orm-stream/src/backpressure.rs:45` 的 `AsyncBackpressureController` 用 `AtomicUsize` + `Notify` 实现容量控制，信号丢失由等待循环兜底。
- 语句缓存有命中/未命中原子计数与 `get_or_prepare` 入口（`packages/sz-orm-core/src/prepared_cache.rs:182`），具备可观测性。

**风险**

- **P2｜`Notify` 单许可合并语义未文档化。** `notify_one` 在无等待者时最多存 1 个许可；连续两次 `release` 只会存 1 个许可。当前实现靠"获得连接者释放时再 notify"保证不死锁，但**只增加唤醒延迟**；高并发下可能表现为偶发长尾，建议在 `pool.rs` 注释中登记该性质，避免后人误改。
- **P3｜cabi 全局 runtime 默认 2 worker。** `packages/sz-orm-cabi/src/lib.rs:239` 提供了 `sz_orm_set_runtime_workers`，但默认 2 个 worker 对 C#/Go 侧并发调用是隐性瓶颈，且"必须在建池前调用"的时序约束只在测试中体现。

**建议**

1. 在 `pool.rs` 的 notify 逻辑处补一条"许可合并"不变量注释，并补一个"两次连续 release 只唤醒一个等待者仍最终全部取得连接"的回归测试。
2. 在 cabi 头文件与 C# 封装中暴露 worker 配置，或把默认值随 CPU 数自适应。

### 3.3 性能（15%，6.5 分）

**亮点（有证据）**

- 基准不是模拟：仓库内已有真实工作负载入口与 criterion 基准（历史报告确认 `run_workload` 已从源码移除、`run_workload_real` 为唯一实现）。
- 性能门禁设计合理：`benchmark` job 在 main 分支 `continue-on-error: false`，退化 ≥10% 阻断（`.github/workflows/ci.yml:748` 起注释说明）。
- 连接池复用 + 语句缓存 + 条件唤醒三件套齐备，性能优化路径是真实存在的。

**风险**

- **P2｜无任何可复现基准数据。** 本次评测**无法执行**基准（`bench-comparison/target` 已被清理，重建需冷编译 diesel/sea-orm/rusqlite-bundled，历史记录估计 20 分钟级且未完成）。README/报告中的性能数字无法在本机复核。
- **P2｜基准条件 `continue-on-error` 在 PR 上为 true。** `.github/workflows/ci.yml:757` 使 PR 阶段性能退化不阻断，只有 main 才阻断——退化代码完全可以先进 main。
- **P3｜性能门禁依赖 GitHub runner 稳定性。** 共享 runner 的抖动会让 10% 阈值时真时假，需自托管 runner 才有公信力。

**建议**

1. 把 criterion baseline 与摘要 JSON 入库（或作为 CI artifact 长期保留），否则"性能无退化"永远无法被外部审计。
2. 性能门禁改为"PR 阻断但允许 label 豁免"，比"PR 不阻断"更符合"防退化"目标。

### 3.4 FFI / ABI 与跨语言（20%，5.5 分）

**亮点（有证据）**

- ABI 设计规范：`#[repr(C)]` 结构体（`packages/sz-orm-cabi/src/lib.rs:121` 的 `PoolConfigC`）、`#[no_mangle] extern "C"` 导出 37 个、`catch_unwind` 33 处（panic 不跨越 FFI 边界）、cbindgen 头文件作为单一声明源（`packages/sz-orm-cabi/include/sz_orm_cabi.h:18` 说明了 Rust 侧分配/释放语义）。
- 内存所有权有运行时防线：`packages/sz-orm-cabi/src/ffi_memory.rs:29` 的 `track` 与 `:40` 的 `untrack` 构成 double-free 注册表，释放路径统一走注册表。
- 能力面不薄：除池/查询/执行外，还导出事务（`packages/sz-orm-cabi/src/lib.rs:829` 起）、模型 CRUD、查询构建器、批量执行（`packages/sz-orm-cabi/src/lib.rs:613`），头文件明确标注参数化条件（`REQ-BND-007`）。
- ABI 演进是追加式的：`sz_orm_set_runtime_workers`（`packages/sz-orm-cabi/src/lib.rs:239`）未改动任何既有结构体布局。

**风险**

- **P0｜构建强依赖网络。** `packages/sz-orm-cabi/build.rs:14`（整改前版本；重写后该行已变为 `use std::fs;`，现行生成逻辑在 `:43`）调 `cbindgen::generate`，其内部执行 `cargo metadata --all-features`；实测离线 `cargo test -p sz-orm-cabi` → **exit 101**（`failed to download actix-http v3.13.5 ... --offline was specified`），联网则在 `Downloading crates ...` 卡死 >15 分钟（`cargo metadata --all-features` 150s 超时 exit 124）。**在受限网络/离线 CI 中 cabi 不可构建**，这对绑定层是致命的可复现性问题。（已整改：生成失败改为回退到仓库内头文件 + 显式跳过开关，离线 `cargo test -p sz-orm-cabi` 现 exit 0，见 §10.2。）
- **P1｜cabi 实际只支持 SQLite。** `packages/sz-orm-cabi/src/lib.rs:281`（整改前该行是 `pool_new` 无条件走 `SqlitePoolHandle::connect` 的函数体；**整改后 `:281` 变成了拒绝非 SQLite DSN 的守卫**）：所有语言绑定**只能连 SQLite**，但 ABI 文档与 README 的"6 语言绑定"读起来像是通用 ABI，评测当时没有任何一处显式声明该限制。（已整改：`packages/sz-orm-cabi/include/sz_orm_cabi.h:4`–`6` 显式声明 UTF-8 与"仅 SQLite DSN"、守卫返回 nullptr 并有回归测试 `lib.rs:2286`，见 §10.2。）
- **P1｜头文件不声明字符编码。** `packages/sz-orm-cabi/include/sz_orm_cabi.h` 全文无 UTF-8 约定，`*mut c_char` 返回的 JSON 是 UTF-8 字节流；绑定方若按本地 ANSI 解码（C# 默认就是）必然乱码。（已整改：契约横幅写入头文件与 `build.rs` 生成的 `OUT_DIR` 副本，C# 侧改用 `LPUTF8Str`/`PtrToStringUTF8`，见 §10.2/§10.3。）
- **P2｜缺少句柄生命周期抽象。** C ABI 的句柄是裸指针，没有 generation/closed 标记；跨语言侧只能靠"约定配对释放"，无法在 ABI 层拦截 use-after-free（`ffi_memory` 只拦截 double-free）。

**建议**

1. 让 `build.rs` 在 `include/sz_orm_cabi.h` 已存在时可跳过 cbindgen（或改用 `cbindgen --no-cargo` 的静态配置），保证离线可构建。
2. 头文件增加两行契约：`All returned strings are NUL-terminated UTF-8`、`This ABI currently supports SQLite DSNs only`。
3. `pool_new` 对非 `sqlite:` DSN 返回明确的错误码，而不是"尝试当 SQLite 打开然后失败"。

### 3.5 SQL 方言广度 / 正确性（10%，7.0 分）

**亮点（有证据）**

- 方言枚举 28 个（`packages/sz-orm-core/src/db_type.rs:11`），`get_dialect` 用穷尽 `match` 分派（`packages/sz-orm-core/src/dialect.rs:2610`），新增变体会编译报错，属于"编译期防漏"。
- 真实驱动覆盖：sqlx（MySQL/PG/SQLite）、`oracle` crate、`tiberius`（MSSQL）、SAP HANA feature——不是"仅生成 SQL 不连库"的空壳。
- SQL 注入门禁是自研规则引擎（R1–R7），本次实测 `--strict` 通过 46 项 Safe；`where_cond`/`or_where` 已从 core 源码移除（不再给用户"字符串拼条件"的口子）。

**风险**

- **P2｜7 个方言默认不编译。** `Phantom-delivery` 报告 **PHANTOM-2 = 308** 个"feature 从未被任何成员启用"的告警（详见 §8.1），说明相当比例的方言/能力在默认构建中不存在；文档若按"支持 28 种方言"表述，会高估默认能力。
- **P2｜正确性证据不可复现。** 本次未运行 `--ignored` 真实库集成测试（MySQL 9.6 / PG 18 / Oracle 23ai 本机存在，但属长跑套件且与并行会话争抢资源）。28 方言的差异（分页、UPSERT、RETURNING、标识符引号）只能靠测试证明，而测试未在本轮执行。
- **P3｜方言抽象是字符串层。** `Dialect` trait 输出 SQL 文本，没有 typed AST 级的跨方言校验；复杂表达式在方言间迁移时仍可能"能拼出来但语义不同"。

**建议**

1. 在 README 方言矩阵加一列"默认编译"，把 7 个 cfg-gated 方言明确标注为按需启用。
2. 为 top-5 方言（MySQL/PG/SQLite/Oracle/MSSQL）建立"同一逻辑查询 → 期望 SQL 快照"的 golden 测试，比"能连上"更能证明正确性。

### 3.6 工程质量体系（10%，5.5 分）

**亮点（有证据）**

- 门禁数量与种类是真的多：`AGENTS.md:51` 起登记了 24/25/26/27/28 等门禁；`scripts/run-gates-v10120-server.sh` 实跑 1–19、21、23、26–31；CI 有 `metrics-real`、`checker-self-tests`、`check-doc-sync`、`coverage`、`mutation`、`security` 等 job。
- 度量真实性门禁有效：`check-metrics-real.py` 输出"包=70 成员=72 测试标注=21183 feature=508 版本=10.13.0"，与 README 声称一致（exit 0）。
- CI 与 deny 的豁免集合**确实同步**：`deny.toml` 21 条 RUSTSEC 豁免与 `.github/workflows/` 中 `--ignore` 集合 diff 完全一致（实测无差异）。
- 幻影交付门禁能跑：PHANTOM-1 = 0、接线断言 4/4 通过。

**风险**

- **P0｜凭据扫描器假绿（已复现）。** `scripts/check-secrets.py` 扫描 2975 文件报"OK: no secrets detected"（exit 0），但 `git ls-files` 中 7 个文件命中私钥头特征，且 `.gitignore` 未覆盖 `tmp_ssh_key2`。根因：`SCAN_EXTENSIONS`（`scripts/check-secrets.py:62`）不含 `.js`，`should_scan_file` 对无扩展名文件放行（`scripts/check-secrets.py:202`），`SENSITIVE_FILE_PATTERNS`（`scripts/check-secrets.py:81`）也不匹配该文件名。另：`.git/config` 的 remote URL 内嵌凭据（仅记录风险类型，未打印内容）。
- **P0/P1｜证据校验器语义空转（已复现）。** `scripts/audit-verify.ps1:96` 起的 `Test-Citation` 只做三件事：文件存在、行号 ≤ 物理行数、被引行非空。本次用两条**纯虚构断言**（"连接池内置量子随机数生成器""零拷贝 protobuf 编码"）引用真实行，脚本输出"通过 2 / 失败 0"、**exit 0**。（本轮**未整改**——属高成本项，列入 §10.7 遗留项，结论仍然有效。）
- **P1｜G24/G25 从未接线。** `check-examples-readme.py`、`check-doc-code-compiles.py` 在 `run-gates-v10120-server.sh`、`gate.ps1`、`.github/workflows/` 中**均无任何引用**；AGENTS 却把它们登记为门禁 24/25。（已整改：三处执行器均已接线，见 §10.4。）
- **P1｜门禁编号重复掩盖覆盖缺口。** `AGENTS.md:51` 起同表内出现两套 26/27/28（`AGENTS.md:51`–`AGENTS.md:61` 区间），真实唯一编号 31 道；`run-gates-v10120-server.sh` 接线 27 道（20/22 外部执行，24/25 缺失）。（已整改：编号统一为 1–34 唯一，UXR 三兄弟改为 32/33/34，见 §10.4。）
- **P1｜本地门禁 `gate.ps1` 只覆盖 19 步。** 缺 G14/G15/G16/G17/G18/G19/G21/G23/G24/G25/G30/G31（`grep "Invoke-Step"` 计数 19），Windows 开发者"本地全过"≠"服务器全过"。（部分整改：补齐 G24/G25/G30–G34；G14–G19/G21/G23 仍缺，见 §10.4 遗留项。）
- **P1｜门禁自检脚本可被绕过（已复现）。** `scripts/check-gate-scripts.py` 的硬编码 PASS 规则要求整行以 `mark N PASS` 开头，`if true; then mark 24 PASS; fi` 与 `true; mark 24 PASS` 两种写法**均不命中**；`||` 假 PASS 规则也只匹配单行形态。且"关键脚本已接入"只做**字符串出现**判断，不校验是否真正执行并传播失败。（已整改：新增恒真条件/同链顺序执行/多行续行/PowerShell 无判据 Invoke-Step 四类规则 + 反例夹具 + pytest 常驻回归，见 §10.5。）
- **P2｜文档数字漂移。** `CHANGELOG.md:34` 称 `--ignore` 列表"24 条"，实际 `deny.toml` 与 CI 均为 **21 条**（集合一致，数字陈旧）。（已整改：改为 21 条并注明复核日期，见 §10.6。）
- **P2｜门禁脚本在 GBK 控制台崩溃。** 本次在 Windows 默认代码页下运行 Python 门禁出现 `UnicodeEncodeError: 'gbk' codec can't encode '\u2705'`，需 `PYTHONIOENCODING=utf-8` 才能跑——本机开发者会看到"门禁红"的假象。
- **P2｜`check-unsafe-audit.py` 是文件级判定。** `scripts/check-unsafe-audit.py:72` 用 `"// SAFETY:" in content` 判断整文件，只要文件内任一 SAFETY 注释存在，其余块缺注释也会放过（当前 316 条注释/278 块掩盖了该缺陷，但规则本身粒度不足）。

**建议（按优先级）**

1. 移除 7 个凭据文件出库（改 `.gitignore` + `git rm --cached` + **轮换密钥**），并把 `.js`/无扩展名/`.txt` 纳入扫描；扫描命中即 fail-closed。
2. `audit-verify` 增加最小语义断言：引用行必须包含被引符号名（如 `file:line` 后紧跟的 `fn xxx` 必须出现在该行±3 行内）。
3. 真正接线 G24/G25（或从 AGENTS 删除，不要"登记即完成"）。
4. 统一门禁编号（1–31 唯一），并让 `gate.ps1` 与 `run-gates-v10120-server.sh` 从同一份清单生成。
5. `check-gate-scripts.py` 改为"解析 AST/语法结构"或至少覆盖 `if <常量>; then mark`、`cmd; mark`、多行 `||` 三类。

### 3.7 产品化 / 可维护性（10%，5.0 分）

**亮点（有证据）**

- 双语 README、148 份 `docs/assessment/*.md`、示例清单门禁（G24 设计）、快速开始与绑定章节都存在。
- JS 类型声明已补齐：`packages/sz-orm-js/index.d.ts` 为 9897 字节（不再是 0 字节）。
- feature gate 化重依赖（actix/redis/oracle 等）让默认构建可控。

**风险**

- **P1｜crates.io 落后 6 个版本。** 实测 crates.io `sz-orm-core` `max_version = 10.7.0`（updated 2026-10-16，downloads 4139），而代码版本为 `Cargo.toml:6` 的 10.13.0；`README.md:453` 的安装示例仍写 `sz-orm-core = "10.7"`。用户按 README 安装得到的是**六个 minor 版本前**的代码。（本轮未发版，属发布动作；文档口径已改为"最新已发布 10.7.0 / 当前代码 10.13.0 未发布"，见 §10.6、§10.7。）
- **P1｜AGENTS 版本声明陈旧。** `AGENTS.md:10` 仍写"sz-orm-core 1.0.0 已发布到 crates.io（2026-07-23）"，与 crates.io 现状（10.7.0）不符。（已整改：经 sparse index 核实后改写为"已发布 30 个版本，最新 10.7.0；当前代码 10.13.0 尚未发布"，见 §10.6。）
- **P1｜英文文档缺绑定章节。** `README.en.md` 中 `cabi`/`C#`/`binding` 出现次数为 **0**；`README.md:593` 与 `README.zh.md:554` 有完整绑定矩阵——国际用户看不到跨语言能力。（已整改：`README.en.md` 新增 Language Bindings 章节 + 过期快照横幅，见 §10.6。）
- **P2｜workspace 内部依赖版本滞后。** 内部 path 依赖统一写 `version = "10.12.0"`（`Cargo.toml:83`）而 workspace 为 10.13.0；`check-publish-consistency.py` 将其视为"下限合法"仅告警。发布时可能出现"声明 10.12 实际 10.13"的混淆。
- **P2｜feature 面过大。** 508 个 feature、308 个从未启用的 feature gate，用户很难判断"哪些能力默认可用"。

**建议**

1. 发版流程把 crates.io 发布作为"版本号变更"的强制后继步骤（或至少让 `--check-crates-io` 进入 CI 阻断）。
2. 修正 `AGENTS.md:10` 与 README 安装示例，统一指向"最新已发布版本"，并给出 git 依赖指引。
3. 同步 `README.en.md` 的绑定章节；把内部 path 依赖版本随 workspace 版本自动同步（`cargo release`/`workspace-hack` 脚本）。

---

## 四、横向对比

| 维度 | sz-orm（C# 绑定） | EF Core | Dapper | sqlpp11 / SOCI（C++） | Diesel | SeaORM | SQLx（Rust） |
|------|------------------|---------|--------|----------------------|--------|--------|--------------|
| 生态成熟度 | 低（crates.io 10.7.0，C# 无 NuGet） | 极高（.NET 官方） | 高 | SOCI 稳定 / sqlpp11 维护放缓 | 高 | 中高 | 高 |
| 语言绑定 | C ABI + 6 语言（仅 SQLite，C# 7/37 API） | 原生 C# | 原生 C# | 原生 C++ | 原生 Rust | 原生 Rust | 原生 Rust |
| 类型安全 | Rust 侧强；C ABI 退化为字符串/JSON | 强（LINQ/编译期） | 弱（SQL 字符串 + 映射） | sqlpp11 强 / SOCI 中 | 强（DSL） | 强 | 强（`query!` 编译期校验） |
| 参数化 | ABI 有（`REQ-BND-007`），**C# 封装未暴露** | 内建 | 内建 | 内建 | 内建 | 内建 | 内建 |
| 迁移工具 | Rust 侧有；C# 无 | 内建 | 无（第三方） | 无 | 内建 | 有 | 无（sqlx-cli 有） |
| 方言广度 | 28 种（7 种默认不编译） | 多 provider | 任意 ADO.NET | 各后端独立 | PG/MySQL/SQLite | 多 | PG/MySQL/SQLite/MSSQL |
| 性能潜力 | 高（Rust 侧池/缓存） | 中 | 高（薄） | 高 | 高 | 中高 | 高 |
| 可观测/运维 | 有 stats/metrics 导出（C# 未封装） | 强 | 弱 | 弱 | 中 | 中 | 中 |
| 主要风险 | 证据体系/凭据/构建联网 | 重、抽象泄漏 | 手写 SQL 风险 | 生态老、工具链 | 编译慢、方言少 | 年轻 | 需 DB 才能编译校验 |

**结论：** sz-orm 的差异化在"一个 ABI 覆盖多语言"与"28 方言"，但这两点目前都停留在**未闭环**状态（仅 SQLite、C# 面窄、构建联网）。与 EF Core/Dapper 比，C# 侧不具备可用性优势；与 SQLx 比，Rust 侧池与缓存有竞争力，但缺少 compile-time query 校验的生态心智。

---

## 五、C# 集成专项（P/Invoke 细节 / 缺口 / 打包 / 工作量 / go-no-go）

### 5.1 结构体布局与调用约定

- 结构体镜像正确：`PoolConfig`(u32,u32,u64,u64)、`QueryResult`(i32,i32,u64,u64)、`QueryJsonResult`(i32,i32,IntPtr) 与 Rust `#[repr(C)]` 一致（`packages/sz-orm-cabi/src/lib.rs:121` 与 `packages/sz-orm-cabi/csharp/SzOrm.cs` 的 `[StructLayout(LayoutKind.Sequential)]`）。
- 调用约定统一 `Cdecl`，与 `extern "C"` 匹配；返回值按值返回结构体（`sz_orm_execute`）在 Windows x64 ABI 下由隐藏指针传递，.NET 能正确处理——**布局层无问题**。

### 5.2 编码（P1，实测不可运行，静态可判定）

- 入参用 `[MarshalAs(UnmanagedType.LPStr)]`（**整改前** `packages/sz-orm-cabi/csharp/SzOrm.cs:64`、`:82`），出参用 `Marshal.PtrToStringAnsi`（**整改前** `packages/sz-orm-cabi/csharp/SzOrm.cs:135`、`:166`）。
- Rust 侧 `CStr::to_str` 要求 **UTF-8**，返回的 JSON 也是 UTF-8。`LPStr`/ANSI 在中文 Windows 上按 GBK 编码，**中文 DSN/中文 SQL/中文数据都会损坏**；`Program.cs` 的 E2E 用例全是 ASCII（`packages/sz-orm-cabi/csharp/SzOrmTest/Program.cs:4` 之后的断言只含 Alice/Bob/Carol），因此该缺陷不会被现有测试发现。
- 修复：入参改 `UnmanagedType.LPUTF8Str`（.NET 4.7+/Core 支持），出参改 `Marshal.PtrToStringUTF8`；或提供 `byte[]` 版 API。

### 5.3 所有权与异常

- 所有权模型正确但"只靠约定"：池句柄/查询结果/字符串都必须由调用方显式释放（`packages/sz-orm-cabi/src/lib.rs:443` 的 `query_result_free`、`packages/sz-orm-cabi/src/lib.rs:317` 的 `pool_free`）。Rust 侧有 track/untrack 注册表拦截 double-free，但 C# 侧没有 `SafeHandle`，无法防 use-after-free。
- 异常语义：Rust panic 被 `catch_unwind` 拦截（如 `packages/sz-orm-cabi/src/lib.rs:346` 的 ping），C# 侧统一抛 `InvalidOperationException`（错误码拼进消息），**异常类型粒度不足**（连接失败/语法错误/约束冲突都是同一种异常），不利于上层重试策略。
- **P1｜终结器竞态。** `packages/sz-orm-cabi/csharp/SzOrm.cs:192` 的 `~SzOrmPool()` 调 `Dispose()`，而 `Dispose` 非线程安全且未对 `_handle` 做原子交换；在进程退出/GC 并发时存在 double-dispose 窗口（Rust 侧注册表会拦截，但会返回错误码被忽略）。（已整改：`Dispose`/终结器均改为 `Interlocked.Exchange` 原子摘取句柄 + `volatile` 已释放标记，见 §10.3；`SafeHandle` 与强类型异常仍未做，列入 §10.7。）

### 5.4 线程安全

- `SzOrmPool` 无任何同步；多个线程同时 `Query/Execute` 依赖 Rust 侧池的并发安全。Rust 池本身是并发的，但 C# 封装没有 `volatile`/`Interlocked` 处理 `_handle`/`_disposed`，**Dispose 与并发调用之间存在 use-after-free 窗口**。应在文档中明确"一个实例可在多线程并发使用，但不得与 Dispose 并发"。

### 5.5 API 面缺口（对照 C ABI 37 个导出）

> 计数勘误（2026-10-28 整改阶段复核）：`packages/sz-orm-cabi/csharp/SzOrm.cs` 实测为 **9 个 `DllImport`/`static extern`**（`grep -c 'static extern'` = 9），`SzOrmPool` 对外 **7 个公开方法**（Create/IsHealthy/Query/Execute/ErrorDescription/BindVersion/Dispose）。本节此前写的"7 个"是把 `sz_orm_version`、`sz_orm_error_description` 等静态方法漏算的计数错误，不影响"事务/参数化 CRUD/批量/统计/指标未封装"的结论。C ABI 侧 `#[no_mangle]` 导出为 37（`grep -c '#\[no_mangle\]' packages/sz-orm-cabi/src/lib.rs`）。

| 能力 | C ABI | C# 封装 | 影响 |
|------|-------|---------|------|
| 连接池/查询/执行/版本/错误描述 | 有 | **有（9 个 P/Invoke 导入）** | 基础可用 |
| 事务 begin/execute/commit/rollback/free | 有（`packages/sz-orm-cabi/src/lib.rs:829` 起） | 无 | 无法做事务 |
| 模型 CRUD（参数化） | 有（头文件 `REQ-BND-007`） | 无 | 只能手写 SQL 字符串（注入面） |
| QueryBuilder（where_eq/order/limit/build） | 有 | 无 | 无 |
| 批量执行 | 有（`packages/sz-orm-cabi/src/lib.rs:613`） | 无 | 无 |
| 池统计/指标 | 有（`packages/sz-orm-cabi/src/lib.rs:556`） | 无 | 无可观测 |
| runtime worker 配置 | 有（`packages/sz-orm-cabi/src/lib.rs:239`） | 无 | 无法调优 |

### 5.6 打包建议

1. 建 NuGet 包（`SzOrm.Client`），native 资产放 `runtimes/win-x64/native/sz_orm_cabi.dll` 等 RID 目录，避免 `csproj` 里硬编码 `..\..\..\..\target\debug`。
2. 提供 `netstandard2.0`（或 `net8.0`）目标而非 `net9.0`，扩大可用面。
3. 加 `SafeHandle` 子类 + `IDisposable`，去掉终结器直接调 `Dispose`。
4. 错误码 → 强类型异常（`SzOrmConnectionException`/`SzOrmQueryException`/`SzOrmConstraintException`）。
5. 发布 `.pdb`/SourceLink，并在 NuGet README 明示"仅 SQLite、UTF-8"。

### 5.7 工作量估算（1 人，熟悉 Rust+FFI）

| 工作 | 估算 |
|------|------|
| 修编码（LPUTF8Str/PtrToStringUTF8）+ 中文用例 | 0.5–1 人日 |
| SafeHandle + Dispose 并发安全 | 1–2 人日 |
| 补齐事务/模型/QB/批量/统计封装 | 3–5 人日 |
| 强类型异常映射 | 0.5–1 人日 |
| NuGet 打包 + 多 RID native 资产 | 2–3 人日 |
| CI job（dotnet build/test 多平台） | 0.5–1 人日 |
| 文档 + 示例 | 1 人日 |
| **合计（可预览）** | **9–14 人日** |
| 生产加固（AOT/Trim/多平台/故障注入） | 另加 10–15 人日 |

### 5.8 Go / No-Go

**No-Go（当前）**：编码缺陷 + 无事务/参数化 API + 零 CI + 无 NuGet + 构建联网，任一项在真实业务里都会出事。
**Go 条件**：完成 §5.7 前 6 项（约 2 周）+ 修复 §3.6 的 P0 凭据与扫描器问题后，可作为"预览版"接入非核心服务。

---

## 六、历史发现复核表

| 历史结论（来源） | 复核方法 | 结论 |
|------------------|----------|------|
| G3 clippy 已清零（2026-10-26 P0-1a） | 实跑 `--all-targets -D warnings` | ❌ **不成立**：exit 1，`v65_prepared_cache_benefit.rs:10` |
| async-trait 升级至 0.1.92 消除 double_must_use | 查 `Cargo.lock:892` | ✅ 成立（0.1.92） |
| 工具链钉 1.99.1 | 查 `rust-toolchain.toml:13` | ✅ 成立 |
| G18 度量同步（21153） | 实跑 `check-metrics-real.py` | ⚠️ 数字已变为 **21183**（README 已同步，历史数字失效） |
| JS `index.d.ts` 已补齐 | 文件大小 9897 字节 | ✅ 成立 |
| cabi runtime worker 可配置 | `packages/sz-orm-cabi/src/lib.rs:239` | ✅ 成立（但 C# 未暴露） |
| cabi SAFETY 紧邻化 | 316 条 SAFETY / 278 块 | ✅ 成立 |
| "C# 绑定 E2E tested"（`README.md:607`） | 全量 grep `.github/workflows/` | ❌ **不可验证**：无任何 dotnet/C# job；`verify_bindings.ps1` 也不含 cabi |
| 覆盖率 90.2%（v9.0.0，2026-09-26） | 工具缺失 | ⚠️ 无法复验；且为 **9.0.0** 数据，外推到 10.13.0 无依据（`AGENTS.md:176`） |
| 变异杀率 75%（v9.0.0） | 工具缺失 | ⚠️ 无法复验（同上） |
| "sz-orm-core 1.0.0 已发布"（`AGENTS.md:10`） | crates.io API | ❌ 不成立：现为 10.7.0（2026-10-16） |
| CHANGELOG "24 条 ignore"（`CHANGELOG.md:34`） | deny.toml vs CI diff | ❌ 数字陈旧：实际 21 条（集合一致） |
| "23 关门禁全部通过" | 清点编号与接线 | ❌ 编号重复（26/27/28 两套），唯一 31 道，接线 27 道，24/25 未接线 |
| 性能无退化（历史报告） | 无基准产物 | ⚠️ 无法复验 |

---

## 七、ROI 排序整改清单

| 优先级 | 整改项 | 预估收益 | 成本 | 证据锚点 |
|--------|--------|----------|------|----------|
| **P0-1** | 移除 7 个凭据文件出库 + 轮换密钥 + `.gitignore` 兜底 | 阻断真实泄露风险（最高） | 0.5 人日 | `tmp_ssh_key2` 等（见 §8.2） |
| **P0-2** | 修 `check-secrets.py` 盲区（`.js`/无扩展名），命中即失败 | 让"安全门禁"重新可信 | 0.5 人日 | `scripts/check-secrets.py:62`、`:202` |
| **P0-3** | 修 clippy 红灯并固定 `--all-targets` 验收 | 恢复 G3 公信力 | 0.2 人日 | `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:10` |
| **P1-1** | cabi 构建去网络化（header 已存在则跳过 cbindgen） | 离线/受限网络可构建 | 1 人日 | `packages/sz-orm-cabi/build.rs:14` |
| **P1-2** | C# 编码修复 + 中文用例 + CI job | 打开中文市场，消除乱码 | 1.5 人日 | `packages/sz-orm-cabi/csharp/SzOrm.cs:64`、`:135` |
| **P1-3** | 接线 G24/G25（或删除登记）+ 统一门禁编号 | 消除"登记即完成"幻觉 | 1 人日 | `AGENTS.md:51`、`AGENTS.md:61` |
| **P1-4** | audit-verify 增加符号级断言 | 让审计结论可被机器检验 | 1 人日 | `scripts/audit-verify.ps1:96` |
| **P1-5** | C# 暴露事务/参数化 CRUD/SafeHandle | 消除注入面与 use-after-free | 4–6 人日 | `packages/sz-orm-cabi/src/lib.rs:829` |
| **P2-1** | crates.io 发版追平 + README 安装示例同步 | 用户装到的不是 6 个版本前的代码 | 1 人日 | `README.md:453` |
| **P2-2** | 基准产物入库 / artifact 持久化 | 性能结论可复验 | 1 人日 | §3.3 |
| **P2-3** | `check-gate-scripts.py` 规则结构化 | 防假绿 | 0.5 人日 | §8.2 R4 |
| **P2-4** | README.en 补绑定章节；AGENTS 版本修正 | 国际化与一致性 | 0.5 人日 | `AGENTS.md:10` |
| **P3-1** | 方言"默认编译"标注 + golden SQL 快照 | 降低 28 方言的宣称风险 | 3–5 人日 | `packages/sz-orm-core/src/db_type.rs:11` |
| **P3-2** | pool notify 许可合并注释 + 回归测试 | 降低长尾误改风险 | 0.5 人日 | `packages/sz-orm-core/src/pool.rs:2368` [must-contain: 条件 notify_one] |

**建议顺序：** P0 全部（1.2 人日）→ P1-3/P1-4（让门禁可信）→ P1-1/P1-2 → P2 → P3。**先修"可信度"，再修"能力"。**

---

## 八、证据记录与索引

### 8.1 实测命令与退出码（本机 2026-10-28）

| # | 命令 | 退出码 | 关键输出 |
|---|------|--------|----------|
| 1 | `cargo fmt --all -- --check` | **1** | `文件名或扩展名太长。 (os error 206)`（Windows 路径长度限制） |
| 2 | `cargo fmt -p sz-orm-core -- --check` | 0 | 无输出（通过） |
| 3 | `cargo fmt -p sz-orm-cabi -- --check` | 0 | 无输出（通过） |
| 4 | `cargo clippy -p sz-orm-core -p sz-orm-cabi --all-targets -- -D warnings` | **1** | `error: doc list item without indentation --> packages\sz-orm-core\tests\v65_prepared_cache_benefit.rs:10:5`；`could not compile sz-orm-core (test "v65_prepared_cache_benefit")` |
| 5 | `CARGO_NET_OFFLINE=true cargo test -p sz-orm-core --lib` | **0** | `test result: ok. 3228 passed; 0 failed; 0 ignored`（31.31s） |
| 6 | `CARGO_NET_OFFLINE=true cargo test -p sz-orm-cabi` | **101** | `failed to run custom build command for sz-orm-cabi`；`cbindgen: unable to generate bindings: CargoMetadata(... failed to download actix-http v3.13.5 ... --offline was specified)` |
| 7 | `cargo test -p sz-orm-cabi`（联网） | 未完成 | 卡在 build-script（cbindgen → cargo metadata），>15 分钟无进展，手动终止 |
| 8 | `timeout 150 cargo metadata --all-features --manifest-path packages/sz-orm-cabi/Cargo.toml` | **124** | 卡在 `Downloading crates ...`（150s 超时） |
| 9 | `python scripts/check-metrics-real.py` | 0 | `包=70 成员=72 测试标注=21183 feature=508 版本=10.13.0`；READM 数字一致 |
| 10 | `python scripts/check-unsafe-audit.py` | 0 | `含 unsafe 关键字文件数: 17`；`SAFETY 双口径皆缺: 0`；`误报: 36` |
| 11 | `python scripts/check-phantom-delivery.py` | 0 | `PHANTOM-1 0 个`；`PHANTOM-2 308 个`；`接线断言 4/4` |
| 12 | `python scripts/check-secrets.py` | 0 | `Scanned 2975 files`；`OK: no secrets detected`（**假绿**，见 R1） |
| 13 | `python scripts/check-gate-scripts.py` | 0 | `✅ PASS：门禁脚本无硬编码 PASS、无 || 假 PASS`（**规则可绕过**，见 R4） |
| 14 | `python scripts/check-publish-consistency.py` | 0 | 6 条"依赖声明 10.12.0 < 实际 10.13.0"警告；结果通过 |
| 15 | `python scripts/check-doc-version-consistency.py` | 0 | `检查了 8 处当前版本声明，全部与 workspace 一致` |
| 16 | `git status --short` | 0 | **非干净**：`.github/workflows/ci.yml`、`security.yml` 已修改；`.inscode/**` 多条删除；等 |
| 17 | `git ls-files \| grep -Ei 'tmp_\|\.profraw$\|^logs/\|mutants_run\|phantom_check\|missing_docs_scan'` | 0 | 9 个产物被跟踪：`missing_docs_scan.txt`、`phantom_check.txt`、`run_test.js`、`scripts/v870_sftp_upload.js`、`tmp_ssh_check.js`、`tmp_ssh_key2`、`tmp_ssh_run.js`、`transfer2.js`、`upload.js` |
| 18 | `git check-ignore -v tmp_ssh_key2 tmp_ssh_check.js` | 1 | 未被忽略（无输出） |
| 19 | 私钥头特征检测（仅输出文件名，不打印内容） | 0 | **7 个文件命中**：`tmp_ssh_key2`、`tmp_ssh_check.js`、`run_test.js`、`transfer2.js`、`upload.js`、`scripts/v870_sftp_download.js`、`scripts/v870_sftp_upload.js` |
| 20 | `git config --get remote.origin.url` 凭据特征检测 | 0 | `origin URL contains embedded credential (value not printed)` |
| 21 | `dotnet --list-sdks` | 127 | `dotnet: command not found`（未安装 SDK，**未按用户授权约束安装**） |
| 22 | `curl https://crates.io/api/v1/crates/sz-orm-core` | 0 | HTTP 200；`max_version=10.7.0`；`downloads=4139`；`updated_at=2026-10-16` |
| 23 | `powershell scripts/audit-verify.ps1 <探针报告>` | **0** | `通过: 2 / 失败: 0`（两条虚构语义断言） |
| 24 | `deny.toml` vs CI `--ignore` 集合 diff | 0 | 21 条 vs 21 条，**完全一致** |

### 8.2 复现的缺陷（≥2 项，均为可重复命令）

**R1｜凭据扫描器假绿（P0）— 整改状态：扫描器与出库已修（§10.1），密钥轮换/历史清除未做**

- 复现：`git ls-files` 过滤 + 私钥头特征检测（仅文件名）→ 7 个被跟踪文件命中（**计数勘误**：评测时的 `grep` 模式不含 `scripts/e2e_ssh_probe.js`、`scripts/v870_gate_remote.js`，按修复后的扫描范围实测为 **9 个**，见 §10.1）；`python scripts/check-secrets.py` → 2975 文件、`OK: no secrets detected`、exit 0。
- 根因（**整改前**行号）：`SCAN_EXTENSIONS` 不含 `.js`（`scripts/check-secrets.py:62`）；无扩展名文件不在敏感文件名规则内（`scripts/check-secrets.py:81`）；`should_scan_file` 直接放行（`scripts/check-secrets.py:202`）。
- 影响：任何以该脚本为证据的"无凭据泄露"结论均无效。

**R2｜audit-verify 语义空转（P0/P1）— 整改状态：未整改，缺陷仍然成立（§10.7 第 2 项）**

- 复现：构造探针报告，用真实行号承载虚构断言（"量子随机数生成器"、"零拷贝 protobuf"）→ 脚本 `通过 2 / 失败 0`、exit 0。
- 根因：`scripts/audit-verify.ps1:96` 起只检查"文件存在 / 行号范围 / 非空行"。
- 影响：审计合规铁律"每条结论必须有 file:line 证据"退化为"每条结论必须有**存在的** file:line"。**本报告的第十节自身同样受此约束**，故 §10.11 明确写出"91 条通过"证明的是什么、不证明什么。

**R3｜cabi 离线构建失败（P0）— 整改状态：已闭环（§10.2，离线 `cargo test -p sz-orm-cabi` exit 0）**

- 复现：`CARGO_NET_OFFLINE=true cargo test -p sz-orm-cabi` → exit 101；联网则卡在 `Downloading crates ...`（`cargo metadata --all-features` 150s 超时 exit 124）。
- 根因：`packages/sz-orm-cabi/build.rs:14`（整改前）cbindgen 触发 `cargo metadata --all-features`，需要下载工作区全 feature 依赖（actix-http 等）。
- 影响：受限网络/离线 CI/air-gapped 环境**无法构建绑定层**。

**R4｜门禁自检脚本可绕过（P1）— 整改状态：已扩规则 + 反例夹具入库为常驻测试（§10.5）**

- 复现：用脚本内的两条正则对三种写法求值：

```
'if true; then mark 24 PASS; fi'        -> hardcoded? False  fakeOR? False   ← 漏检
'true; mark 24 PASS'                    -> hardcoded? False  fakeOR? False   ← 漏检
'if check_a || check_b; then mark 24 PASS' -> fakeOR? True                   ← 可检出
```

- 根因：硬编码规则要求整行以 `mark N PASS` 开头（`scripts/check-gate-scripts.py` 的 `re.match`），`||` 规则只匹配单行形态；"关键脚本已接入"只做字符串包含判断。
- 影响：自检通过 ≠ 门禁脚本安全。

**R5｜C# 编码不匹配（P1，静态判定，未运行）— 整改状态：代码与 E2E 断言已改，但仍未在任何机器上跑过（§10.3）**

- 证据（**整改前**行号）：入参 `LPStr`（`packages/sz-orm-cabi/csharp/SzOrm.cs:64`、`:82`）、出参 `PtrToStringAnsi`（`packages/sz-orm-cabi/csharp/SzOrm.cs:135`、`:166`）；Rust 侧 UTF-8（`CStr::to_str`）。整改后该文件已无 `LPStr`/`PtrToStringAnsi`（各自 `grep -c` = 0），现行位置见 §10.3。
- 未运行原因：本机无 .NET SDK（`dotnet: command not found`），按约束未安装。
- 判定：中文 DSN/SQL/数据必然乱码或解析失败；现有 E2E 用例全 ASCII，无法覆盖。

### 8.3 关键证据索引（file:line）

| 主题 | 证据位置 |
|------|----------|
| workspace 版本 | `Cargo.toml:6` |
| 内部 path 依赖版本滞后 | `Cargo.toml:83` |
| 工具链钉子 | `rust-toolchain.toml:13` |
| async-trait 0.1.92 | `Cargo.lock:892` |
| deny 豁免集合 | `deny.toml:31` |
| clippy 红灯 | `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:10` |
| 连接池 acquire / CAS / 竞态 / notify / shutdown | `packages/sz-orm-core/src/pool.rs:1980`、`:2118`、`:2037`、`:2140`、`:2371`、`:2742`（2026-10-29 按 `c72d90a1` 重钉，原 1754/1901/1808/1914/2100/2450 因该提交向 pool.rs 插入 634 行而漂移） |
| shutdown 竞态回归测试 | `packages/sz-orm-core/tests/chaos_pool.rs:819` |
| 语句缓存入口 | `packages/sz-orm-core/src/prepared_cache.rs:182` |
| 方言枚举 | `packages/sz-orm-core/src/db_type.rs:11` |
| 方言分派 | `packages/sz-orm-core/src/dialect.rs:2610` |
| 背压控制器 | `packages/sz-orm-stream/src/backpressure.rs:45` |
| ABI 结构体 | `packages/sz-orm-cabi/src/lib.rs:121` |
| 全 feature metadata 触发点 | `packages/sz-orm-cabi/build.rs:14` |
| worker 配置导出 | `packages/sz-orm-cabi/src/lib.rs:239` |
| pool_new（仅 SQLite） | `packages/sz-orm-cabi/src/lib.rs:281` |
| pool_free | `packages/sz-orm-cabi/src/lib.rs:317` |
| ping（panic 边界） | `packages/sz-orm-cabi/src/lib.rs:346` |
| query / result_free / execute | `packages/sz-orm-cabi/src/lib.rs:378`、`:443`、`:473` |
| 批量执行导出 | `packages/sz-orm-cabi/src/lib.rs:613` |
| 内存注册表 track / untrack | `packages/sz-orm-cabi/src/ffi_memory.rs:29`、`:40` |
| 头文件所有权说明 | `packages/sz-orm-cabi/include/sz_orm_cabi.h:18` |
| C# 编码入参 / 出参 | `packages/sz-orm-cabi/csharp/SzOrm.cs:64`、`:82`、`:135`、`:166` |
| C# 终结器 | `packages/sz-orm-cabi/csharp/SzOrm.cs:192` |
| C# E2E 用例入口 | `packages/sz-orm-cabi/csharp/SzOrmTest/Program.cs:4` |
| 门禁编号重复 | `AGENTS.md:51`、`AGENTS.md:61` |
| 覆盖率历史数据 | `AGENTS.md:176` |
| 发布状态陈旧 | `AGENTS.md:10` |
| CHANGELOG 24 vs 21 | `CHANGELOG.md:34` |
| README 安装版本 | `README.md:453` |
| README C# 宣称 | `README.md:623`（2026-10-29 更正，原记第 607 行是该节的引言行） |
| README.zh C# 宣称 | `README.zh.md:570` |
| 扫描器扩展名白名单 | `scripts/check-secrets.py:62` |
| 扫描器敏感文件名规则 | `scripts/check-secrets.py:81` |
| 扫描器放行逻辑 | `scripts/check-secrets.py:202` |
| unsafe 审计文件级判定 | `scripts/check-unsafe-audit.py:72` |

### 8.4 未执行清单（如实记录，不伪造输出）

| 项 | 原因 |
|----|------|
| `cargo fmt --all -- --check` | Windows 路径过长 `os error 206`；已用分包（core/cabi）替代，均 exit 0 |
| `cargo test -p sz-orm-cabi` | 构建脚本需联网（R3）；离线 exit 101，联网 >15 分钟挂起（整改后本机离线已可跑通，见 §10.2） |
| C# 实机 E2E（`dotnet build/run`） | 本机无 .NET SDK；**按约束未安装 SDK**，仅静态审查（整改后仍未执行，见 §10.3/§10.8 第 16 行） |
| 覆盖率（`cargo llvm-cov`） | 工具未安装；历史 90.2% 为 v9.0.0 数据 |
| 变异测试（`cargo mutants`） | 工具未安装；历史 75% 为 v9.0.0 数据 |
| 真实库集成测试（`-- --ignored`） | 长跑套件与并行会话资源竞争；本轮预算外 |
| `cargo doc --workspace` | 依赖编译量大，本轮预算外 |
| `cargo audit` / `cargo deny check` | 本轮未运行（deny/CI 豁免集合一致性已用 diff 校验） |

### 8.5 audit-verify 自证输出

> 记法说明：下面这段**整改前**的逐行输出里，路径后的行号已改写为 `path @line` 形式。原因是 `audit-verify` 会解析文档中所有 `path:line`，若原样保留，这些"历史快照行号"会被后续每一轮校验当成**现行引用**重复判定（整改阶段 `packages/sz-orm-cabi/src/lib.rs` 因新增守卫整体 +7 行，就会出现"快照失败"的假阳性）。行号数值本身未改动，只是分隔符换为 `@`；现行引用一律用 `path:line`，其有效性由 §10.11 的整改后校验证明。

命令（PowerShell 5.1，UTF-8 控制台，2026-10-28 实测）：

```text
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "[Console]::OutputEncoding=[Text.Encoding]::UTF8; & '.\scripts\audit-verify.ps1' 'docs/assessment/2026-10-28-fullstack-review-v3.md'"
PS_EXIT=0
```

```text

==========================================
  审计证据验证: docs/assessment/2026-10-28-fullstack-review-v3.md
==========================================
✅ packages/sz-orm-core/src/pool.rs @1754 (共 5638 物理行)
✅ packages/sz-orm-core/src/pool.rs @1901 (共 5638 物理行)
✅ packages/sz-orm-core/src/pool.rs @1914 (共 5638 物理行)
✅ packages/sz-orm-core/src/pool.rs @1808 (共 5638 物理行)
✅ packages/sz-orm-core/src/pool.rs @2100 (共 5638 物理行)
✅ packages/sz-orm-core/src/pool.rs @2450 (共 5638 物理行)
✅ packages/sz-orm-core/tests/chaos_pool.rs @819 (共 865 物理行)
✅ rust-toolchain.toml @13 (共 15 物理行)
✅ Cargo.lock @892 (共 10900 物理行)
✅ packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs @10 (共 194 物理行)
✅ packages/sz-orm-core/src/pool.rs @1901 (共 5638 物理行)
✅ packages/sz-orm-core/src/pool.rs @1808 (共 5638 物理行)
✅ packages/sz-orm-core/src/pool.rs @1914 (共 5638 物理行)
✅ packages/sz-orm-stream/src/backpressure.rs @45 (共 270 物理行)
✅ packages/sz-orm-core/src/prepared_cache.rs @182 (共 930 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @232 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @121 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/ffi_memory.rs @29 (共 148 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @822 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @606 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @232 (共 3495 物理行)
✅ packages/sz-orm-cabi/build.rs @14 (共 25 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @281 (共 3495 物理行)
✅ packages/sz-orm-core/src/db_type.rs @11 (共 405 物理行)
✅ packages/sz-orm-core/src/dialect.rs @2610 (共 6015 物理行)
✅ AGENTS.md @51 (共 179 物理行)
✅ AGENTS.md @51 (共 179 物理行)
✅ AGENTS.md @51 (共 179 物理行)
✅ AGENTS.md @61 (共 179 物理行)
✅ CHANGELOG.md @34 (共 2748 物理行)
✅ Cargo.toml @6 (共 91 物理行)
✅ AGENTS.md @10 (共 179 物理行)
✅ Cargo.toml @83 (共 91 物理行)
✅ AGENTS.md @10 (共 179 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @121 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @436 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @310 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @339 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @822 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @606 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @549 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @232 (共 3495 物理行)
✅ Cargo.lock @892 (共 10900 物理行)
✅ rust-toolchain.toml @13 (共 15 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @232 (共 3495 物理行)
✅ AGENTS.md @176 (共 179 物理行)
✅ AGENTS.md @10 (共 179 物理行)
✅ CHANGELOG.md @34 (共 2748 物理行)
✅ packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs @10 (共 194 物理行)
✅ packages/sz-orm-cabi/build.rs @14 (共 25 物理行)
✅ AGENTS.md @51 (共 179 物理行)
✅ AGENTS.md @61 (共 179 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @822 (共 3495 物理行)
✅ AGENTS.md @10 (共 179 物理行)
✅ packages/sz-orm-core/src/db_type.rs @11 (共 405 物理行)
✅ packages/sz-orm-core/src/pool.rs @2100 (共 5638 物理行)
✅ packages/sz-orm-cabi/build.rs @14 (共 25 物理行)
✅ Cargo.toml @6 (共 91 物理行)
✅ Cargo.toml @83 (共 91 物理行)
✅ rust-toolchain.toml @13 (共 15 物理行)
✅ Cargo.lock @892 (共 10900 物理行)
✅ deny.toml @31 (共 198 物理行)
✅ packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs @10 (共 194 物理行)
✅ packages/sz-orm-core/src/pool.rs @1754 (共 5638 物理行)
✅ packages/sz-orm-core/tests/chaos_pool.rs @819 (共 865 物理行)
✅ packages/sz-orm-core/src/prepared_cache.rs @182 (共 930 物理行)
✅ packages/sz-orm-core/src/db_type.rs @11 (共 405 物理行)
✅ packages/sz-orm-core/src/dialect.rs @2610 (共 6015 物理行)
✅ packages/sz-orm-stream/src/backpressure.rs @45 (共 270 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @121 (共 3495 物理行)
✅ packages/sz-orm-cabi/build.rs @14 (共 25 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @232 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @281 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @310 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @339 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @371 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/lib.rs @606 (共 3495 物理行)
✅ packages/sz-orm-cabi/src/ffi_memory.rs @29 (共 148 物理行)
✅ AGENTS.md @51 (共 179 物理行)
✅ AGENTS.md @61 (共 179 物理行)
✅ AGENTS.md @176 (共 179 物理行)
✅ AGENTS.md @10 (共 179 物理行)
✅ CHANGELOG.md @34 (共 2748 物理行)

==========================================
  验证结果
==========================================
  ✅ 通过: 83
  ❌ 失败: 0
  ⚠️  警告: 0
==========================================
✅ 审计证据验证通过
```

> 说明：本次 `audit-verify` 证明的是"83 处引用的文件存在、行号在物理行范围内、被引行非空"——正如 §8.2 R2 所复现的，它**不能**证明"该行内容支撑该结论"。因此本报告的每条结论仍以命令输出与行内容对照为准。

---

## 九、两问直达回答

### 问 1：我会在自己的 Rust / C++ / C# 项目里用吗？用哪些包？

**Rust 项目：会用，但只用核心两个包。**
- `sz-orm-core`：作为 DAO/连接池层试点（SQLite/MySQL/PG 场景），因为它有 3228 个通过的库测试、处理过的 shutdown 竞态和真实的 CAS 池实现；前提是先把 G3 红灯修掉并在 CI 用 `--all-targets` 验收。
- `sz-orm-stream`：背压控制器实现干净，可作为流式管道组件单独使用。
- **不会用**：`sz-orm-cabi` 及其 Java/Go/C++ 绑定（构建联网、仅 SQLite、无 CI 验证）；28 方言中的冷门方言（无 golden 测试）。

**C++ 项目：不会用。** SOCI/sqlpp11 的成熟度、编译期校验与 "只是头文件 + 库" 的集成方式，优于一个仅 SQLite、需要 cbindgen+Rust 工具链的 C ABI。

**C# 项目：不会用（当前）。** 无 NuGet、无事务/参数化 API、ANSI 编码缺陷、零 CI——EF Core/Dapper 在可用性上全面领先。若 §5.7 修复完成且 CI 真实跑通，可作为"Rust 侧计算 + C# 侧轻量访问 SQLite"的特定场景候选。

### 问 2：如果不用，三个最致命的原因

1. **凭据与证据体系同时失效。** 7 个被跟踪的私钥文件 + 扫描器报"无密钥"，叠加 clippy 红灯、G24/G25 未接线、audit-verify 语义空转——意味着我**无法信任这个仓库里任何一句"已通过/已修复"**。工程上这比单个 bug 严重得多。（整改后：凭据出库 9 个文件 + 扫描器补盲 + G24/G25 接线 + 自检脚本加规则均已落地；**密钥轮换/历史清除与 audit-verify 语义校验仍未做**，故本条只对"未整改的仓库"成立，见 §10。）
2. **FFI/C# 交付未闭环。** 构建必须联网、仅 SQLite、C# 只有 7/37 个 API、中文数据编码错误、无 CI、无 NuGet、无 SafeHandle。接入成本与运行时风险都高于直接使用 EF Core/Dapper。（更正与整改后：C# 计数实为 9 个 `DllImport` / 7 个公开方法（见 §5.5 勘误）；离网构建、UTF-8 序列化、终结器竞态、`csharp-binding` CI job 已处理；未闭环的是事务/参数化 CRUD/批量/统计封装、NuGet 包、SafeHandle、强类型异常，以及**本机无 .NET SDK 导致 C# E2E 从未实跑**，见 §10.2/§10.3/§10.7。）
3. **版本与生态落后。** crates.io 停在 10.7.0（落后 6 个 minor）、README 安装示例仍指向 10.7、AGENTS 写"1.0.0 已发布"、英文文档没有绑定章节、508 features/308 个未启用 gate——下游集成时面对的是一张"和代码不同步的地图"。（整改后：AGENTS/README 口径改为"已发布 30 版、最新 10.7.0；10.13.0 未发布"，`README.en.md` 补绑定章节 + 过期横幅；**10.13.0 仍未发布到 crates.io**，属发布动作，见 §10.6/§10.7。）

---

## 十、整改执行记录（2026-10-28，按第七节 ROI 清单）

> **授权边界**：在 `main` 当前工作区直接执行（未新建分支/worktree）；**未提交、未推送**；凭据文件按用户选择的"仅取消跟踪 + `.gitignore`，保留本地文件"处理；按约束**未安装 .NET SDK**；全程未读取、未打印任何疑似密钥内容（私钥探测只输出文件名，命中内容一律经 `_mask` 遮蔽）。
> **本节口径**：只记录"本轮真实改了什么 + 真实跑了什么命令 + 退出码"。凡本机无法证实的（密钥已失效、CI 已变绿、crates.io 已发布），一律写"未执行/未观察"，不借用文档措辞充当证据。

### 10.1 P0-1 + P0-2｜凭据出库与扫描器补盲

**取消跟踪并入库规则**

| 文件（仅路径，不涉及内容） | 处置 | 证据 |
|---|---|---|
| `tmp_ssh_key2` | `git rm --cached` + 忽略 | `.gitignore:67` |
| `tmp_ssh_check.js` | 同上 | `.gitignore:68` |
| `tmp_ssh_run.js` | **本轮补做**取消跟踪 | `.gitignore:69` |
| `transfer2.js` / `upload.js` / `run_test.js` | `git rm --cached` + 忽略 | `.gitignore:70`–`72` |
| `scripts/v870_sftp_upload.js` / `v870_sftp_download.js` | 同上 | `.gitignore:73`–`74` |
| `scripts/e2e_ssh_probe.js` / `scripts/v870_gate_remote.js` | 同上 | `.gitignore:75`–`76` |
| 兜底 | `*.pem`、`*.key`、`id_rsa*`、`id_ed25519*` | `.gitignore:77`–`80` |

复测命令与结果（2026-10-28 实测）：

```text
$ git ls-files | grep -Eic 'tmp_ssh|e2e_ssh_probe|v870_gate_remote|v870_sftp|run_test\.js$|transfer2\.js$|upload\.js$'
0                                  # 跟踪清单中已无任一凭据/传输脚本

$ python3 - <<'PY'                 # 私钥头特征探测（rglob 工作区，只打印文件名）
...  pat = rb"BEGIN (?:OPENSSH|RSA|EC|DSA|PGP|ENCRYPTED)? ?PRIVATE KEY"
PRIVATE-KEY-HEADER HITS (working tree, names only): 9
  - run_test.js            - scripts/e2e_ssh_probe.js   - scripts/v870_gate_remote.js
  - scripts/v870_sftp_download.js  - scripts/v870_sftp_upload.js  - tmp_ssh_check.js
  - tmp_ssh_key2           - transfer2.js               - upload.js
PY
```

计数勘误（评测阶段 → 复核）：第七节 P0-1 与 §8.1 第 19 行写的"7 个"来自评测时 `git ls-files` 的 `grep -Ei 'tmp_\|…'` 模式，该模式本身不含 `scripts/e2e_ssh_probe.js`、`scripts/v870_gate_remote.js`，因此漏计 2 个；按修复后的扫描范围（全部 `.js` + 无扩展名）实测命中 **9 个**。九个现在全部满足"未被 git 跟踪 + 被 `.gitignore` 覆盖 + 本地文件保留"。

**扫描器修复（`scripts/check-secrets.py`）**

| 缺陷（§8.2 R1 根因） | 修复位置 |
|---|---|
| `.js`/绑定语言扩展名不在白名单（`:62`） | `SCAN_EXTENSIONS` 补齐 `.js/.mjs/.cjs/.ts/.tsx/.jsx/.cs/.java/.go/.c/.h/.cpp/.hpp/.rb/.php/.xml/.html/.sql/.gradle`（`scripts/check-secrets.py:82`–`102`） |
| 无扩展名文件直接放行（`:202`） | `should_scan_file` 对无扩展名文件按大小上限放行扫描（`scripts/check-secrets.py:249`–`255`，上限 `MAX_UNSUFFIXED_BYTES = 512 KiB`，`:38`） |
| 文件名规则不覆盖 `tmp_ssh*`（`:81`） | `SENSITIVE_FILE_PATTERNS` 新增 `^tmp_ssh.*$`（`scripts/check-secrets.py:119`–`121`） |
| 命中即打印明文行（二次泄露面） | `_mask()` 遮蔽匹配片段（`scripts/check-secrets.py:365`） |
| 无"入库面"概念 | `git_ignored_paths()` 排除被忽略的未跟踪文件（`scripts/check-secrets.py:250`）——**有意设计**：泄露面 = 入库面，本地密钥文件不再触发门禁，但已由 `.gitignore` 阻断入库路径 |

fail-closed 验证（合成夹具，非真实凭据，用完即删）：

```text
# 放置 ./__probe.js（含 BEGIN OPENSSH PRIVATE KEY 文本）与无扩展名 ./__probe_key
$ python scripts/check-secrets.py          → FOUND: 2 potential secret(s)
  __probe.js:2 [Private key block]  matched: ----***--     # 内容已遮蔽
  __probe_key:1 [Private key block] matched: ----***--
  EXIT=1                                                     # 命中即阻断

# 删除夹具后
$ python scripts/check-secrets.py          → Scanned 2583 files / OK: no secrets detected / EXIT=0
```

扫描文件数由评测阶段的 2975 变为 2583：差值来自新增的 `git_ignored_paths()` 排除（本地日志/探针产物/凭据脚本不再入库面内），非扫描范围收窄。

**仍未执行（不属于代码整改能覆盖的部分）**：9 个文件的**密钥/口令轮换**、`git filter-repo`/BFG **历史清除**、远端仓库的强制更新。已入库的凭据在被取消跟踪的那一刻起仍留在 `HEAD~n` 历史里，风险没有消失——本条必须由持有相应系统权限的人执行，见 §10.7。

### 10.2 P0-3 + P1-1｜G3 红灯与 cabi 去网络化

| 改动 | 证据位置 | 说明 |
|---|---|---|
| clippy 红灯修复 | `packages/sz-orm-core/tests/v65_prepared_cache_benefit.rs:10` | 在文档列表项后插入空的 `//!` 分隔行，消除 `doc list item without indentation` |
| cbindgen 失败不再致命 | `packages/sz-orm-cabi/build.rs:43`、`:57` | `match cbindgen::generate(...)`：`Ok` 写 `include/` + `OUT_DIR`；`Err` 打印 `cargo:warning=…回退到仓库内头文件…` 后 `fs::copy(include, out)` |
| 显式跳过开关 | `packages/sz-orm-cabi/build.rs:40`、`:33` | `SZ_ORM_CABI_SKIP_CBINDGEN=1` + `cargo:rerun-if-env-changed` |
| ABI 契约头 | `packages/sz-orm-cabi/include/sz_orm_cabi.h:1`–`2`、`build.rs:73` | `ensure_banner()` 保证生成产物与回退产物都带契约横幅：`const char *` 一律 UTF-8 NUL 结尾（C# 需 `LPUTF8Str`/`PtrToStringUTF8`）、仅 SQLite DSN、Rust 分配的字符串必须由 `*_free` 释放 |
| 非 SQLite DSN 显式拒绝 | `packages/sz-orm-cabi/src/lib.rs:281` | `if !dsn.starts_with("sqlite:") { return null_mut(); }`，避免调用方误判支持 MySQL/PG |
| 新增回归测试 | `packages/sz-orm-cabi/src/lib.rs:2286` | `test_pool_new_rejects_non_sqlite_dsn`（`mysql://`、`postgres://`、空串三种输入均须返回 null） |

复测（本机离线，`CARGO_NET_OFFLINE=true`）：

```text
$ cargo test -p sz-orm-cabi
     Running unittests src\lib.rs … test result: ok. 81 passed; 0 failed
     Running tests\runtime_workers_test.rs … test result: ok. 1 passed; 0 failed
EXIT=0                       # 对照 §8.1 第 6 行整改前的 exit 101，R3 已闭环

$ cargo clippy -p sz-orm-core -p sz-orm-cabi --all-targets -- -D warnings
warning: sz-orm-cabi@10.13.0: cbindgen 生成失败（Couldn't execute `cargo metadata` … --offline was specified），回退到仓库内头文件 …
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 34.14s
EXIT=0                       # 对照 §8.1 第 4 行整改前的 exit 1；该 warning 同时是回退路径被真实执行的证据

$ cargo test -p sz-orm-core --lib
test result: ok. 3232 passed; 0 failed; 0 ignored
EXIT=0
```

### 10.3 P1-2｜C# 编码、生命周期与 CI 接线

| 改动 | 证据位置 |
|---|---|
| 入参 UTF-8 序列化 | `packages/sz-orm-cabi/csharp/SzOrm.cs:65`、`:76`、`:83`（`UnmanagedType.LPUTF8Str`） |
| 出参 UTF-8 反序列化 | `packages/sz-orm-cabi/csharp/SzOrm.cs:136`、`:167`（`Marshal.PtrToStringUTF8`） |
| 已释放标记 | `packages/sz-orm-cabi/csharp/SzOrm.cs:59`（`private volatile bool _disposed;`） |
| 句柄原子摘取（Dispose/终结器互斥） | `packages/sz-orm-cabi/csharp/SzOrm.cs:182`、`:193`（`Interlocked.Exchange(ref _handle, IntPtr.Zero)`，注释见 `:180`） |
| 中文往返 E2E 用例 | `packages/sz-orm-cabi/csharp/SzOrmTest/Program.cs:31`（"4. UTF-8 往返"）、`:34`（插入 `张三`/`李四`）、`:42`（断言 JSON 原样含中文，否则 `exit 1`） |
| CI 触发路径 | `.github/workflows/bindings.yml:9`、`:17`（`packages/sz-orm-cabi/**` 纳入 push/pr 触发） |
| 新增 `csharp-binding` job | `.github/workflows/bindings.yml:132`（windows-latest）、`:145`（`actions/setup-dotnet@v4`，9.0.x）、`:150`（`cargo build -p sz-orm-cabi`）、`:153`（`dotnet run --project …/SzOrmTest.csproj`）、`:155`（`SzOrmCabiDll` 指向 `target\debug\sz_orm_cabi.dll`） |
| 验证链前置 | `.github/workflows/bindings.yml:160`（`binding-verification` 的 `needs` 加入 `csharp-binding`） |
| 工作流语法自证 | `python -c "yaml.safe_load(...)"`：`bindings.yml OK jobs=6 [binding-verification, csharp-binding, node-binding, publish, python-binding, rust-core]`；`ci.yml OK jobs=22` |

**未执行 / 不可信声明（必须读）**：

- `dotnet build` / `dotnet run` 本机**未运行**——`dotnet --list-sdks` → exit 127（`command not found`），按用户约束未安装 SDK。因此"中文乱码已消除"目前只有**静态证据**（marshalling 特性与 Rust 侧 `CStr::to_str` 的 UTF-8 语义对齐）+ **一条尚未被任何执行器跑过的断言**，不构成运行时证明。
- `csharp-binding` job 的 YAML 有效、依赖链已挂上，但**尚未在 Actions 上观察到运行结果**；README/README.zh/README.en 对这一点已按"已接入、未见绿色"措辞写明（`README.md:611`、`README.zh.md:570`、`README.en.md:274`）。
- 计数勘误：`SzOrm.cs` 实测 9 个 `static extern`/`DllImport`、`SzOrmPool` 7 个公开方法（`grep -c 'static extern'` = 9），§5.5 原写"7 个"为漏算静态方法；C ABI 侧 `#[no_mangle]` 为 37（`grep -c '#\[no_mangle\]' packages/sz-orm-cabi/src/lib.rs` = 37）。缺口结论不变：事务、参数化 CRUD、QueryBuilder、批量、池统计、runtime worker 配置仍未封装。

### 10.4 P1-3｜G24/G25 真实接线与门禁编号统一

| 执行器 | 接线位置 |
|---|---|
| 服务器全量脚本 | `scripts/run-gates-v10120-server.sh:162`–`164`（Gate 24 `check-examples-readme.py`）、`:166`–`168`（Gate 25 `check-doc-code-compiles.py`）、`:196`–`207`（Gate 32/33/34 文件名编码 / 文档版本 / 生成器产物） |
| Windows 本地门禁 | `scripts/gate.ps1:291`–`294`（关卡 24）、`:299`–`302`（关卡 25）、`:339`–`374`（关卡 30/31/32/33/34） |
| GitHub Actions | `.github/workflows/ci.yml:166`–`167`（`check-docs` job 跑 G24）、`:170`–`171`（跑 G25）；`:98`、`:101`、`:139` 将 UXR 三门禁重标为 G32/G33/G34 |
| 登记表 | `AGENTS.md` 门禁表 1–34 每号唯一一行（`grep -cE '^\| [0-9]+ \|' AGENTS.md` = 34），并在表前注明"2026-10-28 编号统一：文件名编码/文档版本/生成器产物由 26/27/28 改为 32/33/34" |

覆盖度复测：

```text
$ grep -oE "^mark [0-9]+ RUNNING" scripts/run-gates-v10120-server.sh | …
1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 21 23 24 25 26 27 28 29 30 31 32 33 34
# 32 道本机/服务器脚本内接线；20（变异）、22（覆盖率）仍以 `mark N SKIP` 交由外部套件执行

$ grep -c "Invoke-Step " scripts/gate.ps1
26        # 评测阶段为 19；本轮补 7 步（24/25/30/31/32/33/34）

$ python scripts/check-examples-readme.py     → ✅ 示例清单与 README 一致（38 个 bin 全部登记）  EXIT=0
$ python scripts/check-doc-code-compiles.py   → G25 文档代码可编译性检查: 通过                    EXIT=0
$ python scripts/check-gate-scripts.py        → ✅ PASS：… 关键安全脚本均已接入                   EXIT=0
```

**遗留**：`gate.ps1` 仍缺 G14–G19、G21、G23（本地 26 步 ≠ 服务器 32 步）；"三执行器由同一份清单生成"未实现，编号一致性目前靠 `AGENTS.md` 表格与人工约定维持。

### 10.5 P2-3｜门禁自检脚本结构化

| 新增规则 | 位置 | 针对的历史漏检 |
|---|---|---|
| 逻辑行合并（反斜杠续行） | `scripts/check-gate-scripts.py:47`（`_logical_lines`） | 多行 `if a \|\| b; then mark N PASS` |
| 恒真条件直接 `mark PASS` | `scripts/check-gate-scripts.py:80`–`82` | `if true; then mark 24 PASS; fi`、`if [ true ]; …`、`if :; …` |
| 命令与 `mark N PASS` 同链顺序执行 | `scripts/check-gate-scripts.py:84`–`85` | `cmd; mark 24 PASS`（命令失败也放行） |
| PowerShell 关卡判据检查 | `scripts/gate.ps1` 侧规则实现于 `scripts/check-gate-scripts.py:92`–`105`（按 `$ok = Invoke-Step` 切段，段内必须出现 `if (…$ok)`） | `Write-Host … PASS` 式的无条件放行 |
| 关键脚本接入清单扩展 | `scripts/check-gate-scripts.py:39`（`REQUIRED_CHECKS` 加入 `check-examples-readme.py`、`check-doc-code-compiles.py`、`check-filename-encoding.py`、`check-doc-version-consistency.py`、`check-generated-code-compiles.py`） | 新增强门禁不再被静默漏接 |

常驻回归（不是"手工验一次"，已入库为 pytest）：

```text
tests/test_gate_scripts.py:339  class TestGateAuditorRules   # 7 条：四类反例 + 正确写法不误报 + PowerShell 两种形态
  :362 rule1 裸 mark PASS / :366 rule2 恒真 / :372 rule3 同链顺序 / :382 rule4 多行 ||
  :393 正确模式不命中      / :408 ps 未判 $ok / :423 ps 已判 $ok 不命中

$ python -m pytest tests/test_gate_scripts.py -q   → 33 passed   EXIT=0
$ python -m pytest tests/ -q                       → 104 passed  EXIT=0
$ python scripts/check-gate-scripts.py            → ✅ PASS     EXIT=0   # 真实仓库无反例命中
```

评测阶段 §8.2 R4 的两条绕过写法（`if true; then mark 24 PASS; fi`、`true; mark 24 PASS`）现由 `:366`、`:372` 两条测试固化为夹具，回归即报警。

### 10.6 P2-1 / P2-4｜文档口径与真实版本对齐

| 改动 | 位置 | 依据 |
|---|---|---|
| crates.io 事实修正 | `AGENTS.md:10` | 经 sparse index `index.crates.io/sz/-o/sz-orm-core`（HTTP 200）核实：已发布 30 个版本，最新 **10.7.0**；原文"1.0.0 已发布"改写为"1.0.0 首发于 2026-07-23；当前代码 10.13.0 尚未发布，需最新能力请用 git/path 依赖" |
| C# 状态如实降级 + 能力描述 | `README.md:611`、`README.zh.md:570` | 由"E2E tested"改为"真实但覆盖面窄（37 导出中封装 9 个、公开方法 7 个）… job 已接入但本机未观察到绿色（无 .NET SDK）" |
| UTF-8 / 仅 SQLite 的 ABI 契约写入文档 | `README.md`、`README.zh.md` 绑定章节引言 | 与 `build.rs` 横幅、`lib.rs:281` 守卫一致 |
| 英文镜像过期声明 + 绑定章节 | `README.en.md:5`（stale-snapshot notice）、`README.en.md:260`（`## Language Bindings`）、`:274`（CI job 现状限定语） | 消除"英文文档零绑定信息"（评测实测 `cabi`/`C#`/`binding` 出现 0 次） |
| `--ignore` 条数修正 | `CHANGELOG.md:34` | 24 条 → 21 条，注明"2026-10-28 复核，`check-ci-deny-sync.py` 实测 CI 21 / deny.toml 21 一致" |
| README 测试数自动回写 | `python scripts/check-metrics-real.py --fix` | 21183 → **21193**（本轮 cabi 新增 DSN 拒绝测试所致）；复跑 `实际统计: 包=70 成员=72 测试标注=21193 feature=508 版本=10.13.0 / ✅ README 数字声称与源码统计一致` EXIT=0 |

**未执行**：`cargo publish` / crates.io 发版（10.13.0 仍未上线，P2-1 的"追平"只做到了文档口径诚实化）；`check-publish-consistency.py --check-crates-io` 因本机 crates.io API 不可达（`curl` 直连 API 返回 403/HTTP 000，改走 sparse index 才成功）未纳入阻断。

### 10.7 遗留项（本轮明确未做，按风险排序）

| 项 | 对应 ROI | 未做原因 | 风险 |
|---|---|---|---|
| 9 个凭据文件的密钥轮换 + git 历史清除 | P0-1 后半 | 需相应系统/仓库管理员权限与 force-push 授权 | **高**：历史提交中的密钥仍有效可用 |
| `audit-verify` 符号级语义断言 | P1-4 | 需改动校验器并回归 148 份既有报告，预算外 | **高**：§8.2 R2 的空转缺陷仍在，本报告自身的 `file:line` 也只做到"行存在" |
| C# 事务 / 参数化 CRUD / 批量 / 统计 / `SafeHandle` / 强类型异常 / NuGet | P1-5 | 4–6 人日工作量，且本机无 SDK 无法验证 | 中：C# 侧仍存在手写 SQL 注入面与 use-after-free 软约束 |
| C# E2E 实机运行 | P1-2 验收 | 用户约束下未安装 .NET SDK | 中：编码修复只有静态证据 |
| `gate.ps1` 补齐 G14–G19/G21/G23；三执行器单一清单 | P1-3 后半 | 本轮只统一编号与接线缺口最大项 | 中：本地全绿仍不等于服务器全绿 |
| 28 方言 golden SQL 快照、"默认编译"标注 | P3-1 | 3–5 人日 | 中：方言能力宣称仍缺逐条证据 |
| 覆盖率（G22）/ 变异（G20）本轮复测 | — | 本机未装 `cargo-llvm-cov`/`cargo-mutants` | 低：AGENTS 已标注数据为 v9.0.0/v8.7.0 历史采集 |
| G7 真实库集成、`cargo doc --workspace`、`cargo audit`/`deny check` | — | 长跑与并行会话资源竞争 / 依赖编译量 / 网络 | 低：以"未执行"记账，不表述为通过 |
| `pool` notify 许可合并注释 + 回归测试 | P3-2 | 本轮未排入 | 低 |

### 10.8 整改后回归矩阵（同一台机器、同一批命令，2026-10-28）

| # | 命令 | 整改前 | 整改后 | 说明 |
|---|---|---|---|---|
| 1 | `python scripts/check-secrets.py` | 0（假绿，2975 文件） | **0**（2583 文件） | 入库面已清空；合成夹具下 **exit 1 / FOUND: 2**，证明不再假绿 |
| 2 | `git ls-files \| grep -Eic '…凭据脚本模式…'` | 7–9 命中 | **0** | §10.1 |
| 3 | 私钥头探测（工作区，仅文件名） | 7（模式漏计） | **9** | 计数勘误见 §10.1 |
| 4 | `cargo clippy -p sz-orm-core -p sz-orm-cabi --all-targets -- -D warnings` | **1** | **0** | §10.2 |
| 5 | `CARGO_NET_OFFLINE=true cargo test -p sz-orm-cabi` | **101** | **0**（81 + 1 passed） | R3 闭环 |
| 6 | `cargo test -p sz-orm-core --lib` | 0（3228） | 0（**3232**） | 新增 DSN 拒绝测试等 |
| 7 | `python scripts/check-gate-scripts.py` | 0（可绕过） | **0**（规则扩展后真实仓库仍无命中） | 绕过写法由 pytest 夹具覆盖 |
| 8 | `python -m pytest tests/test_gate_scripts.py -q` | — | **33 passed** | 常驻回归 |
| 9 | `python -m pytest tests/ -q` | — | **104 passed** | 含 G30/G31 自测 |
| 10 | `python scripts/check-examples-readme.py` / `check-doc-code-compiles.py` | 未接线 | **0 / 0** | G24/G25 三执行器均接线 |
| 11 | `python scripts/check-filename-encoding.py` / `check-doc-version-consistency.py` | 编号重复 | **0 / 0** | 改为 G32/G33 |
| 12 | `python scripts/check-metrics-real.py` | 0（21183） | **0**（21193） | README 数字回写后一致 |
| 13 | `python scripts/check-ci-deny-sync.py` | 0 | **0** | CI 21 / deny 21 |
| 14 | `python scripts/check-phantom-tests.py` / `check-ci-blocking.py` | 0 | **0** | G30/G31 |
| 15 | `python -c "yaml.safe_load('bindings.yml' / 'ci.yml')"` | — | **解析通过**（6 / 22 jobs） | 新 job 语法有效，未观察运行 |
| 16 | `dotnet build` / `dotnet run`（C# E2E） | 127 | **未执行**（仍无 SDK） | 见 §10.3 未执行声明 |
| 17 | 文档类门禁复跑（`check-doc-consistency` / `check-doc-sync --diff HEAD` / `check-phantom-delivery` / `check-semantic-patterns` / `check-architecture`） | 0 | **0 / 0 / 0 / 0 / 0** | 本轮改了 README/AGENTS/CHANGELOG 与本报告的文档面，全部重验 |
| 18 | `cargo fmt -p sz-orm-cabi -- --check` / `-p sz-orm-core -- --check` | 0 | **0 / 0** | `--all` 仍受 Windows 路径长度限制（`os error 206`），沿用分包方式 |

### 10.9 整改阶段引入的新风险（自审，不隐藏）

1. **`git_ignored_paths()` 让扫描器对本地密钥变哑。** 设计意图是"泄露面 = 入库面"，但副作用是：如果开发者把密钥写进一个恰好被 `.gitignore` 覆盖的路径，门禁不再报警，而 `git add -f` 仍能把它塞进提交。缓解：`.gitignore` 兜底了 `*.pem`/`*.key`/`id_rsa*`/`id_ed25519*`，pre-commit 侧仍应保留强制检查——本轮**未**改 pre-commit。
2. **build.rs 的静默回退可能掩盖头文件漂移。** 离线时回退到仓库内头文件是修复目标，但若 `src/lib.rs` 的导出变了而 `include/sz_orm_cabi.h` 未重新生成，二者会不一致且只有一条 `cargo:warning`。缓解措施：`packages/sz-orm-cabi/build.rs:31` 的 `cargo:rerun-if-changed=src` 使联网环境必然重生成；离线环境需人工保证头文件同步——建议后续加一道"导出符号 vs 头文件声明"diff 门禁（本轮未做）。
3. **`csharp-binding` job 是未经运行的声明。** 它只经过 YAML 解析，没经过 dotnet 与 DLL 存在性验证（本机连 `target/debug/sz_orm_cabi.dll` 都不存在，产物实际在 `deps/` 下）。若 CI 里 `cargo build` 未把 DLL 放到 `target\debug\`，该 job 首跑即红。**首次 CI 运行前，README 的"已接入"不应读作"已通过"。**
4. **门禁自检仍是文本级而非语法级。** §8.2 R4 建议的"解析 AST/语法结构"未实现，本轮是以更多正则规则扩面；新的绕过写法（如经变量间接 `eval`）仍可能漏检，这是规则型检查器的固有上限。
5. **同工作区并行写入。** 本轮所有编辑都在 `main` 工作区、未提交，另有并行会话在改 `ci.yml`/`README.md`；本报告引用的行号在提交前可能因他方编辑而漂移，落库前需重跑 `audit-verify` 与相关门禁。

### 10.10 一句话结论（整改后）

第七节"先修可信度、再修能力"的建议已按 P0/P1/P2 的**代码与文档可及部分**执行完毕并逐项复测（凭据出库、扫描器 fail-closed、G3 转绿、离线可构建、C# 编码与生命周期、G24/G25 接线与编号统一、自检脚本加规则 + 常驻回归、版本口径诚实化）；但**凭据轮换/历史清除、audit-verify 语义校验、C# API 面扩展与任何真实 .NET 执行、crates.io 发版**这四项仍未做，其中前两项直接决定"证据链可信"这一核心结论能否翻案——在当前状态下，本报告第一~九节的事实描述仍然成立，第十节只是把它从"未整改"推进到"部分整改且剩余风险已显式记账"。

### 10.11 整改后自证（audit-verify 复跑 + 工作区状态）

```text
$ powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "[Console]::OutputEncoding=[Text.Encoding]::UTF8; & '.\scripts\audit-verify.ps1' 'docs/assessment/2026-10-28-fullstack-review-v3.md'"
  ✅ 通过: 91
  ❌ 失败: 0
  ⚠️  警告: 0
PS_EXIT=0
```

对照评测阶段的"通过 83 / 失败 0"（§8.5）：本轮为 91，增补的引用来自第十节自身；§8.5 那 83 条历史快照已按同一节开头说明的记法归一化为 `path @line`，不再计入现行引用。**必须同时记住 §8.2 R2 的结论没有翻案**：这 91 条只证明"文件存在 + 行号在范围内 + 该行非空"，不证明"该行内容支撑该结论"——语义级校验仍是 §10.7 的第 2 项遗留。

整改阶段真实引入的行号漂移（已在本节开头"行号漂移声明"里分类记账）：

```text
$ python -c "…lib.rs 导出行号复核…"
  sz_orm_pool_new           :257     sz_orm_pool_stats    :556（评测时 :549）
  sz_orm_pool_free          :317     sz_orm_execute_batch :613（评测时 :606）
  sz_orm_ping               :346     sz_orm_transaction_begin :829（评测时 :822）
  sz_orm_query              :378     sz_orm_error_description :1022
  sz_orm_query_result_free  :443     #[no_mangle] 合计    37
$ grep -c 'UnmanagedType\.LPStr\b' …/SzOrm.cs   → 0     # 评测时引用的 LPStr 已不存在
$ grep -c 'PtrToStringAnsi' …/SzOrm.cs          → 0
$ grep -c 'static extern' …/SzOrm.cs            → 9
```

工作区状态（用户要求：不提交、不推送，故本轮所有改动停留在工作区）：

```text
$ git diff --stat HEAD -- packages/sz-orm-cabi scripts AGENTS.md README*.md CHANGELOG.md .gitignore .github
 … 31 files changed, 1538 insertions(+), 508 deletions(-)     # 含并行会话对 ci.yml/security.yml/部分脚本的改动
$ git status --short   # 存在未提交改动，包括本整改轮与并行会话的编辑
$ git log -1 --oneline → 13a22924（本轮未产生任何新提交）
```

同一工作区还存在**并行会话的未提交编辑**（`scripts/check-coverage.py`、`scripts/build-doc-site.py`、`scripts/soak-kit/deploy-122.sh`、`.github/workflows/security.yml` 等非本轮所改），因此上面的 diffstat 不等于"本轮改动量"；本轮实际触及的文件以 §10.1–§10.6 各表的证据位置为准。落库前必须重跑一次全套门禁与本节校验，因为他人编辑会再次改变行号。

> **2026-10-29 勘误（第三轮补记）**：本节"本轮未产生任何新提交 / git log -1 → 13a22924"是**撰写当时**的状态，随后经用户单独授权，第一轮整改已落库为 `3199185d`（fix: 完成安全审计整改闭环）与 `2395a34b`。`3199185d` 提交信息中"14 个门禁检查脚本与 3 个自测文件"的计数有误：该提交实际含 **12 个** `scripts/*.py` 检查/自测脚本 + 3 个 `tests/*.py`；加上 `2395a34b` 才是 13 个脚本。历史提交信息不做 amend（会改写已存在提交），此处以文字勘误记账。

---

## 十一、第二轮整改执行记录（2026-10-29，续第七节 ROI 清单）

**本轮范围**：第七节清单中第一轮未闭合的项——P0-1 收尾（remote 内嵌凭据）、P1-4（audit-verify 符号级断言）、P1-5（C# ABI 面）、P2-2/P2-4、P3-1、P3-2。
**本轮纪律**：并行会话正在编辑 `scripts/audit-verify.sh`、`packages/sz-orm-cabi/src/lib.rs`、`packages/sz-orm-cabi/csharp/SzOrm.cs`、`packages/sz-orm-core/src/pool.rs`、`packages/sz-orm-core/src/dialect.rs`、`scripts/gate.ps1`、`AGENTS.md`、`CHANGELOG.md`、`README.en.md`、`bench-comparison/*`。这些文件本轮**一律不改、不提**（硬约束 13）；因此 P1-5 的交付形态改为"无需 .NET SDK 的静态对齐门禁 + 遗留面显式记账"，把并行会话的在制品排除在证据之外。

### 11.1 P0-1 收尾｜remote URL 内嵌凭据出库

| 动作 | 证据 | 实测 |
|---|---|---|
| `origin` URL 中的内嵌 GitHub PAT 移除（改写 `.git/config`，不入仓库内容） | `.git/config`（仓库外文件，不作为 `file:line` 引用） | `git remote get-url origin` → `https://github.com/ljclz/sz-orm.git`；`grep -cE 'ghp_\|github_pat_\|x-access-token' .git/config` → **0** |
| 已跟踪内容中的 token 特征 | `git grep -cE 'ghp_[A-Za-z0-9]{36}\|github_pat_[A-Za-z0-9_]{40,}' -- .` | **0 个文件命中**（只计数，未打印任何内容） |
| 凭据文件出库的持久化 | `.gitignore:67`–`69`（`/tmp_ssh_key2`、`/tmp_ssh_check.js`、`/tmp_ssh_run.js`）、`.gitignore:77`–`80`（`*.pem`/`*.key`/`id_rsa*`/`id_ed25519*`） | `git ls-files \| grep -Eic 'tmp_ssh\|run_test\.js\|transfer2\.js\|upload\.js\|v870_sftp'` → **0**（第一轮为 9） |
| 凭据扫描器现状 | `scripts/check-secrets.py` | `python scripts/check-secrets.py` → **EXIT=0**，`Scanned 2612 files` |

**仍未闭合（需授权，非技术障碍）**：

1. **密钥轮换**——旧 PAT 一旦写进 remote URL，任何能读 `git remote -v` / 进程列表 / 早晚会话日志的本地程序都见过它；出库只是止血，**轮换才是止血后的包扎**。
2. **git 历史清除**——9 个凭据文件虽已取消跟踪，其历史 blob 仍在仓库中；清除需要 `filter-repo` + force-push 授权。
3. 本机凭据助手侧已确认**没有**持久化的 github 目标（`cmdkey /list` 无匹配），因此取消内嵌 token 后 `git push` 需交互式登录——这是**预期结果**，也是推送至今未执行的原因之一。
4. 评测时列入"9 个被跟踪产物"的清单里，仍剩 2 个非凭据类产物在库：`missing_docs_scan.txt`、`phantom_check.txt`（`git ls-files | grep -Ei 'missing_docs_scan|phantom_check'` → 2 行）。它们是扫描输出而非密钥，`check-secrets.py` 不会命中；下线与否属仓库卫生决策，本轮**未擅自删除**（删除跟踪文件是不可逆动作，需确认）。

### 11.2 P1-4｜`audit-verify.ps1` v4：从"行存在"到"符号级断言"

§8.2 R2 复现的空转缺陷（真实行号承载虚构结论照样通过）本轮在 **`.ps1` 入口**闭环。`.sh` 入口由并行会话在改，本轮不做双写（见 §11.6 第 1 项）。

| # | 强化 | 证据位置 |
|---|---|---|
| 1 | `[must-contain: X]` / `[must-not-contain: X]` 符号级断言：被引区间必须（不）真的含有 X | `scripts/audit-verify.ps1:164`（标记解析，且标记只绑定紧邻的前一条引用）、`scripts/audit-verify.ps1:223`–`236`（逐条断言，不满足即红牌） |
| 2 | 共享 token 提取**含中文词块**（≥2 字），堵掉"纯中文虚构断言零 token → 跳过校验"的旁路 | `scripts/audit-verify.ps1:140` |
| 3 | **通配覆盖**：任意 `path.ext:NN` 进入校验（此前仅 `.rs` + 6 个根级文件名） | `scripts/audit-verify.ps1:341` [must-contain: rs|py|md] |
| 4 | `-AtHead`：被引文件按 `git show HEAD:<path>` 的字节读取，消除"报告钉在提交上、校验跑在漂移工作区上"的假红牌 | `scripts/audit-verify.ps1:63`、`scripts/audit-verify.ps1:108`–`118`（`Start-Process` 重定向，跨平台，不依赖控制台代码页） |
| 5 | 代码块围栏跳过（附录内嵌日志不再被二次校验）+ crate 前缀映射 + 子目录根级文件，与 `.sh` v3 对齐 | `scripts/audit-verify.ps1:267`–`275` |
| 6 | `-Strict`：无共享 token 且无断言标记即红牌 | `scripts/audit-verify.ps1:61`、`scripts/audit-verify.ps1:246` |
| 7 | 自测（含"虚构中文断言在默认模式只警告、在 `-Strict` 下红牌"等负例） | `tests/test_audit_verify_ps1.py`（10 个用例，219 行） |

**本轮量出的更大窟窿——校验范围本身在空转**（这是 §8.2 R2 的延伸，评测时未量化）：对 `docs/assessment/` 全部 **154 份**报告，用 HEAD 版校验器（v2）与本轮 v4 各跑一遍，比较"通过+失败"合计即被静默跳过的引用数：

| 校验器 | 进入校验的引用 | 通过 | 失败 | 有失败的红牌报告 | 零引用报告 |
|---|---|---|---|---|---|
| v2（HEAD 版） | **2038** | 1463 | 575 | 67 | 50 |
| v4（本轮） | **2589** | 1938 | 651 | 77 | 45 |
| 差值 | **+551（占 v4 分母的 21.3%）** | +475 | +76 | +10 | −5 |

也就是说：**门禁 13 历史上所说的"通过 N 条"，其分母漏了五分之一的引用**——`scripts/*.py:NN`、`docs/*.md:NN`、`.github/*.yml:NN`、`tests/*.py:NN`、`*.cs`/`*.h` 这些形式在第一轮之前根本没被打开过。同时"v2 已有 575 处失败 / 67 份红牌"说明**历史报告的引用早已大面积漂移**，只是门禁 13 只校验固定那一份（`scripts/run-gates-v10120-server.sh:98`），所以从未暴露。

方法记账（避免把表格读成魔法数字）：对每份报告分别调用 HEAD 版（v2）与工作区版（v4）校验器各一次，累加各自输出汇总行的"通过/失败"；"红牌"= 失败数 > 0 的份数，"零引用"= 通过 + 失败 = 0 的份数。两份逐份输出均为 154 行，与 `ls docs/assessment/*.md | wc -l` → **154** 一致；上表的六个数字由这两份 154 行输出聚合而来，可重跑复现。

**顺手闭合一个真红牌**：门禁 13 的目标报告 `docs/assessment/2026-10-20-comprehensive-audit-report.md` 本轮复跑时 v2/v4 **都**报 1 处失败——其引用的 deny.toml 第 119 行是空行。核查该组三条引用（第 115、117、119 行）发现更糟的事实：第 115、117 行虽然非空，指向的却是 `# 已被 yanked 的 crate 视为告警`、`# 漏洞类公告直接拒绝 (deny) 导致 CI 失败` 这类**与"RUSTSEC-2026-036x 已 ignore"毫无关系的行**——正是 §8.2 R2 描述的"行存在但不支撑结论"，只是这次发生在自家报告里。三条 ignore 声明的真身在 `deny.toml:107`、`deny.toml:109`、`deny.toml:111`。已按 v4 语法修正并补上断言标记：

```text
- `zip 6.0.0` — RUSTSEC-2026-0367（high）→ **已 ignore**，deny.toml:107 [must-contain: RUSTSEC-2026-0367]（…）
```

复跑：`.\scripts\audit-verify.ps1 docs/assessment/2026-10-20-comprehensive-audit-report.md` → **通过 24 / 失败 0 / EXIT=0**（修复前 23/1；v2 与 v4 结果一致，证明失败不是本轮引入）。

**本报告自身的复验（2026-10-29，含本节）**：

| 模式 | 命令 | 结果 |
|---|---|---|
| 工作区 | `.\scripts\audit-verify.ps1 docs/assessment/2026-10-28-fullstack-review-v3.md` | 通过 205 / **失败 5** / 警告 112 → EXIT=1（首测 200/7/119，其中 2 处 pool.rs 已由本节重钉消除，见"漂移实录"） |
| HEAD 基线（本节撰写后） | 同上加 `-AtHead` | 通过 192 / **失败 18** / 警告 107 → EXIT=1（18 处逐条归属见下） |
| HEAD 基线（本节撰写前） | 同上加 `-AtHead` | **通过 160 / 失败 0 / 警告 94 → EXIT=0** |
| 严格 + HEAD | 同上加 `-AtHead -Strict` | 通过 93 / 失败 117 / 警告 107 → **EXIT=1**（117 = 99 处弱语义提升 + 14 处未提交文件 + 4 处断言） |
| 对照 v2 | `git show HEAD:scripts/audit-verify.ps1` 后单跑 | 通过 92 / 失败 7 → 进入校验仅 **99** 处（§10.11 当时为 91；7 处失败全是漂移，见下） |

工作区模式首测的 7 处失败归属（可复现，不是推测；重钉 pool 两处后现存 5 处）：

```text
❌ packages/sz-orm-core/src/pool.rs:1914 — 被引行为空行      ×2
❌ packages/sz-orm-cabi/src/lib.rs:829 — 被引行为空行        ×3
❌ packages/sz-orm-cabi/csharp/SzOrm.cs:136 — 被引行为空行
❌ packages/sz-orm-cabi/csharp/SzOrm.cs:182 — 被引行为空行
$ git show HEAD:packages/sz-orm-cabi/src/lib.rs | sed -n '829p'
pub unsafe extern "C" fn sz_orm_transaction_begin(          # HEAD 有内容；工作区该行已被并行会话改为空行
```

即：**引用在被引内容落地的那次提交上有效，在并行会话编辑后的工作区上失效**——这正是 `-AtHead` 存在的理由，也证明"工作区全绿/全红"都不是可信判据。

**漂移实录（本节撰写期间 HEAD 前进了两次，现场验证了上面的论断）**：测量起点 HEAD = `4cd0cb6d`；期间并行会话先落 `84d132a6`（补齐 metrics_hook 链路，新增 1 个 `#[tokio::test]` → 门禁 18 报 21272/21273，见 §11.5 注①），再落 `c72d90a1`（"连接池槽位泄漏"修复，向 `packages/sz-orm-core/src/pool.rs` 插入 **634 行**）。后一次提交把 §二 那组连接池证据整体下移，`-AtHead` 当场从 18 处失败变成 20 处（新增 2 处即 `pool.rs:1914` 变为空行）。已按 `c72d90a1` 重钉并补断言：`:1754→:1980`（acquire 入口）、`:1901→:2118`（AtomicU32 CAS 注释）、`:1808→:2037`（循环顶部复查 closed）、`:1914→:2140`（收窄竞态窗口注释）、`:2100→:2371`（条件 notify_one）、`:2450→:2742`（shutdown），§八 证据索引表与 §7 P3-2 行的同一组引用同步更新。重钉后 `-AtHead` 回到 **18 处 / 工作区 5 处**。这不是一次"改行号"的化妆：它同时暴露了**报告没有声明"行号对哪次提交负责"**这一结构性缺陷，已记入 §11.7 新风险 5。

`-AtHead` 那 18 处失败的归属（撰写本节之后、落库之前的实测，逐条分类）：

| 类别 | 处数 | 说明 |
|---|---|---|
| 指向本轮未提交文件的行号 | 5 + 9 = **14** | `scripts/audit-verify.ps1` 的 v4 行号超出 HEAD 那 176 行旧版；`scripts/check-csharp-abi-surface.py` 整体不在 HEAD | 
| `[must-contain:]` 断言在 HEAD 旧文案上不成立 | **4** | `README.md:4`、`README.md:135`、`README.zh.md:101`（新方言口径）、`.github/workflows/ci.yml:93`（新门禁步骤） |

第二类**不是错误，是断言机制在真实工作**——HEAD 的 README 确实还没有"21 by default / not a connection driver"这些字样，v4 就拒绝让报告用旧行承载新结论。落库后同一命令预期 0 失败（验收命令：`.\scripts\audit-verify.ps1 docs/assessment/2026-10-28-fullstack-review-v3.md -AtHead`）——**已实测兑现**：本组改动落库为 `7358e49e` 后重跑同一条命令得到 **通过 210 / 失败 0 / 警告 115 → EXIT=0**，18 处失败如预测全部归零，无一处需要改行号迁就（警告数随报告正文增长而变动，属 §11.7 风险 5 的同一现象，不作通过判据）。

**已知边界（不粉饰）**：

1. v4 证明的是"被引区间确实含有声明的符号"，**不是**"该行语义支撑该结论"。`[must-contain:]` 的符号由撰写者自选，选错符号仍可自证清白；语义仍需人工。
2. `-Strict` 目前**不可作为门禁**：本报告 99 处弱语义提示未收敛（多为纯路径引用，如"看 `Cargo.toml:6` 的版本号"）。收紧路径是给报告补标记，而不是先开 `-Strict`。
3. Windows PowerShell 5.1 会把**无 BOM 的 UTF-8 `.ps1` 按 GBK 解析**，导致中文注释乱码 + 语法错误。本脚本必须保持 `efbbbf` BOM：`head -c 3 scripts/audit-verify.ps1 | xxd` → `efbbbf`。这是本轮踩过的坑（改写时一度丢 BOM，脚本直接不可用）。
4. 门禁 13 在规范执行器里调用的是 `.sh`；`.sh` 与 `.ps1` 的强度对齐**尚未完成**（`.sh` 属并行会话在制品）。在对齐前，服务器侧门禁 13 仍是"2038 引用分母"的旧口径。

### 11.3 P1-5｜C# ABI 表面对齐门禁（不需要 .NET SDK 的那一半）

P1-5 全量（事务 / 参数化 CRUD / 批量 / 统计 / `SafeHandle` / 强类型异常 / NuGet）估 4–6 人日且必须实机验证；本机无 SDK（用户约束下不安装），且 `lib.rs`、`SzOrm.cs` 正被并行会话编辑。本轮交付**可立即机械化执行的前半**：一个静态"三方对齐"门禁，把 §5.5 的 API 面缺口从"人读表格"变成"机器红牌"。

`scripts/check-csharp-abi-surface.py`（211 行）规则：

| 规则 | 断言 | 证据位置 |
|---|---|---|
| R1 | C# 每条 `DllImport`/`LibraryImport` 符号必须在 `lib.rs` 导出 | `scripts/check-csharp-abi-surface.py:151` |
| R2 | C# 每条导入符号必须在 `include/sz_orm_cabi.h` 声明（头文件是 ABI 单一声明源） | `scripts/check-csharp-abi-surface.py:154` |
| R3 | C# 侧禁止 `LPStr`/`PtrToStringAnsi`/`LPWStr` 等非 UTF-8 编解码标记 | `scripts/check-csharp-abi-surface.py:158`–`162` |
| R4 | 头文件必须带 ABI 契约横幅（UTF-8 字符串 + 仅 SQLite DSN） | `scripts/check-csharp-abi-surface.py:164`–`169` |
| R5 | README 宣称的导出总数必须等于 `lib.rs` 实测数量 | `scripts/check-csharp-abi-surface.py:171`–`180`、`scripts/check-csharp-abi-surface.py:60`（正则覆盖中英两种句式） |
| 记账 | `SafeHandle` 未落地 → 固定 WARN，防止"P1-5 已做完"的口头翻案 | `scripts/check-csharp-abi-surface.py:192` [must-contain: SafeHandle] |
| 基线 | `--at-head` 取提交内容，避免把并行在制品读成事实 | `scripts/check-csharp-abi-surface.py:68`–`70` |
| 失败姿态 | fail-closed：读不到文件即 exit 2 | `scripts/check-csharp-abi-surface.py:211` |

实测两个读数（差值本身即并行会话在制品的量化）：

```text
$ python scripts/check-csharp-abi-surface.py --at-head          # EXIT=0
=== C# ABI 表面对齐校验（读取来源：HEAD）===
  WARN 覆盖面：9 个导出已被 C# 封装，28 个未封装
  WARN 句柄形态：C# 侧仍为裸 IntPtr + 手工 Dispose/终结器，未使用 SafeHandle（P1-5 遗留）
实测：Rust 导出 37，头文件声明 37，C# 导入 9
OK: C# 绑定与 Rust 导出/头文件契约一致

$ python scripts/check-csharp-abi-surface.py                    # EXIT=1（并行会话已把导出加到 41、C# 封装加到 22）
=== C# ABI 表面对齐校验（读取来源：工作区）===
  WARN 覆盖面：22 个导出已被 C# 封装，19 个未封装
  WARN 句柄形态：C# 侧仍为裸 IntPtr + 手工 Dispose/终结器，未使用 SafeHandle（P1-5 遗留）
  RED   R5 README.md 宣称导出总数 [37] 与 lib.rs 实测 41 不一致
  RED   R5 README.zh.md 宣称导出总数 [37] 与 lib.rs 实测 41 不一致
实测：Rust 导出 41，头文件声明 41，C# 导入 22
FAIL: 2 项违规
```

工作区的 R5 红牌**不是缺陷而是设计**：谁改了导出数，README 就必须同批改。README 侧当前宣称位置为 `README.md:623` [must-contain: 37 C ABI exports]、`README.zh.md:570` [must-contain: 37 个 C ABI 导出]（§8.3 原记为 README.md 第 607 行，现按实际行号更正），并行会话落库那 4 个新导出时必须同步——本轮不代做，因为那两个数字属于他人的在制品。

自测：`tests/test_check_csharp_abi_surface.py`（8 个用例，合成仓库夹具覆盖 R1–R5 + fail-closed + 真仓库 `--at-head` 冒烟）。CI 接线：`.github/workflows/ci.yml:90`（两份自测并入 pytest 清单）、`.github/workflows/ci.yml:93` [must-contain: check-csharp-abi-surface]（新增门禁步骤）。

**未做**：`SafeHandle`、事务/参数化 CRUD 的 C# 侧实现，以及任何 `dotnet` 实机运行——三者均需要 `SzOrm.cs`/`lib.rs` 的写权与 SDK，本轮排除。

### 11.4 P3-1（前半）｜"28 种方言"口径标注

P3-1 的"默认编译标注"半边完成，"golden SQL 快照"半边受阻（`packages/sz-orm-core/src/dialect.rs` 属并行会话在制品）。

- `packages/sz-orm-core/src/db_type.rs:5`–`19`：模块文档写明 **枚举变体总数 28 = 21 无条件编译 + 7 `dialect-*` 门控**、默认构建实际可用 **21**（17 SQL + 4 NoSQL）、`--all-features` 下 28（24 SQL + 4 NoSQL），并显式声明**方言层 = SQL 生成/转义，不等于连接驱动**（真实驱动为 sqlx 的 MySQL/PostgreSQL/SQLite + `sz-orm-oracle` + `sz-orm-mssql` + `dialect-saphana-driver` 门控桥接），最后写明门禁 18 取 28 的口径来源。
- `README.md:4` [must-contain: 21 by default]、`README.md:135` [must-contain: not a connection driver]、`README.zh.md:101` [must-contain: 默认编译 21 种]：三处口径同步。
- **刻意不改** `README.zh.md:4` 的"28 SQL 方言"——那是门禁 18（`scripts/check-metrics-real.py`，接线于 `scripts/run-gates-v10120-server.sh:121`）按**枚举变体总数**强制的字符串；改成"21"会让门禁红牌，而"28"本身并没有错，错的是它被读成"28 种可直接连接"。这条张力现在由 `db_type.rs` 文档与 README 表格共同消解。

回归（测量时点）：`check-metrics-real.py` **EXIT=0**——本节的方言口径改动不触碰任何数字；该关随后因并行会话新增 1 个 `#[tokio::test]` 报出 21272/21273 漂移，归因见 §11.5 注①、`check-doc-consistency.py` **EXIT=0**、`check-doc-version-consistency.py` **EXIT=0**（"检查了 8 处当前版本声明，全部与 workspace 一致"）、`check-filename-encoding.py` **EXIT=0**。

### 11.5 本轮回归矩阵（2026-10-29，同一台机器）

| # | 命令 | 退出码 | 关键输出 |
|---|---|---|---|
| 1 | `python -m pytest tests/test_audit_verify_ps1.py tests/test_check_csharp_abi_surface.py -q` | **0** | `18 passed in 9.97s` |
| 2 | `.\scripts\audit-verify.ps1 docs/assessment/2026-10-28-fullstack-review-v3.md -AtHead` | **1**（落库前） | `通过: 192 / 失败: 18`（18 处 = 14 指向本轮未提交文件 + 4 断言在 HEAD 旧文案上不成立，见 §11.2）；落库后复验（`7358e49e`）：**通过 210 / 失败 0 / 警告 115 → EXIT=0** |
| 3 | 同上报去掉 `-AtHead` | **1** | `通过: 205 / 失败: 5`（5 处全部为并行会话漂移，见 §11.2 漂移实录） |
| 4 | `.\scripts\audit-verify.ps1 docs/assessment/2026-10-20-comprehensive-audit-report.md` | **0** | `通过: 24 / 失败: 0`（修复前 23/1） |
| 5 | `python scripts/check-csharp-abi-surface.py --at-head` | **0** | `Rust 导出 37，头文件声明 37，C# 导入 9 … OK` |
| 6 | `python scripts/check-csharp-abi-surface.py` | **1** | R5 两处 `37 vs 41`（在制品，非缺陷） |
| 7 | `python scripts/check-metrics-real.py` | 首跑 **0** / 末次复跑 **1** | 首跑 `结果: ✅ 通过`；末次复跑 8 处 `声称 21272 → 实际 21273`（`README.md:4/8/136`、`README.zh.md:4/8/102/937`），归因见下注① |
| 8 | `python scripts/check-doc-consistency.py` | **0** | `[PASS] 文档与代码一致性校验通过` |
| 9 | `python scripts/check-doc-version-consistency.py` | **0** | `结果: ✅ 通过` |
| 10 | `python scripts/check-filename-encoding.py` | **0** | — |
| 11 | `python scripts/check-secrets.py` | **0** | `Scanned 2612 files … OK: no secrets detected` |
| 12 | `python scripts/check-gate-scripts.py` | **0** | `✅ PASS：门禁脚本无硬编码 PASS、无 || 假 PASS，关键安全脚本均已接入` |
| 13 | `git remote get-url origin` | **0** | `https://github.com/ljclz/sz-orm.git`（无内嵌凭据） |
| 14 | `git log --oneline origin/main..HEAD \| wc -l` | **0** | **23** 个未推送提交（含并行会话提交；推送需用户授权 + 交互式登录） |

**注①（门禁 18 末次复跑转红的归因，非本轮改动）**：本轮对两份 README 的全部改动只有"方言口径"三行，未增删任何数字（`git diff HEAD -- README.md README.zh.md` 中被改的 3 行仍保留 `21272+ tests` 与 `28 种数据库方言`/`28 dialects` 原值，已逐行核对）。转红的原因是计数函数 `scripts/check-metrics-real.py:47` [must-contain: count_test_annotations] 按 `#\[(tokio::)?test\]` 对 `packages/**` 全量扫描，而并行会话提交 `84d132a6`（"补齐 metrics_hook 自动埋点链路文件"）新增的 `packages/sz-orm-sqlx/tests/metrics_wiring_test.rs` 恰含 1 个 `#[tokio::test]`——标注总数 21272 → 21273，README 未同批同步。校验该数字的真伪只需重跑门禁 18；把它改绿是**该提交的收尾**，本轮不代改他人产生的数字（同 §11.3 R5 红牌的处置原则）。
**未执行**（不表述为通过）：`cargo test --workspace`、G7 `--ignored`、`cargo clippy --workspace --all-targets`、覆盖率 G22、变异 G20、`dotnet` 系命令、`bash scripts/audit-verify.sh` 的 v4 对齐验证。原因依次为：长跑与并行会话编译资源竞争、依赖真实服务、本机无 SDK、`.sh` 属他人在制品。

### 11.6 本轮未做 / 受阻项（按风险排序，含归属）

| 项 | ROI | 未做原因 | 风险 |
|---|---|---|---|
| `audit-verify.sh` 与 `.ps1` v4 对齐（符号断言 + 通配覆盖 + `-AtHead` + CJK token） | P1-4 后半 | 该文件正被并行会话大改（`git diff --stat` → 88 插入）；双写必冲突 | **高**：服务器门禁 13 仍是旧口径（分母 2038） |
| 9 个凭据文件的密钥轮换 + git 历史清除 | P0-1 后半 | 需仓库管理员权限与 force-push 授权 | **高**：历史 blob 与旧 PAT 仍可用 |
| 历史 154 份报告的引用清账（651 处失败 / 77 份红牌） | 新增（§11.2 量出） | 属存量债，非本轮范围；每份都要人工判断"漂移"还是"编造" | 中：门禁 13 只校验固定一份，历史红牌继续隐形 |
| 门禁 27 的登记盲区：`check_required_scripts()` 只拼 `run-gates-v10120-server.sh` + `gate.ps1` 两个脚本文本（`scripts/check-gate-scripts.py:118` [must-contain: SERVER_GATE, PS_GATE]），**不读 `.github/workflows/ci.yml`** | P2-3 后半 | 扩规则要同时动 `check-gate-scripts.py` 与 `gate.ps1`（后者为在制品），且新增编号需改 `AGENTS.md` 门禁表（也在他人手里） | 中：只接入 CI 的检查器（含本轮 `check-csharp-abi-surface.py`）被删掉步骤也不会报警 |
| P2-2 基准产物入库 | P2-2 | `bench-comparison/benches/full_comparison.rs` 与 `BENCHMARK_REPORT.md` 均为并行会话未提交改动——此时提交基准数字等于把他人代码产生的结论记在自己账上 | 中：性能结论仍不可复验 |
| P3-1 golden SQL 快照 | P3-1 后半 | `packages/sz-orm-core/src/dialect.rs` 为在制品 | 中：方言能力宣称仍缺逐条证据 |
| P3-2 `pool` notify 许可合并注释 + 回归测试 | P3-2 | `packages/sz-orm-core/src/pool.rs` 为在制品 | 低 |
| P1-5 C# 实现侧（`SafeHandle`/事务/参数化 CRUD）+ E2E 实机 | P1-5 后半 | `lib.rs`/`SzOrm.cs` 为在制品 + 本机无 SDK（用户约束） | 中：C# 侧仍有手写 SQL 注入面与 use-after-free 软约束（现由 WARN 常驻记账） |
| P2-1 crates.io 发布 10.13.0 | P2-1 | 发布是不可逆外部动作，需单独授权 | 中：用户装到的仍是 10.7.0（§8.1 #22） |
| P2-4 文档收口（`AGENTS.md` 版本行、`README.en.md` 绑定章节、`CHANGELOG.md`） | P2-4 | 三个文件均为在制品；本轮只完成 README 双语方言口径半边（§11.4） | 低 |

### 11.7 本轮引入的新风险（自审）

1. **`-AtHead` 会让"工作区在制品"免于校验。** 好处是假红牌消失，代价是：如果某人引用了一个尚未提交的文件行，`-AtHead` 直接判"文件不存在"——这是 fail-closed，可接受；但如果报告长期只对 HEAD 负责，则**并行改动造成的引用失效要等落库后才会暴露**。缓解：§11.5 的 #2 与 #3 两条命令必须成对跑（HEAD 口径 + 工作区口径），只看其中一个都会得到误导性结论。
2. **v4 的 99 处弱语义提示变成"噪声"风险。** 噪声大了，人会开始忽略警告色。故 `-Strict` 明确标注为"当前不可用作门禁"（§11.2 边界 2），并把补标记列为后续动作，而不是假装已经收敛。
3. **通配覆盖引入的 pattern 顺序耦合。** `audit-verify.ps1` 现在有 6 类引用形态，同一行内按"最早出现的引用"消费；若某条绝对路径引用（`file:///` 加 `#L` 行号）之前还留有形如"某点 rs 冒号行号"的文本，匹配次序会影响标记归属。已由 `tests/test_audit_verify_ps1.py` 的"标记绑定只作用于紧邻前一条引用"用例锁住，但后续加形态时仍需扩测试。
4. **BOM 依赖。** v4 是含大量中文注释的 `.ps1`，一旦有工具（含本轮的 Write 流程）剥掉 UTF-8 BOM，脚本在 Windows PowerShell 5.1 下立刻不可用且报错形式迷惑。当前无门禁检查 `.ps1` 的 BOM——**建议**下一轮把"BOM 必须存在"并入 `check-gate-scripts.py`，本轮未做（该文件与他人改动同源）。
5. **报告未声明"行号对哪次提交负责"，重钉本身成了新的不确定性。** 本节撰写期间 HEAD 前进两次，§二/§七/§八 的连接池引用组被 `c72d90a1` 的 634 行插入整体下移（§11.2 漂移实录）。本轮的处理是把这组行号钉到 `c72d90a1` 并加 `[must-contain:]` 断言——断言能让"行存在但内容不符"变成红牌，但**只要并行会话继续向 pool.rs 加行，重钉后的行号仍会失效**。根治方式是给校验器加 `@<commit>` 逐条锚点（`-AtHead` 目前是全报告统一取 HEAD，做不到 per-citation 固定），本轮未实现；在此之前，本报告所有 `file:line` 的默认口径是"**门禁 13 执行时的 HEAD**"，落库前后必须成对复跑（§11.5 #2 + #3），单独任一条都不构成通过凭证。

### 11.8 一句话结论（第二轮后）

第七节的"先修可信度"路线走到这里：**审计校验器（P1-4）与 C# ABI 面（P1-5 前半）第一次具备了机器可判的语义约束**，凭据的**入库面与 remote URL 都干净了**，方言宣称口径不再会被读成"28 种可连"。但可信度债的三块最硬的——**旧密钥仍在历史里、`.sh` 入口仍是旧口径、651 处历史引用无人清账**——都需要授权或他人文件让位，本轮如实记账，不认领。


---

**评测人结语：** sz-orm 已经具备"企业级 ORM"的骨架与少见的自我审计文化；真正拖住它的不是能力，而是**证据链的自我欺骗**——扫描器、校验器、门禁自检、文档数字，每一环都在"看起来在工作"。把 §7 的 P0/P1 做完（约 5 人日），这个项目会从"值得尊敬的原型"变成"可以押注的基础设施"。（2026-10-28 补记：§7 的 P0/P1/P2 代码侧已按 §10 执行，但结语中"做完"的判据里仍有 §10.7 的四项未闭合，故此结论尚未翻案。）
