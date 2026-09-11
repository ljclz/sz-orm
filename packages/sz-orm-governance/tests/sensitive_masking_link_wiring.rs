use std::collections::HashMap;
use sz_orm_governance::masking_recommend::{MaskingRecommendLink, MaskingStrategy};
use sz_orm_governance::sensitive_discoverer::*;

struct LinkDataSource {
    columns: HashMap<String, Vec<String>>,
    rows: HashMap<String, Vec<Vec<String>>>,
}

impl LinkDataSource {
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

impl TableDataSource for LinkDataSource {
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
        limit: usize,
    ) -> Result<Vec<Vec<String>>, SensitiveDiscoverError> {
        let rows = self.rows.get(table).cloned().ok_or_else(|| {
            SensitiveDiscoverError::DataSourceError(format!("表 {} 不存在", table))
        })?;
        Ok(rows.into_iter().take(limit).collect())
    }
}

#[test]
fn link_phone_finding_produces_mask_recommendation() {
    let source = LinkDataSource::new().with_table(
        "users",
        vec!["phone".to_string()],
        vec![vec!["13812345678".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    let recs = MaskingRecommendLink::generate_recommendations(&report);
    let phone_rec = recs.iter().find(|r| r.field == "phone").unwrap();
    assert_eq!(phone_rec.strategy, MaskingStrategy::Mask);
    assert!(phone_rec.reason.contains("手机号"));
}

#[test]
fn link_id_card_finding_produces_hash_recommendation() {
    let source = LinkDataSource::new().with_table(
        "citizens",
        vec!["id_card".to_string()],
        vec![vec!["110101199001011234".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "citizens").unwrap();
    let recs = MaskingRecommendLink::generate_recommendations(&report);
    let id_rec = recs.iter().find(|r| r.field == "id_card").unwrap();
    assert_eq!(id_rec.strategy, MaskingStrategy::Hash);
    assert!(id_rec.reason.contains("身份证"));
}

#[test]
fn link_email_finding_produces_mask_recommendation() {
    let source = LinkDataSource::new().with_table(
        "contacts",
        vec!["email".to_string()],
        vec![vec!["user@example.com".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "contacts").unwrap();
    let recs = MaskingRecommendLink::generate_recommendations(&report);
    let email_rec = recs.iter().find(|r| r.field == "email").unwrap();
    assert_eq!(email_rec.strategy, MaskingStrategy::Mask);
    assert!(email_rec.reason.contains("邮箱"));
}

#[test]
fn link_bank_card_finding_produces_hash_recommendation() {
    let source = LinkDataSource::new().with_table(
        "accounts",
        vec!["card_no".to_string()],
        vec![vec!["6222021234567890123".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "accounts").unwrap();
    let recs = MaskingRecommendLink::generate_recommendations(&report);
    let bank_rec = recs.iter().find(|r| r.field == "card_no").unwrap();
    assert_eq!(bank_rec.strategy, MaskingStrategy::Hash);
    assert!(bank_rec.reason.contains("银行卡"));
}

#[test]
fn link_empty_report_produces_empty_recommendations() {
    let source = LinkDataSource::new().with_table(
        "products",
        vec!["name".to_string()],
        vec![vec!["苹果".to_string()]],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "products").unwrap();
    let recs = MaskingRecommendLink::generate_recommendations(&report);
    assert!(recs.is_empty());
}

#[test]
fn link_multiple_findings_produce_multiple_recommendations() {
    let source = LinkDataSource::new().with_table(
        "users",
        vec!["phone".to_string(), "email".to_string()],
        vec![
            vec!["13812345678".to_string(), "user@example.com".to_string()],
            vec!["15998765432".to_string(), "other@test.org".to_string()],
        ],
    );
    let discoverer = SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
    let report = discoverer.scan_table(&source, "users").unwrap();
    let recs = MaskingRecommendLink::generate_recommendations(&report);
    assert!(recs.len() >= 2);
    assert!(recs.iter().any(|r| r.field == "phone"));
    assert!(recs.iter().any(|r| r.field == "email"));
}
