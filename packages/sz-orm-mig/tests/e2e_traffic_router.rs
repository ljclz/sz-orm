//! 端到端测试：灰度流量路由器
//!
//! 验证 GrayTrafficRouter 的 percentage/header/cookie 三种流量切换方式。

use sz_orm_mig::gray_release::TrafficSwitchMethod;
use sz_orm_mig::gray_traffic_router::GrayTrafficRouter;

#[test]
fn e2e_traffic_router_percentage() {
    let router = GrayTrafficRouter::new(TrafficSwitchMethod::Percentage, 10);
    let mut gray_count = 0;
    for i in 0..100 {
        if router.is_gray_traffic(i, &[], &[]) {
            gray_count += 1;
        }
    }
    assert_eq!(gray_count, 10, "10% 灰度应路由 10/100 请求");
    assert_eq!(router.gray_percentage(), 10);
}

#[test]
fn e2e_traffic_router_header() {
    let router =
        GrayTrafficRouter::new(TrafficSwitchMethod::Header, 100).with_header("x-gray-release");
    assert!(router.is_gray_traffic(0, &[("x-gray-release", "true")], &[]));
    assert!(!router.is_gray_traffic(0, &[("x-gray-release", "false")], &[]));
    assert!(!router.is_gray_traffic(0, &[], &[]));
}

#[test]
fn e2e_traffic_router_cookie() {
    let router =
        GrayTrafficRouter::new(TrafficSwitchMethod::Cookie, 100).with_cookie("gray_release");
    assert!(router.is_gray_traffic(0, &[], &[("gray_release", "true")]));
    assert!(!router.is_gray_traffic(0, &[], &[("gray_release", "false")]));
}
