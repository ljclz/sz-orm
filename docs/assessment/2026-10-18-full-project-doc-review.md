# sz-orm 全项目文档深度审查报告

> **审查日期**：2026-10-18
> **审查范围**：全项目所有文档（docs/ + README + CHANGELOG + AGENTS + specs + 各包 README）
> **审查方法**：基于代码实际 + 工具扫描 + 门禁脚本验证
> **审查结论**：✅ **主体健康，发现并修正 2 处文档过时，记录 6 项技术债**

> **⚠️ 复核修正记录（2026-10-18 二次复核）**：本报告初版有 6 处数字/描述与仓库实际不符，已按下表修正。修正依据均为本机实测（git diff / find / grep / 门禁脚本实际输出）。

| # | 位置 | 初版值 | 复核修正值 | 修正依据 |
|---|------|--------|-----------|---------|
| 1 | §1.1 / §8.3 docs 与 assessment 数量 | 293 / 112 | 294 / 113（含本报告） | 本报告自身计入 docs/ 后各 +1 |
| 2 | §1.1 / §1.2 ADR 数量 | 11 个 | 10 个编号 ADR + README.md 索引 + template.md 模板（共 12 个 .md） | `ls docs/adr/` 实测 |
| 3 | §4.1 测试文件 | 295 | 682（tests/ 目录下含 `#[test]`/`#[tokio::test]` 标注的 .rs） | 原值无法复现（文件名口径实测 292），统一为测试标注口径，命令见附录 |
| 4 | §4.3 / §9.4 边界/fuzz 文件 | 20 | 19 | `find` 实测（tests/ 下文件名含 boundary/fuzz/chaos 的 .rs） |
| 5 | §5.1 README.md:4 修正前值 | `v9.8.0 · 15809+ tests` | `v9.4.0 · 20424+ tests` | `git diff HEAD -- README.md` 实测 |
| 6 | §7 G17 孤儿包 | 0 | 17（警告级，整体通过） | `python scripts/check-architecture.py` 实际输出 |

---

## 一、审查范围与文档规模

### 1.1 文档清单

| 类别 | 数量 | 路径 |
|------|------|------|
| docs/ 下 .md 文件 | 294（含本报告；报告创建前为 293） | `docs/` |
| 交付报告 | 113（含本报告；报告创建前为 112） | `docs/assessment/*.md` |
| 版本规格 | 60+ | `.codeartsdoer/specs/*/` |
| 包级 README | 193 | `packages/*/README.md` |
| 根目录文档 | 10 | `*.md` |
| ADR 架构决策 | 10 个编号 ADR（+README.md 索引 + template.md 模板，共 12 个 .md） | `docs/adr/` |
| 交付文档 | 22 | `docs/delivery/` |

### 1.2 docs/ 子目录结构

```
docs/adr/          — 10 个架构决策记录（0001-0009, 0011）+ README.md 索引 + template.md 模板
docs/assessment/   — 112 个交付/审查报告
docs/audit/        — 审计事件
docs/delivery/     — 22 个版本交付文档
docs/integration/  — 集成测试文档
docs/migration/    — 迁移文档
docs/rfc/          — RFC 模板
docs/spec/         — 旧版本规格（v3.4.0-v4.9.0）
```

---

## 二、全项目伪功能/假实现扫描

### 2.1 占位实现扫描

```bash
grep -rn 'todo!\|unimplemented!\|unreachable!' packages --include="*.rs" | grep -v '/target/'
```

**结果：4 处，全部为注释/doc test 示例，零真实占位实现**

| 位置 | 性质 |
|------|------|
| `packages/sz-orm-auth/src/auth.rs:24` | doc comment 示例 |
| `packages/sz-orm-core/src/pool.rs:1989` | doc comment 示例 |
| `packages/sz-orm-core/tests/qb_migration_lint_test.rs:62` | 测试字符串字面量 |
| `packages/sz-orm-sqlx/src/any_driver.rs:861` | 注释提及"无 todo!" |

### 2.2 crate 级 `#![allow(dead_code)]` 扫描

```bash
find packages -path "*/src/*.rs" -exec grep -l '#!\[allow(dead_code)\]' {} \;
```

**结果：src/ 下 0 处（符合用户规则）**

tests/ 下 2 处（测试辅助模块合理用法，有详细注释说明）：
- `packages/sz-orm-core/tests/common/mod.rs:8` — 共享测试工具模块
- `packages/sz-orm-core/tests/v880_whitehat.rs:8` — 白帽测试模块

---

## 三、全项目幻影交付扫描

