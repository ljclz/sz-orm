//! 增强脱敏审计：带时间戳的详细审计日志、统计分析、合规报告。
//!
//! - [`MaskingAuditEntry`] — 单条审计记录（字段、原始长度、脱敏后长度、时间戳、操作者）
//! - [`MaskingAuditLog`] — 审计日志集合，支持查询、统计、导出
//! - [`MaskingReport`] — 脱敏合规报告生成器

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::MaskingRule;

// ============================================================================
// 增强审计条目
// ============================================================================

/// 脱敏审计条目：记录单次脱敏操作的完整信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingAuditEntry {
    field: String,
    rule: String,
    original_len: usize,
    masked_len: usize,
    timestamp: u64,
    operator: String,
    session_id: String,
}

impl MaskingAuditEntry {
    /// 创建审计条目
    pub fn new(
        field: &str,
        rule: &str,
        original_len: usize,
        masked_len: usize,
        timestamp: u64,
    ) -> Self {
        Self {
            field: field.to_string(),
            rule: rule.to_string(),
            original_len,
            masked_len,
            timestamp,
            operator: String::new(),
            session_id: String::new(),
        }
    }

    /// 设置操作者（链式）
    pub fn with_operator(mut self, operator: &str) -> Self {
        self.operator = operator.to_string();
        self
    }

    /// 设置会话 ID（链式）
    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = session_id.to_string();
        self
    }

    /// 字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 脱敏规则名
    pub fn rule(&self) -> &str {
        &self.rule
    }

    /// 原始值长度
    pub fn original_len(&self) -> usize {
        self.original_len
    }

    /// 脱敏后长度
    pub fn masked_len(&self) -> usize {
        self.masked_len
    }

    /// 时间戳
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// 操作者
    pub fn operator(&self) -> &str {
        &self.operator
    }

    /// 会话 ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 被掩码的字符数
    pub fn masked_chars(&self) -> usize {
        self.original_len.saturating_sub(self.masked_len)
    }

    /// 是否被截断
    pub fn was_truncated(&self) -> bool {
        self.masked_len < self.original_len
    }

    /// 是否被扩展（脱敏后更长）
    pub fn was_extended(&self) -> bool {
        self.masked_len > self.original_len
    }
}

// ============================================================================
// 增强审计日志
// ============================================================================

/// 脱敏审计日志：记录所有脱敏操作，支持查询、统计、导出。
#[derive(Debug, Clone, Default)]
pub struct MaskingAuditLog {
    entries: Vec<MaskingAuditEntry>,
    max_entries: usize,
}

impl MaskingAuditLog {
    /// 创建空审计日志（无上限）
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建有上限的审计日志（超出时丢弃最旧条目）
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries),
            max_entries,
        }
    }

    /// 记录一次脱敏操作
    pub fn log(&mut self, entry: MaskingAuditEntry) {
        if self.max_entries > 0 && self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    /// 便捷记录：字段名 + 规则 + 原始值 + 脱敏值 + 时间戳
    pub fn log_simple(
        &mut self,
        field: &str,
        rule: &str,
        original: &str,
        masked: &str,
        timestamp: u64,
    ) {
        self.log(MaskingAuditEntry::new(
            field,
            rule,
            original.chars().count(),
            masked.chars().count(),
            timestamp,
        ));
    }

    /// 条目数
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// 所有条目
    pub fn entries(&self) -> &[MaskingAuditEntry] {
        &self.entries
    }

    /// 清空日志
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 按字段名查询条目
    pub fn find_by_field(&self, field: &str) -> Vec<&MaskingAuditEntry> {
        self.entries.iter().filter(|e| e.field() == field).collect()
    }

    /// 按时间范围查询条目
    pub fn find_by_time_range(&self, start: u64, end: u64) -> Vec<&MaskingAuditEntry> {
        self.entries
            .iter()
            .filter(|e| (start..=end).contains(&e.timestamp()))
            .collect()
    }

    /// 按操作者查询条目
    pub fn find_by_operator(&self, operator: &str) -> Vec<&MaskingAuditEntry> {
        self.entries
            .iter()
            .filter(|e| e.operator() == operator)
            .collect()
    }

    /// 被掩码的字符总数
    pub fn total_masked_chars(&self) -> usize {
        self.entries.iter().map(|e| e.masked_chars()).sum()
    }

    /// 涉及的不同字段数
    pub fn unique_field_count(&self) -> usize {
        let mut fields: Vec<&str> = self.entries.iter().map(|e| e.field()).collect();
        fields.sort_unstable();
        fields.dedup();
        fields.len()
    }

    /// 各字段的脱敏次数
    pub fn field_stats(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        for entry in &self.entries {
            *stats.entry(entry.field().to_string()).or_insert(0) += 1;
        }
        stats
    }

    /// 各脱敏规则的使用次数
    pub fn rule_stats(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        for entry in &self.entries {
            *stats.entry(entry.rule().to_string()).or_insert(0) += 1;
        }
        stats
    }

    /// 导出为 JSON 字符串
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.entries).unwrap_or_else(|_| "[]".to_string())
    }
}

