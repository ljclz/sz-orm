// Protobuf code generation only happens with the `real` feature enabled.
// Cargo forwards feature flags as `--cfg` when compiling build scripts, and
// the optional build-dependencies are only linked when the feature is on.
//
// 注意：tonic 0.14 起，prost 编译入口从 `tonic_build` 移动到了
// `tonic_prost_build`（旧 `tonic_build::configure()` / `compile_protos()` 已移除）。

#[cfg(feature = "real")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use the vendored protoc so no system installation is required.
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/user.proto"], &["proto"])?;
    Ok(())
}

#[cfg(not(feature = "real"))]
fn main() {}
