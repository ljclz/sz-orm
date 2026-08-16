use std::process::Command;

#[test]
fn phantom1_wiring_all_symbols_connected() {
    let output = Command::new("cargo")
        .args(["run", "-p", "sz-orm-cli", "--", "phantom1-wiring"])
        .output()
        .expect("failed to execute cargo run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let success_count = stdout.matches("✅").count();
    let failure_count = stdout.matches("❌").count();

    assert_eq!(
        failure_count, 0,
        "存在失败的接线断言\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        success_count, 33,
        "预期 33 个接线成功，实际 {success_count}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        output.status.success(),
        "phantom1-wiring 子命令退出码非 0\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
