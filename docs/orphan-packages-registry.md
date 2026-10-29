# 孤儿包登记册（Orphan Packages Registry）

> 版本：v10.13.0 | 建立日期：2026-10-28（架构评测 P2-1）
> 权威判定：`python scripts/check-architecture.py`（门禁 17）——孤儿包 = 无任何非 dev 依赖者的包（生产依赖图不可达）
> 本登记册建立后，门禁 17 会校验**每个新出现的孤儿包必须在下表登记**，未登记即告警。

## 背景

72 个 workspace 成员中 17 个包无任何 workspace 内消费者（含 cli/examples）。**孤儿不等于废弃**：绑定层包（java/go/cpp/python/js）的消费者是各语言生态的用户，基准包（bench）的消费者是 CI job——它们不在 workspace 依赖图上是正常形态。真正需要治理的是"无消费者 + 与数据访问层定位漂移"的边缘包。

## 登记表（2026-10-28 裁定）

| 包 | 状态 | 裁定理由 | 真实消费者 |
|----|------|---------|-----------|
| sz-orm-bench | **保留** | 性能基准工具，CI `benchmark` job 直接 `-p sz-orm-bench` 调用 | CI（.github/workflows/ci.yml benchmark job） |
| sz-orm-python | **保留** | PyO3 绑定，用户经 maturin 构建 wheel；bindings.yml 有 python-binding job | Python 生态用户 + CI |
| sz-orm-js | **保留** | napi-rs 绑定，用户经 npm 构建；bindings.yml 有 node-binding job | Node 生态用户 + CI |
| sz-orm-java | **保留** | JNI 绑定（池级 CRUD），Java 用户加载 sz_orm_java 动态库 | Java 生态用户 |
| sz-orm-go | **保留** | Go 绑定（Windows syscall），Go 用户加载 sz_orm_go 动态库 | Go 生态用户（Windows） |
| sz-orm-cpp | **保留** | C++ RAII 池封装，用户链接 sz_orm_cabi | C++ 生态用户 |
| sz-orm-actix | **保留（实验性）** | actix-web 集成层，用户按需 path/crates 依赖 | actix 用户（按需） |
| sz-orm-es | **保留（实验性）** | Elasticsearch 集成（v10.13.0 已迁移 reqwest 0.12） | ES 用户（按需） |
| sz-orm-mqtt | **保留（实验性）** | MQTT 集成（real-broker feature 引入 rumqttc） | MQTT 用户（按需） |
| sz-orm-websocket | **保留（实验性）** | WebSocket 集成 | WS 用户（按需） |
| sz-orm-lc | **保留（实验性）** | 生命周期管理扩展 | 按需 |
| sz-orm-studio | **保留（实验性）** | 开发者 Studio（真实 SQLite 数据源），与 designer 同族工具链 | 开发工具链用户（按需） |
| sz-orm-agent | **冻结** | AI Agent 扩展包；无生产消费者，与 ORM 核心定位漂移 | 无 |
| sz-orm-governance | **冻结** | 数据治理扩展包；无生产消费者 | 无 |
| sz-orm-mcp | **冻结** | MCP 协议扩展包；无生产消费者 | 无 |
| sz-orm-model-ops | **冻结** | 模型运维扩展包；无生产消费者 | 无 |
| sz-orm-multimodal | **冻结** | 多模态（screenshot/sketch/voice）；与数据访问层定位无关 | 无 |

## 冻结语义

- **冻结 ≠ 移除**：包保留在 workspace 中继续参与编译与门禁（避免 72→67 引发连锁改动）；
- 冻结包**不新增 feature、不新增 API**；README/文档不得宣称其"集成/自动"能力（门禁 15）；
- 后续若连续 2 个版本仍无消费者，升级为"移除候选"，在版本公告中预告后移出 workspace。

## 新增孤儿包处置流程

新包进入 workspace 时如无 workspace 内消费者，必须：①在本登记册登记状态与理由；②或调整设计使其被某个生产包消费。门禁 17 会给出未登记孤儿提示。
