//! 批量操作：批量插入/更新/删除 SQL 生成。
//!
//! - [`BatchInsertBuilder`] — 批量插入 SQL 生成（多行 VALUES）
//! - [`BatchUpdateBuilder`] — 批量更新 SQL 生成（基于主键）
//! - [`BatchDeleteBuilder`] — 批量删除 SQL 生成（基于主键列表）

use napi_derive::napi;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::DbType;

type Result<T> = napi::bindgen_prelude::Result<T>;

fn parse_db_type(s: &str) -> Result<DbType> {
    DbType::from_str(s).ok_or_else(|| napi::Error::from_reason(format!("unknown DbType: {}", s)))
}

fn dialect_or_err(db_type: DbType) -> Result<Box<dyn sz_orm_core::dialect::Dialect>> {
    get_dialect(db_type).map_err(|e| napi::Error::from_reason(e.to_string()))
}

// ============================================================================
// 批量插入
// ============================================================================

/// 批量插入结果
#[napi(object)]
pub struct BatchInsertResult {
    /// 生成的 SQL
    pub sql: String,
    /// 参数总数
    pub param_count: u32,
    /// 行数
    pub row_count: u32,
}

/// 批量插入构建器：生成多行 VALUES 子句的 INSERT 语句。
#[napi]
pub struct BatchInsertBuilder {
    db_type: DbType,
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[napi]
impl BatchInsertBuilder {
    /// 创建批量插入构建器
    #[napi(constructor)]
    pub fn new(db_type: Option<String>, table: String) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        Ok(Self {
            db_type: parse_db_type(&dt)?,
            table,
            columns: vec![],
            rows: vec![],
        })
    }

    /// 设置列名
    #[napi]
    pub fn set_columns(&mut self, columns: Vec<String>) {
        self.columns = columns;
    }

    /// 添加一行数据
    #[napi]
    pub fn add_row(&mut self, values: Vec<String>) {
        self.rows.push(values);
    }

    /// 批量添加多行
    #[napi]
    pub fn add_rows(&mut self, rows: Vec<Vec<String>>) {
        self.rows.extend(rows);
    }

    /// 当前行数
    #[napi]
    pub fn row_count(&self) -> u32 {
        self.rows.len() as u32
    }

    /// 清空已添加的行
    #[napi]
    pub fn clear(&mut self) {
        self.rows.clear();
    }

    /// 构建 INSERT SQL
    #[napi]
    pub fn build(&self) -> Result<BatchInsertResult> {
        let dialect = dialect_or_err(self.db_type)?;
        if self.columns.is_empty() {
            return Err(napi::Error::from_reason("columns not set"));
        }
        if self.rows.is_empty() {
            return Err(napi::Error::from_reason("no rows to insert"));
        }
        let cols: Vec<String> = self.columns.iter().map(|c| dialect.quote(c)).collect();
        let mut sql = format!(
            "INSERT INTO {} ({}) VALUES ",
            dialect.quote(&self.table),
            cols.join(", ")
        );
        let placeholders: Vec<String> = self
            .rows
            .iter()
            .map(|row| {
                if row.len() != self.columns.len() {
                    return format!("({})", vec!["?"; row.len()].join(", "));
                }
                format!("({})", vec!["?"; row.len()].join(", "))
            })
            .collect();
        sql.push_str(&placeholders.join(", "));
        Ok(BatchInsertResult {
            sql,
            param_count: (self.rows.len() * self.columns.len()) as u32,
            row_count: self.rows.len() as u32,
        })
    }

    /// 构建 INSERT ... ON CONFLICT DO NOTHING（PostgreSQL）/ INSERT IGNORE（MySQL）
    #[napi]
    pub fn build_or_ignore(&self) -> Result<BatchInsertResult> {
        let base = self.build()?;
        let suffix = match self.db_type {
            DbType::PostgreSQL => " ON CONFLICT DO NOTHING",
            DbType::MySQL => "",
            _ => "",
        };
        let prefix = if self.db_type == DbType::MySQL {
            "INSERT IGNORE INTO"
        } else {
            "INSERT INTO"
        };
        let sql = if self.db_type == DbType::MySQL {
            base.sql.replacen("INSERT INTO", "INSERT IGNORE INTO", 1)
        } else {
            format!("{}{}", base.sql, suffix)
        };
        let _ = prefix;
        Ok(BatchInsertResult {
            sql,
            param_count: base.param_count,
            row_count: base.row_count,
        })
    }
}

