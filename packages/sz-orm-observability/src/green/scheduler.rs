//! 绿色调度器

use super::types::{InstanceCarbonIntensity, ScheduleDecision};

/// 绿色调度器：优先调度到碳排放强度较低的实例
pub struct GreenScheduler {
    latency_constraint_ms: u64,
}

impl GreenScheduler {
    pub fn new(latency_constraint_ms: u64) -> Self {
        Self {
            latency_constraint_ms,
        }
    }

    pub fn schedule(
        &self,
        instances: &[InstanceCarbonIntensity],
        current_latency_ms: u64,
    ) -> ScheduleDecision {
        if instances.is_empty() {
            return ScheduleDecision {
                selected_instance: String::new(),
                reason: "无可用实例".to_string(),
                fallback: true,
            };
        }

        if current_latency_ms > self.latency_constraint_ms {
            let lowest_load = &instances[0];
            return ScheduleDecision {
                selected_instance: lowest_load.instance_id.clone(),
                reason: "GREEN_SCHEDULER_LATENCY_FALLBACK: 延迟超限，回退到负载均衡".to_string(),
                fallback: true,
            };
        }

        let greenest = instances
            .iter()
            .min_by(|a, b| a.carbon_intensity.partial_cmp(&b.carbon_intensity).unwrap())
            .unwrap();

        let max_intensity = instances
            .iter()
            .map(|i| i.carbon_intensity)
            .fold(0.0_f64, f64::max);
        let diff = (max_intensity - greenest.carbon_intensity) / max_intensity.max(1e-10);

        ScheduleDecision {
            selected_instance: greenest.instance_id.clone(),
            reason: if diff > 0.1 {
                format!("绿色调度：碳强度差异 {:.1}%，选择低排放实例", diff * 100.0)
            } else {
                "负载均衡：碳强度差异不显著".to_string()
            },
            fallback: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_instance(id: &str, intensity: f64) -> InstanceCarbonIntensity {
        InstanceCarbonIntensity {
            instance_id: id.to_string(),
            carbon_intensity: intensity,
            region: "us-east".to_string(),
        }
    }

    #[test]
    fn test_schedule_lowest_carbon() {
        let scheduler = GreenScheduler::new(100);
        let instances = vec![
            make_instance("a", 0.8),
            make_instance("b", 0.3),
            make_instance("c", 0.5),
        ];
        let decision = scheduler.schedule(&instances, 50);
        assert_eq!(decision.selected_instance, "b");
        assert!(!decision.fallback);
    }

    #[test]
    fn test_schedule_latency_fallback() {
        let scheduler = GreenScheduler::new(100);
        let instances = vec![make_instance("a", 0.5)];
        let decision = scheduler.schedule(&instances, 150);
        assert!(decision.fallback);
        assert!(decision.reason.contains("GREEN_SCHEDULER_LATENCY_FALLBACK"));
    }

    #[test]
    fn test_schedule_empty() {
        let scheduler = GreenScheduler::new(100);
        let decision = scheduler.schedule(&[], 50);
        assert!(decision.fallback);
    }

    #[test]
    fn test_schedule_similar_intensity() {
        let scheduler = GreenScheduler::new(100);
        let instances = vec![make_instance("a", 0.50), make_instance("b", 0.51)];
        let decision = scheduler.schedule(&instances, 50);
        assert!(!decision.fallback);
    }
}
