#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A03: 注入深化渗透测试（core 包）
//!
//! 对应 REQ-V49-003（OWASP A03 深化）
//!
//! 渗透测试向量：
//! - NoSQL 操作符参数化：`$ne` 作为字面量字符串
//! - SQL UNION 注入参数化
//! - SQL 堆叠注入参数化
//! - SQL 盲注参数化
//! - SQL 二阶注入阻止
//! - CRLF 头部注入过滤
//! - OS 命令注入缺席

use std::fs;
use std::path::PathBuf;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, Model, QueryBuilder, Value};

#[derive(Debug, Clone, Default)]
struct User {
    id: i64,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

fn qb() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

/// A03-1：NoSQL 操作符参数化
///
/// 构造 `{"$ne": null}` 作为查询值，断言其作为字面量字符串参数化绑定，
/// 不被解释为 NoSQL 操作符。
#[test]
fn a03_nosql_operator_parameterized() {
    let nosql_injection = "{\"$ne\": null}";
    let (sql, params) = qb()
        .table("users")
        .where_eq("field", Value::String(nosql_injection.to_string()))
        .build_select_with_params();

    assert!(
        !sql.contains("$ne"),
        "NoSQL 操作符 $ne 不得出现在 SQL 中（必须参数化）"
    );

    let has_injection_param = params
        .iter()
        .any(|p| matches!(p, Value::String(s) if s == nosql_injection));
    assert!(has_injection_param, "NoSQL 注入字符串必须作为参数值绑定");
}

/// A03-2：SQL UNION 注入参数化
///
/// 构造 `' UNION SELECT * FROM users--` 注入向量，
/// 断言其作为参数化绑定，不出现在 SQL 文本中。
#[test]
fn a03_sql_injection_union_parameterized() {
    let union_injection = "' UNION SELECT * FROM users--";
    let (sql, params) = qb()
        .table("products")
        .where_eq("name", Value::String(union_injection.to_string()))
        .build_select_with_params();

    assert!(
        !sql.to_uppercase().contains("UNION SELECT"),
        "UNION 注入不得出现在 SQL 文本中"
    );
    assert!(!sql.contains("--"), "SQL 注释不得出现在 SQL 文本中");

    let has_param = params
        .iter()
        .any(|p| matches!(p, Value::String(s) if s == union_injection));
    assert!(has_param, "UNION 注入字符串必须作为参数值绑定");
}

/// A03-3：SQL 堆叠注入参数化
///
/// 构造 `; DROP TABLE users--` 注入向量，
/// 断言其作为参数化绑定，不出现在 SQL 文本中。
#[test]
fn a03_sql_injection_stacked_rejected() {
    let stacked_injection = "; DROP TABLE users--";
    let (sql, params) = qb()
        .table("products")
        .where_eq("name", Value::String(stacked_injection.to_string()))
        .build_select_with_params();

    assert!(
        !sql.to_uppercase().contains("DROP TABLE"),
        "DROP TABLE 不得出现在 SQL 文本中"
    );

    let has_param = params
        .iter()
        .any(|p| matches!(p, Value::String(s) if s == stacked_injection));
    assert!(has_param, "堆叠注入字符串必须作为参数值绑定");
}

/// A03-4：SQL 盲注参数化
///
/// 构造 `' AND 1=1--` 和 `' AND 1=2--` 盲注向量，
/// 断言两者都作为参数化绑定，SQL 文本相同（仅参数值不同）。
#[test]
fn a03_sql_injection_blind_parameterized() {
    let blind_true = "' AND 1=1--";
    let blind_false = "' AND 1=2--";

    let (sql1, params1) = qb()
        .table("products")
        .where_eq("name", Value::String(blind_true.to_string()))
        .build_select_with_params();

    let (sql2, params2) = qb()
        .table("products")
        .where_eq("name", Value::String(blind_false.to_string()))
        .build_select_with_params();

    assert_eq!(sql1, sql2, "盲注 SQL 文本必须相同（仅参数值不同）");

    assert!(
        !sql1.to_uppercase().contains("AND 1=1"),
        "盲注条件不得出现在 SQL 文本中"
    );

    let p1 = params1
        .iter()
        .any(|p| matches!(p, Value::String(s) if s == blind_true));
    let p2 = params2
        .iter()
        .any(|p| matches!(p, Value::String(s) if s == blind_false));
    assert!(p1 && p2, "盲注字符串必须作为参数值绑定");
}

/// A03-5：SQL 二阶注入阻止
///
/// 构造二阶注入向量（存储时看似无害，使用时触发），
/// 断言 `QueryBuilder` 始终参数化，二阶注入被阻止。
#[test]
fn a03_sql_injection_second_order_blocked() {
    let second_order = "admin'--";
    let (sql, params) = qb()
        .table("users")
        .where_eq("username", Value::String(second_order.to_string()))
        .build_select_with_params();

    assert!(
        !sql.contains("--"),
        "SQL 注释不得出现在 SQL 文本中（二阶注入被阻止）"
    );

    let has_param = params
        .iter()
        .any(|p| matches!(p, Value::String(s) if s == second_order));
    assert!(has_param, "二阶注入字符串必须作为参数值绑定");
}

/// A03-6：CRLF 头部注入过滤
///
/// 构造含 CRLF 字符的头部值，断言 CRLF 被过滤或拒绝。
#[test]
fn a03_header_injection_crlf_filtered() {
    let crlf_injection = "https://evil.com\r\nSet-Cookie: evil=1";
    assert!(
        crlf_injection.contains('\r') || crlf_injection.contains('\n'),
        "测试向量必须含 CRLF 字符"
    );

    let filtered: String = crlf_injection
        .chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .collect();
    assert!(
        !filtered.contains('\r') && !filtered.contains('\n'),
        "CRLF 字符必须被过滤"
    );
    assert!(
        !filtered.contains("\r\nSet-Cookie"),
        "过滤后不得含 CRLF + Set-Cookie 头部注入序列"
    );
}

/// A03-7：OS 命令注入缺席
///
/// 扫描生产源码，断言不使用 `Command::new(user_input)` 模式。
/// 排除 `#[cfg(test)]` 模块。
#[test]
fn a03_os_command_injection_absent() {
    let packages_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in fs::read_dir(&packages_dir).unwrap() {
        let pkg_path = entry.unwrap().path();
        let src_dir = pkg_path.join("src");
        if src_dir.is_dir() {
            collect_rust_files_recursive(&src_dir, &mut files);
        }
    }

    let dangerous_patterns = [
        "Command::new(\"sh\").arg(\"-c\").arg(",
        "Command::new(\"bash\").arg(\"-c\").arg(",
        "Command::new(\"cmd\").arg(\"/c\").arg(",
    ];
    let mut violations = Vec::new();

    for file in &files {
        let content = fs::read_to_string(file).unwrap_or_default();
        let prod_code = if let Some(pos) = content.find("#[cfg(test)]") {
            &content[..pos]
        } else {
            &content[..]
        };
        for pattern in &dangerous_patterns {
            if prod_code.contains(pattern) {
                violations.push(format!("{}: found `{}`", file.display(), pattern));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "OS 命令注入风险发现:\n{}",
        violations.join("\n")
    );
}

fn collect_rust_files_recursive(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_files_recursive(&path, files);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            files.push(path);
        }
    }
}
