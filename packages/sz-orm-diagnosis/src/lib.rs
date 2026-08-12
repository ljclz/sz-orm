//! # sz-orm-diagnosis — 慢查询自动诊断报告
//!
//! 基于 `slow-query-diagnosis` feature，`SlowQueryDiagnoser` 基于阶段耗时占比
//! 判定根因（PoolExhaustion/SqlInefficiency/LargeResultSet/BuildOverhead/MixedCause），
//! 仅对 slow==true 触发，与优化建议联动。
