//! 工单化错误处理：错误含错误码/分类/工单 ID

use std::fmt;

/// 错误分类
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCategory {
    ValidationError,
    AuthError,
    NotFoundError,
    InternalError,
    RateLimitError,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::ValidationError => write!(f, "ValidationError"),
            ErrorCategory::AuthError => write!(f, "AuthError"),
            ErrorCategory::NotFoundError => write!(f, "NotFoundError"),
            ErrorCategory::InternalError => write!(f, "InternalError"),
            ErrorCategory::RateLimitError => write!(f, "RateLimitError"),
        }
    }
}

/// 工单化错误
#[derive(Debug, Clone)]
pub struct TicketError {
    pub code: String,
    pub category: ErrorCategory,
    pub ticket_id: String,
    pub message: String,
}

impl TicketError {
    pub fn new(code: &str, category: ErrorCategory, message: &str) -> Self {
        let ticket_id = generate_ticket_id();
        Self {
            code: code.to_string(),
            category,
            ticket_id,
            message: message.to_string(),
        }
    }

    pub fn validation(code: &str, message: &str) -> Self {
        Self::new(code, ErrorCategory::ValidationError, message)
    }

    pub fn auth(code: &str, message: &str) -> Self {
        Self::new(code, ErrorCategory::AuthError, message)
    }

    pub fn not_found(code: &str, message: &str) -> Self {
        Self::new(code, ErrorCategory::NotFoundError, message)
    }

    pub fn internal(code: &str, message: &str) -> Self {
        Self::new(code, ErrorCategory::InternalError, message)
    }

    pub fn rate_limit(code: &str, message: &str) -> Self {
        Self::new(code, ErrorCategory::RateLimitError, message)
    }
}

impl fmt::Display for TicketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ({}): {}",
            self.ticket_id, self.code, self.category, self.message
        )
    }
}

impl std::error::Error for TicketError {}

fn generate_ticket_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("ticket-{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticket_error_creation() {
        let err = TicketError::new("ERR_001", ErrorCategory::ValidationError, "invalid input");
        assert_eq!(err.code, "ERR_001");
        assert_eq!(err.category, ErrorCategory::ValidationError);
        assert_eq!(err.message, "invalid input");
        assert!(err.ticket_id.starts_with("ticket-"));
    }

    #[test]
    fn test_ticket_error_unique_ids() {
        let err1 = TicketError::validation("ERR_001", "msg1");
        let err2 = TicketError::validation("ERR_001", "msg2");
        assert_ne!(err1.ticket_id, err2.ticket_id);
    }

    #[test]
    fn test_ticket_error_categories() {
        assert_eq!(
            TicketError::auth("A", "m").category,
            ErrorCategory::AuthError
        );
        assert_eq!(
            TicketError::not_found("A", "m").category,
            ErrorCategory::NotFoundError
        );
        assert_eq!(
            TicketError::internal("A", "m").category,
            ErrorCategory::InternalError
        );
        assert_eq!(
            TicketError::rate_limit("A", "m").category,
            ErrorCategory::RateLimitError
        );
    }

    #[test]
    fn test_ticket_error_display() {
        let err = TicketError::new("ERR_001", ErrorCategory::ValidationError, "bad input");
        let s = err.to_string();
        assert!(s.contains("ERR_001"));
        assert!(s.contains("ValidationError"));
        assert!(s.contains("bad input"));
    }

    #[test]
    fn test_error_category_display() {
        assert_eq!(
            ErrorCategory::ValidationError.to_string(),
            "ValidationError"
        );
        assert_eq!(ErrorCategory::AuthError.to_string(), "AuthError");
        assert_eq!(ErrorCategory::NotFoundError.to_string(), "NotFoundError");
        assert_eq!(ErrorCategory::InternalError.to_string(), "InternalError");
        assert_eq!(ErrorCategory::RateLimitError.to_string(), "RateLimitError");
    }
}
