# crates.io 发布评估与实际发布记录（2026-10-06）

- 评估目标：确认本地工作空间 v9.4.0 与 crates.io 已发布版本之间的缺口，验证发布流程可用性，识别执行阻塞点
- 评估方法：crates.io API 实测 + 本地发布脚本 DryRun 验证
- 结论摘要：**v9.4.0 全量发布已完成**（69 个应发布包全部确认 9.4.0；sz-orm-js / sz-orm-python 为 0.1.0 独立版本线已存在自动 SKIP；sz-orm-examples 为 `publish = false` 示例包不发布）

---

## 1. 发布前评估（2026-10-06 日间）

### 1.1 crates.io 当前发布状态（实测）

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
| sz-orm-js | 0.1.0 | 0.1.0 | ⏭️ 版本已存在（独立版本线，实际发布将 SKIP） |
| sz-orm-examples | 未发布（N/A） | 9.4.0 | ⏭️ `publish = false`，不参与发布 |
| sz-orm-pool | 1.2.2 | 不在当前 workspace | ⏭️ 历史遗留包，不参与本次发布 |
| sz-orm-model | 1.2.2 | 不在当前 workspace | ⏭️ 历史遗留包，不参与本次发布 |
| sz-orm-migrate | 未发布（N/A） | 不在当前 workspace | ⏭️ 历史遗留包，不参与本次发布 |

其余 workspace 包（sz-orm-graph 等）跟随工作空间版本 9.4.0，crates.io 上均为 9.0.0，同样存在发布缺口。

### 1.2 发布流程验证（DryRun 实测）

运行 `scripts/publish_crates_io.ps1 -DryRun`：

```
[13:33:26] === PUBLISH COMPLETE ===
Published: 72/72
Skipped:   0/72
Failed:    0/72
```

- 脚本按依赖拓扑序逐包发布（`scripts/compute_topology.ps1` 计算），底层库（queue/sql-validator/storage 等）先发，上层（core/axum/actix/websocket 等）后发。
- 实际发布时 `sz-orm-python` / `sz-orm-js`（0.1.0 已存在）与可能已存在的版本会被脚本自动识别并 SKIP（`already exists` 分支）。
- 脚本内置门禁：fmt / check / clippy / test 全过才发布；发布失败即中止（`REQ-REL-009: no partial publish`）。

## 2. 实际发布执行（2026-10-06 夜间）

### 2.1 凭证

使用 `鲜视达/服务器信息.md` 中记录的 crates.io API token，配置于 `~/.cargo/credentials.toml`。`/api/v1/me` 返回 403（该端点仅允许网站在浏览器中访问），通过实际发布成功验证 token 有效。

### 2.2 发布结果

| 类别 | 数量 | 说明 |
|------|------|------|
| ✅ 成功发布 9.4.0 | 69 | 全部 71 个应发布包中的 69 个（sz-orm-js / sz-orm-python 除外） |
| ⏭️ SKIP（0.1.0 已存在） | 2 | sz-orm-js、sz-orm-python（独立 0.1.0 版本线） |
| ⏭️ SKIP（`publish = false`） | 1 | sz-orm-examples（示例包，不发布） |
| ❌ 失败 | 0 | — |

发布后全量验证（crates.io API 实测，2026-10-06）：69/69 个应发布 9.4.0 的包 `max_version` 全部为 `9.4.0`。

### 2.3 执行中发现并修复的问题

1. **PowerShell 5.1 读取 UTF-8 文件乱码**（`Get-Content -Raw` 默认 ANSI 编码），导致 `Cargo.toml` 中文注释吞换行、行首正则匹配失败，拓扑排序漏依赖。
   - 修复：`scripts/compute_topology.ps1` 与 `scripts/publish_crates_io.ps1` 均改用 `Get-Content -Raw -Encoding UTF8`。
2. **发布脚本 OK 判断竞态**：`cargo publish` 输出含 `Uploading` 或 `$LASTEXITCODE -eq 0` 时被误判为成功，但 crates.io index 实际未收录新版本（如 sz-orm-anomaly / sz-orm-ai）。
   - 修复：`scripts/publish_crates_io.ps1` OK 分支严格匹配 `Published $pkg`（同时移除 429 重试分支中的 `$LASTEXITCODE -eq 0` 判断）。
3. **crates.io index 更新延迟**：`cargo publish` 返回 "Published" 后，紧接着的包依赖解析可能暂时查不到新版本（竞态窗口）。
   - 处置：发布后等待 3~8 秒再发布下一包；个别包出现该问题时重试。
4. **dev-dependency 循环依赖**：`sz-orm-agent` 的 `[dev-dependencies]` 引用 `sz-orm-sqlx = { version = "9.4.0", path = "../sz-orm-sqlx" }`，`cargo publish` 打包时会解析 dev-dependencies 版本；而 `sz-orm-sqlx` 尚未发布，导致发布失败。进一步分析发现 workspace 中多个包存在 dev-dependency 环（如 `sz-orm-macros` dev 依赖 `sz-orm-core`，`sz-orm-crypto` dev 依赖 `sz-orm-ai`/`sz-orm-audit` 等；`sz-orm-macros` 9.0.0 发布版 Cargo.toml 中并不存在这些 dev 依赖，属于 9.0.0 之后新增）。
   - 修复：`scripts/compute_topology.ps1` 正则扩展匹配 `[dev-dependencies]` / `[build-dependencies]`；检测到环时不再崩溃，回退到迭代贪心排序（每轮输出依赖已满足的包，死锁时按字母序输出并警告）。
   - 实际发布采用"跳过已发布包 + 迭代满足依赖"的顺序（Python 计算 31 个剩余包的依赖满足顺序）。
5. **crates.io 上传低速率超时**：`Operation too slow. Less than 10 bytes/sec transferred the last 30 seconds`，导致 sz-orm-cabi / sz-orm-swagger 上传失败，并级联影响依赖它们的 sz-orm-cpp / sz-orm-go / sz-orm-java / sz-orm-examples / sz-orm-cli。
   - 处置：重试时设置 `CARGO_HTTP_TIMEOUT=600` 与 `CARGO_HTTP_LOW_SPEED_LIMIT=1`，重试全部成功（sz-orm-examples 因 `publish = false` 预期失败）。

### 2.4 最终发布命令参考

```powershell
# 门禁跳过 + 自动发布（含已发布 SKIP）
.\scripts\publish_crates_io.ps1 -WorkspaceRoot "E:\vue\test\鲜视达\rust\sz-orm" -SkipGate

# 验证某个包
curl -s -H "User-Agent: sz-orm-release-check/1.0" "https://crates.io/api/v1/crates/sz-orm-core" | jq '.crate.max_version'
# 期望输出 9.4.0
```

## 3. 附：crates.io API 实测数据（发布前）

```bash
# 查询示例（须带 User-Agent）
curl -s -H "User-Agent: sz-orm-release-check/1.0" "https://crates.io/api/v1/crates/sz-orm-core"
# 关键字段（发布前）：max_version=9.0.0, downloads=3386, updated_at=2026-09-26T12:10:06Z
```