<!--
PR 行为变更审计清单
此模板会在创建 PR 时自动加载。请按顺序勾选每一项。
任何"否"的回答都必须在"说明"区给出理由。
-->

## 一、变更类型

请勾选本次 PR 涉及的变更类型（可多选）：

- [ ] **Bug 修复**（不改变公共 API 行为）
- [ ] **新功能**（新增公共 API）
- [ ] **重构**（不改变行为，仅优化代码结构）
- [ ] **行为变更**（修改了已有公共 API 的契约）
- [ ] **破坏性变更**（修改了已有公共 API 的签名/返回类型/错误类型）
- [ ] **性能优化**
- [ ] **文档/测试**（不改变实现）

## 二、公共 API 影响

如果本次 PR 修改了 `packages/*/src/` 下的公共 API（pub fn / pub struct / pub enum / pub trait），请完成以下检查：

### 2.1 API 变更登记
- [ ] 已在 `docs/api-contracts.md` 中更新契约描述
- [ ] 已运行 `./scripts/audit-api-changes.ps1`（或 `.sh`）并审阅报告

### 2.2 调用方同步
- [ ] 已 grep 全工作空间，列出所有受影响的调用方（见审计脚本输出）
- [ ] 已更新所有调用方以适配新签名
- [ ] 跨包调用方（其他 packages/）已全部更新

### 2.3 测试同步
- [ ] `packages/*/tests/` 下相关测试已更新
- [ ] 已添加/更新契约测试 `packages/sz-orm-core/tests/contracts/`
- [ ] 运行 `cargo test -p sz-orm-core --test contracts` 全部通过

## 三、行为契约检查

如果本次 PR 修改了以下行为，请确认：

### 3.1 返回值变更
- [ ] 返回类型变更（如 `Box<dyn T>` → `PooledConnection`）：所有调用方已适配
- [ ] `Result<T, E>` 的 `E` 变体变更：所有 `match`/`matches!` 调用方已审阅
- [ ] `Option<T>` → `Result<T, E>` 或反向变更：调用方已适配

### 3.2 错误行为变更
- [ ] 错误变体新增/移除/重命名：所有错误处理代码已审阅
- [ ] `panic!` / `unwrap` / `expect` 的添加/移除：已确认是否破坏契约
- [ ] 错误码（如 DB001/PL001）变更：已在 `docs/api-contracts.md` 中更新

### 3.3 生命周期与所有权
- [ ] `&self` → `&mut self` 或反向变更：调用方已适配
- [ ] `Box<T>` / `Arc<T>` / `Rc<T>` 转换：已审阅内存语义
- [ ] `impl Trait` → `dyn Trait` 或反向变更：调用方已适配

### 3.4 异步语义
- [ ] 同步函数 → 异步函数 或反向变更：调用方已适配
- [ ] `Send` / `Sync` 约束变更：跨线程使用方已审阅
- [ ] 取消语义（`Future` drop 行为）变更：已审阅

## 四、并发与状态

如果本次 PR 修改了 `Pool` / `Transaction` / `Cache` 等有状态的组件：

- [ ] 并发场景已测试（`cargo test --test jepsen`、`--test chaos`、`--test stress`）
- [ ] 不变量保持：`close_all` 后 `acquire` 的行为、`commit` 后 `savepoint` 的错误类型等
- [ ] 跨实例状态隔离已验证

## 五、集成门禁

提交 PR 前必须通过：

- [ ] `./scripts/gate.ps1`（Windows）或 `./scripts/gate.sh`（Unix）全部通过
- [ ] `cargo test --workspace` 0 失败
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` 0 警告
- [ ] `cargo fmt --all -- --check` 0 差异

## 六、说明区

对于任何"否"的回答，必须在此说明理由和缓解措施：

```
（在此填写说明）
```

## 七、关联

- 关联 Issue: #
- 关联契约条目: `docs/api-contracts.md` §X.X
- 关联契约测试: `packages/sz-orm-core/tests/contracts/xxx_contract.rs::test_yyy`
