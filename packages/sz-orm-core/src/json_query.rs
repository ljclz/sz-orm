//! JSON 字段查询增强
//!
//! 对应 think-orm 的 JSON 字段查询语法，支持 MySQL JSON 函数 + PostgreSQL jsonb 函数 + SQLite json_extract。
//!
//! # 三种方言差异
//!
//! | 操作 | MySQL | PostgreSQL | SQLite |
//! |------|-------|-----------|--------|
//! | 取字段 | `->'$.field'` | `->>'field'` | `json_extract(col, '$.field')` |
//! | 取路径 | `->'$.a.b'` | `#>>'{a,b}'` | `json_extract(col, '$.a.b')` |
//! | 包含键 | `JSON_CONTAINS(col, '"v"', '$.k')` | `col @> '{"k":"v"}'` | `json_extract(col,'$.k')='v'` |
//! | 数组长度 | `JSON_LENGTH(col)` | `jsonb_array_length(col)` | `json_array_length(col)` |
//!
//! # 用法
//!
//! ```no_run
//! use sz_orm_core::json_query::JsonQuery;
//! use sz_orm_core::DbType;
//!
//! // MySQL: WHERE `prefs`->'$.theme' = 'dark'
//! let cond = JsonQuery::new(DbType::MySQL, "prefs")
//!     .path("theme")
//!     .eq_string("dark");
//! ```

use crate::db_type::DbType;

/// JSON 字段查询构造器
///
/// 提供 think-orm 风格的链式 JSON 字段查询 API，支持 MySQL/PostgreSQL/SQLite 三种方言。
pub struct JsonQuery {
    db_type: DbType,
    column: String,
    /// JSON 路径表达式（如 `theme` 或 `a.b.c`）
    path: Option<String>,
}

impl JsonQuery {
    /// 创建 JSON 查询构造器
    ///
    /// - `db_type`：目标数据库类型
    /// - `column`：JSON 列名
    pub fn new(db_type: DbType, column: impl Into<String>) -> Self {
        Self {
            db_type,
            column: column.into(),
            path: None,
        }
    }

    /// 指定 JSON 路径（如 `theme` 或 `a.b.c`）
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// 构建取字段表达式（左侧值，不含操作符与右侧值）
    ///
    /// - MySQL: `col->'$.field'`
    /// - PostgreSQL: `col->>'field'`
    /// - SQLite: `json_extract(col, '$.field')`
    ///
    /// 当路径为空时（未调用 `path()` 或传入空字符串），直接引用列自身，
    /// 避免生成 `'$.'`（MySQL 非法路径）或 `''` 等非法语法。
    pub fn build_extract(&self) -> String {
        let path = self.path.as_deref().unwrap_or("");
        if path.is_empty() {
            // 空路径：直接引用列自身，等价于取整个 JSON 文档
            return match self.db_type {
                DbType::PostgreSQL => format!("\"{}\"", self.column),
                _ => format!("`{}`", self.column),
            };
        }
        match self.db_type {
            DbType::MySQL => {
                // MySQL: `col`->'$.a.b.c'
                format!("`{}`->'$.{}'", self.column, path)
            }
            DbType::PostgreSQL => {
                // PG: "col"->>'a'->>'b'->>'c'  或  "col"->>'a'（单层）
                let parts: Vec<&str> = path.split('.').collect();
                let mut expr = format!("\"{}\"", self.column);
                for p in parts {
                    expr.push_str(&format!("->>'{}'", p));
                }
                expr
            }
            DbType::Sqlite => {
                // SQLite: json_extract(col, '$.a.b.c')
                format!("json_extract(`{}`, '$.{}')", self.column, path)
            }
            _ => {
                // 不支持的方言回退到 MySQL 语法
                format!("`{}`->'$.{}'", self.column, path)
            }
        }
    }

    /// `=` 字符串值
    pub fn eq_string(self, value: &str) -> String {
        format!("{} = '{}'", self.build_extract(), escape_sql_str(value))
    }

    /// `=` 整数值
    pub fn eq_i64(self, value: i64) -> String {
        format!("{} = {}", self.build_extract(), value)
    }

    /// `=` 浮点值
    pub fn eq_f64(self, value: f64) -> String {
        format!("{} = {}", self.build_extract(), value)
    }

    /// `!=` 字符串值
    pub fn ne_string(self, value: &str) -> String {
        format!("{} != '{}'", self.build_extract(), escape_sql_str(value))
    }

    /// `>` 字符串值
    pub fn gt_string(self, value: &str) -> String {
        format!("{} > '{}'", self.build_extract(), escape_sql_str(value))
    }

    /// `<` 字符串值
    pub fn lt_string(self, value: &str) -> String {
        format!("{} < '{}'", self.build_extract(), escape_sql_str(value))
    }

    /// `>=` 字符串值
    pub fn ge_string(self, value: &str) -> String {
        format!("{} >= '{}'", self.build_extract(), escape_sql_str(value))
    }

