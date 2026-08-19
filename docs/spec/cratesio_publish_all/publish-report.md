# crates.io 发布报告

## 任务信息
- **任务 ID**: TASK-001
- **执行时间**: 2026-08-19 08:57 ~ 09:52（北京时间）
- **项目**: sz-orm v4.9.0
- **目标**: 将 59 个 lib 包发布到 crates.io
- **结果**: ✅ 全部成功（56 个新发布 + 3 个已存在跳过）

## 汇总统计

| 类别 | 数量 | 占比 |
|------|------|------|
| 成功发布 | 56 | 94.9% |
| 跳过（已存在） | 3 | 5.1% |
| 失败 | 0 | 0% |
| **总计** | **59** | **100%** |

## 成功发布的包（56 个）

按拓扑发布顺序排列，版本号 v4.9.0（除特别说明）：

| # | 包名 | 发布时间 | 备注 |
|---|------|----------|------|
| 1 | sz-orm-adaptive | 08:56 | 首次发布 |
| 2 | sz-orm-audit | 08:57:29 | |
| 3 | sz-orm-auth | 08:57:35 | |
| 4 | sz-orm-crypto | 08:57:41 | |
| 5 | sz-orm-es | 08:57:48 | |
| 6 | sz-orm-grpc | 08:57:57 | |
| 7 | sz-orm-limit | 08:59:09 | |
| 8 | sz-orm-logger | 08:59:37 | |
| 9 | sz-orm-masking | 08:59:57 | |
| 10 | sz-orm-mig | 09:00:21 | |
| 11 | sz-orm-mqtt | 09:00:27 | |
| 12 | sz-orm-n1-lint | 09:00:31 | |
| 13 | sz-orm-postgis | 09:00:35 | |
| 14 | sz-orm-rw | 09:00:43 | |
| 15 | sz-orm-scheduler | 09:01:07 | |
| 16 | sz-orm-search | 09:01:11 | |
| 17 | sz-orm-sharding | 09:01:17 | |
| 18 | sz-orm-sql-validator | 09:01:21 | |
| 19 | sz-orm-storage | 09:01:25 | |
| 20 | sz-orm-timeseries | 09:01:46 | |
| 21 | sz-orm-tracing | 09:01:52 | |
| 22 | sz-orm-websocket | 09:02:01 | |
| 23 | sz-orm-back | 09:02:09 | |
| 24 | sz-orm-anomaly | 09:02:17 | |
| 25 | sz-orm-config | 09:02:20 | |
| 26 | sz-orm-queue | 09:02:25 | |
| 27 | sz-orm-flamegraph | 09:02:37 | |
| 28 | sz-orm-diagnosis | 09:02:41 | |
| 29 | sz-orm-explain | 09:02:45 | |
| 30 | sz-orm-macros | 09:03:05 | proc-macro |
| 31 | sz-orm-core | 09:03:16 | 核心包 |
| 32 | sz-orm-graphql | 09:03:30 | |
| 33 | sz-orm-actix | 09:03:37 | |
| 34 | sz-orm-ai | 09:03:58 | |
| 35 | sz-orm-axum | 09:04:04 | |
| 36 | sz-orm-designer | 09:04:16 | |
| 37 | sz-orm-mssql | 09:04:21 | |
| 38 | sz-orm-oracle | 09:05:45 | |
| 39 | sz-orm-parallel | 09:05:54 | |
| 40 | sz-orm-query-builder | 09:06:05 | |
| 41 | sz-orm-stream | 09:06:13 | |
| 42 | sz-orm-advisor | 09:06:17 | |
| 43 | sz-orm-vector | 09:06:25 | |
| 44 | sz-orm-lc | 09:06:32 | |
| 45 | sz-orm-sqlx | 09:06:36 | |
| 46 | sz-orm-dtx | 09:12:47 | |
| 47 | sz-orm-swagger | 09:12:58 | |
| 48 | sz-orm-batch | 09:13:08 | |
| 49 | sz-orm-health | 09:16:03 | 重试成功（首次网络超时） |
| 50 | sz-orm-observability | 09:16:33 | 重试成功（首次网络超时） |
| 51 | sz-orm-wasm | 09:16:51 | 重试成功（首次 429 速率限制） |
| 52 | sz-orm-fusion | 09:17:08 | 重试成功（首次 429 速率限制） |
| 53 | sz-orm-cabi | 09:21:02 | 重试2成功（修复依赖版本号 + 429） |
| 54 | sz-orm-cpp | 09:31:17 | 重试3成功（修复依赖版本号 + 429） |
| 55 | sz-orm-go | 09:41:34 | 重试4成功（修复依赖版本号 + 429） |
| 56 | sz-orm-java | 09:52:32 | 重试5成功（修复依赖版本号 + 429） |

