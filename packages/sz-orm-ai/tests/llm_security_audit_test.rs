//! TASK-013: LLM 安全审计测试

use async_trait::async_trait;
use std::sync::{atomic::AtomicUsize, Arc};
use sz_orm_ai::llm_security_audit::RiskLevel;
use sz_orm_ai::{
    AuditSource, InjectionPattern, InjectionPatternStore, LlmAuditProvider, LlmSecurityAuditor,
    SecurityAuditError,
};

// ==================== Mock LLM Provider ====================

struct MockLlmAuditProvider {
    /// 预设返回结果
    result: (RiskLevel, Vec<String>, Option<String>),
    /// 调用计数
    call_count: Arc<AtomicUsize>,
}

impl MockLlmAuditProvider {
    fn new(result: (RiskLevel, Vec<String>, Option<String>)) -> Self {
        Self {
            result,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl LlmAuditProvider for MockLlmAuditProvider {
    async fn audit(
        &self,
        _input: &str,
    ) -> Result<(RiskLevel, Vec<String>, Option<String>), SecurityAuditError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.result.clone())
    }
}

// ==================== RiskLevel 测试 ====================

#[test]
fn test_risk_level_is_dangerous() {
    assert!(!RiskLevel::Safe.is_dangerous());
    assert!(!RiskLevel::Low.is_dangerous());
    assert!(!RiskLevel::Medium.is_dangerous());
    assert!(RiskLevel::High.is_dangerous());
    assert!(RiskLevel::Critical.is_dangerous());
}

// ==================== InjectionPatternStore 测试 ====================

#[test]
fn test_pattern_store_in_memory() {
    let store = InjectionPatternStore::in_memory();
    assert!(!store.is_empty());
    // 内置 4 个模式
    assert!(store.len() >= 4);
}

#[test]
fn test_pattern_store_check_builtin_or_injection() {
    let mut store = InjectionPatternStore::in_memory();
    let result = store.check("SELECT * FROM users WHERE name = 'admin' OR 1=1");
    assert!(result.is_some());
    assert_eq!(result.unwrap().risk_level, RiskLevel::Critical);
}

#[test]
fn test_pattern_store_check_builtin_union_injection() {
    let mut store = InjectionPatternStore::in_memory();
    let result = store.check("1' UNION SELECT password FROM users");
    assert!(result.is_some());
    assert_eq!(result.unwrap().risk_level, RiskLevel::Critical);
}

#[test]
fn test_pattern_store_check_safe_input() {
    let mut store = InjectionPatternStore::in_memory();
    let result = store.check("SELECT * FROM users WHERE id = $1");
    assert!(result.is_none());
}

#[test]
fn test_pattern_store_add_new_pattern() {
    let mut store = InjectionPatternStore::in_memory();
    let initial_len = store.len();

    let pattern = InjectionPattern {
        name: "custom_injection".to_string(),
        pattern: "EVIL_PATTERN".to_string(),
        risk_level: RiskLevel::High,
        discovered_at: 1700000000,
        hit_count: 0,
    };

    let added = store.add_pattern(pattern).unwrap();
    assert!(added);
    assert_eq!(store.len(), initial_len + 1);
}

#[test]
fn test_pattern_store_add_duplicate_pattern() {
    let mut store = InjectionPatternStore::in_memory();
    let initial_len = store.len();

    let pattern = InjectionPattern {
        name: "custom".to_string(),
        pattern: "' OR 1=1".to_string(), // 与内置模式重复
        risk_level: RiskLevel::High,
        discovered_at: 1700000000,
        hit_count: 0,
    };

    let added = store.add_pattern(pattern).unwrap();
    assert!(!added);
    assert_eq!(store.len(), initial_len);
}

#[test]
fn test_pattern_store_persist_to_file() {
    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("sz_orm_test_patterns.json");

    // 清理旧文件
    let _ = std::fs::remove_file(&path);

    // 创建并添加新模式
    {
        let mut store = InjectionPatternStore::new(path.clone()).unwrap();
        let pattern = InjectionPattern {
            name: "file_pattern".to_string(),
            pattern: "FILE_EVIL".to_string(),
            risk_level: RiskLevel::High,
            discovered_at: 1700000000,
            hit_count: 0,
        };
        store.add_pattern(pattern).unwrap();
    }

    // 重新加载，验证持久化
    let store = InjectionPatternStore::new(path.clone()).unwrap();
    assert!(store.patterns().iter().any(|p| p.name == "file_pattern"));

    // 清理
    let _ = std::fs::remove_file(&path);
}

// ==================== LlmSecurityAuditor 测试 ====================

#[tokio::test]
async fn test_auditor_rule_only_detects_injection() {
    let mut auditor = LlmSecurityAuditor::rule_only();

    let result = auditor
        .audit("SELECT * FROM users WHERE name = 'admin' OR 1=1")
        .await
        .unwrap();

    assert_eq!(result.risk_level, RiskLevel::Critical);
    assert!(!result.detected_patterns.is_empty());
    assert_eq!(result.source, AuditSource::Rule);
    assert!(!result.is_new_pattern);
}

#[tokio::test]
async fn test_auditor_rule_only_safe_input() {
    let mut auditor = LlmSecurityAuditor::rule_only();

    let result = auditor
        .audit("SELECT * FROM users WHERE id = $1")
        .await
        .unwrap();

    assert_eq!(result.risk_level, RiskLevel::Safe);
    assert!(result.detected_patterns.is_empty());
}

#[tokio::test]
async fn test_auditor_llm_fallback_on_unknown_pattern() {
    let llm = MockLlmAuditProvider::new((
        RiskLevel::High,
        vec!["novel_injection".to_string()],
        Some("使用参数化查询".to_string()),
    ));
    let call_count = llm.call_count.clone();

    let mut auditor = LlmSecurityAuditor::with_llm(Box::new(llm));

    // 规则引擎无法识别的输入
    let result = auditor
        .audit("SELECT * FROM users WHERE name = 'novel_attack_vector'")
        .await
        .unwrap();

    // LLM 应被调用
    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(result.risk_level, RiskLevel::High);
    assert_eq!(result.source, AuditSource::Llm);
    assert!(result.is_new_pattern);
    assert!(result.fix_suggestion.is_some());
}

#[tokio::test]
async fn test_auditor_llm_says_safe() {
    let llm = MockLlmAuditProvider::new((RiskLevel::Safe, vec![], None));

    let mut auditor = LlmSecurityAuditor::with_llm(Box::new(llm));

    let result = auditor
        .audit("SELECT * FROM users WHERE name = 'normal_query'")
        .await
        .unwrap();

    assert_eq!(result.risk_level, RiskLevel::Safe);
    assert!(!result.is_new_pattern);
}

#[tokio::test]
async fn test_auditor_rule_detected_skips_llm() {
    let llm = MockLlmAuditProvider::new((RiskLevel::Safe, vec![], None));
    let call_count = llm.call_count.clone();

    let mut auditor = LlmSecurityAuditor::with_llm(Box::new(llm));

    // 规则引擎可识别的注入
    let result = auditor.audit("' OR 1=1").await.unwrap();

    // LLM 不应被调用
    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(result.risk_level, RiskLevel::Critical);
    assert_eq!(result.source, AuditSource::Rule);
}

#[tokio::test]
async fn test_auditor_new_pattern_added_to_store() {
    let llm = MockLlmAuditProvider::new((
        RiskLevel::High,
        vec!["new_attack".to_string()],
        Some("修复建议".to_string()),
    ));

    let mut auditor = LlmSecurityAuditor::with_llm(Box::new(llm));

    let initial_count = auditor.pattern_store().len();

    auditor.audit("some_novel_attack_vector").await.unwrap();

    assert_eq!(auditor.pattern_store().len(), initial_count + 1);
}

#[tokio::test]
async fn test_auditor_llm_medium_risk_not_persisted() {
    // 中风险不应持久化为新模式（只有 High/Critical 才入库）
    let llm = MockLlmAuditProvider::new((RiskLevel::Medium, vec!["medium_risk".to_string()], None));

    let mut auditor = LlmSecurityAuditor::with_llm(Box::new(llm));

    let initial_count = auditor.pattern_store().len();

    let result = auditor.audit("medium_risk_input").await.unwrap();

    assert_eq!(result.risk_level, RiskLevel::Medium);
    assert!(!result.is_new_pattern);
    assert_eq!(auditor.pattern_store().len(), initial_count);
}
