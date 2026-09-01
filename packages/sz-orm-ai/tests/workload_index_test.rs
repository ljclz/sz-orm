//! TASK-009 + TASK-010: 索引工作负载驱动 + 组合优化测试

use sz_orm_ai::BenefitEstimate;
use sz_orm_ai::{
    IndexCombinationOptimizer, IndexSuggestion, IndexType, IndexUsageStats, SlowQueryLog,
    TimeRange, WorkloadDrivenIndexAdvisor, WorkloadSummary,
};

// ==================== 辅助函数 ====================

fn make_slow_query(sql: &str, time_ms: u64, ts: i64) -> SlowQueryLog {
    SlowQueryLog {
        sql: sql.to_string(),
        execution_time_ms: time_ms,
        timestamp: ts,
    }
}

fn make_workload(n: usize) -> WorkloadSummary {
    let logs: Vec<SlowQueryLog> = (0..n)
        .map(|i| {
            make_slow_query(
                &format!(
                    "SELECT * FROM users WHERE age = {} AND status = 'active'",
                    i
                ),
                100 + i as u64,
                1700000000 + i as i64,
            )
        })
        .collect();
    WorkloadSummary::new(
        logs,
        TimeRange {
            start: 1700000000,
            end: 1700086400,
        },
        n as u64,
    )
}

// ==================== WorkloadSummary 测试 ====================

#[test]
fn test_workload_summary_is_sufficient() {
    let wl_50 = make_workload(50);
    assert!(!wl_50.is_sufficient());

    let wl_100 = make_workload(100);
    assert!(wl_100.is_sufficient());

    let wl_200 = make_workload(200);
    assert!(wl_200.is_sufficient());
}

#[test]
fn test_workload_summary_top_n_slow_queries() {
    let wl = make_workload(20);
    let top5 = wl.top_n_slow_queries(5);
    assert_eq!(top5.len(), 5);
    // 按执行时间降序
    assert!(top5[0].execution_time_ms >= top5[1].execution_time_ms);
    assert!(top5[1].execution_time_ms >= top5[2].execution_time_ms);
}

#[test]
fn test_workload_summary_extract_patterns() {
    let wl = make_workload(10);
    let patterns = wl.extract_patterns();
    // 应提取出查询模式
    assert!(!patterns.is_empty());
    // 每个模式应有频率
    for p in &patterns {
        assert!(p.frequency > 0);
    }
}

// ==================== WorkloadDrivenIndexAdvisor 测试 ====================

#[tokio::test]
async fn test_workload_advisor_sufficient_samples() {
    let advisor = WorkloadDrivenIndexAdvisor::new();
    let wl = make_workload(150);

    let result = advisor.advise_for_workload(&wl).await.unwrap();

    assert!(result.is_sufficient);
    assert_eq!(result.sample_size, 150);
}

#[tokio::test]
async fn test_workload_advisor_insufficient_samples() {
    let advisor = WorkloadDrivenIndexAdvisor::new();
    let wl = make_workload(50);

    let result = advisor.advise_for_workload(&wl).await.unwrap();

    assert!(!result.is_sufficient);
    assert_eq!(result.sample_size, 50);
    // 样本量不足时置信度应降低（uncertain = true）
    for s in &result.suggestions {
        assert!(s.expected_benefit.uncertain);
    }
}

#[tokio::test]
async fn test_workload_advisor_with_top_n() {
    let advisor = WorkloadDrivenIndexAdvisor::new().with_top_n(5);
    let wl = make_workload(200);

    let result = advisor.advise_for_workload(&wl).await.unwrap();

    assert!(result.top_n_analyzed <= 5);
}

#[tokio::test]
async fn test_workload_advisor_with_max_indexes() {
    let advisor = WorkloadDrivenIndexAdvisor::new()
        .with_top_n(20)
        .with_max_indexes(3);
    let wl = make_workload(200);

    let result = advisor.advise_for_workload(&wl).await.unwrap();

    assert!(result.suggestions.len() <= 3);
}

// ==================== IndexCombinationOptimizer 测试 ====================

fn make_suggestion(cols: Vec<&str>, speedup: f64) -> IndexSuggestion {
    IndexSuggestion {
        index_columns: cols.iter().map(|s| s.to_string()).collect(),
        index_type: IndexType::BTree,
        ddl_text: format!("CREATE INDEX idx ON t ({})", cols.join(", ")),
        expected_benefit: BenefitEstimate::certain(speedup, 0.8),
        evidence: vec![],
    }
}

#[test]
fn test_combination_optimizer_under_max() {
    let optimizer = IndexCombinationOptimizer::new();
    let candidates = vec![
        make_suggestion(vec!["a"], 2.0),
        make_suggestion(vec!["b"], 3.0),
        make_suggestion(vec!["c"], 1.5),
    ];

    let result = optimizer.optimize(candidates);
    // 3 <= 5，全部保留
    assert_eq!(result.len(), 3);
}

#[test]
fn test_combination_optimizer_over_max() {
    let optimizer = IndexCombinationOptimizer::new();
    let candidates: Vec<IndexSuggestion> = (0..10)
        .map(|i| make_suggestion(vec![&format!("col{}", i)], 5.0 - i as f64 * 0.3))
        .collect();

    let result = optimizer.optimize(candidates);
    // 10 → 5
    assert_eq!(result.len(), 5);
    // 按综合评分降序
    for i in 0..result.len() - 1 {
        assert!(
            result[i].expected_benefit.composite_score()
                >= result[i + 1].expected_benefit.composite_score()
        );
    }
}

