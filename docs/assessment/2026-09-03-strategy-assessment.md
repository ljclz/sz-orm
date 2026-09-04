# SZ-ORM 战略评估报告（2026-09）

> **用途**：为 v6.x~v7.x 开发方向提供决策依据。
> **数据基准**：workspace v5.1.0（CHANGELOG 已有 6.0.0 条目未发版）· 68 包 · main @ 07f920b
> **竞品数据**：2026-09 联网调研（版本号/star/活跃度为当期快照）
> **关联文档**：docs/sz-orm与同类产品对比分析.md（v5.0.0 基准，2026-08-22）、docs/sz-orm-maturity-roadmap.md（58 包口径，2026-08-15）

---

## 1. 现状快照（2026-09-03 实测）

| 维度 | 数据 | 备注 |
|------|------|------|
| 版本 | Cargo.toml 5.1.0 / CHANGELOG 已写 6.0.0 | ⚠️ 版本未同步，6.0.0 未打 tag |
| 规模 | 68 lib 包 + cli + examples = 70 成员 | README 写 63、对比文档写 61，口径漂移 |
| 测试 | 11,557 个 `#[test]`（G4 实跑 10,372 passed） | 含 `#[tokio::test]` 后更多 |
| 方言 | DbType 28 种枚举，含信创 6 种（达梦/Kingbase/OceanBase/PolarDB/GaussDB/GBase） | 竞品无信创支持 |
| 质量门禁 | 23 道门禁全落地，2026-09-03 全量审查 21 关通过 | 本仓库独有工程体系 |
| 生产试点 | sz-pay（真实生产）+ CLI 工具 + 多语言绑定 3 案例 | 均为内部项目 |
| crates.io | 仅 sz-orm-core 发布过 | 其余 67 包未发布 |

## 2. 竞品格局（2026-09 调研）

| 竞品 | 版本 | Stars | 定位 | 与 sz-orm 关系 |
|------|------|-------|------|---------------|
| Diesel | 2.3.12（08-07） | 14.2k | 同步 ORM 标杆，编译期 DSL | 类型安全路线参照 |
| SeaORM | 2.0.2（08-12） | 9.9k | 异步动态 ORM，2.0 大版本落地 | **最直接竞品** |
| sqlx | 0.9.0（05-21） | 17.4k | 异步 SQL 工具包（1.4 亿下载） | sz-orm-sqlx 构建其上 |
| Prisma | 7.10（08-25） | 47.6k | **已放弃 Rust 引擎转 WASM** | 验证嵌入动态语言路线失败 |
| GORM | 1.31.2（06-25） | 39.9k | Go 生态标准 | API 风格参照 |
| SQLAlchemy | 2.0.52（08-11） | 12.1k | Core+ORM 双层鼻祖 | 架构形态最可类比 |
| rbatis | 4.9.7（07-29） | 2.5k | 自研池+动态 SQL | 最接近的 Rust 国产竞品，无绑定层/AI |
| toasty | 0.10.0 | 3.1k | tokio 官方新 ORM（SQL+NoSQL） | 潜在未来威胁，尚早期 |

**关键结论**：
1. **Rust 生态没有"全家桶 ORM"竞品**——带自研连接池 + 28 方言 + 6 语言绑定 + 图数据库 + AI 辅助的 70-crate 形态是独一份。
2. **多租户原生支持**（multi-tenant-enhanced：RLS/配额/审计/连接级租户）在 Rust ORM 中是稀缺能力，竞品全部靠手写。
3. **Rust nl2sql 生态空白**——现有 crate 全是玩具级（下载量 <1000），AI+DB 主流收敛为 MCP server。sz-orm 的 AI 全家（nl2sql/agent/nl-query/model-ops/multimodal/governance）若做实是先发卡位。

## 3. 优劣势分析

### 3.1 竞争力（护城河）

| # | 竞争力 | 证据 | 竞品对照 |
|---|--------|------|----------|
| 1 | **自研连接池性能** | bench：比 sqlx 快 8.7x、比 sea-orm 快 17.5x（2026-08-22 实测） | SeaORM 直接复用 sqlx 池 |
| 2 | **信创方言 6 种** | DbType::Dameng/Kingbase/OceanBase/PolarDB/GaussDB/GBase | 全生态独有，国产化采购刚需 |
| 3 | **多租户原生** | multi-tenant-enhanced + tenant_quota_rls（G22 覆盖率 100%） | 竞品零原生支持 |
| 4 | **AI 全栈先发** | 6 个 AI 包（ai/agent/nl-query/model-ops/multimodal/governance）+ LLM 真实接入 | Rust 生态空白 |
| 5 | **23 道门禁工程体系** | 幻影交付/语义反模式/度量真实性等门禁为 AI 协作开发量身定做 | 无竞品有此体系 |
| 6 | **编译期 DSL 广度** | 88 种表达式（对比文档宣称）vs Diesel ~38 种 | 广度领先，深度待验 |
| 7 | **多语言绑定** | C/Java/Go/C++/Python/JS 六语言 | Rust ORM 独有 |

### 3.2 劣势（诚实清单）

