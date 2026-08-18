//! T-SQL 存储过程构建器
//!
//! 提供 [`TSqlStoredProcedure`] 用于以类型安全方式构建 SQL Server
//! T-SQL 存储过程与函数，支持参数默认值、OUTPUT 参数、表值参数等。

use std::collections::HashMap;
use std::fmt;

/// T-SQL 参数方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TSqlParamDirection {
    /// 输入参数
    Input,
    /// 输出参数（OUTPUT）
    Output,
    /// 输入输出参数（INPUT OUTPUT）
    InputOutput,
    /// 表值参数（TVP）
    TableValued,
}

impl TSqlParamDirection {
    /// 返回 T-SQL 关键字
    #[must_use]
    pub fn as_keyword(&self) -> &'static str {
        match self {
            TSqlParamDirection::Input => "",
            TSqlParamDirection::Output => "OUTPUT",
            TSqlParamDirection::InputOutput => "OUTPUT",
            TSqlParamDirection::TableValued => "READONLY",
        }
    }

    /// 是否为输出参数
    #[must_use]
    pub fn is_output(&self) -> bool {
        matches!(
            self,
            TSqlParamDirection::Output | TSqlParamDirection::InputOutput
        )
    }
}

impl fmt::Display for TSqlParamDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_keyword())
    }
}

/// T-SQL 参数定义
#[derive(Debug, Clone)]
pub struct TSqlParameter {
    /// 参数名（含 @ 前缀）
    pub name: String,
    /// 数据类型
    pub data_type: String,
    /// 方向
    pub direction: TSqlParamDirection,
    /// 默认值（= NULL）
    pub default_value: Option<String>,
    /// 最大长度（VARCHAR/NVARCHAR/VARBINARY）
    pub max_length: Option<i32>,
    /// 精度（DECIMAL/NUMERIC）
    pub precision: Option<u8>,
    /// 标度（DECIMAL/NUMERIC）
    pub scale: Option<u8>,
}

impl TSqlParameter {
    /// 创建输入参数
    #[must_use]
    pub fn input(name: &str, data_type: &str) -> Self {
        Self {
            name: ensure_at_prefix(name),
            data_type: data_type.to_string(),
            direction: TSqlParamDirection::Input,
            default_value: None,
            max_length: None,
            precision: None,
            scale: None,
        }
    }

    /// 创建输出参数
    #[must_use]
    pub fn output(name: &str, data_type: &str) -> Self {
        Self {
            direction: TSqlParamDirection::Output,
            ..Self::input(name, data_type)
        }
    }

    /// 创建输入输出参数
    #[must_use]
    pub fn input_output(name: &str, data_type: &str) -> Self {
        Self {
            direction: TSqlParamDirection::InputOutput,
            ..Self::input(name, data_type)
        }
    }

    /// 创建表值参数
    #[must_use]
    pub fn table_valued(name: &str, table_type: &str) -> Self {
        Self {
            name: ensure_at_prefix(name),
            data_type: table_type.to_string(),
            direction: TSqlParamDirection::TableValued,
            default_value: None,
            max_length: None,
            precision: None,
            scale: None,
        }
    }

    /// 设置默认值
    #[must_use]
    pub fn with_default(mut self, value: &str) -> Self {
        self.default_value = Some(value.to_string());
        self
    }

    /// 设置最大长度（-1 表示 MAX）
    #[must_use]
    pub fn with_max_length(mut self, length: i32) -> Self {
        self.max_length = Some(length);
        self
    }

    /// 设置精度与标度
    #[must_use]
    pub fn with_precision(mut self, precision: u8, scale: u8) -> Self {
        self.precision = Some(precision);
        self.scale = Some(scale);
        self
    }

    /// 生成参数声明（用于 CREATE PROCEDURE）
    #[must_use]
    pub fn to_declaration(&self) -> String {
        let mut result = format!("{} {}", self.name, self.format_data_type());
        if let Some(ref default) = self.default_value {
            result.push_str(&format!(" = {default}"));
        }
        let keyword = self.direction.as_keyword();
        if !keyword.is_empty() {
            result.push(' ');
            result.push_str(keyword);
        }
        result
    }

