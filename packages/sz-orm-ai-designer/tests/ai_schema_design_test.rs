//! TASK-011: AI Schema 设计测试
//!
//! MockLlmProvider 返回预设 Schema，验证生成含用户表/订单表/商品表/订单明细表的建议 Schema。

use async_trait::async_trait;
use sz_orm_ai_designer::{
    AiSchemaDesigner, ColumnDefinition, DesignError, JoinPattern, LlmSchemaProvider, MigrationRisk,
    SchemaDesign, TableDefinition,
};

// ==================== Mock LLM Provider ====================

struct MockLlmSchemaProvider {
    designs: Vec<SchemaDesign>,
    call_index: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl MockLlmSchemaProvider {
    fn new(designs: Vec<SchemaDesign>) -> Self {
        Self {
            designs,
            call_index: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl LlmSchemaProvider for MockLlmSchemaProvider {
    async fn design(&self, _requirement: &str) -> Result<SchemaDesign, DesignError> {
        let idx = self
            .call_index
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if idx < self.designs.len() {
            Ok(self.designs[idx].clone())
        } else {
            Ok(self.designs.last().unwrap().clone())
        }
    }
}

// ==================== 辅助函数 ====================

fn make_column(name: &str, data_type: &str, pk: bool, nullable: bool) -> ColumnDefinition {
    ColumnDefinition {
        name: name.to_string(),
        data_type: data_type.to_string(),
        nullable,
        is_primary_key: pk,
        is_unique: false,
        foreign_key: None,
        default_value: None,
        comment: None,
    }
}

fn make_ecommerce_schema() -> SchemaDesign {
    SchemaDesign {
        tables: vec![
            TableDefinition {
                name: "users".to_string(),
                columns: vec![
                    make_column("id", "BIGINT", true, false),
                    make_column("name", "VARCHAR(100)", false, false),
                    make_column("email", "VARCHAR(200)", false, false),
                ],
                indexes: vec![("idx_users_email".to_string(), vec!["email".to_string()])],
                comment: Some("用户表".to_string()),
            },
            TableDefinition {
                name: "products".to_string(),
                columns: vec![
                    make_column("id", "BIGINT", true, false),
                    make_column("name", "VARCHAR(200)", false, false),
                    make_column("price", "DECIMAL(10,2)", false, false),
                ],
                indexes: vec![],
                comment: Some("商品表".to_string()),
            },
            TableDefinition {
                name: "orders".to_string(),
                columns: vec![
                    make_column("id", "BIGINT", true, false),
                    ColumnDefinition {
                        name: "user_id".to_string(),
                        data_type: "BIGINT".to_string(),
                        nullable: false,
                        is_primary_key: false,
                        is_unique: false,
                        foreign_key: Some("users.id".to_string()),
                        default_value: None,
                        comment: None,
                    },
                    make_column("total", "DECIMAL(10,2)", false, false),
                ],
                indexes: vec![("idx_orders_user".to_string(), vec!["user_id".to_string()])],
                comment: Some("订单表".to_string()),
            },
            TableDefinition {
                name: "order_items".to_string(),
                columns: vec![
                    make_column("id", "BIGINT", true, false),
                    ColumnDefinition {
                        name: "order_id".to_string(),
                        data_type: "BIGINT".to_string(),
                        nullable: false,
                        is_primary_key: false,
                        is_unique: false,
                        foreign_key: Some("orders.id".to_string()),
                        default_value: None,
                        comment: None,
                    },
                    ColumnDefinition {
                        name: "product_id".to_string(),
                        data_type: "BIGINT".to_string(),
                        nullable: false,
                        is_primary_key: false,
                        is_unique: false,
                        foreign_key: Some("products.id".to_string()),
                        default_value: None,
                        comment: None,
                    },
                    make_column("quantity", "INT", false, false),
                ],
                indexes: vec![],
                comment: Some("订单明细表".to_string()),
            },
        ],
        ddl_texts: vec![],
        rationale: "电商订单系统标准 Schema".to_string(),
    }
}

// ==================== design_schema 测试 ====================

#[tokio::test]
async fn test_design_schema_ecommerce() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let result = designer.design_schema("电商订单系统").await.unwrap();

    // 验证生成 4 个表
    assert_eq!(result.design.tables.len(), 4);
    let table_names: Vec<&str> = result
        .design
        .tables
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert!(table_names.contains(&"users"));
    assert!(table_names.contains(&"products"));
    assert!(table_names.contains(&"orders"));
    assert!(table_names.contains(&"order_items"));

    // 验证生成 DDL 文本
    assert!(!result.design.ddl_texts.is_empty());
    for ddl in &result.design.ddl_texts {
        assert!(ddl.contains("CREATE"));
    }

    // 首次成功，无重试
    assert_eq!(result.retries, 0);
    assert!(!result.fixed);
}

#[tokio::test]
async fn test_design_schema_generates_create_table_ddl() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let result = designer.design_schema("电商").await.unwrap();

    // 验证 DDL 包含 CREATE TABLE
    let create_tables: Vec<&String> = result
        .design
        .ddl_texts
        .iter()
        .filter(|ddl| ddl.contains("CREATE TABLE"))
        .collect();
    assert_eq!(create_tables.len(), 4);
}

#[tokio::test]
async fn test_design_schema_generates_create_index_ddl() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let result = designer.design_schema("电商").await.unwrap();

    // 验证 DDL 包含 CREATE INDEX
    let create_indexes: Vec<&String> = result
        .design
        .ddl_texts
        .iter()
        .filter(|ddl| ddl.contains("CREATE INDEX"))
        .collect();
    assert!(create_indexes.len() >= 2); // idx_users_email + idx_orders_user
}

#[tokio::test]
async fn test_design_schema_with_foreign_keys() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let result = designer.design_schema("电商").await.unwrap();

