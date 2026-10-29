//! cbindgen 构建脚本：为 sz-orm-cabi 生成 C 头文件。
//!
//! - 生成到 `OUT_DIR/sz_orm_cabi.h`（构建产物）。
//! - 同时生成到 `packages/sz-orm-cabi/include/sz_orm_cabi.h`（仓库内），
//!   供 Go/C++/Java/C# 等语言直接引用，作为 ABI 契约的单一声明源。
//!
//! 离线回退：cbindgen 生成期间会执行 `cargo metadata --all-features`，依赖未
//! 缓存时会访问网络。生成失败且仓库内头文件已存在时，本脚本回退为直接使用
//! 仓库内头文件并给出 `cargo:warning`，保证 `cargo build/test -p sz-orm-cabi`
//! 可离线完成；需要刷新头文件时联网重建即可。设置
//! `SZ_ORM_CABI_SKIP_CBINDGEN=1` 可强制跳过生成。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// ABI 契约横幅：写入每个头文件，避免调用方在编码/后端/所有权上误用。
const ABI_BANNER: &str = r#"/* ============================================================================
 * sz-orm C ABI 契约（sz-orm-cabi）
 *
 * 字符串编码：所有 `const char *` 入参与返回的 C 字符串均为 UTF-8 且以 NUL
 *   结尾。调用方不得按系统 ANSI 代码页解析（C# 用 LPUTF8Str /
 *   PtrToStringUTF8，Java 用 UTF-8 解码），否则非 ASCII 数据会乱码。
 * 后端范围：当前仅支持 SQLite DSN（`sqlite::memory:`、`sqlite://...`）；
 *   `sz_orm_pool_new` 对其它 DSN 返回 nullptr。
 * 内存所有权：返回的 `char *` 由 Rust 分配，必须用对应的 `*_free` 函数释放。
 * ========================================================================== */
"#;

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-env-changed=SZ_ORM_CABI_SKIP_CBINDGEN");

    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let include_path = crate_dir.join("include").join("sz_orm_cabi.h");
    let out_path = out_dir.join("sz_orm_cabi.h");

    let skip = matches!(env::var("SZ_ORM_CABI_SKIP_CBINDGEN").as_deref(), Ok("1"));

    if !skip {
        match cbindgen::generate(&crate_dir) {
            Ok(bindings) => {
                // 仓库内头文件是 ABI 契约交付物（供 Go/C++/Java/C# 引用）。
                // 注意：write_to_file 在文件内容未变化时返回 false（noop 优化），
                // 因此返回值不能作为失败判断。
                let include_dir = crate_dir.join("include");
                fs::create_dir_all(&include_dir).expect("create include dir");
                let _ = bindings.write_to_file(&include_path);
                let _ = bindings.write_to_file(&out_path);
                ensure_banner(&include_path);
                ensure_banner(&out_path);
                return;
            }
            Err(err) => println!(
                "cargo:warning=cbindgen 生成失败（{err}），回退到仓库内头文件 {}",
                include_path.display()
            ),
        }
    }

    if !include_path.is_file() {
        panic!(
            "sz-orm-cabi: 无可用头文件——cbindgen 未生成且 {} 不存在",
            include_path.display()
        );
    }
    fs::copy(&include_path, &out_path).expect("copy include header to OUT_DIR");
    ensure_banner(&out_path);
}

fn ensure_banner(path: &Path) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    if content.starts_with(ABI_BANNER) {
        return;
    }
    fs::write(path, format!("{ABI_BANNER}{content}")).expect("write header with ABI banner");
}