    /// 格式化数据类型（含长度/精度）
    fn format_data_type(&self) -> String {
        if let Some(p) = self.precision {
            let s = self.scale.unwrap_or(0);
            return format!("{}({p}, {s})", self.data_type);
        }
        if let Some(len) = self.max_length {
            if len == -1 {
                return format!("{}(MAX)", self.data_type);
            }
            return format!("{}({len})", self.data_type);
        }
        self.data_type.clone()
    }
}

/// 确保参数名以 @ 开头
fn ensure_at_prefix(name: &str) -> String {
    if name.starts_with('@') {
        name.to_string()
    } else {
        format!("@{name}")
    }
}

/// T-SQL 存储过程构建器
#[derive(Debug, Clone)]
pub struct TSqlStoredProcedure {
    /// 过程名（schema.name）
    name: String,
    /// 是否为函数
    is_function: bool,
    /// 返回类型（仅函数）
    return_type: Option<String>,
    /// 参数列表
    params: Vec<TSqlParameter>,
    /// schema 前缀
    schema: Option<String>,
    /// 过程体
    body: String,
    /// 是否使用 WITH ENCRYPTION
    with_encryption: bool,
    /// 是否使用 WITH RECOMPILE
    with_recompile: bool,
    /// 是否使用 WITH SCHEMABINDING（仅函数）
    with_schemabinding: bool,
}

impl TSqlStoredProcedure {
    /// 创建存储过程构建器
    #[must_use]
    pub fn procedure(name: &str) -> Self {
        Self {
            name: name.to_string(),
            is_function: false,
            return_type: None,
            params: Vec::new(),
            schema: None,
            body: String::new(),
            with_encryption: false,
            with_recompile: false,
            with_schemabinding: false,
        }
    }

    /// 创建函数构建器
    #[must_use]
    pub fn function(name: &str, return_type: &str) -> Self {
        Self {
            name: name.to_string(),
            is_function: true,
            return_type: Some(return_type.to_string()),
            params: Vec::new(),
            schema: None,
            body: String::new(),
            with_encryption: false,
            with_recompile: false,
            with_schemabinding: false,
        }
    }

    /// 设置 schema
    #[must_use]
    pub fn with_schema(mut self, schema: &str) -> Self {
        self.schema = Some(schema.to_string());
        self
    }

    /// 设置过程体
    #[must_use]
    pub fn with_body(mut self, body: &str) -> Self {
        self.body = body.to_string();
        self
    }

    /// 启用 WITH ENCRYPTION
    #[must_use]
    pub fn with_encryption(mut self) -> Self {
        self.with_encryption = true;
        self
    }

    /// 启用 WITH RECOMPILE
    #[must_use]
    pub fn with_recompile(mut self) -> Self {
        self.with_recompile = true;
        self
    }

    /// 启用 WITH SCHEMABINDING（仅函数）
    #[must_use]
    pub fn with_schemabinding(mut self) -> Self {
        self.with_schemabinding = true;
        self
    }

    /// 添加参数
    #[must_use]
    pub fn param(mut self, param: TSqlParameter) -> Self {
        self.params.push(param);
        self
    }

    /// 添加输入参数（便捷方法）
    #[must_use]
    pub fn input_param(mut self, name: &str, data_type: &str) -> Self {
        self.params.push(TSqlParameter::input(name, data_type));
        self
    }

    /// 添加输出参数（便捷方法）
    #[must_use]
    pub fn output_param(mut self, name: &str, data_type: &str) -> Self {
        self.params.push(TSqlParameter::output(name, data_type));
        self
    }

    /// 获取完全限定名
    #[must_use]
    pub fn full_name(&self) -> String {
        match &self.schema {
            Some(s) => format!("{s}.{}", self.name),
            None => self.name.clone(),
        }
    }