### 3.1 PHANTOM 检查

```
结果: PHANTOM-1 0 个 | PHANTOM-2 311 个 | 符号通过 38 | 接线断言 4/4
✅ 门禁 15 通过
```

- **PHANTOM-1: 0 个**（无幻影交付）
- PHANTOM-2: 311 个（警告级，feature gate 无成员启用，符合按需启用设计）

---

## 四、全项目测试覆盖

### 4.1 测试统计

| 维度 | 实测值 |
|------|--------|
| `#[test]` 函数 | 17,727 |
| `#[tokio::test]` 函数 | 3,842 |
| **测试函数总计** | **21,569** |
| 测试文件（tests/ 目录下含 `#[test]`/`#[tokio::test]` 标注的 .rs） | 682 |
| **0 测试包** | **0 个**（所有 70 个包都有测试） |

### 4.2 OWASP/安全测试清单（44 个文件）

覆盖 OWASP Top 10 全部：
- A01 访问控制：`owasp_a01_access_control.rs`（auth/core）
- A02 密码学：`owasp_a02_crypto_failures.rs`（config/core/crypto）
- A03 注入：`owasp_a03_injection.rs`（core/lc/swagger）
- A04 不安全设计：`owasp_a04_insecure_design.rs`（core/dtx/grpc/wasm）
- A05 配置错误：`owasp_a05_misconfig.rs`（config/core）
- A06 易受攻击组件：`owasp_a06_vulnerable_components.rs`（core）
- A07 完整性：`owasp_a07_integrity.rs`（audit）
- A08 日志失败：`owasp_a08_logging_failures.rs`（audit/masking）
- A09 认证失败：`owasp_a09_auth_failures.rs`（auth）
- A10 SSRF：`owasp_a10_ssrf.rs`（core/wasm）
- CSRF：`owasp_csrf.rs`（auth）
- XSS：`owasp_xss.rs`（lc）
- 文件上传：`owasp_file_upload.rs`（storage）
- 竞态条件：`owasp_race_conditions.rs`（core/dtx）
- v10.10.0 扩展：`owasp_v10100_*.rs`（17 个攻击向量）

### 4.3 边界/fuzz 测试清单（19 个文件）

- `boundary_*.rs`：边界条件测试
- `fuzz.rs`：模糊测试
- `differential_fuzz.rs`：差分模糊测试（proptest）
- `chaos_boundary_test.rs`：混沌边界测试

---

## 五、文档与代码一致性

### 5.1 已修正的文档不一致

| 位置 | 修正前 | 修正后 | 状态 |
|------|--------|--------|------|
| `README.md:4` | `v9.4.0 · 20424+ tests`（git HEAD 实测值） | `v10.10.0 · 21569+ tests` | ✅ 已修正 |
| `README.md:123` | `~139,000 LOC (src ~115,000 + tests ~20,000)` | `~639,200 LOC (src ~504,900 + tests ~131,400 + benches ~2,700 + examples ~200)` | ✅ 已修正 |

### 5.2 G18 度量真实性复验

```
实际统计: 包=70 成员=72 测试标注=21569 feature=517
✅ README 数字声称与源码统计一致
```

### 5.3 记录为技术债的文档问题

| # | 问题 | 严重度 | 说明 |
|---|------|--------|------|
| 1 | `README.md:44` `v4.3.0 includes 56 workspace members` | 低 | 历史描述，非当前状态 |
| 2 | `README.md:132` `## v3.4.0 New Features` 作为最新特性标题 | 低 | 历史特性段，CHANGELOG 有完整版本线 |
| 3 | `README.md:126` `Minimum Rust version 1.94.0+` vs AGENTS.md `rust-version = "1.81"` | 中 | 需确认实际最低版本 |
| 4 | `docs/delivery/` 缺 v10.6.0-v10.10.0 | 低 | docs/assessment/ 有对应交付记录 |
| 5 | ADR 缺 0010 | 低 | 0001-0009, 0011 连续，0010 未使用 |
| 6 | CHANGELOG 版本缺口（多个中间版本无条目） | 低 | 小版本/修复版本未全部记入 |

---

## 六、TODO.md 未完成项

| # | 项目 | 状态 | 说明 |
|---|------|------|------|
| 1 | 变异测试补跑 | ⚠️ 阻塞 | 沙箱限制，门禁 20 在 CI 环境执行 |
| 2 | 布隆过滤器双实现合并 | 待办 | 阶段 3 架构债，dist_cache vs warmup 双实现 |

---

## 七、门禁脚本验证

