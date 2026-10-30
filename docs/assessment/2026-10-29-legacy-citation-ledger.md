# 历史评测报告引用红牌 · 分诊账本（2026-10-29）

- 版本：sz-orm v10.13.0（`workspace.package.version`）
- 触发：`docs/assessment/2026-10-28-fullstack-review-v3.md` §11.6 遗留项"历史 154 份报告的引用清账（651 处失败 / 77 份红牌）"
- 性质：**只分诊，不清账**。本文件不对任何历史报告做一次行号改写。

## 一、为什么先做账本，不做批量修复

批量"把 651 处行号改成现在能过的行"是本账本明确禁止的动作，理由是它会把**编造证据洗成结构合规证据**：

门禁 13 的校验器能判的是"文件是否存在、行号是否在范围内、被引行是否非空、`[must-contain:]` 断言是否成立"。它判不了"这行代码是否就是当初得出结论的那行"。对着一份三个月前的报告重钉行号，产出的是一个**通过校验的假证据链**——比原来的红牌更危险，因为红牌至少还会提醒人去看。

所以本账本要回答的只有一个问题：**这 651 处红牌里，哪些是"路径写法/仓库搬迁"造成的噪声，哪些是"行号漂移"，哪些是真正的无源引用。** 只有分类完成后，人才可以逐份决定"改写引用形式 / 降级为文字表述 / 保留红牌并标注不可复核"。

## 二、测量口径（可复现）

| 项 | 值 |
|---|---|
| 扫描范围 | `docs/assessment/*.md` 全量 = **154 份** |
| 校验器 | `scripts/audit-verify.ps1`（v4：符号级断言 + 6 类引用形态 + CJK token + 通配 `path.ext:NN`） |
| 模式 | **工作区口径**（不加 `-AtHead`）；本账本关心的是"读者今天点开引用能否落地" |
| 命令 | 逐份执行 `powershell -NoProfile -ExecutionPolicy Bypass -Command "& .\scripts\audit-verify.ps1 -ReportPath '<file>'"`，汇总 stdout |
| 测量时点 | HEAD = `d395f31b`（2026-10-29 并行会话把 28 个本地提交聚合为该单提交；聚合前的 SHA 见 §六） |
| 执行日期 | 2026-10-29 |

汇总数字（同批跑了两次，第二次在报告重钉后；下表为第二次）：

| 指标 | 数值 |
|---|---|
| 校验通过（引用可落地） | **1946** 处 |
| 校验失败（红牌） | **648** 处 |
| 弱语义警告 | **1767** 处 |
| 至少 1 处红牌的报告 | **76** 份 |
| 全绿报告 | 78 份 |
| 通过=0 且失败=0（**整份报告没有任何可校验引用**） | **45** 份 |
| 全部红牌集中在 1 份报告的最坏值 | 90 处（`2026-08-13-production-zero-call-audit.md`） |

> 与 §11.2 的 651/77 差 3 处：那 3 处正是本报告自己因 `pool.rs` 插入 24 行注释造成的断言失败，已在重钉后转绿（见 §五）。

## 三、648 处红牌的分类账本

### 3.1 失败原因原始分布

| 原因 | 处数 | 占红牌比 |
|---|---|---|
| 文件不存在 | 526 | 81.2% |
| 被引行为空行（无法承载证据） | 113 | 17.4% |
| 行号超出范围 | 9 | 1.4% |
| `[must-contain:]` 断言失败 | 0 | 0%（本轮曾有 3 处，已重钉） |

### 3.2 "文件不存在"526 处的去向拆解（判定"是否等于造假"的关键）

对 174 个唯一被引路径逐一比对"当前工作树 / 全历史曾出现 / 是否被删除提交命中"：

