//! v6.7.0 密钥轮换零停机：双密钥共存 + 旧密钥删除保护。
//!
//! 轮换过渡期：新密钥写、旧密钥读，无引用后才删旧密钥。

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStatus {
    Active,
    Deprecating,
    Retired,
}

#[derive(Debug, Clone)]
pub struct ActiveKey {
    pub key_id: String,
    pub key: Vec<u8>,
    pub status: KeyStatus,
}

pub struct KeyRotationEnhanced {
    keys: Mutex<HashMap<String, ActiveKey>>,
    write_key_id: Mutex<String>,
    reference_counts: Mutex<HashMap<String, u64>>,
}

impl KeyRotationEnhanced {
    pub fn new(initial_key_id: &str, initial_key: Vec<u8>) -> Self {
        let mut keys = HashMap::new();
        keys.insert(
            initial_key_id.to_string(),
            ActiveKey {
                key_id: initial_key_id.to_string(),
                key: initial_key,
                status: KeyStatus::Active,
            },
        );
        Self {
            keys: Mutex::new(keys),
            write_key_id: Mutex::new(initial_key_id.to_string()),
            reference_counts: Mutex::new(HashMap::new()),
        }
    }

    pub fn rotate_key(&self, new_key_id: &str, new_key: Vec<u8>) -> Result<(), String> {
        let mut keys = self.keys.lock().unwrap();
        if keys.contains_key(new_key_id) {
            return Err(format!("密钥 {} 已存在", new_key_id));
        }
        let old_key_id = self.write_key_id.lock().unwrap().clone();
        if let Some(old_key) = keys.get_mut(&old_key_id) {
            old_key.status = KeyStatus::Deprecating;
        }
        keys.insert(
            new_key_id.to_string(),
            ActiveKey {
                key_id: new_key_id.to_string(),
                key: new_key,
                status: KeyStatus::Active,
            },
        );
        *self.write_key_id.lock().unwrap() = new_key_id.to_string();
        Ok(())
    }

    pub fn encrypt_with_write_key(&self, plaintext: &[u8]) -> Result<(String, Vec<u8>), String> {
        let keys = self.keys.lock().unwrap();
        let write_id = self.write_key_id.lock().unwrap().clone();
        let active_key = keys.get(&write_id).ok_or("写密钥不存在")?;
        if active_key.status != KeyStatus::Active {
            return Err("写密钥非 Active 状态".to_string());
        }
        let encrypted = xor_cipher(plaintext, &active_key.key);
        Ok((write_id, encrypted))
    }

    pub fn decrypt_with_any_active_key(
        &self,
        key_id: &str,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, String> {
        let keys = self.keys.lock().unwrap();
        let key = keys
            .get(key_id)
            .ok_or_else(|| format!("密钥 {} 不存在", key_id))?;
        if key.status == KeyStatus::Retired {
            return Err(format!("密钥 {} 已退役", key_id));
        }
        Ok(xor_cipher(ciphertext, &key.key))
    }

    pub fn add_reference(&self, key_id: &str) {
        let mut refs = self.reference_counts.lock().unwrap();
        *refs.entry(key_id.to_string()).or_insert(0) += 1;
    }

    pub fn release_reference(&self, key_id: &str) {
        let mut refs = self.reference_counts.lock().unwrap();
        if let Some(count) = refs.get_mut(key_id) {
            *count = count.saturating_sub(1);
        }
    }

    pub fn delete_key(&self, key_id: &str) -> Result<(), String> {
        let refs = self.reference_counts.lock().unwrap();
        let count = *refs.get(key_id).unwrap_or(&0);
        if count > 0 {
            return Err(format!(
                "KEY_ROTATION_INCOMPLETE: 密钥 {} 仍有 {} 个引用，拒绝删除",
                key_id, count
            ));
        }
        drop(refs);
        let mut keys = self.keys.lock().unwrap();
        keys.remove(key_id)
            .ok_or_else(|| format!("密钥 {} 不存在", key_id))?;
        Ok(())
    }

    pub fn key_status(&self, key_id: &str) -> Option<KeyStatus> {
        self.keys
            .lock()
            .unwrap()
            .get(key_id)
            .map(|k| k.status.clone())
    }

    pub fn write_key_id(&self) -> String {
        self.write_key_id.lock().unwrap().clone()
    }

    pub fn active_key_count(&self) -> usize {
        self.keys
            .lock()
            .unwrap()
            .values()
            .filter(|k| k.status != KeyStatus::Retired)
            .count()
    }

    pub fn reference_count(&self, key_id: &str) -> u64 {
        *self
            .reference_counts
            .lock()
            .unwrap()
            .get(key_id)
            .unwrap_or(&0)
    }
}

fn xor_cipher(data: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % key.len()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_key_double_coexistence() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();

        let (key_id, ciphertext) = mgr.encrypt_with_write_key(b"secret").unwrap();
        assert_eq!(key_id, "k2");

        let decrypted = mgr
            .decrypt_with_any_active_key(&key_id, &ciphertext)
            .unwrap();
        assert_eq!(decrypted, b"secret");
    }

    #[test]
    fn old_key_still_decrypts_after_rotation() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        let (old_id, old_ct) = mgr.encrypt_with_write_key(b"old-data").unwrap();
        assert_eq!(old_id, "k1");

        mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();

        let decrypted = mgr.decrypt_with_any_active_key("k1", &old_ct).unwrap();
        assert_eq!(decrypted, b"old-data");
    }

    #[test]
    fn new_data_uses_new_key() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();

        let (id, _) = mgr.encrypt_with_write_key(b"new-data").unwrap();
        assert_eq!(id, "k2");
    }

    #[test]
    fn delete_key_with_references_rejected() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();
        mgr.add_reference("k1");
        let result = mgr.delete_key("k1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("KEY_ROTATION_INCOMPLETE"));
    }

    #[test]
    fn delete_key_after_references_released() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();
        mgr.add_reference("k1");
        mgr.add_reference("k1");
        mgr.release_reference("k1");
        mgr.release_reference("k1");
        let result = mgr.delete_key("k1");
        assert!(result.is_ok());
    }

    #[test]
    fn key_status_transitions() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        assert_eq!(mgr.key_status("k1"), Some(KeyStatus::Active));
        mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();
        assert_eq!(mgr.key_status("k1"), Some(KeyStatus::Deprecating));
        assert_eq!(mgr.key_status("k2"), Some(KeyStatus::Active));
    }

    #[test]
    fn active_key_count_after_rotation() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        assert_eq!(mgr.active_key_count(), 1);
        mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();
        assert_eq!(mgr.active_key_count(), 2);
    }

    #[test]
    fn duplicate_key_id_rejected() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        let result = mgr.rotate_key("k1", b"key-v2".to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn reference_count_tracking() {
        let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
        mgr.add_reference("k1");
        mgr.add_reference("k1");
        assert_eq!(mgr.reference_count("k1"), 2);
        mgr.release_reference("k1");
        assert_eq!(mgr.reference_count("k1"), 1);
    }
}