| 门禁 | 脚本 | 结果 |
|------|------|------|
| G15 | `check-phantom-delivery.py` | ✅ PHANTOM-1: 0 |
| G16 | `check-semantic-patterns.py` | ✅ 硬规则 0，软规则 7（提示） |
| G17 | `check-architecture.py` | ✅ 通过（无重复实现；孤儿包 17 个，警告级，独立库包属正常形态） |
| G18 | `check-metrics-real.py` | ✅ 包=70 成员=72 测试=21569 feature=517 |
| G19 | `check-publish-consistency.py` | ✅ workspace 10.10.0 |
| G23 | `check-unused-deps.py` | ✅ 无未用依赖 |

---

## 八、版本交付完整性

### 8.1 CHANGELOG 版本线

51 个版本（1.0.0 → 10.10.0），最新 5 个：
- 10.10.0（2026-10-17）
- 10.9.0（2026-10-17）
- 10.8.0（2026-10-17）
- 10.7.0（2026-10-16）
- 10.6.0（2026-10-16）

### 8.2 specs 版本规格

60+ 个版本规格目录，覆盖 v2 → v10.10.0 全部版本。

### 8.3 docs/assessment/ 交付报告

113 个报告（含本报告；报告创建前为 112），覆盖 v8.7.0 → v10.10.0 全部版本。

---

## 九、审查结论

### 9.1 总体结论

**sz-orm 全项目文档深度审查通过，主体健康。**

### 9.2 已修正问题

- README.md 2 处过时度量已修正（版本号、测试数、LOC）

### 9.3 剩余技术债（6 项，均为低/中严重度）

1. README.md 历史版本描述过时（非当前状态，不影响理解）
2. README.md Rust 最低版本不一致（需确认）
3. docs/delivery/ 缺 v10.6.0-v10.10.0（assessment/ 有替代）
4. ADR 缺 0010（编号未使用）
5. CHANGELOG 中间版本缺口（小版本未全部记入）
6. TODO.md 2 项待办（变异测试沙箱限制 + 布隆过滤器合并）

### 9.4 核心指标

| 维度 | 状态 |
|------|------|
| 伪功能/假实现 | ✅ 0 真实占位 |
| crate 级 dead_code | ✅ src/ 下 0 处 |
| 幻影交付 | ✅ PHANTOM-1: 0 |
| 0 测试包 | ✅ 0 个 |
| 未用依赖 | ✅ 0 个 |
| OWASP 覆盖 | ✅ A01-A10 全覆盖 |
| 边界/fuzz 测试 | ✅ 19 个文件 |
| 文档一致性 | ✅ G18 通过（已修正 2 处） |
| 门禁脚本 | ✅ G15-G19 + G23 全通过 |

---

## 附录：审查命令复现

```bash
# 1. 文档枚举
find docs -name "*.md" | wc -l
find packages -name "README.md" -not -path "*/target/*" | wc -l

# 2. 伪功能扫描
grep -rn 'todo!\|unimplemented!\|unreachable!' packages --include="*.rs" | grep -v '/target/'

# 3. crate 级 dead_code 扫描
find packages -path "*/src/*.rs" -exec grep -l '#!\[allow(dead_code)\]' {} \;

# 4. 0 测试包扫描
for d in packages/*/; do
  pkg=$(basename "$d")
  test_count=$(grep -rn '#\[test\]\|#\[tokio::test\]' "$d" --include="*.rs" | grep -v '/target/' | wc -l)
  [ "$test_count" -eq 0 ] && echo "0 测试: $pkg"
done

# 5. OWASP/安全测试清单
find packages -name "*owasp*" -o -name "*security*test*" -o -name "*attack*" -o -name "*pentest*"

# 6. 门禁脚本
python scripts/check-phantom-delivery.py
python scripts/check-semantic-patterns.py
python scripts/check-architecture.py
python scripts/check-metrics-real.py
python scripts/check-publish-consistency.py
python scripts/check-unused-deps.py

# 7. 测试文件统计（tests/ 目录下含测试标注的 .rs 文件，实测 682）
find packages -path "*/tests/*.rs" -not -path "*/target/*" -print0 | xargs -0 grep -l '#\[test\]\|#\[tokio::test\]' | wc -l

# 8. 边界/fuzz 测试文件统计（tests/ 下文件名含 boundary/fuzz/chaos 的 .rs，实测 19）
find packages -path "*/tests/*.rs" \( -name "*boundary*" -o -name "*fuzz*" -o -name "*chaos*" \) -not -path "*/target/*" | wc -l
```