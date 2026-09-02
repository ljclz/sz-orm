//! 多模态多轮对话（TASK-030）

use crate::types::{Modality, MultimodalError};
use serde::{Deserialize, Serialize};

/// 对话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogMessage {
    pub role: MessageRole,
    pub content: String,
    pub modality: Modality,
    pub timestamp: String,
}

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

/// 上下文窗口
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub messages: Vec<DialogMessage>,
    pub max_tokens: usize,
    pub current_tokens: usize,
}

impl ContextWindow {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_tokens,
            current_tokens: 0,
        }
    }

    pub fn add_message(&mut self, message: DialogMessage) -> bool {
        let msg_tokens = Self::estimate_tokens(&message.content);
        if self.current_tokens + msg_tokens > self.max_tokens {
            self.evict_oldest(msg_tokens);
        }

        self.current_tokens += msg_tokens;
        self.messages.push(message);
        true
    }

    fn evict_oldest(&mut self, needed_tokens: usize) {
        while self.current_tokens + needed_tokens > self.max_tokens && !self.messages.is_empty() {
            let oldest = self.messages.remove(0);
            self.current_tokens -= Self::estimate_tokens(&oldest.content);
        }
    }

    fn estimate_tokens(text: &str) -> usize {
        text.split_whitespace().count() + 1
    }

    pub fn is_full(&self) -> bool {
        self.current_tokens >= self.max_tokens
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.current_tokens = 0;
    }
}

/// 多模态对话管理器
pub struct MultimodalDialog {
    context: ContextWindow,
    current_modality: Modality,
}

impl MultimodalDialog {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            context: ContextWindow::new(max_tokens),
            current_modality: Modality::Text,
        }
    }

    /// 用户输入（指定模态）
    pub fn input(
        &mut self,
        content: &str,
        modality: Modality,
    ) -> Result<DialogMessage, MultimodalError> {
        if content.is_empty() {
            return Err(MultimodalError::RenderFallback);
        }

        self.current_modality = modality.clone();
        let message = DialogMessage {
            role: MessageRole::User,
            content: content.to_string(),
            modality,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.context.add_message(message.clone());
        Ok(message)
    }

    /// 助手回复
    pub fn reply(&mut self, content: &str) -> DialogMessage {
        let message = DialogMessage {
            role: MessageRole::Assistant,
            content: content.to_string(),
            modality: self.current_modality.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.context.add_message(message.clone());
        message
    }

    /// 获取上下文窗口
    pub fn context(&self) -> &ContextWindow {
        &self.context
    }

    /// 切换模态
    pub fn switch_modality(&mut self, modality: Modality) {
        self.current_modality = modality;
    }

    /// 获取当前模态
    pub fn current_modality(&self) -> &Modality {
        &self.current_modality
    }

    /// 清空对话
    pub fn clear(&mut self) {
        self.context.clear();
    }

    /// 对话轮数
    pub fn turn_count(&self) -> usize {
        self.context.messages.len() / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_basic_flow() {
        let mut dialog = MultimodalDialog::new(1000);

        let user_msg = dialog.input("查询用户", Modality::Text).unwrap();
        assert_eq!(user_msg.role, MessageRole::User);

        let assistant_msg = dialog.reply("SELECT * FROM users");
        assert_eq!(assistant_msg.role, MessageRole::Assistant);

        assert_eq!(dialog.turn_count(), 1);
    }

    #[test]
    fn test_dialog_voice_modality() {
        let mut dialog = MultimodalDialog::new(1000);

        dialog.input("语音查询", Modality::Voice).unwrap();
        assert_eq!(*dialog.current_modality(), Modality::Voice);

        let reply = dialog.reply("正在处理语音查询");
        assert_eq!(reply.modality, Modality::Voice);
    }

    #[test]
    fn test_context_window_eviction() {
        let mut dialog = MultimodalDialog::new(10);

        for i in 0..20 {
            dialog
                .input(&format!("消息 {}", i), Modality::Text)
                .unwrap();
            dialog.reply("回复");
        }

        assert!(
            dialog.context().current_tokens <= 10,
            "Token 数不应超过上限"
        );
    }

    #[test]
    fn test_switch_modality() {
        let mut dialog = MultimodalDialog::new(1000);
        assert_eq!(*dialog.current_modality(), Modality::Text);

        dialog.switch_modality(Modality::Voice);
        assert_eq!(*dialog.current_modality(), Modality::Voice);

        dialog.switch_modality(Modality::ErDiagram);
        assert_eq!(*dialog.current_modality(), Modality::ErDiagram);
    }

    #[test]
    fn test_empty_input_fails() {
        let mut dialog = MultimodalDialog::new(1000);
        assert!(dialog.input("", Modality::Text).is_err());
    }

    #[test]
    fn test_clear_dialog() {
        let mut dialog = MultimodalDialog::new(1000);
        dialog.input("测试", Modality::Text).unwrap();
        dialog.reply("回复");

        dialog.clear();
        assert_eq!(dialog.context().messages.len(), 0);
        assert_eq!(dialog.turn_count(), 0);
    }

    #[test]
    fn test_multi_turn_dialog() {
        let mut dialog = MultimodalDialog::new(10000);

        dialog.input("查询用户", Modality::Text).unwrap();
        dialog.reply("SELECT * FROM users");

        dialog.input("只看活跃的", Modality::Text).unwrap();
        dialog.reply("SELECT * FROM users WHERE active = true");

        dialog.input("按姓名排序", Modality::Text).unwrap();
        dialog.reply("SELECT * FROM users WHERE active = true ORDER BY name");

        assert_eq!(dialog.turn_count(), 3);
        assert_eq!(dialog.context().messages.len(), 6);
    }
}
