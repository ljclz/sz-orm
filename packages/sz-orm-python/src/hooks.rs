//! PyHooks — 钩子系统包装器

use pyo3::prelude::*;
use std::collections::HashMap;

/// 钩子事件类型
#[pyclass(name = "HookEvent")]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PyHookEvent {
    pub event: u8,
}

#[pymethods]
impl PyHookEvent {
    #[classattr]
    const BEFORE_SAVE: u8 = 0;

    #[classattr]
    const AFTER_SAVE: u8 = 1;

    #[classattr]
    const BEFORE_DELETE: u8 = 2;

    #[classattr]
    const AFTER_DELETE: u8 = 3;

    #[classattr]
    const BEFORE_FIND: u8 = 4;

    #[classattr]
    const AFTER_FIND: u8 = 5;
}

/// Python 钩子注册器
#[pyclass(name = "Hooks")]
pub struct PyHooks {
    callbacks: HashMap<u8, Vec<PyObject>>,
}

#[pymethods]
impl PyHooks {
    #[new]
    fn new() -> Self {
        Self {
            callbacks: HashMap::new(),
        }
    }

    /// 注册回调
    fn register(&mut self, event: u8, callback: PyObject) {
        self.callbacks.entry(event).or_default().push(callback);
    }

    /// 注销指定事件的所有回调
    fn clear(&mut self, event: u8) {
        self.callbacks.remove(&event);
    }

    /// 注销所有回调
    fn clear_all(&mut self) {
        self.callbacks.clear();
    }

    /// 返回指定事件的回调数
    fn count(&self, event: u8) -> usize {
        self.callbacks.get(&event).map(|v| v.len()).unwrap_or(0)
    }

    /// 返回所有已注册事件
    fn events(&self) -> Vec<u8> {
        self.callbacks.keys().copied().collect()
    }

    /// 触发事件（调用所有回调）
    fn dispatch(&self, py: Python, event: u8) -> PyResult<()> {
        if let Some(callbacks) = self.callbacks.get(&event) {
            for cb in callbacks {
                cb.call0(py)?;
            }
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        let total: usize = self.callbacks.values().map(|v| v.len()).sum();
        format!(
            "Hooks(events={}, callbacks={})",
            self.callbacks.len(),
            total
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hooks_new() {
        let h = PyHooks::new();
        assert_eq!(h.count(0), 0);
    }

    #[test]
    fn test_hooks_events() {
        assert_eq!(PyHookEvent::BEFORE_SAVE, 0);
        assert_eq!(PyHookEvent::AFTER_SAVE, 1);
        assert_eq!(PyHookEvent::BEFORE_DELETE, 2);
        assert_eq!(PyHookEvent::AFTER_DELETE, 3);
    }
}
