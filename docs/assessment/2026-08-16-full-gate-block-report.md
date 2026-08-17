# 门禁阻断报告（2026-08-16 全量）

- 分支 / commit：`main` @ `bcc1f42`（feat: 路线图 M1-M5 全量完成）
- 模式：`/sz-orm-review` 全量 23 关（G1~G23）
- 状态机：`G1 → G2 → G3 → G4 → G5 ❌ → G6 ❌ → FAILED`（红牌即停，G7~G23 未执行）
- 前置：`cargo clean`（清掉 240.6 GiB 损坏缓存，E0786 元数据损坏 + 磁盘 116G→330G 可用）；全量编译限 `-j 4`（20 核默认并行导致 32GB 内存耗尽：mmap rmeta failed / STATUS_STACK_OVERFLOW）

## 结果表

| G# | 门禁 | 状态 | 说明 |
|----|------|------|------|
| 1 | 格式检查 | ✅ | 0 diff（5.4s） |
| 2 | 编译检查 | ✅ | exit 0 |
| 3 | clippy 严格模式 | ✅ | exit 0，零警告（29.2s） |
| 4 | 单元/集成测试 | ✅ | **8040 passed / 0 failed / 258 ignored**（260 套件全 ok；258 个 ignored 为 G7 集成测试） |
| 5 | 文档构建 | ❌ | rdkafka-sys 编译失败（见下） |
| 6 | 安全审计 | ❌ | licenses FAILED（xxhash-rust BSL-1.0）+ 13 advisories（见下） |
| 7~23 | 其余关卡 | ⏹ | 红牌即停，未执行 |

## G5 失败详情（环境问题）

- **命令**：`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features -j 4`
- **根因**：`rdkafka-sys v4.10.0+2.12.1` build script 编译 librdkafka 失败（exit 101）。`--all-features` 激活 sz-orm-mqtt 的 rdkafka 依赖。
- **定性**：本机 Windows 已知环境问题（见本机记忆 local-dev-environment：rdkafka cmake 环境问题）；cmake 4.2.0-rc2 存在但 MSVC 侧编译 librdkafka 失败。CI（Ubuntu）无此问题。
- **建议**：G5 在 CI 上验证（Linux 正常）；本机如需过 G5，需修复 rdkafka-sys 的 Windows 编译环境（MSVC 工具链/依赖），或确认该 feature 门控策略。

## G6 失败详情（1 个真实新问题 + 13 个既有 advisory）

### 6.1 licenses FAILED（真实新问题，bcc1f42 引入）

- **违规**：`xxhash-rust v0.8.18` 使用 **BSL-1.0**（Boost Software License），不在 deny.toml allow 白名单
- **依赖链**：`sz-orm-core` ← sz-orm-actix / sz-orm-ai / sz-orm-advisor / sz-orm-vector（提交 bcc1f42 新引入）
- **建议**：① deny.toml allow 列表补 `BSL-1.0`（若团队接受该许可证）；② 或替换 xxhash-rust 为白名单内等价实现。需团队决策。

### 6.2 advisories（13 个，多为 feature-gated / dev-only / 生态现状）

| Crate | Advisory | 严重度 | 定性 |
|-------|----------|--------|------|
| pyo3 | RUSTSEC-2025-0020（PyString buffer overflow） | — | 绑定层（sz-orm-python），需升级 pyo3 |
| pyo3 | RUSTSEC-2026-0177（Missing Sync bound） | — | 绑定层 |
| quick-xml | RUSTSEC-2026-0194（7.5 high，重复属性名二次方） | high | 传递依赖，需升级 |
| quick-xml | RUSTSEC-2026-0195（7.5 high，ns 声明内存耗尽 DoS） | high | 传递依赖 |
| rkyv | RUSTSEC-2026-0235（out-of-bounds reads） | — | 传递依赖 |
| rsa | RUSTSEC-2023-0071（5.9 medium，Marvin Attack） | medium | 传递依赖 |
| rustls-webpki | RUSTSEC-2026-0098/0099/0104（+0049 已豁免） | — | 0.103.13 已是最新，无修复版；real-es/real-broker feature 门控（deny.toml 已登记 0049 先例） |
| paste | RUSTSEC-2024-0436（不再维护） | — | 传递依赖 |
| rustls-pemfile | （certificate 相关） | — | 传递依赖 |

- **定性**：无一项来自本次审查的代码变更；绝大多数为 dev-dependencies / feature-gated 依赖链或生态无修复版状态。
- **建议**：① 对 feature-gated/dev-only 项按 deny.toml 0049 先例登记豁免（`.cargo/audit.toml`）；② pyo3 / quick-xml / rsa 等有修复版的升级后重跑。

## 结论

❌ **阻塞**：G5（环境）、G6（1 真实 + 13 既有）未通过。修复路径：
1. **G6-licenses**（必做，团队决策）：deny.toml 补 BSL-1.0 或替换 xxhash-rust
2. **G6-advisories**：登记豁免或升级，二选一后重跑
3. **G5**：CI 验证通过即可作为环境豁免依据；本机修复 rdkafka 环境为可选

修复后从 **G5** 重跑（G7~G23 未执行，需完成全量）。

---
*生成：sz-orm-review skill 钩子 on_gate_failed（2026-08-16）*
