//! GraphQL 查询中间表示（IR）与解析器
//!
//! 将 GraphQL 查询文本解析为结构化 IR，供复杂度计算和 N+1 消除使用。
//! 解析器为自研轻量递归下降解析器，不依赖 async-graphql，
//! 与既有 `real` feature 正交。

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// GraphQL 操作类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphQLOperation {
    Query,
    Mutation,
    Subscription,
}

/// GraphQL 值字面量
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GraphQLValue {
    Int(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Null,
    Enum(String),
    List(Vec<GraphQLValue>),
    Object(HashMap<String, GraphQLValue>),
    Variable(String),
}

/// GraphQL 指令
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQLDirective {
    pub name: String,
    pub arguments: HashMap<String, GraphQLValue>,
}

/// GraphQL 选择集项
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQLSelection {
    pub name: String,
    pub alias: Option<String>,
    pub arguments: HashMap<String, GraphQLValue>,
    pub directives: Vec<GraphQLDirective>,
    pub selection_set: Vec<GraphQLSelection>,
}

/// GraphQL 查询中间表示
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQLIR {
    pub operation: GraphQLOperation,
    pub selection_set: Vec<GraphQLSelection>,
}

/// GraphQL 解析错误
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQLParseError {
    pub position: usize,
    pub message: String,
}

impl fmt::Display for GraphQLParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GraphQL parse error at position {}: {}",
            self.position, self.message
        )
    }
}

impl std::error::Error for GraphQLParseError {}

/// 解析 GraphQL 查询文本为 IR
///
/// `variables` 参数保留用于执行时变量替换，解析阶段保持变量引用
/// （`GraphQLValue::Variable`），不在此处替换。
pub fn parse_query(
    query_text: &str,
    variables: Option<Value>,
) -> Result<GraphQLIR, GraphQLParseError> {
    let _ = variables;
    let mut parser = Parser::new(query_text);
    parser.parse_document()
}

