//! sz-orm-python 构建脚本。
//!
//! Linux 上 `cargo test` 构建的是测试可执行文件，需要显式链接 libpython（否则链接器
//! 报未定义符号如 `PyType_IsSubtype`）；而 `extension-module` 构建的 cdylib 恰恰不能
//! 链接 libpython（maturin 的 manylinux 审计会直接失败），两者必须分开处理。

fn main() {
    // 扩展模块构建：不注入任何 libpython 链接指令
    if std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some() {
        return;
    }
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
