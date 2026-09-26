//! A/B 统计显著性引擎：基于 Welch t 检验判定实验组/对照组差异显著性
//!
//! p < 0.05 判定显著，产出胜出方案 + p 值 + 置信度。
//! 复用既有 `AbTestOrchestrator` 分流结果。

use serde::{Deserialize, Serialize};

use super::XaiError;

/// A/B 实验组标识
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbGroup {
    Control,
    Treatment,
}

/// A/B 实验组数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbGroupData {
    pub group: AbGroup,
    pub sample_size: usize,
    pub successes: usize,
    pub failures: usize,
}

impl AbGroupData {
    pub fn success_rate(&self) -> f64 {
        if self.sample_size == 0 {
            return 0.0;
        }
        self.successes as f64 / self.sample_size as f64
    }

    pub fn variance(&self) -> f64 {
        if self.sample_size <= 1 {
            return 0.0;
        }
        let p = self.success_rate();
        p * (1.0 - p)
    }
}

/// A/B 实验数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbExperimentData {
    pub experiment_id: String,
    pub control: AbGroupData,
    pub treatment: AbGroupData,
}

/// 显著性判定结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AbSignificanceResult {
    Significant {
        winner: AbGroup,
        p_value: f64,
        confidence: f64,
    },
    NotSignificant {
        p_value: f64,
        suggestion: String,
    },
    SampleInsufficient {
        current: usize,
        target: usize,
    },
}

/// A/B 统计显著性引擎
pub struct AbStatSignificanceEngine {
    min_sample_size: usize,
    significance_level: f64,
}

impl AbStatSignificanceEngine {
    pub fn new(min_sample_size: usize, significance_level: f64) -> Self {
        Self {
            min_sample_size: min_sample_size.max(1),
            significance_level: significance_level.clamp(0.001, 0.999),
        }
    }

    pub fn min_sample_size(&self) -> usize {
        self.min_sample_size
    }

    pub fn significance_level(&self) -> f64 {
        self.significance_level
    }

    /// 评估 A/B 实验显著性
    pub async fn evaluate(
        &self,
        experiment: &AbExperimentData,
    ) -> Result<AbSignificanceResult, XaiError> {
        let total = experiment.control.sample_size + experiment.treatment.sample_size;
        if experiment.control.sample_size < self.min_sample_size
            || experiment.treatment.sample_size < self.min_sample_size
        {
            return Ok(AbSignificanceResult::SampleInsufficient {
                current: total,
                target: self.min_sample_size * 2,
            });
        }
        let p_value = self.welch_t_test_p_value(
            experiment.control.success_rate(),
            experiment.treatment.success_rate(),
            experiment.control.variance(),
            experiment.treatment.variance(),
            experiment.control.sample_size,
            experiment.treatment.sample_size,
        );
        if p_value < self.significance_level {
            let winner = if experiment.treatment.success_rate() > experiment.control.success_rate()
            {
                AbGroup::Treatment
            } else {
                AbGroup::Control
            };
            let confidence = 1.0 - p_value;
            Ok(AbSignificanceResult::Significant {
                winner,
                p_value,
                confidence,
            })
        } else {
            Ok(AbSignificanceResult::NotSignificant {
                p_value,
                suggestion: format!(
                    "p={:.4} ≥ α={:.2}，差异不显著，建议增加样本量或延长实验周期",
                    p_value, self.significance_level
                ),
            })
        }
    }

    /// Welch t 检验近似 p 值（双侧）
    fn welch_t_test_p_value(
        &self,
        mean_a: f64,
        mean_b: f64,
        var_a: f64,
        var_b: f64,
        n_a: usize,
        n_b: usize,
    ) -> f64 {
        let n_a_f = n_a as f64;
        let n_b_f = n_b as f64;
        let se = (var_a / n_a_f + var_b / n_b_f).sqrt();
        if se < 1e-12 {
            return 1.0;
        }
        let t_stat = (mean_b - mean_a).abs() / se;
        let df_num = (var_a / n_a_f + var_b / n_b_f).powi(2);
        let df_den =
            (var_a / n_a_f).powi(2) / (n_a_f - 1.0) + (var_b / n_b_f).powi(2) / (n_b_f - 1.0);
        let df = if df_den < df_num * 1e-15 {
            n_a_f + n_b_f - 2.0
        } else {
            df_num / df_den
        };
        self.two_sided_t_p_value(t_stat, df)
    }