    // 验证 orders 表有外键
    let orders = result
        .design
        .tables
        .iter()
        .find(|t| t.name == "orders")
        .unwrap();
    let user_id = orders.columns.iter().find(|c| c.name == "user_id").unwrap();
    assert!(user_id.foreign_key.is_some());
    assert_eq!(user_id.foreign_key.as_deref(), Some("users.id"));
}

#[tokio::test]
async fn test_design_schema_retry_on_syntax_error() {
    // 第 1 次：返回语法错误的 DDL（通过空表名模拟）
    let bad_design = SchemaDesign {
        tables: vec![TableDefinition {
            name: "".to_string(), // 空表名会导致语法错误
            columns: vec![make_column("id", "INT", true, false)],
            indexes: vec![],
            comment: None,
        }],
        ddl_texts: vec![],
        rationale: "bad".to_string(),
    };

    // 第 2 次：返回合法 Schema
    let good_design = make_ecommerce_schema();

    let llm = MockLlmSchemaProvider::new(vec![bad_design, good_design]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let result = designer.design_schema("电商").await.unwrap();

    // 重试 1 次后成功
    assert_eq!(result.retries, 1);
    assert!(result.fixed);
    assert_eq!(result.design.tables.len(), 4);
}

#[tokio::test]
async fn test_design_schema_max_retries_exhausted() {
    // 始终返回语法错误的 DDL
    let bad_design = SchemaDesign {
        tables: vec![TableDefinition {
            name: "".to_string(),
            columns: vec![make_column("id", "INT", true, false)],
            indexes: vec![],
            comment: None,
        }],
        ddl_texts: vec![],
        rationale: "bad".to_string(),
    };

    let llm = MockLlmSchemaProvider::new(vec![bad_design]);
    let designer = AiSchemaDesigner::new(Box::new(llm)).with_max_retries(2);

    let result = designer.design_schema("电商").await;
    assert!(result.is_err());
}

// ==================== analyze_migration_impact 测试 ====================

#[tokio::test]
async fn test_analyze_migration_impact_add_table() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let old_schema = SchemaDesign {
        tables: vec![],
        ddl_texts: vec![],
        rationale: "empty".to_string(),
    };
    let new_schema = make_ecommerce_schema();

    let report = designer.analyze_migration_impact(&old_schema, &new_schema);

    assert!(report.affected_queries > 0);
    assert_eq!(report.risk_level, MigrationRisk::Low);
}

#[tokio::test]
async fn test_analyze_migration_impact_remove_table() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let old_schema = make_ecommerce_schema();
    let new_schema = SchemaDesign {
        tables: vec![],
        ddl_texts: vec![],
        rationale: "empty".to_string(),
    };

    let report = designer.analyze_migration_impact(&old_schema, &new_schema);

    assert!(report.affected_queries > 0);
    assert_eq!(report.risk_level, MigrationRisk::High); // 删除表是高风险
}

#[tokio::test]
async fn test_analyze_migration_impact_no_change() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let schema = make_ecommerce_schema();
    let report = designer.analyze_migration_impact(&schema, &schema);

    assert_eq!(report.affected_queries, 0);
    assert_eq!(report.risk_level, MigrationRisk::Low);
}

// ==================== denormalization_advice 测试 ====================

#[tokio::test]
async fn test_denormalization_advice_with_frequent_joins() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let joins = vec![JoinPattern {
        from_table: "order_items".to_string(),
        to_table: "products".to_string(),
        foreign_key: "product_id".to_string(),
        frequently_accessed_columns: vec!["name".to_string(), "price".to_string()],
        frequency: 1000,
    }];

    let advice = designer.denormalization_advice(&joins);

    assert_eq!(advice.joins_reduced, 1);
    assert_eq!(advice.redundant_columns.len(), 2);
    assert!(advice.reason.contains("频繁 JOIN"));
}

#[tokio::test]
async fn test_denormalization_advice_no_joins() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let advice = designer.denormalization_advice(&[]);

    assert_eq!(advice.joins_reduced, 0);
    assert!(advice.redundant_columns.is_empty());
    assert!(advice.reason.contains("无频繁 JOIN"));
}

#[tokio::test]
async fn test_denormalization_advice_multiple_joins() {
    let llm = MockLlmSchemaProvider::new(vec![make_ecommerce_schema()]);
    let designer = AiSchemaDesigner::new(Box::new(llm));

    let joins = vec![
        JoinPattern {
            from_table: "orders".to_string(),
            to_table: "users".to_string(),
            foreign_key: "user_id".to_string(),
            frequently_accessed_columns: vec!["name".to_string()],
            frequency: 500,
        },
        JoinPattern {
            from_table: "order_items".to_string(),
            to_table: "products".to_string(),
            foreign_key: "product_id".to_string(),
            frequently_accessed_columns: vec!["name".to_string(), "price".to_string()],
            frequency: 1000,
        },
    ];

    let advice = designer.denormalization_advice(&joins);

    assert_eq!(advice.joins_reduced, 2);
    assert_eq!(advice.redundant_columns.len(), 3); // 1 + 2
}
