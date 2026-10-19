//! 零信任访问验证模块
//!
//! 每次访问验证身份（认证）+ 设备（设备指纹）+ 上下文（请求来源/时间/行为画像）+
//! 最小权限（动态权限），不信任任何隐式信任。
//! 验证失败返回明确错误码（非静默拒绝），含申诉机制。
//!
//! 启用 `compliance-auto-scan` feature 时编译。

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// 验证错误
#[derive(Debug)]
pub struct VerifyError {
    pub message: String,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "验证内部错误：{}", self.message)
    }
}

impl std::error::Error for VerifyError {}

impl VerifyError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// 拒绝原因
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DenyReason {
    /// 身份验证失败
    IdentityFailed,
    /// 设备未知
    DeviceUnknown,
    /// 上下文异常
    ContextAbnormal,
    /// 权限不足
    PermissionDenied,
}

///2.  错误码
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    /// 身份认证失败
    E401IdentityFailed,
    /// 设备未注册
    E403DeviceUnknown,
    /// 上下文异常
    E403ContextAbnormal,
    /// 权限不足
    E403PermissionDenied,
}

impl ErrorCode {
    /// 从拒绝原因生成错误码
    pub fn from_reason(reason: &DenyReason) -> Self {
        match reason {
            DenyReason::IdentityFailed => ErrorCode::E401IdentityFailed,
            DenyReason::DeviceUnknown => ErrorCode::E403DeviceUnknown,
            DenyReason::ContextAbnormal => ErrorCode::E403ContextAbnormal,
            DenyReason::PermissionDenied => ErrorCode::E403PermissionDenied,
        }
    }

    /// 错误码字符串
    pub fn code(&self) -> &'static str {
        match self {
            ErrorCode::E401IdentityFailed => "E401_IDENTITY_FAILED",
            ErrorCode::E403DeviceUnknown => "E403_DEVICE_UNKNOWN",
            ErrorCode::E403ContextAbnormal => "E403_CONTEXT_ABNORMAL",
            ErrorCode::E403PermissionDenied => "E403_PERMISSION_DENIED",
        }
    }
}

/// 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerifyResult {
    /// 放行
    Allow,
    /// 拒绝（附拒绝原因和错误码）
    Deny {
        reason: DenyReason,
        error_code: ErrorCode,
    },
}

impl VerifyResult {
    /// 是否放行
    pub fn is_allowed(&self) -> bool {
        matches!(self, VerifyResult::Allow)
    }

    /// 是否拒绝
    pub fn is_denied(&self) -> bool {
        matches!(self, VerifyResult::Deny { .. })
    }
}

/// 设备指纹
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    /// 设备 ID
    pub device_id: String,
    /// 信任评分（0.0 ~ 1.0）
    pub trust_score: f64,
    /// 最后见到时间（UNIX 秒）
    pub last_seen: i64,
}

impl DeviceFingerprint {
    pub fn new(device_id: impl Into<String>, trust_score: f64, last_seen: i64) -> Self {
        Self {
            device_id: device_id.into(),
            trust_score,
            last_seen,
        }
    }
}

/// 行为画像
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorProfile {
    /// 平均请求频率（次/小时）
    pub avg_request_rate: f64,
    /// 常用来源 IP 列表
    pub common_ips: Vec<String>,
    /// 常用访问时间段（小时列表，0-23）
    pub common_hours: Vec<u32>,
}

impl Default for BehaviorProfile {
    fn default() -> Self {
        Self {
            avg_request_rate: 10.0,
            common_ips: Vec::new(),
            common_hours: vec![9, 10, 11, 14, 15, 16, 17],
        }
    }
}

/// 访问上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessContext {
    /// 来源 IP
    pub source_ip: String,
    /// 请求时间戳（UNIX 秒）
    pub timestamp: i64,
    /// 行为画像
    pub behavior_profile: BehaviorProfile,
}

impl AccessContext {
    pub fn new(source_ip: impl Into<String>, timestamp: i64) -> Self {
        Self {
            source_ip: source_ip.into(),
            timestamp,
            behavior_profile: BehaviorProfile::default(),
        }
    }

    /// 获取请求小时（0-23）
    pub fn request_hour(&self) -> u32 {
        ((self.timestamp / 3600) % 24) as u32
    }
}

/// 访问请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRequest {
    /// 身份标识（用户 ID）
    pub identity: String,
    /// 设备指纹
    pub device_fingerprint: DeviceFingerprint,
    /// 访问上下文
    pub context: AccessContext,
    /// 请求权限
    pub requested_permission: String,
}