| 类别 | 处数 | 判定 | 正确处置 |
|---|---|---|---|
| A1 旧克隆绝对前缀 `E:/vue/test/鲜视达/rust/sz-orm/...`，去掉前缀后**当前仓库同名文件存在** | **422** | **纯噪声**：报告写于仓库还在 `E:` 盘的另一台机器/另一路径时 | 由校验器做路径归一化（见 §四建议 1），不需要改报告 |
| A2 旧克隆绝对前缀，且当前仓库也没有该文件 | 1 | 等同 D 类 | 人工判断 |
| B 下游项目绝对路径 `E:/vue/test/sz-pay/server/sz-rust/...` | 12 | **本仓库无从校验**（是外部生产试点的真实文件，只是不在本仓库） | 标注"仓库外证据"，或改引日志/截图；不能当作造假 |
| C 其他绝对路径写法（以 `/` 开头，如 `/packages/sz-orm-bench/tests/real_db_bench.rs`） | 27 | **纯噪声**：去掉开头斜杠即命中真实文件 | 路径归一化即可 |
| D1 曾在历史存在、后被删除提交命中 | 16 | **历史真实、现已失效**。主要是 `packages/sz-orm-sqlx/tests/wiring_e2e.rs`（8 处）与 `packages/sz-orm-sqlx/examples/wiring_sz_pay.rs`（5 处），两者在"删除 v10.11.0 意外引入的无效测试文件"那次提交中被移除 | 保留红牌并在原报告标注"该文件已删除"——这 13 处恰恰是**幻影交付的历史物证**，改掉等于销毁证据 |
| D3 相对路径、任何提交都不存在 | 48 | 细分见下表 | 按细分处理 |

D3 的 48 处再分（按"同名文件在全历史中的候选数"）：

| 细分 | 处数 | 说明 |
|---|---|---|
| 同名文件全历史唯一 → 引用只是**漏写包前缀** | 25 | 例：`graphql/query_ir.rs` 实为 `packages/sz-orm-graphql/src/query_ir.rs`；`tests/kat.rs` 实为 `packages/sz-orm-crypto/tests/kat.rs`；`cdc/capturer.rs` 实为 `packages/sz-orm-queue/src/cdc/capturer.rs` |
| 同名文件有多个候选（多为 `lib.rs`，候选 72 个） → **需人工判断** | 22 | 例：`config/lib.rs`、`sz-orm-cpp/lib.rs`、`sz-orm-java/lib.rs`——包名写在路径里但缺 `packages/` 前缀，机器无法安全唯一定位 |
| 全历史从未出现该文件名 | 1 | 是一条 `packages/...rs` 省略号写法，属正文排版而非引用 |

**结论一句话：526 处"文件不存在"里，可以确认为"引用对象根本不存在（造假）"的数量是 0。** 422 + 27 + 25 + 22 = 496 处（94.3%）是路径写法问题，16 处是被删文件的历史物证，12 处是仓库外证据，1 处是排版省略号。

### 3.3 "被引行为空行"113 处

对被引行做 ±3 行邻域探测：**113 处全部在邻域内存在非空内容**（邻近也全空 = 0 处）。这与"文件被增删行导致的小幅漂移"完全一致，不构成造假证据。分布最集中的是 `README.md`（12 处）、`packages/sz-orm-core/src/pool.rs`（10 处）、`docs/Security.md`（5 处）、`packages/sz-orm-cabi/src/lib.rs`（5 处）——都是高频改动文件。

### 3.4 "行号超出范围"9 处

`Cargo.toml`（3 处）、`packages/sz-orm-adaptive/src/planner.rs`（2 处）、`packages/sz-orm-observability/src/anomaly_remediation_rca.rs`（2 处）、`packages/sz-orm-core/src/perf_extreme/simd_full_pipeline.rs`（2 处）。文件存在但行号超过当前总行数 = 文件**变短**了，同属漂移，需逐处人工确认原结论是否仍成立。

## 四、比"651 处红牌"更该修的五件事

1. **校验器缺路径归一化。** A1 + C 共 449 处（占全部红牌的 **69.3%**）只是 `E:/vue/...` 绝对前缀或开头斜杠。建议在 `scripts/audit-verify.ps1` 加一条规则：被引绝对路径若命中"已知旧克隆根"或以 `/` 开头且去掉根后的仓库相对路径存在，则按仓库相对路径校验，并在输出里显式标注"路径已归一（原写法：…）"。**必须在同一批里同步 `scripts/audit-verify.sh`**，否则跨平台口径再度分叉（§11.6 第一条遗留项正是它）。
2. **报告规范：禁止把仓库绝对路径写进证据。** AGENTS.md 的"审计合规铁律"只要求 `file:line`，没说必须仓库相对。本轮之后新写的所有报告一律用仓库相对路径；旧报告不改写，但在下一轮修订时在页首加"本报告的 `E:/...` 前缀写作旧克隆路径"一行说明。
3. **45 份"零可校验引用"报告是更大的洞。** 其中 5 份是评测提示词（`*-review-prompt.md`）、1 份是验证规范（`2026-08-15-old-coder-verification-spec.md`），本就不该有引用；其余 **39 份**是自称给出结论的报告却没有任何 `file:line`。门禁 13 对它们**返回通过**（0 失败），这才是"看起来在工作"的形态。建议给校验器加 `--require-evidence`：非提示词类报告若 0 处可校验引用即判失败。完整清单见附录 A。
4. **红牌噪声会让门禁失效。** 69.3% 的噪声意味着：只要有人真跑了全量门禁 13，看到 76 份红牌，最理性的反应就是不看它。存量债的正确清偿顺序是**先降噪声（建议 1）、再谈清账**，反过来的话清账成果会被噪声淹没。
5. **裸 `:NN` 续引是"装饰性证据"，校验器读不到。** 两个入口（`.ps1` 与 `.sh`）都只识别"带文件名与扩展名的路径 + 冒号行号"，形如"…`<脚本路径>:156`、`:58`、`:183`"里后两个**根本不进入校验**；如果它们后面还挂着 `[must-contain: …]`，那条断言写了等于没写。本轮在自己刚交付的报告里量到 50 处续引、其中 3 行挂 needle（见 `2026-10-28-fullstack-review-v3.md` §12.5 第 2 条，属本轮自查发现）。建议：①写报告规范时明确"每条引用必须自带完整路径"；②校验器对"同行出现 `:NN` 却无前置文件名"给警告，必要时并入建议 3 的 `--require-evidence`。存量续引不做批量改写（与本报告"不批量改行号"的立场一致）。