    /// 双侧 t 检验 p 值：大自由度用正态近似，小自由度用不完全 Beta
    fn two_sided_t_p_value(&self, t: f64, df: f64) -> f64 {
        if t < 1e-12 {
            return 1.0;
        }
        if df > 30.0 {
            return self.normal_two_sided_p_value(t);
        }
        let x = df / (df + t * t);
        let half_df = df / 2.0;
        let beta = self.incomplete_beta(x, half_df, 0.5);
        beta.clamp(0.0, 1.0)
    }

    /// 标准正态双侧 p 值（基于 erf 近似）
    fn normal_two_sided_p_value(&self, t: f64) -> f64 {
        let p = 0.3275911_f64;
        let a1 = 0.254829592_f64;
        let a2 = -0.284496736_f64;
        let a3 = 1.421413741_f64;
        let a4 = -1.453152027_f64;
        let a5 = 1.061405429_f64;
        let ax = (t / std::f64::consts::SQRT_2).abs();
        let tt = 1.0 / (1.0 + p * ax);
        let erf = 1.0
            - (a1 * tt + a2 * tt * tt + a3 * tt.powi(3) + a4 * tt.powi(4) + a5 * tt.powi(5))
                * (-ax * ax).exp();
        // 双侧 p = 2(1 - Φ(|t|))：erf 必须取绝对值，否则负 t 时 cdf<0.5 → p>1
        let cdf = 0.5 * (1.0 + erf.abs());
        2.0 * (1.0 - cdf)
    }

    /// 不完全 Beta 函数近似（连分数展开）
    fn incomplete_beta(&self, x: f64, a: f64, b: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let lbeta = self.lgamma(a + b) - self.lgamma(a) - self.lgamma(b);
        let front = (lbeta + a * x.ln() + b * (1.0 - x).ln()).exp();
        if x < (a + 1.0) / (a + b + 2.0) {
            front * self.beta_cf(x, a, b) / a
        } else {
            1.0 - front * self.beta_cf(1.0 - x, b, a) / b
        }
    }

