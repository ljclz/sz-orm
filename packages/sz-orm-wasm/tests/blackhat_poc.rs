//! 黑帽审计 PoC（攻击者视角）——2026-08-14
//!
//! 对应白帽报告：H-2（SQL 白名单粒度）、M-16（配置语义反转）、
//! M-17（错误信息回显）。
//! 运行：cargo test -p sz-orm-wasm --features wasm-real-db --test blackhat_poc

use std::sync::Arc;
use sz_orm_core::{Connection, ConnectionFactory, DbError};
use sz_orm_wasm::real_db::protocol::ProxyRequest;
use sz_orm_wasm::real_db::sql_whitelist::WasmDbSqlWhitelist;
use sz_orm_wasm::real_db::{
    AuthConfig, ProxyServerConfig, RateLimitConfig, WasmProxyServer, WasmRealDbError,
};
use sz_orm_wasm::WasmQuery;

/// 测试工厂：永远连接失败（用于配置层语义验证，请求应在前置检查被拦截）
struct FailingFactory;

#[async_trait::async_trait]
impl ConnectionFactory for FailingFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Err(DbError::Internal("no backend".to_string()))
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（H-2 修复验证）：多语句注入被切断
//
// 修复前（黑帽实证）：`SELECT 1; DELETE FROM users; UPDATE ...` 通过白名单。
// 修复后：语句分隔符后存在非空白内容即拒绝；尾部单分号与字符串内分号不误伤。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_sql_whitelist_multistatement_blocked() {
    let wl = WasmDbSqlWhitelist::new();

    // 攻击载荷（修复前通过）：分号后尾随 DML 必须被拒
    assert!(
        !wl.validate("SELECT 1; DELETE FROM users"),
        "多语句注入必须被拒绝"
    );
    assert!(
        !wl.validate("SELECT 1; UPDATE accounts SET balance=0"),
        "多语句注入必须被拒绝"
    );

    // 不误伤：尾部单分号
    assert!(
        wl.validate("SELECT 1;"),
        "尾部单分号应放行（合法 SQL 风格）"
    );
    // 不误伤：字符串字面量内的分号（引号状态机）
    assert!(
        wl.validate("SELECT * FROM users WHERE name = 'a;b'"),
        "字符串内分号不应被误判为多语句"
    );
    // 不误伤：反斜杠转义 + 双写单引号
    assert!(
        wl.validate("SELECT * FROM t WHERE s = 'it\\'s; ok'"),
        "转义引号内分号不应误判"
    );
    println!("[regress-H-2] ✅ 多语句注入被切断，合法分号用法不误伤");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（H-2 修复验证）：MySQL 文件读写原语被禁止
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_sql_whitelist_mysql_file_surface_blocked() {
    let wl = WasmDbSqlWhitelist::new();

    // 修复前（黑帽实证）：INTO OUTFILE / LOAD_FILE 均通过白名单
    assert!(
        !wl.validate("SELECT * FROM users INTO OUTFILE '/tmp/steal.csv'"),
        "INTO OUTFILE 必须被拒绝"
    );
    assert!(
        !wl.validate("SELECT LOAD_FILE('/etc/passwd')"),
        "LOAD_FILE 必须被拒绝"
    );
    assert!(
        !wl.validate("SELECT * FROM t INTO DUMPFILE '/tmp/x'"),
        "INTO DUMPFILE 必须被拒绝"
    );
    println!("[regress-H-2] ✅ MySQL 文件读写原语被白名单拦截");
}

// ═══════════════════════════════════════════════════════════════════════════
// 设计决策记录：DML（INSERT/UPDATE/DELETE）默认为白名单允许项
//
// H-2 白帽发现的"DML 全开无 WHERE 约束"属产品语义（代理允许 WASM 端写操作），
// 修复选择：保留 DML 但阻断文件读写原语与多语句注入。若部署要求只读，
// 需显式配置 WhitelistConfig 或使用数据库最小权限账号。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn note_dml_still_allowed_by_design() {
    let wl = WasmDbSqlWhitelist::new();
    assert!(wl.validate("DELETE FROM users"), "DML 仍允许（产品语义）");
    assert!(
        wl.validate("UPDATE accounts SET balance = 0"),
        "DML 仍允许（产品语义）"
    );
    println!("[note] DML 全开为设计语义，残余风险见白帽报告 H-2（需最小权限 DB 账号）");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（H-2 补充）：注释内嵌多语句被多语句检测拦截
//
// 注释拆分重组关键字（DR/**/OP）为已知残余风险（无分号时无法被本层检测），
// 需解析器级防护；但注释内携带分号的多语句载荷已被分号检测覆盖。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_sql_whitelist_comment_multistatement_blocked() {
    let wl = WasmDbSqlWhitelist::new();
    // 注释内嵌多语句（含分号）：必须被分号检测拦截
    assert!(
        !wl.validate("SELECT 1 /*!50000;DROP TABLE users*/"),
        "注释内嵌多语句必须被拒绝"
    );
    // 直接 DROP 仍被关键字检测拦截
    assert!(!wl.validate("DROP TABLE users"), "DROP 必须被拒绝");
    println!("[regress-H-2] ✅ 注释内嵌多语句被拦截；DR/**/OP 无分号重组为已知残余风险");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（M-16 修复验证）：AuthConfig::disabled() 语义=放行
