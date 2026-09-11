use std::collections::HashMap;
use sz_orm_governance::sensitive_discoverer::*;

struct WiringDataSource {
    columns: HashMap<String, Vec<String>>,
    rows: HashMap<String, Vec<Vec<String>>>,
    denied_tables: Vec<String>,
}

impl WiringDataSource {
    fn new() -> Self {
        Self {
            columns: HashMap::new(),
            rows: HashMap::new(),
            denied_tables: Vec::new(),
        }
    }

    fn with_table(mut self, table: &str, columns: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        self.columns.insert(table.to_string(), columns);
        self.rows.insert(table.to_string(), rows);
        self
    }

    fn with_denied(mut self, table: &str) -> Self {
        self.denied_tables.push(table.to_string());
        self
    }
}

impl TableDataSource for WiringDataSource {
    fn get_columns(&self, table: &str) -> Result<Vec<String>, SensitiveDiscoverError> {
        if self.denied_tables.contains(&table.to_string()) {
            return Err(SensitiveDiscoverError::TableAccessDenied(table.to_string()));
        }
        self.columns
            .get(table)
            .cloned()
            .ok_or_else(|| SensitiveDiscoverError::DataSourceError(format!("表 {} 不存在", table)))
    }

    fn get_sample_rows(
        &self,
        table: &str,
        _columns: &[String],
        limit: usize,
    ) -> Result<Vec<Vec<String>>, SensitiveDiscoverError> {
        if self.denied_tables.contains(&table.to_string()) {
            return Err(SensitiveDiscoverError::TableAccessDenied(table.to_string()));
        }
        let rows = self.rows.get(table).cloned().ok_or_else(|| {
            SensitiveDiscoverError::DataSourceError(format!("表 {} 不存在", table))
        })?;
        Ok(rows.into_iter().take(limit).collect())
    }
}

fn phone_rows() -> Vec<Vec<String>> {
    vec![
        vec!["13812345678".to_string()],
        vec!["15998765432".to_string()],
        vec!["18600001111".to_string()],
        vec!["not_a_phone".to_string()],
    ]
}

#[test]
fn wiring_scan_phone_field_produces_finding() {
    let source =
        WiringDataSource::new().with_table("users", vec!["phone".to_string()], phone_rows());
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    let phone_finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Phone);
    assert!(phone_finding.is_some());
    assert_eq!(phone_finding.unwrap().field, "phone");
}

#[test]
fn wiring_phone_finding_has_hit_count() {
    let source =
        WiringDataSource::new().with_table("users", vec!["phone".to_string()], phone_rows());
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    let phone_finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Phone)
        .unwrap();
    assert_eq!(phone_finding.hit_count, 3);
}

#[test]
fn wiring_phone_finding_has_confidence() {
    let source =
        WiringDataSource::new().with_table("users", vec!["phone".to_string()], phone_rows());
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    let phone_finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Phone)
        .unwrap();
    assert!((phone_finding.confidence - 0.75).abs() < 0.001);
}

#[test]
fn wiring_id_card_detection() {
    let source = WiringDataSource::new().with_table(
        "citizens",
        vec!["id_card".to_string()],
        vec![
            vec!["110101199001011234".to_string()],
            vec!["220102199002022345".to_string()],
        ],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "citizens").unwrap();
    let id_finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::IdCard);
    assert!(id_finding.is_some());
    assert_eq!(id_finding.unwrap().hit_count, 2);
}

#[test]
fn wiring_email_detection() {
    let source = WiringDataSource::new().with_table(
        "contacts",
        vec!["email".to_string()],
        vec![
            vec!["user1@example.com".to_string()],
            vec!["user2@test.org".to_string()],
        ],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "contacts").unwrap();
    let email_finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Email);
    assert!(email_finding.is_some());
}

#[test]
fn wiring_denied_table_skipped_and_recorded() {
    let source = WiringDataSource::new()
        .with_denied("restricted_table")
        .with_table(
            "public",
            vec!["id".to_string()],
            vec![vec!["1".to_string()]],
        );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "restricted_table").unwrap();
    assert!(report.findings.is_empty());
    assert_eq!(report.skipped_tables, vec!["restricted_table"]);
}

#[test]
fn wiring_scan_multiple_tables_aggregates() {
    let source = WiringDataSource::new()
        .with_table("users", vec!["phone".to_string()], phone_rows())
        .with_table(
            "contacts",
            vec!["email".to_string()],
            vec![vec!["test@example.com".to_string()]],
        )
        .with_denied("secret");
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer
        .scan_tables(&source, &["users", "contacts", "secret"])
        .unwrap();
    assert!(report.findings.len() >= 2);
    assert!(report.skipped_tables.contains(&"secret".to_string()));
}

#[test]
fn wiring_clean_table_no_findings() {
    let source = WiringDataSource::new().with_table(
        "products",
        vec!["name".to_string()],
        vec![vec!["苹果".to_string()], vec!["香蕉".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "products").unwrap();
    assert!(report.findings.is_empty());
    assert!(report.skipped_tables.is_empty());
}

#[test]
fn wiring_custom_rule_works() {
    let source = WiringDataSource::new().with_table(
        "logs",
        vec!["code".to_string()],
        vec![
            vec!["ERR-001".to_string()],
            vec!["ERR-002".to_string()],
            vec!["OK".to_string()],
        ],
    );
    let discoverer = SensitiveDiscoverer::new(
        vec![SensitiveRule::custom("错误码", r"^ERR-\d+")],
        SampleStrategy::default(),
    )
    .unwrap();
    let report = discoverer.scan_table(&source, "logs").unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Other)
        .unwrap();
    assert_eq!(finding.hit_count, 2);
}

#[test]
fn wiring_sample_strategy_max_samples_respected() {
    let rows: Vec<Vec<String>> = (0..200).map(|i| vec![format!("138{:08}", i)]).collect();
    let source = WiringDataSource::new().with_table("big_table", vec!["phone".to_string()], rows);
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy {
        sample_rate: 1.0,
        max_samples: 50,
    })
    .unwrap();
    let report = discoverer.scan_table(&source, "big_table").unwrap();
    let phone_finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Phone)
        .unwrap();
    assert!(phone_finding.hit_count <= 50);
}

#[test]
fn wiring_finding_contains_table_and_field_names() {
    let source =
        WiringDataSource::new().with_table("users", vec!["phone".to_string()], phone_rows());
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    let phone_finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Phone)
        .unwrap();
    assert_eq!(phone_finding.table, "users");
    assert_eq!(phone_finding.field, "phone");
}
