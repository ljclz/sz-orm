//! 敏感数据自动发现（v6.8.0 GOV-SENSITIVE-01 + GOV-SENSITIVE-SEC）
//!
//! 按规则与采样策略扫描目标表样本，正则匹配敏感数据（手机号/身份证/银行卡/邮箱），
//! 置信度 = 命中样本数 / 总样本数，`DataMasker::apply` 生成脱敏样例。
//! `masked_example` 严格保证不含明文敏感值（GOV-SENSITIVE-SEC）。

use regex::Regex;
use sz_orm_masking::{DataMasker, MaskingRule};

/// 敏感数据类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SensitiveType {
    Phone,
    IdCard,
    BankCard,
    Email,
    Other,
}

impl SensitiveType {
    fn to_masking_rule(&self) -> MaskingRule {
        match self {
            SensitiveType::Phone => MaskingRule::Phone,
            SensitiveType::IdCard => MaskingRule::IdCard,
            SensitiveType::BankCard => MaskingRule::BankCard,
            SensitiveType::Email => MaskingRule::Email,
            SensitiveType::Other => MaskingRule::Custom("***".to_string()),
        }
    }
}

/// 敏感数据检测规则
#[derive(Debug, Clone)]
pub struct SensitiveRule {
    pub sensitive_type: SensitiveType,
    pub pattern: String,
    pub name: String,
}

impl SensitiveRule {
    /// 创建手机号检测规则
    pub fn phone() -> Self {
        Self {
            sensitive_type: SensitiveType::Phone,
            pattern: r"^1[3-9]\d{9}$".to_string(),
            name: "手机号".to_string(),
        }
    }

    /// 创建身份证号检测规则
    pub fn id_card() -> Self {
        Self {
            sensitive_type: SensitiveType::IdCard,
            pattern: r"^\d{17}[\dXx]$".to_string(),
            name: "身份证号".to_string(),
        }
    }

    /// 创建银行卡号检测规则
    pub fn bank_card() -> Self {
        Self {
            sensitive_type: SensitiveType::BankCard,
            pattern: r"^\d{16,19}$".to_string(),
            name: "银行卡号".to_string(),
        }
    }

    /// 创建邮箱检测规则
    pub fn email() -> Self {
        Self {
            sensitive_type: SensitiveType::Email,
            pattern: r"^[\w.+-]+@[\w-]+\.[\w.-]+$".to_string(),
            name: "邮箱".to_string(),
        }
    }

    /// 创建自定义检测规则
    pub fn custom(name: &str, pattern: &str) -> Self {
        Self {
            sensitive_type: SensitiveType::Other,
            pattern: pattern.to_string(),
            name: name.to_string(),
        }
    }

    fn compile(&self) -> Result<Regex, SensitiveDiscoverError> {
        Regex::new(&self.pattern).map_err(|e| SensitiveDiscoverError::InvalidPattern(e.to_string()))
    }
}

/// 采样策略
#[derive(Debug, Clone)]
pub struct SampleStrategy {
    pub sample_rate: f32,
    pub max_samples: usize,
}

impl Default for SampleStrategy {
    fn default() -> Self {
        Self {
            sample_rate: 1.0,
            max_samples: 100,
        }
    }
}

/// 表数据源 trait（解耦数据库依赖，便于测试注入）
pub trait TableDataSource: Send + Sync {
    /// 获取表的列名列表
    fn get_columns(&self, table: &str) -> Result<Vec<String>, SensitiveDiscoverError>;

    /// 获取样本数据（返回行列表，每行是各列的字符串值）
    fn get_sample_rows(
        &self,
        table: &str,
        columns: &[String],
        limit: usize,
    ) -> Result<Vec<Vec<String>>, SensitiveDiscoverError>;
}

/// 发现结果
#[derive(Debug, Clone)]
pub struct Finding {
    pub table: String,
    pub field: String,
    pub suspected_type: SensitiveType,
    pub hit_count: u64,
    pub confidence: f32,
    pub masked_example: String,
}

/// 扫描报告
#[derive(Debug, Clone)]
pub struct ScanReport {
    pub findings: Vec<Finding>,
    pub skipped_tables: Vec<String>,
}

/// 敏感数据发现错误
#[derive(Debug, Clone)]
pub enum SensitiveDiscoverError {
    InvalidPattern(String),
    TableAccessDenied(String),
    DataSourceError(String),
}

impl std::fmt::Display for SensitiveDiscoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SensitiveDiscoverError::InvalidPattern(msg) => write!(f, "无效正则模式: {}", msg),
            SensitiveDiscoverError::TableAccessDenied(table) => {
                write!(f, "表 {} 访问被拒绝", table)
            }
            SensitiveDiscoverError::DataSourceError(msg) => write!(f, "数据源错误: {}", msg),
        }
    }
}

