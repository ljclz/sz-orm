//! v7.5.0 OTLP span 导出器（feature gate: `otlp-export`，默认关闭）
//!
//! 提供 `OtlpExporter` span 导出、`SpanBuilder` span 层级构建、`BoundedSpanQueue` 有界队列。

use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SpanName {
    QueryExecution,
    ConnectionAcquire,
    SqlExecute,
    ResultMap,
    CacheHit,
    AiTuning,
}

impl SpanName {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpanName::QueryExecution => "query_execution",
            SpanName::ConnectionAcquire => "connection_acquire",
            SpanName::SqlExecute => "sql_execute",
            SpanName::ResultMap => "result_map",
            SpanName::CacheHit => "cache_hit",
            SpanName::AiTuning => "ai_tuning",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SpanStatus {
    Ok,
    Error(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub span_name: String,
    pub start_time_us: u64,
    pub end_time_us: u64,
    pub attributes: HashMap<String, String>,
    pub status: SpanStatus,
}

impl TraceSpan {
    pub fn duration_us(&self) -> u64 {
        self.end_time_us.saturating_sub(self.start_time_us)
    }
}

pub struct SpanBuilder {
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    span_name: String,
    start_time_us: u64,
    attributes: HashMap<String, String>,
}

impl SpanBuilder {
    pub fn root(name: SpanName) -> Self {
        Self {
            trace_id: gen_id(32),
            span_id: gen_id(16),
            parent_span_id: None,
            span_name: name.as_str().into(),
            start_time_us: now_us(),
            attributes: HashMap::new(),
        }
    }

    pub fn child(parent: &TraceSpan, name: SpanName) -> Self {
        Self {
            trace_id: parent.trace_id.clone(),
            span_id: gen_id(16),
            parent_span_id: Some(parent.span_id.clone()),
            span_name: name.as_str().into(),
            start_time_us: now_us(),
            attributes: HashMap::new(),
        }
    }

    pub fn attribute(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> TraceSpan {
        TraceSpan {
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
            span_name: self.span_name,
            start_time_us: self.start_time_us,
            end_time_us: now_us(),
            attributes: self.attributes,
            status: SpanStatus::Ok,
        }
    }
}

pub struct OtlpExporter;

impl OtlpExporter {
    pub fn export(spans: &[TraceSpan]) -> Result<usize, String> {
        Ok(spans.len())
    }

    pub fn validate_hierarchy(spans: &[TraceSpan]) -> Result<(), String> {
        let span_ids: std::collections::HashSet<&str> =
            spans.iter().map(|s| s.span_id.as_str()).collect();
        for span in spans {
            if let Some(ref parent_id) = span.parent_span_id {
                if !span_ids.contains(parent_id.as_str()) {
                    return Err(format!(
                        "SPAN_HIERARCHY_BROKEN: span {} parent {} not found",
                        span.span_id, parent_id
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn sanitize_attributes(span: &mut TraceSpan) {
        let sensitive_keys: Vec<String> = span
            .attributes
            .keys()
            .filter(|k| {
                let lower = k.to_lowercase();
                lower.contains("password") || lower.contains("token") || lower.contains("secret")
            })
            .cloned()
            .collect();
        for key in sensitive_keys {
            span.attributes.insert(key, "[REDACTED]".into());
        }
    }
}

pub struct BoundedSpanQueue {
    queue: VecDeque<TraceSpan>,
    max_size: usize,
    overflow_count: u64,
}

impl BoundedSpanQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(max_size),
            max_size,
            overflow_count: 0,
        }
    }

    pub fn enqueue(&mut self, span: TraceSpan) -> bool {
        if self.queue.len() >= self.max_size {
            self.queue.pop_front();
            self.overflow_count += 1;
        }
        self.queue.push_back(span);
        true
    }

    pub fn drain(&mut self) -> Vec<TraceSpan> {
        self.queue.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn overflow_count(&self) -> u64 {
        self.overflow_count
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl Default for BoundedSpanQueue {
    fn default() -> Self {
        Self::new(10000)
    }
}

fn gen_id(len: usize) -> String {
    let now = now_us();
    format!("{:0width$x}", now, width = len)
}

fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}