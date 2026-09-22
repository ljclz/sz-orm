//! 闭环中断检测器：分析自治执行历史，连续中断 ≥ 3 轮告警
//!
//! 告警码 `AI_AUTONOMOUS_LOOP_BROKEN`，附中断点和重试信息。

use std::time::Instant;

use serde::{Deserialize, Serialize};

/// 中断原因
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BreakReason {
    ExecutionFailed,
    VerificationFailed,
    Timeout,
    OutOfBound,
    TakenOver,
}

/// 闭环执行记录（用于中断检测）
#[derive(Debug, Clone)]
pub struct LoopRecord {
    pub round: u64,
    pub timestamp: Instant,
    pub success: bool,
    pub break_reason: Option<BreakReason>,
    pub retry_count: u32,
}

/// 中断告警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopBreakAlert {
    pub alert_code: String,
    pub consecutive_breaks: usize,
    pub break_rounds: Vec<u64>,
    pub last_reason: BreakReason,
    pub total_retries: u32,
    pub suggestion: String,
}

/// 闭环中断检测器
pub struct AutonomousLoopBreakDetector {
    max_consecutive_breaks: usize,
}

impl AutonomousLoopBreakDetector {
    pub fn new(max_consecutive_breaks: usize) -> Self {
        Self {
            max_consecutive_breaks: max_consecutive_breaks.max(1),
        }
    }

    pub fn max_consecutive_breaks(&self) -> usize {
        self.max_consecutive_breaks
    }

    /// 分析历史，连续中断达阈值返回告警
    pub fn check(&self, history: &[LoopRecord]) -> Option<LoopBreakAlert> {
        if history.is_empty() {
            return None;
        }
        let mut consecutive = 0usize;
        let mut break_rounds = Vec::new();
        let mut total_retries = 0u32;
        let mut last_reason = BreakReason::ExecutionFailed;
        for record in history {
            if record.success {
                consecutive = 0;
                break_rounds.clear();
            } else {
                consecutive += 1;
                break_rounds.push(record.round);
                total_retries += record.retry_count;
                if let Some(reason) = &record.break_reason {
                    last_reason = reason.clone();
                }
            }
        }
        if consecutive < self.max_consecutive_breaks {
            return None;
        }
        let suggestion = match last_reason {
            BreakReason::ExecutionFailed => "检查动作执行器配置和依赖服务可用性".to_string(),
            BreakReason::VerificationFailed => "检查验证阈值和健康检查端点".to_string(),
            BreakReason::Timeout => "增大单轮超时或优化执行链路".to_string(),
            BreakReason::OutOfBound => "调整边界约束或排查异常输入".to_string(),
            BreakReason::TakenOver => "等待人工接管释放后恢复自治".to_string(),
        };
        Some(LoopBreakAlert {
            alert_code: "AI_AUTONOMOUS_LOOP_BROKEN".to_string(),
            consecutive_breaks: consecutive,
            break_rounds,
            last_reason,
            total_retries,
            suggestion,
        })
    }
}

impl Default for AutonomousLoopBreakDetector {
    fn default() -> Self {
        Self::new(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(round: u64, success: bool, reason: Option<BreakReason>) -> LoopRecord {
        LoopRecord {
            round,
            timestamp: Instant::now(),
            success,
            break_reason: reason,
            retry_count: if success { 0 } else { 1 },
        }
    }

    #[test]
    fn test_empty_history_no_alert() {
        let detector = AutonomousLoopBreakDetector::default();
        assert!(detector.check(&[]).is_none());
    }

    #[test]
    fn test_consecutive_3_breaks_alert() {
        let detector = AutonomousLoopBreakDetector::default();
        let history = vec![
            make_record(1, false, Some(BreakReason::ExecutionFailed)),
            make_record(2, false, Some(BreakReason::ExecutionFailed)),
            make_record(3, false, Some(BreakReason::ExecutionFailed)),
        ];
        let alert = detector.check(&history).unwrap();
        assert_eq!(alert.alert_code, "AI_AUTONOMOUS_LOOP_BROKEN");
        assert_eq!(alert.consecutive_breaks, 3);
        assert_eq!(alert.break_rounds, vec![1, 2, 3]);
        assert_eq!(alert.total_retries, 3);
    }

    #[test]
    fn test_below_threshold_no_alert() {
        let detector = AutonomousLoopBreakDetector::default();
        let history = vec![
            make_record(1, false, Some(BreakReason::ExecutionFailed)),
            make_record(2, false, Some(BreakReason::ExecutionFailed)),
        ];
        assert!(detector.check(&history).is_none());
    }

    #[test]
    fn test_success_resets_counter() {
        let detector = AutonomousLoopBreakDetector::default();
        let history = vec![
            make_record(1, false, Some(BreakReason::ExecutionFailed)),
            make_record(2, false, Some(BreakReason::ExecutionFailed)),
            make_record(3, true, None),
            make_record(4, false, Some(BreakReason::Timeout)),
        ];
        assert!(detector.check(&history).is_none());
    }

    #[test]
    fn test_break_after_success_alert() {
        let detector = AutonomousLoopBreakDetector::default();
        let history = vec![
            make_record(1, true, None),
            make_record(2, false, Some(BreakReason::Timeout)),
            make_record(3, false, Some(BreakReason::Timeout)),
            make_record(4, false, Some(BreakReason::Timeout)),
        ];
        let alert = detector.check(&history).unwrap();
        assert_eq!(alert.consecutive_breaks, 3);
        assert_eq!(alert.break_rounds, vec![2, 3, 4]);
        assert!(matches!(alert.last_reason, BreakReason::Timeout));
    }

    #[test]
    fn test_custom_threshold() {
        let detector = AutonomousLoopBreakDetector::new(5);
        let history: Vec<_> = (1..=4)
            .map(|r| make_record(r, false, Some(BreakReason::ExecutionFailed)))
            .collect();
        assert!(detector.check(&history).is_none());
        let history: Vec<_> = (1..=5)
            .map(|r| make_record(r, false, Some(BreakReason::ExecutionFailed)))
            .collect();
        assert!(detector.check(&history).is_some());
    }

    #[test]
    fn test_suggestion_by_reason() {
        let detector = AutonomousLoopBreakDetector::default();
        let reasons = vec![
            BreakReason::ExecutionFailed,
            BreakReason::VerificationFailed,
            BreakReason::Timeout,
            BreakReason::OutOfBound,
            BreakReason::TakenOver,
        ];
        for reason in reasons {
            let history = vec![
                make_record(1, false, Some(reason.clone())),
                make_record(2, false, Some(reason.clone())),
                make_record(3, false, Some(reason.clone())),
            ];
            let alert = detector.check(&history).unwrap();
            assert!(!alert.suggestion.is_empty());
        }
    }
}