    /// Beta 连分数（Lentz 算法）
    fn beta_cf(&self, x: f64, a: f64, b: f64) -> f64 {
        const EPS: f64 = 1e-12;
        const MAX_ITER: usize = 200;
        let qab = a + b;
        let qap = a + 1.0;
        let qam = a - 1.0;
        let mut c = 1.0;
        let mut d = 1.0 - qab * x / qap;
        if d.abs() < EPS {
            d = EPS;
        }
        d = 1.0 / d;
        let mut h = d;
        for m in 1..=MAX_ITER {
            let m_f = m as f64;
            let m2 = 2.0 * m_f;
            let aa = m_f * (b - m_f) * x / ((qam + m2) * (a + m2));
            d = 1.0 + aa * d;
            if d.abs() < EPS {
                d = EPS;
            }
            c = 1.0 + aa / c;
            if c.abs() < EPS {
                c = EPS;
            }
            d = 1.0 / d;
            h *= d * c;
            let aa = -(a + m_f) * (qab + m_f) * x / ((a + m2) * (qap + m2));
            d = 1.0 + aa * d;
            if d.abs() < EPS {
                d = EPS;
            }
            c = 1.0 + aa / c;
            if c.abs() < EPS {
                c = EPS;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < EPS {
                break;
            }
        }
        h
    }

    /// Lanczos 近似 ln(Gamma(x))
    fn lgamma(&self, x: f64) -> f64 {
        const G: f64 = 7.0;
        const C: [f64; 9] = [
            0.9999999999998099,
            676.5203681218851,
            -1259.1392167224028,
            771.3234287776531,
            -176.61501716214001,
            12.507343628395565,
            -0.13857109526572012,
            9.984369578019572e-6,
            1.5056327351493116e-7,
        ];
        if x < 0.5 {
            return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln()
                - self.lgamma(1.0 - x);
        }
        let x = x - 1.0;
        let mut a = C[0];
        let t = x + G + 0.5;
        for (i, ci) in C.iter().enumerate().skip(1) {
            a += ci / (x + i as f64);
        }
        0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
    }
}

impl Default for AbStatSignificanceEngine {
    fn default() -> Self {
        Self::new(1000, 0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_group(group: AbGroup, n: usize, successes: usize) -> AbGroupData {
        AbGroupData {
            group,
            sample_size: n,
            successes,
            failures: n - successes,
        }
    }

    #[tokio::test]
    async fn test_significant_treatment_wins() {
        let engine = AbStatSignificanceEngine::new(100, 0.05);
        let exp = AbExperimentData {
            experiment_id: "exp1".to_string(),
            control: make_group(AbGroup::Control, 1000, 400),
            treatment: make_group(AbGroup::Treatment, 1000, 550),
        };
        let result = engine.evaluate(&exp).await.unwrap();
        match result {
            AbSignificanceResult::Significant {
                winner,
                p_value,
                confidence,
            } => {
                assert_eq!(winner, AbGroup::Treatment);
                assert!(p_value < 0.05);
                assert!(confidence > 0.95);
            }
            _ => panic!("Expected Significant"),
        }
    }

    #[tokio::test]
    async fn test_significant_control_wins() {
        let engine = AbStatSignificanceEngine::new(100, 0.05);
        let exp = AbExperimentData {
            experiment_id: "exp2".to_string(),
            control: make_group(AbGroup::Control, 1000, 600),
            treatment: make_group(AbGroup::Treatment, 1000, 400),
        };
        let result = engine.evaluate(&exp).await.unwrap();
        match result {
            AbSignificanceResult::Significant { winner, .. } => {
                assert_eq!(winner, AbGroup::Control);
            }
            _ => panic!("Expected Significant"),
        }
    }

    #[tokio::test]
    async fn test_not_significant() {
        let engine = AbStatSignificanceEngine::new(100, 0.05);
        let exp = AbExperimentData {
            experiment_id: "exp3".to_string(),
            control: make_group(AbGroup::Control, 1000, 500),
            treatment: make_group(AbGroup::Treatment, 1000, 510),
        };
        let result = engine.evaluate(&exp).await.unwrap();
        match result {
            AbSignificanceResult::NotSignificant {
                p_value,
                suggestion,
            } => {
                assert!(p_value >= 0.05);
                assert!(!suggestion.is_empty());
            }
            _ => panic!("Expected NotSignificant"),
        }
    }

    #[tokio::test]
    async fn test_sample_insufficient() {
        let engine = AbStatSignificanceEngine::new(1000, 0.05);
        let exp = AbExperimentData {
            experiment_id: "exp4".to_string(),
            control: make_group(AbGroup::Control, 100, 50),
            treatment: make_group(AbGroup::Treatment, 100, 55),
        };
        let result = engine.evaluate(&exp).await.unwrap();
        match result {
            AbSignificanceResult::SampleInsufficient { current, target } => {
                assert_eq!(current, 200);
                assert_eq!(target, 2000);
            }
            _ => panic!("Expected SampleInsufficient"),
        }
    }

    #[tokio::test]
    async fn test_groups_not_polluted() {
        let engine = AbStatSignificanceEngine::new(100, 0.05);
        let exp = AbExperimentData {
            experiment_id: "exp5".to_string(),
            control: make_group(AbGroup::Control, 500, 200),
            treatment: make_group(AbGroup::Treatment, 500, 300),
        };
        let result = engine.evaluate(&exp).await.unwrap();
        assert!(matches!(result, AbSignificanceResult::Significant { .. }));
        assert_eq!(exp.control.sample_size, 500);
        assert_eq!(exp.treatment.sample_size, 500);
    }

    #[test]
    fn test_success_rate() {
        let g = make_group(AbGroup::Control, 100, 30);
        assert!((g.success_rate() - 0.3).abs() < 1e-9);
    }

    #[test]
    fn test_variance() {
        let g = make_group(AbGroup::Control, 100, 50);
        let expected = 0.5 * 0.5;
        assert!((g.variance() - expected).abs() < 1e-9);
    }

    #[test]
    fn test_zero_sample_rate() {
        let g = AbGroupData {
            group: AbGroup::Control,
            sample_size: 0,
            successes: 0,
            failures: 0,
        };
        assert_eq!(g.success_rate(), 0.0);
        assert_eq!(g.variance(), 0.0);
    }

    #[tokio::test]
    async fn test_large_sample_significant() {
        let engine = AbStatSignificanceEngine::new(1000, 0.05);
        let exp = AbExperimentData {
            experiment_id: "exp6".to_string(),
            control: make_group(AbGroup::Control, 10000, 4000),
            treatment: make_group(AbGroup::Treatment, 10000, 4400),
        };
        let result = engine.evaluate(&exp).await.unwrap();
        match result {
            AbSignificanceResult::Significant {
                winner, p_value, ..
            } => {
                assert_eq!(winner, AbGroup::Treatment);
                assert!(p_value < 0.05);
            }
            _ => panic!("Expected Significant for large sample"),
        }
    }

    // === 数值辅助函数锚定测试（2026-09-26 审计 G22 覆盖率移交项）===

    #[test]
    fn test_lgamma_known_values() {
        let ev = AbStatSignificanceEngine::default();
        // lgamma(5.0) = ln(24)
        assert!((ev.lgamma(5.0) - 24f64.ln()).abs() < 1e-6);
        // 反射分支：lgamma(0.5) = ln(sqrt(pi))
        assert!((ev.lgamma(0.5) - (std::f64::consts::PI.sqrt()).ln()).abs() < 1e-6);
    }

    #[test]
    fn test_incomplete_beta_boundaries_and_symmetry() {
        let ev = AbStatSignificanceEngine::default();
        assert_eq!(ev.incomplete_beta(0.0, 2.0, 2.0), 0.0);
        assert_eq!(ev.incomplete_beta(1.0, 2.0, 2.0), 1.0);
        // a=b=2 对称：x=0.5 → 0.5（连分数正支）
        assert!((ev.incomplete_beta(0.5, 2.0, 2.0) - 0.5).abs() < 1e-6);
        // x > (a+1)/(a+b+2) 的 else 支：Beta(2,2) CDF(0.75) = 3x²-2x³ = 0.84375
        assert!((ev.incomplete_beta(0.75, 2.0, 2.0) - 0.84375).abs() < 1e-6);
    }

    #[test]
    fn test_normal_two_sided_p_value_symmetry() {
        let ev = AbStatSignificanceEngine::default();
        assert!((ev.normal_two_sided_p_value(0.0) - 1.0).abs() < 1e-6);
        assert!((ev.normal_two_sided_p_value(1.96) - 0.05).abs() < 1e-3);
        // 负 t 分支（sign = -1）
        assert!((ev.normal_two_sided_p_value(-1.96) - 0.05).abs() < 1e-3);
    }

    #[test]
    fn test_two_sided_t_p_value_branches() {
        let ev = AbStatSignificanceEngine::default();
        // t≈0 → 1.0
        assert!((ev.two_sided_t_p_value(0.0, 10.0) - 1.0).abs() < 1e-12);
        // 小自由度走不完全 Beta 支
        let p_small_df = ev.two_sided_t_p_value(2.0, 5.0);
        assert!(p_small_df > 0.0 && p_small_df < 0.2, "p={p_small_df}");
        // 大自由度走正态近似支：t=1.96, df=1000 → ≈0.05
        assert!((ev.two_sided_t_p_value(1.96, 1000.0) - 0.05).abs() < 1e-2);
    }
}