#[test]
fn test_combination_optimizer_redundant_removal() {
    let optimizer = IndexCombinationOptimizer::new();
    // idx(a) 被 idx(a, b) 覆盖
    let candidates = vec![
        make_suggestion(vec!["a"], 2.0),
        make_suggestion(vec!["a", "b"], 3.0),
        make_suggestion(vec!["b"], 1.5),
    ];

    let result = optimizer.optimize(candidates);
    // idx(a) 应被移除（被 idx(a,b) 覆盖）
    let has_a_only = result
        .iter()
        .any(|s| s.index_columns == vec!["a".to_string()]);
    assert!(!has_a_only, "idx(a) should be removed as redundant");
    // idx(a,b) 和 idx(b) 应保留
    let has_ab = result
        .iter()
        .any(|s| s.index_columns == vec!["a".to_string(), "b".to_string()]);
    assert!(has_ab);
}

#[test]
fn test_combination_optimizer_with_max_indexes() {
    let optimizer = IndexCombinationOptimizer::new().with_max_indexes(3);
    let candidates: Vec<IndexSuggestion> = (0..8)
        .map(|i| make_suggestion(vec![&format!("col{}", i)], 5.0 - i as f64 * 0.3))
        .collect();

    let result = optimizer.optimize(candidates);
    assert!(result.len() <= 3);
}

#[test]
fn test_combination_optimizer_quantify_benefit() {
    let optimizer = IndexCombinationOptimizer::new();
    let suggestion = make_suggestion(vec!["a", "b"], 3.0);

    let benefit = optimizer.quantify_benefit(&suggestion, 1_000_000, 0.01);

    // 加速比 > 1.0
    assert!(benefit.speedup_ratio > 1.0);
    // 写入开销 = 2 列 * 0.1 = 0.2
    assert!((benefit.write_overhead - 0.2).abs() < 1e-6);
    // 存储开销 = 1_000_000 * 2 * 8 / 1024 / 1024 ≈ 15.26 MB
    assert!(benefit.storage_cost_mb > 0.0);
}

#[test]
fn test_combination_optimizer_detect_redundant() {
    let optimizer = IndexCombinationOptimizer::new();
    let existing = vec![
        vec!["a".to_string()], // 被 (a, b) 覆盖
        vec!["a".to_string(), "b".to_string()],
        vec!["c".to_string()], // 独立
    ];

    let (keep, redundant) = optimizer.detect_redundant(existing);

    assert_eq!(keep.len(), 2); // (a,b) 和 (c)
    assert_eq!(redundant.len(), 1); // (a)
    assert_eq!(redundant[0].redundant_columns, vec!["a".to_string()]);
    assert_eq!(
        redundant[0].covered_by_columns,
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(redundant[0].reason.contains("覆盖"));
}

// ==================== IndexUsageStats 测试 ====================

#[test]
fn test_index_usage_stats_unused() {
    let stats = IndexUsageStats {
        index_name: "idx_a".to_string(),
        columns: vec!["a".to_string()],
        usage_count: 0,
        last_used: 0,
    };
    assert!(stats.is_unused());
    assert!(stats.is_low_usage(10));
}

#[test]
fn test_index_usage_stats_active() {
    let stats = IndexUsageStats {
        index_name: "idx_b".to_string(),
        columns: vec!["b".to_string()],
        usage_count: 100,
        last_used: 1700000000,
    };
    assert!(!stats.is_unused());
    assert!(!stats.is_low_usage(10));
}

#[test]
fn test_detect_unused_indexes() {
    let stats = vec![
        IndexUsageStats {
            index_name: "idx_a".to_string(),
            columns: vec!["a".to_string()],
            usage_count: 0,
            last_used: 0,
        },
        IndexUsageStats {
            index_name: "idx_b".to_string(),
            columns: vec!["b".to_string()],
            usage_count: 50,
            last_used: 1700000000,
        },
        IndexUsageStats {
            index_name: "idx_c".to_string(),
            columns: vec!["c".to_string()],
            usage_count: 0,
            last_used: 0,
        },
    ];

    let unused = sz_orm_ai::detect_unused_indexes(&stats);
    assert_eq!(unused.len(), 2);
}

#[test]
fn test_detect_low_usage_indexes() {
    let stats = vec![
        IndexUsageStats {
            index_name: "idx_a".to_string(),
            columns: vec!["a".to_string()],
            usage_count: 5,
            last_used: 1700000000,
        },
        IndexUsageStats {
            index_name: "idx_b".to_string(),
            columns: vec!["b".to_string()],
            usage_count: 100,
            last_used: 1700000000,
        },
    ];

    let low_usage = sz_orm_ai::detect_low_usage_indexes(&stats, 10);
    assert_eq!(low_usage.len(), 1);
    assert_eq!(low_usage[0].index_name, "idx_a");
}

// ==================== BenefitEstimate 扩展字段测试 ====================

#[test]
fn test_benefit_estimate_composite_score() {
    let be = BenefitEstimate::certain(5.0, 0.9)
        .with_write_overhead(0.4)
        .with_storage_cost(20.0);
    // composite = 5.0 - 0.4*0.5 - 20.0*0.01 = 5.0 - 0.2 - 0.2 = 4.6
    assert!((be.composite_score() - 4.6).abs() < 1e-6);
}

#[test]
fn test_benefit_estimate_default_overhead_zero() {
    let be = BenefitEstimate::certain(3.0, 0.8);
    assert!((be.write_overhead - 0.0).abs() < 1e-6);
    assert!((be.storage_cost_mb - 0.0).abs() < 1e-6);
}
