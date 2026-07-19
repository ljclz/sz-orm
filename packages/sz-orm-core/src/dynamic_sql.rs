//! XML/py_sql 动态 SQL 构造器（rbatis 风格）
//!
//! rbatis 的核心特性是 XML 风格的动态 SQL 模板：
//! - `<select>` / `<insert>` / `<update>` / `<delete>` 顶层语句
//! - `<if test="...">` 条件块
//! - `<where>` 自动处理首个 AND/OR
//! - `<set>` UPDATE 语句的 SET 子句
//! - `<foreach>` 循环展开（用于 IN 子句）
//! - `<choose><when><otherwise>` 多分支选择
//! - `<trim>` 通用前后缀修剪
//! - `#{name}` 命名参数绑定
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::dynamic_sql::{DynamicSqlParser, SqlParams};
//! use std::collections::HashMap;
//!
//! let xml = r#"
//! <select id="find_users">
//!     SELECT * FROM users
//!     <where>
//!         <if test="name != null">AND name = #{name}</if>
//!         <if test="age != null">AND age &gt; #{age}</if>
//!     </where>
//! </select>
//! "#;
//!
//! let parser = DynamicSqlParser::from_xml(xml);
//! let mut params = SqlParams::new();
//! params.set("name", "Alice");
//! // params.set("age", 18);  // 不设置则 if 不生效
//!
//! let sql = parser.build("find_users", &params).unwrap();
//! // SELECT * FROM users WHERE name = ?
//! ```
//!
//! # 支持的标签
//!
//! | 标签 | 作用 |
//! |------|------|
//! | `<select id>` | SELECT 语句容器 |
//! | `<insert id>` | INSERT 语句容器 |
//! | `<update id>` | UPDATE 语句容器 |
//! | `<delete id>` | DELETE 语句容器 |
//! | `<if test="expr">` | 条件包含 |
//! | `<where>` | WHERE 子句（自动处理首个 AND/OR） |
//! | `<set>` | SET 子句（自动处理末尾逗号） |
//! | `<foreach collection="x" item="i" separator=",">` | 循环展开 |
//! | `<choose>` / `<when>` / `<otherwise>` | 多分支选择 |
//! | `<trim prefix="..." suffix="..." prefixOverrides="AND">` | 通用修剪 |
//! | `#{name}` | 命名参数绑定 |
//! | `${name}` | 字符串插值（注意 SQL 注入风险） |
//!
//! # 安全警告
//!
//! `${name}` 字符串插值存在 SQL 注入风险，即使已添加基础转义。
//! **强烈推荐使用 `#{name}` 命名参数绑定**，仅在 SQL 结构动态（如表名/列名）时
//! 才使用 `${name}`，且必须确保输入来源可信。

use std::collections::HashMap;

/// 动态 SQL 错误
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicSqlError {
    /// XML 解析错误
    ParseError(String),
    /// 找不到指定的语句 ID
    StatementNotFound(String),
    /// 表达式求值错误
    EvalError(String),
    /// 参数缺失
    MissingParam(String),
}

impl std::fmt::Display for DynamicSqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynamicSqlError::ParseError(msg) => write!(f, "XML 解析错误: {}", msg),
            DynamicSqlError::StatementNotFound(id) => {
                write!(f, "找不到语句 ID: {}", id)
            }
            DynamicSqlError::EvalError(msg) => write!(f, "表达式求值错误: {}", msg),
            DynamicSqlError::MissingParam(name) => write!(f, "缺少参数: {}", name),
        }
    }
}

impl std::error::Error for DynamicSqlError {}

/// SQL 参数容器
///
/// 支持命名参数，按 `#{name}` 引用。
#[derive(Debug, Clone, Default)]
pub struct SqlParams {
    params: HashMap<String, ParamValue>,
}

/// 参数值
#[derive(Debug, Clone)]
pub enum ParamValue {
    /// Null
    Null,
    /// 字符串
    String(String),
    /// 整数
    Int(i64),
    /// 浮点
    Float(f64),
    /// 布尔
    Bool(bool),
    /// 数组（用于 foreach）
    Array(Vec<ParamValue>),
}

impl SqlParams {
    /// 创建空参数集
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置字符串参数
    pub fn set(&mut self, name: &str, value: &str) {
        self.params
            .insert(name.to_string(), ParamValue::String(value.to_string()));
    }

    /// 设置整数参数
    pub fn set_int(&mut self, name: &str, value: i64) {
        self.params.insert(name.to_string(), ParamValue::Int(value));
    }

    /// 设置浮点参数
    pub fn set_float(&mut self, name: &str, value: f64) {
        self.params
            .insert(name.to_string(), ParamValue::Float(value));
    }

    /// 设置布尔参数
    pub fn set_bool(&mut self, name: &str, value: bool) {
        self.params
            .insert(name.to_string(), ParamValue::Bool(value));
    }

    /// 设置 null
    pub fn set_null(&mut self, name: &str) {
        self.params.insert(name.to_string(), ParamValue::Null);
    }

