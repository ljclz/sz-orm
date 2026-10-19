# v10.8.0 T1：crates.io 发布 v10.7.0 交付记录

> 版本：v10.8.0 任务 T1
> 日期：2026-10-16
> 目标：69 个可发布包全部在 crates.io 上确认 v10.7.0

## 1. 发布概况

| 指标 | 数值 |
|------|------|
| workspace 版本 | 10.7.0（[Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6)） |
| 可发布包总数 | 69（排除 sz-orm-examples 私有、sz-orm-js/sz-orm-python 独立 0.1.0 版本线） |
| 本次新发布 | 25 |
| 已存在（already exists） | 44 |
| 失败 | 0 |
| 总计确认 | 69/69 ✅ |

## 2. 本次新发布 25 包

| # | 包名 | 状态 |
|---|------|------|
| 1 | sz-orm-actix | ✅ success |
| 2 | sz-orm-advisor | ✅ success |
| 3 | sz-orm-agent | ✅ success |
| 4 | sz-orm-ai-designer | ✅ success（重试后） |
| 5 | sz-orm-ai-migration | ✅ success（重试后） |
| 6 | sz-orm-anomaly | ✅ success（重试后） |
| 7 | sz-orm-bench | ✅ success |
| 8 | sz-orm-cli | ✅ success |
| 9 | sz-orm-cpp | ✅ success |
| 10 | sz-orm-designer | ✅ success（重试后） |
| 11 | sz-orm-diagnosis | ✅ success（重试后） |
| 12 | sz-orm-flamegraph | ✅ success |
| 13 | sz-orm-governance | ✅ success（重试后） |
| 14 | sz-orm-graph | ✅ success |
| 15 | sz-orm-java | ✅ success（重试后） |
| 16 | sz-orm-lc | ✅ success |
| 17 | sz-orm-lsp | ✅ success |
| 18 | sz-orm-mcp | ✅ success |
| 19 | sz-orm-model-ops | ✅ success |
| 20 | sz-orm-n1-lint | ✅ success |
| 21 | sz-orm-nl-query | ✅ success（重试后） |
| 22 | sz-orm-parallel | ✅ success |
| 23 | sz-orm-query-builder | ✅ success（重试后） |
| 24 | sz-orm-stream | ✅ success |
| 25 | sz-orm-wasm | ✅ success |

## 3. 已存在 44 包（crates.io 上 v10.7.0 已发布）

sz-orm-adaptive, sz-orm-ai, sz-orm-audit, sz-orm-auth, sz-orm-axum, sz-orm-back, sz-orm-batch, sz-orm-cabi, sz-orm-config, sz-orm-core, sz-orm-crypto, sz-orm-dtx, sz-orm-es, sz-orm-explain, sz-orm-fusion, sz-orm-go, sz-orm-graphql, sz-orm-grpc, sz-orm-health, sz-orm-limit, sz-orm-logger, sz-orm-macros, sz-orm-masking, sz-orm-mig, sz-orm-mqtt, sz-orm-mssql, sz-orm-multimodal, sz-orm-observability, sz-orm-oracle, sz-orm-postgis, sz-orm-queue, sz-orm-rw, sz-orm-scheduler, sz-orm-search, sz-orm-sharding, sz-orm-sql-validator, sz-orm-sqlx, sz-orm-storage, sz-orm-studio, sz-orm-swagger, sz-orm-timeseries, sz-orm-tracing, sz-orm-vector, sz-orm-websocket

## 4. 未参与发布 3 包

| 包名 | 版本 | 原因 |
|------|------|------|
| sz-orm-examples | 10.7.0 | publish = []（私有，不发布） |
| sz-orm-js | 0.1.0 | 独立版本线 |
| sz-orm-python | 0.1.0 | 独立版本线 |

## 5. 发布过程

### 5.1 发布策略

- 使用 `cargo publish -p <包名> --allow-dirty` 逐包发布
- 每包间隔 3 秒避免速率限制
- 遇 429 Too Many Requests 时等待 65 秒后重试
- "already exists on crates.io index"（400）视为已发布，跳过

### 5.2 速率限制处理

crates.io 对已存在包的更新有速率限制（约 10 次/分钟）。发布过程中多次触发 429，通过 65 秒等待 + 重试全部解决。

### 5.3 文件锁处理

sz-orm-adaptive 和 sz-orm-ai 在首次尝试时遇到 "Blocking waiting for file lock on package cache"（并发 cargo 进程冲突），重试后均确认已存在于 crates.io。

## 6. 证据

- workspace 版本声明：[Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6) `version = "10.7.0"`
- 发布日志：`/tmp/publish_final_log.txt`（69 包逐包发布结果）
- 发布脚本：`/tmp/publish_all_v1070.sh`（含全部 69 包列表和重试逻辑）
- crates.io 确认：44 包返回 "already exists on crates.io index"，25 包返回 success

## 7. 验证方法

```bash
# 验证 workspace 版本
grep 'version = "10.7.0"' Cargo.toml  # 应匹配第 6 行

# 验证包数量
cargo metadata --no-deps --format-version 1 | python -c "
import json, sys
data = json.load(sys.stdin)
pkgs = [p['name'] for p in data['packages'] if p.get('publish') is None and p['version'] == '10.7.0']
print(f'可发布 v10.7.0 包数: {len(pkgs)}')  # 应为 69
"
```

## 8. 结论

T1 完成。69 个可发布包全部在 crates.io 上确认 v10.7.0（25 新发布 + 44 已存在），0 失败。独立版本线 sz-orm-js/sz-orm-python 0.1.0 和私有包 sz-orm-examples 不参与本次发布。