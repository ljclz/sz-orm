//! 分页提取与辅助。
//!
//! - [`PaginationExtractor`] — 从查询参数提取分页
//! - [`Pagination`] — 分页参数

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// Pagination — 分页参数
// ============================================================================

/// 分页参数辅助
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagination {
    page: u64,
    per_page: u64,
}

impl Pagination {
    /// 创建分页（page 从 1 开始）
    pub fn new(page: u64, per_page: u64) -> Self {
        Self {
            page: page.max(1),
            per_page: per_page.max(1),
        }
    }

    /// 当前页码
    pub fn page(&self) -> u64 {
        self.page
    }

    /// 每页行数
    pub fn per_page(&self) -> u64 {
        self.per_page
    }

    /// SQL OFFSET
    pub fn offset(&self) -> u64 {
        (self.page - 1) * self.per_page
    }

    /// SQL LIMIT
    pub fn limit(&self) -> u64 {
        self.per_page
    }

    /// 总页数
    pub fn total_pages(&self, total: u64) -> u64 {
        if total == 0 {
            0
        } else {
            total.div_ceil(self.per_page)
        }
    }

    /// 是否有下一页
    pub fn has_next(&self, total: u64) -> bool {
        self.page < self.total_pages(total)
    }

    /// 是否有上一页
    pub fn has_prev(&self) -> bool {
        self.page > 1
    }
}

// ============================================================================
// PaginationExtractor — 从查询参数提取分页
// ============================================================================

/// 分页提取器
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaginationExtractor {
    page: u64,
    per_page: u64,
    max_per_page: u64,
}

impl Default for PaginationExtractor {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
            max_per_page: 100,
        }
    }
}

impl PaginationExtractor {
    /// 创建分页提取器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置默认每页条数（链式）
    pub fn with_default_per_page(mut self, n: u64) -> Self {
        self.per_page = n.max(1);
        self
    }

    /// 设置最大每页条数（链式）
    pub fn with_max_per_page(mut self, n: u64) -> Self {
        self.max_per_page = n.max(1);
        self
    }

    /// 默认每页条数
    pub fn default_per_page(&self) -> u64 {
        self.per_page
    }

    /// 最大每页条数
    pub fn max_per_page(&self) -> u64 {
        self.max_per_page
    }

    /// 从 HashMap 提取分页
    pub fn extract(&self, params: &HashMap<String, String>) -> Pagination {
        let page = params
            .get("page")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1)
            .max(1);
        let per_page = params
            .get("per_page")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(self.per_page)
            .clamp(1, self.max_per_page);
        Pagination::new(page, per_page)
    }

    /// 从键值对列表提取分页
    pub fn extract_pairs(&self, pairs: &[(&str, &str)]) -> Pagination {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.extract(&map)
    }

    /// 从可选参数提取分页
    pub fn extract_raw(&self, page: Option<&str>, per_page: Option<&str>) -> Pagination {
        let p = page.and_then(|s| s.parse::<u64>().ok()).unwrap_or(1).max(1);
        let pp = per_page
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(self.per_page)
            .clamp(1, self.max_per_page);
        Pagination::new(p, pp)
    }
}

// ============================================================================
// SortParams — 排序参数
// ============================================================================

/// 排序方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl Default for SortDirection {
    fn default() -> Self {
        Self::Asc
    }
}

impl SortDirection {
    /// 转为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            SortDirection::Asc => "asc",
            SortDirection::Desc => "desc",
        }
    }

    /// SQL 关键字
    pub fn sql_keyword(&self) -> &'static str {
        match self {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        }
    }

    /// 从字符串解析
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "desc" => SortDirection::Desc,
            _ => SortDirection::Asc,
        }
    }
}

/// 排序参数
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortParams {
    field: String,
    direction: SortDirection,
}

impl SortParams {
    /// 创建排序参数
    pub fn new(field: &str, direction: SortDirection) -> Self {
        Self {
            field: field.to_string(),
            direction,
        }
    }

    /// 从字符串解析（如 "name" 或 "-name" 表示降序）
    pub fn parse(s: &str) -> Self {
        if let Some(rest) = s.strip_prefix('-') {
            Self::new(rest, SortDirection::Desc)
        } else {
            Self::new(s, SortDirection::Asc)
        }
    }

    /// 排序字段
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 排序方向
    pub fn direction(&self) -> SortDirection {
        self.direction
    }

