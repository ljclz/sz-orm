//! phantom2 端到端接线测试 — 验证 4 个 CLI 子命令生产调用点可达
//!
//! 运行：cargo test -p sz-orm-cli --test cli_phantom2_wiring_test --features wasm-build,hot-reload,lsp-server

use std::process::Command;

fn cli_bin() -> String {
    env!("CARGO_BIN_EXE_sz-orm").to_string()
}

#[test]
fn test_wasm_build_config_e2e() {
    let output = Command::new(cli_bin())
        .args(["wasm", "build-config", "--target", "web"])
        .output()
        .expect("执行 CLI 失败");
    assert!(output.status.success(), "wasm build-config 应成功");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--target web"),
        "应包含 --target web: {}",
        stdout
    );
    assert!(stdout.contains("pkg-web"), "应包含 pkg-web: {}", stdout);
}

#[test]
fn test_config_hot_reload_e2e() {
    let output = Command::new(cli_bin())
        .args([
            "config",
            "hot-reload",
            "--key",
            "log.level",
            "--value",
            "debug",
        ])
        .output()
        .expect("执行 CLI 失败");
    assert!(output.status.success(), "config hot-reload 应成功");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("applied_count"),
        "应包含 applied_count: {}",
        stdout
    );
}

#[cfg(feature = "lsp-server")]
#[test]
fn test_lsp_e2e() {
    use sz_orm_lsp::server::LspServer;
    let mut server = LspServer::new();
    let request = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}"#;
    let response = server.handle_json_rpc(request);
    assert!(
        response.contains("jsonrpc") || response.contains("capabilities"),
        "LSP 应返回 JSON-RPC 响应: {}",
        response
    );
}

#[test]
fn test_migrate_derive_down_e2e() {
    use std::io::Write;
    let temp_dir = std::env::temp_dir();
    let sql_file = temp_dir.join("phantom2_test_up.sql");
    let mut f = std::fs::File::create(&sql_file).expect("创建临时 SQL 文件失败");
    f.write_all(b"CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
        .unwrap();
    drop(f);

    let output = Command::new(cli_bin())
        .args(["migrate:derive-down", "--up", sql_file.to_str().unwrap()])
        .output()
        .expect("执行 CLI 失败");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DROP TABLE") || stdout.contains("users"),
        "derive-down 应包含 DROP TABLE users: {}",
        stdout
    );

    std::fs::remove_file(&sql_file).ok();
}
