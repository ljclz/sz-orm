//! 多轮对话上下文
//!
//! 管理多轮自然语言查询的对话历史和上下文。

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 单轮对话摘要
#[derive(Debug, Clone)]
pub struct TurnSummary {
    /// 轮次编号
    pub turn: usize,
    /// 用户输入
    pub user_input: String,
    /// 生成的 SQL
    pub generated_sql: String,
    /// 是否成功
    pub success: bool,
}

/// 多轮对话上下文
pub struct MultiTurnContext {
    /// 会话 ID
    session_id: String,
    /// 对话历史
    history: Vec<TurnSummary>,
    /// 上下文变量（如前几轮提取的表名、列名等）
    context_vars: HashMap<String, String>,
    /// 最大轮数
    max_turns: usize,
    /// 会话创建时间
    created_at: Instant,
    /// 会话超时
    timeout: Duration,
}

impl MultiTurnContext {
    /// 创建新的多轮对话上下文
    pub fn new(session_id: &str, max_turns: usize, timeout: Duration) -> Self {
        Self {
            session_id: session_id.to_string(),
            history: Vec::new(),
            context_vars: HashMap::new(),
            max_turns,
            created_at: Instant::now(),
            timeout,
        }
    }

    /// 会话 ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 添加一轮对话
    pub fn add_turn(
        &mut self,
        user_input: &str,
        generated_sql: &str,
        success: bool,
    ) -> Result<&TurnSummary, String> {
        if self.history.len() >= self.max_turns {
            return Err(format!("已达到最大轮数 {}", self.max_turns));
        }
        if self.created_at.elapsed() > self.timeout {
            return Err("会话已超时".into());
        }
        let turn = self.history.len() + 1;
        self.history.push(TurnSummary {
            turn,
            user_input: user_input.to_string(),
            generated_sql: generated_sql.to_string(),
            success,
        });
        Ok(self.history.last().unwrap())
    }

    /// 设置上下文变量
    pub fn set_var(&mut self, key: &str, value: &str) {
        self.context_vars.insert(key.to_string(), value.to_string());
    }

    /// 获取上下文变量
    pub fn get_var(&self, key: &str) -> Option<&String> {
        self.context_vars.get(key)
    }

    /// 对话历史
    pub fn history(&self) -> &[TurnSummary] {
        &self.history
    }

    /// 当前轮次
    pub fn current_turn(&self) -> usize {
        self.history.len()
    }

    /// 是否已超时
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.timeout
    }

    /// 重置会话
    pub fn reset(&mut self) {
        self.history.clear();
        self.context_vars.clear();
        self.created_at = Instant::now();
    }

    /// 构建带上下文的提示词
    pub fn build_prompt(&self, nl_query: &str) -> String {
        let mut prompt = String::new();
        if !self.history.is_empty() {
            prompt.push_str("前序对话:\n");
            for turn in &self.history {
                prompt.push_str(&format!(
                    "  用户: {}\n  SQL: {}\n",
                    turn.user_input, turn.generated_sql
                ));
            }
            prompt.push('\n');
        }
        if !self.context_vars.is_empty() {
            prompt.push_str("上下文变量:\n");
            for (k, v) in &self.context_vars {
                prompt.push_str(&format!("  {} = {}\n", k, v));
            }
            prompt.push('\n');
        }
        prompt.push_str("当前查询: ");
        prompt.push_str(nl_query);
        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_turn() {
        let mut ctx = MultiTurnContext::new("s1", 10, Duration::from_secs(60));
        let turn = ctx
            .add_turn("查询用户表", "SELECT * FROM users", true)
            .unwrap();
        assert_eq!(turn.turn, 1);
        assert_eq!(ctx.current_turn(), 1);
    }

    #[test]
    fn test_max_turns_exceeded() {
        let mut ctx = MultiTurnContext::new("s1", 2, Duration::from_secs(60));
        ctx.add_turn("q1", "sql1", true).unwrap();
        ctx.add_turn("q2", "sql2", true).unwrap();
        assert!(ctx.add_turn("q3", "sql3", true).is_err());
    }

    #[test]
    fn test_context_vars() {
        let mut ctx = MultiTurnContext::new("s1", 10, Duration::from_secs(60));
        ctx.set_var("table", "users");
        assert_eq!(ctx.get_var("table"), Some(&"users".to_string()));
        assert_eq!(ctx.get_var("missing"), None);
    }

    #[test]
    fn test_build_prompt_empty() {
        let ctx = MultiTurnContext::new("s1", 10, Duration::from_secs(60));
        let prompt = ctx.build_prompt("查询所有用户");
        assert!(prompt.contains("当前查询: 查询所有用户"));
        assert!(!prompt.contains("前序对话"));
    }

    #[test]
    fn test_build_prompt_with_history() {
        let mut ctx = MultiTurnContext::new("s1", 10, Duration::from_secs(60));
        ctx.add_turn("查询用户", "SELECT * FROM users", true)
            .unwrap();
        let prompt = ctx.build_prompt("按年龄排序");
        assert!(prompt.contains("前序对话"));
        assert!(prompt.contains("查询用户"));
        assert!(prompt.contains("按年龄排序"));
    }

    #[test]
    fn test_reset() {
        let mut ctx = MultiTurnContext::new("s1", 10, Duration::from_secs(60));
        ctx.add_turn("q1", "sql1", true).unwrap();
        ctx.set_var("k", "v");
        ctx.reset();
        assert_eq!(ctx.current_turn(), 0);
        assert!(ctx.get_var("k").is_none());
    }
}
