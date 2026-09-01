use std::env;
use std::process::ExitCode;

use sz_orm_studio::{ServerConfig, WebGuiServer};

fn main() -> ExitCode {
    let addr = env::args()
        .skip(1)
        .find(|a| a.starts_with("--addr="))
        .and_then(|a| a.strip_prefix("--addr=").map(|s| s.to_string()))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port: u16 = env::args()
        .skip(1)
        .find(|a| a.starts_with("--port="))
        .and_then(|a| a.strip_prefix("--port=").and_then(|s| s.parse().ok()))
        .unwrap_or(8080);

    let config = ServerConfig::new(addr, port);
    println!("sz-orm-studio 启动于 http://{}", config.bind_addr());

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("创建 tokio runtime 失败: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(WebGuiServer::new(config).start()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("服务器错误: {}", e);
            ExitCode::FAILURE
        }
    }
}
