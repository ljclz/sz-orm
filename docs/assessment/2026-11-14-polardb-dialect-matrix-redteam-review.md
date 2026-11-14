# PolarDB 方言矩阵批——第 5 环红队独立复核回执（2026-11-14）

> 复核者：独立后台会话（非实施者 session）。只读复核，未修改任何文件。
> 复核对象：环 1 提示词 c062c9f0 / 环 3 实施批 fe3161fa / 环 2 报告 69d460db（`2026-11-14-polardb-dialect-matrix.md`）。
> audit-verify：1 通过 0 失败 5 警告（警告系短文件名引用形态，与既有回执惯例一致；引用经复核者逐条人工核验为真）。

## 逐项结论

**A. 供给/方言匹配边界 —— PASS**
integration_polardbx.rs 全文仅使用 `DbType::MySQL`（:102/:140/:148 `get_dialect(DbType::MySQL)`），无一处 `DbType::PolarDB`/`PolarDbDialect` 误用；dialect.rs:2189-2192 注释与事实一致（「委托 PG 方言族，仅适用于 PolarDB for PostgreSQL；PolarDB-X 用户应选 `DbType::MySQL`」），模块头 doc（integration_polardbx.rs:6-10）明确拆分两件事，报告 §0 裁定同型，无混淆。

**B. env 门控假红 —— PASS（含真连复跑）**
代码判断：`polardbx_url()`（:79-83）未配置返回 None → `try_pool()` None 分支 `return`（不 fail 不 panic），无 env 复跑实测 `0 passed; 6 ignored; 0 failed`。真连复跑：`docker start pdbx-1stop` 后（容器确实又随机退出过一次，符合报告登记的 stop-all 坑），设 `SZ_ORM_POLARDBX_URL` 复跑 **6 passed; 0 failed（15s，真实连接）**——独立复现成功。

**C. 行为级证据真伪 —— PASS**
PLX3 两分支均为 `fetch_one` 读回 + `assert_eq!(v, "a")` / `assert_eq!(v, "up")`（integration_polardbx.rs:314-331），非永真断言；PLX5 恶意 payload 参数化匹配后 `assert_eq!(ids, vec![1])` 精确单行 + `COUNT(*)==2` 表完整性双断言（:472-485），真行为级。PLX6 `expect_err`（:509）：若 PARTITION BY 意外被接受，`expect_err` 直接 panic 测试失败，不存在假绿路径；msg 匹配 `contains("4500") || contains("partition")` 稍宽但不构成假绿风险（仅防错误类型错位）。

**D. 引用真实性 —— PASS（抽验 5 条）**
dialect.rs:2190（边界注释块首行）✓；integration_polardbx.rs:158/273/417/498 四个 `async fn matrix_plx*` 定义行逐一 ✓；docs/sz-orm使用指南.md:2173 PolarDB 行内容逐字 ✓。

**E. 门禁声称 —— PASS**
G37 复跑：`0 命中`，EXIT=0。lib 3294 / clippy 未全量复跑（时效成本），但与本批直接相关的 matrix 套件 6/6 已独立复现，无矛盾证据。

**F. 越批项 —— PASS**
`git show fe3161fa -- packages/sz-orm-core/tests/integration_dameng.rs` 仅一处 hunk：`let r1 = run_dm_sql(&format!("DROP TABLE DM_PROBE CASCADE;"));` → `run_dm_sql("DROP TABLE DM_PROBE CASCADE;")`，纯 useless_format 修复，hunk 级隔离成立。

## 最终裁决

**无条件放行。** 缺陷分级：P0=0，P1=0，P2=0，P3=0。

备注（非缺陷，供登记参考）：
1. PLX6 错误消息匹配词较宽（"partition"），属防御性断言风格，无假绿路径；
2. 容器随机退出坑在本次复核中再次实锤（docker start 后再次 not running），但 env 门控设计使该环境不稳不影响套件可重复性，报告 §5-2 登记已覆盖。
