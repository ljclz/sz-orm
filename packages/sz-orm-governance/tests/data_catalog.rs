//! TASK-021 集成测试：数据目录生成端到端验证

use sz_orm_governance::data_catalog::DataCatalogBuilder;

#[test]
fn test_build_catalog_with_business_meaning() {
    let builder = DataCatalogBuilder::new();
    let catalog = builder.build(
        "orders",
        &[
            ("order_id", "BIGINT"),
            ("customer_name", "VARCHAR"),
            ("amount", "DECIMAL"),
        ],
    );

    assert_eq!(catalog.table, "orders");
    assert_eq!(catalog.columns.len(), 3);
    assert_eq!(catalog.columns[0].business_meaning, "唯一标识符");
    assert_eq!(catalog.columns[1].business_meaning, "名称");
    assert_eq!(catalog.columns[2].business_meaning, "金额");
}

#[test]
fn test_quality_score_is_average() {
    let builder = DataCatalogBuilder::new();
    let catalog = builder.build("test_table", &[("a", "INT"), ("b", "INT"), ("c", "INT")]);
    assert!(
        (catalog.quality_score - 0.85).abs() < 1e-10,
        "质量分数应为列分数平均值: {}",
        catalog.quality_score
    );
}

#[test]
fn test_empty_columns() {
    let builder = DataCatalogBuilder::new();
    let catalog = builder.build("empty_table", &[]);
    assert_eq!(catalog.columns.len(), 0);
    assert_eq!(catalog.quality_score, 0.0);
}
