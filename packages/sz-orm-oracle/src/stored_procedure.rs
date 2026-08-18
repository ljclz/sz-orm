//! Oracle PL/SQL 存储过程调用构建器
//!
//! 提供 [`StoredProcedureBuilder`] 用于以类型安全的方式构建 PL/SQL
//! 存储过程与函数调用，支持 IN/OUT/IN OUT 参数模式、命名参数、
//! 批量调用、结果集游标返回等。

use std::collections::HashMap;
use std::fmt;

/// PL/SQL 参数模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamMode {
    /// 输入参数（IN）
    In,
    /// 输出参数（OUT）
    Out,
    /// 输入输出参数（IN OUT）
    InOut,
}

impl ParamMode {
    /// 返回 PL/SQL 关键字
    #[must_use]
    pub fn as_keyword(&self) -> &'static str {
        match self {
            ParamMode::In => "IN",
            ParamMode::Out => "OUT",
            ParamMode::InOut => "IN OUT",
        }
    }
}

impl fmt::Display for ParamMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_keyword())
    }
}

/// PL/SQL 参数类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Number,
    Varchar2,
    Date,
    Timestamp,
    Clob,
    Blob,
    RefCursor,
    Boolean,
    Integer,
    PlsqlTable,
}

impl ParamType {
    /// 返回 SQL 类型名
    #[must_use]
    pub fn as_sql(&self) -> &'static str {
        match self {
            ParamType::Number => "NUMBER",
            ParamType::Varchar2 => "VARCHAR2",
            ParamType::Date => "DATE",
            ParamType::Timestamp => "TIMESTAMP",
            ParamType::Clob => "CLOB",
            ParamType::Blob => "BLOB",
            ParamType::RefCursor => "SYS_REFCURSOR",
            ParamType::Boolean => "BOOLEAN",
            ParamType::Integer => "INTEGER",
            ParamType::PlsqlTable => "TABLE OF VARCHAR2",
        }
    }
}

/// PL/SQL 参数定义
#[derive(Debug, Clone)]
pub struct ProcedureParam {
    /// 参数名
    pub name: String,
    /// 参数模式
    pub mode: ParamMode,
    /// 参数类型
    pub param_type: ParamType,
    /// 字符串字面量值（用于生成匿名块）
    pub literal_value: Option<String>,
}

impl ProcedureParam {
    /// 创建 IN 参数
    #[must_use]
    pub fn in_param(name: &str, param_type: ParamType) -> Self {
        Self {
            name: name.to_string(),
            mode: ParamMode::In,
            param_type,
            literal_value: None,
        }
    }

    /// 创建 OUT 参数
    #[must_use]
    pub fn out_param(name: &str, param_type: ParamType) -> Self {
        Self {
            name: name.to_string(),
            mode: ParamMode::Out,
            param_type,
            literal_value: None,
        }
    }

    /// 创建 IN OUT 参数
    #[must_use]
    pub fn in_out_param(name: &str, param_type: ParamType) -> Self {
        Self {
            name: name.to_string(),
            mode: ParamMode::InOut,
            param_type,
            literal_value: None,
        }
    }

    /// 设置字面量值
    #[must_use]
    pub fn with_value(mut self, value: &str) -> Self {
        self.literal_value = Some(value.to_string());
        self
    }

    /// 是否为输出参数（OUT 或 IN OUT）
    #[must_use]
    pub fn is_output(&self) -> bool {
        matches!(self.mode, ParamMode::Out | ParamMode::InOut)
    }

    /// 是否为输入参数（IN 或 IN OUT）
    #[must_use]
    pub fn is_input(&self) -> bool {
        matches!(self.mode, ParamMode::In | ParamMode::InOut)
    }
}

/// PL/SQL 存储过程调用构建器
///
/// 以类型安全方式构建存储过程/函数调用，生成匿名 PL/SQL 块。
#[derive(Debug, Clone)]
pub struct StoredProcedureBuilder {
    /// 过程名（schema.name 格式）
    name: String,
    /// 是否为函数（有返回值）
    is_function: bool,
    /// 返回类型（仅函数）
    return_type: Option<ParamType>,
    /// 参数列表（保持插入顺序）
    params: Vec<ProcedureParam>,
    /// 是否使用命名参数调用
    use_named_args: bool,
    /// schema 前缀
    schema: Option<String>,
}

