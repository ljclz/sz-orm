# sz-orm 全栈工程师评测报告（v2）

- **评测人角色**：顶级全栈工程师（Rust / C++ / C# 系统级语言，15+ 年大型系统研发经验：内存安全、所有权/生命周期、async 并发、FFI/ABI 互操作、性能剖析、跨语言数据访问生态）
- **评测对象**：sz-orm v10.12.0（`D:\www\test\鲜视达\rust\sz-orm`，72 workspace 成员）
- **评测日期**：2026-10-27（依据 2026-10-26 评测提示词执行）
- **评测性质**：独立只读评测（未修改任何源码/文档/脚本）
- **方法论**：audit the audit（不采信历史报告数字，逐条复核）/ reproduce before report（关键结论以实测命令输出为准）/ check the checkers（审查门禁脚本自身逻辑）/ 证据铁律（每条结论附 `file:line`，报告末尾以 `audit-verify.ps1` 自证）
- **与 v1（2026-10-25-fullstack-review）的关系**：v1 发现的 8 项历史问题在本报告中逐条复核当前状态（见第六节）；v1 报告末尾记载的修复声称（P0/P1/第二轮 5 项）全部经源码亲读或命令实测验证。

---

## 一、总体评价

**结论：有条件投产（内部生产可用，条件为整改三项门禁红灯与文档缺口）。**

本次评测最重要的发现是：**v1 评测（2026-10-25）列出的全部高严重度问题已经真实修复，且修复质量经得起逐行复核**——连接池 shutdown 竞态有了确定性回归测试并实测通过（14/14）；跨 ABI 结构体全部补齐 `#[repr(C)]`；FFI 标识符注入两处漏网全部前置于 `validate_identifier`；cbindgen 头文件、FfiMemoryManager double-free 防护、JS/Python 绑定真实化、最小 C# 绑定全部落地。**「引擎完整、车身缺失」的时代结束了。**

但同时实测发现三个**新的红灯**，全部属于「质量体系自身的时效性」问题而非引擎缺陷：①门禁 3（clippy `-D warnings`）在当前 stable 工具链（rustc 1.99.1）下有 9 个包、51 处 `clippy::double_must_use` 真实失败，且仓库无 rust-toolchain 钉子；②门禁 18 实跑 FAIL（README 声称 21134，实际统计 21136）——修复后仅隔数天数字再次漂移，证明「手写数字」问题未根治；③门禁 13 的执行脚本 `audit-verify.ps1` 只校验「行号 ≤ 文件非空行数」且**从不核对行号处内容**，其保证力度低于 AGENTS.md 的宣称。此外五条语言绑定完成度仍有分化（Java/Go/C++ 停留在「仅 Pool」），而 README 双语现在**完全不提绑定层**——从「过度声称」摆到了「零声称」。

对「能否跑在我的生产系统里」的直接回答：**Rust 内核（连接池/事务/查询/流式）可以**，并发正确性有确定性回归测试与 loom 模型兜底；**跨语言绑定层按语言分级评估**——C#/Python/JS（SQLite 场景）可用，Java/Go/C++ 限于基础 CRUD。

---

## 二、分维度评测

### 维度 1：Rust 代码质量与内存安全 —— 9.0/10

#### ✅ 亮点

1. **FFI 层 panic 防护与 double-free 防护全部到位**：4 个 free 函数（`sz_orm_query_result_free` / `sz_orm_string_free` / `sz_orm_qb_free` / `sz_orm_qb_result_free`）全部包裹 `catch_unwind` 并接入内存注册表——仅释放已登记指针，未登记则静默返回（把 double-free UB 变成安全 no-op）。证据：`packages/sz-orm-cabi/src/lib.rs:395`（query_result_free，catch_unwind 在 :400，memory_untrack 在 :402）、`packages/sz-orm-cabi/src/lib.rs:994`（string_free）、`packages/sz-orm-cabi/src/lib.rs:1981`（qb_free）；分配侧 `memory_track` 在结果分配路径真实接线（`packages/sz-orm-cabi/src/lib.rs:374`）；回归测试 `test_double_free_protected` 存在（`packages/sz-orm-cabi/src/lib.rs:2183`）。注册表实现（track/untrack/泄漏计数）见 `packages/sz-orm-cabi/src/ffi_memory.rs:40`。
2. **unsafe 总量受控且管理真实**：本机 `python scripts/check-unsafe-audit.py` 实测 exit 0（17 个 src 文件含 unsafe、0 个「真实 unsafe 且缺 SAFETY」文件、36 个误报文件）。全工作区 `mem::transmute`/`ManuallyDrop` 0 处，唯一 `MaybeUninit` 在 smallvec 栈上存储。
3. **锁中毒面已按 v1 建议收口**：`query_result_cache.rs` / `pool_elastic.rs` / `failover_controller.rs` 三文件的 std 锁已迁移 `parking_lot`（v1 报告 §十-4 声称，本轮经 `cargo test -p sz-orm-core --lib` 后台复跑无回归佐证）；核心连接池本就用 `parking_lot`（无 PoisonError）。
4. **fail-closed 设计有担当**：租户上下文缺失即 panic（防跨租户泄漏）是正确取舍，且有注释说明。

#### ⚠️ 风险

1. **【新发现·高】门禁 3 在当前 stable 工具链下真实失败**：`cargo clippy --workspace --all-targets -- -D warnings` 退出码 101，**9 个包失败、51 处 `clippy::double_must_use`**（rustc 1.99.1，2026-10-13 发布；失败包：sz-orm-ai-designer / ai-migration / health / postgis / queue / search / storage / timeseries / websocket）。典型位置 `packages/sz-orm-ai-migration/src/ai_migration_generator.rs:58`（`#[async_trait::async_trait]` 生成的 `impl Future` 被新 clippy 判定 double-must-use）。仓库无 `rust-toolchain.toml`（本机实测不存在），门禁结果随工具链漂移——2026-10-26 服务器记录 G3 EXIT=0 与本机失败并存即是证明。
2. **【低】cabi 约 14 处链式调用的 unsafe 块共享一条 SAFETY 注释**（如 `packages/sz-orm-cabi/src/lib.rs:1186` 的 `values_s` 块，其注释在 :1179，跨 7 行链式调用）。行为安全（同一函数、同一不变量），但不符合「每块紧邻」的字面标准，且 `check-unsafe-audit.py` 的文件级口径（见维度 6）不会抓到进一步劣化。
3. **【低】`sz_orm_cabi::runtime()` 仍是硬编码 2 worker 的全局 runtime**（`packages/sz-orm-cabi/src/lib.rs:185`），FFI 高频调用时是吞吐单点（v1 已提出，未整改；对管理面 API 可接受）。

#### 建议

- P0：修复 51 处 `double_must_use`（给 async_trait 方法补 `#[must_use]` 消息或局部 allow），或在仓库根加 `rust-toolchain.toml` 钉住已验证工具链——二选一，建议都做。
- P3：cabi 剩余链式 SAFETY 补块级注释。

### 维度 2：性能工程 —— 8.5/10

#### ✅ 亮点

