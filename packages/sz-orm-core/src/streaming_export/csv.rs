//! # CSV 导出器
//!
//! 基于 `csv` crate 逐行写出，峰值内存 = 单行 + CSV 缓冲。

use super::{ExportConfig, ExportResult};
use crate::value::Value;
use std::io::Write;

/// CSV 导出器
pub struct CsvExporter<W: Write> {
    writer: csv::Writer<W>,
    config: ExportConfig,
    header_written: bool,
}

impl<W: Write> CsvExporter<W> {
    /// 创建 CSV 导出器
    pub fn new(writer: W, config: ExportConfig) -> Self {
        let csv_writer = csv::WriterBuilder::new()
            .delimiter(config.delimiter as u8)
            .has_headers(config.with_header)
            .from_writer(writer);
        Self {
            writer: csv_writer,
            config,
            header_written: false,
        }
    }

    /// 写入表头
    pub fn write_header(&mut self, columns: &[String]) -> std::io::Result<()> {
        if self.config.with_header && !self.header_written {
            self.writer.write_record(columns)?;
            self.header_written = true;
        }
        Ok(())
    }

    /// 写入一行数据
    pub fn write_row(&mut self, row: &[Value]) -> std::io::Result<()> {
        let fields: Vec<String> = row.iter().map(value_to_csv_string).collect();
        self.writer.write_record(&fields)?;
        Ok(())
    }

    /// 完成导出，返回结果
    pub fn finish(mut self) -> std::io::Result<ExportResult> {
        self.writer.flush()?;
        Ok(ExportResult {
            rows_exported: 0,
            bytes_written: 0,
        })
    }

    /// 导出所有行（从迭代器）
    pub fn export<I>(&mut self, columns: &[String], rows: I) -> std::io::Result<ExportResult>
    where
        I: IntoIterator<Item = Vec<Value>>,
    {
        self.write_header(columns)?;
        let mut count = 0u64;
        for row in rows {
            self.write_row(&row)?;
            count += 1;
        }
        self.writer.flush()?;
        Ok(ExportResult {
            rows_exported: count,
            bytes_written: 0,
        })
    }
}

/// 将 Value 转为 CSV 字符串
fn value_to_csv_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::I64(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::String(s) => s.clone(),
        Value::Bytes(b) => format!("{:?}", b),
        Value::Date(s) => s.clone(),
        Value::DateTime(s) => s.clone(),
        Value::Json(s) => s.clone(),
        Value::Decimal(s) => s.clone(),
        _ => format!("{:?}", v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming_export::{ExportConfig, ExportResult};

    #[test]
    fn test_csv_export_basic() {
        let config = ExportConfig::default();
        let mut exporter = CsvExporter::new(Vec::new(), config);
        let columns = vec!["id".to_string(), "name".to_string()];
        let rows = vec![
            vec![Value::I64(1), Value::String("Alice".to_string())],
            vec![Value::I64(2), Value::String("Bob".to_string())],
        ];
        let result = exporter.export(&columns, rows).unwrap();
        assert_eq!(result.rows_exported, 2);
    }

    #[test]
    fn test_csv_export_no_header() {
        let config = ExportConfig {
            with_header: false,
            delimiter: ',',
            batch_size: 100,
        };
        let mut exporter = CsvExporter::new(Vec::new(), config);
        let columns = vec!["id".to_string()];
        let rows = vec![vec![Value::I64(1)]];
        let result = exporter.export(&columns, rows).unwrap();
        assert_eq!(result.rows_exported, 1);
    }

    #[test]
    fn test_csv_export_empty() {
        let config = ExportConfig::default();
        let mut exporter = CsvExporter::new(Vec::new(), config);
        let columns = vec!["id".to_string()];
        let rows: Vec<Vec<Value>> = vec![];
        let result = exporter.export(&columns, rows).unwrap();
        assert_eq!(result.rows_exported, 0);
    }

    #[test]
    fn test_csv_export_semicolon_delimiter() {
        let config = ExportConfig {
            with_header: true,
            delimiter: ';',
            batch_size: 100,
        };
        let mut exporter = CsvExporter::new(Vec::new(), config);
        let columns = vec!["id".to_string(), "name".to_string()];
        let rows = vec![vec![Value::I64(1), Value::String("Alice".to_string())]];
        let result = exporter.export(&columns, rows).unwrap();
        assert_eq!(result.rows_exported, 1);
    }

    #[test]
    fn test_value_to_csv_string() {
        assert_eq!(value_to_csv_string(&Value::Null), "");
        assert_eq!(value_to_csv_string(&Value::I64(42)), "42");
        assert_eq!(value_to_csv_string(&Value::F64(3.14)), "3.14");
        assert_eq!(value_to_csv_string(&Value::Bool(true)), "true");
        assert_eq!(
            value_to_csv_string(&Value::String("hello".to_string())),
            "hello"
        );
    }
}