impl std::error::Error for SensitiveDiscoverError {}

/// 敏感数据发现器
pub struct SensitiveDiscoverer {
    rules: Vec<SensitiveRule>,
    sample_strategy: SampleStrategy,
    compiled_rules: Vec<(SensitiveType, Regex, String)>,
}

impl SensitiveDiscoverer {
    /// 创建敏感数据发现器
    pub fn new(
        rules: Vec<SensitiveRule>,
        sample_strategy: SampleStrategy,
    ) -> Result<Self, SensitiveDiscoverError> {
        let mut compiled_rules = Vec::with_capacity(rules.len());
        for rule in &rules {
            let regex = rule.compile()?;
            compiled_rules.push((rule.sensitive_type.clone(), regex, rule.name.clone()));
        }
        Ok(Self {
            rules,
            sample_strategy,
            compiled_rules,
        })
    }

    /// 使用默认规则创建（手机号 + 身份证 + 银行卡 + 邮箱）
    pub fn with_default_rules(
        sample_strategy: SampleStrategy,
    ) -> Result<Self, SensitiveDiscoverError> {
        Self::new(
            vec![
                SensitiveRule::phone(),
                SensitiveRule::id_card(),
                SensitiveRule::bank_card(),
                SensitiveRule::email(),
            ],
            sample_strategy,
        )
    }

    /// 扫描单个表
    pub fn scan_table(
        &self,
        source: &dyn TableDataSource,
        table: &str,
    ) -> Result<ScanReport, SensitiveDiscoverError> {
        let columns = match source.get_columns(table) {
            Ok(cols) => cols,
            Err(SensitiveDiscoverError::TableAccessDenied(_)) => {
                return Ok(ScanReport {
                    findings: Vec::new(),
                    skipped_tables: vec![table.to_string()],
                });
            }
            Err(e) => return Err(e),
        };

        let sample_limit = self.sample_strategy.max_samples;
        let rows = match source.get_sample_rows(table, &columns, sample_limit) {
            Ok(r) => r,
            Err(SensitiveDiscoverError::TableAccessDenied(_)) => {
                return Ok(ScanReport {
                    findings: Vec::new(),
                    skipped_tables: vec![table.to_string()],
                });
            }
            Err(e) => return Err(e),
        };

        let total_samples = rows.len() as u64;
        let mut findings = Vec::new();

        for (col_idx, col_name) in columns.iter().enumerate() {
            for (sensitive_type, regex, _rule_name) in &self.compiled_rules {
                let mut hit_count = 0u64;
                let mut first_hit_value: Option<String> = None;

                for row in &rows {
                    if col_idx >= row.len() {
                        continue;
                    }
                    let value = &row[col_idx];
                    if value.is_empty() {
                        continue;
                    }
                    if regex.is_match(value) {
                        hit_count += 1;
                        if first_hit_value.is_none() {
                            first_hit_value = Some(value.clone());
                        }
                    }
                }

                if hit_count > 0 {
                    let confidence = if total_samples > 0 {
                        hit_count as f32 / total_samples as f32
                    } else {
                        0.0
                    };

                    let masked_example = match &first_hit_value {
                        Some(val) => {
                            let masking_rule = sensitive_type.to_masking_rule();
                            let masked = DataMasker::apply(&masking_rule, val);
                            if Self::contains_plaintext(&masked, val, sensitive_type) {
                                DataMasker::apply(&MaskingRule::Custom("***".to_string()), val)
                            } else {
                                masked
                            }
                        }
                        None => "***".to_string(),
                    };

                    findings.push(Finding {
                        table: table.to_string(),
                        field: col_name.clone(),
                        suspected_type: sensitive_type.clone(),
                        hit_count,
                        confidence,
                        masked_example,
                    });

                    break;
                }
            }
        }

        Ok(ScanReport {
            findings,
            skipped_tables: Vec::new(),
        })
    }

    /// 扫描多个表
    pub fn scan_tables(
        &self,
        source: &dyn TableDataSource,
        tables: &[&str],
    ) -> Result<ScanReport, SensitiveDiscoverError> {
        let mut all_findings = Vec::new();
        let mut all_skipped = Vec::new();

        for table in tables {
            let report = self.scan_table(source, table)?;
            all_findings.extend(report.findings);
            all_skipped.extend(report.skipped_tables);
        }

        Ok(ScanReport {
            findings: all_findings,
            skipped_tables: all_skipped,
        })
    }

