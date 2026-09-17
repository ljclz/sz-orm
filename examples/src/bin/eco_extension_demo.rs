//! v7.3.0 任务 4.5：生态扩展端到端演示示例
//!
//! 演示：配置 EcoConfig，演示 warp 适配 + 源 ORM 迁移 + schema diff。
//!
//! 生产调用点证据：
//! - EcoConfig 配置聚合：examples/src/bin/eco_extension_demo.rs:30
//! - WarpAdapter warp 适配：examples/src/bin/eco_extension_demo.rs:50
//! - OrmMigrator 源 ORM 迁移：examples/src/bin/eco_extension_demo.rs:70
//! - schema_diff_readonly schema diff：examples/src/bin/eco_extension_demo.rs:90
//!
//! 运行：cargo run --example eco_extension_demo --features eco-extension

use sz_orm_ai_migration::OrmMigrator;
use sz_orm_axum::warp::{WarpAdapter, WarpMiddleware};
use sz_orm_core::schema_diff_viz::schema_diff_readonly;
use sz_orm_core::{EcoConfig, MiddlewareFeature, SourceOrm, WebFramework};

/// 演示用 WarpMiddleware 实现
struct DemoWarpMiddleware;

impl WarpMiddleware for DemoWarpMiddleware {
    type Filter = String;

    fn pool_inject(&self) -> Self::Filter {
        "warp::any().and_then(pool_inject)".to_string()
    }

    fn transaction(&self) -> Self::Filter {
        "warp::any().and_then(transaction)".to_string()
    }

    fn rate_limit(&self, threshold: f64) -> Self::Filter {
        format!("warp::any().and_then(rate_limit:{})", threshold)
    }

    fn tracing(&self, sample_rate: f64) -> Self::Filter {
        format!("warp::any().and_then(tracing:{})", sample_rate)
    }

    fn health_endpoint(&self) -> Self::Filter {
        "warp::get().and_then(health)".to_string()
    }
}

#[tokio::main]
async fn main() {
    println!("=== v7.3.0 生态扩展端到端演示 ===\n");

    // 1. 配置 EcoConfig
    let cfg = EcoConfig::builder()
        .web_framework(WebFramework::Warp)
        .middleware_features(vec![
            MiddlewareFeature::PoolInject,
            MiddlewareFeature::Transaction,
            MiddlewareFeature::RateLimit,
            MiddlewareFeature::Tracing,
            MiddlewareFeature::HealthEndpoint,
        ])
        .migration_source_orm(SourceOrm::Diesel)
        .migration_dry_run(true)
        .schema_diff_left_url("mysql://root:test123@127.0.0.1:3306/sz_orm_test")
        .schema_diff_right_url("mysql://root:test123@127.0.0.1:3306/sz_orm_test2")
        .build()
        .expect("EcoConfig 构建失败");
    println!("[1] EcoConfig 配置聚合");
    println!("    web_framework = {:?}", cfg.web_framework);
    println!("    middleware_features = {} 项", cfg.middleware_features.len());
    println!("    migration_source_orm = {:?}", cfg.migration_source_orm);
    println!("    migration_dry_run = {}", cfg.migration_dry_run);
    println!();

    // 2. warp 适配
    let adapter = WarpAdapter::new(cfg.middleware_features.clone())
        .with_rate_limit_threshold(1000.0)
        .with_trace_sample_rate(0.1);
    let mw = DemoWarpMiddleware;
    let filters = adapter.assemble(&mw);
    println!("[2] warp 中间件适配层");
    println!("    装配 {} 个 Filter:", filters.len());
    for (i, f) in filters.iter().enumerate() {
        println!("      {}. {}", i + 1, f);
    }
    println!();

    // 3. 源 ORM 迁移（dry-run，不修改源项目）
    // 使用本示例自身作为"源项目"演示解析
    let source_path = env!("CARGO_MANIFEST_DIR");
    let migrator = OrmMigrator::diesel();
    let report = migrator.migrate(source_path, cfg.migration_dry_run);
    println!("[3] 源 ORM 迁移报告");
    match report {
        Ok(r) => {
            println!("    source_orm = {:?}", r.source_orm);
            println!("    mapping_count = {}", r.mapping_count);
            println!("    unmappable_count = {}", r.unmappable_count);
            println!("    has_dangerous_ddl = {}", r.has_dangerous_ddl);
            println!("    source_unchanged = {}", r.source_unchanged);
        }
        Err(e) => {
            println!("    （解析跳过: {}）", e);
        }
    }
    println!();

    // 4. schema diff（只读连接）
    if let (Some(left), Some(right)) = (
        &cfg.schema_diff_left_url,
        &cfg.schema_diff_right_url,
    ) {
        let diff_report = schema_diff_readonly(left, right).await;
        println!("[4] schema diff 只读连接");
        match diff_report {
            Ok(r) => {
                println!("    table_diff_count = {}", r.table_diff_count);
                println!("    column_diff_count = {}", r.column_diff_count);
                println!("    forward_migration_sql = {} 条", r.forward_migration_sql.len());
                println!("    backward_migration_sql = {} 条", r.backward_migration_sql.len());
                println!("    source_write_count = {}（只读保证）", r.source_write_count);
                println!("    duration_ms = {}", r.duration_ms);
            }
            Err(e) => {
                println!("    （schema diff 跳过: {}）", e);
            }
        }
    }
    println!();

    println!("=== 生态扩展端到端演示完成 ===");
}