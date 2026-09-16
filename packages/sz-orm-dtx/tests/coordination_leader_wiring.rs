//! Leader 选举端到端接线测试

use std::sync::Arc;

use sz_orm_dtx::coordination::{
    CoordinationBackend, FencingTokenGenerator, InMemoryBackend, LeaderElection, LeaderState,
    ServiceInstance, ServiceRegistry,
};

#[tokio::test]
async fn wiring_leader_election_basic() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let election = LeaderElection::new(backend, "test", "node1", 10);
    assert!(election.try_become_leader().await.unwrap());
    assert_eq!(election.state(), LeaderState::Leader);
}

#[tokio::test]
async fn wiring_leader_election_contention() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let e1 = LeaderElection::new(backend.clone(), "test", "node1", 10);
    let e2 = LeaderElection::new(backend, "test", "node2", 10);
    e1.try_become_leader().await.unwrap();
    assert!(!e2.try_become_leader().await.unwrap());
    assert_eq!(e2.state(), LeaderState::Follower);
}

#[tokio::test]
async fn wiring_leader_resign_re_elect() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let e1 = LeaderElection::new(backend.clone(), "test", "node1", 10);
    let e2 = LeaderElection::new(backend, "test", "node2", 10);
    e1.try_become_leader().await.unwrap();
    e1.resign().await.unwrap();
    assert!(e2.try_become_leader().await.unwrap());
}

#[tokio::test]
async fn wiring_leader_heartbeat() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let election = LeaderElection::new(backend, "test", "node1", 10);
    election.try_become_leader().await.unwrap();
    assert!(election.heartbeat().await.unwrap());
}

#[tokio::test]
async fn wiring_fencing_token_monotonic() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let gen = FencingTokenGenerator::new(backend, "test");
    let t1 = gen.next_token().await.unwrap();
    let t2 = gen.next_token().await.unwrap();
    assert!(t1 < t2);
}

#[tokio::test]
async fn wiring_service_registry() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let registry = ServiceRegistry::new(backend, 30);
    let instance = ServiceInstance::new("user-svc", "inst-1", "10.0.0.1", 8080);
    registry.register(&instance).await.unwrap();
    let found = registry.discover("user-svc").await.unwrap();
    assert_eq!(found.len(), 1);
    registry.deregister(&instance).await.unwrap();
    let found = registry.discover("user-svc").await.unwrap();
    assert!(found.is_empty());
}
