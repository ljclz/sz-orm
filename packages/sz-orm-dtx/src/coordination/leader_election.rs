//! Leader 选举 — 多数派投票
//!
//! 基于 Fencing Token + 分布式锁实现 Leader 选举。

use tracing::debug;

use super::backend::{CoordinationError, SharedBackend};
use super::fencing::FencingTokenGenerator;
use super::lock::DistributedLock;

/// Leader 选举状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaderState {
    /// 当前节点是 Leader
    Leader,
    /// 当前节点是 Follower
    Follower,
    /// 选举进行中
    Candidate,
}

/// Leader 选举
pub struct LeaderElection {
    backend: SharedBackend,
    election_key: String,
    node_id: String,
    ttl_secs: u64,
    fencing: FencingTokenGenerator,
    state: parking_lot::Mutex<LeaderState>,
    current_leader: parking_lot::Mutex<Option<String>>,
    lock_token: parking_lot::Mutex<Option<String>>,
}

impl LeaderElection {
    /// 创建 Leader 选举实例
    pub fn new(backend: SharedBackend, election_name: &str, node_id: &str, ttl_secs: u64) -> Self {
        let fencing = FencingTokenGenerator::new(backend.clone(), election_name);
        Self {
            backend,
            election_key: format!("leader:{}", election_name),
            node_id: node_id.to_string(),
            ttl_secs,
            fencing,
            state: parking_lot::Mutex::new(LeaderState::Candidate),
            current_leader: parking_lot::Mutex::new(None),
            lock_token: parking_lot::Mutex::new(None),
        }
    }

    /// 尝试成为 Leader
    pub async fn try_become_leader(&self) -> Result<bool, CoordinationError> {
        let lock = DistributedLock::new(self.backend.clone(), &self.election_key, self.ttl_secs);
        match lock.try_acquire().await? {
            Some(guard) => {
                let token = self.fencing.next_token().await?;
                *self.lock_token.lock() = Some(guard.token);
                debug!(
                    "节点 {} 当选 Leader，fencing token = {}",
                    self.node_id, token
                );
                *self.state.lock() = LeaderState::Leader;
                *self.current_leader.lock() = Some(self.node_id.clone());
                Ok(true)
            }
            None => {
                let leader = self.backend.get(&self.election_key).await?;
                *self.state.lock() = LeaderState::Follower;
                *self.current_leader.lock() = leader;
                Ok(false)
            }
        }
    }

    /// 获取当前 Leader
    pub async fn current_leader(&self) -> Result<Option<String>, CoordinationError> {
        self.backend.get(&self.election_key).await
    }

    /// 当前状态
    pub fn state(&self) -> LeaderState {
        self.state.lock().clone()
    }

    /// 是否是 Leader
    pub fn is_leader(&self) -> bool {
        *self.state.lock() == LeaderState::Leader
    }

    /// 节点 ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// 选举键
    pub fn election_key(&self) -> &str {
        &self.election_key
    }

    /// 心跳续约（Leader 调用）
    pub async fn heartbeat(&self) -> Result<bool, CoordinationError> {
        if !self.is_leader() {
            return Ok(false);
        }
        let token = self.lock_token.lock().clone();
        match token {
            Some(t) => {
                let current = self.backend.get(&self.election_key).await?;
                match current {
                    Some(val) if val == t => {
                        self.backend
                            .set(&self.election_key, &t, self.ttl_secs)
                            .await?;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            None => Ok(false),
        }
    }

    /// 主动辞去 Leader
    pub async fn resign(&self) -> Result<bool, CoordinationError> {
        if !self.is_leader() {
            return Ok(false);
        }
        let token = self.lock_token.lock().take();
        let result = match token {
            Some(t) => self.backend.del_if_match(&self.election_key, &t).await?,
            None => false,
        };
        *self.state.lock() = LeaderState::Candidate;
        *self.current_leader.lock() = None;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::backend::InMemoryBackend;
    use super::*;

    fn make_backend() -> SharedBackend {
        Arc::new(InMemoryBackend::new())
    }

    #[tokio::test]
    async fn test_first_node_becomes_leader() {
        let backend = make_backend();
        let election = LeaderElection::new(backend, "test", "node1", 10);
        assert!(election.try_become_leader().await.unwrap());
        assert!(election.is_leader());
    }

    #[tokio::test]
    async fn test_second_node_is_follower() {
        let backend = make_backend();
        let e1 = LeaderElection::new(backend.clone(), "test", "node1", 10);
        let e2 = LeaderElection::new(backend, "test", "node2", 10);
        e1.try_become_leader().await.unwrap();
        assert!(!e2.try_become_leader().await.unwrap());
        assert!(!e2.is_leader());
    }

    #[tokio::test]
    async fn test_leader_state_tracking() {
        let backend = make_backend();
        let election = LeaderElection::new(backend, "test", "node1", 10);
        assert_eq!(election.state(), LeaderState::Candidate);
        election.try_become_leader().await.unwrap();
        assert_eq!(election.state(), LeaderState::Leader);
    }

    #[tokio::test]
    async fn test_resign_and_re_elect() {
        let backend = make_backend();
        let e1 = LeaderElection::new(backend.clone(), "test", "node1", 10);
        let e2 = LeaderElection::new(backend, "test", "node2", 10);
        e1.try_become_leader().await.unwrap();
        e1.resign().await.unwrap();
        assert!(e2.try_become_leader().await.unwrap());
    }

    #[tokio::test]
    async fn test_heartbeat_as_leader() {
        let backend = make_backend();
        let election = LeaderElection::new(backend, "test", "node1", 10);
        election.try_become_leader().await.unwrap();
        assert!(election.heartbeat().await.unwrap());
    }

    #[tokio::test]
    async fn test_heartbeat_as_follower_fails() {
        let backend = make_backend();
        let e1 = LeaderElection::new(backend.clone(), "test", "node1", 10);
        let e2 = LeaderElection::new(backend, "test", "node2", 10);
        e1.try_become_leader().await.unwrap();
        e2.try_become_leader().await.unwrap();
        assert!(!e2.heartbeat().await.unwrap());
    }

    #[tokio::test]
    async fn test_current_leader_query() {
        let backend = make_backend();
        let election = LeaderElection::new(backend, "test", "node1", 10);
        election.try_become_leader().await.unwrap();
        let leader = election.current_leader().await.unwrap();
        assert!(leader.is_some());
    }

    #[tokio::test]
    async fn test_node_id_and_election_key() {
        let backend = make_backend();
        let election = LeaderElection::new(backend, "my_election", "my_node", 10);
        assert_eq!(election.node_id(), "my_node");
        assert_eq!(election.election_key(), "leader:my_election");
    }
}