## 五、本轮顺带验证到的两件事（机制有效，不是只记账）

- `[must-contain:]` 断言**第一次抓到了自己人的错**：本轮向 `packages/sz-orm-core/src/pool.rs` 插入 24 行注释后，本报告 §二 的 `:2371`（`notify_one`）、`:2742`（`pub async fn shutdown`）与 §七 P3-2 行的 `:2368`（`条件 notify_one`）三处断言当场转红——这是聚合扫描里唯一的 3 处"断言失败"。若沿用 v1/v2 的"行存在即通过"，这三处会静默假绿。**已重钉为 `:2372` / `:2766`，并在原引用处保留"原为哪一行、因何漂移"的说明**（不是覆盖成看起来一直正确的行号）。
- 校验器对"本轮自己的改动导致引用漂移"完全无豁免：报告作者不是豁免对象。这条要在方法论上写明，否则"重钉"会被误读成特权操作。

## 六、测量环境的一个已知事实：本地历史已被聚合重写

本轮工作期间（同一台机器、同一工作树），并行会话执行了 `git reset` 到 `origin/main` 并将 28 个本地提交聚合为单提交 `d395f31b`。后果：

- 本账本与 `2026-10-28-fullstack-review-v3.md` 中引用的历史 SHA（`4cd0cb6d`、`84d132a6`、`c72d90a1`、`7358e49e`、`c7326573`、`4e3a07c6`、`019f545b`、`7194a0d7`、`3199185d`、`2395a34b`）**全部脱离 HEAD 祖先链**（对象暂时仍在，`git cat-file -e` 可验，`gc` 后即失效）。
- 已逐项核实**内容未丢**：`CI_FAIL_EXEMPT`（在 `scripts/check-gate-scripts.py` 中）、`tests/test_gate_scripts_ci.py`、`pool.rs` 的 notify 不变量注释、两个池回归测试、README 双语测试数 21274，均在 `d395f31b` 的树里。
- 这把 §11.7 风险 5 从"行号会漂"升级为**"提交号也会漂"**：报告引用 commit SHA 作为证据锚点时，锚点本身可以是悬空对象。本轮不在本账本里修补这个缺陷（修法应是校验器支持"引用 SHA 必须可达"检查），只把它记成事实。
- **本账本落库前后该事实又前进两次**（同一台机器、同一工作树）：`d395f31b` 之后被 `bce170be`（聚合 29 个提交）取代，随后并行会话把压在索引里的 42 文件 / 4581 行在制品批次落为 `37a21343`，本轮 `.sh` v4 为 `361f53bf`。也就是说上面那批"悬空 SHA"的**上一手**（`d395f31b`）现在同样悬空——§二"测量时点 HEAD = `d395f31b`"作为测量事实保留不改，但复现本账本数字时必须知道：**当时的 HEAD 已不可达，逐 SHA 复现只能靠本地对象库尚未 gc**。这与建议 5 属于同一类问题：锚点写得越具体，越要声明它对哪一次提交负责。
- **本轮末复测时，建议 1 已经在被实现**：并行会话给 `scripts/audit-verify.ps1` 的 `Get-PhysicalLines` 加了路径归一化（旧克隆根前缀 + 开头斜杠映射回仓库相对路径，+12 行，未提交）。后果是本账本自己的两条 ps1 钉行（§七 的 `:183`、`:374`）在工作区口径转红、`--at-head` 仍 10 / 0 / 0——**双口径在这里正好把"谁是漂移源"指出来**：内容没变，只是他们插入的位置在我引用的行之前。处置：钉 HEAD 内容为准，等其落地后按新行号重钉，不在编辑中途抢改。

