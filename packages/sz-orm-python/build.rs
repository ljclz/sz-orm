//! sz-orm-python 构建脚本。
//!
//! pyo3 `extension-module` 模式下，Linux 上 `cargo test` 构建的是测试可执行文件，
//! 需要显式链接 libpython（否则链接器报未定义符号如 `PyType_IsSubtype`）。
//! 这里从 `python3-config --embed --ldflags` 解析 `-l` / `-L` 指令注入链接器。

fn main() {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("python3-config")
            .arg("--embed")
            .arg("--ldflags")
            .output();
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for part in stdout.split_whitespace() {
                    if let Some(lib) = part.strip_prefix("-l") {
                        println!("cargo:rustc-link-lib={lib}");
                    } else if let Some(dir) = part.strip_prefix("-L") {
                        println!("cargo:rustc-link-search=native={dir}");
                    }
                }
            }
        }
    }
}