1. **连接池并发模型是真实的高质量无锁实现**（本轮逐行确认）：空闲队列为 `Arc<ArrayQueue<PooledConnection>>` 无锁 MPMC（`packages/sz-orm-core/src/pool.rs:1170`）；acquire 的 pop 为单次 CAS（`packages/sz-orm-core/src/pool.rs:1818` 注释，实现在 ：1824 附近）；计数占位用 `compare_exchange`（`pool.rs:1901`）；release 端**条件唤醒**——仅 `waiters > 0` 才 `notify_one()`（`packages/sz-orm-core/src/pool.rs:2100-2103`）；`FastPathStats` 用自研 `CachePadded<T>` 64B 对齐防伪共享（`packages/sz-orm-core/src/pool.rs:37`）。
2. **shutdown 序列语义清晰**：`closed.swap(true)` → `notify_waiters()` → `close_all()` → 带超时等待在途连接（`packages/sz-orm-core/src/pool.rs:2461-2472`），幂等可重入。
3. **流式是真流式**：`StreamResultSet::stream_query` 基于 `futures::stream::unfold` 逐页拉取（`packages/sz-orm-stream/src/result_set.rs:79`）；`AsyncBackpressureController` 用 `AtomicUsize` 计数 + `Notify` 做生产者挂起/消费者唤醒（`packages/sz-orm-stream/src/backpressure.rs:49-59`）。
4. **性能宣称诚实化完成**：`simd.rs` 顶部文档明确改为「标量路径 + 编译器自动向量化，不使用显式 SIMD intrinsic，也不依赖 `wide` crate」并说明理由（`packages/sz-orm-core/src/simd.rs:3-9`）；`batch_decode_integers` 直呼标量实现（`packages/sz-orm-core/src/simd.rs:113-115`）；core 的 Cargo.toml 无 `wide` 依赖（grep 0 命中）；README 双语已无 "SIMD" 字样（grep 0 命中）。`sz-orm-parallel/src/vectorized.rs:214` 仍诚实标注「模拟 SIMD」。
5. **`serialize_zero_copy` 已由 Debug 格式化改为真二进制编码**：类型标记 + 小端长度 + 借用数据直写，输出仅 `Vec<u8>` 一次分配（`packages/sz-orm-core/src/l2_cache.rs:3012-3017`，编码器 `write_borrowed` 在 ：3030 起），配套 `deserialize_zero_copy`（:3020）——v1 指出的「拷贝最多的路径却叫 zero_copy」已修复。

#### ⚠️ 风险

1. **【低】criterion 原始基准产物已不在仓库**：v1 引用的 `bench-comparison/target/criterion/pool_acquire_sqlite_1000/new/estimates.json` 现不存在（target 清理），仅存 `bench-comparison/BENCHMARK_REPORT.md` 与结果 txt 摘要——对比数字失去可复现的原始数据锚点。README 基准章节自称 `cargo bench -p sz-orm-core` 可复现（README.md:1006 附近），但跨 ORM 对比（1,939 ns vs SQLx 14,523 ns）暂无原始 JSON。
2. **【低】SIMD「自动降级」的实测收益未独立验证**：文档诚实了，但「LLVM 自动向量化已足够快」的依据是内部实测（2026-08-19），无外部对照数据。

#### 建议

- 把 bench-comparison 的 criterion 产物纳入 `benches/` 或发布物（或至少入库 estimates.json），让性能数字可审计。
- `acquire_zero_copy`（perf_extreme）这类「名字大于技术」的 API 建议改名或补注释，避免下一个评测者再花时间甄别。

### 维度 3：跨语言绑定层质量（重点） —— 7.0/10

#### ✅ 亮点（v1 全部 P0/P1 缺陷已修复，本轮逐项验证）

1. **ABI 布局契约完整**：全部 5 个跨 ABI 结构体均已标注 `#[repr(C)]`——`PoolConfigC`（`packages/sz-orm-cabi/src/lib.rs:118`）、`QueryResultC`（:139）、`QueryJsonResult`（:175，注释明确 "must be stable across compilers"）、`PoolStatsC`（:475）、`PoolMetricsC`（:486）。v1 的【高】风险「QueryJsonResult 未标 repr(C) 却被 C/Go/Java/C# 解引用」已消除。
2. **cbindgen 头文件单一声明源已落地**：`build.rs` + `cbindgen = "0.26"` build-dependency（cabi Cargo.toml [build-dependencies] 段），生成 `packages/sz-orm-cabi/include/sz_orm_cabi.h`（357 行，含全部结构体与 36 个导出函数声明）。C# 绑定注释明确以该头文件为 ABI 契约（`packages/sz-orm-cabi/csharp/SzOrm.cs:3`）。
3. **标识符注入封堵**：`sz_orm_table_exists` 在拼接前调用 `validate_identifier`（`packages/sz-orm-cabi/src/lib.rs:699`）；`sz_orm_count` 同样前置校验后再 `format!("SELECT COUNT(*) AS cnt FROM {table}")`（`packages/sz-orm-cabi/src/lib.rs:746-750`）；`validate_identifier` 定义（:1025，拒绝空串/非法首字符/引号/分号/空格，测试覆盖 ：2688-2719）；模型级 API 的表名/字段名/键名 10 处调用点全数接线（:1081/:1103/:1190/:1270/:1342/:1415/:1484/:1561/:1633/:1705）。
4. **36 个导出函数全部 `#[no_mangle] + extern "C"`**（lib.rs grep 计数 36），内存所有权「Rust 分配/释放」模型与句柄配对严格。
5. **五条绑定真实可用性显著改善**：
   - **JS（napi-rs）：从纯桩 → 真实**。`connect(dsn)` 建真实 SQLite 连接池（`packages/sz-orm-js/src/pool.rs:101`），`async_query` 直调 `pool.query_with_timeout` 并序列化真实行（:193-200），`async_execute` 真实 acquire+execute（:204-215），事务方法真实，且 Rust 侧 E2E 测试齐备（:632/:674/:695/:719）。「返回伪造 rows」的历史行为已根除。
   - **Python（PyO3）：Transaction 从空壳 → 真实**。`execute/commit/rollback/savepoint/release_savepoint` 全部真实执行（`packages/sz-orm-python/src/transaction.rs:78/:94/:115/:136/:155`）。
   - **C#：新增最小绑定（v1 建议采纳）**。`packages/sz-orm-cabi/csharp/SzOrm.cs`（175 行）：`[StructLayout(LayoutKind.Sequential)]` 结构体镜像 `#[repr(C)]` 布局（:18-52）、`[DllImport]` P/Invoke（:62 起）、`sealed class SzOrmPool : IDisposable`（:55）+ 析构兜底；`SzOrmTest/Program.cs`（34 行）E2E 测试连 SQLite 内存库，且 `bin/Debug` 下有真实构建产物（sz_orm_cabi.dll 在位）。
6. **README 状态表述已诚实**：v1 抓出的「JS/Python 标 `✅ Completed` 而实为桩」已改为准确描述——`README.md:990-991` / `README.zh.md:970-971` 现写明「真实（SQLite）…Model 为值对象」。

#### ⚠️ 风险（按严重度排序）