    /// SQL ORDER BY 片段
    pub fn to_sql(&self) -> String {
        format!("{} {}", self.field, self.direction.sql_keyword())
    }
}

// ============================================================================
// FilterParams — 过滤参数
// ============================================================================

/// 过滤操作符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    In,
}

impl FilterOp {
    /// SQL 操作符
    pub fn sql_op(&self) -> &'static str {
        match self {
            FilterOp::Eq => "=",
            FilterOp::Ne => "!=",
            FilterOp::Gt => ">",
            FilterOp::Gte => ">=",
            FilterOp::Lt => "<",
            FilterOp::Lte => "<=",
            FilterOp::Like => "LIKE",
            FilterOp::In => "IN",
        }
    }

    /// 从字符串解析
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "eq" => Some(FilterOp::Eq),
            "ne" => Some(FilterOp::Ne),
            "gt" => Some(FilterOp::Gt),
            "gte" => Some(FilterOp::Gte),
            "lt" => Some(FilterOp::Lt),
            "lte" => Some(FilterOp::Lte),
            "like" => Some(FilterOp::Like),
            "in" => Some(FilterOp::In),
            _ => None,
        }
    }
}

/// 过滤条件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterCondition {
    field: String,
    op: FilterOp,
    value: String,
}

impl FilterCondition {
    /// 创建过滤条件
    pub fn new(field: &str, op: FilterOp, value: &str) -> Self {
        Self {
            field: field.to_string(),
            op,
            value: value.to_string(),
        }
    }

    /// 字段
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 操作符
    pub fn op(&self) -> &FilterOp {
        &self.op
    }

    /// 值
    pub fn value(&self) -> &str {
        &self.value
    }

    /// 转为 WHERE 片段（参数化占位符 `?`）
    pub fn to_where_clause(&self) -> String {
        if self.op == FilterOp::In {
            format!("{} IN (?)", self.field)
        } else {
            format!("{} {} ?", self.field, self.op.sql_op())
        }
    }
}

/// 过滤参数集合
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilterParams {
    conditions: Vec<FilterCondition>,
}

impl FilterParams {
    /// 创建空过滤参数
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加条件（链式）
    pub fn add(mut self, field: &str, op: FilterOp, value: &str) -> Self {
        self.conditions.push(FilterCondition::new(field, op, value));
        self
    }

    /// 条件数
    pub fn count(&self) -> usize {
        self.conditions.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }

    /// 条件列表
    pub fn conditions(&self) -> &[FilterCondition] {
        &self.conditions
    }

    /// 生成 WHERE 子句（AND 连接）
    pub fn to_where_clause(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            let clauses: Vec<String> = self
                .conditions
                .iter()
                .map(|c| c.to_where_clause())
                .collect();
            format!("WHERE {}", clauses.join(" AND "))
        }
    }

    /// 生成参数值列表
    pub fn to_values(&self) -> Vec<&str> {
        self.conditions.iter().map(|c| c.value()).collect()
    }
}

// ============================================================================
// QueryParams — 综合查询参数
// ============================================================================

/// 综合查询参数：分页 + 排序 + 过滤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParams {
    pagination: Pagination,
    sort: Option<SortParams>,
    filters: FilterParams,
}

impl QueryParams {
    /// 创建查询参数
    pub fn new(pagination: Pagination) -> Self {
        Self {
            pagination,
            sort: None,
            filters: FilterParams::new(),
        }
    }

    /// 设置排序（链式）
    pub fn sort(mut self, sort: SortParams) -> Self {
        self.sort = Some(sort);
        self
    }

    /// 添加过滤条件（链式）
    pub fn filter(mut self, field: &str, op: FilterOp, value: &str) -> Self {
        self.filters = self.filters.add(field, op, value);
        self
    }

    /// 分页
    pub fn pagination(&self) -> &Pagination {
        &self.pagination
    }

    /// 排序
    pub fn sort_value(&self) -> Option<&SortParams> {
        self.sort.as_ref()
    }

    /// 过滤
    pub fn filters(&self) -> &FilterParams {
        &self.filters
    }