    /// 生成参数声明列表
    fn build_param_list(&self) -> String {
        self.params
            .iter()
            .map(|p| p.to_declaration())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// 生成 WITH 选项
    fn build_with_options(&self) -> String {
        let mut options = Vec::new();
        if self.with_encryption {
            options.push("ENCRYPTION".to_string());
        }
        if self.with_recompile {
            options.push("RECOMPILE".to_string());
        }
        if self.with_schemabinding && self.is_function {
            options.push("SCHEMABINDING".to_string());
        }
        if options.is_empty() {
            String::new()
        } else {
            format!(" WITH {}", options.join(", "))
        }
    }

    /// 生成 CREATE PROCEDURE/FUNCTION DDL
    #[must_use]
    pub fn build_create(&self) -> String {
        let param_list = self.build_param_list();
        let with_opts = self.build_with_options();
        if self.is_function {
            let ret = self.return_type.as_deref().unwrap_or("INT");
            format!(
                "CREATE OR ALTER FUNCTION {} ({})\nRETURNS {}\nAS\nBEGIN\n  {}\nEND{};",
                self.full_name(),
                param_list,
                ret,
                self.body,
                with_opts
            )
        } else {
            let params = if param_list.is_empty() {
                String::new()
            } else {
                format!(" {param_list}")
            };
            format!(
                "CREATE OR ALTER PROCEDURE {}{}\nAS\nBEGIN\n  {}\nEND{};",
                self.full_name(),
                params,
                self.body,
                with_opts
            )
        }
    }

    /// 生成 DROP 语句
    #[must_use]
    pub fn build_drop(&self) -> String {
        let kind = if self.is_function {
            "FUNCTION"
        } else {
            "PROCEDURE"
        };
        format!("DROP {} IF EXISTS {};", kind, self.full_name())
    }

    /// 生成 EXEC 调用语句
    #[must_use]
    pub fn build_exec(&self) -> String {
        let args: Vec<String> = self
            .params
            .iter()
            .map(|p| {
                if p.direction.is_output() {
                    format!("{} OUTPUT", p.name)
                } else {
                    p.name.clone()
                }
            })
            .collect();
        format!("EXEC {} {};", self.full_name(), args.join(", "))
    }

    /// 生成 EXEC 调用语句（带字面量值）
    #[must_use]
    pub fn build_exec_with_values(&self, values: &[&str]) -> String {
        let args: Vec<String> = self
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let val = values.get(i).copied().unwrap_or("NULL");
                if p.direction.is_output() {
                    format!("{} = {} OUTPUT", p.name, val)
                } else {
                    format!("{} = {}", p.name, val)
                }
            })
            .collect();
        format!("EXEC {} {};", self.full_name(), args.join(", "))
    }

    /// 参数数量
    #[must_use]
    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    /// 输出参数数量
    #[must_use]
    pub fn output_param_count(&self) -> usize {
        self.params
            .iter()
            .filter(|p| p.direction.is_output())
            .count()
    }

    /// 转换为参数映射
    #[must_use]
    pub fn to_param_map(&self) -> HashMap<String, TSqlParameter> {
        self.params
            .iter()
            .map(|p| (p.name.clone(), p.clone()))
            .collect()
    }
}

impl fmt::Display for TSqlStoredProcedure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.build_create())
    }
}

/// T-SQL 批量调用
#[derive(Debug, Clone)]
pub struct TSqlBatchExec {
    /// 基础构建器
    procedure: TSqlStoredProcedure,
    /// 批量参数值
    batch_values: Vec<Vec<String>>,
}

impl TSqlBatchExec {
    /// 创建批量调用
    #[must_use]
    pub fn new(procedure: TSqlStoredProcedure) -> Self {
        Self {
            procedure,
            batch_values: Vec::new(),
        }
    }

    /// 添加一行参数值
    #[must_use]
    pub fn add_row(mut self, values: Vec<String>) -> Self {
        self.batch_values.push(values);
        self
    }

    /// 生成批量 EXEC 语句
    #[must_use]
    pub fn build(&self) -> String {
        let mut calls = Vec::new();
        let full_name = self.procedure.full_name();
        let param_names: Vec<&str> = self
            .procedure
            .params
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        for row in &self.batch_values {
            let args: Vec<String> = param_names
                .iter()
                .zip(row.iter())
                .map(|(n, v)| format!("{n} = {v}"))
                .collect();
            calls.push(format!("EXEC {} {};", full_name, args.join(", ")));
        }
        calls.join("\n")
    }

    /// 行数
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.batch_values.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsql_param_direction_keyword() {
        assert_eq!(TSqlParamDirection::Input.as_keyword(), "");
        assert_eq!(TSqlParamDirection::Output.as_keyword(), "OUTPUT");
        assert_eq!(TSqlParamDirection::InputOutput.as_keyword(), "OUTPUT");
        assert_eq!(TSqlParamDirection::TableValued.as_keyword(), "READONLY");
    }