    /// `<=` 字符串值
    pub fn le_string(self, value: &str) -> String {
        format!("{} <= '{}'", self.build_extract(), escape_sql_str(value))
    }

    /// `>=` 整数值
    pub fn ge_i64(self, value: i64) -> String {
        format!("{} >= {}", self.build_extract(), value)
    }

    /// `<=` 整数值
    pub fn le_i64(self, value: i64) -> String {
        format!("{} <= {}", self.build_extract(), value)
    }

    /// `>` 整数值
    pub fn gt_i64(self, value: i64) -> String {
        format!("{} > {}", self.build_extract(), value)
    }

    /// `<` 整数值
    pub fn lt_i64(self, value: i64) -> String {
        format!("{} < {}", self.build_extract(), value)
    }

    /// `BETWEEN` 整数范围（包含两端）
    pub fn between_i64(self, low: i64, high: i64) -> String {
        format!("{} BETWEEN {} AND {}", self.build_extract(), low, high)
    }

    /// `IN (字符串列表)`
    pub fn in_strs(self, values: &[&str]) -> String {
        let list: Vec<String> = values
            .iter()
            .map(|v| format!("'{}'", escape_sql_str(v)))
            .collect();
        format!("{} IN ({})", self.build_extract(), list.join(", "))
    }

    /// `IN (整数列表)`
    pub fn in_i64s(self, values: &[i64]) -> String {
        let list: Vec<String> = values.iter().map(|v| v.to_string()).collect();
        format!("{} IN ({})", self.build_extract(), list.join(", "))
    }

    /// `LIKE` 字符串值
    pub fn like(self, value: &str) -> String {
        format!(
            "{} LIKE '%{}%'",
            self.build_extract(),
            escape_sql_str(value)
        )
    }

    /// `IS NULL`
    pub fn is_null(self) -> String {
        format!("{} IS NULL", self.build_extract())
    }

    /// `IS NOT NULL`
    pub fn is_not_null(self) -> String {
        format!("{} IS NOT NULL", self.build_extract())
    }

    /// 键存在性检查（路径下有键）
    ///
    /// - MySQL: `JSON_CONTAINS_PATH(col, 'one', '$.path')`
    /// - PostgreSQL: `col ? 'path'`（顶层键）/ `col #? '{path}'`（路径）
    /// - SQLite: `json_type(col, '$.path') IS NOT NULL`
    pub fn has_key(self) -> String {
        let path = self.path.as_deref().unwrap_or("");
        match self.db_type {
            DbType::MySQL => format!("JSON_CONTAINS_PATH(`{}`, 'one', '$.{}')", self.column, path),
            DbType::PostgreSQL => {
                // PG: 顶层用 ?，多层路径用 #>
                let parts: Vec<&str> = path.split('.').collect();
                if parts.len() <= 1 {
                    format!("\"{}\" ? '{}'", self.column, path)
                } else {
                    let path_braced = parts.join(",");
                    format!(
                        "\"{}\" #> '{{{{{}}}}}' IS NOT NULL",
                        self.column, path_braced
                    )
                }
            }
            DbType::Sqlite => format!("json_type(`{}`, '$.{}') IS NOT NULL", self.column, path),
            _ => format!("JSON_CONTAINS_PATH(`{}`, 'one', '$.{}')", self.column, path),
        }
    }

    /// JSON 类型检查（判断 JSON 值类型）
    ///
    /// - MySQL: `JSON_TYPE(col->'$.path') = 'INTEGER'`
    /// - PostgreSQL: `json_typeof(col#>>'{path}') = 'integer'`
    /// - SQLite: `json_type(col, '$.path') = 'integer'`
    ///
    /// `expected_type` 应为小写（'integer'/'string'/'boolean'/'array'/'object'/'null'）。
    /// 在 MySQL 中会被自动转为大写。
    pub fn json_type_eq(self, expected_type: &str) -> String {
        let path = self.path.as_deref().unwrap_or("");
        match self.db_type {
            DbType::MySQL => {
                let upper = expected_type.to_uppercase();
                format!("JSON_TYPE(`{}`->'$.{}') = '{}'", self.column, path, upper)
            }
            DbType::PostgreSQL => {
                let parts: Vec<&str> = path.split('.').collect();
                let path_braced = parts.join(",");
                format!(
                    "json_typeof(\"{}\"#>>'{{{}}}') = '{}'",
                    self.column, path_braced, expected_type
                )
            }
            DbType::Sqlite => format!(
                "json_type(`{}`, '$.{}') = '{}'",
                self.column, path, expected_type
            ),
            _ => {
                let upper = expected_type.to_uppercase();
                format!("JSON_TYPE(`{}`->'$.{}') = '{}'", self.column, path, upper)
            }
        }
    }