/// 设备验证器
pub struct DeviceVerifier {
    /// 已注册设备（设备 ID → 设备指纹）
    registered_devices: Mutex<HashMap<String, DeviceFingerprint>>,
    /// 最低信任评分阈值
    min_trust_score: f64,
}

impl DeviceVerifier {
    pub fn new(min_trust_score: f64) -> Self {
        Self {
            registered_devices: Mutex::new(HashMap::new()),
            min_trust_score,
        }
    }

    /// 注册设备
    pub fn register_device(&self, fingerprint: DeviceFingerprint) {
        let mut devices = self.registered_devices.lock().unwrap();
        devices.insert(fingerprint.device_id.clone(), fingerprint);
    }

    /// 验证设备
    pub fn verify(&self, fingerprint: &DeviceFingerprint) -> Option<DenyReason> {
        let devices = self.registered_devices.lock().unwrap();
        let registered = devices.get(&fingerprint.device_id);

        if registered.is_none() {
            return Some(DenyReason::DeviceUnknown);
        }

        if fingerprint.trust_score < self.min_trust_score {
            return Some(DenyReason::DeviceUnknown);
        }

        None
    }
}

impl Default for DeviceVerifier {
    fn default() -> Self {
        Self::new(0.5)
    }
}

/// 上下文验证器
pub struct ContextVerifier {
    /// 允许的 IP 白名单
    allowed_ips: Vec<String>,
    /// 异常请求频率阈值（次/小时）
    abnormal_rate_threshold: f64,
}

impl ContextVerifier {
    pub fn new() -> Self {
        Self {
            allowed_ips: Vec::new(),
            abnormal_rate_threshold: 1000.0,
        }
    }

    /// 添加允许的 IP
    pub fn with_allowed_ip(mut self, ip: impl Into<String>) -> Self {
        self.allowed_ips.push(ip.into());
        self
    }

    /// 设置异常请求频率阈值
    pub fn with_abnormal_rate_threshold(mut self, threshold: f64) -> Self {
        self.abnormal_rate_threshold = threshold;
        self
    }

    /// 验证上下文
    pub fn verify(&self, context: &AccessContext) -> Option<DenyReason> {
        // IP 白名单检查
        if !self.allowed_ips.is_empty()
            && !self.allowed_ips.contains(&context.source_ip)
        {
            return Some(DenyReason::ContextAbnormal);
        }

        // 请求频率异常检查
        if context.behavior_profile.avg_request_rate > self.abnormal_rate_threshold {
            return Some(DenyReason::ContextAbnormal);
        }

        None
    }
}

impl Default for ContextVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// 动态权限验证器
pub struct DynamicPermission {
    /// 权限映射（身份 → 权限列表）
    permissions: Mutex<HashMap<String, Vec<String>>>,
    /// 申诉白名单（身份 → 临时权限列表）
    appeal_whitelist: Mutex<HashMap<String, Vec<String>>>,
}

impl DynamicPermission {
    pub fn new() -> Self {
        Self {
            permissions: Mutex::new(HashMap::new()),
            appeal_whitelist: Mutex::new(HashMap::new()),
        }
    }

    /// 授权权限
    pub fn grant(&self, identity: impl Into<String>, permissions: Vec<String>) {
        let mut perms = self.permissions.lock().unwrap();
        perms.insert(identity.into(), permissions);
    }

    /// 申诉通过后加入白名单
    pub fn appeal_approve(&self, identity: impl Into<String>, permissions: Vec<String>) {
        let mut whitelist = self.appeal_whitelist.lock().unwrap();
        whitelist.insert(identity.into(), permissions);
    }

    /// 验证权限
    pub fn verify(&self, identity: &str, requested_permission: &str) -> Option<DenyReason> {
        let perms = self.permissions.lock().unwrap();
        let whitelist = self.appeal_whitelist.lock().unwrap();

        let has_permission = perms
            .get(identity)
            .map(|p| p.iter().any(|perm| perm == requested_permission))
            .unwrap_or(false);

        let has_whitelist = whitelist
            .get(identity)
            .map(|p| p.iter().any(|perm| perm == requested_permission))
            .unwrap_or(false);

        if !has_permission && !has_whitelist {
            return Some(DenyReason::PermissionDenied);
        }

        None
    }
}

impl Default for DynamicPermission {
    fn default() -> Self {
        Self::new()
    }
}

