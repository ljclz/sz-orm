// Protobuf code generation only happens with the `real` feature enabled.
// Cargo forwards feature flags as `--cfg` when compiling build scripts, and
// the optional build-dependencies are only linked when the feature is on.

#[cfg(feature = "real")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use the vendored protoc so no system installation is required.
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/user.proto"], &["proto"])?;
    Ok(())
}

#[cfg(not(feature = "real"))]
fn main() {}