// ============================================================================
// 批量更新
// ============================================================================

/// 批量更新结果
#[napi(object)]
pub struct BatchUpdateResult {
    /// 生成的 SQL 语句列表（每行一条 UPDATE）
    pub sqls: Vec<String>,
    /// 语句数
    pub statement_count: u32,
}

/// 批量更新构建器：基于主键生成多条 UPDATE 语句。
#[napi]
pub struct BatchUpdateBuilder {
    db_type: DbType,
    table: String,
    pk_column: String,
    set_columns: Vec<String>,
    rows: Vec<(String, Vec<String>)>,
}

#[napi]
impl BatchUpdateBuilder {
    /// 创建批量更新构建器
    #[napi(constructor)]
    pub fn new(db_type: Option<String>, table: String, pk_column: String) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        Ok(Self {
            db_type: parse_db_type(&dt)?,
            table,
            pk_column,
            set_columns: vec![],
            rows: vec![],
        })
    }

    /// 设置要更新的列
    #[napi]
    pub fn set_columns(&mut self, columns: Vec<String>) {
        self.set_columns = columns;
    }

    /// 添加一行更新（pk_value + 各列新值）
    #[napi]
    pub fn add_row(&mut self, pk_value: String, values: Vec<String>) {
        self.rows.push((pk_value, values));
    }

    /// 当前行数
    #[napi]
    pub fn row_count(&self) -> u32 {
        self.rows.len() as u32
    }

    /// 构建多条 UPDATE SQL
    #[napi]
    pub fn build(&self) -> Result<BatchUpdateResult> {
        let dialect = dialect_or_err(self.db_type)?;
        if self.set_columns.is_empty() {
            return Err(napi::Error::from_reason("no columns to update"));
        }
        if self.rows.is_empty() {
            return Err(napi::Error::from_reason("no rows to update"));
        }
        let sqls: Vec<String> = self
            .rows
            .iter()
            .map(|(pk, values)| {
                let sets: Vec<String> = self
                    .set_columns
                    .iter()
                    .zip(values.iter())
                    .map(|(col, _)| format!("{} = ?", dialect.quote(col)))
                    .collect();
                format!(
                    "UPDATE {} SET {} WHERE {} = ?",
                    dialect.quote(&self.table),
                    sets.join(", "),
                    dialect.quote(&self.pk_column)
                )
            })
            .collect();
        Ok(BatchUpdateResult {
            statement_count: sqls.len() as u32,
            sqls,
        })
    }

    /// 清空已添加的行
    #[napi]
    pub fn clear(&mut self) {
        self.rows.clear();
    }
}

// ============================================================================
// 批量删除
// ============================================================================

/// 批量删除结果
#[napi(object)]
pub struct BatchDeleteResult {
    /// 生成的 SQL
    pub sql: String,
    /// 参数数（主键数）
    pub param_count: u32,
}

/// 批量删除构建器：基于主键列表生成 DELETE ... WHERE pk IN (?, ?, ...) 语句。
#[napi]
pub struct BatchDeleteBuilder {
    db_type: DbType,
    table: String,
    pk_column: String,
    pk_values: Vec<String>,
}

#[napi]
impl BatchDeleteBuilder {
    /// 创建批量删除构建器
    #[napi(constructor)]
    pub fn new(db_type: Option<String>, table: String, pk_column: String) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        Ok(Self {
            db_type: parse_db_type(&dt)?,
            table,
            pk_column,
            pk_values: vec![],
        })
    }

    /// 添加主键值
    #[napi]
    pub fn add(&mut self, pk_value: String) {
        self.pk_values.push(pk_value);
    }

    /// 批量添加主键值
    #[napi]
    pub fn add_many(&mut self, values: Vec<String>) {
        self.pk_values.extend(values);
    }

    /// 当前主键数
    #[napi]
    pub fn count(&self) -> u32 {
        self.pk_values.len() as u32
    }

    /// 构建 DELETE SQL
    #[napi]
    pub fn build(&self) -> Result<BatchDeleteResult> {
        let dialect = dialect_or_err(self.db_type)?;
        if self.pk_values.is_empty() {
            return Err(napi::Error::from_reason("no primary keys to delete"));
        }
        let placeholders: Vec<String> = self.pk_values.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "DELETE FROM {} WHERE {} IN ({})",
            dialect.quote(&self.table),
            dialect.quote(&self.pk_column),
            placeholders.join(", ")
        );
        Ok(BatchDeleteResult {
            sql,
            param_count: self.pk_values.len() as u32,
        })
    }

    /// 清空
    #[napi]
    pub fn clear(&mut self) {
        self.pk_values.clear();
    }
}

