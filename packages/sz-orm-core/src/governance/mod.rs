//! 编译期数据治理（v4.3.0 M3-T3/T4，`compile-governance` feature）
//!
//! 由 [`GovernedModel`] trait + [`Governed`] 派生宏（sz-orm-macros）驱动：
//!
//! - **编译期强制**：`#[pii]` 字段必须声明 `#[mask(strategy = "...")]`，
//!   策略必须在白名单内（hash/partial/replace/encrypt），违反即编译失败
//! - **运行时元数据**：`pii_fields()` 暴露 PII 字段与脱敏策略，
//!   供脱敏执行（sz-orm-masking）与审计（sz-orm-audit）消费
//! - **合规报告**：[`compliance_report`] 生成 GDPR/等保清单（JSON 可审计）
//!
//! ```ignore
//! use sz_orm_core::governance::{compliance_report, GovernedModel};
//!
//! #[derive(sz_orm_macros::Governed)]
//! struct User {
//!     id: i64,
//!     #[pii]
//!     #[mask(strategy = "partial")]
//!     email: String,
//! }
//!
//! let report = compliance_report(&[User::pii_fields()]);
//! ```

/// 数据治理模型 trait（由 `#[derive(Governed)]` 自动实现）
pub trait GovernedModel {
    /// 返回 PII 字段列表：`(字段名, 脱敏策略)`，策略 ∈ {hash, partial, replace, encrypt}
    fn pii_fields() -> Vec<(&'static str, &'static str)>;
}

/// 单条 PII 字段合规条目
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PiiFieldEntry {
    /// 字段名
    pub field: String,
    /// 脱敏策略
    pub strategy: String,
}

/// 合规报告（GDPR / 等保清单）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ComplianceReport {
    /// 全部 PII 字段清单
    pub pii_fields: Vec<PiiFieldEntry>,
    /// 数据保留天数（`None` = 未配置）
    pub retention_days: Option<u32>,
    /// 报告生成时间（ISO 8601）
    pub generated_at: String,
}

impl ComplianceReport {
    /// 序列化为 JSON（供审计工具/CI 消费）
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// PII 字段数量
    pub fn pii_field_count(&self) -> usize {
        self.pii_fields.len()
    }
}

/// 从多个模型的 `pii_fields()` 汇总生成合规报告
pub fn compliance_report(models: &[Vec<(&'static str, &'static str)>]) -> ComplianceReport {
    let mut seen = std::collections::HashSet::new();
    let mut pii_fields = Vec::new();
    for model_fields in models {
        for (field, strategy) in model_fields {
            if seen.insert((*field, *strategy)) {
                pii_fields.push(PiiFieldEntry {
                    field: (*field).to_string(),
                    strategy: (*strategy).to_string(),
                });
            }
        }
    }
    ComplianceReport {
        pii_fields,
        retention_days: None,
        generated_at: now_iso8601(),
    }
}

/// 设置数据保留天数（合规策略配置）
pub fn with_retention(mut report: ComplianceReport, days: u32) -> ComplianceReport {
    report.retention_days = Some(days);
    report
}

/// 当前 UTC 时间 ISO 8601（无外部依赖，Hinnant civil-from-days 算法）
fn now_iso8601() -> String {
    let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return "1970-01-01T00:00:00Z".to_string();
    };
    let secs = d.as_secs();
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct User;
    impl GovernedModel for User {
        fn pii_fields() -> Vec<(&'static str, &'static str)> {
            vec![("email", "partial"), ("phone", "hash")]
        }
    }

    struct Order;
    impl GovernedModel for Order {
        fn pii_fields() -> Vec<(&'static str, &'static str)> {
            vec![("phone", "hash")] // 与 User 重叠，应去重
        }
    }

    #[test]
    fn aggregates_and_deduplicates() {
        let report = compliance_report(&[User::pii_fields(), Order::pii_fields()]);
        assert_eq!(report.pii_field_count(), 2);
        assert!(report.pii_fields.contains(&PiiFieldEntry {
            field: "email".into(),
            strategy: "partial".into()
        }));
        assert!(report.pii_fields.contains(&PiiFieldEntry {
            field: "phone".into(),
            strategy: "hash".into()
        }));
    }

    #[test]
    fn json_output_is_valid() {
        let report = with_retention(compliance_report(&[User::pii_fields()]), 730);
        let json = report.to_json().expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["retention_days"], 730);
        assert_eq!(parsed["pii_fields"].as_array().unwrap().len(), 2);
        assert!(parsed["generated_at"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn empty_models_produce_empty_report() {
        let report = compliance_report(&[]);
        assert_eq!(report.pii_field_count(), 0);
        assert!(report.to_json().is_ok());
    }
}
