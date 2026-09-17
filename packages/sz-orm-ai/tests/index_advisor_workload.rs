//! 任务 3.3 测试：扩展智能索引推荐负载建模与收益预估
//!
//! 验证：
//! - 负载采样
//! - 收益预估
//! - 按收益排序
//! - 负收益标注
//! - 理由非空

use std::collections::HashMap;

use sz_orm_ai::{IndexAdvisor, IndexRecommendation, QueryPattern, TableStats, WorkloadModel};

/// 构造测试负载模型
fn build_workload() -> WorkloadModel {
    let mut table_stats = HashMap::new();
    table_stats.insert(
        "users".to_string(),
        TableStats {
            row_count: 100_000,
            existing_indexes: vec![],
        },
    );
    table_stats.insert(
        "orders".to_string(),
        TableStats {
            row_count: 50, // 小表，索引收益为负
            existing_indexes: vec![],
        },
    );
    table_stats.insert(
        "products".to_string(),
        TableStats {
            row_count: 10_000,
            existing_indexes: vec!["name".to_string()], // name 列已有索引
        },
    );

    WorkloadModel {
        patterns: vec![
            QueryPattern {
                sql_template: "SELECT * FROM users WHERE email = $1".to_string(),
                frequency: 100,
                columns_accessed: vec!["email".to_string()],
            },
            QueryPattern {
                sql_template: "SELECT * FROM orders WHERE user_id = $1".to_string(),
                frequency: 10,
                columns_accessed: vec!["user_id".to_string()],
            },
            QueryPattern {
                sql_template: "SELECT * FROM products WHERE name = $1".to_string(),
                frequency: 50,
                columns_accessed: vec!["name".to_string()],
            },
        ],
        table_stats,
    }
}

/// 负载采样：推荐结果非空
#[tokio::test]
async fn test_workload_sampling_non_empty() {
    let advisor = IndexAdvisor::new();
    let workload = build_workload();
    let recommendations = advisor.recommend(&workload).await.unwrap();
    assert!(!recommendations.is_empty(), "负载采样应产生推荐");
}

/// 收益预估：大表索引有正收益
#[tokio::test]
async fn test_positive_benefit_for_large_table() {
    let advisor = IndexAdvisor::new();
    let workload = build_workload();
    let recommendations = advisor.recommend(&workload).await.unwrap();

    // users 表（100000 行）应有正收益推荐
    let users_rec = recommendations
        .iter()
        .find(|r| r.suggestion.ddl_text.contains("users"));
    assert!(users_rec.is_some(), "users 表应有索引推荐");
    let rec = users_rec.unwrap();
    assert!(
        rec.estimated_scan_reduction > 0.99,
        "大表扫描降低比例应 > 0.99，实际：{}",
        rec.estimated_scan_reduction
    );
    assert!(
        rec.estimated_latency_reduction > 0.0,
        "大表索引延迟降低应 > 0，实际：{}",
        rec.estimated_latency_reduction
    );
}

/// 负收益标注：小表索引有负收益
#[tokio::test]
async fn test_negative_benefit_for_small_table() {
    let advisor = IndexAdvisor::new();
    let workload = build_workload();
    let recommendations = advisor.recommend(&workload).await.unwrap();

    // orders 表（50 行）应有负收益推荐
    let orders_rec = recommendations
        .iter()
        .find(|r| r.suggestion.ddl_text.contains("orders"));
    assert!(orders_rec.is_some(), "orders 表应有索引推荐");
    let rec = orders_rec.unwrap();
    assert!(
        rec.estimated_latency_reduction < 0.0,
        "小表索引延迟降低应 < 0（负收益），实际：{}",
        rec.estimated_latency_reduction
    );
    assert!(
        rec.reason.contains("负收益"),
        "负收益应在 reason 中标注，实际 reason：{}",
        rec.reason
    );
}

/// 按收益排序：推荐结果按延迟降低比例降序
#[tokio::test]
async fn test_sorted_by_benefit_desc() {
    let advisor = IndexAdvisor::new();
    let workload = build_workload();
    let recommendations = advisor.recommend(&workload).await.unwrap();

    for i in 0..recommendations.len().saturating_sub(1) {
        assert!(
            recommendations[i].estimated_latency_reduction
                >= recommendations[i + 1].estimated_latency_reduction,
            "推荐结果应按延迟降低比例降序排序，位置 {} 违反",
            i
        );
    }
}

/// 理由非空：所有推荐结果 reason 非空
#[tokio::test]
async fn test_reason_non_empty() {
    let advisor = IndexAdvisor::new();
    let workload = build_workload();
    let recommendations = advisor.recommend(&workload).await.unwrap();

    for rec in &recommendations {
        assert!(
            !rec.reason.is_empty(),
            "推荐理由不应为空，DDL：{}",
            rec.suggestion.ddl_text
        );
    }
}

/// 已有索引的列不重复推荐
#[tokio::test]
async fn test_skip_existing_index() {
    let advisor = IndexAdvisor::new();
    let workload = build_workload();
    let recommendations = advisor.recommend(&workload).await.unwrap();

    // products 表 name 列已有索引，不应重复推荐
    let products_rec = recommendations.iter().find(|r| {
        r.suggestion.ddl_text.contains("products")
            && r.suggestion.index_columns.contains(&"name".to_string())
    });
    assert!(products_rec.is_none(), "已有索引的列不应重复推荐");
}

/// 空负载报错
#[tokio::test]
async fn test_empty_workload_error() {
    let advisor = IndexAdvisor::new();
    let workload = WorkloadModel {
        patterns: vec![],
        table_stats: HashMap::new(),
    };
    let result = advisor.recommend(&workload).await;
    assert!(result.is_err());
}

/// 扫描降低比例范围 [0, 1]
#[tokio::test]
async fn test_scan_reduction_range() {
    let advisor = IndexAdvisor::new();
    let workload = build_workload();
    let recommendations = advisor.recommend(&workload).await.unwrap();

    for rec in &recommendations {
        assert!(
            rec.estimated_scan_reduction >= 0.0 && rec.estimated_scan_reduction <= 1.0,
            "扫描降低比例应在 [0, 1] 范围，实际：{}",
            rec.estimated_scan_reduction
        );
    }
}

/// IndexRecommendation Send + Sync
#[test]
fn test_index_recommendation_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<IndexRecommendation>();
    assert_send_sync::<TableStats>();
    assert_send_sync::<WorkloadModel>();
}