// ============================================================================
// 脱敏合规报告
// ============================================================================

/// 脱敏合规报告：汇总审计日志，生成统计报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingReport {
    total_operations: usize,
    total_masked_chars: usize,
    unique_fields: usize,
    field_breakdown: Vec<FieldReport>,
    rule_breakdown: Vec<RuleReport>,
    truncated_count: usize,
    extended_count: usize,
}

/// 单字段的报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldReport {
    field: String,
    count: usize,
    masked_chars: usize,
}

/// 单规则的报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleReport {
    rule: String,
    count: usize,
}

impl MaskingReport {
    /// 从审计日志生成报告
    pub fn from_audit_log(log: &MaskingAuditLog) -> Self {
        let entries = log.entries();
        let total_operations = entries.len();
        let total_masked_chars = entries.iter().map(|e| e.masked_chars()).sum();
        let truncated_count = entries.iter().filter(|e| e.was_truncated()).count();
        let extended_count = entries.iter().filter(|e| e.was_extended()).count();

        let mut field_map: HashMap<String, (usize, usize)> = HashMap::new();
        for entry in entries {
            let slot = field_map.entry(entry.field().to_string()).or_insert((0, 0));
            slot.0 += 1;
            slot.1 += entry.masked_chars();
        }
        let mut field_breakdown: Vec<FieldReport> = field_map
            .into_iter()
            .map(|(field, (count, masked_chars))| FieldReport {
                field,
                count,
                masked_chars,
            })
            .collect();
        field_breakdown.sort_by_key(|b| std::cmp::Reverse(b.count));

        let mut rule_map: HashMap<String, usize> = HashMap::new();
        for entry in entries {
            *rule_map.entry(entry.rule().to_string()).or_insert(0) += 1;
        }
        let mut rule_breakdown: Vec<RuleReport> = rule_map
            .into_iter()
            .map(|(rule, count)| RuleReport { rule, count })
            .collect();
        rule_breakdown.sort_by_key(|b| std::cmp::Reverse(b.count));

        let unique_fields = field_breakdown.len();

        Self {
            total_operations,
            total_masked_chars,
            unique_fields,
            field_breakdown,
            rule_breakdown,
            truncated_count,
            extended_count,
        }
    }

    /// 总操作数
    pub fn total_operations(&self) -> usize {
        self.total_operations
    }

    /// 被掩码字符总数
    pub fn total_masked_chars(&self) -> usize {
        self.total_masked_chars
    }

    /// 涉及字段数
    pub fn unique_fields(&self) -> usize {
        self.unique_fields
    }

    /// 被截断的条目数
    pub fn truncated_count(&self) -> usize {
        self.truncated_count
    }

    /// 被扩展的条目数
    pub fn extended_count(&self) -> usize {
        self.extended_count
    }

    /// 字段报告
    pub fn field_breakdown(&self) -> &[FieldReport] {
        &self.field_breakdown
    }

    /// 规则报告
    pub fn rule_breakdown(&self) -> &[RuleReport] {
        &self.rule_breakdown
    }

    /// 导出为 JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

impl FieldReport {
    /// 字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 脱敏次数
    pub fn count(&self) -> usize {
        self.count
    }

    /// 掩码字符数
    pub fn masked_chars(&self) -> usize {
        self.masked_chars
    }
}

impl RuleReport {
    /// 规则名
    pub fn rule(&self) -> &str {
        &self.rule
    }

