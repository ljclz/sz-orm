# szrsql 线先在项转交反馈（致 szrsql 维护方，2026-11-13）

> **性质**：跨批转交说明（非评测报告）——sz-orm 方言矩阵系列批（OR1~OR6 / OR7~OR10 / FTS 横向 / ALTER INDEX-FK 批二）的四次红队复核均裁定以下三项为「szrsql 发布线先在项，转交 szrsql 维护方处置」。本文档给出每项的精确位置、根因与**可直接执行的修复方案**，供 szrsql 维护方会话直接采纳。
> **为什么转交**：三项均位于 szrsql 发布线的在途面（szrsql 集成测试 / CI szrsql service / cabi SzRSQL 路由），按并行会话协作纪律（硬约束 13「不打包并行会话的半成品改动」）不由方言矩阵批顺手修改，避免 hunk 冲突与审计混装。
> **验收口径**：三项处置后 `python scripts/check-secrets.py` EXIT=0、`python scripts/check-phantom-tests.py` EXIT=0（G30 红牌清除），门禁表 G9/G30 恢复全绿。

---

## 项 1（G9 secrets）：CI szrsql service 连接串被凭据扫描器命中

- **位置**：[.github/workflows/integration.yml:97](../../.github/workflows/integration.yml#L97)（`SZ_ORM_SZRSQL_URL: postgres://postgres:trust@127.0.0.1:6543/postgres`）
- **根因**：`trust` 是 szrsql 本地 trust 认证**模式名**（非密码），但形态与「连接串内嵌凭据」规则匹配（`scanner` 规则：URL userinfo 密码段）。同文件 94/95 行 MySQL/PG 的 `szormtestpwd` 为注册过的测试占位串未命中，两处 `trust` 形态未豁免。
- **推荐处置（二选一）**：
  1. **豁免登记**（推荐，语义正确）：在 `scripts/check-secrets.py` 的豁免机制中登记 `postgres:trust@` 精确形态（前例：2026-10-31 红队批的 `<pw>`/FAKE_VALUE 精确豁免语义——`trust` 为认证模式名，非凭据）。注意豁免必须是**精确形态匹配**而非宽泛放行（防同形态真密码漏检）。
  2. **形态改写**：URL 改为不含 userinfo 的形态（szrsql trust 模式下 pgwire 是否接受无密码连接需 szrsql 侧验证；若必须保留 userinfo，可用 CI 环境变量拼接 `postgres://postgres:${SZRSQL_TEST_AUTH}@...` 使扫描器命中的是变量引用而非字面量）。
- **连带项**：`packages/sz-orm-sqlx/tests/integration_szrsql_kill_recover.rs:74` 同形态命中（`format!("postgres://postgres:trust@127.0.0.1:{PORT}/postgres")`）——处置 1 时两处一并豁免；处置 2 时测试侧改用与 CI 相同的拼接形态。

## 项 2（G9 secrets）：szrsql kill_recover 测试连接串同型命中

- **位置**：[packages/sz-orm-sqlx/tests/integration_szrsql_kill_recover.rs:74](../../packages/sz-orm-sqlx/tests/integration_szrsql_kill_recover.rs#L74)
- **根因/处置**：同项 1（同形态、同豁免或同改写）。
- **附注**：该测试为 szrsql 上游 9 项修复闭环复测时新增的池故障恢复测试（M 批交付），修复时勿动断言语义。

## 项 3（G30 幻影测试）：cabi SzRSQL DSN 归一化测试零断言

- **位置**：[packages/sz-orm-cabi/src/lib.rs:2636](../../packages/sz-orm-cabi/src/lib.rs#L2636)（`test_pool_new_szrsql_dsn_normalized_to_pg`）
- **根因**：测试意图是验证「`szrsql://` 路由语义（不再被当作非法 DSN 直接拒绝）」，但归一化逻辑内联在 `sz_orm_pool_new`（[lib.rs:289-293](../../packages/sz-orm-cabi/src/lib.rs#L289)），唯一可观测出口是「连接成功与否的指针」——本机无 6543 服务时不可判定，测试退化为只调不 assert（`if !pool.is_null() { free }`），被门禁 30 P3「零断言」硬规则命中。
- **推荐处置（结构化修复，正解）**：
  1. 把归一化逻辑提取为独立纯函数（不依赖网络/运行时）：

     ```rust
     /// szrsql:// DSN → postgres:// 归一化（非 szrsql 前缀返回 None）
     fn normalize_szrsql_dsn(dsn: &str) -> Option<String> {
         if dsn.to_lowercase().starts_with("szrsql://") {
             Some(format!("postgres://{}", &dsn["szrsql://".len()..]))
         } else {
             None
         }
     }
     ```

     `sz_orm_pool_new` 内改为调用该函数（行为零变化）。
  2. 测试改为直接断言归一化语义（真断言、零网络依赖）：

     ```rust
     #[test]
     fn test_szrsql_dsn_normalization() {
         assert_eq!(
             normalize_szrsql_dsn("szrsql://postgres:trust@127.0.0.1:6543/postgres"),
             Some("postgres://postgres:trust@127.0.0.1:6543/postgres".to_string())
         );
         assert_eq!(normalize_szrsql_dsn("sqlite://x.db"), None);
         assert_eq!(normalize_szrsql_dsn("mysql://x"), None);
         // 大小写不敏感前缀
         assert!(normalize_szrsql_dsn("SzRSQL://x").is_some());
     }
     ```

     原零断言测试删除或改造为此形态（保留 `sz_orm_pool_new` 全链路探针可作为附加 `#[ignore]` 测试，但必须有可判定断言）。
  3. 顺带收益：归一化语义从「仅集成面可观测」升级为「单元面可回归」，后续 szrsql DSN 变更（如带 query 参数的形态）有独立回归点。
- **注意**：`sz_orm_pool_new` 改造涉及 C ABI 面与 go/java 绑定转发路径（同口径注释见 [lib.rs:285-288](../../packages/sz-orm-cabi/src/lib.rs#L285)），属 szrsql 发布线在途面——故转交而非方言批顺手修改。

## 附：修复验收清单（szrsql 维护方执行后逐项留输出）

1. `python scripts/check-secrets.py`：EXIT=0（项 1/2 清除；若走豁免登记，`grep -c "trust@" scripts/check-secrets.py` 应含精确形态条目）
2. `cargo test -p sz-orm-cabi --lib`：全过（项 3 新断言转绿）
3. `python scripts/check-phantom-tests.py`：EXIT=0（G30 红牌清除）
4. `cargo test -p sz-orm-sqlx --test integration_szrsql_kill_recover -- --ignored`：行为不回归（若 SZ_ORM_SZRSQL_URL/6543 就绪）
5. 门禁 27（check-gate-scripts.py）：若 check-secrets.py 修改，自测须同步（该脚本会校验 G9 辅助脚本接线）

## 转交来源批次链（证据锚点）

| 批次 | 报告 | 回执项 |
|------|------|--------|
| OR1~OR6 矩阵 | [2026-11-11-oracle-dialect-matrix.md](2026-11-11-oracle-dialect-matrix.md) §4 门禁表 | 红队 R-2（secrets）/R-3（G30） |
| OR7~OR10 边界 | [2026-11-12-oracle-dialect-boundary.md](2026-11-12-oracle-dialect-boundary.md) §4 | R-3 同源 |
| FTS 横向 | [2026-11-12-fts-quote-sweep.md](2026-11-12-fts-quote-sweep.md) §4 | 同源持续披露 |
| ALTER 批二 | [2026-11-12-alter-index-fk-quote-sweep.md](2026-11-12-alter-index-fk-quote-sweep.md) §4 | 同源持续披露 |
