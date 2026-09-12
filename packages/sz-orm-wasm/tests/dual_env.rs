//! # WASM 双环境适配集成测试（v6.9.0 REQ-BND-WASM §8.2）
//!
//! 验证浏览器/Node.js 双环境构建配置、沙箱隔离。

use sz_orm_wasm::{
    detect_environment, verify_sandbox_isolation, DualEnvConfig, RuntimeEnv, SandboxConfig,
    SandboxedFs, WasmBuildTarget,
};

#[test]
fn test_browser_build_config() {
    let config = DualEnvConfig::new();
    let cmd = config.build_command(WasmBuildTarget::Web);
    assert!(
        cmd.contains("--target web"),
        "browser build must use --target web"
    );
    assert!(
        cmd.contains("pkg-web"),
        "browser output dir must be pkg-web"
    );
}

#[test]
fn test_nodejs_build_config() {
    let config = DualEnvConfig::new();
    let cmd = config.build_command(WasmBuildTarget::Nodejs);
    assert!(
        cmd.contains("--target nodejs"),
        "nodejs build must use --target nodejs"
    );
    assert!(
        cmd.contains("pkg-nodejs"),
        "nodejs output dir must be pkg-nodejs"
    );
}

#[test]
fn test_browser_loading_simulation() {
    let db = sz_orm_wasm::WasmDatabase::new();
    let create_sql = "CREATE TABLE users (id INTEGER, name TEXT)";
    db.execute(sz_orm_wasm::WasmQuery::new(create_sql))
        .expect("CREATE TABLE should succeed");

    let insert_sql = "INSERT INTO users (id, name) VALUES (?, ?)";
    let params = vec![serde_json::json!(1), serde_json::json!("Alice")];
    db.execute(sz_orm_wasm::WasmQuery::with_params(insert_sql, params))
        .expect("INSERT should succeed");

    let select_sql = "SELECT * FROM users";
    let rows = db
        .query(sz_orm_wasm::WasmQuery::new(select_sql))
        .expect("SELECT should succeed");
    assert_eq!(rows.len(), 1);
    let row = rows[0].as_object().unwrap();
    assert_eq!(row.get("name").unwrap(), &serde_json::json!("Alice"));
}

#[test]
fn test_nodejs_loading_simulation() {
    let db = sz_orm_wasm::WasmDatabase::new();
    db.execute(sz_orm_wasm::WasmQuery::new(
        "CREATE TABLE products (id INTEGER, name TEXT, price REAL)",
    ))
    .expect("CREATE TABLE should succeed");

    let params = vec![
        serde_json::json!(1),
        serde_json::json!("Widget"),
        serde_json::json!(9.99),
    ];
    db.execute(sz_orm_wasm::WasmQuery::with_params(
        "INSERT INTO products (id, name, price) VALUES (?, ?, ?)",
        params,
    ))
    .expect("INSERT should succeed");

    let rows = db
        .query(sz_orm_wasm::WasmQuery::new("SELECT * FROM products"))
        .expect("SELECT should succeed");
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_sandbox_isolation_blocks_passwd() {
    let sandbox = SandboxedFs::new(SandboxConfig::deny_all());
    let result = sandbox.check_read("/etc/passwd");
    assert!(result.is_err(), "/etc/passwd must be blocked by sandbox");
}

#[test]
fn test_sandbox_isolation_blocks_shadow() {
    let sandbox = SandboxedFs::new(SandboxConfig::deny_all());
    let result = sandbox.check_read("/etc/shadow");
    assert!(result.is_err(), "/etc/shadow must be blocked by sandbox");
}

#[test]
fn test_sandbox_isolation_blocks_ssh_keys() {
    let sandbox = SandboxedFs::new(SandboxConfig::deny_all());
    let result = sandbox.check_read("~/.ssh/id_rsa");
    assert!(result.is_err(), "SSH keys must be blocked by sandbox");
}

#[test]
fn test_sandbox_isolation_comprehensive() {
    let sandbox = SandboxedFs::new(SandboxConfig::deny_all());
    let results = verify_sandbox_isolation(&sandbox);
    assert!(
        results.iter().all(|r| r.blocked),
        "all sensitive paths must be blocked"
    );
    assert!(results.iter().any(|r| r.path == "/etc/passwd"));
    assert!(results.iter().any(|r| r.path == "/etc/shadow"));
    assert!(results.iter().any(|r| r.path == "~/.ssh/id_rsa"));
}

#[test]
fn test_sandbox_allows_permitted_path() {
    let sandbox = SandboxedFs::new(SandboxConfig::allow_rw("/tmp/data"));
    assert!(sandbox.check_read("/tmp/data/file.txt").is_ok());
    assert!(sandbox.check_read("/etc/passwd").is_err());
}

#[test]
fn test_runtime_env_detection() {
    let env = detect_environment();
    assert!(
        matches!(
            env,
            RuntimeEnv::Unknown | RuntimeEnv::Browser | RuntimeEnv::Nodejs
        ),
        "detect_environment must return a valid RuntimeEnv"
    );
}

#[test]
fn test_dual_env_config_both_targets() {
    let config = DualEnvConfig::new();
    let web_cmd = config.build_command(WasmBuildTarget::Web);
    let nodejs_cmd = config.build_command(WasmBuildTarget::Nodejs);
    assert_ne!(web_cmd, nodejs_cmd);
    assert!(web_cmd.contains("web"));
    assert!(nodejs_cmd.contains("nodejs"));
}