| # | 劣势 | 严重度 | 说明 |
|---|------|--------|------|
| 1 | **单作者 + 无社区** | 高 | Diesel/SeaORM/sqlx 均多人维护；sz-orm 全部 commit 出自 1 人（含 AI 协作） |
| 2 | **生产案例单薄** | 高 | 仅 sz-pay 一个真实生产案例；README 自评 "Early production ready (internal)" |
| 3 | **67/68 包未发布 crates.io** | 高 | README "published on crates.io" 宣称与实际严重不符 |
| 4 | **方言广而不深** | 中 | 28 方言枚举，但真实驱动集成的约 10 种；Informix/Firebird/SapHana 等 SQL 生成 only |
| 5 | **版本/文档口径漂移** | 中 | 6.0.0 未发版、包数 3 种口径、测试数 3 种口径 |
| 6 | **API 稳定性欠账** | 中 | v5→v6 连续破坏性变更（M2 简化、migration-guide-v5-to-v6），下游迁移成本高 |
| 7 | **依赖树健康** | 低 | h2/chacha20 等传递依赖漏洞靠豁免维持，audit 基线 15 条豁免偏多 |
| 8 | **AI 包真实性风险** | 中 | 6 个 AI 包 2026-09-02 集中交付，需按门禁 15 标准持续验证调用点 |

## 4. 差距与缺失（按风险排序）

### P0 — 信任问题（不修则一切竞争力无意义）

1. **发布诚信**：把 README 的 "published on crates.io" 修正为实际状态；或完成 6.0.0 真实发布（至少 ai/agent/nl-query 三个 AI 包 + core）。
2. **版本收敛**：Cargo.toml 5.1.0 vs CHANGELOG 6.0.0 二选一收敛，打 git tag。
3. **文档口径统一**：包数（63/61/68）、测试数（10200+/11557/12678）全仓库单一口径，由 check-metrics-real.py 强制。

### P1 — 竞争力兑现（把"独有"变成"可用"）

4. **AI 包打实**：nl2sql 端到端 demo（连真实 MySQL 跑通 NL→SQL→结果→图表）；按门禁 15 逐包验证调用点；发布 MCP server 形态（sz-orm-mcp）——这是 2026 AI+DB 主流形态。
5. **信创方言真实驱动**：达梦/Kingbase 至少一种接真实驱动（JDBC 桥 or C 驱动 FFI），否则信创优势停留在枚举。
6. **SeaORM 2.0 对标补课**：2.0 的 derives 重构、streaming 查询、连接池指标——逐项对照补齐 sz-orm 对应能力并写对比表。

### P2 — 生态与长期

7. **第二生产案例**：sz-pay 之外再落地 1 个真实项目（哪怕内部工具），消灭"单案例"弱点。
8. **toasty 跟踪**：tokio 官方 ORM 若成熟，SQL+NoSQL 统一 API 会成为新标准，需每季度评估。
9. **依赖树瘦身**：audit 豁免 15 条逐条复核，能升级的升级；G6 每月跑一次。
10. **成熟度路线图收尾**：25 个 🟡 包按 roadmap 升 ✅（估 15-20 工作日），68 包口径重写 roadmap。

## 5. 定位建议（v6/v7 战略）

**当前定位**："Rust 全栈数据访问框架（内部生产就绪）"——大而全但信任度不足。

**建议转向**：**"AI 时代的信创数据访问层"** 双轮定位：
- **轮 1（对内/信创市场）**：多租户 + 信创方言 + 生产就绪检查，服务国产化 Rust 项目。这是竞品完全空白、外部 ORM 无法快速复制的组合。
- **轮 2（对社区/AI 市场）**：sz-orm-mcp + nl2sql 闭环作为开源钩子，吃 Rust AI+DB 空白期红利。Prisma 放弃 Rust 引擎留下的空档，恰好是"Rust 原生数据工具"的机会窗口。

**收缩建议**：六语言绑定（C/Java/Go/C++/Python/JS）维护成本 > 收益，建议冻结新功能只保 bugfix，把精力让给 AI 轮。

## 6. 下一步开发路线图（建议 v6.0.0 → v6.1 → v7）

| 里程碑 | 内容 | 工作量 | 验收标准 |
|--------|------|--------|----------|
| **v6.0.0 发版周** | 版本收敛（Cargo.toml=6.0.0）+ tag + README 口径修正 + ai/agent/nl-query 发布 crates.io | 2-3 天 | G19 一致性全绿；crates.io 可见 4 包 |
| **v6.0.x AI 打实** | nl2sql 端到端 demo（真实 MySQL）+ sz-orm-mcp server + AI 6 包门禁 15 逐包验证 | 5-7 天 | demo 视频可复现；MCP 可被 Claude/Cursor 调用 |
| **v6.1 信创** | 达梦或 Kingbase 真实驱动 POC（选一种）+ e2e 集成测试入 G7 | 5-8 天 | 真实 DB 上 CRUD/事务/迁移 8 类核心路径通过 |
| **v6.2 对标** | SeaORM 2.0 能力对照表 + 缺口补齐（streaming、池指标） | 3-5 天 | 对比文档 v6 版更新 |
| **v7 方向决策** | 按信创市场反馈 + toasty 成熟度再定：分布式深化 or AI 深化 or 生态建设 | 季度评审 | — |

## 7. 一句话总结

**sz-orm 拥有 Rust 生态独一无二的能力组合（信创方言 + 多租户 + AI 全栈 + 自研高性能池），但这些优势当前全部停留在"内部可验证"状态；v6 的核心任务不是继续加宽（68 包已够宽），而是把独有能力做实、发出、可被外部验证——先兑现信任，再谈竞争力。**
