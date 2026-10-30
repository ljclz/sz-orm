//! v9.3.0 T9：pool 安全行为测试
//!
//! 2026-10-27 SDET 整改：原断言只回显刚赋值的配置字段（`max_size == 100`），
//! 现改为验证 `PoolConfigBuilder` 校验拒绝非法配置、`PoolConfig::validate` 的真实错误路径。

use sz_orm_core::{PoolConfig, PoolConfigBuilder};

#[test]
fn test_config_builder_rejects_zero_max_size() {
    let result = PoolConfigBuilder::new().max_size(0).build();
    assert!(result.is_err(), "max_size=0 的池配置必须被校验拒绝");
}

#[test]
fn test_config_builder_rejects_min_idle_above_max_size() {
    let result = PoolConfigBuilder::new().max_size(10).min_idle(20).build();
    assert!(result.is_err(), "min_idle > max_size 的配置必须被校验拒绝");
}

#[test]
fn test_valid_config_passes_validation() {
    let config: PoolConfig = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(2)
        .build()
        .expect("合法配置应通过校验");
    assert!(config.validate().is_ok(), "已构建配置的 validate 应通过");
}