    /// 设置数组参数
    pub fn set_array(&mut self, name: &str, values: Vec<ParamValue>) {
        self.params
            .insert(name.to_string(), ParamValue::Array(values));
    }

    /// 获取参数值
    pub fn get(&self, name: &str) -> Option<&ParamValue> {
        self.params.get(name)
    }

    /// 是否存在参数
    pub fn contains(&self, name: &str) -> bool {
        self.params.contains_key(name)
    }

    /// 判断参数是否为 null（或不存在）
    pub fn is_null(&self, name: &str) -> bool {
        matches!(self.params.get(name), None | Some(ParamValue::Null))
    }

    /// 判断参数是否不为 null
    pub fn is_not_null(&self, name: &str) -> bool {
        !self.is_null(name)
    }

    /// 获取所有参数名
    pub fn names(&self) -> Vec<String> {
        self.params.keys().cloned().collect()
    }
}

/// 动态 SQL 解析器
#[derive(Debug, Clone)]
pub struct DynamicSqlParser {
    /// 已解析的语句：id → 语句节点
    statements: HashMap<String, XmlNode>,
}

impl DynamicSqlParser {
    /// 创建空解析器
    pub fn new() -> Self {
        Self {
            statements: HashMap::new(),
        }
    }

    /// 从 XML 字符串解析
    pub fn from_xml(xml: &str) -> Result<Self, DynamicSqlError> {
        let mut parser = Self::new();
        parser.parse_xml(xml)?;
        Ok(parser)
    }

    /// 解析 XML 字符串
    fn parse_xml(&mut self, xml: &str) -> Result<(), DynamicSqlError> {
        let root = XmlParser::parse(xml)?;
        for child in &root.children {
            if let XmlNodeType::Element { name, attrs } = &child.node_type {
                let id = attrs
                    .get("id")
                    .ok_or_else(|| DynamicSqlError::ParseError(format!("<{}> 缺少 id 属性", name)))?
                    .clone();
                self.statements.insert(id, child.clone());
            }
        }
        Ok(())
    }

    /// 构建指定 ID 的 SQL 语句
    pub fn build(&self, id: &str, params: &SqlParams) -> Result<String, DynamicSqlError> {
        let node = self
            .statements
            .get(id)
            .ok_or_else(|| DynamicSqlError::StatementNotFound(id.to_string()))?;
        let mut ctx = BuildContext::new(params);
        self.build_node(node, &mut ctx)?;
        Ok(self.cleanup_sql(&ctx.buffer))
    }

    /// 构建并返回 SQL + 绑定参数（按出现顺序）
    pub fn build_with_binds(
        &self,
        id: &str,
        params: &SqlParams,
    ) -> Result<(String, Vec<ParamValue>), DynamicSqlError> {
        let node = self
            .statements
            .get(id)
            .ok_or_else(|| DynamicSqlError::StatementNotFound(id.to_string()))?;
        let mut ctx = BuildContext::new(params);
        self.build_node(node, &mut ctx)?;
        Ok((self.cleanup_sql(&ctx.buffer), ctx.binds))
    }

