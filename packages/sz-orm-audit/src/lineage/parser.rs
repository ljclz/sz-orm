//! SQL 依赖解析器：使用 sqlparser 解析 SQL 提取表/字段依赖关系。

use sqlparser::ast::{
    Assignment, Expr, FunctionArg, FunctionArgExpr, FunctionArguments, Ident, ObjectName, Query,
    SelectItem, SetExpr, Statement, TableFactor, TableWithJoins,
};
use sqlparser::dialect::{
    AnsiDialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::Parser;
use std::collections::HashMap;

use super::graph::{EdgeType, LineageEdge, LineageError, LineageNodeId};

/// lineage 解析方言
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineageDialect {
    MySQL,
    PostgreSQL,
    SQLite,
    Ansi,
    Generic,
}

impl LineageDialect {
    fn to_sqlparser_dialect(self) -> Box<dyn sqlparser::dialect::Dialect> {
        match self {
            LineageDialect::MySQL => Box::new(MySqlDialect {}),
            LineageDialect::PostgreSQL => Box::new(PostgreSqlDialect {}),
            LineageDialect::SQLite => Box::new(SQLiteDialect {}),
            LineageDialect::Ansi => Box::new(AnsiDialect {}),
            LineageDialect::Generic => Box::new(GenericDialect {}),
        }
    }
}

/// SQL 依赖解析器
pub struct LineageSqlParser {
    dialect: Box<dyn sqlparser::dialect::Dialect>,
}

impl LineageSqlParser {
    pub fn new(dialect: LineageDialect) -> Self {
        Self {
            dialect: dialect.to_sqlparser_dialect(),
        }
    }

    /// 解析 SQL，提取表/字段依赖边
    pub fn parse(&self, sql: &str) -> Result<Vec<LineageEdge>, LineageError> {
        let statements = Parser::parse_sql(self.dialect.as_ref(), sql)
            .map_err(|e| LineageError::ParseFailed(e.to_string()))?;

        let mut edges = Vec::new();
        for stmt in statements {
            self.extract_from_statement(&stmt, &mut edges);
        }
        Ok(edges)
    }

    fn extract_from_statement(&self, stmt: &Statement, edges: &mut Vec<LineageEdge>) {
        match stmt {
            Statement::Insert(insert) => self.extract_insert(insert, edges),
            Statement::Update {
                table,
                assignments,
                from,
                ..
            } => self.extract_update(table, assignments, from.as_ref(), edges),
            Statement::CreateView {
                name,
                query,
                materialized,
                ..
            } => self.extract_create_view(name, query, *materialized, edges),
            Statement::Query(q) => self.extract_join_edges_from_query(q, edges),
            _ => {}
        }
    }

    /// INSERT INTO target (cols) SELECT ... FROM sources
    fn extract_insert(&self, insert: &sqlparser::ast::Insert, edges: &mut Vec<LineageEdge>) {
        let target_table = object_name_to_string(&insert.table_name);
        if target_table.is_empty() {
            return;
        }

        let source = match &insert.source {
            Some(q) => q,
            None => return,
        };

        let (source_tables, alias_map) = self.extract_source_tables_from_query(source);
        if source_tables.is_empty() {
            return;
        }

        let projection = match &*source.body {
            SetExpr::Select(s) => &s.projection,
            _ => return,
        };

        let target_columns: Vec<String> = if !insert.columns.is_empty() {
            insert.columns.iter().map(|c| c.value.clone()).collect()
        } else {
            projection
                .iter()
                .map(|item| select_item_alias(item).unwrap_or_else(|| "unknown".to_string()))
                .collect()
        };

        for (i, item) in projection.iter().enumerate() {
            let target_col = target_columns.get(i).cloned().unwrap_or_else(|| {
                select_item_alias(item).unwrap_or_else(|| "unknown".to_string())
            });

            let source_refs = extract_column_refs(item);
            for source_ref in source_refs {
                let (src_table, src_col) = resolve_ref(&source_ref, &source_tables, &alias_map);
                if !src_table.is_empty() {
                    edges.push(LineageEdge::new(
                        LineageNodeId::new(src_table, src_col),
                        LineageNodeId::new(&target_table, &target_col),
                        EdgeType::Derived,
                    ));
                }
            }
        }

        self.extract_join_edges_from_query(source, edges);
    }

    /// UPDATE target SET col = expr FROM sources
    fn extract_update(
        &self,
        table: &TableWithJoins,
        assignments: &[Assignment],
        from: Option<&TableWithJoins>,
        edges: &mut Vec<LineageEdge>,
    ) {
        let target_table = table_factor_to_string(&table.relation);
        if target_table.is_empty() {
            return;
        }

        let mut source_tables = Vec::new();
        let mut alias_map = HashMap::new();
        if let Some(f) = from {
            collect_table_with_alias(&f.relation, &mut source_tables, &mut alias_map);
            for join in &f.joins {
                collect_table_with_alias(&join.relation, &mut source_tables, &mut alias_map);
            }
        }

        for assignment in assignments {
            let target_col = idents_to_string(&assignment.id);
            let refs = extract_expr_refs(&assignment.value);
            for ref_name in refs {
                let (src_table, src_col) = resolve_ref(&ref_name, &source_tables, &alias_map);
                if !src_table.is_empty() {
                    edges.push(LineageEdge::new(
                        LineageNodeId::new(src_table, src_col),
                        LineageNodeId::new(&target_table, &target_col),
                        EdgeType::Derived,
                    ));
                }
            }
        }
    }

    /// CREATE [MATERIALIZED] VIEW name AS SELECT ... FROM sources
    fn extract_create_view(
        &self,
        name: &ObjectName,
        query: &Query,
        materialized: bool,
        edges: &mut Vec<LineageEdge>,
    ) {
        let view_name = object_name_to_string(name);
        if view_name.is_empty() {
            return;
        }

        let (source_tables, alias_map) = self.extract_source_tables_from_query(query);
        if source_tables.is_empty() {
            return;
        }

        let projection = match &*query.body {
            SetExpr::Select(s) => &s.projection,
            _ => return,
        };

        for item in projection {
            let target_col = select_item_alias(item).unwrap_or_else(|| {
                extract_column_refs(item)
                    .first()
                    .map(|r| r.split('.').next_back().unwrap_or(r).to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            });

            let source_refs = extract_column_refs(item);
            for source_ref in source_refs {
                let (src_table, src_col) = resolve_ref(&source_ref, &source_tables, &alias_map);
                if !src_table.is_empty() {
                    edges.push(LineageEdge::new(
                        LineageNodeId::new(src_table, src_col),
                        LineageNodeId::new(&view_name, &target_col),
                        if materialized {
                            EdgeType::DirectDependency
                        } else {
                            EdgeType::Derived
                        },
                    ));
                }
            }
        }

        self.extract_join_edges_from_query(query, edges);
    }

    /// 从 Query 提取所有源表名和别名映射
    fn extract_source_tables_from_query(
        &self,
        query: &Query,
    ) -> (Vec<String>, HashMap<String, String>) {
        let mut tables = Vec::new();
        let mut alias_map = HashMap::new();
        if let SetExpr::Select(select) = &*query.body {
            for twj in &select.from {
                collect_table_with_alias(&twj.relation, &mut tables, &mut alias_map);
                for join in &twj.joins {
                    collect_table_with_alias(&join.relation, &mut tables, &mut alias_map);
                }
            }
        }
        (tables, alias_map)
    }

    /// 从 Query 提取 JOIN 依赖边
    fn extract_join_edges_from_query(&self, query: &Query, edges: &mut Vec<LineageEdge>) {
        if let SetExpr::Select(select) = &*query.body {
            for twj in &select.from {
                for join in &twj.joins {
                    let left_table = table_factor_to_string(&twj.relation);
                    let right_table = table_factor_to_string(&join.relation);
                    if !left_table.is_empty() && !right_table.is_empty() {
                        edges.push(LineageEdge::new(
                            LineageNodeId::new(&right_table, "id"),
                            LineageNodeId::new(&left_table, "id"),
                            EdgeType::Join,
                        ));
                    }
                }
            }
        }
    }
}

fn object_name_to_string(name: &ObjectName) -> String {
    name.0.last().map(|i| i.value.clone()).unwrap_or_default()
}

fn table_factor_to_string(factor: &TableFactor) -> String {
    match factor {
        TableFactor::Table { name, .. } => object_name_to_string(name),
        _ => String::new(),
    }
}

fn idents_to_string(idents: &[Ident]) -> String {
    idents.last().map(|i| i.value.clone()).unwrap_or_default()
}

/// 从 TableFactor 提取表名和别名，加入 source_tables 和 alias_map
fn collect_table_with_alias(
    factor: &TableFactor,
    tables: &mut Vec<String>,
    alias_map: &mut HashMap<String, String>,
) {
    if let TableFactor::Table { name, alias, .. } = factor {
        let table_name = object_name_to_string(name);
        if !table_name.is_empty() {
            tables.push(table_name.clone());
            alias_map.insert(table_name.clone(), table_name.clone());
            if let Some(a) = alias {
                alias_map.insert(a.name.value.clone(), table_name);
            }
        }
    }
}

/// 从 SelectItem 提取别名（或表达式名）
fn select_item_alias(item: &SelectItem) -> Option<String> {
    match item {
        SelectItem::ExprWithAlias { alias, .. } => Some(alias.value.clone()),
        SelectItem::UnnamedExpr(Expr::Identifier(ident)) => Some(ident.value.clone()),
        SelectItem::UnnamedExpr(Expr::CompoundIdentifier(idents)) => {
            idents.last().map(|i| i.value.clone())
        }
        _ => None,
    }
}

/// 从 SelectItem 提取列引用（table.column 或 column）
fn extract_column_refs(item: &SelectItem) -> Vec<String> {
    match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
            extract_expr_refs(expr)
        }
        _ => Vec::new(),
    }
}

