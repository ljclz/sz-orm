//! LLM 适配器热切换
//!
//! 运行时切换 LLM 适配器，无需重启服务。

use parking_lot::RwLock;

/// LLM 适配器槽位
#[derive(Debug, Clone)]
pub struct LlmAdapterSlot {
    /// 适配器名称
    pub name: String,
    /// 模型标识
    pub model: String,
    /// 是否为活跃适配器
    pub is_active: bool,
}

/// LLM 热切换器
pub struct LlmHotSwapper {
    slots: RwLock<Vec<LlmAdapterSlot>>,
    active_idx: RwLock<usize>,
}

impl Default for LlmHotSwapper {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmHotSwapper {
    /// 创建热切换器
    pub fn new() -> Self {
        Self {
            slots: RwLock::new(Vec::new()),
            active_idx: RwLock::new(0),
        }
    }

    /// 注册适配器
    pub fn register(&self, name: &str, model: &str) -> usize {
        let mut slots = self.slots.write();
        let idx = slots.len();
        slots.push(LlmAdapterSlot {
            name: name.to_string(),
            model: model.to_string(),
            is_active: idx == 0,
        });
        idx
    }

    /// 切换到指定适配器
    pub fn swap_to(&self, idx: usize) -> Result<LlmAdapterSlot, String> {
        let slots = self.slots.read();
        if idx >= slots.len() {
            return Err(format!("适配器索引 {} 不存在", idx));
        }
        drop(slots);

        let mut active = self.active_idx.write();
        *active = idx;

        let mut slots = self.slots.write();
        for (i, slot) in slots.iter_mut().enumerate() {
            slot.is_active = i == idx;
        }
        Ok(slots[idx].clone())
    }

    /// 获取当前活跃适配器
    pub fn active(&self) -> Option<LlmAdapterSlot> {
        let slots = self.slots.read();
        let active = *self.active_idx.read();
        slots.get(active).cloned()
    }

    /// 列出所有适配器
    pub fn list(&self) -> Vec<LlmAdapterSlot> {
        self.slots.read().clone()
    }

    /// 移除适配器
    pub fn remove(&self, idx: usize) -> Result<(), String> {
        let mut slots = self.slots.write();
        if idx >= slots.len() {
            return Err(format!("适配器索引 {} 不存在", idx));
        }
        if slots.len() == 1 {
            return Err("不能移除最后一个适配器".into());
        }
        slots.remove(idx);
        let mut active = self.active_idx.write();
        if *active >= slots.len() {
            *active = 0;
        }
        for (i, slot) in slots.iter_mut().enumerate() {
            slot.is_active = i == *active;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_active() {
        let swapper = LlmHotSwapper::new();
        swapper.register("openai", "gpt-4");
        swapper.register("claude", "claude-3");
        let active = swapper.active().unwrap();
        assert_eq!(active.name, "openai");
        assert!(active.is_active);
    }

    #[test]
    fn test_swap() {
        let swapper = LlmHotSwapper::new();
        swapper.register("openai", "gpt-4");
        swapper.register("claude", "claude-3");
        let swapped = swapper.swap_to(1).unwrap();
        assert_eq!(swapped.name, "claude");
        let active = swapper.active().unwrap();
        assert_eq!(active.name, "claude");
    }

    #[test]
    fn test_swap_out_of_bounds() {
        let swapper = LlmHotSwapper::new();
        swapper.register("a", "m1");
        assert!(swapper.swap_to(5).is_err());
    }

    #[test]
    fn test_list() {
        let swapper = LlmHotSwapper::new();
        swapper.register("a", "m1");
        swapper.register("b", "m2");
        let list = swapper.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_remove() {
        let swapper = LlmHotSwapper::new();
        swapper.register("a", "m1");
        swapper.register("b", "m2");
        swapper.remove(0).unwrap();
        assert_eq!(swapper.list().len(), 1);
        assert_eq!(swapper.active().unwrap().name, "b");
    }

    #[test]
    fn test_remove_last_fails() {
        let swapper = LlmHotSwapper::new();
        swapper.register("a", "m1");
        assert!(swapper.remove(0).is_err());
    }
}