// =========================================================================
// 轻量递归下降解析器
// =========================================================================

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn parse_document(&mut self) -> Result<GraphQLIR, GraphQLParseError> {
        self.skip_ws();
        if self.is_eof() {
            return Err(self.err("Empty query"));
        }

        let operation = if self.peek() == Some(b'{') {
            GraphQLOperation::Query
        } else {
            let name = self.parse_name()?;
            match name.as_str() {
                "query" => GraphQLOperation::Query,
                "mutation" => GraphQLOperation::Mutation,
                "subscription" => GraphQLOperation::Subscription,
                other => {
                    return Err(self.err(format!(
                        "Expected 'query', 'mutation', or 'subscription', got '{other}'"
                    )));
                }
            }
        };

        self.skip_ws();

        if self.peek().map(is_name_start).unwrap_or(false) {
            let _op_name = self.parse_name()?;
            self.skip_ws();
        }

        if self.peek() == Some(b'(') {
            self.skip_balanced(b'(', b')')?;
            self.skip_ws();
        }

        let selection_set = self.parse_selection_set()?;

        Ok(GraphQLIR {
            operation,
            selection_set,
        })
    }

    fn parse_selection_set(&mut self) -> Result<Vec<GraphQLSelection>, GraphQLParseError> {
        self.expect(b'{')?;
        let mut selections = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'}') => {
                    self.advance();
                    break;
                }
                Some(b'.') => {
                    self.skip_fragment()?;
                }
                Some(_) => {
                    selections.push(self.parse_selection()?);
                }
                None => return Err(self.err("Unexpected EOF in selection set")),
            }
        }
        Ok(selections)
    }

    fn skip_fragment(&mut self) -> Result<(), GraphQLParseError> {
        for _ in 0..3 {
            if self.peek() == Some(b'.') {
                self.advance();
            } else {
                return Err(self.err("Expected '...' for fragment spread"));
            }
        }
        self.skip_ws();
        if self.peek().map(is_name_start).unwrap_or(false) {
            let _ = self.parse_name()?;
            self.skip_ws();
        }
        if self.peek() == Some(b'@') {
            let _ = self.parse_directive()?;
        }
        Ok(())
    }

    fn parse_selection(&mut self) -> Result<GraphQLSelection, GraphQLParseError> {
        let first = self.parse_name()?;
        self.skip_ws();

        let (alias, name) = if self.peek() == Some(b':') {
            self.advance();
            self.skip_ws();
            let field_name = self.parse_name()?;
            (Some(first), field_name)
        } else {
            (None, first)
        };

        self.skip_ws();
        let arguments = if self.peek() == Some(b'(') {
            self.parse_arguments()?
        } else {
            HashMap::new()
        };

        self.skip_ws();
        let mut directives = Vec::new();
        while self.peek() == Some(b'@') {
            directives.push(self.parse_directive()?);
            self.skip_ws();
        }

        self.skip_ws();
        let selection_set = if self.peek() == Some(b'{') {
            self.parse_selection_set()?
        } else {
            Vec::new()
        };

        Ok(GraphQLSelection {
            name,
            alias,
            arguments,
            directives,
            selection_set,
        })
    }

    fn parse_arguments(&mut self) -> Result<HashMap<String, GraphQLValue>, GraphQLParseError> {
        self.expect(b'(')?;
        let mut args = HashMap::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(b')') {
                self.advance();
                break;
            }
            let key = self.parse_name()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let val = self.parse_value()?;
            args.insert(key, val);
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.advance();
            }
        }
        Ok(args)
    }

    fn parse_directive(&mut self) -> Result<GraphQLDirective, GraphQLParseError> {
        self.expect(b'@')?;
        let name = self.parse_name()?;
        self.skip_ws();
        let arguments = if self.peek() == Some(b'(') {
            self.parse_arguments()?
        } else {
            HashMap::new()
        };
        Ok(GraphQLDirective { name, arguments })
    }

    fn parse_value(&mut self) -> Result<GraphQLValue, GraphQLParseError> {
        self.skip_ws();
        match self.peek() {
            Some(b'$') => {
                self.advance();
                let name = self.parse_name()?;
                Ok(GraphQLValue::Variable(name))
            }
            Some(b'"') => {
                let s = self.parse_string()?;
                Ok(GraphQLValue::String(s))
            }
            Some(b'[') => self.parse_list_value(),
            Some(b'{') => self.parse_object_value(),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) if is_name_start(c) => {
                let name = self.parse_name()?;
                match name.as_str() {
                    "true" => Ok(GraphQLValue::Boolean(true)),
                    "false" => Ok(GraphQLValue::Boolean(false)),
                    "null" => Ok(GraphQLValue::Null),
                    other => Ok(GraphQLValue::Enum(other.to_string())),
                }
            }
            Some(c) => Err(self.err(format!("Unexpected character '{}'", c as char))),
            None => Err(self.err("Unexpected EOF while parsing value")),
        }
    }

    fn parse_number(&mut self) -> Result<GraphQLValue, GraphQLParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.advance();
        }
        while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            self.advance();
        }
        let is_float = if self.peek() == Some(b'.') {
            self.advance();
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.advance();
            }
            true
        } else {
            false
        };
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.advance();
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.advance();
            }
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.advance();
            }
            let text = std::str::from_utf8(&self.input[start..self.pos])
                .map_err(|_| self.err("Invalid UTF-8 in number"))?;
            return text
                .parse::<f64>()
                .map(GraphQLValue::Float)
                .map_err(|e| self.err(format!("Invalid float: {e}")));
        }
        let text = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| self.err("Invalid UTF-8 in number"))?;
        if is_float {
            text.parse::<f64>()
                .map(GraphQLValue::Float)
                .map_err(|e| self.err(format!("Invalid float: {e}")))
        } else {
            text.parse::<i64>()
                .map(GraphQLValue::Int)
                .map_err(|e| self.err(format!("Invalid int: {e}")))
        }
    }

    fn parse_list_value(&mut self) -> Result<GraphQLValue, GraphQLParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.advance();
                break;
            }
            items.push(self.parse_value()?);
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.advance();
            }
        }
        Ok(GraphQLValue::List(items))
    }

    fn parse_object_value(&mut self) -> Result<GraphQLValue, GraphQLParseError> {
        self.expect(b'{')?;
        let mut map = HashMap::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.advance();
                break;
            }
            let key = self.parse_name()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.advance();
            }
        }
        Ok(GraphQLValue::Object(map))
    }

    fn parse_string(&mut self) -> Result<String, GraphQLParseError> {
        if self.peek() == Some(b'"')
            && self.peek_at(1) == Some(b'"')
            && self.peek_at(2) == Some(b'"')
        {
            return self.parse_block_string();
        }
        self.expect(b'"')?;
        let mut s = String::new();
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.advance();
                    break;
                }
                Some(b'\\') => {
                    self.advance();
                    match self.peek() {
                        Some(b'"') => s.push('"'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'/') => s.push('/'),
                        Some(b'n') => s.push('\n'),
                        Some(b't') => s.push('\t'),
                        Some(b'r') => s.push('\r'),
                        Some(b'b') => s.push('\u{0008}'),
                        Some(b'f') => s.push('\u{000C}'),
                        Some(b'u') => {
                            self.advance();
                            let cp = self.parse_unicode_escape()?;
                            s.push(cp);
                            continue;
                        }
                        Some(c) => return Err(self.err(format!("Invalid escape: \\{}", c as char))),
                        None => return Err(self.err("Unexpected EOF in string escape")),
                    }
                    self.advance();
                }
                Some(c) => {
                    s.push(c as char);
                    self.advance();
                }
                None => return Err(self.err("Unexpected EOF in string")),
            }
        }
        Ok(s)
    }

    fn parse_block_string(&mut self) -> Result<String, GraphQLParseError> {
        for _ in 0..3 {
            self.expect(b'"')?;
        }
        let mut raw = String::new();
        loop {
            if self.peek() == Some(b'"')
                && self.peek_at(1) == Some(b'"')
                && self.peek_at(2) == Some(b'"')
            {
                self.advance();
                self.advance();
                self.advance();
                break;
            }
            match self.peek() {
                Some(c) => {
                    raw.push(c as char);
                    self.advance();
                }
                None => return Err(self.err("Unexpected EOF in block string")),
            }
        }
        Ok(raw
            .lines()
            .map(|l| l.trim_start().to_string())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn parse_unicode_escape(&mut self) -> Result<char, GraphQLParseError> {
        let mut code = 0u32;
        for _ in 0..4 {
            let c = self
                .peek()
                .ok_or_else(|| self.err("Unexpected EOF in unicode escape"))?;
            code = code * 16
                + (c as char)
                    .to_digit(16)
                    .ok_or_else(|| self.err(format!("Invalid hex digit: {}", c as char)))?;
            self.advance();
        }
        char::from_u32(code).ok_or_else(|| self.err(format!("Invalid unicode code point: {code}")))
    }

    fn parse_name(&mut self) -> Result<String, GraphQLParseError> {
        self.skip_ws();
        let start = self.pos;
        if !self.peek().map(is_name_start).unwrap_or(false) {
            return Err(self.err("Expected name"));
        }
        self.advance();
        while self.peek().map(is_name_continue).unwrap_or(false) {
            self.advance();
        }
        std::str::from_utf8(&self.input[start..self.pos])
            .map(|s| s.to_string())
            .map_err(|_| self.err("Invalid UTF-8 in name"))
    }

    fn skip_balanced(&mut self, open: u8, close: u8) -> Result<(), GraphQLParseError> {
        self.expect(open)?;
        let mut depth = 1;
        while depth > 0 {
            match self.peek() {
                Some(c) if c == open => {
                    depth += 1;
                    self.advance();
                }
                Some(c) if c == close => {
                    depth -= 1;
                    self.advance();
                }
                Some(b'"') => {
                    let _ = self.parse_string()?;
                }
                Some(_) => self.advance(),
                None => return Err(self.err("Unexpected EOF in balanced delimiter")),
            }
        }
        Ok(())
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() || c == b',' {
                self.advance();
            } else if c == b'#' {
                while self.peek().map(|c| c != b'\n').unwrap_or(false) {
                    self.advance();
                }
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn expect(&mut self, c: u8) -> Result<(), GraphQLParseError> {
        if self.peek() == Some(c) {
            self.advance();
            Ok(())
        } else {
            Err(self.err(format!(
                "Expected '{}', got {}",
                c as char,
                self.peek()
                    .map(|c| format!("'{}'", c as char))
                    .unwrap_or("EOF".to_string())
            )))
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn err(&self, msg: impl Into<String>) -> GraphQLParseError {
        GraphQLParseError {
            position: self.pos,
            message: msg.into(),
        }
    }
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_name_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shorthand_query() {
        let ir = parse_query("{ user { id name } }", None).unwrap();
        assert_eq!(ir.operation, GraphQLOperation::Query);
        assert_eq!(ir.selection_set.len(), 1);
        assert_eq!(ir.selection_set[0].name, "user");
        assert_eq!(ir.selection_set[0].selection_set.len(), 2);
        assert_eq!(ir.selection_set[0].selection_set[0].name, "id");
        assert_eq!(ir.selection_set[0].selection_set[1].name, "name");
    }

    #[test]
    fn test_parse_named_query() {
        let ir = parse_query("query GetUser { user { id } }", None).unwrap();
        assert_eq!(ir.operation, GraphQLOperation::Query);
        assert_eq!(ir.selection_set[0].name, "user");
    }

    #[test]
    fn test_parse_mutation() {
        let ir = parse_query(
            "mutation CreateUser { createUser(name: \"Alice\") { id } }",
            None,
        )
        .unwrap();
        assert_eq!(ir.operation, GraphQLOperation::Mutation);
        assert_eq!(ir.selection_set[0].name, "createUser");
        assert_eq!(
            ir.selection_set[0].arguments.get("name"),
            Some(&GraphQLValue::String("Alice".to_string()))
        );
    }

    #[test]
    fn test_parse_subscription() {
        let ir = parse_query("subscription OnUpdate { onUpdate { id } }", None).unwrap();
        assert_eq!(ir.operation, GraphQLOperation::Subscription);
    }

    #[test]
    fn test_parse_variable_definitions_skipped() {
        let ir = parse_query(
            "query GetUser($id: ID!, $limit: Int) { user(id: $id) { id } }",
            None,
        )
        .unwrap();
        assert_eq!(ir.operation, GraphQLOperation::Query);
        assert_eq!(
            ir.selection_set[0].arguments.get("id"),
            Some(&GraphQLValue::Variable("id".to_string()))
        );
    }

    #[test]
    fn test_parse_alias() {
        let ir = parse_query("{ aliasedUser: user { id } }", None).unwrap();
        assert_eq!(ir.selection_set[0].alias, Some("aliasedUser".to_string()));
        assert_eq!(ir.selection_set[0].name, "user");
    }

    #[test]
    fn test_parse_directives() {
        let ir = parse_query("{ user @include(if: true) { id @skip(if: false) } }", None).unwrap();
        assert_eq!(ir.selection_set[0].directives.len(), 1);
        assert_eq!(ir.selection_set[0].directives[0].name, "include");
        assert_eq!(
            ir.selection_set[0].directives[0].arguments.get("if"),
            Some(&GraphQLValue::Boolean(true))
        );
        assert_eq!(ir.selection_set[0].selection_set[0].directives.len(), 1);
        assert_eq!(
            ir.selection_set[0].selection_set[0].directives[0].name,
            "skip"
        );
    }

    #[test]
    fn test_parse_value_types() {
        let ir = parse_query(
            "{ f(intArg: 42, floatArg: 2.5, strArg: \"hello\", boolArg: true, nullArg: null, enumArg: ASC, listArg: [1, 2, 3], objArg: {key: \"val\"}) { id } }",
            None,
        )
        .unwrap();
        let args = &ir.selection_set[0].arguments;
        assert_eq!(args.get("intArg"), Some(&GraphQLValue::Int(42)));
        assert!(
            matches!(args.get("floatArg"), Some(GraphQLValue::Float(f)) if (*f - 2.5).abs() < 1e-10)
        );
        assert_eq!(
            args.get("strArg"),
            Some(&GraphQLValue::String("hello".to_string()))
        );
        assert_eq!(args.get("boolArg"), Some(&GraphQLValue::Boolean(true)));
        assert_eq!(args.get("nullArg"), Some(&GraphQLValue::Null));
        assert_eq!(
            args.get("enumArg"),
            Some(&GraphQLValue::Enum("ASC".to_string()))
        );
        assert!(matches!(args.get("listArg"), Some(GraphQLValue::List(_))));
        assert!(matches!(args.get("objArg"), Some(GraphQLValue::Object(_))));
    }

    #[test]
    fn test_parse_nested_selection() {
        let ir = parse_query(
            "{ user { id name orders { id total items { name price } } } }",
            None,
        )
        .unwrap();
        let user = &ir.selection_set[0];
        assert_eq!(user.name, "user");
        assert_eq!(user.selection_set.len(), 3);
        let orders = &user.selection_set[2];
        assert_eq!(orders.name, "orders");
        assert_eq!(orders.selection_set.len(), 3);
        let items = &orders.selection_set[2];
        assert_eq!(items.name, "items");
        assert_eq!(items.selection_set.len(), 2);
    }

    #[test]
    fn test_parse_error_empty() {
        let result = parse_query("", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Empty"));
    }

    #[test]
    fn test_parse_error_invalid_operation() {
        let result = parse_query("foobar { id }", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Expected"));
    }

    #[test]
    fn test_parse_error_unexpected_eof() {
        let result = parse_query("{ user { id ", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_fragment_spread() {
        let ir = parse_query("{ user { id ...UserFields } }", None).unwrap();
        assert_eq!(ir.selection_set[0].selection_set.len(), 1);
    }

    #[test]
    fn test_parse_negative_number() {
        let ir = parse_query("{ f(arg: -42) { id } }", None).unwrap();
        assert_eq!(
            ir.selection_set[0].arguments.get("arg"),
            Some(&GraphQLValue::Int(-42))
        );
    }

    #[test]
    fn test_parse_iri_roundtrip_semantics() {
        let query = "{ user(id: 1) { id name email } }";
        let ir = parse_query(query, None).unwrap();
        assert_eq!(ir.operation, GraphQLOperation::Query);
        assert_eq!(ir.selection_set.len(), 1);
        let sel = &ir.selection_set[0];
        assert_eq!(sel.name, "user");
        assert_eq!(sel.arguments.len(), 1);
        assert_eq!(sel.selection_set.len(), 3);
    }
}
