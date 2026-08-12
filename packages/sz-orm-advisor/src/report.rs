//! JSON 报告输出（`query-advisor` feature）
//!
//! 将优化建议列表序列化为 JSON，可被 CI/IDE 消费。

use crate::suggestion::OptimizationSuggestion;
use serde::{Deserialize, Serialize};

/// JSON 报告结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorReport {
    /// 报告生成时间（ISO 8601）
    pub generated_at: String,
    /// 建议列表
    pub suggestions: Vec<OptimizationSuggestion>,
    /// 建议总数
    pub total: usize,
    /// 需人工确认的建议数
    pub needs_confirmation: usize,
}

/// 将建议列表序列化为 JSON 报告字符串
pub fn to_json(suggestions: &[OptimizationSuggestion]) -> String {
    let report = AdvisorReport {
        generated_at: now_iso8601(),
        needs_confirmation: suggestions
            .iter()
            .filter(|s| s.needs_manual_confirmation())
            .count(),
        total: suggestions.len(),
        suggestions: suggestions.to_vec(),
    };
    serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into())
}

/// 从 JSON 反序列化为建议列表
pub fn from_json(json: &str) -> Result<AdvisorReport, serde_json::Error> {
    serde_json::from_str(json)
}

fn now_iso8601() -> String {
    let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return "1970-01-01T00:00:00Z".into();
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
    use crate::suggestion::SuggestionType;

    fn sample_suggestion(confidence: f64) -> OptimizationSuggestion {
        OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "SELECT * FROM users".into(),
            description: "test".into(),
            action: "CREATE INDEX idx ON users(id)".into(),
            confidence,
            estimated_improvement: Some("90% faster".into()),
            conflict_note: None,
        }
    }

    #[test]
    fn json_roundtrip() {
        let suggestions = vec![
            sample_suggestion(0.9),
            sample_suggestion(0.3),
            sample_suggestion(0.7),
        ];
        let json = to_json(&suggestions);
        let report = from_json(&json).unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.suggestions.len(), 3);
        assert_eq!(report.needs_confirmation, 1);
    }

    #[test]
    fn empty_suggestions() {
        let json = to_json(&[]);
        let report = from_json(&json).unwrap();
        assert_eq!(report.total, 0);
        assert!(report.suggestions.is_empty());
    }

    #[test]
    fn needs_confirmation_count() {
        let suggestions = vec![sample_suggestion(0.9), sample_suggestion(0.3)];
        let json = to_json(&suggestions);
        let report = from_json(&json).unwrap();
        assert_eq!(report.needs_confirmation, 1);
    }
}
