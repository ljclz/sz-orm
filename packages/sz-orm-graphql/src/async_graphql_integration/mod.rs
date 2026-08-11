//! # GraphQL 深度集成
//!
//! 提供 `AsyncGraphqlBridge`（async-graphql Schema 对接 + DataLoader）、
//! Subscription（基于 CDC ChangeEvent）、Relay 分页、Federation 联邦 schema、
//! 工单化错误处理。

pub mod bridge;
pub mod error;
pub mod federation;
pub mod relay;
pub mod subscription;

pub use bridge::AsyncGraphqlBridge;
pub use error::{ErrorCategory, TicketError};
pub use federation::FederationGateway;
pub use relay::{relay_paginate, PageInfo, RelayConnection, RelayEdge};
pub use subscription::SubscriptionSource;
