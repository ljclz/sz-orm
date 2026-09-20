//! 灰度流量路由器

use super::gray_release::TrafficSwitchMethod;

/// 灰度流量路由器：支持 percentage/header/cookie 三种流量切换方式
pub struct GrayTrafficRouter {
    method: TrafficSwitchMethod,
    gray_percentage: u32,
    gray_header: String,
    gray_cookie: String,
}

impl GrayTrafficRouter {
    pub fn new(method: TrafficSwitchMethod, gray_percentage: u32) -> Self {
        Self {
            method,
            gray_percentage,
            gray_header: "x-gray-release".to_string(),
            gray_cookie: "gray_release".to_string(),
        }
    }

    pub fn with_header(mut self, header: &str) -> Self {
        self.gray_header = header.to_string();
        self
    }

    pub fn with_cookie(mut self, cookie: &str) -> Self {
        self.gray_cookie = cookie.to_string();
        self
    }

    pub fn is_gray_traffic(
        &self,
        request_id: u64,
        headers: &[(&str, &str)],
        cookies: &[(&str, &str)],
    ) -> bool {
        match self.method {
            TrafficSwitchMethod::Percentage => (request_id % 100) < self.gray_percentage as u64,
            TrafficSwitchMethod::Header => headers
                .iter()
                .any(|(k, v)| *k == self.gray_header && *v == "true"),
            TrafficSwitchMethod::Cookie => cookies
                .iter()
                .any(|(k, v)| *k == self.gray_cookie && *v == "true"),
        }
    }

    pub fn method(&self) -> &TrafficSwitchMethod {
        &self.method
    }

    pub fn gray_percentage(&self) -> u32 {
        self.gray_percentage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentage_routing() {
        let router = GrayTrafficRouter::new(TrafficSwitchMethod::Percentage, 10);
        let mut gray_count = 0;
        for i in 0..100 {
            if router.is_gray_traffic(i, &[], &[]) {
                gray_count += 1;
            }
        }
        assert_eq!(gray_count, 10);
    }

    #[test]
    fn test_header_routing() {
        let router = GrayTrafficRouter::new(TrafficSwitchMethod::Header, 100);
        assert!(router.is_gray_traffic(1, &[("x-gray-release", "true")], &[]));
        assert!(!router.is_gray_traffic(1, &[("x-gray-release", "false")], &[]));
        assert!(!router.is_gray_traffic(1, &[], &[]));
    }

    #[test]
    fn test_cookie_routing() {
        let router = GrayTrafficRouter::new(TrafficSwitchMethod::Cookie, 100);
        assert!(router.is_gray_traffic(1, &[], &[("gray_release", "true")]));
        assert!(!router.is_gray_traffic(1, &[], &[("gray_release", "false")]));
    }

    #[test]
    fn test_custom_header() {
        let router =
            GrayTrafficRouter::new(TrafficSwitchMethod::Header, 100).with_header("x-canary");
        assert!(router.is_gray_traffic(1, &[("x-canary", "true")], &[]));
    }

    #[test]
    fn test_custom_cookie() {
        let router = GrayTrafficRouter::new(TrafficSwitchMethod::Cookie, 100).with_cookie("canary");
        assert!(router.is_gray_traffic(1, &[], &[("canary", "true")]));
    }

    #[test]
    fn test_zero_percentage() {
        let router = GrayTrafficRouter::new(TrafficSwitchMethod::Percentage, 0);
        for i in 0..100 {
            assert!(!router.is_gray_traffic(i, &[], &[]));
        }
    }
}
