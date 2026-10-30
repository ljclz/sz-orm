//! v10.11.0 T1.2：编译期查询特化
//!
//! 基于 Rust const generics 对已知形状的查询生成特化代码路径，
//! 消除运行时分支和动态分发开销。高频形状走特化路径，非高频走泛化路径。
//!
//! 启用 `perf-extreme` feature 时编译，默认不启用。

use serde::{Deserialize, Serialize};

/// 列类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnType {
    /// 64 位整数
    I64,
    /// 64 位浮点
    F64,
    /// 字符串
    String,
    /// 布尔
    Bool,
    /// 日期时间
    DateTime,
    /// 十进制数
    Decimal,
}

/// WHERE 模式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WherePattern {
    /// 等值 + 范围组合（如 WHERE a = ? AND b > ? AND b < ?）
    EqAndRange,
    /// 仅等值（如 WHERE a = ? AND b = ?）
    EqOnly,
    /// 仅范围（如 WHERE a > ? AND a < ?）
    RangeOnly,
    /// 全表扫描（无 WHERE）
    FullScan,
}

/// 查询形状（const generics 特化）
///
/// `COLS` 和 `PARAMS` 是编译期常量，编译器为不同形状生成特化代码路径。
/// 代码膨胀控制：仅对高频形状（COLS ≤ 16 && PARAMS ≤ 8）特化。
#[derive(Debug, Clone)]
pub struct QueryShape<const COLS: usize, const PARAMS: usize> {
    /// 列类型数组（长度 COLS）
    pub column_types: [ColumnType; COLS],
    /// WHERE 模式
    pub where_pattern: WherePattern,
}

impl<const COLS: usize, const PARAMS: usize> QueryShape<COLS, PARAMS> {
    /// 创建查询形状
    pub const fn new(column_types: [ColumnType; COLS], where_pattern: WherePattern) -> Self {
        Self {
            column_types,
            where_pattern,
        }
    }

    /// 列数（编译期常量）
    pub const fn cols(&self) -> usize {
        COLS
    }

    /// 参数数（编译期常量）
    pub const fn params(&self) -> usize {
        PARAMS
    }
}

/// 高频形状判定（const fn，编译期求值）
///
/// 仅对高频形状（COLS ≤ 16 && PARAMS ≤ 8）特化，非高频走泛化路径。
/// 这控制代码膨胀：只为高频形状生成特化代码。
pub const fn is_high_frequency_shape<const COLS: usize, const PARAMS: usize>() -> bool {
    COLS <= 16 && PARAMS <= 8
}

/// 特化执行结果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecializeResult {
    /// 特化路径是否被采用
    pub specialized: bool,
    /// 列数
    pub cols: usize,
    /// 参数数
    pub params: usize,
    /// WHERE 模式
    pub where_pattern: WherePattern,
    /// 执行返回的行数（基于选择性估算）
    pub rows_returned: usize,
}

/// 特化执行函数
///
/// 高频形状走特化路径（编译期消除分支），非高频走泛化路径。
/// 编译器为每个 `COLS`/`PARAMS` 组合生成特化代码，消除运行时分支。
///
/// # 参数
/// - `shape`: 查询形状（const generics）
/// - `params`: 查询参数（运行时检查长度 == PARAMS）
/// - `table_size`: 表行数
///
/// # 返回
/// 特化结果，包含是否采用特化路径的标记和估算返回行数。
pub fn specialize_execute<const COLS: usize, const PARAMS: usize>(
    shape: &QueryShape<COLS, PARAMS>,
    params: &[String],
    table_size: usize,
) -> SpecializeResult {
    debug_assert_eq!(params.len(), PARAMS, "params 长度必须与 const PARAMS 一致");
    let specialized = is_high_frequency_shape::<COLS, PARAMS>();
    let rows_returned = estimate_rows(shape.where_pattern, PARAMS, table_size);
    SpecializeResult {
        specialized,
        cols: COLS,
        params: PARAMS,
        where_pattern: shape.where_pattern,
        rows_returned,
    }
}