impl StoredProcedureBuilder {
    /// 创建存储过程构建器
    #[must_use]
    pub fn procedure(name: &str) -> Self {
        Self {
            name: name.to_string(),
            is_function: false,
            return_type: None,
            params: Vec::new(),
            use_named_args: false,
            schema: None,
        }
    }

    /// 创建函数构建器
    #[must_use]
    pub fn function(name: &str, return_type: ParamType) -> Self {
        Self {
            name: name.to_string(),
            is_function: true,
            return_type: Some(return_type),
            params: Vec::new(),
            use_named_args: false,
            schema: None,
        }
    }

    /// 设置 schema 前缀
    #[must_use]
    pub fn with_schema(mut self, schema: &str) -> Self {
        self.schema = Some(schema.to_string());
        self
    }

    /// 启用命名参数调用（`proc(p1 => v1, p2 => v2)`）
    #[must_use]
    pub fn with_named_args(mut self) -> Self {
        self.use_named_args = true;
        self
    }

    /// 添加 IN 参数
    #[must_use]
    pub fn in_param(mut self, name: &str, param_type: ParamType) -> Self {
        self.params.push(ProcedureParam::in_param(name, param_type));
        self
    }

    /// 添加 IN 参数并设置字面量值
    #[must_use]
    pub fn in_value(mut self, name: &str, param_type: ParamType, value: &str) -> Self {
        self.params
            .push(ProcedureParam::in_param(name, param_type).with_value(value));
        self
    }

    /// 添加 OUT 参数
    #[must_use]
    pub fn out_param(mut self, name: &str, param_type: ParamType) -> Self {
        self.params
            .push(ProcedureParam::out_param(name, param_type));
        self
    }

    /// 添加 IN OUT 参数
    #[must_use]
    pub fn in_out_param(mut self, name: &str, param_type: ParamType) -> Self {
        self.params
            .push(ProcedureParam::in_out_param(name, param_type));
        self
    }

    /// 添加 IN OUT 参数并设置字面量值
    #[must_use]
    pub fn in_out_value(mut self, name: &str, param_type: ParamType, value: &str) -> Self {
        self.params
            .push(ProcedureParam::in_out_param(name, param_type).with_value(value));
        self
    }

    /// 获取完全限定名（schema.name）
    #[must_use]
    pub fn full_name(&self) -> String {
        match &self.schema {
            Some(s) => format!("{s}.{}", self.name),
            None => self.name.clone(),
        }
    }

    /// 获取参数列表
    #[must_use]
    pub fn params(&self) -> &[ProcedureParam] {
        &self.params
    }

    /// 生成参数声明部分（DECLARE 块）
    fn build_declarations(&self) -> String {
        let mut decls = Vec::new();
        for p in &self.params {
            let init = match (&p.literal_value, p.mode) {
                (Some(v), ParamMode::In | ParamMode::InOut) => {
                    format!(" := '{}'", v)
                }
                _ => String::new(),
            };
            decls.push(format!(
                "  {} {}{}{}",
                p.name,
                p.param_type.as_sql(),
                init,
                ";"
            ));
        }
        if self.is_function {
            if let Some(rt) = self.return_type {
                decls.push(format!("  result {};", rt.as_sql()));
            }
        }
        decls.join("\n")
    }

    /// 生成调用语句部分（BEGIN 块内）
    fn build_call(&self) -> String {
        let args: Vec<String> = if self.use_named_args {
            self.params
                .iter()
                .map(|p| format!("{} => {}", p.name, p.name))
                .collect()
        } else {
            self.params.iter().map(|p| p.name.clone()).collect()
        };
        let joined = args.join(", ");
        if self.is_function {
            format!("  result := {}({});", self.full_name(), joined)
        } else {
            format!("  {}({});", self.full_name(), joined)
        }
    }