    /// 检查脱敏结果是否仍包含明文（GOV-SENSITIVE-SEC 安全防线）
    fn contains_plaintext(masked: &str, original: &str, sensitive_type: &SensitiveType) -> bool {
        if masked == original {
            return true;
        }
        match sensitive_type {
            SensitiveType::Phone => Self::contains_phone_plaintext(masked, original),
            SensitiveType::IdCard => Self::contains_idcard_plaintext(masked, original),
            SensitiveType::BankCard => Self::contains_bankcard_plaintext(masked, original),
            SensitiveType::Email => Self::contains_email_plaintext(masked, original),
            SensitiveType::Other => masked == original,
        }
    }

    /// 检查手机号脱敏后是否仍包含明文片段
    fn contains_phone_plaintext(masked: &str, original: &str) -> bool {
        let digits: String = original.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 7 {
            let tail = &digits[digits.len() - 4..];
            let middle = &digits[3..digits.len() - 4];
            return masked.contains(middle)
                || (masked.contains(tail) && masked.len() > 4 && masked != original);
        }
        false
    }

    /// 检查身份证号脱敏后是否仍包含明文片段
    fn contains_idcard_plaintext(masked: &str, original: &str) -> bool {
        let digits: String = original.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.len() >= 10 && masked.contains(&digits[4..digits.len() - 4])
    }

    /// 检查银行卡号脱敏后是否仍包含明文片段
    fn contains_bankcard_plaintext(masked: &str, original: &str) -> bool {
        let digits: String = original.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.len() >= 8 && masked.contains(&digits[4..digits.len() - 4])
    }

    /// 检查邮箱脱敏后是否仍包含明文片段
    fn contains_email_plaintext(masked: &str, original: &str) -> bool {
        let at_pos = original.find('@');
        if let Some(at) = at_pos {
            let local_part = &original[..at];
            local_part.len() >= 3 && masked.contains(local_part)
        } else {
            false
        }
    }

    /// 获取规则列表
    pub fn rules(&self) -> &[SensitiveRule] {
        &self.rules
    }

