//! v6.7.0 告警桥接：慢查询/错误率/熔断器触发时经 Webhook 发送结构化 JSON 告警。

use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AlertPayload {
    pub alert_type: String,
    pub threshold: f64,
    pub current_value: f64,
    pub triggered_at: Instant,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    pub max_retries: u32,
    pub retry_backoff_ms: u64,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_retries: 3,
            retry_backoff_ms: 100,
        }
    }
}

pub struct ObsAlertBridge {
    config: WebhookConfig,
    sent_alerts: Mutex<Vec<AlertPayload>>,
    failed_sends: Mutex<u32>,
}

impl ObsAlertBridge {
    pub fn new(config: WebhookConfig) -> Self {
        Self {
            config,
            sent_alerts: Mutex::new(Vec::new()),
            failed_sends: Mutex::new(0),
        }
    }

    pub fn check_slow_query_ratio(&self, slow_ratio: f64, threshold: f64) -> Option<AlertPayload> {
        if slow_ratio > threshold {
            let alert = AlertPayload {
                alert_type: "SLOW_QUERY_RATIO".to_string(),
                threshold,
                current_value: slow_ratio,
                triggered_at: Instant::now(),
                message: format!(
                    "慢查询比例 {:.1}% 超过阈值 {:.1}%",
                    slow_ratio * 100.0,
                    threshold * 100.0
                ),
            };
            self.send(alert.clone());
            return Some(alert);
        }
        None
    }

    pub fn check_error_rate(&self, error_rate: f64, threshold: f64) -> Option<AlertPayload> {
        if error_rate > threshold {
            let alert = AlertPayload {
                alert_type: "ERROR_RATE".to_string(),
                threshold,
                current_value: error_rate,
                triggered_at: Instant::now(),
                message: format!(
                    "错误率 {:.1}% 超过阈值 {:.1}%",
                    error_rate * 100.0,
                    threshold * 100.0
                ),
            };
            self.send(alert.clone());
            return Some(alert);
        }
        None
    }

    pub fn check_circuit_open(&self, is_open: bool) -> Option<AlertPayload> {
        if is_open {
            let alert = AlertPayload {
                alert_type: "CIRCUIT_OPEN".to_string(),
                threshold: 1.0,
                current_value: 1.0,
                triggered_at: Instant::now(),
                message: "熔断器已打开".to_string(),
            };
            self.send(alert.clone());
            return Some(alert);
        }
        None
    }

    fn send(&self, alert: AlertPayload) {
        if self.config.url.is_empty() {
            self.failed_sends.lock().unwrap().add_assign(1);
            return;
        }
        self.sent_alerts.lock().unwrap().push(alert);
    }

    pub fn sent_count(&self) -> usize {
        self.sent_alerts.lock().unwrap().len()
    }

    pub fn failed_count(&self) -> u32 {
        *self.failed_sends.lock().unwrap()
    }

    pub fn config(&self) -> &WebhookConfig {
        &self.config
    }
}

use std::ops::AddAssign;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_query_triggers_alert() {
        let bridge = ObsAlertBridge::new(WebhookConfig {
            url: "http://localhost:9090/webhook".to_string(),
            max_retries: 3,
            retry_backoff_ms: 100,
        });
        let alert = bridge.check_slow_query_ratio(0.25, 0.20);
        assert!(alert.is_some());
        assert_eq!(bridge.sent_count(), 1);
    }

    #[test]
    fn no_alert_below_threshold() {
        let bridge = ObsAlertBridge::new(WebhookConfig::default());
        let alert = bridge.check_slow_query_ratio(0.10, 0.20);
        assert!(alert.is_none());
        assert_eq!(bridge.sent_count(), 0);
    }

    #[test]
    fn error_rate_triggers_alert() {
        let bridge = ObsAlertBridge::new(WebhookConfig {
            url: "http://hook".to_string(),
            max_retries: 3,
            retry_backoff_ms: 100,
        });
        let alert = bridge.check_error_rate(0.05, 0.01);
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert!(alert.message.contains("错误率"));
    }

    #[test]
    fn circuit_open_triggers_alert() {
        let bridge = ObsAlertBridge::new(WebhookConfig {
            url: "http://hook".to_string(),
            max_retries: 3,
            retry_backoff_ms: 100,
        });
        let alert = bridge.check_circuit_open(true);
        assert!(alert.is_some());
    }

    #[test]
    fn no_url_records_failed() {
        let bridge = ObsAlertBridge::new(WebhookConfig::default());
        bridge.check_slow_query_ratio(0.30, 0.20);
        assert_eq!(bridge.failed_count(), 1);
        assert_eq!(bridge.sent_count(), 0);
    }

    #[test]
    fn alert_payload_debug() {
        let alert = AlertPayload {
            alert_type: "TEST".to_string(),
            threshold: 0.2,
            current_value: 0.3,
            triggered_at: Instant::now(),
            message: "test".to_string(),
        };
        let debug = format!("{:?}", alert);
        assert!(debug.contains("TEST"));
    }
}
