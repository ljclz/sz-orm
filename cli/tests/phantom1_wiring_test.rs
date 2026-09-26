use std::process::Command;

#[test]
fn test_phantom1_wiring_cli() {
    let output = Command::new("cargo")
        .args(["run", "-p", "sz-orm-cli", "--", "phantom1-wiring"])
        .output()
        .expect("failed to execute cargo run");
    assert!(
        output.status.success(),
        "phantom1-wiring 应成功: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}
