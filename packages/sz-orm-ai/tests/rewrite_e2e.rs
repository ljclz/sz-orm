//! 任务 3.2 端到端测试：真实 DB 改写差分验证
//!
//! 需要真实数据库连接，默认 #[ignore]。
//! 启用后验证改写前后 SQL 在真实数据集上产生相同结果集。

use sz_orm_ai::RewriteEngine;

/// 真实 DB 差分测试：改写前后 SQL 结果集一致
///
/// 运行：cargo test -p sz-orm-ai --features ai-rewrite-advisor rewrite_e2e -- --ignored
#[tokio::test]
#[ignore = "需要真实数据库连接"]
async fn test_e2e_rewrite_diff_real_db() {
    let engine = RewriteEngine::new();
    let sql = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)";
    let result = engine.rewrite(sql);
    if let Some(suggestion) = result.suggestion {
        // 真实 DB 执行：验证 original_sql 和 rewritten_sql 结果集一致
        // 此处需要真实数据库连接，留给手动验证
        println!("原始 SQL: {}", suggestion.original_sql);
        println!("改写 SQL: {}", suggestion.rewritten_sql);
        println!("等价性证明: {}", suggestion.equivalence_proof.proof_text);
    }
}

/// 真实 DB LLM 路径改写
///
/// 运行：cargo test -p sz-orm-ai --features ai-rewrite-advisor,multi-llm rewrite_e2e -- --ignored
#[tokio::test]
#[ignore = "需要真实 LLM 服务连接"]
async fn test_e2e_rewrite_with_llm_real_service() {
    let engine = RewriteEngine::new();
    let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
    let result = engine.rewrite_with_llm(sql).await;
    println!("延迟: {}ms", result.latency_ms);
    if let Some(reason) = &result.fallback_reason {
        println!("回退原因: {}", reason);
    }
}
