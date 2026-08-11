//! 跨语言参与者注册中心
//!
//! 管理跨语言参与者的注册、发现与健康检查。

use super::{CrossLangParticipantDesc, CrossLangTxError, ParticipantLanguage};
use parking_lot::RwLock;
use std::collections::HashMap;

/// 参与者注册中心
pub struct CrossLangParticipantRegistry {
    participants: RwLock<HashMap<String, RegisteredParticipant>>,
}

/// 已注册的参与者信息
#[derive(Debug, Clone)]
pub struct RegisteredParticipant {
    pub desc: CrossLangParticipantDesc,
    pub registered_at: u64,
    pub healthy: bool,
    pub last_heartbeat: u64,
}

impl CrossLangParticipantRegistry {
    pub fn new() -> Self {
        Self {
            participants: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(
        &self,
        desc: CrossLangParticipantDesc,
        timestamp: u64,
    ) -> Result<(), CrossLangTxError> {
        let mut participants = self.participants.write();
        if participants.contains_key(&desc.resource_id) {
            return Err(CrossLangTxError::RecoveryConflict);
        }
        participants.insert(
            desc.resource_id.clone(),
            RegisteredParticipant {
                desc,
                registered_at: timestamp,
                healthy: true,
                last_heartbeat: timestamp,
            },
        );
        Ok(())
    }

    pub fn deregister(&self, resource_id: &str) -> Option<RegisteredParticipant> {
        self.participants.write().remove(resource_id)
    }

    pub fn get(&self, resource_id: &str) -> Option<RegisteredParticipant> {
        self.participants.read().get(resource_id).cloned()
    }

    pub fn list(&self) -> Vec<RegisteredParticipant> {
        self.participants.read().values().cloned().collect()
    }

    pub fn list_by_language(&self, language: ParticipantLanguage) -> Vec<RegisteredParticipant> {
        self.participants
            .read()
            .values()
            .filter(|p| p.desc.language == language)
            .cloned()
            .collect()
    }

    pub fn heartbeat(&self, resource_id: &str, timestamp: u64) -> Result<(), CrossLangTxError> {
        let mut participants = self.participants.write();
        let participant = participants.get_mut(resource_id).ok_or_else(|| {
            CrossLangTxError::RemoteCall(format!("participant not found: {resource_id}"))
        })?;
        participant.last_heartbeat = timestamp;
        participant.healthy = true;
        Ok(())
    }

    pub fn mark_unhealthy(&self, resource_id: &str) -> Result<(), CrossLangTxError> {
        let mut participants = self.participants.write();
        let participant = participants.get_mut(resource_id).ok_or_else(|| {
            CrossLangTxError::RemoteCall(format!("participant not found: {resource_id}"))
        })?;
        participant.healthy = false;
        Ok(())
    }

    pub fn healthy_participants(&self) -> Vec<RegisteredParticipant> {
        self.participants
            .read()
            .values()
            .filter(|p| p.healthy)
            .cloned()
            .collect()
    }

    pub fn check_stale(&self, current_time: u64, stale_threshold_ms: u64) -> Vec<String> {
        let mut participants = self.participants.write();
        let mut stale = Vec::new();
        for (id, participant) in participants.iter_mut() {
            if current_time.saturating_sub(participant.last_heartbeat) > stale_threshold_ms {
                participant.healthy = false;
                stale.push(id.clone());
            }
        }
        stale
    }

    pub fn len(&self) -> usize {
        self.participants.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.participants.read().is_empty()
    }
}

impl Default for CrossLangParticipantRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_lang::{ParticipantAuth, ParticipantTransport};

    fn make_desc(id: &str, lang: ParticipantLanguage) -> CrossLangParticipantDesc {
        CrossLangParticipantDesc {
            resource_id: id.to_string(),
            language: lang,
            transport: ParticipantTransport::Grpc,
            endpoint: format!("grpc://localhost:8080/{id}"),
            auth: ParticipantAuth::Token("token".to_string()),
            protocol_version: 1,
        }
    }

    #[test]
    fn test_register_and_get() {
        let registry = CrossLangParticipantRegistry::new();
        let desc = make_desc("p-1", ParticipantLanguage::Go);
        registry.register(desc.clone(), 1000).unwrap();
        let participant = registry.get("p-1").unwrap();
        assert_eq!(participant.desc.resource_id, "p-1");
        assert!(participant.healthy);
    }

    #[test]
    fn test_duplicate_register_fails() {
        let registry = CrossLangParticipantRegistry::new();
        let desc = make_desc("p-1", ParticipantLanguage::Go);
        registry.register(desc, 1000).unwrap();
        let desc2 = make_desc("p-1", ParticipantLanguage::Java);
        let result = registry.register(desc2, 2000);
        assert!(result.is_err());
    }

    #[test]
    fn test_deregister() {
        let registry = CrossLangParticipantRegistry::new();
        registry
            .register(make_desc("p-1", ParticipantLanguage::Go), 1000)
            .unwrap();
        let removed = registry.deregister("p-1").unwrap();
        assert_eq!(removed.desc.resource_id, "p-1");
        assert!(registry.get("p-1").is_none());
    }

    #[test]
    fn test_list_by_language() {
        let registry = CrossLangParticipantRegistry::new();
        registry
            .register(make_desc("p-1", ParticipantLanguage::Go), 1000)
            .unwrap();
        registry
            .register(make_desc("p-2", ParticipantLanguage::Java), 1000)
            .unwrap();
        registry
            .register(make_desc("p-3", ParticipantLanguage::Go), 1000)
            .unwrap();
        let go_participants = registry.list_by_language(ParticipantLanguage::Go);
        assert_eq!(go_participants.len(), 2);
    }

    #[test]
    fn test_heartbeat() {
        let registry = CrossLangParticipantRegistry::new();
        registry
            .register(make_desc("p-1", ParticipantLanguage::Go), 1000)
            .unwrap();
        registry.heartbeat("p-1", 2000).unwrap();
        let participant = registry.get("p-1").unwrap();
        assert_eq!(participant.last_heartbeat, 2000);
        assert!(participant.healthy);
    }

    #[test]
    fn test_mark_unhealthy() {
        let registry = CrossLangParticipantRegistry::new();
        registry
            .register(make_desc("p-1", ParticipantLanguage::Go), 1000)
            .unwrap();
        registry.mark_unhealthy("p-1").unwrap();
        let participant = registry.get("p-1").unwrap();
        assert!(!participant.healthy);
    }

    #[test]
    fn test_healthy_participants() {
        let registry = CrossLangParticipantRegistry::new();
        registry
            .register(make_desc("p-1", ParticipantLanguage::Go), 1000)
            .unwrap();
        registry
            .register(make_desc("p-2", ParticipantLanguage::Java), 1000)
            .unwrap();
        registry.mark_unhealthy("p-2").unwrap();
        let healthy = registry.healthy_participants();
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].desc.resource_id, "p-1");
    }

    #[test]
    fn test_check_stale() {
        let registry = CrossLangParticipantRegistry::new();
        registry
            .register(make_desc("p-1", ParticipantLanguage::Go), 1000)
            .unwrap();
        registry
            .register(make_desc("p-2", ParticipantLanguage::Java), 3000)
            .unwrap();
        let stale = registry.check_stale(5000, 3000);
        assert_eq!(stale.len(), 1);
        assert!(stale.contains(&"p-1".to_string()));
    }

    #[test]
    fn test_len_and_is_empty() {
        let registry = CrossLangParticipantRegistry::new();
        assert!(registry.is_empty());
        registry
            .register(make_desc("p-1", ParticipantLanguage::Go), 1000)
            .unwrap();
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
    }
}