    /// 生成输出参数提取部分（DBMS_OUTPUT.PUT_LINE）
    fn build_output_extraction(&self) -> String {
        let outputs: Vec<String> = self
            .params
            .iter()
            .filter(|p| p.is_output())
            .map(|p| format!("  DBMS_OUTPUT.PUT_LINE('{}=' || {});", p.name, p.name))
            .collect();
        outputs.join("\n")
    }

    /// 生成完整 PL/SQL 匿名块
    #[must_use]
    pub fn build(&self) -> String {
        let decls = self.build_declarations();
        let call = self.build_call();
        let outputs = self.build_output_extraction();
        if decls.is_empty() {
            if outputs.is_empty() {
                format!("BEGIN\n{}\nEND;", call)
            } else {
                format!("BEGIN\n{}\n{}\nEND;", call, outputs)
            }
        } else if outputs.is_empty() {
            format!("DECLARE\n{}\nBEGIN\n{}\nEND;", decls, call)
        } else {
            format!("DECLARE\n{}\nBEGIN\n{}\n{}\nEND;", decls, call, outputs)
        }
    }

    /// 生成 CREATE PROCEDURE DDL
    ///
    /// 用于在数据库中创建该存储过程（与调用匿名块不同）。
    #[must_use]
    pub fn build_create_ddl(&self, body: &str) -> String {
        let params_ddl: Vec<String> = self
            .params
            .iter()
            .map(|p| {
                format!(
                    "{} {} {}",
                    p.name,
                    p.mode.as_keyword(),
                    p.param_type.as_sql()
                )
            })
            .collect();
        let params_str = params_ddl.join(", ");
        let header = if self.is_function {
            let rt = self.return_type.map(|t| t.as_sql()).unwrap_or("NUMBER");
            format!(
                "CREATE OR REPLACE FUNCTION {} ({}) RETURN {} AS",
                self.full_name(),
                params_str,
                rt
            )
        } else {
            format!(
                "CREATE OR REPLACE PROCEDURE {} ({}) AS",
                self.full_name(),
                params_str
            )
        };
        format!("{}\nBEGIN\n  {}\nEND;", header, body)
    }

    /// 生成 DROP 语句
    #[must_use]
    pub fn build_drop_ddl(&self) -> String {
        let kind = if self.is_function {
            "FUNCTION"
        } else {
            "PROCEDURE"
        };
        format!("DROP {} {};", kind, self.full_name())
    }

    /// 统计参数数量
    #[must_use]
    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    /// 统计输出参数数量
    #[must_use]
    pub fn output_param_count(&self) -> usize {
        self.params.iter().filter(|p| p.is_output()).count()
    }

    /// 统计输入参数数量
    #[must_use]
    pub fn input_param_count(&self) -> usize {
        self.params.iter().filter(|p| p.is_input()).count()
    }

    /// 转换为参数映射（名称 -> 参数定义）
    #[must_use]
    pub fn to_param_map(&self) -> HashMap<String, ProcedureParam> {
        self.params
            .iter()
            .map(|p| (p.name.clone(), p.clone()))
            .collect()
    }
}

impl fmt::Display for StoredProcedureBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.build())
    }
}

/// 批量存储过程调用
///
/// 对同一存储过程生成多次调用的匿名块，减少网络往返。
#[derive(Debug, Clone)]
pub struct BatchProcedureCall {
    /// 基础构建器
    builder: StoredProcedureBuilder,
    /// 批量参数值（每行一组）
    batch_values: Vec<Vec<String>>,
}

impl BatchProcedureCall {
    /// 创建批量调用
    #[must_use]
    pub fn new(builder: StoredProcedureBuilder) -> Self {
        Self {
            builder,
            batch_values: Vec::new(),
        }
    }

    /// 添加一行参数值
    #[must_use]
    pub fn add_row(mut self, values: Vec<String>) -> Self {
        self.batch_values.push(values);
        self
    }

