//! 端到端测试：能耗数据脱敏器
//!
//! 验证 EnergyDataMasker 防止泄露基础设施拓扑。

use sz_orm_observability::green::{EnergyDataMasker, EnergyMetrics};

#[test]
fn e2e_energy_mask_instance_id() {
    let masker = EnergyDataMasker::new();
    let masked = masker.mask_instance_id("instance-123456");
    assert!(masked.starts_with("inst"));
    assert!(masked.contains("****"));
    assert!(!masked.contains("123456"), "脱敏后不应包含完整 ID");
}

#[test]
fn e2e_energy_mask_short_id() {
    let masker = EnergyDataMasker::new();
    let masked = masker.mask_instance_id("ab");
    assert_eq!(masked, "****", "短 ID 应完全脱敏");
}

#[test]
fn e2e_energy_mask_region() {
    let masker = EnergyDataMasker::new();
    let masked = masker.mask_region("us-east-1");
    assert!(masked.starts_with("us"));
    assert!(masked.contains("**"));
    assert!(!masked.contains("east-1"), "脱敏后不应包含完整 region");
}

#[test]
fn e2e_energy_mask_metrics_preserved() {
    let masker = EnergyDataMasker::new();
    let metrics = EnergyMetrics {
        timestamp: 1000,
        cpu_energy_kwh: 1.5,
        memory_energy_kwh: 0.5,
        io_energy_kwh: 0.3,
        network_energy_kwh: 0.2,
    };
    let masked = masker.mask_metrics(&metrics);
    assert_eq!(
        masked.total_kwh(),
        metrics.total_kwh(),
        "能耗数值不应被脱敏"
    );
}