    /// 数组包含某元素（JSON_CONTAINS / @> / json_extract LIKE）
    ///
    /// - MySQL: `JSON_CONTAINS(col, '"v"', '$.path')`
    /// - PostgreSQL: `col @> '{"path":"v"}'`（简化：用 path 拼接）
    /// - SQLite: `EXISTS (SELECT 1 FROM json_each(json_extract(col, '$.path')) WHERE value = 'v')`
    pub fn contains(self, value: &str) -> String {
        match self.db_type {
            DbType::MySQL => {
                let path = self.path.as_deref().unwrap_or("");
                format!(
                    "JSON_CONTAINS(`{}`, '\"{}\"', '$.{}')",
                    self.column,
                    escape_sql_str(value),
                    path
                )
            }
            DbType::PostgreSQL => {
                // PG: col @> '{"key":"value"}' 形式（path 为单层时直接用）
                let path = self.path.as_deref().unwrap_or("");
                format!(
                    "\"{}\" @> '{{\"{}\":\"{}\"}}'",
                    self.column,
                    path,
                    escape_sql_str(value)
                )
            }
            DbType::Sqlite => {
                // SQLite: json_each().value 返回的是已解码的 JSON 值（如 `rust`），而非 JSON 编码的 `"rust"`。
                // 因此 WHERE value = 'rust'，不需要在两侧再包裹双引号。
                let path = self.path.as_deref().unwrap_or("");
                format!(
                    "EXISTS (SELECT 1 FROM json_each(json_extract(`{}`, '$.{}')) WHERE value = '{}')",
                    self.column,
                    path,
                    escape_sql_str(value)
                )
            }
            _ => {
                let path = self.path.as_deref().unwrap_or("");
                format!(
                    "JSON_CONTAINS(`{}`, '\"{}\"', '$.{}')",
                    self.column,
                    escape_sql_str(value),
                    path
                )
            }
        }
    }

    /// 数组长度比较
    ///
    /// - MySQL: `JSON_LENGTH(col->'$.path') = N`
    /// - PG: `jsonb_array_length(col#>>'{a,b}') = N`
    /// - SQLite: `json_array_length(json_extract(col, '$.path')) = N`
    pub fn array_length_eq(self, length: i64) -> String {
        let path = self.path.as_deref().unwrap_or("");
        match self.db_type {
            DbType::MySQL => {
                format!("JSON_LENGTH(`{}`->'$.{}') = {}", self.column, path, length)
            }
            DbType::PostgreSQL => {
                let parts: Vec<&str> = path.split('.').collect();
                let path_str = parts.join(",");
                format!(
                    "jsonb_array_length(\"{}\"#>>'{{{}}}') = {}",
                    self.column, path_str, length
                )
            }
            DbType::Sqlite => {
                format!(
                    "json_array_length(json_extract(`{}`, '$.{}')) = {}",
                    self.column, path, length
                )
            }
            _ => {
                format!("JSON_LENGTH(`{}`->'$.{}') = {}", self.column, path, length)
            }
        }
    }

    /// 返回列名（不带方言处理）
    pub fn column(&self) -> &str {
        &self.column
    }

    /// 返回数据库类型
    pub fn db_type(&self) -> DbType {
        self.db_type
    }
}

/// JSON 字段更新构造器
///
/// 提供 think-orm 风格的 JSON 字段 SET 子句构造。
///
/// - MySQL: `JSON_SET(col, '$.key', 'value')`
/// - PG: `jsonb_set(col, '{key}', '"value"')`
/// - SQLite: `json_set(col, '$.key', 'value')`
pub struct JsonUpdate {
    db_type: DbType,
    column: String,
    sets: Vec<(String, String)>,
    /// 数组追加操作（key, value 的 SQL 字面量表达式）
    array_appends: Vec<(String, String)>,
    /// 需要删除的 JSON 路径
    removes: Vec<String>,
}

impl JsonUpdate {
    /// 创建 JSON 更新构造器
    pub fn new(db_type: DbType, column: impl Into<String>) -> Self {
        Self {
            db_type,
            column: column.into(),
            sets: Vec::new(),
            array_appends: Vec::new(),
            removes: Vec::new(),
        }
    }

    /// 添加一个 SET 项（key → value，value 为字符串）
    #[must_use]
    pub fn set_str(mut self, key: impl Into<String>, value: &str) -> Self {
        self.sets
            .push((key.into(), format!("'{}'", escape_sql_str(value))));
        self
    }

    /// 添加一个 SET 项（key → value，value 为 i64）
    #[must_use]
    pub fn set_i64(mut self, key: impl Into<String>, value: i64) -> Self {
        self.sets.push((key.into(), value.to_string()));
        self
    }

    /// 添加一个 SET 项（key → value，value 为 bool）
    #[must_use]
    pub fn set_bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.sets.push((
            key.into(),
            if value {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ));
        self
    }