    /// 生成批量调用匿名块
    #[must_use]
    pub fn build(&self) -> String {
        let mut calls = Vec::new();
        let full_name = self.builder.full_name();
        let param_names: Vec<&str> = self
            .builder
            .params
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        for row in &self.batch_values {
            let assigns: Vec<String> = param_names
                .iter()
                .zip(row.iter())
                .map(|(n, v)| format!("  {n} := '{v}';"))
                .collect();
            let args: Vec<String> = param_names.iter().map(|n| n.to_string()).collect();
            let call = if self.builder.is_function {
                format!("  result := {}({});", full_name, args.join(", "))
            } else {
                format!("  {}({});", full_name, args.join(", "))
            };
            calls.push(format!("{}\n{}", assigns.join("\n"), call));
        }
        let decls = self.builder.build_declarations();
        format!("DECLARE\n{}\nBEGIN\n{}\nEND;", decls, calls.join("\n"))
    }

    /// 批量行数
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.batch_values.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_param_mode_keyword() {
        assert_eq!(ParamMode::In.as_keyword(), "IN");
        assert_eq!(ParamMode::Out.as_keyword(), "OUT");
        assert_eq!(ParamMode::InOut.as_keyword(), "IN OUT");
    }

    #[test]
    fn test_param_type_sql() {
        assert_eq!(ParamType::Number.as_sql(), "NUMBER");
        assert_eq!(ParamType::RefCursor.as_sql(), "SYS_REFCURSOR");
        assert_eq!(ParamType::Varchar2.as_sql(), "VARCHAR2");
    }

    #[test]
    fn test_procedure_param_in() {
        let p = ProcedureParam::in_param("p1", ParamType::Number);
        assert!(p.is_input());
        assert!(!p.is_output());
    }

    #[test]
    fn test_procedure_param_out() {
        let p = ProcedureParam::out_param("p2", ParamType::Varchar2);
        assert!(!p.is_input());
        assert!(p.is_output());
    }

    #[test]
    fn test_procedure_param_in_out() {
        let p = ProcedureParam::in_out_param("p3", ParamType::Date);
        assert!(p.is_input());
        assert!(p.is_output());
    }

    #[test]
    fn test_procedure_param_with_value() {
        let p = ProcedureParam::in_param("p1", ParamType::Number).with_value("42");
        assert_eq!(p.literal_value.as_deref(), Some("42"));
    }

    #[test]
    fn test_stored_procedure_basic() {
        let sp = StoredProcedureBuilder::procedure("my_proc")
            .in_value("p1", ParamType::Number, "42")
            .out_param("p2", ParamType::Varchar2);
        let sql = sp.build();
        assert!(sql.contains("DECLARE"));
        assert!(sql.contains("BEGIN"));
        assert!(sql.contains("my_proc(p1, p2)"));
        assert!(sql.contains("p1 NUMBER := '42';"));
        assert!(sql.contains("DBMS_OUTPUT.PUT_LINE('p2=' || p2)"));
    }

    #[test]
    fn test_stored_procedure_function() {
        let sp = StoredProcedureBuilder::function("my_func", ParamType::Number).in_value(
            "x",
            ParamType::Number,
            "10",
        );
        let sql = sp.build();
        assert!(sql.contains("result NUMBER;"));
        assert!(sql.contains("result := my_func(x)"));
    }

    #[test]
    fn test_stored_procedure_named_args() {
        let sp = StoredProcedureBuilder::procedure("my_proc")
            .with_named_args()
            .in_param("p1", ParamType::Number)
            .in_param("p2", ParamType::Varchar2);
        let sql = sp.build();
        assert!(sql.contains("p1 => p1"));
        assert!(sql.contains("p2 => p2"));
    }

    #[test]
    fn test_stored_procedure_schema() {
        let sp = StoredProcedureBuilder::procedure("my_proc")
            .with_schema("my_schema")
            .in_param("p1", ParamType::Number);
        let sql = sp.build();
        assert!(sql.contains("my_schema.my_proc"));
    }

    #[test]
    fn test_stored_procedure_full_name() {
        let sp = StoredProcedureBuilder::procedure("my_proc").with_schema("app");
        assert_eq!(sp.full_name(), "app.my_proc");
    }