    /// 生成 SQL 片段（WHERE + ORDER BY + LIMIT/OFFSET）
    pub fn to_sql(&self) -> String {
        let mut sql = String::new();

        if !self.filters.is_empty() {
            sql.push_str(&self.filters.to_where_clause());
        }

        if let Some(sort) = &self.sort {
            if !sql.is_empty() {
                sql.push(' ');
            }
            sql.push_str(&format!("ORDER BY {}", sort.to_sql()));
        }

        if !sql.is_empty() {
            sql.push(' ');
        }
        sql.push_str(&format!(
            "LIMIT {} OFFSET {}",
            self.pagination.limit(),
            self.pagination.offset()
        ));

        sql
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- Pagination -----

    #[test]
    fn pagination_new() {
        let p = Pagination::new(1, 20);
        assert_eq!(p.page(), 1);
        assert_eq!(p.per_page(), 20);
    }

    #[test]
    fn pagination_zero_clamped() {
        let p = Pagination::new(0, 0);
        assert_eq!(p.page(), 1);
        assert_eq!(p.per_page(), 1);
    }

    #[test]
    fn pagination_offset() {
        let p = Pagination::new(3, 20);
        assert_eq!(p.offset(), 40);
    }

    #[test]
    fn pagination_total_pages() {
        let p = Pagination::new(1, 10);
        assert_eq!(p.total_pages(100), 10);
        assert_eq!(p.total_pages(105), 11);
        assert_eq!(p.total_pages(0), 0);
    }

    #[test]
    fn pagination_has_next() {
        let p = Pagination::new(1, 10);
        assert!(p.has_next(100));
        assert!(!p.has_next(10));
    }

    #[test]
    fn pagination_has_prev() {
        assert!(!Pagination::new(1, 10).has_prev());
        assert!(Pagination::new(2, 10).has_prev());
    }

    // ----- PaginationExtractor -----

    #[test]
    fn extractor_default() {
        let e = PaginationExtractor::new();
        assert_eq!(e.default_per_page(), 20);
        assert_eq!(e.max_per_page(), 100);
    }

    #[test]
    fn extract_from_map() {
        let e = PaginationExtractor::new();
        let mut params = HashMap::new();
        params.insert("page".to_string(), "3".to_string());
        params.insert("per_page".to_string(), "50".to_string());
        let p = e.extract(&params);
        assert_eq!(p.page(), 3);
        assert_eq!(p.per_page(), 50);
    }

    #[test]
    fn extract_defaults() {
        let e = PaginationExtractor::new();
        let p = e.extract(&HashMap::new());
        assert_eq!(p.page(), 1);
        assert_eq!(p.per_page(), 20);
    }

    #[test]
    fn extract_clamp_max() {
        let e = PaginationExtractor::new().with_max_per_page(50);
        let mut params = HashMap::new();
        params.insert("per_page".to_string(), "1000".to_string());
        let p = e.extract(&params);
        assert_eq!(p.per_page(), 50);
    }

    #[test]
    fn extract_invalid() {
        let e = PaginationExtractor::new();
        let mut params = HashMap::new();
        params.insert("page".to_string(), "abc".to_string());
        let p = e.extract(&params);
        assert_eq!(p.page(), 1);
    }

    #[test]
    fn extract_pairs() {
        let e = PaginationExtractor::new();
        let p = e.extract_pairs(&[("page", "2"), ("per_page", "30")]);
        assert_eq!(p.page(), 2);
        assert_eq!(p.per_page(), 30);
    }

    #[test]
    fn extract_raw() {
        let e = PaginationExtractor::new();
        let p = e.extract_raw(Some("5"), Some("15"));
        assert_eq!(p.page(), 5);
        assert_eq!(p.per_page(), 15);
    }

    #[test]
    fn extract_raw_none() {
        let e = PaginationExtractor::new();
        let p = e.extract_raw(None, None);
        assert_eq!(p.page(), 1);
        assert_eq!(p.per_page(), 20);
    }

    // ----- SortDirection -----

    #[test]
    fn sort_direction_as_str() {
        assert_eq!(SortDirection::Asc.as_str(), "asc");
        assert_eq!(SortDirection::Desc.as_str(), "desc");
    }

    #[test]
    fn sort_direction_sql() {
        assert_eq!(SortDirection::Asc.sql_keyword(), "ASC");
        assert_eq!(SortDirection::Desc.sql_keyword(), "DESC");
    }

    #[test]
    fn sort_direction_parse() {
        assert_eq!(SortDirection::parse("asc"), SortDirection::Asc);
        assert_eq!(SortDirection::parse("desc"), SortDirection::Desc);
        assert_eq!(SortDirection::parse("invalid"), SortDirection::Asc);
    }

    // ----- SortParams -----

    #[test]
    fn sort_params_new() {
        let s = SortParams::new("name", SortDirection::Asc);
        assert_eq!(s.field(), "name");
        assert_eq!(s.direction(), SortDirection::Asc);
    }

    #[test]
    fn sort_params_parse_asc() {
        let s = SortParams::parse("name");
        assert_eq!(s.field(), "name");
        assert_eq!(s.direction(), SortDirection::Asc);
    }

    #[test]
    fn sort_params_parse_desc() {
        let s = SortParams::parse("-name");
        assert_eq!(s.field(), "name");
        assert_eq!(s.direction(), SortDirection::Desc);
    }

    #[test]
    fn sort_params_to_sql() {
        let s = SortParams::new("name", SortDirection::Desc);
        assert_eq!(s.to_sql(), "name DESC");
    }

    // ----- FilterOp -----

    #[test]
    fn filter_op_sql() {
        assert_eq!(FilterOp::Eq.sql_op(), "=");
        assert_eq!(FilterOp::Ne.sql_op(), "!=");
        assert_eq!(FilterOp::Gt.sql_op(), ">");
        assert_eq!(FilterOp::Like.sql_op(), "LIKE");
        assert_eq!(FilterOp::In.sql_op(), "IN");
    }

    #[test]
    fn filter_op_parse() {
        assert_eq!(FilterOp::parse("eq"), Some(FilterOp::Eq));
        assert_eq!(FilterOp::parse("invalid"), None);
    }

    // ----- FilterCondition -----

    #[test]
    fn filter_condition_new() {
        let c = FilterCondition::new("name", FilterOp::Eq, "Alice");
        assert_eq!(c.field(), "name");
        assert_eq!(c.value(), "Alice");
    }

    #[test]
    fn filter_condition_to_where() {
        let c = FilterCondition::new("name", FilterOp::Eq, "Alice");
        assert_eq!(c.to_where_clause(), "name = ?");
    }

    #[test]
    fn filter_condition_to_where_in() {
        let c = FilterCondition::new("id", FilterOp::In, "1,2,3");
        assert_eq!(c.to_where_clause(), "id IN (?)");
    }

    // ----- FilterParams -----

    #[test]
    fn filter_params_empty() {
        let f = FilterParams::new();
        assert!(f.is_empty());
        assert_eq!(f.to_where_clause(), "");
    }

    #[test]
    fn filter_params_add() {
        let f =
            FilterParams::new()
                .add("name", FilterOp::Eq, "Alice")
                .add("age", FilterOp::Gte, "18");
        assert_eq!(f.count(), 2);
        assert!(!f.is_empty());
    }

    #[test]
    fn filter_params_to_where() {
        let f =
            FilterParams::new()
                .add("name", FilterOp::Eq, "Alice")
                .add("age", FilterOp::Gte, "18");
        let where_clause = f.to_where_clause();
        assert!(where_clause.starts_with("WHERE "));
        assert!(where_clause.contains("AND"));
    }

    #[test]
    fn filter_params_to_values() {
        let f =
            FilterParams::new()
                .add("name", FilterOp::Eq, "Alice")
                .add("age", FilterOp::Gte, "18");
        let values = f.to_values();
        assert_eq!(values, vec!["Alice", "18"]);
    }

    // ----- QueryParams -----

    #[test]
    fn query_params_new() {
        let q = QueryParams::new(Pagination::new(1, 10));
        assert_eq!(q.pagination().page(), 1);
        assert!(q.sort_value().is_none());
        assert!(q.filters().is_empty());
    }

    #[test]
    fn query_params_with_sort() {
        let q = QueryParams::new(Pagination::new(1, 10))
            .sort(SortParams::new("name", SortDirection::Asc));
        assert!(q.sort_value().is_some());
    }

    #[test]
    fn query_params_with_filter() {
        let q = QueryParams::new(Pagination::new(1, 10)).filter("name", FilterOp::Eq, "Alice");
        assert!(!q.filters().is_empty());
    }

    #[test]
    fn query_params_to_sql() {
        let q = QueryParams::new(Pagination::new(2, 10))
            .sort(SortParams::new("name", SortDirection::Desc))
            .filter("age", FilterOp::Gte, "18");
        let sql = q.to_sql();
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("LIMIT"));
        assert!(sql.contains("OFFSET"));
    }

    #[test]
    fn query_params_to_sql_simple() {
        let q = QueryParams::new(Pagination::new(1, 10));
        let sql = q.to_sql();
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 0"));
    }
}