/// 从 Expr 提取所有字段引用
fn extract_expr_refs(expr: &Expr) -> Vec<String> {
    let mut refs = Vec::new();
    collect_refs(expr, &mut refs);
    refs
}

fn collect_refs(expr: &Expr, refs: &mut Vec<String>) {
    match expr {
        Expr::Identifier(ident) => {
            refs.push(ident.value.clone());
        }
        Expr::CompoundIdentifier(idents) => {
            let parts: Vec<String> = idents.iter().map(|i| i.value.clone()).collect();
            refs.push(parts.join("."));
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_refs(left, refs);
            collect_refs(right, refs);
        }
        Expr::Nested(expr) => collect_refs(expr, refs),
        Expr::UnaryOp { expr, .. } => collect_refs(expr, refs),
        Expr::Function(func) => {
            if let FunctionArguments::List(list) = &func.args {
                for arg in &list.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => collect_refs(e, refs),
                        FunctionArg::Named {
                            arg: FunctionArgExpr::Expr(e),
                            ..
                        } => collect_refs(e, refs),
                        _ => {}
                    }
                }
            }
        }
        Expr::Cast { expr, .. } => collect_refs(expr, refs),
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
        } => {
            if let Some(op) = operand {
                collect_refs(op, refs);
            }
            for cond in conditions {
                collect_refs(cond, refs);
            }
            for res in results {
                collect_refs(res, refs);
            }
            if let Some(er) = else_result {
                collect_refs(er, refs);
            }
        }
        _ => {}
    }
}