    #[test]
    fn test_stored_procedure_no_params() {
        let sp = StoredProcedureBuilder::procedure("simple_proc");
        let sql = sp.build();
        assert!(sql.contains("simple_proc()"));
    }

    #[test]
    fn test_stored_procedure_create_ddl() {
        let sp = StoredProcedureBuilder::procedure("my_proc")
            .in_param("p1", ParamType::Number)
            .out_param("p2", ParamType::Varchar2);
        let ddl = sp.build_create_ddl("NULL;");
        assert!(ddl.contains("CREATE OR REPLACE PROCEDURE"));
        assert!(ddl.contains("p1 IN NUMBER"));
        assert!(ddl.contains("p2 OUT VARCHAR2"));
    }

    #[test]
    fn test_stored_procedure_create_function_ddl() {
        let sp = StoredProcedureBuilder::function("my_func", ParamType::Number)
            .in_param("x", ParamType::Number);
        let ddl = sp.build_create_ddl("RETURN x;");
        assert!(ddl.contains("CREATE OR REPLACE FUNCTION"));
        assert!(ddl.contains("RETURN NUMBER"));
    }

    #[test]
    fn test_stored_procedure_drop_ddl() {
        let sp = StoredProcedureBuilder::procedure("my_proc");
        assert_eq!(sp.build_drop_ddl(), "DROP PROCEDURE my_proc;");
    }

    #[test]
    fn test_stored_procedure_drop_function_ddl() {
        let sp = StoredProcedureBuilder::function("my_func", ParamType::Number);
        assert_eq!(sp.build_drop_ddl(), "DROP FUNCTION my_func;");
    }

    #[test]
    fn test_stored_procedure_param_counts() {
        let sp = StoredProcedureBuilder::procedure("my_proc")
            .in_param("p1", ParamType::Number)
            .out_param("p2", ParamType::Varchar2)
            .in_out_param("p3", ParamType::Date);
        assert_eq!(sp.param_count(), 3);
        assert_eq!(sp.input_param_count(), 2);
        assert_eq!(sp.output_param_count(), 2);
    }

    #[test]
    fn test_stored_procedure_to_param_map() {
        let sp = StoredProcedureBuilder::procedure("my_proc")
            .in_param("p1", ParamType::Number)
            .out_param("p2", ParamType::Varchar2);
        let map = sp.to_param_map();
        assert!(map.contains_key("p1"));
        assert!(map.contains_key("p2"));
    }

    #[test]
    fn test_stored_procedure_display() {
        let sp =
            StoredProcedureBuilder::procedure("my_proc").in_value("p1", ParamType::Number, "1");
        let s = format!("{}", sp);
        assert!(s.contains("my_proc"));
    }

    #[test]
    fn test_batch_procedure_call() {
        let sp = StoredProcedureBuilder::procedure("my_proc").in_param("p1", ParamType::Number);
        let batch = BatchProcedureCall::new(sp)
            .add_row(vec!["1".to_string()])
            .add_row(vec!["2".to_string()])
            .add_row(vec!["3".to_string()]);
        let sql = batch.build();
        assert!(sql.contains("my_proc(p1)"));
        assert_eq!(batch.row_count(), 3);
    }

    #[test]
    fn test_batch_procedure_call_empty() {
        let sp = StoredProcedureBuilder::procedure("my_proc").in_param("p1", ParamType::Number);
        let batch = BatchProcedureCall::new(sp);
        let sql = batch.build();
        assert!(sql.contains("BEGIN"));
        assert_eq!(batch.row_count(), 0);
    }

    #[test]
    fn test_in_out_param_with_value() {
        let sp = StoredProcedureBuilder::procedure("my_proc").in_out_value(
            "p1",
            ParamType::Number,
            "100",
        );
        let sql = sp.build();
        assert!(sql.contains("p1 NUMBER := '100'"));
        assert!(sql.contains("DBMS_OUTPUT.PUT_LINE('p1=' || p1)"));
    }
}
