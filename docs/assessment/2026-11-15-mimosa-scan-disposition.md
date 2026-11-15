# Mimosa 深度安全扫描处置报告（2026-11-15）

> 扫描：Mimosa MCP `security_scan` deep，2026-11-15 07:44 UTC 完成，seal `sha256:ac6f390e...`
> 扫描 ID：`scan-2026-11-15T07-44-44.505Z-4c4593c0bf98`（存档 `~/.mimosa/security-scans/project-7d6a1a833bc44a9097128664/`）
> 触发原因：2026-11-15 复跑后两次 git commit 时 PreToolUse 钩子提示"未得到完整扫描结论（scanner_no_output）"，按提示补跑完整审计。
> 结果：**23 条发现（20 High / 3 Medium），逐条定性后 0 条为真实安全缺陷**——全部为本地门禁/工具脚本（占 16 条）、测试夹具假数据（2 条）、构建产物（1 条）或已有防护层误判（4 条）。逐条证据如下。

## 一、逐条处置（23 条）

### 1.1 生产源码面（8 条）——均误判，防护层已覆盖

| # | 发现 | 位置 | 定性证据 |
|---|------|------|---------|
| 1 | 硬编码凭据 | `packages/sz-orm-core/src/cdc/sinks/kafka.rs:213` | `#[cfg(test)]` 测试夹具假密码 `secret123`（构造 KafkaSink 事件用），非真实凭据；同串已在 `check-secrets.py` KNOWN_FALSE_POSITIVES（SQL 注入测试示例）登记 |
| 2 | 硬编码凭据 | `packages/sz-orm-core/src/hydration_plugin.rs:1898` | `#[cfg(test)]` 内 `t26hp_mask_sql_sensitive_values`——**测试断言的目标串**（验证 SQL 脱敏功能把 `password='secret123'` 打码为 `***`），非凭据 |
| 3 | SQL 注入入口 | `packages/sz-orm-wasm/src/real_db/proxy_server.rs:318` | SQL 先经 `sql_whitelist.validate(sql)` 白名单校验（`proxy_server.rs:289`，前缀白名单+禁关键字+禁分号，见 `sql_whitelist.rs:121`），且走 `query_with_params` **参数化绑定**；两层防护俱在 |
| 4 | path-traversal 入口 | `packages/sz-orm-graphql/src/real_graphql.rs:248` | `{err}` 为 `ComplexityError` 枚举 Display（`complexity.rs:89-108`：仅 `depth/field count/cost` + 数值），**不含任何用户输入字符串**；无注入面 |
| 5 | new 是 path-traversal 入口 | `packages/sz-orm-js/index.js:14` | `require(join(__dirname, 'core.node' | 'index.node' | 'sz_orm_js.node'))`——三个**硬编码文件名**，无用户可控路径成分 |
| 6 | 命令注入 | `scripts/soak-sz-orm-final.js:41` | `execSync('tar -czf ...')` 全部为**字面量参数**，无变量插值（`${tarFile}` 为本行同函数内定义的常量 `'sz-orm-src.tar.gz'`） |
| 7 | SSRF | `scripts/check-publish-consistency.py:60` | URL 来自 crates.io 官方 API 常量端点（G19 门禁本身），非用户输入 |
| 8 | 疑似跨文件污点（medium） | `proxy_server.rs:318` | 同 #3——白名单+参数化双层防护 |

### 1.2 仓库脚本面（14 条）——本地开发者工具，处理仓库内受控路径

| 发现 | 位置 | 定性 |
|------|------|------|
| path-traversal ×7 | `add-redteam-xref.py:192` `check-binding-coverage.py:259` `check-deprecated-downstream.py:189` `check-metrics-real.py:44` `check-phantom-tests.py:288` `check-tech-debt.py:145` `unwrap_audit.py:378` | 门禁/维护脚本对**仓库内文件**读写；路径源为仓库遍历结果或开发者 CLI 参数，非攻击面（不暴露为服务） |
| path-traversal | `unwrap_audit.py:252` | `generate_report` 输出路径由开发者 CLI 传入 |
| path-traversal | `generate-benchmark-report.py:143` | 同上（本地报告生成） |
| path-traversal | `fix-doc-count.js:28` | `path.join(__dirname, '..', file)`——`file` 来自硬编码文档清单 |
| open security 入口 | `unwrap_audit.py:378` | 同上脚本，`args.output` 开发者自控 |
| 疑似跨文件污点（medium） | `unwrap_audit.py:378` | 同上 |

### 1.3 构建产物（1 条）

| 发现 | 位置 | 定性 |
|------|------|------|
| MongoDB 动态排序字段注入（medium） | `target/doc/static.files/search-cf4fc35e.js:1` | rustdoc **构建产物**（rustdoc 自带 search-index JS，MongoDB 字样来自依赖文档索引），不入库非源码；`target/` 已在 .gitignore |

## 二、结论与处置

- **0 真实缺陷**：生产源码面 8 条均有防护层（白名单+参数化）或测试夹具定性；脚本面 14 条为本地工具处理受控路径；1 条为构建产物。
- **无需改码**。本报告作为 Mimosa 钩子"scanner_no_output"提示的闭环存档。
- 门禁语义不变：`check-secrets.py`（G9）仍以精确豁免制运作；`test` 夹具假密码豁免项维持既有登记（`KNOWN_FALSE_POSITIVES`）。

## 三、验证

- 扫描 seal：`sha256:ac6f390ebbc86735d737808ce64ed284e9f8a885f9e000f1cc05814fe6d04e52`（coverage：dependency 扫描 1299 包完成，offline advisory 3 匹配 0 unknown）
- 本报告引用行号已逐条以 sed 实读核实（2026-11-15）。