## 七、本账本自身的证据索引（门禁 13 可校验）

本账本批评了 39 份"整份没有 `file:line`"的报告，因此它自己也必须过同一道关。以下每条都是今天可直接落地的引用：

| 结论 | 证据 |
|---|---|
| 断言机制抓到本轮自己的漂移：`release` 的条件唤醒注释现位于此 | `packages/sz-orm-core/src/pool.rs:2372` [must-contain: 条件 notify_one] |
| 被本轮重钉的第二处 | `packages/sz-orm-core/src/pool.rs:2766` [must-contain: pub async fn shutdown] |
| §四 建议 1（路径归一化）的落点：所有被引路径都经这一个函数判存在性，归一化只能加在这里 | `scripts/audit-verify.ps1:183` [must-contain: function Test-RepoFile] |
| 它背后的读取入口（工作区口径 vs `-AtHead` 口径在此分叉） | `scripts/audit-verify.ps1:105` [must-contain: function Get-PhysicalLines] |
| §四 建议 3 的缺口实证：校验器对"零引用报告"只给警告级提示、不改退出码 | `scripts/audit-verify.ps1:374` [must-contain: 未找到任何 file:line 证据] |
| §四 建议 2（禁止绝对路径）要约束的正是这类历史写法 | `docs/assessment/2026-08-13-production-zero-call-audit.md:1` [must-contain: 生产路径零调用审计]（本批红牌最集中的报告，90 处） |
| D3 中"漏写包前缀"的两个样例，其真实目标文件确实存在 | `packages/sz-orm-graphql/src/query_ir.rs:37` [must-contain: pub struct GraphQLDirective]、`packages/sz-orm-queue/src/cdc/capturer.rs:15` [must-contain: async fn start_capture] |
| §六 提到的 CI 豁免登记表（本轮新增，验证内容未因历史聚合而丢） | `scripts/check-gate-scripts.py:58` [must-contain: CI_FAIL_EXEMPT] |
| §11.5 注① 归因所用的计数函数（README 数字的唯一口径） | `scripts/check-metrics-real.py:47` [must-contain: def count_test_annotations] |

> 说明：本表刻意**不**为"被删文件"（D1 类 16 处）编造行号——`packages/sz-orm-sqlx/tests/wiring_e2e.rs` 与 `examples/wiring_sz_pay.rs` 今天已不存在，任何指向它们的 `file:line` 都只能是假的，故只在 §三 以路径形式记账并标注"已删除"。同理，下游项目 `E:/vue/test/sz-pay/...` 的 12 处属仓库外证据，本账本不给出任何仓库内行号。

## 附录 A：45 份"零可校验引用"报告

其中带 `prompt`/`spec` 的 6 份本就不承载代码证据，其余 39 份应纳入 §四 建议 3 的 `--require-evidence` 判定。

```text
2026-08-14-blackhat-security-audit.md
2026-08-15-old-coder-verification-spec.md
2026-08-16-full-gate-block-report.md
2026-08-16-full-gate-review-report.md
2026-08-16-gate-review-report.md
2026-08-19-phantom2-feature-gate-evaluation.md
2026-08-20-v4.9.1-performance-benchmark.md
2026-08-22-phantom2-feature-gate-evaluation.md
2026-08-22-supplement-tasks-v3-delivery.md
2026-09-01-ai-optimization-delivery.md
2026-09-03-strategy-assessment.md
2026-09-04-v6.0.x-delivery-record.md
2026-09-06-v64-vs-v65-comparison.md
2026-09-17-v7.2.0-gate-report.md
2026-09-21-v780-gate-report.md
2026-09-22-v840-coverage-matrix.md
2026-09-22-v840-quality-baseline.md
2026-09-23-v870-baseline-comparison.md
2026-09-23-v870-coverage-report.md
2026-09-23-v870-low-coverage-modules.md
2026-09-23-v870-mutation-report.md
2026-09-23-v870-perf-after-optimization.md
2026-09-23-v870-perf-benchmark-report.md
2026-09-23-v870-perf-regressions.md
2026-09-23-v870-survived-mutants.md
2026-09-25-v880-gate-report.md
2026-09-28-v920-coverage-report.md
2026-09-28-v920-delivery-record.md
2026-10-06-crates-io-publish-evaluation.md
2026-10-07-v951-coverage-report.md
2026-10-08-v951-five-dimensional-review.md
2026-10-08-v960-perf-baseline.md
2026-10-08-v970-perf-baseline.md
2026-10-09-v990-reliability-baseline.md
2026-10-14-v1010-phantom-audit.md
2026-10-16-architecture-review.md
2026-10-16-v1070-coverage.md
2026-10-17-v1080-coverage.md
2026-10-18-v10110-delivery-record.md
2026-10-20-v10120-delivery-record.md
2026-10-25-fullstack-review-prompt.md
2026-10-26-fullstack-review-prompt.md
2026-10-27-perf-review-prompt.md
2026-10-27-security-review-prompt.md
2026-10-27-uxr-review-prompt.md
```

