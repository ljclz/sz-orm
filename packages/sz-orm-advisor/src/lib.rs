//! # sz-orm-advisor — 查询自动优化2.0 建议引擎 + 智能闭环联动
//!
//! 基于 `query-advisor` feature，规则引擎分析 EXPLAIN 计划 + 自适应统计，
//! 生成六种可执行优化建议（AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize）。
//!
//! 基于 `query-intelligence-loop` feature，串联 EXPLAIN → 自适应 → 诊断 → 建议 四步闭环。