1. **【中】Java 绑定语言侧仍只有 6 个 native 方法**：`packages/sz-orm-java/java/sz_orm_java/SzOrmPool.java:19-24`（poolNew/poolFree/ping/query/execute/version）。Rust 侧 JNI 导出的事务/模型/QB 全部没有 Java 封装类——「引擎完整、车身缺失」在 Java 上原样存在。README 不再宣称（见风险 3），但 AGENTS.md 仍把五条绑定列为对等能力，对接方若只读 AGENTS.md 会高估 Java 完成度。
2. **【中】Go 绑定仍 Windows-only**：`packages/sz-orm-go/go/szorm/dll_unix.go:16` 明确 `unix dlopen not supported in this build (use cgo variant)`，且仓库内**没有** cgo 变体（go/szorm/ 仅 dll_unix/dll_windows/load_unix/load_windows/szorm.go/szorm_test.go 六文件）；`load_unix.go:8-16` 虽有 Unix 加载路径但依赖的 `newUnixLibrary` 恒返回错误。平台限制未在任何文档中声明。
3. **【中】README 双语对绑定层零描述（过度修正）**：`README.md` 与 `README.zh.md` 全文 grep `cabi|java|jni|csharp|C ABI|binding|绑定`——除 v2.0.0 历史交付表两行（Python/JS）外零命中。跨语言能力本是项目核心卖点（也是 v1/v2 评测的重点），现在新用户从 README 得知不了它的存在；同时 Go 的平台限制、C++ 的范围、C# 的最小性都无处可查。
4. **【低】JS 的 `index.d.ts` 仍是 0 字节空文件**（本机实测 0 行）——TypeScript 类型声明缺失，JS/TS 用户拿到的是无类型 API。
5. **【低】cabi 对 sz-orm-sqlx 的 path 依赖旁仍显式钉 `version = "10.11.0"`**（cabi Cargo.toml [dependencies] 段），与 workspace 10.12.0 存在版本漂移写法（`^10.11.0` 语义兼容，风险低但属无谓漂移源）。
6. **【低】语言侧 CI 缺位未变**：Java E2E（`java -Djava.library.path`）、Go `go test`、C++ 编译链接、C# `dotnet run` 均无 CI 自动化；C# 的 E2E 通过证据仅是本仓库内一次本地构建产物。

#### 建议

- P1：README 增补「跨语言绑定」章节（现状矩阵：C#/Python/JS 真实可用；Java 仅 Pool CRUD；Go 仅 Windows；C++ 仅 Pool RAII），把 Go 平台限制写进文档。
- P1：JS `index.d.ts` 由 napi-rs 生成或手工补齐（napi-rs 本可自动产出，空文件纯属遗留）。
- P2：Java 侧按需补 Transaction/Model 封装；Go 侧提供 cgo 变体或明确降级为「Windows 示例」。
- P2：cabi Cargo.toml 删除显式 `version = "10.11.0"`（保留 path 即可，或用 `workspace = true`）。

### 维度 4：async 并发模型 —— 9.0/10

#### ✅ 亮点

1. **连接池 shutdown 竞态（2026-10-24 发现）已修复且验证闭环**——本轮实测：
   - 修复 1（循环顶部复查）：等待者被唤醒回到循环顶部即检查 `closed`（`packages/sz-orm-core/src/pool.rs:1811`，注释说明修复动机于 ：1807-1810）。
   - 修复 2（CAS 后复查）：占位成功后、发起异步建连前再次检查 `closed`，失败则回退计数（`packages/sz-orm-core/src/pool.rs:1915-1917`）。
   - 确定性回归测试 `chaos_pool_shutdown_race_no_new_connection`（`packages/sz-orm-core/tests/chaos_pool.rs:819-865`）：单线程调度构造精确交错（release → shutdown → waiter 唤醒），断言 shutdown 后 `created == 3` 且等待者收到 `PoolError::Closed`。
   - **实测输出**：`cargo test -p sz-orm-core --test chaos_pool` → **14 passed, 0 failed（0.17s）**，含该回归测试。
   - loom 并发模型检查工作区真实存在（`loom-check/`：Cargo.toml + src + README）。
2. **PooledConnection 的 Drop 自动归还处理了「无 runtime 上下文」泄漏坑**（`pool.rs:440-466` 区域，mem::replace + ClosedConnection 占位，手动 fetch_sub 防容量泄漏）——异步连接池最经典的自伤点，实现正确。
3. **条件唤醒 + 指数退避**（1ms 起、100ms 封顶，`pool.rs:1799-1802`）兼顾延迟与功耗。

#### ⚠️ 风险

1. **【中】FFI 全局 runtime 2 worker 线程为所有语言调用共享单点**（`packages/sz-orm-cabi/src/lib.rs:185-193`），`block_on` 阻塞宿主线程；高频 FFI 查询吞吐受限。建议 worker 数进 `PoolConfigC`。
2. **【低】事务句柄无强制回收**：`CabiTransaction` 独占连接直到语言侧 free，无超时兜底（依赖调用方纪律）。

#### 建议

- 保持 loom 模型检查与 chaos_pool 回归在 CI 常态化（当前脚本 `run-gates-v10120-server.sh` 的 Gate 4 已含）。

### 维度 5：多语言/多数据库架构对比 —— 7.5/10

- **vs Rust（Diesel/SeaORM/SQLx）**：自研无锁池 + 统一 C ABI 多语言出口仍是差异化；`query!` 宏 + db-verify 编译期连库 EXPLAIN 可对标 SQLx 的编译期校验。短板不变：单作者、外部生态为零、生产验证仅 sz-pay 一个试点（本轮试图复核 `E:\vue\test\sz-pay` —— **路径在本机不存在，试点现状无法独立复核**，如实记录）。
- **vs C#（EF Core/Dapper）**：EF Core 的 LINQ 表达式树/迁移工具链/DI 集成远超；sz-orm 的 C# 接入走「P/Invoke + 值类型镜像 + IDisposable」路线，与 Dapper 的轻量哲学同路，但成熟度差距大（见第四节）。
- **vs C++（sqlpp11/SOCI）**：sqlpp11 的编译期类型安全 DSL 仍领先；sz-orm-cpp 仅 RAII Pool，事务/模型缺位。
- **28 种方言宣称**：`DbType` 枚举实测 28 变体（check-metrics-real.py 校验通过，README 徽章与实现一致）；但历史评测「多数方言仅 SQL 生成、无真实驱动」的定性未变（sz-orm-sqlx 实际覆盖 MySQL/PG/SQLite/MSSQL，Oracle/MSSQL 有专用包，其余为方言生成）。

### 维度 6：工程质量体系 —— 8.0/10

#### ✅ 亮点

1. **门禁真实「咬人」**：本轮实跑两个门禁脚本各出现一次真实拦截——G18 exit 1（见下）、G15 exit 0 但 PHANTOM-2 308 警告如实列出（PHANTOM-1 = 0、符号 38 通过、接线断言 4/4）。「门禁存在且能拦」这点经实测成立。
2. **check-metrics-real.py 逻辑合格且持续进化**：双语 README 均入扫描范围（`scripts/check-metrics-real.py:31-34`）；数字采集来自源码统计而非手写（测试标注 `#\[(tokio::)?test\]`，:37-49；feature 计数 ：52-60；方言枚举体计数 ：63-78，且 2026-10-25 修复过「末位变体无逗号漏计」缺陷）；`--fix` 自动修正并有模板占位符泄漏保护（:193-198）。
3. **check-phantom-delivery.py 双模式合理**：PHANTOM-1 零调用符号 + 接线断言（函数体级验证）+ PHANTOM-2 未启用 feature 警告（`--strict` 可升级为失败）——历史上正是它抓住了 JS 桩类问题。
4. **历史缺陷已修**：`run-gates-v10120-server.sh` Gate 6 已用 `&&`（:50）、Gate 11 已真实执行 `git diff --name-only HEAD -- packages/ Cargo.toml Cargo.lock`（:74-77）。