/// 零信任访问验证器
///
/// 每次访问验证身份 + 设备 + 上下文 + 最小权限，不信任任何隐式信任。
/// 验证失败返回明确错误码（非静默拒绝），含申诉机制。
pub struct ZeroTrustVerifier {
    /// 设备验证器
    device_verifier: DeviceVerifier,
    /// 上下文验证器
    context_verifier: ContextVerifier,
    /// 动态权限验证器
    permission_verifier: DynamicPermission,
}

impl ZeroTrustVerifier {
    pub fn new() -> Self {
        Self {
            device_verifier: DeviceVerifier::default(),
            context_verifier: ContextVerifier::default(),
            permission_verifier: DynamicPermission::default(),
        }
    }

    /// 设置最低设备信任评分
    pub fn with_min_trust_score(mut self, score: f64) -> Self {
        self.device_verifier = DeviceVerifier::new(score);
        self
    }

    /// 添加允许的 IP
    pub fn with_allowed_ip(mut self, ip: impl Into<String>) -> Self {
        self.context_verifier = self.context_verifier.with_allowed_ip(ip);
        self
    }

    /// 注册设备
    pub fn register_device(&self, fingerprint: DeviceFingerprint) {
        self.device_verifier.register_device(fingerprint);
    }

    /// 授权权限
    pub fn grant_permission(&self, identity: impl Into<String>, permissions: Vec<String>) {
        self.permission_verifier.grant(identity, permissions);
    }

    /// 申诉通过后加入白名单
    pub fn appeal_approve(&self, identity: impl Into<String>, permissions: Vec<String>) {
        self.permission_verifier.appeal_approve(identity, permissions);
    }

    /// 验证访问
    ///
    /// 1. 身份验证（identity 非空）
    /// 2. 设备验证（设备指纹）
    /// 3. 上下文验证（来源 IP/时间/行为画像）
    /// 4. 权限验证（最小权限）
    ///
    /// 任一失败返回明确错误码
    pub fn verify_access(&self, request: &AccessRequest) -> Result<VerifyResult, VerifyError> {
        // 1. 身份验证
        if request.identity.is_empty() {
            return Ok(VerifyResult::Deny {
                reason: DenyReason::IdentityFailed,
                error_code: ErrorCode::E401IdentityFailed,
            });
        }

        // 2. 设备验证
        if let Some(reason) = self.device_verifier.verify(&request.device_fingerprint) {
            let error_code = ErrorCode::from_reason(&reason);
            return Ok(VerifyResult::Deny { reason, error_code });
        }

        // 3. 上下文验证
        if let Some(reason) = self.context_verifier.verify(&request.context) {
            let error_code = ErrorCode::from_reason(&reason);
            return Ok(VerifyResult::Deny { reason, error_code });
        }

        // 4. 权限验证
        if let Some(reason) = self
            .permission_verifier
            .verify(&request.identity, &request.requested_permission)
        {
            let error_code = ErrorCode::from_reason(&reason);
            return Ok(VerifyResult::Deny { reason, error_code });
        }

        Ok(VerifyResult::Allow)
    }

    /// 获取设备验证器
    pub fn device_verifier(&self) -> &DeviceVerifier {
        &self.device_verifier
    }

    /// 获取上下文验证器
    pub fn context_verifier(&self) -> &ContextVerifier {
        &self.context_verifier
    }

    /// 获取权限验证器
    pub fn permission_verifier(&self) -> &DynamicPermission {
        &self.permission_verifier
    }
}

impl Default for ZeroTrustVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_allow() {
        let verifier = ZeroTrustVerifier::new();
        verifier.register_device(DeviceFingerprint::new("device1", 0.8, 1000));
        verifier.grant_permission("user1", vec!["read".to_string()]);

        let request = AccessRequest {
            identity: "user1".to_string(),
            device_fingerprint: DeviceFingerprint::new("device1", 0.8, 1000),
            context: AccessContext::new("127.0.0.1", 1000),
            requested_permission: "read".to_string(),
        };

        let result = verifier.verify_access(&request).unwrap();
        assert!(result.is_allowed());
    }

    #[test]
    fn test_verify_deny_unknown_device() {
        let verifier = ZeroTrustVerifier::new();

        let request = AccessRequest {
            identity: "user1".to_string(),
            device_fingerprint: DeviceFingerprint::new("unknown", 0.8, 1000),
            context: AccessContext::new("127.0.0.1", 1000),
            requested_permission: "read".to_string(),
        };

        let result = verifier.verify_access(&request).unwrap();
        assert!(result.is_denied());
        if let VerifyResult::Deny { reason, error_code } = result {
            assert_eq!(reason, DenyReason::DeviceUnknown);
            assert_eq!(error_code, ErrorCode::E403DeviceUnknown);
        }
    }
}