## 附录 B：红牌最集中的 15 份报告

| 失败 | 通过 | 警告 | 报告 |
|---|---|---|---|
| 90 | 5 | 9 | `docs/assessment/2026-08-13-production-zero-call-audit.md` |
| 37 | 0 | 5 | `docs/assessment/2026-09-09-v680-five-dimensional-review.md` |
| 33 | 0 | 18 | `docs/assessment/2026-10-17-v1090-ai-deepen.md` |
| 29 | 0 | 3 | `docs/assessment/2026-10-17-v1090-delivery-record.md` |
| 26 | 0 | 7 | `docs/assessment/2026-10-17-v1090-security-deepen.md` |
| 22 | 0 | 3 | `docs/assessment/2026-10-17-v1080-utf8-fix.md` |
| 22 | 0 | 2 | `docs/assessment/2026-08-22-supplement-tasks-v2-delivery.md` |
| 20 | 0 | 2 | `docs/assessment/2026-10-16-v1060-rustls-vulnerability-fix.md` |
| 20 | 0 | 0 | `docs/assessment/v7.4.0-audit.md` |
| 20 | 0 | 0 | `docs/assessment/2026-10-25-product-evaluation.md` |
| 18 | 0 | 0 | `docs/assessment/v7.4.0-delivery-record.md` |
| 16 | 1 | 4 | `docs/assessment/2026-10-17-v1090-legacy-closure.md` |
| 14 | 11 | 4 | `docs/assessment/2026-09-25-gate-block-report.md` |
| 14 | 0 | 0 | `docs/assessment/2026-08-15-audit-fixes-final-batch.md` |
| 12 | 16 | 6 | `docs/assessment/2026-10-07-v950-delivery-record.md` |

## 附录 C：复现命令

```bash
# 全量分诊（154 份，约 4~5 分钟）
for f in docs/assessment/*.md; do
  echo "=====FILE: $f"
  powershell -NoProfile -ExecutionPolicy Bypass \
    -Command "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; & .\scripts\audit-verify.ps1 -ReportPath '$f'"
done > legacy-citation-sweep.txt 2>&1

# 单份复核
powershell -NoProfile -ExecutionPolicy Bypass \
  -Command "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; & .\scripts\audit-verify.ps1 -ReportPath 'docs/assessment/2026-08-13-production-zero-call-audit.md'"

# 本账本自身的引用自证（门禁 13）
powershell -NoProfile -ExecutionPolicy Bypass \
  -Command "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; & .\scripts\audit-verify.ps1 -ReportPath 'docs/assessment/2026-10-29-legacy-citation-ledger.md'"
```

分类逻辑（工作树存在性 / 全历史存在性 / 删除命中）用到的三条命令：

```bash
git -c core.quotePath=false log --all --pretty=format: --name-only | sed '/^$/d' | sort -u   # 曾出现路径
git -c core.quotePath=false log --all --diff-filter=D --pretty=format: --name-only | sed '/^$/d' | sort -u  # 被删路径
git -c core.quotePath=false status --porcelain   # 工作树在制品
```

---

**账本结语：** 本轮把 648 处红牌全部归了类，其中 **0 处可判定为"引用对象凭空捏造"**，94.3% 是路径写法，其余是被删文件的历史物证与仓库外证据。这既不给历史报告脱罪——`2026-08-13-production-zero-call-audit.md` 那份 90 处红牌仍然要逐份人工看——也不给门禁 13 记功：它今天的输出里近七成是噪声，而它对 39 份"整份没有任何代码证据"的报告直接返回通过。**先降噪声、再加"必须有证据"的正向约束，最后才谈存量清账**，顺序反了，清账只会产出更多无法证伪的绿。