/// 解析字段引用为 (table, column)，使用别名映射
fn resolve_ref(
    ref_name: &str,
    source_tables: &[String],
    alias_map: &HashMap<String, String>,
) -> (String, String) {
    let parts: Vec<&str> = ref_name.split('.').collect();
    if parts.len() >= 2 {
        let table_or_alias = parts[parts.len() - 2].to_string();
        let column = parts[parts.len() - 1].to_string();
        let real_table = alias_map
            .get(&table_or_alias)
            .cloned()
            .unwrap_or(table_or_alias);
        (real_table, column)
    } else if !source_tables.is_empty() {
        (source_tables[0].clone(), ref_name.to_string())
    } else {
        (String::new(), ref_name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(sql: &str) -> Vec<LineageEdge> {
        let parser = LineageSqlParser::new(LineageDialect::PostgreSQL);
        parser.parse(sql).unwrap_or_default()
    }

    #[test]
    fn test_insert_select_with_columns() {
        let sql = "INSERT INTO report (name, amount) SELECT users.name, orders.amount FROM users JOIN orders ON users.id = orders.user_id";
        let edges = parse(sql);

        let has_report_name = edges.iter().any(|e| {
            e.source == LineageNodeId::new("users", "name")
                && e.target == LineageNodeId::new("report", "name")
        });
        assert!(
            has_report_name,
            "should have edge users.name -> report.name"
        );

        let has_report_amount = edges.iter().any(|e| {
            e.source == LineageNodeId::new("orders", "amount")
                && e.target == LineageNodeId::new("report", "amount")
        });
        assert!(
            has_report_amount,
            "should have edge orders.amount -> report.amount"
        );
    }

    #[test]
    fn test_create_view() {
        let sql = "CREATE VIEW v AS SELECT a, b FROM t";
        let edges = parse(sql);

        let has_v_a = edges.iter().any(|e| {
            e.source == LineageNodeId::new("t", "a") && e.target == LineageNodeId::new("v", "a")
        });
        assert!(has_v_a, "should have edge t.a -> v.a");

        let has_v_b = edges.iter().any(|e| {
            e.source == LineageNodeId::new("t", "b") && e.target == LineageNodeId::new("v", "b")
        });
        assert!(has_v_b, "should have edge t.b -> v.b");
    }

    #[test]
    fn test_create_materialized_view() {
        let sql = "CREATE MATERIALIZED VIEW mv AS SELECT a, b FROM t";
        let edges = parse(sql);

        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("t", "a")
                && e.target == LineageNodeId::new("mv", "a")
                && e.edge_type == EdgeType::DirectDependency
        }));
        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("t", "b")
                && e.target == LineageNodeId::new("mv", "b")
                && e.edge_type == EdgeType::DirectDependency
        }));
    }

    #[test]
    fn test_update_with_from() {
        let sql = "UPDATE report SET name = users.name FROM users";
        let edges = parse(sql);

        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("users", "name")
                && e.target == LineageNodeId::new("report", "name")
        }));
    }

    #[test]
    fn test_join_dependency() {
        let sql = "SELECT * FROM a JOIN b ON a.id = b.aid";
        let edges = parse(sql);

        assert!(edges.iter().any(|e| {
            e.edge_type == EdgeType::Join
                && e.source == LineageNodeId::new("b", "id")
                && e.target == LineageNodeId::new("a", "id")
        }));
    }

    #[test]
    fn test_parse_error_returns_err() {
        let parser = LineageSqlParser::new(LineageDialect::PostgreSQL);
        let result = parser.parse("THIS IS NOT VALID SQL !!!");
        assert!(result.is_err());
        match result.unwrap_err() {
            LineageError::ParseFailed(_) => {}
            other => panic!("expected ParseFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_insert_without_column_list() {
        let sql = "INSERT INTO report SELECT users.name FROM users";
        let edges = parse(sql);

        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("users", "name")
                && e.target == LineageNodeId::new("report", "name")
        }));
    }

    #[test]
    fn test_create_view_with_alias() {
        let sql = "CREATE VIEW v AS SELECT t.a AS x, t.b AS y FROM t";
        let edges = parse(sql);

        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("t", "a") && e.target == LineageNodeId::new("v", "x")
        }));
        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("t", "b") && e.target == LineageNodeId::new("v", "y")
        }));
    }

    #[test]
    fn test_multiple_source_tables() {
        let sql = "CREATE VIEW v AS SELECT a.x, b.y FROM a JOIN b ON a.id = b.aid";
        let edges = parse(sql);

        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("a", "x") && e.target == LineageNodeId::new("v", "x")
        }));
        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("b", "y") && e.target == LineageNodeId::new("v", "y")
        }));
    }

    #[test]
    fn test_unsupported_statement_returns_empty() {
        let sql = "DROP TABLE IF EXISTS foo";
        let edges = parse(sql);
        assert!(edges.is_empty());
    }

    #[test]
    fn test_mysql_dialect() {
        let parser = LineageSqlParser::new(LineageDialect::MySQL);
        let sql = "CREATE VIEW v AS SELECT a, b FROM t";
        let edges = parser.parse(sql).unwrap_or_default();
        assert!(!edges.is_empty());
    }

    #[test]
    fn test_sqlite_dialect() {
        let parser = LineageSqlParser::new(LineageDialect::SQLite);
        let sql = "CREATE VIEW v AS SELECT a, b FROM t";
        let edges = parser.parse(sql).unwrap_or_default();
        assert!(!edges.is_empty());
    }

    #[test]
    fn test_insert_with_join_sources() {
        let sql = "INSERT INTO report (name, amount) SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = o.user_id";
        let edges = parse(sql);

        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("users", "name")
                && e.target == LineageNodeId::new("report", "name")
        }));
    }

    #[test]
    fn test_update_multiple_assignments() {
        let sql = "UPDATE report SET name = users.name, email = users.email FROM users";
        let edges = parse(sql);

        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("users", "name")
                && e.target == LineageNodeId::new("report", "name")
        }));
        assert!(edges.iter().any(|e| {
            e.source == LineageNodeId::new("users", "email")
                && e.target == LineageNodeId::new("report", "email")
        }));
    }
}
