//! 碳抵消建议：基于市场数据生成抵消建议（建议不执行交易）

use super::CarbonError;

/// 碳抵消建议结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OffsetSuggestion {
    /// 抵消量（kgCO2e）
    pub offset_amount: f64,
    /// 成本估算（USD）
    pub cost_estimate: f64,
    /// 碳信用类型
    pub credit_type: String,
    /// 是否可执行（恒为 false，建议不执行交易）
    pub executable: bool,
}

/// 碳抵消顾问
///
/// 基于市场数据为不可避免排放生成抵消建议。
/// `executable` 恒为 false，仅提供建议而非执行交易。
pub struct CarbonOffsetAdvisor {
    /// 市场可用标志
    market_available: bool,
    /// 碳信用单价（USD / 吨 CO2e）
    credit_price_per_ton: f64,
    /// 碳信用类型
    credit_type: String,
}

impl CarbonOffsetAdvisor {
    /// 创建新的碳抵消顾问
    ///
    /// `credit_price_per_ton` 为碳信用单价（USD/吨），`credit_type` 为信用类型
    pub fn new(credit_price_per_ton: f64, credit_type: impl Into<String>) -> Self {
        Self {
            market_available: true,
            credit_price_per_ton,
            credit_type: credit_type.into(),
        }
    }

    /// 标记市场数据不可用
    pub fn with_market_unavailable(mut self) -> Self {
        self.market_available = false;
        self
    }

    /// 为不可避免排放生成抵消建议
    ///
    /// `unavoidable_emission` 为不可避免排放量（kgCO2e）
    pub fn advise(&self, unavoidable_emission: f64) -> Result<OffsetSuggestion, CarbonError> {
        if !self.market_available {
            return Err(CarbonError::OffsetDataUnavailable);
        }
        if unavoidable_emission < 0.0 {
            return Err(CarbonError::OffsetDataUnavailable);
        }
        let cost_estimate = unavoidable_emission / 1000.0 * self.credit_price_per_ton;
        Ok(OffsetSuggestion {
            offset_amount: unavoidable_emission,
            cost_estimate,
            credit_type: self.credit_type.clone(),
            executable: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advise_generates_suggestion() {
        let advisor = CarbonOffsetAdvisor::new(50.0, "VCS");
        let suggestion = advisor.advise(2000.0).unwrap();
        assert_eq!(suggestion.offset_amount, 2000.0);
        assert!((suggestion.cost_estimate - 100.0).abs() < 1e-9);
        assert_eq!(suggestion.credit_type, "VCS");
    }

    #[test]
    fn test_executable_always_false() {
        let advisor = CarbonOffsetAdvisor::new(50.0, "VCS");
        let suggestion = advisor.advise(500.0).unwrap();
        assert!(!suggestion.executable, "executable 必须恒为 false");
    }

    #[test]
    fn test_market_unavailable_error() {
        let advisor = CarbonOffsetAdvisor::new(50.0, "VCS").with_market_unavailable();
        let result = advisor.advise(500.0);
        assert!(matches!(result, Err(CarbonError::OffsetDataUnavailable)));
    }

    #[test]
    fn test_negative_emission_error() {
        let advisor = CarbonOffsetAdvisor::new(50.0, "VCS");
        let result = advisor.advise(-100.0);
        assert!(matches!(result, Err(CarbonError::OffsetDataUnavailable)));
    }

    #[test]
    fn test_zero_emission() {
        let advisor = CarbonOffsetAdvisor::new(50.0, "Gold Standard");
        let suggestion = advisor.advise(0.0).unwrap();
        assert_eq!(suggestion.offset_amount, 0.0);
        assert!((suggestion.cost_estimate - 0.0).abs() < 1e-9);
        assert!(!suggestion.executable);
    }
}
