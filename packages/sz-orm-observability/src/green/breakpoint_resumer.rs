//! 能耗采集断点续采器

use super::types::EnergyMetrics;

/// 能耗采集断点续采器
pub struct EnergyBreakpointResumer {
    last_timestamp: i64,
    gaps: Vec<(i64, i64)>,
}

impl EnergyBreakpointResumer {
    pub fn new() -> Self {
        Self {
            last_timestamp: 0,
            gaps: Vec::new(),
        }
    }

    pub fn record(&mut self, metrics: &EnergyMetrics) {
        if self.last_timestamp > 0 && metrics.timestamp - self.last_timestamp > 60 {
            self.gaps.push((self.last_timestamp, metrics.timestamp));
        }
        self.last_timestamp = metrics.timestamp;
    }

    pub fn has_gaps(&self) -> bool {
        !self.gaps.is_empty()
    }

    pub fn gap_count(&self) -> usize {
        self.gaps.len()
    }

    pub fn gaps(&self) -> &[(i64, i64)] {
        &self.gaps
    }

    pub fn can_backfill(&self, gap: &(i64, i64)) -> bool {
        gap.1 - gap.0 < 3600
    }
}

impl Default for EnergyBreakpointResumer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(ts: i64) -> EnergyMetrics {
        EnergyMetrics {
            timestamp: ts,
            cpu_energy_kwh: 1.0,
            memory_energy_kwh: 0.5,
            io_energy_kwh: 0.3,
            network_energy_kwh: 0.2,
        }
    }

    #[test]
    fn test_no_gap() {
        let mut resumer = EnergyBreakpointResumer::new();
        resumer.record(&make_metrics(100));
        resumer.record(&make_metrics(110));
        assert!(!resumer.has_gaps());
    }

    #[test]
    fn test_gap_detected() {
        let mut resumer = EnergyBreakpointResumer::new();
        resumer.record(&make_metrics(100));
        resumer.record(&make_metrics(200));
        assert!(resumer.has_gaps());
        assert_eq!(resumer.gap_count(), 1);
    }

    #[test]
    fn test_can_backfill() {
        let mut resumer = EnergyBreakpointResumer::new();
        resumer.record(&make_metrics(100));
        resumer.record(&make_metrics(200));
        assert!(resumer.can_backfill(&resumer.gaps()[0]));
    }
}
