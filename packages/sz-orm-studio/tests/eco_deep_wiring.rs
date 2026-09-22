//! 生态扩展深化端到端接线测试（v8.1.0 组 6）
//!
//! ④ DeveloperPortal 页面加载 ≤ 2s 文档/API/试用/看板/市场入口

#![cfg(feature = "developer-portal")]

use std::time::Instant;

use sz_orm_studio::{DeveloperPortal, PortalPage, PortalRequest};

/// 端到端测试 ④：DeveloperPortal 页面加载 ≤ 2s 文档/API/试用/看板/市场入口
#[test]
fn test_eco_deep_developer_portal_all_pages() {
    let portal = DeveloperPortal::new();
    let start = Instant::now();

    for page in [
        PortalPage::Docs,
        PortalPage::ApiReference,
        PortalPage::TryIt,
        PortalPage::Dashboard,
        PortalPage::Marketplace,
    ] {
        let req = PortalRequest {
            path: page.path().to_string(),
            query: std::collections::HashMap::new(),
        };
        let resp = portal.serve(&req).unwrap();
        assert_eq!(resp.status, 200, "页面 {:?} 不可访问", page);
        assert!(!resp.degraded);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed <= std::time::Duration::from_secs(2),
        "所有页面加载 {:?} > 2s",
        elapsed
    );
}

/// 端到端测试 ④b：门户降级模式
#[test]
fn test_eco_deep_developer_portal_degraded() {
    let portal = DeveloperPortal::degraded();
    let req = PortalRequest {
        path: "/docs".to_string(),
        query: std::collections::HashMap::new(),
    };
    let resp = portal.serve(&req).unwrap();
    assert!(resp.degraded);
    assert!(resp.body.contains("降级"));
}

/// 端到端测试 ④c：门户健康指标
#[test]
fn test_eco_deep_developer_portal_health() {
    let portal = DeveloperPortal::new();
    for _ in 0..10 {
        let req = PortalRequest {
            path: "/docs".to_string(),
            query: std::collections::HashMap::new(),
        };
        portal.serve(&req).unwrap();
    }
    let health = portal.health();
    assert_eq!(health.total_requests, 10);
    assert_eq!(health.availability_rate, 1.0);
}