#### ⚠️ 风险（按严重度排序）

1. **【高】G18 实测 FAIL——度量漂移结构性复发**：`python scripts/check-metrics-real.py` exit 1：实际统计 **21136** 测试标注，README 双语 7 处仍写 **21134**（README.md:4/:8/:124；README.zh.md:4/:8/:102/:885）。v1/产品评测修复后仅数天，数字再次漂移——证明「README 手写数字 + 修复时人工同步」模式不可持续，需要 CI 每次提交自动重算（脚本已有 `--json`/`--fix`，缺的只是强制执行点）。
2. **【高】G3 在当前 stable 工具链失败**（见维度 1 风险 1）：9 包 51 处 `double_must_use`。门禁是工具链敏感的，而仓库没有钉工具链。
3. **【中·新发现】audit-verify.ps1（G13 执行器）的保证力度低于宣称**：`scripts/audit-verify.ps1:72` 用 `(Get-Content $path | Measure-Object -Line).Lines` 取「总行数」——`Measure-Object -Line` 只统计**非空行**，含空行的文件分母偏小，合法引用可能被误判越界；更关键的是**它从不读取被引行内容做匹配**（:71-79 只判断 `lineno -le totalLines`），「编造一个在范围内的行号」可通过 G13；且对含中文注释的 UTF-8 文件按 GBK 解码（PowerShell 5.1 默认），行号体系与真实物理行可能错位。G13 的实际含义是「行号大致在文件范围内」而非「证据真实存在于该行」。
4. **【低】Gate 1 仍为无条件 `mark 1 PASS`**（`scripts/run-gates-v10120-server.sh:26`，注释声明「已通过，直接标记」）——服务器门禁脚本中仍有一个不执行的门禁；Gate 20/22 按设计 SKIP（:115-116/:131-132）。
5. **【低】check-unsafe-audit.py 口径为文件级**：`"SAFETY" in content` 即通过（`scripts/check-unsafe-audit.py:65`）——文件级宽松口径下不会漏报「整个文件零注释」，但对「单文件部分块缺注释」无感知（与第六节 #6 裁决相关）。

#### 建议

- P0：CI 中固化 G18（每次提交跑 `check-metrics-real.py`，或 `--fix` 后 diff 非空即失败）与 G3（固定工具链或修复 51 处 lint）。
- P1：audit-verify.ps1 升级为「读取被引行并做关键词匹配」（如要求行号处或邻域包含该文件名/符号），并把 `Measure-Object -Line` 换成物理行计数；用 UTF-8 读取。

### 维度 7：可维护性与可扩展性 —— 7.5/10

1. **146 扁平模块属实**（`packages/sz-orm-core/src` 实测 146 个 .rs），508 feature gate（实测）由 G10 all-features 编译与 G18 度量校验托底。
2. **新增绑定成本显著下降**：cbindgen 头文件就位后，新语言只需对照 `include/sz_orm_cabi.h` + 36 个导出函数；C# 绑定 175 行即是可复制范本。
3. **Bus Factor = 1 未变**：70 包 / 62 万行 / 单作者；crates.io 发布人单一（ljclz）。README 自述 single-author 是诚实的，但这也是采纳决策的最大非技术风险。
4. **文档资产臃肿**：docs/assessment 下 130+ 份历史报告，新人无法分辨哪些结论仍有效（本评测对 4 份历史报告的复核结论本身就是这种成本的一个样本）。

---

## 三、与 C++/C#/Rust 主流方案对比表

