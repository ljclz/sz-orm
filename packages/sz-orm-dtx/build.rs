// Protobuf code generation only happens with the `cross-lang-dtx` feature enabled.
// 与 sz-orm-grpc/build.rs 模式一致：vendored protoc + tonic-prost-build。

#[cfg(feature = "cross-lang-dtx")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/cross_lang_tx.proto"], &["proto"])?;
    Ok(())
}

#[cfg(not(feature = "cross-lang-dtx"))]
fn main() {}
