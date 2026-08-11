//! # 流式导出（`streaming-export` feature）
//!
//! 提供 `CsvExporter` 逐行导出 CSV，峰值内存 = 单行 + CSV 缓冲。

pub mod csv;

use serde::{Deserialize, Serialize};

/// 导出配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// 是否包含表头
    pub with_header: bool,
    /// CSV 分隔符（默认逗号）
    pub delimiter: char,
    /// 批次大小（行数）
    pub batch_size: usize,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            with_header: true,
            delimiter: ',',
            batch_size: 1000,
        }
    }
}

/// 导出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    /// 导出行数
    pub rows_exported: u64,
    /// 导出字节数
    pub bytes_written: u64,
}