    #[test]
    fn test_tsql_param_direction_is_output() {
        assert!(!TSqlParamDirection::Input.is_output());
        assert!(TSqlParamDirection::Output.is_output());
        assert!(TSqlParamDirection::InputOutput.is_output());
        assert!(!TSqlParamDirection::TableValued.is_output());
    }

    #[test]
    fn test_tsql_parameter_input() {
        let p = TSqlParameter::input("p1", "INT");
        assert_eq!(p.name, "@p1");
        assert_eq!(p.data_type, "INT");
        assert_eq!(p.direction, TSqlParamDirection::Input);
    }

    #[test]
    fn test_tsql_parameter_with_at_prefix() {
        let p = TSqlParameter::input("@p1", "INT");
        assert_eq!(p.name, "@p1");
    }

    #[test]
    fn test_tsql_parameter_output() {
        let p = TSqlParameter::output("p1", "VARCHAR(100)");
        assert!(p.direction.is_output());
    }

    #[test]
    fn test_tsql_parameter_table_valued() {
        let p = TSqlParameter::table_valued("p1", "dbo.IntTable");
        assert_eq!(p.direction, TSqlParamDirection::TableValued);
    }

    #[test]
    fn test_tsql_parameter_with_default() {
        let p = TSqlParameter::input("p1", "INT").with_default("0");
        assert_eq!(p.default_value.as_deref(), Some("0"));
    }

    #[test]
    fn test_tsql_parameter_with_max_length() {
        let p = TSqlParameter::input("p1", "VARCHAR").with_max_length(-1);
        let decl = p.to_declaration();
        assert!(decl.contains("VARCHAR(MAX)"));
    }

    #[test]
    fn test_tsql_parameter_with_precision() {
        let p = TSqlParameter::input("p1", "DECIMAL").with_precision(10, 2);
        let decl = p.to_declaration();
        assert!(decl.contains("DECIMAL(10, 2)"));
    }

    #[test]
    fn test_tsql_parameter_to_declaration_input() {
        let p = TSqlParameter::input("p1", "INT");
        assert_eq!(p.to_declaration(), "@p1 INT");
    }

    #[test]
    fn test_tsql_parameter_to_declaration_output() {
        let p = TSqlParameter::output("p1", "INT");
        assert_eq!(p.to_declaration(), "@p1 INT OUTPUT");
    }

    #[test]
    fn test_tsql_parameter_to_declaration_with_default() {
        let p = TSqlParameter::input("p1", "INT").with_default("42");
        assert_eq!(p.to_declaration(), "@p1 INT = 42");
    }

    #[test]
    fn test_stored_procedure_create_basic() {
        let sp = TSqlStoredProcedure::procedure("my_proc")
            .with_body("SELECT 1")
            .input_param("p1", "INT");
        let ddl = sp.build_create();
        assert!(ddl.contains("CREATE OR ALTER PROCEDURE"));
        assert!(ddl.contains("@p1 INT"));
        assert!(ddl.contains("SELECT 1"));
    }

    #[test]
    fn test_stored_procedure_create_with_schema() {
        let sp = TSqlStoredProcedure::procedure("my_proc")
            .with_schema("dbo")
            .with_body("SELECT 1");
        let ddl = sp.build_create();
        assert!(ddl.contains("dbo.my_proc"));
    }

    #[test]
    fn test_stored_procedure_create_function() {
        let sp = TSqlStoredProcedure::function("my_func", "INT")
            .with_body("RETURN 1")
            .input_param("x", "INT");
        let ddl = sp.build_create();
        assert!(ddl.contains("CREATE OR ALTER FUNCTION"));
        assert!(ddl.contains("RETURNS INT"));
    }

    #[test]
    fn test_stored_procedure_with_encryption() {
        let sp = TSqlStoredProcedure::procedure("my_proc")
            .with_body("SELECT 1")
            .with_encryption();
        let ddl = sp.build_create();
        assert!(ddl.contains("WITH ENCRYPTION"));
    }

