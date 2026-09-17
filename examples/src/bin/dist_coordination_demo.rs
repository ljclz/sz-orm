//! 分布式协调 demo
//!
//! 展示分布式锁 + Leader 选举 + 服务注册/发现。

use std::sync::Arc;
use std::time::Duration;

use sz_orm_dtx::coordination::{
    CoordinationBackend, DistributedLock, InMemoryBackend, LeaderElection, ServiceInstance,
    ServiceRegistry,
};

#[tokio::main]
async fn main() {
    println!("=== sz-orm 分布式协调 demo ===\n");

    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());

    let lock = DistributedLock::new(backend.clone(), "my_resource", 30);
    lock.acquire().await.unwrap();
    println!("锁获取成功: {}", lock.key());
    assert!(lock.renew().await.unwrap());
    println!("锁续约成功");
    lock.release().await.unwrap();
    println!("锁释放成功\n");

    let e1 = LeaderElection::new(backend.clone(), "my_cluster", "node-1", 30);
    let e2 = LeaderElection::new(backend.clone(), "my_cluster", "node-2", 30);
    e1.try_become_leader().await.unwrap();
    println!("Leader: node-1 (state={:?})", e1.state());
    e2.try_become_leader().await.unwrap();
    println!("node-2: state={:?} (follower)", e2.state());
    e1.resign().await.unwrap();
    e2.try_become_leader().await.unwrap();
    println!("重新选举后 Leader: node-2 (state={:?})\n", e2.state());

    let registry = ServiceRegistry::new(backend, 30);
    let svc1 = ServiceInstance::new("order-service", "inst-1", "10.0.0.1", 8080)
        .with_metadata("version", "1.0");
    let svc2 = ServiceInstance::new("order-service", "inst-2", "10.0.0.2", 8080)
        .with_metadata("version", "1.0");
    registry.register(&svc1).await.unwrap();
    registry.register(&svc2).await.unwrap();
    println!("注册 2 个 order-service 实例");
    let instances = registry.discover("order-service").await.unwrap();
    println!("发现 {} 个实例:", instances.len());
    for inst in &instances {
        println!("  - {}:{} (id={})", inst.host, inst.port, inst.instance_id);
    }
    registry.heartbeat(&svc1).await.unwrap();
    println!("心跳发送成功");
    registry.deregister(&svc2).await.unwrap();
    let remaining = registry.discover("order-service").await.unwrap();
    println!("注销后剩余 {} 个实例", remaining.len());

    tokio::time::sleep(Duration::from_millis(10)).await;
    println!("\n=== demo 完成 ===");
}
