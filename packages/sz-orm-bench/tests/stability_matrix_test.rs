//! v7.5.0 StabilityMatrixRunner 端到端测试

use sz_orm_bench::{StabilityMatrixRunner, WorkloadType};

#[test]
fn test_stability_matrix_runner_default() {
    let runner = StabilityMatrixRunner::new();
    let report = runner.run();
    assert_eq!(report.workload_count, 6);
    assert_eq!(report.fault_type_count, 5);
    assert_eq!(report.entries.len(), 30);
}

#[test]
fn test_stability_matrix_entry_coverage() {
    let runner = StabilityMatrixRunner::new();
    let report = runner.run();
    let workloads: std::collections::HashSet<_> =
        report.entries.iter().map(|e| e.workload.clone()).collect();
    assert!(workloads.contains("single_row_query"));
    assert!(workloads.contains("simd_compare"));
    let faults: std::collections::HashSet<_> = report
        .entries
        .iter()
        .map(|e| e.fault_type.clone())
        .collect();
    assert!(faults.contains("connection_exhaust"));
    assert!(faults.contains("slow_query_storm"));
}

#[test]
fn test_stability_matrix_real_chaos_flag() {
    let runner = StabilityMatrixRunner::new();
    let report = runner.run();
    let real_chaos: Vec<_> = report.entries.iter().filter(|e| e.is_real_chaos).collect();
    assert!(!real_chaos.is_empty());
    for entry in &real_chaos {
        assert!(entry.fault_type == "connection_exhaust" || entry.fault_type == "slow_query_storm");
    }
}

#[test]
fn test_stability_matrix_json_output() {
    let runner = StabilityMatrixRunner::new();
    let report = runner.run();
    let json = runner.to_json(&report).unwrap();
    assert!(json.contains("entries"));
    assert!(json.contains("overall_availability"));
}

#[test]
fn test_stability_matrix_custom_workloads() {
    let runner = StabilityMatrixRunner::new()
        .with_workloads(vec![WorkloadType::SingleRowQuery, WorkloadType::BatchQuery]);
    let report = runner.run();
    assert_eq!(report.workload_count, 2);
    assert_eq!(report.entries.len(), 10);
}
