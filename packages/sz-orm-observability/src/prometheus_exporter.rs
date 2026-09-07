//! v6.7.0 Prometheus exposition format 导出。

use std::collections::HashMap;
use std::sync::Mutex;

pub struct PrometheusExporter {
    counters: Mutex<HashMap<String, (String, f64)>>,
    gauges: Mutex<HashMap<String, (String, f64)>>,
    histograms: Mutex<HashMap<String, (String, Vec<f64>)>>,
}

impl PrometheusExporter {
    pub fn new() -> Self {
        Self {
            counters: Mutex::new(HashMap::new()),
            gauges: Mutex::new(HashMap::new()),
            histograms: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_counter(&self, name: &str, help: &str) {
        self.counters
            .lock()
            .unwrap()
            .insert(name.to_string(), (help.to_string(), 0.0));
    }

    pub fn inc_counter(&self, name: &str, delta: f64) {
        if let Some(entry) = self.counters.lock().unwrap().get_mut(name) {
            entry.1 += delta;
        }
    }

    pub fn register_gauge(&self, name: &str, help: &str) {
        self.gauges
            .lock()
            .unwrap()
            .insert(name.to_string(), (help.to_string(), 0.0));
    }

    pub fn set_gauge(&self, name: &str, value: f64) {
        if let Some(entry) = self.gauges.lock().unwrap().get_mut(name) {
            entry.1 = value;
        }
    }

    pub fn register_histogram(&self, name: &str, help: &str) {
        self.histograms
            .lock()
            .unwrap()
            .insert(name.to_string(), (help.to_string(), Vec::new()));
    }

    pub fn observe_histogram(&self, name: &str, value: f64) {
        if let Some(entry) = self.histograms.lock().unwrap().get_mut(name) {
            entry.1.push(value);
        }
    }

    pub fn export(&self) -> String {
        let mut output = String::new();

        for (name, (help, value)) in self.counters.lock().unwrap().iter() {
            output.push_str(&format!("# HELP {} {}\n", name, help));
            output.push_str(&format!("# TYPE {} counter\n", name));
            output.push_str(&format!("{} {}\n", name, value));
        }

        for (name, (help, value)) in self.gauges.lock().unwrap().iter() {
            output.push_str(&format!("# HELP {} {}\n", name, help));
            output.push_str(&format!("# TYPE {} gauge\n", name));
            output.push_str(&format!("{} {}\n", name, value));
        }

        for (name, (help, observations)) in self.histograms.lock().unwrap().iter() {
            output.push_str(&format!("# HELP {} {}\n", name, help));
            output.push_str(&format!("# TYPE {} histogram\n", name));
            let count = observations.len();
            let sum: f64 = observations.iter().sum();
            output.push_str(&format!("{}_count {}\n", name, count));
            output.push_str(&format!("{}_sum {}\n", name, sum));
        }

        output
    }
}

impl Default for PrometheusExporter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn format_labels(labels: &HashMap<String, String>) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!("{}=\"{}\"", k, v))
        .collect();
    parts.sort();
    format!("{{{}}}", parts.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_counter_format() {
        let exporter = PrometheusExporter::new();
        exporter.register_counter("sz_orm_query_total", "Total queries");
        exporter.inc_counter("sz_orm_query_total", 42.0);
        let output = exporter.export();
        assert!(output.contains("# HELP sz_orm_query_total Total queries"));
        assert!(output.contains("# TYPE sz_orm_query_total counter"));
        assert!(output.contains("sz_orm_query_total 42"));
    }

    #[test]
    fn export_gauge_format() {
        let exporter = PrometheusExporter::new();
        exporter.register_gauge("sz_orm_pool_size", "Pool size");
        exporter.set_gauge("sz_orm_pool_size", 15.0);
        let output = exporter.export();
        assert!(output.contains("# TYPE sz_orm_pool_size gauge"));
        assert!(output.contains("sz_orm_pool_size 15"));
    }

    #[test]
    fn export_histogram_format() {
        let exporter = PrometheusExporter::new();
        exporter.register_histogram("sz_orm_query_duration", "Query duration");
        exporter.observe_histogram("sz_orm_query_duration", 0.1);
        exporter.observe_histogram("sz_orm_query_duration", 0.3);
        let output = exporter.export();
        assert!(output.contains("# TYPE sz_orm_query_duration histogram"));
        assert!(output.contains("sz_orm_query_duration_count 2"));
        assert!(output.contains("sz_orm_query_duration_sum 0.4"));
    }

    #[test]
    fn format_labels_empty() {
        let labels = HashMap::new();
        assert_eq!(format_labels(&labels), "");
    }

    #[test]
    fn format_labels_sorted() {
        let mut labels = HashMap::new();
        labels.insert("b".to_string(), "2".to_string());
        labels.insert("a".to_string(), "1".to_string());
        let formatted = format_labels(&labels);
        assert!(formatted.contains("a=\"1\""));
        assert!(formatted.contains("b=\"2\""));
    }

    #[test]
    fn multiple_metrics_export() {
        let exporter = PrometheusExporter::new();
        exporter.register_counter("c1", "counter 1");
        exporter.register_gauge("g1", "gauge 1");
        exporter.inc_counter("c1", 10.0);
        exporter.set_gauge("g1", 5.0);
        let output = exporter.export();
        assert!(output.contains("c1 10"));
        assert!(output.contains("g1 5"));
    }

    #[test]
    fn wiring_exporter_usable() {
        let exporter = PrometheusExporter::new();
        exporter.register_counter("test", "test help");
        let output = exporter.export();
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
    }
}
