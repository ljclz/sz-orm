//! v7.0.0 窗口分配器
//!
//! Tumbling / Sliding / Session 窗口 + 水位线推进。

use std::time::Duration;

/// 窗口类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// 滚动窗口（不重叠）
    Tumbling,
    /// 滑动窗口（重叠）
    Sliding,
    /// 会话窗口（活跃间隙）
    Session,
}

/// 窗口配置
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub window_type: WindowType,
    pub window_size: Duration,
    pub slide_size: Option<Duration>,
}

impl WindowConfig {
    pub fn tumbling(size: Duration) -> Self {
        Self {
            window_type: WindowType::Tumbling,
            window_size: size,
            slide_size: None,
        }
    }

    pub fn sliding(size: Duration, slide: Duration) -> Self {
        Self {
            window_type: WindowType::Sliding,
            window_size: size,
            slide_size: Some(slide),
        }
    }

    pub fn session(size: Duration) -> Self {
        Self {
            window_type: WindowType::Session,
            window_size: size,
            slide_size: None,
        }
    }
}

/// 窗口
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl Window {
    pub fn contains(&self, ts_ms: i64) -> bool {
        ts_ms >= self.start_ms && ts_ms < self.end_ms
    }
}

/// 事件
#[derive(Debug, Clone)]
pub struct Event {
    pub event_time_ms: i64,
    pub data: serde_json::Value,
}

impl Event {
    pub fn new(event_time_ms: i64, data: serde_json::Value) -> Self {
        Self {
            event_time_ms,
            data,
        }
    }
}

/// 水位线
#[derive(Debug, Clone)]
pub struct Watermark {
    pub timestamp_ms: i64,
}

impl Watermark {
    pub fn new(timestamp_ms: i64) -> Self {
        Self { timestamp_ms }
    }

    pub fn advance(&mut self, ts_ms: i64) {
        if ts_ms > self.timestamp_ms {
            self.timestamp_ms = ts_ms;
        }
    }
}

/// 窗口分配器
pub struct WindowAssigner {
    config: WindowConfig,
}

impl WindowAssigner {
    pub fn new(config: WindowConfig) -> Self {
        Self { config }
    }

    /// 分配窗口
    pub fn assign(&self, event: &Event) -> Vec<Window> {
        match self.config.window_type {
            WindowType::Tumbling => self.assign_tumbling(event),
            WindowType::Sliding => self.assign_sliding(event),
            WindowType::Session => self.assign_session(event),
        }
    }

    fn assign_tumbling(&self, event: &Event) -> Vec<Window> {
        let size = self.config.window_size.as_millis() as i64;
        let start = (event.event_time_ms / size) * size;
        vec![Window {
            start_ms: start,
            end_ms: start + size,
        }]
    }

    fn assign_sliding(&self, event: &Event) -> Vec<Window> {
        let size = self.config.window_size.as_millis() as i64;
        let slide = self
            .config
            .slide_size
            .unwrap_or(self.config.window_size)
            .as_millis() as i64;
        let mut windows = Vec::new();
        let first_start = (event.event_time_ms / slide) * slide - size + slide;
        let mut start = first_start;
        while start <= event.event_time_ms {
            windows.push(Window {
                start_ms: start,
                end_ms: start + size,
            });
            start += slide;
        }
        windows
    }

    fn assign_session(&self, event: &Event) -> Vec<Window> {
        let size = self.config.window_size.as_millis() as i64;
        vec![Window {
            start_ms: event.event_time_ms,
            end_ms: event.event_time_ms + size,
        }]
    }

    /// 检查窗口是否应触发
    pub fn should_trigger(&self, window: &Window, watermark: &Watermark) -> bool {
        watermark.timestamp_ms >= window.end_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tumbling_non_overlapping() {
        let assigner = WindowAssigner::new(WindowConfig::tumbling(Duration::from_millis(100)));
        let e1 = Event::new(50, serde_json::json!(1));
        let e2 = Event::new(150, serde_json::json!(2));
        let w1 = assigner.assign(&e1);
        let w2 = assigner.assign(&e2);
        assert_eq!(w1[0].start_ms, 0);
        assert_eq!(w1[0].end_ms, 100);
        assert_eq!(w2[0].start_ms, 100);
        assert_eq!(w2[0].end_ms, 200);
    }

    #[test]
    fn test_sliding_overlapping() {
        let assigner = WindowAssigner::new(WindowConfig::sliding(
            Duration::from_millis(100),
            Duration::from_millis(50),
        ));
        let e = Event::new(120, serde_json::json!(1));
        let windows = assigner.assign(&e);
        assert!(windows.len() > 1);
    }

    #[test]
    fn test_session_window() {
        let assigner = WindowAssigner::new(WindowConfig::session(Duration::from_millis(5000)));
        let e = Event::new(1000, serde_json::json!(1));
        let w = assigner.assign(&e);
        assert_eq!(w[0].start_ms, 1000);
        assert_eq!(w[0].end_ms, 6000);
    }

    #[test]
    fn test_watermark_advance() {
        let mut wm = Watermark::new(100);
        wm.advance(200);
        assert_eq!(wm.timestamp_ms, 200);
        wm.advance(150);
        assert_eq!(wm.timestamp_ms, 200);
    }

    #[test]
    fn test_should_trigger() {
        let assigner = WindowAssigner::new(WindowConfig::tumbling(Duration::from_millis(100)));
        let window = Window {
            start_ms: 0,
            end_ms: 100,
        };
        let wm = Watermark::new(100);
        assert!(assigner.should_trigger(&window, &wm));
        let wm2 = Watermark::new(50);
        assert!(!assigner.should_trigger(&window, &wm2));
    }

    #[test]
    fn test_window_contains() {
        let w = Window {
            start_ms: 0,
            end_ms: 100,
        };
        assert!(w.contains(50));
        assert!(!w.contains(100));
        assert!(!w.contains(-1));
    }
}
