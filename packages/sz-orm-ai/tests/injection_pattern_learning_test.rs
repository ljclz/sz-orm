//! TASK-019: InjectionPatternStore 模式学习单元测试
//!
//! 验证 LLM 识别新注入模式 → 入库 → 后续相同模式被规则引擎直接识别。

use std::path::PathBuf;

use sz_orm_ai::llm_security_audit::{InjectionPattern, InjectionPatternStore, RiskLevel};

#[test]
fn test_in_memory_store() {
    let store = InjectionPatternStore::in_memory();
    assert!(store.len() >= 4);
    assert!(!store.is_empty());
}

#[test]
fn test_check_builtin_pattern() {
    let mut store = InjectionPatternStore::in_memory();
    let result = store.check("' OR 1=1 --");
    assert!(result.is_some());
    let pattern = result.unwrap();
    assert_eq!(pattern.risk_level, RiskLevel::Critical);
    assert_eq!(pattern.name, "or_1_eq_1");
}

#[test]
fn test_check_no_match() {
    let mut store = InjectionPatternStore::in_memory();
    let result = store.check("SELECT id FROM users WHERE id = 1");
    assert!(result.is_none());
}

#[test]
fn test_add_new_pattern() {
    let mut store = InjectionPatternStore::in_memory();
    let initial_count = store.len();

    let pattern = InjectionPattern {
        name: "custom_injection".to_string(),
        pattern: "' XOR 1=1".to_string(),
        risk_level: RiskLevel::High,
        discovered_at: 1234567890,
        hit_count: 0,
    };
    let added = store.add_pattern(pattern).unwrap();
    assert!(added);
    assert_eq!(store.len(), initial_count + 1);
}

#[test]
fn test_add_duplicate_pattern() {
    let mut store = InjectionPatternStore::in_memory();
    let pattern = InjectionPattern {
        name: "custom".to_string(),
        pattern: "' OR 1=1".to_string(),
        risk_level: RiskLevel::Critical,
        discovered_at: 0,
        hit_count: 0,
    };
    let added = store.add_pattern(pattern).unwrap();
    assert!(!added);
}

#[test]
fn test_new_pattern_then_check() {
    let mut store = InjectionPatternStore::in_memory();

    let pattern = InjectionPattern {
        name: "xor_injection".to_string(),
        pattern: "' XOR 1=1".to_string(),
        risk_level: RiskLevel::High,
        discovered_at: 0,
        hit_count: 0,
    };
    store.add_pattern(pattern).unwrap();

    let result = store.check("' XOR 1=1");
    assert!(result.is_some());
    assert_eq!(result.unwrap().name, "xor_injection");
}

#[test]
fn test_persist_and_load() {
    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("sz_orm_test_patterns.json");

    let _ = std::fs::remove_file(&path);

    {
        let mut store = InjectionPatternStore::new(path.clone()).unwrap();
        let pattern = InjectionPattern {
            name: "persisted_pattern".to_string(),
            pattern: "' XOR 1=1".to_string(),
            risk_level: RiskLevel::High,
            discovered_at: 1234567890,
            hit_count: 0,
        };
        store.add_pattern(pattern).unwrap();
    }

    {
        let store = InjectionPatternStore::new(path.clone()).unwrap();
        assert!(store
            .patterns()
            .iter()
            .any(|p| p.name == "persisted_pattern"));
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_load_patterns_method() {
    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("sz_orm_test_load_patterns.json");

    let _ = std::fs::remove_file(&path);

    {
        let mut store = InjectionPatternStore::new(path.clone()).unwrap();
        let pattern = InjectionPattern {
            name: "loaded_pattern".to_string(),
            pattern: "' XOR 1=1".to_string(),
            risk_level: RiskLevel::High,
            discovered_at: 0,
            hit_count: 0,
        };
        store.add_pattern(pattern).unwrap();
    }

    {
        let mut store = InjectionPatternStore::in_memory().with_store_path(path.clone());
        let count = store.load_patterns().unwrap();
        assert!(count > 0);
        assert!(store.patterns().iter().any(|p| p.name == "loaded_pattern"));
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_save_patterns() {
    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("sz_orm_test_save_patterns.json");
    let _ = std::fs::remove_file(&path);

    let mut store = InjectionPatternStore::in_memory().with_store_path(path.clone());
    let pattern = InjectionPattern {
        name: "saved_pattern".to_string(),
        pattern: "' XOR 1=1".to_string(),
        risk_level: RiskLevel::High,
        discovered_at: 0,
        hit_count: 0,
    };
    store.add_pattern(pattern).unwrap();
    store.save_patterns().unwrap();
    assert!(path.exists());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_load_patterns_no_file() {
    let path = PathBuf::from("/nonexistent/path/patterns.json");
    let mut store = InjectionPatternStore::in_memory().with_store_path(path);
    let result = store.load_patterns();
    assert!(result.is_ok());
    assert!(result.unwrap() >= 4);
}

#[test]
fn test_store_path() {
    let path = PathBuf::from("/tmp/test.json");
    let store = InjectionPatternStore::in_memory().with_store_path(path.clone());
    assert_eq!(store.store_path(), Some(&path));
}

#[test]
fn test_hit_count_increments() {
    let mut store = InjectionPatternStore::in_memory();
    let initial_count = store.check("' OR 1=1").map(|p| p.hit_count).unwrap_or(0);
    let after_count = store.check("' OR 1=1").map(|p| p.hit_count).unwrap_or(0);
    assert!(after_count > initial_count);
}

#[test]
fn test_patterns_view() {
    let store = InjectionPatternStore::in_memory();
    let patterns = store.patterns();
    assert!(!patterns.is_empty());
    assert!(patterns.iter().any(|p| p.name == "or_1_eq_1"));
}