    /// 数组追加元素（字符串）：将 value 追加到 col.path 指向的数组末尾
    ///
    /// - MySQL: `col = JSON_ARRAY_APPEND(col, '$.path', 'v')`
    /// - PG: `col = jsonb_set(col, '{path}', (col#>'{path}') || to_jsonb('v'::text))`
    /// - SQLite: `col = json_set(col, '$.path', json_insert(col->'$.path', '$[#]', 'v'))`
    ///
    /// 注：追加多个元素请多次调用。
    #[must_use]
    pub fn array_append_str(mut self, key: impl Into<String>, value: &str) -> Self {
        // 复用 sets，但用特殊标记区分；这里直接构建为完整 SQL 表达式存入
        // 改为：在 build_set 时用专门的处理逻辑
        let k = key.into();
        let v = format!("'{}'", escape_sql_str(value));
        self.array_appends.push((k, v));
        self
    }

    /// 数组追加元素（整数）
    #[must_use]
    pub fn array_append_i64(mut self, key: impl Into<String>, value: i64) -> Self {
        let k = key.into();
        let v = value.to_string();
        self.array_appends.push((k, v));
        self
    }

    /// 删除指定 JSON 路径的字段
    ///
    /// - MySQL: `col = JSON_REMOVE(col, '$.key')`
    /// - PG: `col = col - 'key'`
    /// - SQLite: `col = json_remove(col, '$.key')`
    #[must_use]
    pub fn remove_key(mut self, key: impl Into<String>) -> Self {
        self.removes.push(key.into());
        self
    }

    /// 构建 SET 子句片段（不含 `SET` 关键字）
    ///
    /// 返回可直接拼到 `UPDATE ... SET <此处>` 的字符串。
    /// 若同时存在 set / array_append / remove 操作，将按 SET → APPEND → REMOVE 顺序合并到同一列。
    pub fn build_set(&self) -> String {
        // 三类操作均为空：返回恒等赋值
        let empty =
            self.sets.is_empty() && self.array_appends.is_empty() && self.removes.is_empty();
        if empty {
            return match self.db_type {
                DbType::PostgreSQL => format!("\"{}\" = \"{}\"", self.column, self.column),
                _ => format!("`{}` = `{}`", self.column, self.column),
            };
        }

        match self.db_type {
            DbType::MySQL => self.build_set_mysql(),
            DbType::PostgreSQL => self.build_set_pg(),
            DbType::Sqlite => self.build_set_sqlite(),
            _ => self.build_set_mysql(),
        }
    }

    fn build_set_mysql(&self) -> String {
        // MySQL: 链式嵌套 col = JSON_REMOVE(JSON_ARRAY_APPEND(JSON_SET(col, ...), ...), ...)
        let mut expr = format!("`{}`", self.column);

        // 1. SET
        if !self.sets.is_empty() {
            let args: Vec<String> = self
                .sets
                .iter()
                .map(|(k, v)| format!("'$.{}', {}", k, v))
                .collect();
            expr = format!("JSON_SET({}, {})", expr, args.join(", "));
        }

        // 2. ARRAY_APPEND
        for (k, v) in &self.array_appends {
            expr = format!("JSON_ARRAY_APPEND({}, '$.{}', {})", expr, k, v);
        }

        // 3. REMOVE
        if !self.removes.is_empty() {
            let args: Vec<String> = self.removes.iter().map(|k| format!("'$.{}'", k)).collect();
            expr = format!("JSON_REMOVE({}, {})", expr, args.join(", "));
        }

        format!("`{}` = {}", self.column, expr)
    }

    fn build_set_pg(&self) -> String {
        // PG: 链式嵌套 "col" = (col - 'rm' || jsonb_build_array(...)...) 等
        // 简化：SET 用 jsonb_set 链式，APPEND 用 ||，REMOVE 用 -
        let mut expr = format!("\"{}\"", self.column);

        // 1. SET (链式 jsonb_set)
        for (k, v) in &self.sets {
            expr = format!("jsonb_set({}, '{{{}}}', {})", expr, k, v);
        }

        // 2. ARRAY_APPEND (用 || 拼接单元素数组)
        for (k, v) in &self.array_appends {
            // 把 v 包装为 to_jsonb 形式后追加到 #>'{k}' 数组
            // v 可能是 'value' 或 100，统一用 to_jsonb 处理
            // 由于 expr 是 String，需 clone 才能在 format! 中使用两次
            let current = expr.clone();
            expr = format!(
                "jsonb_set({}, '{{{}}}', ({}#>'{{{}}}') || to_jsonb({}::text))",
                current, k, current, k, v
            );
        }

        // 3. REMOVE (用 -)
        for k in &self.removes {
            expr = format!("({} - '{}')", expr, k);
        }

        format!("\"{}\" = {}", self.column, expr)
    }

    fn build_set_sqlite(&self) -> String {
        let mut expr = format!("`{}`", self.column);

        // 1. SET
        if !self.sets.is_empty() {
            let args: Vec<String> = self
                .sets
                .iter()
                .map(|(k, v)| format!("'$.{}', {}", k, v))
                .collect();
            expr = format!("json_set({}, {})", expr, args.join(", "));
        }

        // 2. ARRAY_APPEND（用 json_insert 在 '$[#]' 位置追加）
        for (k, v) in &self.array_appends {
            let current = expr.clone();
            expr = format!(
                "json_set({}, '$.{}', json_insert({}->'$.{}', '$[#]', {}))",
                current, k, current, k, v
            );
        }

        // 3. REMOVE
        if !self.removes.is_empty() {
            let args: Vec<String> = self.removes.iter().map(|k| format!("'$.{}'", k)).collect();
            expr = format!("json_remove({}, {})", expr, args.join(", "));
        }

        format!("`{}` = {}", self.column, expr)
    }
}

