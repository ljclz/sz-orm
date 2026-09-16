//! NL 查询结果格式化器
//!
//! 将 SQL 查询结果格式化为用户可读的形式。

use std::collections::HashMap;

/// 格式化后的查询结果
#[derive(Debug, Clone)]
pub struct FormattedResult {
    /// 表格头（列名）
    pub headers: Vec<String>,
    /// 数据行
    pub rows: Vec<Vec<String>>,
    /// 结果摘要
    pub summary: String,
}

/// NL 查询结果格式化器
pub struct NlResultFormatter {
    /// 最大显示行数
    max_display_rows: usize,
    /// 是否显示行号
    show_row_numbers: bool,
}

impl Default for NlResultFormatter {
    fn default() -> Self {
        Self::new(50)
    }
}

impl NlResultFormatter {
    /// 创建格式化器
    pub fn new(max_display_rows: usize) -> Self {
        Self {
            max_display_rows,
            show_row_numbers: true,
        }
    }

    /// 隐藏行号
    pub fn without_row_numbers(mut self) -> Self {
        self.show_row_numbers = false;
        self
    }

    /// 格式化查询结果
    pub fn format(&self, headers: &[String], rows: &[Vec<String>]) -> FormattedResult {
        let total = rows.len();
        let display_rows: Vec<Vec<String>> = rows
            .iter()
            .take(self.max_display_rows)
            .enumerate()
            .map(|(idx, row)| {
                if self.show_row_numbers {
                    let mut r = vec![(idx + 1).to_string()];
                    r.extend(row.iter().cloned());
                    r
                } else {
                    row.clone()
                }
            })
            .collect();

        let mut display_headers = Vec::new();
        if self.show_row_numbers {
            display_headers.push("#".to_string());
        }
        display_headers.extend(headers.iter().cloned());

        let summary = if total > self.max_display_rows {
            format!(
                "共 {} 行，显示前 {} 行（省略 {} 行）",
                total,
                self.max_display_rows,
                total - self.max_display_rows
            )
        } else if total == 0 {
            "查询返回 0 行".to_string()
        } else {
            format!("共 {} 行", total)
        };

        FormattedResult {
            headers: display_headers,
            rows: display_rows,
            summary,
        }
    }

    /// 格式化为 Markdown 表格
    pub fn to_markdown(&self, result: &FormattedResult) -> String {
        let mut sb = String::new();
        sb.push_str("| ");
        sb.push_str(&result.headers.join(" | "));
        sb.push_str(" |\n");
        sb.push_str("| ");
        sb.push_str(
            &result
                .headers
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | "),
        );
        sb.push_str(" |\n");
        for row in &result.rows {
            sb.push_str("| ");
            sb.push_str(&row.join(" | "));
            sb.push_str(" |\n");
        }
        sb.push_str(&format!("\n*{}*", result.summary));
        sb
    }

    /// 从 HashMap 行构建
    pub fn format_from_maps(
        &self,
        headers: &[String],
        rows: &[HashMap<String, String>],
    ) -> FormattedResult {
        let string_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|map| {
                headers
                    .iter()
                    .map(|h| map.get(h).cloned().unwrap_or_default())
                    .collect()
            })
            .collect();
        self.format(headers, &string_rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_basic() {
        let fmt = NlResultFormatter::default();
        let result = fmt.format(
            &["name".into(), "age".into()],
            &[
                vec!["Alice".into(), "30".into()],
                vec!["Bob".into(), "25".into()],
            ],
        );
        assert_eq!(result.headers, vec!["#", "name", "age"]);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0][0], "1");
        assert!(result.summary.contains("2 行"));
    }

    #[test]
    fn test_format_empty() {
        let fmt = NlResultFormatter::default();
        let result = fmt.format(&["name".into()], &[]);
        assert_eq!(result.rows.len(), 0);
        assert!(result.summary.contains("0 行"));
    }

    #[test]
    fn test_format_truncated() {
        let fmt = NlResultFormatter::new(2);
        let rows: Vec<Vec<String>> = (0..10).map(|i| vec![format!("user{}", i)]).collect();
        let result = fmt.format(&["name".into()], &rows);
        assert_eq!(result.rows.len(), 2);
        assert!(result.summary.contains("省略"));
    }

    #[test]
    fn test_without_row_numbers() {
        let fmt = NlResultFormatter::default().without_row_numbers();
        let result = fmt.format(&["name".into()], &[vec!["Alice".into()]]);
        assert_eq!(result.headers, vec!["name"]);
        assert_eq!(result.rows[0], vec!["Alice"]);
    }

    #[test]
    fn test_to_markdown() {
        let fmt = NlResultFormatter::default();
        let result = fmt.format(&["name".into()], &[vec!["Alice".into()]]);
        let md = fmt.to_markdown(&result);
        assert!(md.contains("| # | name |"));
        assert!(md.contains("| 1 | Alice |"));
    }

    #[test]
    fn test_format_from_maps() {
        let fmt = NlResultFormatter::default().without_row_numbers();
        let mut map = HashMap::new();
        map.insert("name".into(), "Alice".into());
        let result = fmt.format_from_maps(&["name".into()], &[map]);
        assert_eq!(result.rows[0], vec!["Alice"]);
    }
}
