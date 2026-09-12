//! PyObserver — 观察者模式包装器

use pyo3::prelude::*;
use std::collections::HashMap;

/// Python 观察者管理器
#[pyclass(name = "Observer")]
pub struct PyObserver {
    subscribers: HashMap<String, Vec<PyObject>>,
    error_count: usize,
}

#[pymethods]
impl PyObserver {
    #[new]
    fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            error_count: 0,
        }
    }

    /// 订阅事件
    fn subscribe(&mut self, event: &str, callback: PyObject) {
        self.subscribers
            .entry(event.to_string())
            .or_default()
            .push(callback);
    }

    /// 取消订阅
    fn unsubscribe(&mut self, event: &str) {
        self.subscribers.remove(event);
    }

    /// 通知所有订阅者
    fn notify(&mut self, py: Python, event: &str) -> PyResult<()> {
        if let Some(callbacks) = self.subscribers.get(event) {
            for cb in callbacks {
                if let Err(e) = cb.call0(py) {
                    self.error_count += 1;
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// 订阅者数量
    fn subscriber_count(&self, event: &str) -> usize {
        self.subscribers.get(event).map(|v| v.len()).unwrap_or(0)
    }

    /// 总订阅者数量
    fn total_subscribers(&self) -> usize {
        self.subscribers.values().map(|v| v.len()).sum()
    }

    /// 错误计数
    fn error_count(&self) -> usize {
        self.error_count
    }

    /// 清空所有订阅
    fn clear(&mut self) {
        self.subscribers.clear();
    }

    fn __repr__(&self) -> String {
        format!(
            "Observer(events={}, subscribers={}, errors={})",
            self.subscribers.len(),
            self.total_subscribers(),
            self.error_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observer_new() {
        let o = PyObserver::new();
        assert_eq!(o.total_subscribers(), 0);
        assert_eq!(o.error_count(), 0);
    }
}