/// 转义 SQL 字符串中的单引号
fn escape_sql_str(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== JsonQuery::build_extract 三方言测试 =====

    #[test]
    fn mysql_extract_single_field() {
        let q = JsonQuery::new(DbType::MySQL, "prefs").path("theme");
        assert_eq!(q.build_extract(), "`prefs`->'$.theme'");
    }

    #[test]
    fn mysql_extract_nested_path() {
        let q = JsonQuery::new(DbType::MySQL, "prefs").path("a.b.c");
        assert_eq!(q.build_extract(), "`prefs`->'$.a.b.c'");
    }

    #[test]
    fn pg_extract_single_field() {
        let q = JsonQuery::new(DbType::PostgreSQL, "prefs").path("theme");
        assert_eq!(q.build_extract(), "\"prefs\"->>'theme'");
    }

    #[test]
    fn pg_extract_nested_path() {
        let q = JsonQuery::new(DbType::PostgreSQL, "prefs").path("a.b.c");
        assert_eq!(q.build_extract(), "\"prefs\"->>'a'->>'b'->>'c'");
    }

    #[test]
    fn sqlite_extract_single_field() {
        let q = JsonQuery::new(DbType::Sqlite, "prefs").path("theme");
        assert_eq!(q.build_extract(), "json_extract(`prefs`, '$.theme')");
    }

    #[test]
    fn sqlite_extract_nested_path() {
        let q = JsonQuery::new(DbType::Sqlite, "prefs").path("a.b.c");
        assert_eq!(q.build_extract(), "json_extract(`prefs`, '$.a.b.c')");
    }

    // ===== 比较操作测试 =====

    #[test]
    fn mysql_eq_string() {
        let cond = JsonQuery::new(DbType::MySQL, "prefs")
            .path("theme")
            .eq_string("dark");
        assert_eq!(cond, "`prefs`->'$.theme' = 'dark'");
    }

    #[test]
    fn mysql_eq_i64() {
        let cond = JsonQuery::new(DbType::MySQL, "stats")
            .path("visits")
            .eq_i64(100);
        assert_eq!(cond, "`stats`->'$.visits' = 100");
    }

    #[test]
    fn mysql_eq_f64() {
        let cond = JsonQuery::new(DbType::MySQL, "stats")
            .path("rate")
            .eq_f64(0.95);
        assert!(cond.starts_with("`stats`->'$.rate' = 0.95"));
    }

    #[test]
    fn mysql_ne_string() {
        let cond = JsonQuery::new(DbType::MySQL, "prefs")
            .path("theme")
            .ne_string("dark");
        assert_eq!(cond, "`prefs`->'$.theme' != 'dark'");
    }

    #[test]
    fn mysql_gt_lt_string() {
        let gt = JsonQuery::new(DbType::MySQL, "prefs")
            .path("name")
            .gt_string("m");
        assert_eq!(gt, "`prefs`->'$.name' > 'm'");
        let lt = JsonQuery::new(DbType::MySQL, "prefs")
            .path("name")
            .lt_string("n");
        assert_eq!(lt, "`prefs`->'$.name' < 'n'");
    }

    #[test]
    fn mysql_like() {
        let cond = JsonQuery::new(DbType::MySQL, "prefs")
            .path("bio")
            .like("engineer");
        assert_eq!(cond, "`prefs`->'$.bio' LIKE '%engineer%'");
    }

    #[test]
    fn mysql_is_null_and_not_null() {
        let n = JsonQuery::new(DbType::MySQL, "prefs").path("opt").is_null();
        assert_eq!(n, "`prefs`->'$.opt' IS NULL");
        let nn = JsonQuery::new(DbType::MySQL, "prefs")
            .path("opt")
            .is_not_null();
        assert_eq!(nn, "`prefs`->'$.opt' IS NOT NULL");
    }

    // ===== 转义测试 =====

    #[test]
    fn escape_single_quote_in_value() {
        let cond = JsonQuery::new(DbType::MySQL, "prefs")
            .path("name")
            .eq_string("O'Brien");
        assert_eq!(cond, "`prefs`->'$.name' = 'O''Brien'");
    }

    // ===== contains 三方言测试 =====

    #[test]
    fn mysql_contains() {
        let cond = JsonQuery::new(DbType::MySQL, "tags")
            .path("category")
            .contains("rust");
        assert_eq!(cond, "JSON_CONTAINS(`tags`, '\"rust\"', '$.category')");
    }

    #[test]
    fn pg_contains() {
        let cond = JsonQuery::new(DbType::PostgreSQL, "tags")
            .path("category")
            .contains("rust");
        assert_eq!(cond, "\"tags\" @> '{\"category\":\"rust\"}'");
    }

    #[test]
    fn sqlite_contains() {
        let cond = JsonQuery::new(DbType::Sqlite, "tags")
            .path("category")
            .contains("rust");
        // SQLite json_each().value 返回已解码的值（rust），不需要再包裹双引号
        assert_eq!(
            cond,
            "EXISTS (SELECT 1 FROM json_each(json_extract(`tags`, '$.category')) WHERE value = 'rust')"
        );
    }

    // ===== array_length 三方言测试 =====

    #[test]
    fn mysql_array_length() {
        let cond = JsonQuery::new(DbType::MySQL, "items")
            .path("list")
            .array_length_eq(3);
        assert_eq!(cond, "JSON_LENGTH(`items`->'$.list') = 3");
    }

    #[test]
    fn pg_array_length() {
        let cond = JsonQuery::new(DbType::PostgreSQL, "items")
            .path("a.b")
            .array_length_eq(3);
        assert_eq!(cond, "jsonb_array_length(\"items\"#>>'{a,b}') = 3");
    }

    #[test]
    fn sqlite_array_length() {
        let cond = JsonQuery::new(DbType::Sqlite, "items")
            .path("list")
            .array_length_eq(3);
        assert_eq!(
            cond,
            "json_array_length(json_extract(`items`, '$.list')) = 3"
        );
    }

    // ===== JsonUpdate 测试 =====

    #[test]
    fn mysql_json_set_single() {
        let set = JsonUpdate::new(DbType::MySQL, "prefs")
            .set_str("theme", "dark")
            .build_set();
        assert_eq!(set, "`prefs` = JSON_SET(`prefs`, '$.theme', 'dark')");
    }

    #[test]
    fn mysql_json_set_multi() {
        let set = JsonUpdate::new(DbType::MySQL, "prefs")
            .set_str("theme", "dark")
            .set_i64("volume", 80)
            .set_bool("autoplay", true)
            .build_set();
        assert!(set.contains("JSON_SET(`prefs`"));
        assert!(set.contains("'$.theme', 'dark'"));
        assert!(set.contains("'$.volume', 80"));
        assert!(set.contains("'$.autoplay', true"));
    }

    #[test]
    fn pg_json_set_single() {
        let set = JsonUpdate::new(DbType::PostgreSQL, "prefs")
            .set_str("theme", "dark")
            .build_set();
        // PG: 顶层包裹 "col" = jsonb_set(...)
        assert_eq!(set, "\"prefs\" = jsonb_set(\"prefs\", '{theme}', 'dark')");
    }

    #[test]
    fn sqlite_json_set_single() {
        let set = JsonUpdate::new(DbType::Sqlite, "prefs")
            .set_str("theme", "dark")
            .build_set();
        assert_eq!(set, "`prefs` = json_set(`prefs`, '$.theme', 'dark')");
    }

    #[test]
    fn json_update_empty_set() {
        let set = JsonUpdate::new(DbType::MySQL, "prefs").build_set();
        assert_eq!(set, "`prefs` = `prefs`");
    }

    // ===== 整合测试：JSON 查询 + 主查询 =====

    #[test]
    fn json_query_integrate_with_quick_query() {
        use crate::dialect::get_dialect;
        use crate::quick_query::Db;

        let dialect = get_dialect(DbType::MySQL).expect("MySQL");
        let json_cond = JsonQuery::new(DbType::MySQL, "prefs")
            .path("theme")
            .eq_string("dark");
        let sql = Db::new(dialect)
            .name("users")
            .where_cond(json_cond)
            .build_select();
        assert_eq!(
            sql,
            "SELECT * FROM `users` WHERE `prefs`->'$.theme' = 'dark'"
        );
    }

    #[test]
    fn json_update_integrate_with_quick_query() {
        use crate::dialect::get_dialect;

        let _dialect = get_dialect(DbType::MySQL).expect("MySQL");
        let set_clause = JsonUpdate::new(DbType::MySQL, "prefs")
            .set_str("theme", "light")
            .build_set();
        // 验证 SET 子句正确性，可直接拼到 UPDATE ... SET <此处>
        assert!(set_clause.contains("JSON_SET(`prefs`"));
        assert!(set_clause.contains("'$.theme', 'light'"));

        // 拼接 UPDATE SQL：UPDATE `users` SET <set_clause> WHERE id = 1
        let sql = format!("UPDATE `users` SET {} WHERE id = 1", set_clause);
        assert!(sql.starts_with("UPDATE `users` SET `prefs` = JSON_SET(`prefs`"));
        assert!(sql.contains("WHERE id = 1"));
    }

    // ===== 边界/极端测试 =====

    #[test]
    fn empty_path_extracts_root() {
        // 空路径应直接引用列自身，避免 MySQL 的 `'$.'` 非法路径表达式
        let q = JsonQuery::new(DbType::MySQL, "data").build_extract();
        assert_eq!(q, "`data`");
    }

    #[test]
    fn empty_path_extracts_root_pg() {
        // PostgreSQL 空路径也应直接引用列自身
        let q = JsonQuery::new(DbType::PostgreSQL, "data").build_extract();
        assert_eq!(q, "\"data\"");
    }

    #[test]
    fn empty_path_extracts_root_sqlite() {
        // SQLite 空路径也应直接引用列自身
        let q = JsonQuery::new(DbType::Sqlite, "data").build_extract();
        assert_eq!(q, "`data`");
    }

    #[test]
    fn unsupported_db_falls_back_to_mysql() {
        let q = JsonQuery::new(DbType::Redis, "data").path("x");
        // Redis 不支持 JSON，应回退到 MySQL 语法
        assert_eq!(q.build_extract(), "`data`->'$.x'");
    }

    #[test]
    fn special_chars_in_value_escaped() {
        // 反斜杠不转义（SQL 标准只要求转义单引号），但单引号必须转义
        let cond = JsonQuery::new(DbType::MySQL, "d")
            .path("k")
            .eq_string("a'b'c");
        assert_eq!(cond, "`d`->'$.k' = 'a''b''c'");
    }

    // ===== v0.2.0+ 增强：>= / <= / IN / BETWEEN / has_key / json_type_eq 测试 =====

    #[test]
    fn mysql_ge_le_string() {
        let ge = JsonQuery::new(DbType::MySQL, "d").path("k").ge_string("m");
        assert_eq!(ge, "`d`->'$.k' >= 'm'");
        let le = JsonQuery::new(DbType::MySQL, "d").path("k").le_string("m");
        assert_eq!(le, "`d`->'$.k' <= 'm'");
    }

    #[test]
    fn mysql_ge_le_i64() {
        let ge = JsonQuery::new(DbType::MySQL, "d").path("k").ge_i64(10);
        assert_eq!(ge, "`d`->'$.k' >= 10");
        let le = JsonQuery::new(DbType::MySQL, "d").path("k").le_i64(99);
        assert_eq!(le, "`d`->'$.k' <= 99");
        let gt = JsonQuery::new(DbType::MySQL, "d").path("k").gt_i64(5);
        assert_eq!(gt, "`d`->'$.k' > 5");
        let lt = JsonQuery::new(DbType::MySQL, "d").path("k").lt_i64(8);
        assert_eq!(lt, "`d`->'$.k' < 8");
    }

    #[test]
    fn mysql_between_i64() {
        let cond = JsonQuery::new(DbType::MySQL, "stats")
            .path("visits")
            .between_i64(10, 100);
        assert_eq!(cond, "`stats`->'$.visits' BETWEEN 10 AND 100");
    }

    #[test]
    fn mysql_in_strs() {
        let cond = JsonQuery::new(DbType::MySQL, "prefs")
            .path("theme")
            .in_strs(&["dark", "light"]);
        assert_eq!(cond, "`prefs`->'$.theme' IN ('dark', 'light')");
    }

    #[test]
    fn mysql_in_i64s() {
        let cond = JsonQuery::new(DbType::MySQL, "stats")
            .path("level")
            .in_i64s(&[1, 2, 3]);
        assert_eq!(cond, "`stats`->'$.level' IN (1, 2, 3)");
    }

    #[test]
    fn mysql_in_strs_with_quote_escape() {
        let cond = JsonQuery::new(DbType::MySQL, "d")
            .path("k")
            .in_strs(&["a'b", "c"]);
        assert_eq!(cond, "`d`->'$.k' IN ('a''b', 'c')");
    }

    #[test]
    fn mysql_in_empty_list() {
        // 空列表生成 IN ()，语义上等价于 false（标准 SQL 行为）
        let cond = JsonQuery::new(DbType::MySQL, "d").path("k").in_strs(&[]);
        assert_eq!(cond, "`d`->'$.k' IN ()");
    }

    #[test]
    fn mysql_has_key() {
        let cond = JsonQuery::new(DbType::MySQL, "prefs")
            .path("theme")
            .has_key();
        assert_eq!(cond, "JSON_CONTAINS_PATH(`prefs`, 'one', '$.theme')");
    }

    #[test]
    fn pg_has_key_single_level() {
        let cond = JsonQuery::new(DbType::PostgreSQL, "prefs")
            .path("theme")
            .has_key();
        assert_eq!(cond, "\"prefs\" ? 'theme'");
    }

    #[test]
    fn pg_has_key_multi_level() {
        let cond = JsonQuery::new(DbType::PostgreSQL, "prefs")
            .path("a.b.c")
            .has_key();
        // 多层路径用 #> + IS NOT NULL
        assert!(cond.contains("#>"));
        assert!(cond.contains("IS NOT NULL"));
    }

    #[test]
    fn sqlite_has_key() {
        let cond = JsonQuery::new(DbType::Sqlite, "prefs")
            .path("theme")
            .has_key();
        assert_eq!(cond, "json_type(`prefs`, '$.theme') IS NOT NULL");
    }

    #[test]
    fn mysql_json_type_eq_integer() {
        let cond = JsonQuery::new(DbType::MySQL, "stats")
            .path("visits")
            .json_type_eq("integer");
        // MySQL JSON_TYPE 返回大写
        assert_eq!(cond, "JSON_TYPE(`stats`->'$.visits') = 'INTEGER'");
    }

    #[test]
    fn mysql_json_type_eq_array() {
        let cond = JsonQuery::new(DbType::MySQL, "data")
            .path("tags")
            .json_type_eq("array");
        assert_eq!(cond, "JSON_TYPE(`data`->'$.tags') = 'ARRAY'");
    }

    #[test]
    fn pg_json_type_eq() {
        let cond = JsonQuery::new(DbType::PostgreSQL, "stats")
            .path("visits")
            .json_type_eq("integer");
        assert_eq!(cond, "json_typeof(\"stats\"#>>'{visits}') = 'integer'");
    }

    #[test]
    fn sqlite_json_type_eq() {
        let cond = JsonQuery::new(DbType::Sqlite, "data")
            .path("tags")
            .json_type_eq("array");
        assert_eq!(cond, "json_type(`data`, '$.tags') = 'array'");
    }

    // ===== JsonUpdate 增强：array_append / remove_key 测试 =====

    #[test]
    fn mysql_array_append_str_single() {
        let set = JsonUpdate::new(DbType::MySQL, "tags")
            .array_append_str("list", "rust")
            .build_set();
        assert_eq!(set, "`tags` = JSON_ARRAY_APPEND(`tags`, '$.list', 'rust')");
    }

    #[test]
    fn mysql_array_append_i64_single() {
        let set = JsonUpdate::new(DbType::MySQL, "nums")
            .array_append_i64("list", 42)
            .build_set();
        assert_eq!(set, "`nums` = JSON_ARRAY_APPEND(`nums`, '$.list', 42)");
    }

    #[test]
    fn mysql_array_append_multiple() {
        let set = JsonUpdate::new(DbType::MySQL, "tags")
            .array_append_str("list", "rust")
            .array_append_str("list", "orm")
            .build_set();
        // 多次追加应嵌套
        assert!(set.contains("JSON_ARRAY_APPEND(JSON_ARRAY_APPEND"));
        assert!(set.contains("'rust'"));
        assert!(set.contains("'orm'"));
    }

    #[test]
    fn mysql_remove_key_single() {
        let set = JsonUpdate::new(DbType::MySQL, "prefs")
            .remove_key("deprecated_field")
            .build_set();
        assert_eq!(set, "`prefs` = JSON_REMOVE(`prefs`, '$.deprecated_field')");
    }

    #[test]
    fn mysql_remove_key_multiple() {
        let set = JsonUpdate::new(DbType::MySQL, "prefs")
            .remove_key("a")
            .remove_key("b")
            .build_set();
        assert_eq!(set, "`prefs` = JSON_REMOVE(`prefs`, '$.a', '$.b')");
    }

    #[test]
    fn mysql_combined_set_append_remove() {
        let set = JsonUpdate::new(DbType::MySQL, "prefs")
            .set_str("theme", "dark")
            .array_append_str("tags", "new")
            .remove_key("old_field")
            .build_set();
        // SET 在最内层，REMOVE 在最外层（链式嵌套）
        assert!(set.starts_with("`prefs` = JSON_REMOVE(JSON_ARRAY_APPEND(JSON_SET("));
        assert!(set.contains("'$.theme', 'dark'"));
        assert!(set.contains("'$.tags', 'new'"));
        assert!(set.contains("'$.old_field'"));
    }

    #[test]
    fn sqlite_remove_key() {
        let set = JsonUpdate::new(DbType::Sqlite, "prefs")
            .remove_key("old")
            .build_set();
        assert_eq!(set, "`prefs` = json_remove(`prefs`, '$.old')");
    }

    #[test]
    fn sqlite_array_append() {
        let set = JsonUpdate::new(DbType::Sqlite, "tags")
            .array_append_str("list", "rust")
            .build_set();
        assert!(set.contains("json_set"));
        assert!(set.contains("json_insert"));
        assert!(set.contains("'rust'"));
    }

    #[test]
    fn pg_remove_key() {
        let set = JsonUpdate::new(DbType::PostgreSQL, "prefs")
            .remove_key("old")
            .build_set();
        assert_eq!(set, "\"prefs\" = (\"prefs\" - 'old')");
    }

    #[test]
    fn pg_combined_set_remove() {
        let set = JsonUpdate::new(DbType::PostgreSQL, "prefs")
            .set_str("theme", "dark")
            .remove_key("old")
            .build_set();
        assert!(set.contains("jsonb_set"));
        // PG 删除用 `- 'key'` 操作符
        assert!(set.contains("- 'old'"));
    }
}