// ============================================================================
// 批量操作统计
// ============================================================================

/// 批量操作统计信息
#[napi(object)]
pub struct BatchStats {
    pub total_operations: u32,
    pub total_params: u32,
    pub avg_params_per_op: f64,
}

impl BatchStats {
    /// 从操作数和参数数计算统计
    pub fn compute(total_operations: u32, total_params: u32) -> Self {
        let avg = if total_operations == 0 {
            0.0
        } else {
            total_params as f64 / total_operations as f64
        };
        Self {
            total_operations,
            total_params,
            avg_params_per_op: avg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- BatchInsertBuilder -----

    #[test]
    fn batch_insert_basic() {
        let mut b = BatchInsertBuilder::new(None, "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string(), "age".to_string()]);
        b.add_row(vec!["Alice".to_string(), "30".to_string()]);
        b.add_row(vec!["Bob".to_string(), "25".to_string()]);
        let result = b.build().unwrap();
        assert!(result.sql.contains("INSERT INTO"));
        assert!(result.sql.contains("users"));
        assert!(result.sql.contains("name"));
        assert!(result.sql.contains("age"));
        assert_eq!(result.row_count, 2);
        assert_eq!(result.param_count, 4);
    }

    #[test]
    fn batch_insert_postgres() {
        let mut b =
            BatchInsertBuilder::new(Some("postgres".to_string()), "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row(vec!["Alice".to_string()]);
        let result = b.build().unwrap();
        assert!(result.sql.contains("\"users\""));
    }

    #[test]
    fn batch_insert_sqlite() {
        let mut b =
            BatchInsertBuilder::new(Some("sqlite".to_string()), "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row(vec!["Alice".to_string()]);
        let result = b.build().unwrap();
        assert!(result.sql.contains("\"users\""));
    }

    #[test]
    fn batch_insert_no_columns_error() {
        let b = BatchInsertBuilder::new(None, "users".to_string()).unwrap();
        assert!(b.build().is_err());
    }

    #[test]
    fn batch_insert_no_rows_error() {
        let mut b = BatchInsertBuilder::new(None, "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        assert!(b.build().is_err());
    }

    #[test]
    fn batch_insert_row_count() {
        let mut b = BatchInsertBuilder::new(None, "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row(vec!["Alice".to_string()]);
        b.add_row(vec!["Bob".to_string()]);
        assert_eq!(b.row_count(), 2);
    }

    #[test]
    fn batch_insert_add_rows_batch() {
        let mut b = BatchInsertBuilder::new(None, "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_rows(vec![
            vec!["Alice".to_string()],
            vec!["Bob".to_string()],
            vec!["Carol".to_string()],
        ]);
        assert_eq!(b.row_count(), 3);
    }

    #[test]
    fn batch_insert_clear() {
        let mut b = BatchInsertBuilder::new(None, "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row(vec!["Alice".to_string()]);
        b.clear();
        assert_eq!(b.row_count(), 0);
    }

    #[test]
    fn batch_insert_unknown_db_type() {
        assert!(BatchInsertBuilder::new(Some("unknown".to_string()), "t".to_string()).is_err());
    }

    #[test]
    fn batch_insert_or_ignore_mysql() {
        let mut b =
            BatchInsertBuilder::new(Some("mysql".to_string()), "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row(vec!["Alice".to_string()]);
        let result = b.build_or_ignore().unwrap();
        assert!(result.sql.contains("INSERT IGNORE"));
    }

    #[test]
    fn batch_insert_or_ignore_postgres() {
        let mut b =
            BatchInsertBuilder::new(Some("postgres".to_string()), "users".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row(vec!["Alice".to_string()]);
        let result = b.build_or_ignore().unwrap();
        assert!(result.sql.contains("ON CONFLICT DO NOTHING"));
    }

    #[test]
    fn batch_insert_multiple_placeholders() {
        let mut b = BatchInsertBuilder::new(None, "users".to_string()).unwrap();
        b.set_columns(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        b.add_row(vec!["1".to_string(), "2".to_string(), "3".to_string()]);
        let result = b.build().unwrap();
        assert!(result.sql.contains("(?, ?, ?)"));
    }

    // ----- BatchUpdateBuilder -----

    #[test]
    fn batch_update_basic() {
        let mut b = BatchUpdateBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.set_columns(vec!["name".to_string(), "age".to_string()]);
        b.add_row("1".to_string(), vec!["Alice".to_string(), "30".to_string()]);
        b.add_row("2".to_string(), vec!["Bob".to_string(), "25".to_string()]);
        let result = b.build().unwrap();
        assert_eq!(result.statement_count, 2);
        assert!(result.sqls[0].contains("UPDATE"));
        assert!(result.sqls[0].contains("SET"));
        assert!(result.sqls[0].contains("WHERE"));
    }

    #[test]
    fn batch_update_no_columns_error() {
        let b = BatchUpdateBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        assert!(b.build().is_err());
    }

    #[test]
    fn batch_update_no_rows_error() {
        let mut b = BatchUpdateBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        assert!(b.build().is_err());
    }

    #[test]
    fn batch_update_row_count() {
        let mut b = BatchUpdateBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row("1".to_string(), vec!["Alice".to_string()]);
        b.add_row("2".to_string(), vec!["Bob".to_string()]);
        assert_eq!(b.row_count(), 2);
    }

    #[test]
    fn batch_update_clear() {
        let mut b = BatchUpdateBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row("1".to_string(), vec!["Alice".to_string()]);
        b.clear();
        assert_eq!(b.row_count(), 0);
    }

    #[test]
    fn batch_update_postgres_quoting() {
        let mut b = BatchUpdateBuilder::new(
            Some("postgres".to_string()),
            "users".to_string(),
            "id".to_string(),
        )
        .unwrap();
        b.set_columns(vec!["name".to_string()]);
        b.add_row("1".to_string(), vec!["Alice".to_string()]);
        let result = b.build().unwrap();
        assert!(result.sqls[0].contains("\"users\""));
        assert!(result.sqls[0].contains("\"name\""));
    }

    // ----- BatchDeleteBuilder -----

    #[test]
    fn batch_delete_basic() {
        let mut b = BatchDeleteBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.add("1".to_string());
        b.add("2".to_string());
        b.add("3".to_string());
        let result = b.build().unwrap();
        assert!(result.sql.contains("DELETE FROM"));
        assert!(result.sql.contains("IN (?, ?, ?)"));
        assert_eq!(result.param_count, 3);
    }

    #[test]
    fn batch_delete_add_many() {
        let mut b = BatchDeleteBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.add_many(vec!["1".to_string(), "2".to_string(), "3".to_string()]);
        assert_eq!(b.count(), 3);
    }

    #[test]
    fn batch_delete_empty_error() {
        let b = BatchDeleteBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        assert!(b.build().is_err());
    }

    #[test]
    fn batch_delete_clear() {
        let mut b = BatchDeleteBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.add("1".to_string());
        b.clear();
        assert_eq!(b.count(), 0);
    }

    #[test]
    fn batch_delete_count() {
        let mut b = BatchDeleteBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.add("1".to_string());
        b.add("2".to_string());
        assert_eq!(b.count(), 2);
    }

    #[test]
    fn batch_delete_single() {
        let mut b = BatchDeleteBuilder::new(None, "users".to_string(), "id".to_string()).unwrap();
        b.add("42".to_string());
        let result = b.build().unwrap();
        assert!(result.sql.contains("IN (?)"));
        assert_eq!(result.param_count, 1);
    }

    // ----- BatchStats -----

    #[test]
    fn batch_stats_compute() {
        let stats = BatchStats::compute(10, 50);
        assert_eq!(stats.total_operations, 10);
        assert_eq!(stats.total_params, 50);
        assert_eq!(stats.avg_params_per_op, 5.0);
    }

    #[test]
    fn batch_stats_zero_ops() {
        let stats = BatchStats::compute(0, 0);
        assert_eq!(stats.avg_params_per_op, 0.0);
    }

    #[test]
    fn batch_stats_fractional_avg() {
        let stats = BatchStats::compute(3, 10);
        assert!((stats.avg_params_per_op - 3.3333333333333335).abs() < 0.001);
    }
}
