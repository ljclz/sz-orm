//! Config Center 适配层端到端测试
use sz_orm_core::config_adapter::{config_count, config_get, config_set};

#[test]
fn test_config_set_and_get() {
    config_set("e2e_key", "e2e_value");
    let val = config_get("e2e_key");
    assert_eq!(val, Some("e2e_value".to_string()));
}

#[test]
fn test_config_count_increments() {
    let before = config_count();
    config_set("e2e_count_test", "1");
    let after = config_count();
    assert!(after > before);
}