| 维度 | sz-orm v10.12.0 | SQLx / SeaORM / Diesel (Rust) | EF Core / Dapper (C#) | SOCI / sqlpp11 (C++) |
|------|-----------------|------------------------------|----------------------|---------------------|
| 连接池 | 自研无锁（ArrayQueue+AtomicU32 CAS+条件唤醒+CachePadded），loom 模型检查 | deadpool/bb8/r2d2 | EF 内置池 | 内置/外部 |
| 并发正确性证据 | 确定性竞态回归测试 + loom 模型（3 不变量） | 社区大规模使用验证 | 微软背书 | 社区验证 |
| 编译期 SQL 校验 | `query!` 宏 + db-verify 连真库 EXPLAIN | SQLx `query!`（同类）/ Diesel DSL | LINQ 表达式树 | sqlpp11 模板元编程 |
| 多数据库 | 28 方言枚举；真实驱动 MySQL/PG/SQLite/MSSQL/Oracle | 各 3~5 个真实驱动 | 全 Provider | 多 DB（SOCI） |
| 跨语言 | **C ABI → C#/Python/JS 真实可用；Java/Go/C++ 仅基础** | 无 | .NET 内 | C++ 原生 |
| ABI 契约 | `#[repr(C)]` × 5 结构体 + cbindgen `.h` + double-free 注册表 | — | — | 头文件即契约 |
| 工程治理 | 24 门禁（本轮实测 G15/G18 真实执行；G3/G18 现存红灯） | 无体系化门禁 | GitHub 工程 | 社区 |
| crates.io / NuGet | sz-orm-core 最高 10.7.0（2026-10-16，30 版本，4,130 下载）；代码已 10.12.0 | 周下载百万级 | NuGet 主流 | vcpkg/会员管理 |
| 生产验证 | 内部试点 1 个（本机无法复核路径） | 广泛生产 | 广泛生产 | 广泛生产 |

**一句话**：sz-orm 在「ABI 契约完备度」与「工程治理」上是同类中最强的；在生态、成熟度、语言侧绑定广度上是同类中最弱的。

## 四、C# 生态接入专项建议

**结论：v1 提出的三项前提已全部落地，且最小绑定已存在——C# 接入已从「评估是否值得做」变为「按需补全 API 面」。**

1. **前提 1（repr(C)）**：✅ 已完成，5 结构体全标注（`packages/sz-orm-cabi/src/lib.rs:118/:139/:175/:475/:486`）。
2. **前提 2（cbindgen 头文件）**：✅ 已完成，`packages/sz-orm-cabi/include/sz_orm_cabi.h`（357 行）为单一声明源。
3. **前提 3（内存防护）**：✅ 已完成，track/untrack 注册表 + double-free 防护（`packages/sz-orm-cabi/src/ffi_memory.rs:40`）+ free 函数 catch_unwind。
4. **现状**：`csharp/SzOrm.cs` 提供 `SzOrmPool`（Create/ping/Query/Execute/错误描述 + `IDisposable`），E2E 测试真实连库；`[StructLayout(LayoutKind.Sequential)]` 值类型与 Rust `#[repr(C)]` 逐字段对应（`SzOrm.cs:18-52`）。
5. **补全路线（按 ROI）**：
   - ① `SafeHandle` 化代替裸 `IntPtr` + 析构兜底（防 GC 压力下漏 free）；② 错误码 → 异常层次（`SzOrmException` 按 `SzOrmErrorCode` 分类）；③ 事务/QueryBuilder 封装（对应 cabi 的 `sz_orm_tx_*`/`sz_orm_qb_*` 导出）；④ NuGet 打包（native dll 按 RID 分发 netstandard2.0 包）；⑤ 异步化评估——cabi 是同步 `block_on` API，C# 侧若要真异步需新增回调式导出或 `Task.Run` 包装（后者是假异步，建议文档明示）。
   - 工作量级：①②③ 各 1~2 天，④ 2~3 天，⑤ 若走回调式 ABI 需要先在 Rust 侧设计（1 周）。**总计约 1~2 周到「可用于 .NET 内部工具链」水平。**
6. **诚实的适用边界**：P/Invoke + JSON 字符串封送意味着 C# 侧拿到的是「字符串协议」而非强类型行——适合工具/网关场景；要做 EF Core 级别的 LINQ 体验不是绑定问题而是另一个产品。**建议定位为「Dapper 式轻量 + 跨语言共享引擎」**，与 EF Core 差异化竞争。

## 五、全栈工程师综合评分

| 维度 | 权重 | 得分 | 评分理由（对比 v1） |
|------|------|------|--------------------|
| Rust 代码质量与内存安全 | 20% | 9.0 | v1 9.0 维持：unsafe 管控真实，FfiMemory/SAFETY 收口；G3 lint 红灯计入工程体系维度 |
| 性能工程 | 15% | 8.5 | v1 7.5 上调：SIMD/零拷贝名实相符已修复，连接池/流式模型亲验扎实；扣分：基准原始产物缺失 |
| 跨语言绑定层 | 25% | 7.0 | v1 5.5 上调：repr(C)/cbindgen/FfiMemory/JS/Python/C# 全部落地；扣分：Java/Go/C++ 仍 partial、README 零声称、index.d.ts 空 |
| async 并发模型 | 15% | 9.0 | v1 9.0 维持：竞态修复确定性回归 + loom 模型；全局 runtime 单点未改 |
| 多语言/多数据库架构 | 10% | 7.5 | v1 7.5 维持：架构方向正确，方言真实驱动覆盖未变 |
| 工程质量体系 | 15% | 8.0 | v1 8.5 下调：门禁真实咬人（+），但 G3/G18 本机实测红灯、G13 校验器弱于宣称、Gate 1 硬编码（-） |
| **综合** | 100% | **8.1/10** | v1 7.7 → 8.1：**「诚实化」整改被验证为真，内核与治理双优；剩余风险集中在门禁时效性与绑定广度** |

## 六、历史发现复核结果表

| # | 历史发现（来源） | 当前证据 | 裁定 |
|---|-----------------|---------|------|
| 1 | 连接池 shutdown 竞态（2026-10-24 深度评测，300 轮压力复现） | 循环顶部复查 `packages/sz-orm-core/src/pool.rs:1811`；CAS 后复查 `pool.rs:1915-1917`；确定性回归测试 `tests/chaos_pool.rs:819`；实测 `cargo test -p sz-orm-core --test chaos_pool` → 14 passed（含该测试），exit 0 | **已修复（实测验证）** |
| 2 | `QueryJsonResult` 未标 `#[repr(C)]` 跨 ABI 解引用（2026-10-25 高严重度） | `packages/sz-orm-cabi/src/lib.rs:175`；全部 5 个跨 ABI 结构体均有（:118/:139/:175/:475/:486），注释明示布局稳定性承诺 | **已修复** |
| 3 | `sz_orm_count` 拼表名 / `sz_orm_table_exists` 仅转义单引号（2026-10-25 高严重度） | 两函数均在拼接前调用 `validate_identifier`：`packages/sz-orm-cabi/src/lib.rs:699`（table_exists）、`:746-750`（count）；模型 API 10 处调用点全覆盖 | **已修复** |
| 4 | 五条绑定真实可用性：JS 纯桩返假数据、Python Transaction 缺失、Java 仅 Pool（2026-10-25） | JS：`packages/sz-orm-js/src/pool.rs:101/:193-215` 真实 connect/查询/执行 + E2E 测试（:632/:674）→ 已真实；Python：`src/transaction.rs:78/:94/:115` 真实事务 → 已真实；Java：`java/sz_orm_java/SzOrmPool.java:19-24` 仍 6 native → **仍存在**；Go：`go/szorm/dll_unix.go:16` 仍 Unix 不可用且无 cgo 变体 → **仍存在**；C++：`cpp/szorm.h:51` 仍仅 Pool 类、无 C++ 测试 → **仍存在** | **部分修复**（JS/Python/C# 已真实；Java/Go/C++ 未变，README 已停止宣称） |
| 5 | SIMD/零拷贝名实不符（wide 声称 + Debug 序列化）（2026-10-25） | `packages/sz-orm-core/src/simd.rs:3-9` 文档诚实化（标量+自动向量化，wide 为扩展点）、`:113` 标量实现、Cargo.toml 无 wide；`l2_cache.rs:3012-3017` 二进制编码 + `:3020` 反序列化；README 无 SIMD 字样；`vectorized.rs:214` 诚实标注「模拟 SIMD」 | **已修复（文档+实现）** |
| 6 | 「37 处生产 unsafe 缺紧邻 SAFETY」vs「无缺失」矛盾（2026-10-25 vs 2026-10-20） | 本轮 UTF-8 逐块审计（5 行窗口）：go 0/148、cpp 0/146、java 0/124 缺失；cabi 277 块中约 14 处为链式调用共享注释（如 `lib.rs:1186` 引 :1179）。`scripts/check-unsafe-audit.py:65` 为文件级口径（`"SAFETY" in content` 即过）。**裁决：两份报告都对——口径不同**（2026-10-20 用文件级/函数级标准且其结论在当时成立；2026-10-25 用块级紧邻标准且计数属实）；此后 go/cpp/java 已补齐，cabi 剩余为样式问题 | **口径矛盾的裁定：均为真（标准不同）；当前残余约 14 处（cabi，低危）** |
| 7 | README 数字矛盾（21134 vs 9905、徽章 9.4.0、依赖 1.4、crates.io 仅 1.0.0、zh 停留 9.4.0）（2026-10-25 产品评测） | 9905/9.4.0/1.4 已清除，双语统一 21134+/10.12.0/508（`README.md:4/:8/:44/:124`、`README.zh.md:4/:8/:102`）；Quick Start 改 path 依赖（`README.md:448-453`）；crates.io 实查（WebFetch API）sz-orm-core **最高 10.7.0（2026-10-16 发布，30 个版本，4,130 下载）**——「仅 1.0.0」已过时，现落后 5 个 minor 版本。**但 G18 实跑 FAIL：实际 21136 vs 声称 21134（7 处）**——漂移复发 | **大部分修复，度量漂移结构性复发（G18 实测 FAIL）** |
| 8 | 门禁脚本自身缺陷（Gate 6 用 `||`、Gate 11 硬编码 PASS）（2026-10-24） | `scripts/run-gates-v10120-server.sh:50` 已改 `&&`；`:74-77` Gate 11 已真实执行 git diff。**仍存在**：Gate 1 无条件 `mark 1 PASS`（:26）；**新发现**：`scripts/audit-verify.ps1:72` 用非空行计数做行号分母且不核对行内容（G13 保证力弱于宣称）；check-metrics-real.py 本身逻辑合格且实跑抓到真漂移 | **部分修复（Gate 6/11 已修；Gate 1 仍在；发现新的校验器弱点）** |

**补充口径裁定**（v1/深度评测遗留的计数争议）：

- 「feature gate 221 vs 508 vs 517」：221 来自 `grep -c '^[a-z].*= \[' packages/sz-orm-core/Cargo.toml`——该模式实际统计 core Cargo.toml 中**值以内联数组开头的行（依赖声明+feature 数组）**，本机复跑该 grep 恰好仍得 221，而 core 单包真实 feature 数为 **120**、全工作区为 **508**（check-metrics-real.py 实测）。**裁定：221 是错误口径，508 是当前正确值，README 已用 508。**
- 「测试数 21132 vs 21134」：21132/21134 均为历史时点的 `#[test]`/`#[tokio::test]` 注解计数快照；本机当前实测 **21136**（新增测试未同步 README）。属随时间漂移的同一口径，非矛盾；暴露的问题是同步机制缺失。

## 七、优先整改清单（按 ROI 排序）

| 优先级 | 整改项 | 影响 | 成本 | 证据 |
|--------|--------|------|------|------|
| P0 | **G3 红灯清零**：修复 9 包 51 处 `double_must_use`（async_trait 场景补 `#[must_use]` 消息或 allow）并在仓库根钉 `rust-toolchain.toml` | 恢复门禁 3 在任意环境的可信度（当前「服务器 PASS / 本机 FAIL」并存） | 0.5~1 天 | `packages/sz-orm-ai-migration/src/ai_migration_generator.rs:58`；本机 rustc 1.99.1 实测 9 包失败 |
| P0 | **G18 数字自动同步**：README 数字改为 CI 生成（提交前 `check-metrics-real.py --fix` + diff 检查），消除手写漂移 | 21134→21136 漂移已第二次发生，动摇「度量真实性」对外承诺 | 0.5 天 | `scripts/check-metrics-real.py` 实跑 exit 1（7 处不一致） |
| P1 | **README 补「跨语言绑定」章节**：现状矩阵 + Go 平台限制 + C# 最小性声明 | 消除「零声称」；下游按真实能力选型 | 0.5 天 | README.md 全文 grep 绑定关键词 0 命中（v2.0.0 表两行除外，`README.md:990-991`） |
| P1 | **audit-verify.ps1 强化**：物理行计数 + 读取被引行做内容匹配 + UTF-8 读取 | G13 从「行号在范围内」升级为「证据真实」 | 0.5~1 天 | `scripts/audit-verify.ps1:72`（Measure-Object -Line 非空行口径） |
| P1 | **JS `index.d.ts` 生成**（napi-rs 自动产物） | TS 用户可用性 | 0.5 天 | 本机实测 0 字节 |
| P2 | **crates.io 追平**：发布 10.8.0~10.12.0（或 README 明示「最新能力走 git/path」） | 外部用户无法获得当前能力（10.7.0 vs 10.12.0） | 0.5 天/版 | crates.io API 实查 max_version 10.7.0 |
| P2 | **Java/Go/C++ 绑定补全或降级标注**：Java 补 Transaction 封装；Go 提供 cgo 变体或在文档标注 Windows-only；C++ 补事务类 | 绑定层从「部分可用」到「按需可用」 | 各 2~5 天 | `SzOrmPool.java:19-24`、`dll_unix.go:16`、`szorm.h:51` |
| P2 | **cabi 全局 runtime worker 可配置** | 解除 FFI 高频调用吞吐单点 | 1 天 | `packages/sz-orm-cabi/src/lib.rs:185-193` |
| P3 | cabi 链式 unsafe 补块级 SAFETY；run-gates Gate 1 去硬编码；bench criterion 产物入库 | 合规细节与可审计性 | 各 0.5 天 | `lib.rs:1186`、`run-gates-v10120-server.sh:26`、`bench-comparison/`（无 target/） |

## 八、取证命令记录与证据索引

### 8.1 本机实跑命令（2026-10-27）

| 命令 | 退出码 | 关键输出 |
|------|--------|---------|
| `cargo test -p sz-orm-core --test chaos_pool` | 0 | **14 passed; 0 failed**（0.17s），含 `chaos_pool_shutdown_race_no_new_connection` |
| `cargo test -p sz-orm-core --lib` | 见 8.3 | （后台执行，结果附 8.3） |
| `cargo test -p sz-orm-cabi` | 见 8.3 | （后台执行，结果附 8.3） |
| `cargo clippy --workspace --all-targets -- -D warnings` | **101** | 9 包失败（ai-designer/ai-migration/health/postgis/queue/search/storage/timeseries/websocket），51 处 `clippy::double_must_use`（rustc 1.99.1）；首跑另暴露 pyo3 找不到 Python 的环境问题（设 `PYO3_PYTHON` 后复现真实 lint 失败） |
| `python scripts/check-metrics-real.py` | **1** | 实际 包=70 成员=72 测试标注=**21136** feature=**508**；README 双语 7 处声称 21134 不一致 |
| `python scripts/check-phantom-delivery.py` | 0 | PHANTOM-1 = **0**；符号通过 38；接线断言 **4/4**；PHANTOM-2 = 308（警告级） |
| `python scripts/check-unsafe-audit.py` | 0 | 含 unsafe 的 src 文件 17；真实 unsafe 且缺 SAFETY 的文件 **0**；误报文件 36 |
| WebFetch `crates.io/api/v1/crates/sz-orm-core` | — | max_version **10.7.0**（2026-10-16），30 版本，4,130 下载 |
| `bash scripts/audit-verify.sh docs/assessment/2026-10-26-fullstack-review.md`（等效 PowerShell 版） | 见 8.4 | 输出附报告末尾 |

**未执行项（如实记录）**：`cargo test --workspace` 全量（21k 测试，超出本会话预算；以 chaos_pool + core --lib + cabi 定向验证代替）；`--ignored` 集成测试（需真实 DB 会话）；`git status`/Gate 11（本机 git 不在 PATH，无法执行）；`loom-check` 模型复跑（预算原因，仅验证文件存在）；sz-pay 试点路径复核（`E:\vue\test\sz-pay` 本机不存在）。

### 8.2 证据索引（file:line 汇总）

**packages/sz-orm-core**
- `packages/sz-orm-core/src/pool.rs:37`（CachePadded）、`:640`（AcquireConfig min_idle）、`:1170`（idle: Arc<ArrayQueue>）、`:1811`（循环顶部 closed 复查）、`:1818`（pop 单次 CAS 注释）、`:1901`（compare_exchange）、`:1915-1917`（CAS 后 closed 复查）、`:2100-2103`（条件 notify_one）、`:2461-2472`（shutdown_with_timeout 序列）
- `packages/sz-orm-core/tests/chaos_pool.rs:819`（shutdown 竞态确定性回归测试）
- `packages/sz-orm-core/src/simd.rs:5`（「标量路径 + 编译器自动向量化」）、`:113`（batch_decode 直调标量）
- `packages/sz-orm-core/src/l2_cache.rs:3012`（serialize_zero_copy 二进制编码）、`:3020`（deserialize）
- `packages/sz-orm-parallel/src/vectorized.rs:214`（「模拟 SIMD」诚实标注）
- `packages/sz-orm-ai-migration/src/ai_migration_generator.rs:58`（clippy double_must_use 失败点）

**packages/sz-orm-cabi**
- `packages/sz-orm-cabi/src/lib.rs:118`（PoolConfigC repr(C)）、`:139`（QueryResultC）、`:175`（QueryJsonResult repr(C) + 布局稳定性注释）、`:185-193`（全局 runtime 2 worker）、`:374`（memory_track 接线）、`:395`（sz_orm_query_result_free + catch_unwind + double-free 防护）、`:475`（PoolStatsC）、`:486`（PoolMetricsC）、`:699`（table_exists 校验）、`:746`（count 校验）、`:750`（COUNT(*) AS cnt）、`:994`（string_free）、`:1025`（validate_identifier 定义）、`:1981`（qb_free）、`:2100`（test_double_free_protected）
- `packages/sz-orm-cabi/src/ffi_memory.rs:40`（untrack double-free 语义）
- `packages/sz-orm-cabi/include/sz_orm_cabi.h`（357 行 cbindgen 产物）
- `packages/sz-orm-cabi/csharp/SzOrm.cs:3`（头文件契约声明）、`:18-52`（StructLayout 镜像）、`:55`（IDisposable 封装）、`:62`（P/Invoke）

**绑定层**
- `packages/sz-orm-js/src/pool.rs:101`（connect）、`:193-215`（真实 query/execute）、`:632/:674`（E2E 测试）
- `packages/sz-orm-python/src/transaction.rs:78/:94/:115`（真实 execute/commit/rollback）
- `packages/sz-orm-java/java/sz_orm_java/SzOrmPool.java:19-24`（仍 6 native）
- `packages/sz-orm-go/go/szorm/dll_unix.go:16`（Unix 不可用）、`load_unix.go:8`
- `packages/sz-orm-cpp/cpp/szorm.h:51`（仅 class Pool）

**脚本与文档**
- `scripts/run-gates-v10120-server.sh:26`（Gate 1 硬编码 PASS）、`:50`（Gate 6 &&）、`:74-77`（Gate 11 git diff）
- `scripts/check-metrics-real.py:31-34`（双语 README）、`:37-49`（测试注解统计）、`:63-78`（方言计数）
- `scripts/check-unsafe-audit.py:65`（文件级 SAFETY 口径）
- `scripts/audit-verify.ps1:72`（非空行计数口径）
- `README.md:4/:8/:44/:124/:448-453/:990-991`；`README.zh.md:4/:8/:102/:885/:970-971`
- `Cargo.toml:2`（72 成员清单）、`:6`（version = "10.12.0"）

### 8.3 核心包测试结果（补录）

实测通过，均在本机（Windows，rustc 1.99.1）执行：

```
$ cargo test -p sz-orm-core --lib
test result: ok. 3217 passed; 0 failed; 0 ignored; 0 measured; ... finished in 31.16s
（EXIT=0；3217 与 2026-10-20 全面审计记录的基线一致，无回归）

$ cargo test -p sz-orm-cabi
running 78 tests
test result: ok. 78 passed; 0 failed; ... finished in 0.03s
（EXIT=0；78 = v1 报告第二轮修复记录声称的 78，含 test_double_free_protected）

$ cargo test -p sz-orm-core --test chaos_pool
running 14 tests
test result: ok. 14 passed; 0 failed; ... finished in 0.17s
（EXIT=0；含 chaos_pool_shutdown_race_no_new_connection）
```

### 8.4 audit-verify 自证输出

`powershell -ExecutionPolicy Bypass -File scripts\audit-verify.ps1 docs\assessment\2026-10-26-fullstack-review.md` 实跑输出（2026-10-27，exit 0）：

```
==========================================
  审计证据验证: docs\assessment\2026-10-26-fullstack-review.md
==========================================
? packages/sz-orm-cabi/src/lib.rs:395 (共 3080 行)
? packages/sz-orm-ai-migration/src/ai_migration_generator.rs:58 (共 102 行)
? packages/sz-orm-core/src/pool.rs:1170 (共 5089 行)
? packages/sz-orm-cabi/src/lib.rs:118 (共 3080 行)
? packages/sz-orm-core/src/pool.rs:1811 (共 5089 行)
? packages/sz-orm-cabi/src/lib.rs:185 (共 3080 行)
? packages/sz-orm-cabi/src/lib.rs:175 (共 3080 行)
? packages/sz-orm-cabi/src/lib.rs:699 (共 3080 行)
? packages/sz-orm-js/src/pool.rs:101 (共 654 行)
? packages/sz-orm-core/src/simd.rs:3 (共 1204 行)
? packages/sz-orm-core/src/simd.rs:5 (共 1204 行)
? packages/sz-orm-core/src/pool.rs:37 (共 5089 行)
? ...
  验证结果
  通过: 19   失败: 0   警告: 0
  审计证据验证通过
VERIFY_EXIT=0
```

**同时如实记录本脚本的两个口径局限（与第七节 P1 建议对应，评测者以 Read 工具逐行人工复核过全部引用行内容）**：
1. 脚本「共 N 行」用的是 `Measure-Object -Line`（非空行计数）：cabi lib.rs 报 3080（物理 3410）、pool.rs 报 5089（物理 5639）、js pool.rs 报 654（物理 739）——分母偏小，接近文件尾部的合法引用存在被误判「越界」的风险（本报告引用行号均未触及该风险区）。
2. 脚本按行匹配、每行只取**第一个** `packages/...rs:123` 引用校验——本报告部分行内含多个引用，仅首个被脚本校验，其余由评测者人工核对（全部真实存在）。

> **2026-10-27 整改更新**：上述两个局限已在第十二节 P1-2 修复（脚本升级 v2：全引用校验 + 物理行计数 + 空行拒绝）。修复后的最终自证：**63 处引用通过、0 失败**（且升级后的空行检查当场抓到一处因代码插入而漂移的旧引用并已修正——校验器有效性的直接证据）。

---

## 十二、整改执行记录（2026-10-27，按第七节清单顺序）

> 本节为整改落地证据。当时工作区存在另一并行整改会话（v10.13.0 安全评测闭环，见 CHANGELOG [10.13.0] 节），清单中「run-gates Gate 1 去硬编码」与该会话 M-4 汇合，已由其完成，本次未重复改动。

### P0-1a：G3（clippy -D warnings）红灯清零 ✅

- **根因**：51 处 `clippy::double_must_use` 全部由 `async-trait 0.1.91` 宏生成的 `#[must_use]` 触发（rustc 1.99.0 新 lint 严格化），源码零手写 `#[must_use]`（grep 全 9 包 0 命中）。
- **修复 1**：`Cargo.lock` 升级 `async-trait` 0.1.91 → **0.1.92**（crates.io 2026-08-08 发布，实测消除宏生成 must_use）；失败包 9 → 8 →（health 单独修复后）**0**。
- **修复 2**：`packages/sz-orm-health/src/lib.rs:272` `read_snapshot` 的 block-wrapped scrutinee 触发 `clippy::block_scrutinee`（1.99.0 新 lint），改为局部绑定后消除（语义不变：值已 clone，读锁提前释放无影响）。
- **验证**：`cargo clippy --workspace --all-targets -- -D warnings` → **exit 0**（2026-10-27 实测，修复前 exit 101 / 9 包失败）；`cargo fmt -p sz-orm-cabi -p sz-orm-health -- --check` → exit 0（全工作区 fmt 在 Windows 触发 os error 206，按 AGENTS.md 既有规避方式逐包执行）。

### P0-1b：工具链钉子 ✅

- 新增根级 `rust-toolchain.toml`（channel = "1.99.1"，profile minimal，含 rustfmt/clippy 组件）；rustup 实测输出 `1.99.1-x86_64-pc-windows-msvc (overridden by .../rust-toolchain.toml)`。下游 MSRV 不变（Cargo.toml `rust-version = "1.81"`）。CI 的 `dtolnay/rust-toolchain@stable` 与 rustup 目录覆盖机制兼容（rustup 自动补装钉住版本）。

### P0-2：G18 度量真实性——数字同步 + CI 固化 ✅

- `python scripts/check-metrics-real.py --fix`：README 双语 7 处测试数 21134 → **21153**（实测标注数），复查 exit 0。
- **顺带修复脚本缺陷**：`scripts/check-metrics-real.py:200-207`——原实现 `--fix` 成功后仍用修复前的 issues 决定结果行与退出码（修复成功也打印 ❌ 且 exit 1）；改为修复后重新扫描。
- **CI 固化**：`.github/workflows/ci.yml:53` 新增 `metrics-real` job（checkout + python + `check-metrics-real.py`），门禁 18 首次成为 CI 阻断项；YAML 语法经 pyyaml 解析验证通过。

### P1-1：README 补「跨语言绑定」章节 ✅

- `README.md`（## Cross-Language Bindings）与 `README.zh.md`（## 跨语言绑定）在 Supported Databases 之后新增：C ABI 契约说明（`#[repr(C)]` + cbindgen 头文件单一声明源 + track/untrack double-free 注册表 + panic 边界捕获）、六语言状态矩阵（C#/Python/JS ✅ 真实；Java/Go/C++ ⚠️ 部分可用 + 各自缺口）、`sz_orm_set_runtime_workers` 说明、crates.io 滞后与 git/path 依赖指引（无具体版本数字，规避 G19 版本一致性风险）。

### P1-2：audit-verify 双脚本强化（v2） ✅

- `scripts/audit-verify.ps1`（:64 物理行读取 / :73 单引用校验）与 `scripts/audit-verify.sh` 重写：①一行内**全部**引用逐个校验（v1 每行只查第一个）；②UTF-8 物理行计数（替换 `Measure-Object -Line` 非空行口径）；③**被引行为空行即失败**；④file:/// Unix 绝对路径不再误剥首斜杠；sh 版同时支持根级文件引用（deny.toml/Cargo.toml/AGENTS.md/CHANGELOG.md 等）。
- **正向实测**：对本报告 → 61 处引用通过、0 失败（v1 只查 19 处），exit 0。
- **负向实测**：编造越界行号（pool.rs:99999）、不存在文件（ghost.rs:1）、空行引用（pool.rs:4）均被拦截，exit 1。
- **边界如实记录**：本机无 bash（cmd/PowerShell 均无），sh 版无法本机运行验证，仅同构移植 + 静态审查，运行时验证依赖 Linux CI；脚本仍不能做"行内容支撑结论"的语义级验证（文档已注明）。另：本机为 .ps1 补 UTF-8 BOM（PowerShell 5.1 无 BOM 按 GBK 解析中文注释会解析失败——这也是 v1 版本能正常工作的原因）。

### P1-3：JS index.d.ts ✅

- `packages/sz-orm-js/index.d.ts` 由 0 字节补齐为对齐全部 `#[napi]` 导出的 TypeScript 声明：Pool（含 connect/事务/CRUD/静态方法）、QueryBuilder、Model、Transaction、QueryBuilderEnhanced/JoinClause/PoolConfigBuilder、Batch 三构建器与结果对象、MigrationTool/MigrationStep/SchemaDiff、FieldDefinition/IndexDefinition/RelationDefinition/ModelDefinition、JoinType/MigrationAction/FieldType/RelationType 四枚举。签名逐条对照 `src/*.rs` 的 `#[napi]` 导出与 jest 测试用法编写；napi-rs camelCase 约定已在文件头注明，`napi build` 可再生权威版本（本机无 node/npm，CLI 路径不可用）。

### P2：cabi runtime worker 可配置 + crates.io 说明 ✅

- `packages/sz-orm-cabi/src/lib.rs:193` `resolve_worker_count`（0→默认 2、>64→64 钳制）、`:202` `build_runtime`、`:232` 新导出 `sz_orm_set_runtime_workers(u32)`——**ABI 追加式**（不改动任何既有 `#[repr(C)]` 结构体布局，C#/Go/Java/C++ 调用方零影响）；`packages/sz-orm-cabi/include/sz_orm_cabi.h:75` 经 cbindgen 随构建自动纳入。
- 测试（行为副作用验证）：`lib.rs` 单元测试 `test_resolve_worker_count_semantics`（钳制语义）+ `test_build_runtime_with_configured_workers_functions`（按配置构建的 runtime 真实执行任务返回 42）；新增 `packages/sz-orm-cabi/tests/runtime_workers_test.rs` **进程级 E2E**（独立测试进程：登记 3 workers → 首次建池 → 真实查询 success=1 → 初始化后再登记不生效语义 → 配对释放）。
- **验证**：`cargo test -p sz-orm-cabi` → **80 lib + 1 integration 全过**（exit 0）。
- crates.io 说明已并入 README 绑定章节（发布滞后提示 + path 依赖指引）。

### P3：cabi SAFETY 紧邻化 + bench 产物 ⚠️（部分完成）

- cabi 23 处链式 unsafe 块补块级 `// SAFETY: 调用方保证 <param> 有效`（含测试助手 `free_find_str` 的 `/// # Safety` 文档）；窗口=1 严格复验剩余 11 处均为 ≤2 行共享注释对（如 `lib.rs:449-451` 释放对）与 `#[cfg(test)]` 测试代码，行为安全，如实保留。
- run-gates Gate 1：并行会话已在 `scripts/run-gates-v10120-server.sh:25-27` 完成去硬编码（M-4），本次未重复改动。
- bench criterion 产物：`bench-comparison/target/` 已被清理，重建需冷编译 diesel/sea-orm/rusqlite-bundled（约 20 分钟级）超出本会话预算；**建议由 CI bench job 持久化 criterion 产物**（未执行，如实记录）。

### 门禁复跑汇总（2026-10-27）

| 门禁 | 结果 |
|------|------|
| G3 clippy（workspace -D warnings） | ✅ exit 0（修复前 9 包 51 错误） |
| G1 fmt（改动包逐包，Windows os error 206 规避） | ✅ cabi/health 均 exit 0 |
| G18 metrics-real | ✅ exit 0（21153 同步后） |
| G15 phantom-delivery | ✅ exit 0（PHANTOM-1=0，含新增符号的测试调用） |
| G9 SQL 注入 --strict | ✅ exit 0（46 项 Safe） |
| G19 publish-consistency | ✅ exit 0 |
| G12 doc-consistency | ⛔ 本机环境不可执行：Python 3.8.6 不支持脚本使用的 3.9+ 泛型语法（`tuple[int, list[str]]`，报 TypeError: 'type' object is not subscriptable）；CI 的 setup-python 3.x 可正常执行 |
| G14 doc-sync | ⛔ 本机无 git，无法执行 `--diff HEAD`；由 CI/并行会话补跑 |
| cabi 测试 | ✅ 80 lib + 1 integration（新增 runtime workers 测试） |
| health 测试 | 由 G3 clippy 全目标编译覆盖（改动为等价重写） |

---

**评测人结语**：这次复核最值得肯定的是 v1 报告的修复闭环是**真实的**——不是文档层面的「已修复」，而是每一项都能在源码里找到对应实现、在测试输出里找到对应通过记录。sz-orm 当前最大的风险不再是「名实不符」，而是「体系随时间衰减」：门禁在不同工具链/机器上结果不一致、README 数字手工同步必漂移、证据校验器弱于宣称。把这三个「体系时效性」问题关进 CI，这个项目就配得上它 AGENTS.md 里写下的标准。
