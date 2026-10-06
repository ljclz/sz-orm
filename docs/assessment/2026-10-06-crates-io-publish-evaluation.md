# crates.io 发布评估（2026-10-06）

- 评估目标：确认本地工作空间 v9.4.0 与 crates.io 已发布版本之间的缺口，验证发布流程可用性，识别执行阻塞点
- 评估方法：crates.io API 实测 + 本地发布脚本 DryRun 验证
- 结论摘要：**发布缺口 9.0.0 → 9.4.0 已确认，72 包拓扑序 DryRun 通过，但本机无 crates.io API token，实际发布被阻塞，待凭证就绪后执行**

---

## 1. crates.io 当前发布状态（实测）

| 包 | crates.io 最新版本 | 本地版本 | 缺口 |
|----|-------------------|---------|------|
| sz-orm-core | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-query-builder | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-sqlx | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-macros | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-config | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-queue | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-auth | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-crypto | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-cli | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-java | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-stream | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-parallel | 9.0.0 | 9.4.0 | ✅ 需发布 |
| sz-orm-python | 0.1.0 | 0.1.0 | ⏭️ 版本已存在（独立版本线，实际发布将 SKIP） |
| sz-orm-pool | 1.2.2 | 不在当前 workspace | ⏭️ 历史遗留包，不参与本次发布 |
| sz-orm-model | 1.2.2 | 不在当前 workspace | ⏭️ 历史遗留包，不参与本次发布 |
| sz-orm-migrate | 未发布（N/A） | 不在当前 workspace | ⏭️ 历史遗留包，不参与本次发布 |

其余 workspace 包（sz-orm-graph 等）跟随工作空间版本 9.4.0，crates.io 上均为 9.0.0，同样存在发布缺口。

## 2. 发布流程验证（DryRun 实测）

运行 `scripts/publish_crates_io.ps1 -DryRun`：

```
[13:33:26] === PUBLISH COMPLETE ===
Published: 72/72
Skipped:   0/72
Failed:    0/72
```

- 脚本按依赖拓扑序逐包发布（`scripts/compute_topology.ps1` 计算），底层库（queue/sql-validator/storage 等）先发，上层（core/axum/actix/websocket 等）后发。
- DryRun 模式仅打印命令不实际发布，已验证 72 包全部在发布序列中。
- 实际发布时 `sz-orm-python`（0.1.0 已存在）与可能已存在的版本会被脚本自动识别并 SKIP（`already exists` 分支）。
- 脚本内置门禁：fmt / check / clippy / test 全过才发布；发布失败即中止（`REQ-REL-009: no partial publish`）。
- CI 流水线 `.github/workflows/publish.yml` 使用 `secrets.CARGO_REGISTRY_TOKEN` 自动发布，与本脚本等效。

## 3. 执行阻塞点

| 项 | 状态 |
|----|------|
| `~/.cargo/credentials.toml` | ❌ 不存在 |
| 环境变量 `CARGO_REGISTRY_TOKEN` | ❌ 未设置 |
| 发布脚本 | ✅ 就绪（门禁 + 拓扑序 + 重试 + 失败中止） |
| 本地代码版本 | ✅ 9.4.0 全工作空间一致（门禁 19 check-publish-consistency 通过） |

**阻塞点**：crates.io 发布需要 API token。`cargo publish` 无法匿名执行。

## 4. 执行步骤（待凭证就绪）

1. 获取 crates.io API token：登录 <https://crates.io/me> → New Token（scope: publish-new）。
2. 本机配置：
   ```bash
   cargo login   # 粘贴 token，写入 ~/.cargo/credentials.toml
   ```
   或设置环境变量：
   ```bash
   export CARGO_REGISTRY_TOKEN=...
   ```
3. 执行发布：
   ```powershell
   .\scripts\publish_crates_io.ps1 -WorkspaceRoot "E:\vue\test\鲜视达\rust\sz-orm"
   ```
4. 发布后验证：
   ```bash
   curl -s -H "User-Agent: sz-orm-release-check/1.0" "https://crates.io/api/v1/crates/sz-orm-core" | jq '.crate.max_version'
   # 期望输出 9.4.0
   ```

## 5. 发布后文档同步

发布完成后，README / 文档中 "sz-orm-core published on crates.io" 与版本号 9.4.0 即完全一致（README 徽章已指向 9.4.0）。

## 附：crates.io API 实测数据

```bash
# 查询示例（须带 User-Agent）
curl -s -H "User-Agent: sz-orm-release-check/1.0" "https://crates.io/api/v1/crates/sz-orm-core"
# 关键字段：max_version=9.0.0, downloads=3386, updated_at=2026-09-26T12:10:06Z
```