    /// 使用次数
    pub fn count(&self) -> usize {
        self.count
    }
}

// ============================================================================
// 规则名辅助
// ============================================================================

/// 获取脱敏规则的可读名称
pub fn rule_name(rule: &MaskingRule) -> &'static str {
    match rule {
        MaskingRule::Phone => "phone",
        MaskingRule::Email => "email",
        MaskingRule::IdCard => "idcard",
        MaskingRule::BankCard => "bankcard",
        MaskingRule::Name => "name",
        MaskingRule::Address => "address",
        MaskingRule::Ip => "ip",
        MaskingRule::Imei => "imei",
        MaskingRule::Plate => "plate",
        MaskingRule::Custom(_) => "custom",
        MaskingRule::Password => "password",
        MaskingRule::ApiKey => "apikey",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- MaskingAuditEntry -----

    #[test]
    fn audit_entry_new() {
        let e = MaskingAuditEntry::new("phone", "phone", 11, 11, 1000);
        assert_eq!(e.field(), "phone");
        assert_eq!(e.rule(), "phone");
        assert_eq!(e.original_len(), 11);
        assert_eq!(e.masked_len(), 11);
        assert_eq!(e.timestamp(), 1000);
    }

    #[test]
    fn audit_entry_with_operator() {
        let e = MaskingAuditEntry::new("phone", "phone", 11, 11, 0).with_operator("admin");
        assert_eq!(e.operator(), "admin");
    }

    #[test]
    fn audit_entry_with_session() {
        let e = MaskingAuditEntry::new("phone", "phone", 11, 11, 0).with_session("sess123");
        assert_eq!(e.session_id(), "sess123");
    }

    #[test]
    fn audit_entry_masked_chars() {
        let e = MaskingAuditEntry::new("phone", "phone", 11, 7, 0);
        assert_eq!(e.masked_chars(), 4);
    }

    #[test]
    fn audit_entry_was_truncated() {
        let e = MaskingAuditEntry::new("phone", "phone", 11, 7, 0);
        assert!(e.was_truncated());
    }

    #[test]
    fn audit_entry_was_extended() {
        let e = MaskingAuditEntry::new("phone", "phone", 5, 15, 0);
        assert!(e.was_extended());
    }

    #[test]
    fn audit_entry_not_truncated_not_extended() {
        let e = MaskingAuditEntry::new("phone", "phone", 11, 11, 0);
        assert!(!e.was_truncated());
        assert!(!e.was_extended());
    }

    // ----- MaskingAuditLog -----

    #[test]
    fn audit_log_new_empty() {
        let log = MaskingAuditLog::new();
        assert_eq!(log.entry_count(), 0);
    }

    #[test]
    fn audit_log_log_entry() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "phone", 11, 11, 0));
        assert_eq!(log.entry_count(), 1);
    }

    #[test]
    fn audit_log_log_simple() {
        let mut log = MaskingAuditLog::new();
        log.log_simple("phone", "phone", "13812345678", "138****5678", 0);
        assert_eq!(log.entry_count(), 1);
    }

    #[test]
    fn audit_log_clear() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "phone", 11, 11, 0));
        log.clear();
        assert_eq!(log.entry_count(), 0);
    }

    #[test]
    fn audit_log_find_by_field() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "phone", 11, 11, 0));
        log.log(MaskingAuditEntry::new("email", "email", 15, 15, 0));
        log.log(MaskingAuditEntry::new("phone", "phone", 11, 11, 1));
        let results = log.find_by_field("phone");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn audit_log_find_by_time_range() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("a", "r", 1, 1, 100));
        log.log(MaskingAuditEntry::new("b", "r", 1, 1, 200));
        log.log(MaskingAuditEntry::new("c", "r", 1, 1, 300));
        let results = log.find_by_time_range(150, 250);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn audit_log_find_by_operator() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("a", "r", 1, 1, 0).with_operator("admin"));
        log.log(MaskingAuditEntry::new("b", "r", 1, 1, 0).with_operator("user"));
        let results = log.find_by_operator("admin");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn audit_log_total_masked_chars() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("a", "r", 10, 6, 0));
        log.log(MaskingAuditEntry::new("b", "r", 8, 4, 0));
        assert_eq!(log.total_masked_chars(), 8);
    }

    #[test]
    fn audit_log_unique_field_count() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "r", 1, 1, 0));
        log.log(MaskingAuditEntry::new("email", "r", 1, 1, 0));
        log.log(MaskingAuditEntry::new("phone", "r", 1, 1, 0));
        assert_eq!(log.unique_field_count(), 2);
    }

    #[test]
    fn audit_log_field_stats() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "r", 1, 1, 0));
        log.log(MaskingAuditEntry::new("phone", "r", 1, 1, 0));
        log.log(MaskingAuditEntry::new("email", "r", 1, 1, 0));
        let stats = log.field_stats();
        assert_eq!(stats["phone"], 2);
        assert_eq!(stats["email"], 1);
    }

    #[test]
    fn audit_log_rule_stats() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("a", "phone", 1, 1, 0));
        log.log(MaskingAuditEntry::new("b", "email", 1, 1, 0));
        log.log(MaskingAuditEntry::new("c", "phone", 1, 1, 0));
        let stats = log.rule_stats();
        assert_eq!(stats["phone"], 2);
        assert_eq!(stats["email"], 1);
    }

    #[test]
    fn audit_log_to_json() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "phone", 11, 11, 0));
        let json = log.to_json();
        assert!(json.contains("phone"));
    }

    #[test]
    fn audit_log_capacity_eviction() {
        let mut log = MaskingAuditLog::with_capacity(2);
        log.log(MaskingAuditEntry::new("a", "r", 1, 1, 0));
        log.log(MaskingAuditEntry::new("b", "r", 1, 1, 0));
        log.log(MaskingAuditEntry::new("c", "r", 1, 1, 0));
        assert_eq!(log.entry_count(), 2);
        assert_eq!(log.entries()[0].field(), "b");
    }

    // ----- MaskingReport -----

    #[test]
    fn report_empty_log() {
        let log = MaskingAuditLog::new();
        let report = MaskingReport::from_audit_log(&log);
        assert_eq!(report.total_operations(), 0);
        assert_eq!(report.total_masked_chars(), 0);
        assert_eq!(report.unique_fields(), 0);
    }

    #[test]
    fn report_total_operations() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("a", "r", 10, 6, 0));
        log.log(MaskingAuditEntry::new("b", "r", 8, 4, 0));
        let report = MaskingReport::from_audit_log(&log);
        assert_eq!(report.total_operations(), 2);
        assert_eq!(report.total_masked_chars(), 8);
    }

    #[test]
    fn report_unique_fields() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "r", 1, 1, 0));
        log.log(MaskingAuditEntry::new("email", "r", 1, 1, 0));
        let report = MaskingReport::from_audit_log(&log);
        assert_eq!(report.unique_fields(), 2);
    }

    #[test]
    fn report_truncated_count() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("a", "r", 10, 6, 0));
        log.log(MaskingAuditEntry::new("b", "r", 5, 5, 0));
        let report = MaskingReport::from_audit_log(&log);
        assert_eq!(report.truncated_count(), 1);
    }

    #[test]
    fn report_extended_count() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("a", "r", 5, 15, 0));
        log.log(MaskingAuditEntry::new("b", "r", 5, 5, 0));
        let report = MaskingReport::from_audit_log(&log);
        assert_eq!(report.extended_count(), 1);
    }

    #[test]
    fn report_field_breakdown() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "r", 10, 6, 0));
        log.log(MaskingAuditEntry::new("phone", "r", 10, 6, 0));
        log.log(MaskingAuditEntry::new("email", "r", 15, 13, 0));
        let report = MaskingReport::from_audit_log(&log);
        let fields = report.field_breakdown();
        assert_eq!(fields.len(), 2);
        // phone 有 2 次，排第一
        assert_eq!(fields[0].field(), "phone");
        assert_eq!(fields[0].count(), 2);
    }

    #[test]
    fn report_rule_breakdown() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("a", "phone", 1, 1, 0));
        log.log(MaskingAuditEntry::new("b", "phone", 1, 1, 0));
        log.log(MaskingAuditEntry::new("c", "email", 1, 1, 0));
        let report = MaskingReport::from_audit_log(&log);
        let rules = report.rule_breakdown();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].rule(), "phone");
        assert_eq!(rules[0].count(), 2);
    }

    #[test]
    fn report_to_json() {
        let mut log = MaskingAuditLog::new();
        log.log(MaskingAuditEntry::new("phone", "phone", 11, 11, 0));
        let report = MaskingReport::from_audit_log(&log);
        let json = report.to_json();
        assert!(json.contains("total_operations"));
    }

    // ----- rule_name -----

    #[test]
    fn rule_name_all_variants() {
        assert_eq!(rule_name(&MaskingRule::Phone), "phone");
        assert_eq!(rule_name(&MaskingRule::Email), "email");
        assert_eq!(rule_name(&MaskingRule::IdCard), "idcard");
        assert_eq!(rule_name(&MaskingRule::BankCard), "bankcard");
        assert_eq!(rule_name(&MaskingRule::Name), "name");
        assert_eq!(rule_name(&MaskingRule::Address), "address");
        assert_eq!(rule_name(&MaskingRule::Ip), "ip");
        assert_eq!(rule_name(&MaskingRule::Imei), "imei");
        assert_eq!(rule_name(&MaskingRule::Plate), "plate");
        assert_eq!(rule_name(&MaskingRule::Custom("3,2".into())), "custom");
        assert_eq!(rule_name(&MaskingRule::Password), "password");
        assert_eq!(rule_name(&MaskingRule::ApiKey), "apikey");
    }
}
