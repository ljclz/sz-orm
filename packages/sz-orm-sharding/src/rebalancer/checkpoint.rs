//! 断点续传：位点管理

use std::collections::HashMap;
use std::sync::RwLock;

/// 断点位点
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub task_id: String,
    pub migrated_rows: u64,
    pub last_source_shard: String,
    pub last_target_shard: String,
    pub completed_migrations: Vec<String>,
}

/// 断点存储 trait
pub trait CheckpointStore: Send + Sync {
    fn save(&self, checkpoint: &Checkpoint) -> Result<(), String>;
    fn load(&self, task_id: &str) -> Option<Checkpoint>;
    fn delete(&self, task_id: &str) -> Result<(), String>;
}

/// 内存断点存储
pub struct MemoryCheckpointStore {
    checkpoints: RwLock<HashMap<String, Checkpoint>>,
}

impl MemoryCheckpointStore {
    pub fn new() -> Self {
        Self {
            checkpoints: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryCheckpointStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointStore for MemoryCheckpointStore {
    fn save(&self, checkpoint: &Checkpoint) -> Result<(), String> {
        let mut store = self
            .checkpoints
            .write()
            .map_err(|e| format!("lock poisoned: {e}"))?;
        store.insert(checkpoint.task_id.clone(), checkpoint.clone());
        Ok(())
    }

    fn load(&self, task_id: &str) -> Option<Checkpoint> {
        let store = self.checkpoints.read().ok()?;
        store.get(task_id).cloned()
    }

    fn delete(&self, task_id: &str) -> Result<(), String> {
        let mut store = self
            .checkpoints
            .write()
            .map_err(|e| format!("lock poisoned: {e}"))?;
        store.remove(task_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_checkpoint_save_load() {
        let store = MemoryCheckpointStore::new();
        let cp = Checkpoint {
            task_id: "task1".to_string(),
            migrated_rows: 100,
            last_source_shard: "s1".to_string(),
            last_target_shard: "s2".to_string(),
            completed_migrations: vec!["s1->s2".to_string()],
        };

        store.save(&cp).unwrap();
        let loaded = store.load("task1").unwrap();
        assert_eq!(loaded.migrated_rows, 100);
        assert_eq!(loaded.last_source_shard, "s1");
    }

    #[test]
    fn test_memory_checkpoint_delete() {
        let store = MemoryCheckpointStore::new();
        let cp = Checkpoint {
            task_id: "task1".to_string(),
            migrated_rows: 50,
            last_source_shard: "s1".to_string(),
            last_target_shard: "s2".to_string(),
            completed_migrations: vec![],
        };

        store.save(&cp).unwrap();
        assert!(store.load("task1").is_some());

        store.delete("task1").unwrap();
        assert!(store.load("task1").is_none());
    }

    #[test]
    fn test_memory_checkpoint_not_found() {
        let store = MemoryCheckpointStore::new();
        assert!(store.load("nonexistent").is_none());
    }

    #[test]
    fn test_memory_checkpoint_overwrite() {
        let store = MemoryCheckpointStore::new();

        let cp1 = Checkpoint {
            task_id: "task1".to_string(),
            migrated_rows: 50,
            last_source_shard: "s1".to_string(),
            last_target_shard: "s2".to_string(),
            completed_migrations: vec![],
        };
        store.save(&cp1).unwrap();

        let cp2 = Checkpoint {
            task_id: "task1".to_string(),
            migrated_rows: 100,
            last_source_shard: "s1".to_string(),
            last_target_shard: "s3".to_string(),
            completed_migrations: vec!["s1->s2".to_string()],
        };
        store.save(&cp2).unwrap();

        let loaded = store.load("task1").unwrap();
        assert_eq!(loaded.migrated_rows, 100);
    }
}