    /// 获取采样策略
    pub fn sample_strategy(&self) -> &SampleStrategy {
        &self.sample_strategy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockDataSource {
        columns: HashMap<String, Vec<String>>,
        rows: HashMap<String, Vec<Vec<String>>>,
        denied_tables: Vec<String>,
    }

    impl MockDataSource {
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

    impl TableDataSource for MockDataSource {
        fn get_columns(&self, table: &str) -> Result<Vec<String>, SensitiveDiscoverError> {
            if self.denied_tables.contains(&table.to_string()) {
                return Err(SensitiveDiscoverError::TableAccessDenied(table.to_string()));
            }
            self.columns.get(table).cloned().ok_or_else(|| {
                SensitiveDiscoverError::DataSourceError(format!("表 {} 不存在", table))
            })
        }

        fn get_sample_rows(
            &self,
            table: &str,
            _columns: &[String],
            _limit: usize,
        ) -> Result<Vec<Vec<String>>, SensitiveDiscoverError> {
            if self.denied_tables.contains(&table.to_string()) {
                return Err(SensitiveDiscoverError::TableAccessDenied(table.to_string()));
            }
            self.rows.get(table).cloned().ok_or_else(|| {
                SensitiveDiscoverError::DataSourceError(format!("表 {} 不存在", table))
            })
        }
    }

    #[test]
    fn discoverer_with_default_rules_creates_four_rules() {
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        assert_eq!(discoverer.rules().len(), 4);
    }

    #[test]
    fn scan_table_detects_phone_numbers() {
        let source = MockDataSource::new().with_table(
            "users",
            vec!["id".to_string(), "phone".to_string()],
            vec![
                vec!["1".to_string(), "13812345678".to_string()],
                vec!["2".to_string(), "15998765432".to_string()],
                vec!["3".to_string(), "12345".to_string()],
            ],
        );
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer.scan_table(&source, "users").unwrap();
        let phone_finding = report
            .findings
            .iter()
            .find(|f| f.suspected_type == SensitiveType::Phone)
            .unwrap();
        assert_eq!(phone_finding.hit_count, 2);
        assert!((phone_finding.confidence - 0.6667).abs() < 0.01);
    }

    #[test]
    fn scan_table_detects_id_cards() {
        let source = MockDataSource::new().with_table(
            "citizens",
            vec!["name".to_string(), "id_card".to_string()],
            vec![
                vec!["张三".to_string(), "110101199001011234".to_string()],
                vec!["李四".to_string(), "220102199002022345".to_string()],
            ],
        );
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer.scan_table(&source, "citizens").unwrap();
        let id_finding = report
            .findings
            .iter()
            .find(|f| f.suspected_type == SensitiveType::IdCard)
            .unwrap();
        assert_eq!(id_finding.hit_count, 2);
    }

    #[test]
    fn scan_table_detects_emails() {
        let source = MockDataSource::new().with_table(
            "contacts",
            vec!["name".to_string(), "email".to_string()],
            vec![
                vec!["张三".to_string(), "zhangsan@example.com".to_string()],
                vec!["李四".to_string(), "lisi@test.org".to_string()],
                vec!["王五".to_string(), "not_an_email".to_string()],
            ],
        );
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer.scan_table(&source, "contacts").unwrap();
        let email_finding = report
            .findings
            .iter()
            .find(|f| f.suspected_type == SensitiveType::Email)
            .unwrap();
        assert_eq!(email_finding.hit_count, 2);
    }

    #[test]
    fn scan_table_detects_bank_cards() {
        let source = MockDataSource::new().with_table(
            "accounts",
            vec!["name".to_string(), "card_no".to_string()],
            vec![
                vec!["张三".to_string(), "6222021234567890123".to_string()],
                vec!["李四".to_string(), "6225887654321098".to_string()],
            ],
        );
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer.scan_table(&source, "accounts").unwrap();
        let bank_finding = report
            .findings
            .iter()
            .find(|f| f.suspected_type == SensitiveType::BankCard);
        assert!(bank_finding.is_some());
    }

    #[test]
    fn scan_table_no_findings_for_clean_data() {
        let source = MockDataSource::new().with_table(
            "products",
            vec!["id".to_string(), "name".to_string()],
            vec![
                vec!["1".to_string(), "苹果".to_string()],
                vec!["2".to_string(), "香蕉".to_string()],
            ],
        );
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer.scan_table(&source, "products").unwrap();
        assert!(report.findings.is_empty());
    }

    #[test]
    fn scan_table_skips_denied_tables() {
        let source = MockDataSource::new()
            .with_denied("secret_table")
            .with_table(
                "public_table",
                vec!["id".to_string()],
                vec![vec!["1".to_string()]],
            );
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer.scan_table(&source, "secret_table").unwrap();
        assert!(report.findings.is_empty());
        assert_eq!(report.skipped_tables, vec!["secret_table"]);
    }

    #[test]
    fn scan_tables_aggregates_multiple_tables() {
        let source = MockDataSource::new()
            .with_table(
                "users",
                vec!["phone".to_string()],
                vec![vec!["13812345678".to_string()]],
            )
            .with_table(
                "contacts",
                vec!["email".to_string()],
                vec![vec!["test@example.com".to_string()]],
            )
            .with_denied("secret");
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer
            .scan_tables(&source, &["users", "contacts", "secret"])
            .unwrap();
        assert!(report.findings.len() >= 2);
        assert!(report.skipped_tables.contains(&"secret".to_string()));
    }

    #[test]
    fn finding_masked_example_does_not_contain_plaintext() {
        let source = MockDataSource::new().with_table(
            "users",
            vec!["phone".to_string()],
            vec![vec!["13812345678".to_string()]],
        );
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer.scan_table(&source, "users").unwrap();
        let phone_finding = report
            .findings
            .iter()
            .find(|f| f.suspected_type == SensitiveType::Phone)
            .unwrap();
        assert!(!phone_finding.masked_example.contains("13812345678"));
        assert!(!phone_finding.masked_example.contains("12345678"));
    }

    #[test]
    fn finding_confidence_is_hit_count_over_total() {
        let source = MockDataSource::new().with_table(
            "users",
            vec!["phone".to_string()],
            vec![
                vec!["13812345678".to_string()],
                vec!["15998765432".to_string()],
                vec!["not_phone".to_string()],
                vec!["also_not".to_string()],
            ],
        );
        let discoverer =
            SensitiveDiscoverer::with_default_rules(SampleStrategy::default()).unwrap();
        let report = discoverer.scan_table(&source, "users").unwrap();
        let phone_finding = report
            .findings
            .iter()
            .find(|f| f.suspected_type == SensitiveType::Phone)
            .unwrap();
        assert_eq!(phone_finding.hit_count, 2);
        assert!((phone_finding.confidence - 0.5).abs() < 0.001);
    }

    #[test]
    fn custom_rule_detects_custom_pattern() {
        let source = MockDataSource::new().with_table(
            "logs",
            vec!["message".to_string()],
            vec![
                vec!["ERROR-001: system failure".to_string()],
                vec!["ERROR-002: timeout".to_string()],
                vec!["INFO: normal operation".to_string()],
            ],
        );
        let discoverer = SensitiveDiscoverer::new(
            vec![SensitiveRule::custom("错误码", r"^ERROR-\d+")],
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
}
