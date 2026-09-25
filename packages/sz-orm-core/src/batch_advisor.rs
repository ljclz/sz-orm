//! v8.8.0 BatchSizeAdvisor — 批量操作自适应分批建议
//!
//! 根据数据库类型、列数、总行数和参数上限计算最优批量大小，
//! 避免超过数据库参数绑定上限，同时最大化批量吞吐量。

use crate::dialect::DbType;

/// 数据库参数绑定上限
fn param_limit(db_type: DbType) -> usize {
    match db_type {
        DbType::MySQL => 65535,
        DbType::PostgreSQL => 32767,
        DbType::Oracle => 1000,
        DbType::Sqlite => 999,
        DbType::SqlServer => 2100,
        DbType::ClickHouse => 65535,
        DbType::Db2 => 32767,
        _ => 1000,
    }
}

/// 批量大小建议器
pub struct BatchSizeAdvisor;

impl BatchSizeAdvisor {
    /// 根据数据库类型、列数、总行数计算最优批量大小
    ///
    /// 约束：
    /// - batch_size * columns <= param_limit（不超参数上限）
    /// - batch_size <= total_rows（不超过总行数）
    /// - batch_size >= 1（至少 1 行）
    /// - 优先 500 行，其次受参数上限限制
    pub fn advise(db_type: DbType, columns: usize, total_rows: usize) -> usize {
        if columns == 0 || total_rows == 0 {
            return 1;
        }
        let limit = param_limit(db_type);
        let max_by_params = limit / columns;
        let ideal = 500usize;
        let batch_size = ideal.min(max_by_params).min(total_rows);
        batch_size.max(1)
    }

    /// 将行列表按建议的批量大小分片，返回每片的 (start, end) 索引范围
    pub fn batch_ranges(db_type: DbType, columns: usize, total_rows: usize) -> Vec<(usize, usize)> {
        let batch_size = Self::advise(db_type, columns, total_rows);
        let mut ranges = Vec::with_capacity(total_rows.div_ceil(batch_size));
        let mut start = 0;
        while start < total_rows {
            let end = (start + batch_size).min(total_rows);
            ranges.push((start, end));
            start = end;
        }
        ranges
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advise_respects_param_limit() {
        let batch = BatchSizeAdvisor::advise(DbType::Oracle, 10, 10000);
        assert!(
            batch * 10 <= 1000,
            "batch * columns must not exceed param limit"
        );
    }

    #[test]
    fn test_advise_respects_total_rows() {
        let batch = BatchSizeAdvisor::advise(DbType::MySQL, 3, 100);
        assert!(batch <= 100, "batch must not exceed total rows");
    }

    #[test]
    fn test_advise_minimum_one() {
        let batch = BatchSizeAdvisor::advise(DbType::Sqlite, 0, 100);
        assert_eq!(batch, 1, "zero columns should return 1");
    }

    #[test]
    fn test_advise_ideal_500() {
        let batch = BatchSizeAdvisor::advise(DbType::MySQL, 2, 10000);
        assert_eq!(batch, 500, "should use ideal 500 when no constraint hit");
    }

    #[test]
    fn test_batch_ranges_cover_all() {
        let ranges = BatchSizeAdvisor::batch_ranges(DbType::MySQL, 2, 1000);
        let covered: usize = ranges.iter().map(|(s, e)| e - s).sum();
        assert_eq!(covered, 1000, "ranges must cover all rows");
    }
}
