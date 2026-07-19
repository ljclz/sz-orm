//! SZ-ORM 公共 API 行为契约测试套件
//!
//! 对应 `docs/api-contracts.md` 中的契约定义。
//!
//! ## 设计目标
//!
//! 与单元测试不同，契约测试**不验证实现细节**，而是验证公共 API 的行为契约：
//! - 不变量（invariants）：调用前后必须成立的状态条件
//! - 状态转换：API 调用导致的状态机迁移
//! - 错误条件：什么输入产生什么错误
//! - 行为变更检测：v0.2.0 的行为变更必须由对应契约测试锁定
//!
//! ## 运行方式
//!
//! ```sh
//! # 单独运行契约测试
//! cargo test -p sz-orm-core --test contracts
//!
//! # 通过集成门禁运行（包括契约测试）
//! ./scripts/gate.ps1   # Windows
//! ./scripts/gate.sh    # Unix
//! ```
//!
//! ## 新增契约测试的规则
//!
//! 任何修改公共 API 行为的 PR 必须同步：
//! 1. 更新 `docs/api-contracts.md` 中对应章节
//! 2. 在本套件中新增/修改契约测试
//! 3. 在 `docs/api-contracts.md` 附录 A 记录行为变更

#![cfg(test)]

#[path = "common/mod.rs"]
mod common;

#[path = "contracts/cache_contract.rs"]
mod cache_contract;
#[path = "contracts/dialect_contract.rs"]
mod dialect_contract;
#[path = "contracts/dynamic_sql_contract.rs"]
mod dynamic_sql_contract;
#[path = "contracts/error_contract.rs"]
mod error_contract;
#[path = "contracts/hooks_contract.rs"]
mod hooks_contract;
#[path = "contracts/migration_contract.rs"]
mod migration_contract;
#[path = "contracts/model_contract.rs"]
mod model_contract;
#[path = "contracts/pool_contract.rs"]
mod pool_contract;
#[path = "contracts/query_contract.rs"]
mod query_contract;
#[path = "contracts/queryable_contract.rs"]
mod queryable_contract;
#[path = "contracts/transaction_contract.rs"]
mod transaction_contract;
#[path = "contracts/value_contract.rs"]
mod value_contract;
