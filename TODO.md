# sz-orm 待办清单

> 自动生成：2026-08-16
> 最后更新：2026-10-06

## 文档同步

### ✅ 已完成：对比文档同步 cabi 新 API

**来源**：门禁 14 文档同步检查触发

**状态**：✅ 已完成（2026-10-06）

**完成说明**：
- `docs/sz-orm与同类产品对比分析.md` §4.1 C ABI 行已补充 transaction/batch API 描述（事务 `begin/commit/rollback/execute/free`、model 事务变体 `insert_tx/update_tx/delete_tx/find_tx`、批量 `execute_batch`）
- cabi 36 个 `#[no_mangle]` 导出与文档一致

---

## 依赖清理

### 已完成：63 个未用依赖登记

**状态**：✅ 已完成（2026-08-16）

**详情**：
- 34 个包的 63 个未用依赖已登记到 `[package.metadata.cargo-machete]`
- 门禁 23 从 66 个警告降至 1 个（bench-comparison 包的 libsqlite3-sys）

**剩余 1 个警告**：✅ 已解决（2026-10-06）
- `bench-comparison` 包的 `libsqlite3-sys`：源码无引用，已从 `bench-comparison/Cargo.toml` 移除直接依赖及冗余的 cargo-machete 豁免登记；该包编译验证通过

---

## 发布前检查

### 待办：README 测试数更新

**状态**：✅ 已完成（2026-08-16）

**详情**：
- README.md 4 处测试数 9205 → 9387
- 实际统计：7,882 `#[test]` + 1,505 `#[tokio::test]` = 9,387

---

### 待办：变异测试补跑

**状态**：⚠️ 阻塞（沙箱限制）

**原因**：
- 门禁 20 变异测试尝试访问 `F:\test\data\.probe_*` 目录
- 被 TRAE 沙箱拦截

**解决方案**：
1. 配置沙箱规则允许访问该目录
2. 或在沙箱外运行 `python scripts/check-mutation-coverage.py`

---

## 架构债

### 待办：布隆过滤器双实现合并

**来源**：门禁 17 架构一致性扫描

**详情**：
- `dist_cache` 包的击穿守卫布隆过滤器
- `warmup` 包的穿透过滤布隆过滤器
- 合并列为阶段 3 架构债

**预计处理时间**：v4.10.0 规划

---

## 文档修正（已完成）

### ✅ 对比分析文档行号错位与 LOC 虚高修正

**提交**：`a9db540 fix(docs): 修正对比分析文档行号错位与 LOC 虚高`

**修正内容**：
- 行号错位 4 处（keyset/sql-verify/pool/dialect-security）
- LOC 虚高：310,980 → 284,202（排除 target/ 实测）
- 测试数：9,242 → 9,305 → 9,387（最终口径）
- 58 个包 LOC 全部更新
- 派生宏：17 → 20
- 状态标记：sz-orm-masking 🔵 → 🟡

**验证**：
- `check-doc-consistency.py` PASS
- Grep 扫描旧值残留：零残留
