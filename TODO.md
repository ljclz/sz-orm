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
---

## 文档技术债修正（2026-10-18 完成）

### ✅ 已完成：README.md 过时度量修正

**状态**：✅ 已完成（2026-10-18）

**修正内容**：
- `README.md:4` 版本 `v9.4.0/20424+` → `v10.10.0/21569+`
- `README.md:8` 测试徽章 `20437+` → `21569+`
- `README.md:44` `v4.3.0 includes 56 workspace members` → `v10.10.0 includes 72 workspace members`
- `README.md:122-123` 测试数 `20437` → `21569`、LOC `~139,000` → `~639,200`
- `README.md:126` Rust 最低版本 `1.94.0+` → `1.81.0+`（与 Cargo.toml `rust-version = "1.81"` 一致）

**验证**：G18 度量真实性通过

---

### ✅ 已完成：docs/delivery/ 缺口补齐

**状态**：✅ 已完成（2026-10-18）

**修正内容**：
- 新增 `docs/delivery/v10.6.0-delivery.md`（指向 assessment/ 交付记录）
- 新增 `docs/delivery/v10.7.0-delivery.md`
- 新增 `docs/delivery/v10.8.0-delivery.md`
- 新增 `docs/delivery/v10.9.0-delivery.md`
- 新增 `docs/delivery/v10.10.0-delivery.md`

---

### ✅ 已完成：ADR 0010 补齐

**状态**：✅ 已完成（2026-10-18）

**修正内容**：
- 新增 `docs/adr/0010-workspace-包数量冻结在72.md`（记录"不新增 crate"架构决策）
- 更新 `docs/adr/README.md` 索引添加 0010 条目

---

### 待办：CHANGELOG 中间版本缺口

**状态**：⚠️ 低优先级（后续版本补充）

**详情**：
- CHANGELOG 有 51 个版本条目，specs 有 60+ 个版本目录
- 部分小版本/修复版本未记入 CHANGELOG
- 不影响功能，仅影响版本历史完整性

---

### ✅ 已完成：布隆过滤器双实现合并

**状态**：✅ 已完成（v4.7.0 已合并）

**详情**：
- `packages/sz-orm-core/src/bloom.rs:3` — v4.7.0 架构债清零：合并双实现为单一公共模块
- `packages/sz-orm-core/src/dist_cache.rs:648` — BloomFilterGuard 已复用 `crate::bloom::BloomFilter`
- 原 bloomfilter crate 依赖已移除

---

### ✅ 已确认：变异测试补跑

**状态**：✅ 本机可运行（2026-10-18 确认）

**详情**：
- cargo-mutants 27.1.0 本机可用，非沙箱限制
- sz-orm-config 包 442 变异体实测：基线通过，4 个 MISSED（可后续改进）
- 全量变异测试耗时较长，CI 环境执行（门禁 20 实测数据采集日期 2026-09-23）
