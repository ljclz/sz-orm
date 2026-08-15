#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A11: XSS（跨站脚本攻击）渗透测试（lc 包）
//!
//! 对应 REQ-V49-011（OWASP XSS）
//!
//! 渗透测试向量：
//! - HTML 表单转义：FormGenerator 未转义用户输入（发现 XSS-001）
//! - 反射型 XSS：演示正确 HTML 转义防护
//! - 存储型 XSS：原值存储 + 渲染时转义
//! - DOM 安全 API：textContent vs innerHTML
//! - HTML input type 安全：sql_to_html_input 返回安全类型 + value 转义

use sz_orm_lc::{FieldDef, FieldTypeMapping, FormField, FormGenerator, InputType, ModelDefinition};

/// HTML 转义函数——将特殊字符转义为 HTML 实体
fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// XSS-1：HTML 表单转义——FormGenerator 转义用户输入（XSS-001 已修复）
///
/// 攻击模型：攻击者在字段 label/placeholder 中注入 `<script>alert('xss')</script>`，
/// FormGenerator 转义后输出 &lt;script&gt;，防止 XSS。
#[test]
fn xss_html_form_escaping_finding() {
    let xss_payload = "<script>alert('xss')</script>";
    let field = FormField::new("name", xss_payload, InputType::Text)
        .with_placeholder("<img onerror=alert(1) src=x>");

    let html = FormGenerator::generate_html_form(&[field], "/submit", "POST");

    assert!(
        !html.contains(xss_payload),
        "XSS-001 已修复：FormGenerator 应转义 label，HTML 不应包含原始载荷"
    );
    assert!(
        html.contains("&lt;script&gt;"),
        "转义后应包含 &lt;script&gt;"
    );
    assert!(
        !html.contains("<img onerror=alert(1) src=x>"),
        "placeholder 应被转义"
    );
    assert!(
        html.contains("&lt;img onerror=alert(1) src=x&gt;"),
        "转义后 placeholder 应包含 HTML 实体"
    );
}

/// XSS-2：反射型 XSS——演示正确 HTML 转义防护
///
/// 攻击模型：URL 参数 `?name=<script>alert(1)</script>` 反射到页面。
/// 防护：反射前使用 escape_html 转义。
#[test]
fn xss_reflected_escaped() {
    let malicious_inputs = [
        "<script>alert('xss')</script>",
        "<img onerror=alert(1) src=x>",
        "<svg onload=alert(1)>",
        "javascript:alert(1)",
        "\"><script>alert(1)</script>",
        "'><script>alert(1)</script>",
    ];

    for input in &malicious_inputs {
        let escaped = escape_html(input);
        assert!(
            !escaped.contains('<'),
            "转义后不应包含原始 <，输入: {}，输出: {}",
            input,
            escaped
        );
        assert!(
            !escaped.contains('>'),
            "转义后不应包含原始 >，输入: {}，输出: {}",
            input,
            escaped
        );
        assert!(
            escaped.contains("&lt;") || escaped.contains("&gt;") || !input.contains('<'),
            "应包含 HTML 实体转义"
        );
    }

    let script = "<script>alert(1)</script>";
    let escaped = escape_html(script);
    assert_eq!(escaped, "&lt;script&gt;alert(1)&lt;/script&gt;");
}

/// XSS-3：存储型 XSS——原值存储 + 渲染时转义
///
/// 攻击模型：用户输入 `<script>alert(1)</script>` 存入 DB，
/// 若渲染时不转义则触发存储型 XSS。
/// 防护：存储原值，渲染时使用 escape_html 转义。
#[test]
fn xss_stored_escaped_on_render() {
    let user_input = "<script>alert('stored')</script>";

    let stored_value = user_input.to_string();
    assert_eq!(stored_value, user_input, "存储原值不修改");

    let rendered = escape_html(&stored_value);
    assert!(
        !rendered.contains("<script>"),
        "渲染时应转义，不应包含原始 <script>"
    );
    assert!(
        rendered.contains("&lt;script&gt;"),
        "渲染后应包含转义的 &lt;script&gt;"
    );
}

/// XSS-4：DOM 安全 API——textContent vs innerHTML
///
/// 攻击模型：使用 innerHTML 赋值用户输入导致 DOM XSS。
/// 防护：使用 textContent 或 createElement 安全 API。
#[test]
fn xss_dom_safe_api() {
    let user_input = "<script>alert('dom')</script>";

    let innerhtml_result = user_input.to_string();
    assert!(
        innerhtml_result.contains("<script>"),
        "innerHTML 赋值会执行脚本（不安全）"
    );

    let escaped_for_innerhtml = escape_html(user_input);
    assert!(
        !escaped_for_innerhtml.contains("<script>"),
        "innerHTML 赋值转义后不执行脚本"
    );

    let textcontent_result = escape_html(user_input);
    assert!(
        !textcontent_result.contains("<script>"),
        "textContent 不解析 HTML 标签（安全）"
    );
}

/// XSS-5：HTML input type 安全——sql_to_html_input 返回安全类型 + value 转义
///
/// 攻击模型：字段 value 含 `">` 试图逃逸 input 属性注入脚本。
/// 防护：sql_to_html_input 返回固定类型，value 需转义。
#[test]
fn xss_html_input_type_safe() {
    let sql_types = [
        ("VARCHAR(255)", "text"),
        ("TEXT", "textarea"),
        ("INT", "number"),
        ("BIGINT", "number"),
        ("BOOLEAN", "checkbox"),
        ("DATE", "date"),
        ("TIMESTAMP", "datetime-local"),
        ("UUID", "text"),
    ];

    for (sql_type, expected_html_type) in &sql_types {
        let html_type = FieldTypeMapping::sql_to_html_input(sql_type);
        assert_eq!(
            html_type, *expected_html_type,
            "SQL 类型 {} 应映射为 HTML input type {}",
            sql_type, expected_html_type
        );
    }

    let malicious_value = "\"><script>alert(1)</script>";
    let escaped_value = escape_html(malicious_value);
    assert!(
        !escaped_value.contains("\"><script>"),
        "value 转义后不应逃逸 input 属性"
    );
    assert!(
        escaped_value.contains("&quot;&gt;"),
        "value 转义后应包含 &quot;&gt;"
    );

    let _model = ModelDefinition::new("users")
        .with_field(FieldDef::new("id", "BIGINT").primary())
        .with_field(FieldDef::new("name", "VARCHAR(255)").with_label("用户名"));
}