## 跳过的包（3 个，已存在于 crates.io）

| # | 包名 | 版本 | 说明 |
|---|------|------|------|
| 1 | sz-orm-graph | 0.1.0 | 独立版本线，已发布 |
| 2 | sz-orm-js | 0.1.0 | 独立版本线，已发布 |
| 3 | sz-orm-python | 0.1.0 | 独立版本线，已发布 |

## 失败的包

无。所有失败均在重试后成功。

## 发布过程中的问题与修复

### 问题 1：拓扑排序错误
- **原因**: `topo_sort.py` 跳过了 optional 依赖，但 `cargo publish` 要求 optional 依赖也已在 crates.io
- **修复**: 移除 optional 依赖跳过逻辑，重新生成拓扑顺序
- **文件**: `docs/spec/cratesio_publish_all/topo_sort.py:38-40`

### 问题 2：注释误识别为依赖
- **原因**: `topo_sort.py` 未跳过注释行，注释中的 `sz-orm-core` 被误识别为依赖，导致虚假循环依赖
- **修复**: 添加注释跳过逻辑 `if stripped.startswith("#"): continue`
- **文件**: `docs/spec/cratesio_publish_all/topo_sort.py:39-40`

### 问题 3：依赖缺少版本号
- **原因**: 4 个包的 path 依赖未指定 version 字段，`cargo publish` 要求所有依赖必须有版本号
- **修复**: 为 4 个包的依赖添加 `version = "4.9.0"`
- **文件**:
  - `packages/sz-orm-cabi/Cargo.toml:19` — `sz-orm-sqlx` 添加 version
  - `packages/sz-orm-cpp/Cargo.toml:18` — `sz-orm-cabi` 添加 version
  - `packages/sz-orm-go/Cargo.toml:18` — `sz-orm-cabi` 添加 version
  - `packages/sz-orm-java/Cargo.toml:18` — `sz-orm-cabi` 添加 version

### 问题 4：网络超时
- **包**: sz-orm-health, sz-orm-observability
- **原因**: 上传时网络超时（Operation too slow. Less than 10 bytes/sec transferred the last 30 seconds）
- **解决**: 重试后成功

### 问题 5：crates.io 速率限制（429 Too Many Requests）
- **包**: sz-orm-wasm, sz-orm-fusion, sz-orm-cabi, sz-orm-cpp, sz-orm-go, sz-orm-java
- **原因**: 短时间内发布过多新 crate，触发 crates.io 速率限制
- **解决**: 每次等待 8 分钟后重试，逐个成功

## 验证结果

通过 `cargo search` 验证关键包已上传到 crates.io：

```
FOUND: sz-orm-core
FOUND: sz-orm-macros
FOUND: sz-orm-cabi
FOUND: sz-orm-java
FOUND: sz-orm-cpp
FOUND: sz-orm-go
FOUND: sz-orm-adaptive
```

## 修改的文件清单

| 文件 | 修改内容 |
|------|----------|
| `docs/spec/cratesio_publish_all/topo_sort.py` | 移除 optional 依赖跳过 + 添加注释跳过 |
| `docs/spec/cratesio_publish_all/topo-order.txt` | 重新生成拓扑顺序 |
| `packages/sz-orm-cabi/Cargo.toml` | sz-orm-sqlx 依赖添加 version = "4.9.0" |
| `packages/sz-orm-cpp/Cargo.toml` | sz-orm-cabi 依赖添加 version = "4.9.0" |
| `packages/sz-orm-go/Cargo.toml` | sz-orm-cabi 依赖添加 version = "4.9.0" |
| `packages/sz-orm-java/Cargo.toml` | sz-orm-cabi 依赖添加 version = "4.9.0" |
| `docs/spec/cratesio_publish_all/publish-log.txt` | 发布日志（自动生成） |
| `docs/spec/cratesio_publish_all/publish-range.ps1` | 批量发布脚本 |
| `docs/spec/cratesio_publish_all/publish-retry.ps1` | 重试发布脚本 |

## 结论

✅ **TASK-001 完成**：59 个 lib 包全部处理完毕，56 个成功发布到 crates.io，3 个已存在跳过，0 个失败。