//
// 修复前（黑帽实证）：disabled() 实际拒绝一切请求（文档与实现相反，
// 部署者按文档"禁用鉴权"会遭遇全拒或误以为安全）。修复后：enabled=false
// 跳过 token 校验；enabled=true 无 token 时仍拒绝（fail-closed）。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_auth_disabled_allows_requests() {
    // disabled() → 不校验 token，请求直达后续检查（factory 失败证明已通过鉴权）
    let config = ProxyServerConfig::new().with_auth(AuthConfig::disabled());
    let mut server = WasmProxyServer::new(Arc::new(FailingFactory), config);

    let request = ProxyRequest {
        session_id: "sess-1".to_string(),
        token: "any-token".to_string(),
        query: WasmQuery::new("SELECT 1"),
        transaction_id: None,
    };

    let result = runtime().block_on(server.handle_request(request.clone()));
    assert!(
        !matches!(result, Err(WasmRealDbError::AuthFailed)),
        "disabled() 不应拒绝请求（M-16 修复失效）"
    );
    // 通过鉴权后应到达 DB 层（factory 失败）
    assert!(matches!(result, Err(WasmRealDbError::QueryFailed { .. })));
    println!("[regress-M-16] ✅ disabled() 语义修正：跳过鉴权直达查询层");

    // 反向验证：enabled=true 且无有效 token 仍拒绝（fail-closed 保留）
    let config2 = ProxyServerConfig::new().with_auth(AuthConfig::new());
    let mut server2 = WasmProxyServer::new(Arc::new(FailingFactory), config2);
    let result2 = runtime().block_on(server2.handle_request(request.clone()));
    assert!(
        matches!(result2, Err(WasmRealDbError::AuthFailed)),
        "启用鉴权时无有效 token 必须拒绝"
    );
    println!("[regress-M-16] ✅ enabled=true 无 token 仍 fail-closed");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（M-16 修复验证）：RateLimitConfig.enabled=false 真实关闭限流
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_rate_limit_enabled_flag_effective() {
    let config = ProxyServerConfig::new()
        .with_auth(AuthConfig::new().with_token("valid-token"))
        .with_rate_limit(RateLimitConfig {
            enabled: false,
            max_qps: 2,
            burst_size: 2,
        });
    let mut server = WasmProxyServer::new(Arc::new(FailingFactory), config);

    let request = ProxyRequest {
        session_id: "sess-1".to_string(),
        token: "valid-token".to_string(),
        query: WasmQuery::new("SELECT 1"),
        transaction_id: None,
    };

    // enabled=false：3 次请求都不应被限流（都应到达 factory → QueryFailed）
    for i in 0..3 {
        let result = runtime().block_on(server.handle_request(request.clone()));
        assert!(
            !matches!(result, Err(WasmRealDbError::RateLimited)),
            "第 {i} 次请求不得被限流（enabled=false 失效）"
        );
        assert!(matches!(result, Err(WasmRealDbError::QueryFailed { .. })));
    }
    println!("[regress-M-16] ✅ enabled=false 真实关闭限流");

    // 对照：enabled=true 时限流生效（第 3 次被 RateLimited）
    let config2 = ProxyServerConfig::new()
        .with_auth(AuthConfig::new().with_token("valid-token"))
        .with_rate_limit(RateLimitConfig {
            enabled: true,
            max_qps: 2,
            burst_size: 2,
        });
    let mut server2 = WasmProxyServer::new(Arc::new(FailingFactory), config2);
    let mut outcomes = vec![];
    for _ in 0..3 {
        outcomes.push(runtime().block_on(server2.handle_request(request.clone())));
    }
    assert!(
        matches!(outcomes.last(), Some(Err(WasmRealDbError::RateLimited))),
        "enabled=true 时限流必须生效"
    );
    println!("[regress-M-16] ✅ enabled=true 限流正常");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（M-17 修复验证）：底层错误详情不再回显
//
// 修复前（黑帽实证）：QueryFailed/SqlRejected 错误携带底层 DB 错误原文
// 与完整 SQL（信息泄露）。修复后：统一为静态脱敏消息。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_db_error_detail_not_echoed() {
    let config = ProxyServerConfig::new()
        .with_auth(AuthConfig::new().with_token("valid-token"))
        .with_whitelist(Default::default());
    let mut server = WasmProxyServer::new(Arc::new(FailingFactory), config);

    // SQL 被拒时：不得回显完整 SQL
    let rejected = ProxyRequest {
        session_id: "sess-1".to_string(),
        token: "valid-token".to_string(),
        query: WasmQuery::new("DROP TABLE internal_secret_table"),
        transaction_id: None,
    };
    let r1 = runtime().block_on(server.handle_request(rejected));
    if let Err(WasmRealDbError::SqlRejected { reason }) = r1 {
        assert!(
            !reason.contains("internal_secret_table"),
            "拒绝原因不得包含 SQL 内容"
        );
        println!("[regress-M-17] ✅ SQL 拒绝消息已脱敏: {reason}");
    } else {
        panic!("预期 SqlRejected");
    }

    // 查询失败时：不得回显底层错误原文
    let query = ProxyRequest {
        session_id: "sess-1".to_string(),
        token: "valid-token".to_string(),
        query: WasmQuery::new("SELECT * FROM internal_secret_table"),
        transaction_id: None,
    };
    let r2 = runtime().block_on(server.handle_request(query));
    if let Err(WasmRealDbError::QueryFailed { reason }) = r2 {
        assert!(
            !reason.contains("no backend") && !reason.contains("Internal"),
            "查询失败消息不得携带底层错误详情: {reason}"
        );
        println!("[regress-M-17] ✅ 查询失败消息已脱敏: {reason}");
    } else {
        panic!("预期 QueryFailed");
    }
}
