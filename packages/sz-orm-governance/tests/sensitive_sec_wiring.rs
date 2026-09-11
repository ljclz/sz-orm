use std::collections::HashMap;
use sz_orm_governance::sensitive_discoverer::*;

struct SecDataSource {
    columns: HashMap<String, Vec<String>>,
    rows: HashMap<String, Vec<Vec<String>>>,
}

impl SecDataSource {
    fn new() -> Self {
        Self {
            columns: HashMap::new(),
            rows: HashMap::new(),
        }
    }

    fn with_table(mut self, table: &str, columns: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        self.columns.insert(table.to_string(), columns);
        self.rows.insert(table.to_string(), rows);
        self
    }
}

impl TableDataSource for SecDataSource {
    fn get_columns(&self, table: &str) -> Result<Vec<String>, SensitiveDiscoverError> {
        self.columns
            .get(table)
            .cloned()
            .ok_or_else(|| SensitiveDiscoverError::DataSourceError(format!("表 {} 不存在", table)))
    }

    fn get_sample_rows(
        &self,
        table: &str,
        _columns: &[String],
        _limit: usize,
    ) -> Result<Vec<Vec<String>>, SensitiveDiscoverError> {
        self.rows
            .get(table)
            .cloned()
            .ok_or_else(|| SensitiveDiscoverError::DataSourceError(format!("表 {} 不存在", table)))
    }
}

#[test]
fn sec_phone_masked_example_no_plaintext() {
    let source = SecDataSource::new().with_table(
        "users",
        vec!["phone".to_string()],
        vec![vec!["13812345678".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Phone)
        .unwrap();
    assert!(!finding.masked_example.contains("13812345678"));
    assert!(!finding.masked_example.contains("12345678"));
    assert!(!finding.masked_example.contains("1381234"));
}

#[test]
fn sec_id_card_masked_example_no_plaintext() {
    let source = SecDataSource::new().with_table(
        "citizens",
        vec!["id_card".to_string()],
        vec![vec!["110101199001011234".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "citizens").unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::IdCard)
        .unwrap();
    assert!(!finding.masked_example.contains("110101199001011234"));
    assert!(!finding.masked_example.contains("199001011234"));
    assert!(!finding.masked_example.contains("1101011990"));
}

#[test]
fn sec_email_masked_example_no_plaintext() {
    let source = SecDataSource::new().with_table(
        "contacts",
        vec!["email".to_string()],
        vec![vec!["zhangsan@example.com".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "contacts").unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::Email)
        .unwrap();
    assert!(!finding.masked_example.contains("zhangsan@example.com"));
    assert!(!finding.masked_example.contains("zhangsan"));
}

#[test]
fn sec_bank_card_masked_example_no_plaintext() {
    let source = SecDataSource::new().with_table(
        "accounts",
        vec!["card_no".to_string()],
        vec![vec!["6222021234567890123".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "accounts").unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.suspected_type == SensitiveType::BankCard)
        .unwrap();
    assert!(!finding.masked_example.contains("6222021234567890123"));
    assert!(!finding.masked_example.contains("21234567890"));
}

#[test]
fn sec_masked_example_always_present() {
    let source = SecDataSource::new().with_table(
        "users",
        vec!["phone".to_string()],
        vec![vec!["13812345678".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    for finding in &report.findings {
        assert!(!finding.masked_example.is_empty());
    }
}

#[test]
fn sec_report_contains_only_masked_examples_not_raw() {
    let phone = "13812345678";
    let source = SecDataSource::new().with_table(
        "users",
        vec!["phone".to_string()],
        vec![vec![phone.to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    let report_json = format!("{:?}", report);
    assert!(!report_json.contains(phone));
}
