//! 脱敏策略推荐+复核（TASK-020）

use crate::types::GovernanceError;
use serde::{Deserialize, Serialize};

/// 脱敏策略类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MaskingStrategy {
    Mask,
    Hash,
    Replace,
}

/// 脱敏推荐结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingRecommendation {
    pub field: String,
    pub strategy: MaskingStrategy,
    pub reason: String,
}

/// 脱敏推荐器
pub struct MaskingRecommender;

impl MaskingRecommender {
    pub fn new() -> Self {
        Self
    }

    pub fn recommend(&self, fields: &[(&str, &str)]) -> Vec<MaskingRecommendation> {
        let mut recommendations = Vec::new();
        for (field, field_type) in fields {
            let (strategy, reason) = if Self::is_phone(field) {
                (MaskingStrategy::Mask, "手机号需掩码脱敏".to_string())
            } else if Self::is_email(field) {
                (MaskingStrategy::Mask, "邮箱需掩码脱敏".to_string())
            } else if Self::is_id_card(field) {
                (MaskingStrategy::Hash, "身份证号需哈希脱敏".to_string())
            } else if *field_type == "PASSWORD" || Self::is_password(field) {
                (MaskingStrategy::Hash, "密码需哈希脱敏".to_string())
            } else {
                (MaskingStrategy::Replace, "普通字段替换脱敏".to_string())
            };
            recommendations.push(MaskingRecommendation {
                field: field.to_string(),
                strategy,
                reason,
            });
        }
        recommendations
    }

    pub fn review(
        &self,
        recommendations: &[MaskingRecommendation],
        all_fields: &[(&str, &str)],
    ) -> Result<(), GovernanceError> {
        let recommended_fields: std::collections::HashSet<_> =
            recommendations.iter().map(|r| r.field.as_str()).collect();
        let mut missing = Vec::new();
        for (field, _) in all_fields {
            if Self::is_sensitive(field) && !recommended_fields.contains(*field) {
                missing.push(field.to_string());
            }
        }
        if !missing.is_empty() {
            return Err(GovernanceError::ComplianceAuditFailed(format!(
                "遗漏敏感字段: {}",
                missing.join(", ")
            )));
        }
        Ok(())
    }

    fn is_phone(field: &str) -> bool {
        let f = field.to_lowercase();
        f.contains("phone") || f.contains("mobile") || f.contains("tel")
    }

    fn is_email(field: &str) -> bool {
        field.to_lowercase().contains("email") || field.to_lowercase().contains("mail")
    }

    fn is_id_card(field: &str) -> bool {
        let f = field.to_lowercase();
        f.contains("id_card") || f.contains("idcard") || f.contains("identity")
    }

    fn is_password(field: &str) -> bool {
        field.to_lowercase().contains("password") || field.to_lowercase().contains("pwd")
    }

    fn is_sensitive(field: &str) -> bool {
        Self::is_phone(field)
            || Self::is_email(field)
            || Self::is_id_card(field)
            || Self::is_password(field)
    }
}

impl Default for MaskingRecommender {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommend_phone() {
        let recommender = MaskingRecommender::new();
        let recs = recommender.recommend(&[("phone_number", "VARCHAR")]);
        assert_eq!(recs[0].strategy, MaskingStrategy::Mask);
    }

    #[test]
    fn test_review_missing_sensitive() {
        let recommender = MaskingRecommender::new();
        let recs = recommender.recommend(&[("name", "VARCHAR")]);
        let result = recommender.review(&recs, &[("name", "VARCHAR"), ("phone", "VARCHAR")]);
        assert!(result.is_err(), "遗漏 phone 应报错");
    }

    #[test]
    fn test_review_pass() {
        let recommender = MaskingRecommender::new();
        let recs = recommender.recommend(&[("phone", "VARCHAR"), ("name", "VARCHAR")]);
        let result = recommender.review(&recs, &[("phone", "VARCHAR"), ("name", "VARCHAR")]);
        assert!(result.is_ok());
    }
}