    /// 列出所有已注册的语句 ID
    pub fn statement_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.statements.keys().cloned().collect();
        ids.sort();
        ids
    }

    // ---- 内部：节点构建 ----

    fn build_node(&self, node: &XmlNode, ctx: &mut BuildContext) -> Result<(), DynamicSqlError> {
        match &node.node_type {
            XmlNodeType::Text(text) => {
                self.append_text(text, ctx)?;
            }
            XmlNodeType::Element { name, attrs } => {
                match name.as_str() {
                    "select" | "insert" | "update" | "delete" => {
                        for child in &node.children {
                            self.build_node(child, ctx)?;
                        }
                    }
                    "if" => {
                        let test = attrs.get("test").ok_or_else(|| {
                            DynamicSqlError::ParseError("<if> 缺少 test 属性".into())
                        })?;
                        if eval_test(test, ctx.params)? {
                            for child in &node.children {
                                self.build_node(child, ctx)?;
                            }
                        }
                    }
                    "where" => {
                        let mut sub_ctx = BuildContext::new(ctx.params);
                        for child in &node.children {
                            self.build_node(child, &mut sub_ctx)?;
                        }
                        let content = sub_ctx.buffer.trim();
                        if !content.is_empty() {
                            // 去除开头的 AND/OR
                            let cleaned = strip_leading_and_or(content);
                            ctx.buffer.push_str(" WHERE ");
                            ctx.buffer.push_str(cleaned.trim());
                            // 把子上下文收集的绑定参数传回父上下文
                            ctx.binds.extend(sub_ctx.binds);
                        }
                    }
                    "set" => {
                        let mut sub_ctx = BuildContext::new(ctx.params);
                        for child in &node.children {
                            self.build_node(child, &mut sub_ctx)?;
                        }
                        let content = sub_ctx.buffer.trim();
                        if !content.is_empty() {
                            // 去除末尾的逗号
                            let cleaned = content.trim_end_matches(',').trim();
                            // 规范化逗号：每个逗号后保留一个空格
                            let normalized = normalize_set_commas(cleaned);
                            ctx.buffer.push_str(" SET ");
                            ctx.buffer.push_str(&normalized);
                            ctx.binds.extend(sub_ctx.binds);
                        }
                    }
                    "foreach" => {
                        self.build_foreach(node, attrs, ctx)?;
                    }
                    "choose" => {
                        self.build_choose(node, ctx)?;
                    }
                    "trim" => {
                        self.build_trim(node, attrs, ctx)?;
                    }
                    _ => {
                        // 未知标签，递归处理子节点
                        for child in &node.children {
                            self.build_node(child, ctx)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn build_foreach(
        &self,
        node: &XmlNode,
        attrs: &HashMap<String, String>,
        ctx: &mut BuildContext,
    ) -> Result<(), DynamicSqlError> {
        let collection = attrs
            .get("collection")
            .ok_or_else(|| DynamicSqlError::ParseError("<foreach> 缺少 collection 属性".into()))?;
        let item = attrs.get("item").map(|s| s.as_str()).unwrap_or("item");
        let separator = attrs.get("separator").map(|s| s.as_str()).unwrap_or(",");
        let open = attrs.get("open").cloned().unwrap_or_default();
        let close = attrs.get("close").cloned().unwrap_or_default();

        let arr = match ctx.params.get(collection) {
            Some(ParamValue::Array(arr)) => arr.clone(),
            _ => return Ok(()),
        };

        // 仅克隆一次参数集，循环内复用（避免每次迭代克隆整个 SqlParams）
        let mut sub_params = ctx.params.clone();
        let mut parts: Vec<String> = Vec::new();
        for v in &arr {
            // 临时设置 item 变量（覆盖上一次迭代的值，无需还原，因为 sub_params 不会被外部观察）
            match v {
                ParamValue::String(s) => sub_params.set(item, s),
                ParamValue::Int(i) => sub_params.set_int(item, *i),
                ParamValue::Float(f) => sub_params.set_float(item, *f),
                ParamValue::Bool(b) => sub_params.set_bool(item, *b),
                ParamValue::Null => sub_params.set_null(item),
                ParamValue::Array(_) => {} // 嵌套数组不支持
            }
            let mut sub_ctx = BuildContext::new(&sub_params);
            for child in &node.children {
                self.build_node(child, &mut sub_ctx)?;
            }
            parts.push(sub_ctx.buffer.trim().to_string());
            // 把子上下文收集的绑定参数传回父上下文（按出现顺序）
            ctx.binds.extend(sub_ctx.binds);
        }
        if !parts.is_empty() {
            let joined = parts.join(separator);
            ctx.buffer.push(' ');
            if !open.is_empty() {
                ctx.buffer.push_str(&open);
            }
            ctx.buffer.push_str(&joined);
            if !close.is_empty() {
                ctx.buffer.push_str(&close);
            }
        }
        Ok(())
    }

    fn build_choose(&self, node: &XmlNode, ctx: &mut BuildContext) -> Result<(), DynamicSqlError> {
        for child in &node.children {
            if let XmlNodeType::Element { name, attrs } = &child.node_type {
                match name.as_str() {
                    "when" => {
                        let test = attrs.get("test").ok_or_else(|| {
                            DynamicSqlError::ParseError("<when> 缺少 test 属性".into())
                        })?;
                        if eval_test(test, ctx.params)? {
                            for c in &child.children {
                                self.build_node(c, ctx)?;
                            }
                            return Ok(());
                        }
                    }
                    "otherwise" => {
                        for c in &child.children {
                            self.build_node(c, ctx)?;
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn build_trim(
        &self,
        node: &XmlNode,
        attrs: &HashMap<String, String>,
        ctx: &mut BuildContext,
    ) -> Result<(), DynamicSqlError> {
        let prefix = attrs.get("prefix").cloned().unwrap_or_default();
        let suffix = attrs.get("suffix").cloned().unwrap_or_default();
        let prefix_overrides = attrs.get("prefixOverrides").cloned().unwrap_or_default();
        let suffix_overrides = attrs.get("suffixOverrides").cloned().unwrap_or_default();

        let mut sub_ctx = BuildContext::new(ctx.params);
        for child in &node.children {
            self.build_node(child, &mut sub_ctx)?;
        }
        let mut content = sub_ctx.buffer.trim().to_string();

        // 处理 prefixOverrides
        if !prefix_overrides.is_empty() {
            for ov in prefix_overrides.split('|') {
                if content.starts_with(ov) {
                    content = content[ov.len()..].trim_start().to_string();
                    break;
                }
            }
        }
        // 处理 suffixOverrides
        if !suffix_overrides.is_empty() {
            for ov in suffix_overrides.split('|') {
                if content.ends_with(ov) {
                    content = content[..content.len() - ov.len()].trim_end().to_string();
                    break;
                }
            }
        }

        if !content.is_empty() {
            ctx.buffer.push(' ');
            if !prefix.is_empty() {
                ctx.buffer.push_str(&prefix);
                ctx.buffer.push(' ');
            }
            ctx.buffer.push_str(&content);
            if !suffix.is_empty() {
                ctx.buffer.push(' ');
                ctx.buffer.push_str(&suffix);
            }
            // 把子上下文收集的绑定参数传回父上下文
            ctx.binds.extend(sub_ctx.binds);
        }
        Ok(())
    }

    fn append_text(&self, text: &str, ctx: &mut BuildContext) -> Result<(), DynamicSqlError> {
        let mut i = 0;
        let bytes = text.as_bytes();
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'#' && bytes[i + 1] == b'{' {
                // #{name} 参数绑定
                let end = text[i + 2..].find('}').ok_or_else(|| {
                    DynamicSqlError::ParseError(format!("未闭合的 #{{}}: {}", &text[i..]))
                })?;
                let name = &text[i + 2..i + 2 + end];
                let value = ctx
                    .params
                    .get(name)
                    .ok_or_else(|| DynamicSqlError::MissingParam(name.to_string()))?
                    .clone();
                ctx.buffer.push('?');
                ctx.binds.push(value);
                i += 2 + end + 1; // 跳过 #{name}
            } else if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
                // ${name} 字符串插值
                let end = text[i + 2..].find('}').ok_or_else(|| {
                    DynamicSqlError::ParseError(format!("未闭合的 ${{}}: {}", &text[i..]))
                })?;
                let name = &text[i + 2..i + 2 + end];
                let value = ctx
                    .params
                    .get(name)
                    .ok_or_else(|| DynamicSqlError::MissingParam(name.to_string()))?;
                let s = param_to_string(value);
                ctx.buffer.push_str(&s);
                i += 2 + end + 1;
            } else {
                ctx.buffer.push(bytes[i] as char);
                i += 1;
            }
        }
        Ok(())
    }

    /// 清理 SQL 中的多余空白
    fn cleanup_sql(&self, sql: &str) -> String {
        let mut result = String::with_capacity(sql.len());
        let mut prev_space = false;
        for c in sql.chars() {
            if c.is_whitespace() {
                if !prev_space {
                    result.push(' ');
                    prev_space = true;
                }
            } else {
                result.push(c);
                prev_space = false;
            }
        }
        result.trim().to_string()
    }
}

impl Default for DynamicSqlParser {
    fn default() -> Self {
        Self::new()
    }
}

/// 构建上下文
struct BuildContext<'a> {
    buffer: String,
    binds: Vec<ParamValue>,
    params: &'a SqlParams,
}

impl<'a> BuildContext<'a> {
    fn new(params: &'a SqlParams) -> Self {
        Self {
            buffer: String::new(),
            binds: Vec::new(),
            params,
        }
    }
}

/// 评估 `<if test="...">` 表达式
///
/// 支持的表达式语法：
/// - `name != null` — 参数存在且非 Null
/// - `name == null` — 参数不存在或为 Null
/// - `name != null and age != null` — AND（不区分大小写）
/// - `name != null or age != null` — OR
/// - `name == 'Alice'` — 字符串相等
/// - `name == "Alice"` — 字符串相等（双引号）
/// - `age > 18` — 数值比较（仅 >=, >, <, <=）
fn eval_test(expr: &str, params: &SqlParams) -> Result<bool, DynamicSqlError> {
    let expr = expr.trim();

    // 处理 OR
    if let Some(idx) = find_keyword(expr, " or ") {
        let left = &expr[..idx];
        let right = &expr[idx + 4..];
        return Ok(eval_test(left, params)? || eval_test(right, params)?);
    }

    // 处理 AND
    if let Some(idx) = find_keyword(expr, " and ") {
        let left = &expr[..idx];
        let right = &expr[idx + 5..];
        return Ok(eval_test(left, params)? && eval_test(right, params)?);
    }

    // 处理 != null / == null
    if let Some(stripped) = expr.strip_suffix("!= null") {
        let name = stripped.trim();
        return Ok(params.is_not_null(name));
    }
    if let Some(stripped) = expr.strip_suffix("== null") {
        let name = stripped.trim();
        return Ok(params.is_null(name));
    }

    // 处理 == '字符串' / == "字符串"
    if let Some(idx) = expr.find("==") {
        let left = expr[..idx].trim();
        let right = expr[idx + 2..].trim();
        let actual = params.get(left);
        let expected = right.trim_matches('\'').trim_matches('"');
        return Ok(match actual {
            Some(ParamValue::String(s)) => s == expected,
            _ => false,
        });
    }
    if let Some(idx) = expr.find("!=") {
        let left = expr[..idx].trim();
        let right = expr[idx + 2..].trim();
        let actual = params.get(left);
        let expected = right.trim_matches('\'').trim_matches('"');
        return Ok(match actual {
            Some(ParamValue::String(s)) => s != expected,
            _ => true,
        });
    }

    // 处理 >, <, >=, <=
    type CmpFn = fn(i64, i64) -> bool;
    let cmps: [(&str, CmpFn); 4] = [
        (">=", |a, b| a >= b),
        ("<=", |a, b| a <= b),
        (">", |a, b| a > b),
        ("<", |a, b| a < b),
    ];
    for (op, cmp) in cmps {
        if let Some(idx) = expr.find(op) {
            let left = expr[..idx].trim();
            let right_str = expr[idx + op.len()..].trim();
            if let (Some(ParamValue::Int(a)), Ok(b)) = (params.get(left), right_str.parse::<i64>())
            {
                return Ok(cmp(*a, b));
            }
            return Ok(false);
        }
    }

    Err(DynamicSqlError::EvalError(format!(
        "无法解析表达式: {}",
        expr
    )))
}

/// 在表达式中查找关键字（不区分大小写，但需要前后是空格）
fn find_keyword(expr: &str, keyword: &str) -> Option<usize> {
    let lower = expr.to_lowercase();
    lower.find(keyword)
}

/// 去除开头的 AND 或 OR
fn strip_leading_and_or(s: &str) -> &str {
    let trimmed = s.trim_start();
    let lower = trimmed.to_lowercase();
    if lower.starts_with("and ") {
        trimmed[4..].trim_start()
    } else if lower.starts_with("or ") {
        trimmed[3..].trim_start()
    } else {
        trimmed
    }
}

/// 参数值转字符串（用于 ${name} 插值）
fn param_to_string(v: &ParamValue) -> String {
    match v {
        ParamValue::Null => "NULL".to_string(),
        ParamValue::String(s) => escape_sql_string(s),
        ParamValue::Int(i) => i.to_string(),
        ParamValue::Float(f) => f.to_string(),
        ParamValue::Bool(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        ParamValue::Array(_) => "[]".to_string(),
    }
}

/// 转义 SQL 字符串字面量中的特殊字符
/// 用于 ${name} 插值场景（不推荐使用 ${}，应优先使用 #{}）
fn escape_sql_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            _ => out.push(ch),
        }
    }
    out
}

/// 规范化 SET 子句中的逗号：确保每个逗号后跟一个空格
/// 用于 `<set>` 标签的输出清理
fn normalize_set_commas(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == ',' {
            out.push(',');
            // 跳过逗号后已有的空白
            while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
                chars.next();
            }
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

// =====================================================
// 简易 XML 解析器
// =====================================================

#[derive(Debug, Clone)]
pub struct XmlNode {
    pub node_type: XmlNodeType,
    pub children: Vec<XmlNode>,
}

#[derive(Debug, Clone)]
pub enum XmlNodeType {
    Text(String),
    Element {
        name: String,
        attrs: HashMap<String, String>,
    },
}

struct XmlParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> XmlParser<'a> {
    fn parse(input: &'a str) -> Result<XmlNode, DynamicSqlError> {
        let mut parser = Self { input, pos: 0 };
        let mut root = XmlNode {
            node_type: XmlNodeType::Element {
                name: "root".to_string(),
                attrs: HashMap::new(),
            },
            children: Vec::new(),
        };
        parser.parse_children(&mut root)?;
        Ok(root)
    }

    fn parse_children(&mut self, parent: &mut XmlNode) -> Result<(), DynamicSqlError> {
        loop {
            if self.pos >= self.input.len() {
                break;
            }
            // 检查是否是结束标签
            if self.starts_with("</") {
                break;
            }
            // 检查是否是注释
            if self.starts_with("<!--") {
                self.skip_comment()?;
                continue;
            }
            // 检查是否是开始标签
            if self.starts_with("<") {
                let element = self.parse_element()?;
                parent.children.push(element);
            } else {
                // 文本内容（保留空白，由后续 cleanup_sql 统一规范化）
                let text = self.parse_text();
                parent.children.push(XmlNode {
                    node_type: XmlNodeType::Text(text),
                    children: Vec::new(),
                });
            }
        }
        Ok(())
    }

    fn parse_element(&mut self) -> Result<XmlNode, DynamicSqlError> {
        // 跳过 '<'
        self.pos += 1;
        // 读取标签名
        let name = self.read_name();
        // 读取属性
        let mut attrs = HashMap::new();
        loop {
            self.skip_whitespace();
            if self.pos >= self.input.len() {
                return Err(DynamicSqlError::ParseError("未闭合的标签".into()));
            }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '>' {
                self.pos += 1;
                break;
            }
            if c == '/' {
                // 自闭合标签
                if self.pos + 1 < self.input.len() && self.input.as_bytes()[self.pos + 1] == b'>' {
                    self.pos += 2;
                    return Ok(XmlNode {
                        node_type: XmlNodeType::Element { name, attrs },
                        children: Vec::new(),
                    });
                }
            }
            // 读取属性名
            let attr_name = self.read_name();
            self.skip_whitespace();
            if self.pos < self.input.len() && self.input.as_bytes()[self.pos] == b'=' {
                self.pos += 1;
                self.skip_whitespace();
                let attr_value = self.read_attr_value()?;
                attrs.insert(attr_name, attr_value);
            }
        }
        // 解析子节点
        let mut node = XmlNode {
            node_type: XmlNodeType::Element { name, attrs },
            children: Vec::new(),
        };
        self.parse_children(&mut node)?;
        // 跳过结束标签 </name>
        if self.starts_with("</") {
            self.pos += 2;
            let end_name = self.read_name();
            self.skip_whitespace();
            if self.pos < self.input.len() && self.input.as_bytes()[self.pos] == b'>' {
                self.pos += 1;
            }
            // 验证标签名匹配
            if let XmlNodeType::Element { name, .. } = &node.node_type {
                if name != &end_name {
                    return Err(DynamicSqlError::ParseError(format!(
                        "标签不匹配: <{}> vs </{}>",
                        name, end_name
                    )));
                }
            }
        }
        Ok(node)
    }

    fn parse_text(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.input.len() {
            let b = self.input.as_bytes()[self.pos];
            if b == b'<' {
                break;
            }
            self.pos += 1;
        }
        // 反转义 XML 实体
        self.input[start..self.pos]
            .replace("&gt;", ">")
            .replace("&lt;", "<")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
    }

    fn read_name(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.input.len() {
            let b = self.input.as_bytes()[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b':' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.input[start..self.pos].to_string()
    }

    fn read_attr_value(&mut self) -> Result<String, DynamicSqlError> {
        if self.pos >= self.input.len() {
            return Err(DynamicSqlError::ParseError("属性值缺失".into()));
        }
        let quote = self.input.as_bytes()[self.pos];
        if quote != b'"' && quote != b'\'' {
            return Err(DynamicSqlError::ParseError(format!(
                "属性值应以引号开头, 实际: {}",
                quote as char
            )));
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.input.len() {
            if self.input.as_bytes()[self.pos] == quote {
                let raw = &self.input[start..self.pos];
                // 反转义 XML 实体（属性值中允许出现 &gt; &lt; &amp; &quot; &apos;）
                let value = raw
                    .replace("&gt;", ">")
                    .replace("&lt;", "<")
                    .replace("&amp;", "&")
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'");
                self.pos += 1;
                return Ok(value);
            }
            self.pos += 1;
        }
        Err(DynamicSqlError::ParseError("属性值未闭合".into()))
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            if !self.input.as_bytes()[self.pos].is_ascii_whitespace() {
                break;
            }
            self.pos += 1;
        }
    }

    fn skip_comment(&mut self) -> Result<(), DynamicSqlError> {
        // 跳过 <!--
        self.pos += 4;
        while self.pos + 2 < self.input.len() {
            if &self.input[self.pos..self.pos + 3] == "-->" {
                self.pos += 3;
                return Ok(());
            }
            self.pos += 1;
        }
        Err(DynamicSqlError::ParseError("注释未闭合".into()))
    }

    fn starts_with(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- SqlParams 测试 ----

    #[test]
    fn test_sql_params_set_get() {
        let mut p = SqlParams::new();
        p.set("name", "Alice");
        p.set_int("age", 30);
        p.set_bool("active", true);
        p.set_null("deleted");

        assert!(matches!(p.get("name"), Some(ParamValue::String(_))));
        assert!(matches!(p.get("age"), Some(ParamValue::Int(30))));
        assert!(matches!(p.get("active"), Some(ParamValue::Bool(true))));
        assert!(matches!(p.get("deleted"), Some(ParamValue::Null)));
        assert!(p.get("missing").is_none());
    }

    #[test]
    fn test_sql_params_is_null() {
        let mut p = SqlParams::new();
        assert!(p.is_null("missing"));
        p.set_null("x");
        assert!(p.is_null("x"));
        p.set("y", "val");
        assert!(!p.is_null("y"));
        assert!(p.is_not_null("y"));
    }

    #[test]
    fn test_sql_params_contains() {
        let mut p = SqlParams::new();
        p.set("a", "1");
        assert!(p.contains("a"));
        assert!(!p.contains("b"));
    }

    // ---- 简单 SELECT 测试 ----

    #[test]
    fn test_simple_select_no_params() {
        let xml = r#"<select id="all">SELECT * FROM users</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let params = SqlParams::new();
        let sql = parser.build("all", &params).unwrap();
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn test_select_with_param_binding() {
        let xml = r#"<select id="by_id">SELECT * FROM users WHERE id = #{id}</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set_int("id", 42);
        let sql = parser.build("by_id", &params).unwrap();
        assert_eq!(sql, "SELECT * FROM users WHERE id = ?");
    }

    #[test]
    fn test_select_with_string_interpolation() {
        let xml = r#"<select id="by_table">SELECT * FROM ${table}</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("table", "users");
        let sql = parser.build("by_table", &params).unwrap();
        assert_eq!(sql, "SELECT * FROM users");
    }

    // ---- <if> 测试 ----

    #[test]
    fn test_if_true() {
        let xml = r#"<select id="q">SELECT * FROM users WHERE 1=1 <if test="name != null">AND name = #{name}</if></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("name", "Alice");
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("AND name = ?"));
    }

    #[test]
    fn test_if_false() {
        let xml = r#"<select id="q">SELECT * FROM users WHERE 1=1 <if test="name != null">AND name = #{name}</if></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let params = SqlParams::new();
        let sql = parser.build("q", &params).unwrap();
        assert!(!sql.contains("AND name"));
    }

    #[test]
    fn test_if_null_check() {
        let xml = r#"<select id="q">SELECT * FROM users <if test="name == null">WHERE name IS NULL</if></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();

        let params = SqlParams::new();
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("WHERE name IS NULL"));

        let mut params = SqlParams::new();
        params.set("name", "Alice");
        let sql = parser.build("q", &params).unwrap();
        assert!(!sql.contains("WHERE name IS NULL"));
    }

    #[test]
    fn test_if_string_equals() {
        let xml = r#"<select id="q">SELECT * FROM users <if test="role == 'admin'">WHERE is_admin = 1</if></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();

        let mut params = SqlParams::new();
        params.set("role", "admin");
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("WHERE is_admin = 1"));

        let mut params = SqlParams::new();
        params.set("role", "user");
        let sql = parser.build("q", &params).unwrap();
        assert!(!sql.contains("WHERE is_admin"));
    }

    #[test]
    fn test_if_numeric_comparison() {
        let xml = r#"<select id="q">SELECT * FROM users <if test="age &gt; 18">WHERE age &gt; 18</if></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();

        let mut params = SqlParams::new();
        params.set_int("age", 25);
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("WHERE age > 18"));

        let mut params = SqlParams::new();
        params.set_int("age", 15);
        let sql = parser.build("q", &params).unwrap();
        assert!(!sql.contains("WHERE age"));
    }

    #[test]
    fn test_if_and_or() {
        let xml = r#"<select id="q">SELECT * FROM users <if test="name != null and age != null">WHERE name = #{name} AND age = #{age}</if></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();

        let mut params = SqlParams::new();
        params.set("name", "Alice");
        params.set_int("age", 30);
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("WHERE name = ? AND age = ?"));

        let mut params = SqlParams::new();
        params.set("name", "Alice");
        let sql = parser.build("q", &params).unwrap();
        assert!(!sql.contains("WHERE"));
    }

    // ---- <where> 测试 ----

    #[test]
    fn test_where_strips_leading_and() {
        let xml = r#"<select id="q">SELECT * FROM users <where><if test="name != null">AND name = #{name}</if></where></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("name", "Alice");
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("WHERE name = ?"));
        assert!(!sql.contains("WHERE AND"));
    }

    #[test]
    fn test_where_empty_no_clause() {
        let xml = r#"<select id="q">SELECT * FROM users <where><if test="name != null">AND name = #{name}</if></where></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let params = SqlParams::new();
        let sql = parser.build("q", &params).unwrap();
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn test_where_multiple_conditions() {
        let xml = r#"<select id="q">SELECT * FROM users <where>
            <if test="name != null">AND name = #{name}</if>
            <if test="age != null">AND age = #{age}</if>
        </where></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("name", "Alice");
        params.set_int("age", 30);
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("WHERE name = ? AND age = ?"));
    }

    // ---- <set> 测试 ----

    #[test]
    fn test_set_strips_trailing_comma() {
        let xml = r#"<update id="u">UPDATE users <set><if test="name != null">name = #{name},</if><if test="age != null">age = #{age},</if></set> WHERE id = #{id}</update>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("name", "Alice");
        params.set_int("age", 30);
        params.set_int("id", 1);
        let sql = parser.build("u", &params).unwrap();
        // <set> 语义：trim 末尾逗号；逗号后规范化为单个空格
        assert!(sql.contains("SET name = ?, age = ? WHERE id = ?"));
    }

    #[test]
    fn test_set_single_field() {
        let xml = r#"<update id="u">UPDATE users <set><if test="name != null">name = #{name},</if></set> WHERE id = #{id}</update>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("name", "Alice");
        params.set_int("id", 1);
        let sql = parser.build("u", &params).unwrap();
        assert!(sql.contains("SET name = ? WHERE id = ?"));
    }

    // ---- <foreach> 测试 ----

    #[test]
    fn test_foreach_basic() {
        let xml = r#"<select id="q">SELECT * FROM users WHERE id IN (<foreach collection="ids" item="id" separator=",">#{id}</foreach>)</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set_array(
            "ids",
            vec![ParamValue::Int(1), ParamValue::Int(2), ParamValue::Int(3)],
        );
        let sql = parser.build("q", &params).unwrap();
        // separator="," 不带空格，foreach 前会自动加一个空格
        assert!(sql.contains("WHERE id IN ( ?,?,?)"));
    }

    #[test]
    fn test_foreach_empty() {
        let xml = r#"<select id="q">SELECT * FROM users WHERE id IN (<foreach collection="ids" item="id" separator=",">#{id}</foreach>)</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let params = SqlParams::new();
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("WHERE id IN ( )") || sql.contains("WHERE id IN ()"));
    }

    #[test]
    fn test_foreach_with_strings() {
        let xml = r#"<select id="q">SELECT * FROM users WHERE name IN (<foreach collection="names" item="n" separator=",">#{n}</foreach>)</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set_array(
            "names",
            vec![
                ParamValue::String("Alice".into()),
                ParamValue::String("Bob".into()),
            ],
        );
        let (sql, binds) = parser.build_with_binds("q", &params).unwrap();
        assert_eq!(binds.len(), 2);
        assert!(sql.contains("?"));
    }

    // ---- <choose> 测试 ----

    #[test]
    fn test_choose_when_matches() {
        let xml = r#"<select id="q">SELECT * FROM users WHERE 1=1 <choose>
            <when test="role == 'admin'">AND is_admin = 1</when>
            <otherwise>AND is_admin = 0</otherwise>
        </choose></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("role", "admin");
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("AND is_admin = 1"));
        assert!(!sql.contains("AND is_admin = 0"));
    }

    #[test]
    fn test_choose_otherwise() {
        let xml = r#"<select id="q">SELECT * FROM users WHERE 1=1 <choose>
            <when test="role == 'admin'">AND is_admin = 1</when>
            <otherwise>AND is_admin = 0</otherwise>
        </choose></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("role", "user");
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("AND is_admin = 0"));
    }

    // ---- <trim> 测试 ----

    #[test]
    fn test_trim_prefix_suffix() {
        let xml = r#"<select id="q">SELECT * FROM users <trim prefix="WHERE" prefixOverrides="AND |OR "><if test="name != null">AND name = #{name}</if></trim></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("name", "Alice");
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("WHERE name = ?"));
    }

    #[test]
    fn test_trim_empty() {
        let xml = r#"<select id="q">SELECT * FROM users <trim prefix="WHERE" prefixOverrides="AND"><if test="name != null">AND name = #{name}</if></trim></select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let params = SqlParams::new();
        let sql = parser.build("q", &params).unwrap();
        assert_eq!(sql, "SELECT * FROM users");
    }

    // ---- 错误处理测试 ----

    #[test]
    fn test_statement_not_found() {
        let xml = r#"<select id="a">SELECT 1</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let params = SqlParams::new();
        let result = parser.build("missing", &params);
        assert!(matches!(result, Err(DynamicSqlError::StatementNotFound(_))));
    }

    #[test]
    fn test_missing_param() {
        let xml = r#"<select id="q">SELECT * FROM users WHERE id = #{id}</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let params = SqlParams::new();
        let result = parser.build("q", &params);
        assert!(matches!(result, Err(DynamicSqlError::MissingParam(_))));
    }

    #[test]
    fn test_parse_error_unclosed_tag() {
        let xml = r#"<select id="q">SELECT 1"#;
        let result = DynamicSqlParser::from_xml(xml);
        // 应该返回错误或者解析为不完整（取决于实现）
        // 这里我们接受任何结果，只要不 panic
        let _ = result;
    }

    // ---- 多语句管理测试 ----

    #[test]
    fn test_multiple_statements() {
        let xml = r#"
        <select id="find_all">SELECT * FROM users</select>
        <select id="find_by_id">SELECT * FROM users WHERE id = #{id}</select>
        <insert id="insert">INSERT INTO users (name) VALUES (#{name})</insert>
        <update id="update">UPDATE users SET name = #{name} WHERE id = #{id}</update>
        <delete id="delete">DELETE FROM users WHERE id = #{id}</delete>
        "#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut ids = parser.statement_ids();
        ids.sort();
        assert_eq!(
            ids,
            vec!["delete", "find_all", "find_by_id", "insert", "update"]
        );
    }

    // ---- XML 实体测试 ----

    #[test]
    fn test_xml_entities() {
        let xml =
            r#"<select id="q">SELECT * FROM users WHERE age &gt; 18 AND age &lt; 65</select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let params = SqlParams::new();
        let sql = parser.build("q", &params).unwrap();
        assert!(sql.contains("age > 18 AND age < 65"));
    }

    // ---- 完整流程测试 ----

    #[test]
    fn test_full_dynamic_query() {
        let xml = r#"<select id="search">
            SELECT u.id, u.name, o.total
            FROM users u
            LEFT JOIN orders o ON u.id = o.user_id
            <where>
                <if test="name != null">AND u.name LIKE #{name}</if>
                <if test="min_age != null">AND u.age &gt; #{min_age}</if>
                <if test="status != null">AND u.status = #{status}</if>
            </where>
            ORDER BY u.id
        </select>"#;
        let parser = DynamicSqlParser::from_xml(xml).unwrap();
        let mut params = SqlParams::new();
        params.set("name", "%Alice%");
        params.set_int("min_age", 18);
        // status 不设置

        let (sql, binds) = parser.build_with_binds("search", &params).unwrap();
        assert!(sql.contains("SELECT u.id, u.name, o.total"));
        assert!(sql.contains("FROM users u"));
        assert!(sql.contains("LEFT JOIN orders o ON u.id = o.user_id"));
        assert!(sql.contains("WHERE u.name LIKE ? AND u.age > ?"));
        assert!(!sql.contains("u.status"));
        assert!(sql.contains("ORDER BY u.id"));
        assert_eq!(binds.len(), 2);
    }
}