    #[test]
    fn test_stored_procedure_with_recompile() {
        let sp = TSqlStoredProcedure::procedure("my_proc")
            .with_body("SELECT 1")
            .with_recompile();
        let ddl = sp.build_create();
        assert!(ddl.contains("WITH RECOMPILE"));
    }

    #[test]
    fn test_stored_procedure_with_schemabinding() {
        let sp = TSqlStoredProcedure::function("my_func", "INT")
            .with_body("RETURN 1")
            .with_schemabinding();
        let ddl = sp.build_create();
        assert!(ddl.contains("SCHEMABINDING"));
    }

    #[test]
    fn test_stored_procedure_drop() {
        let sp = TSqlStoredProcedure::procedure("my_proc");
        assert_eq!(sp.build_drop(), "DROP PROCEDURE IF EXISTS my_proc;");
    }

    #[test]
    fn test_stored_procedure_drop_function() {
        let sp = TSqlStoredProcedure::function("my_func", "INT");
        assert_eq!(sp.build_drop(), "DROP FUNCTION IF EXISTS my_func;");
    }

    #[test]
    fn test_stored_procedure_exec() {
        let sp = TSqlStoredProcedure::procedure("my_proc")
            .input_param("p1", "INT")
            .output_param("p2", "INT");
        let exec = sp.build_exec();
        assert!(exec.contains("EXEC my_proc"));
        assert!(exec.contains("@p1"));
        assert!(exec.contains("@p2 OUTPUT"));
    }

    #[test]
    fn test_stored_procedure_exec_with_values() {
        let sp = TSqlStoredProcedure::procedure("my_proc")
            .input_param("p1", "INT")
            .input_param("p2", "VARCHAR(50)");
        let exec = sp.build_exec_with_values(&["42", "'hello'"]);
        assert!(exec.contains("@p1 = 42"));
        assert!(exec.contains("@p2 = 'hello'"));
    }

    #[test]
    fn test_stored_procedure_param_count() {
        let sp = TSqlStoredProcedure::procedure("my_proc")
            .input_param("p1", "INT")
            .output_param("p2", "INT");
        assert_eq!(sp.param_count(), 2);
        assert_eq!(sp.output_param_count(), 1);
    }

    #[test]
    fn test_stored_procedure_to_param_map() {
        let sp = TSqlStoredProcedure::procedure("my_proc")
            .input_param("p1", "INT")
            .output_param("p2", "INT");
        let map = sp.to_param_map();
        assert!(map.contains_key("@p1"));
        assert!(map.contains_key("@p2"));
    }

    #[test]
    fn test_stored_procedure_display() {
        let sp = TSqlStoredProcedure::procedure("my_proc").with_body("SELECT 1");
        let s = format!("{}", sp);
        assert!(s.contains("CREATE OR ALTER PROCEDURE"));
    }

    #[test]
    fn test_stored_procedure_no_params() {
        let sp = TSqlStoredProcedure::procedure("my_proc").with_body("SELECT 1");
        let ddl = sp.build_create();
        assert!(ddl.contains("my_proc\nAS"));
    }

    #[test]
    fn test_batch_exec() {
        let sp = TSqlStoredProcedure::procedure("my_proc").input_param("p1", "INT");
        let batch = TSqlBatchExec::new(sp)
            .add_row(vec!["1".to_string()])
            .add_row(vec!["2".to_string()]);
        let sql = batch.build();
        assert!(sql.contains("EXEC my_proc @p1 = 1"));
        assert!(sql.contains("EXEC my_proc @p1 = 2"));
        assert_eq!(batch.row_count(), 2);
    }

    #[test]
    fn test_batch_exec_empty() {
        let sp = TSqlStoredProcedure::procedure("my_proc").input_param("p1", "INT");
        let batch = TSqlBatchExec::new(sp);
        let sql = batch.build();
        assert!(sql.is_empty());
        assert_eq!(batch.row_count(), 0);
    }

    #[test]
    fn test_full_name_no_schema() {
        let sp = TSqlStoredProcedure::procedure("my_proc");
        assert_eq!(sp.full_name(), "my_proc");
    }

    #[test]
    fn test_full_name_with_schema() {
        let sp = TSqlStoredProcedure::procedure("my_proc").with_schema("app");
        assert_eq!(sp.full_name(), "app.my_proc");
    }
}