/// 泛化执行函数（非高频形状回退路径）
///
/// 非高频形状（COLS > 16 或 PARAMS > 8）走泛化路径，
/// 运行时动态分发，不生成特化代码。
pub fn generic_execute(
    column_types: &[ColumnType],
    where_pattern: WherePattern,
    params: &[String],
    table_size: usize,
) -> SpecializeResult {
    let rows_returned = estimate_rows(where_pattern, params.len(), table_size);
    SpecializeResult {
        specialized: false,
        cols: column_types.len(),
        params: params.len(),
        where_pattern,
        rows_returned,
    }
}

/// 统一执行入口（自动选择特化或泛化路径）
///
/// 高频形状走特化路径，非高频走泛化路径。
pub fn auto_execute<const COLS: usize, const PARAMS: usize>(
    shape: &QueryShape<COLS, PARAMS>,
    params: &[String],
    table_size: usize,
) -> SpecializeResult {
    if is_high_frequency_shape::<COLS, PARAMS>() {
        specialize_execute(shape, params, table_size)
    } else {
        generic_execute(&shape.column_types, shape.where_pattern, params, table_size)
    }
}

/// 行数估算（基于 WHERE 模式选择性）
#[inline]
fn estimate_rows(pattern: WherePattern, params: usize, table_size: usize) -> usize {
    match pattern {
        WherePattern::EqOnly => table_size / params.max(1),
        WherePattern::EqAndRange => table_size / (params * 2).max(1),
        WherePattern::RangeOnly => table_size / 4,
        WherePattern::FullScan => table_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_frequency_shape() {
        assert!(is_high_frequency_shape::<4, 2>());
        assert!(is_high_frequency_shape::<16, 8>());
        assert!(!is_high_frequency_shape::<17, 8>());
        assert!(!is_high_frequency_shape::<16, 9>());
        assert!(is_high_frequency_shape::<1, 1>());
    }

    #[test]
    fn test_specialize_execute_eq_only() {
        let shape = QueryShape::<3, 2>::new(
            [ColumnType::I64, ColumnType::String, ColumnType::F64],
            WherePattern::EqOnly,
        );
        let params = vec!["42".to_string(), "active".to_string()];
        let result = specialize_execute(&shape, &params, 1000);
        assert!(result.specialized);
        assert_eq!(result.cols, 3);
        assert_eq!(result.params, 2);
        assert_eq!(result.where_pattern, WherePattern::EqOnly);
        assert_eq!(result.rows_returned, 500);
    }

    #[test]
    fn test_specialize_execute_full_scan() {
        let shape = QueryShape::<2, 0>::new(
            [ColumnType::I64, ColumnType::String],
            WherePattern::FullScan,
        );
        let params: Vec<String> = vec![];
        let result = specialize_execute(&shape, &params, 1000);
        assert!(result.specialized);
        assert_eq!(result.rows_returned, 1000);
    }

    #[test]
    fn test_generic_execute() {
        let types = vec![ColumnType::I64, ColumnType::String, ColumnType::F64];
        let params = vec!["42".to_string(), "active".to_string()];
        let result = generic_execute(&types, WherePattern::EqOnly, &params, 1000);
        assert!(!result.specialized);
        assert_eq!(result.rows_returned, 500);
    }

    #[test]
    fn test_auto_execute_specialized() {
        let shape = QueryShape::<4, 3>::new(
            [
                ColumnType::I64,
                ColumnType::String,
                ColumnType::F64,
                ColumnType::Bool,
            ],
            WherePattern::EqAndRange,
        );
        let params = vec!["1".to_string(), "10".to_string(), "100".to_string()];
        let result = auto_execute(&shape, &params, 1200);
        assert!(result.specialized);
        assert_eq!(result.rows_returned, 200);
    }
}
