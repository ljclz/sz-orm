//! Transaction — 事务

use napi_derive::napi;

#[napi]
pub struct Transaction {
    active: bool,
    isolation: String,
    read_only: bool,
}

#[napi]
impl Transaction {
    #[napi(constructor)]
    pub fn new(isolation: Option<String>, read_only: Option<bool>) -> Self {
        Self {
            active: false,
            isolation: isolation.unwrap_or_else(|| "read_committed".to_string()),
            read_only: read_only.unwrap_or(false),
        }
    }

    #[napi(getter)]
    pub fn is_active(&self) -> bool {
        self.active
    }

    #[napi(getter)]
    pub fn isolation(&self) -> String {
        self.isolation.clone()
    }

    #[napi(getter)]
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    #[napi]
    pub fn status(&self) -> String {
        format!(
            "Transaction(active={}, isolation={}, read_only={})",
            self.active, self.isolation, self.read_only
        )
    }
}
