//! 刷新令牌存储（Refresh Token Store）
//!
//! 实现安全的刷新令牌管理，支持：
//! - **令牌轮换（Rotation）**：每次使用刷新令牌时签发新令牌，旧令牌立即失效
//! - **令牌撤销（Revocation）**：主动撤销单个令牌或整个令牌家族
//! - **重放检测（Replay Detection）**：检测到已使用的刷新令牌被再次提交时，
//!   撤销该令牌所属的整个家族（Token Family），防止令牌窃取
//! - **家族追踪（Family Tracking）**：同一认证会话产生的所有刷新令牌属于同一家族
//!
//! ## 工作流程
//!
//! 1. 用户登录 -> `issue_family(access, refresh)` 创建新家族
//! 2. 刷新令牌 -> `refresh(old_refresh, new_access, new_refresh)` 轮换令牌
//! 3. 重复使用旧令牌 -> `refresh()` 返回 `TokenFamilyError::ReplayDetected`，
//!    自动撤销整个家族
//! 4. 登出 -> `revoke_token()` 或 `revoke_family()` 撤销令牌

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AuthError;

/// 令牌家族错误
///
/// 表示刷新令牌操作中的安全相关错误。
#[derive(Debug)]
pub enum TokenFamilyError {
    /// 刷新令牌不存在或已被撤销
    NotFound(String),
    /// 刷新令牌已被使用（重放攻击检测）
    ///
    /// 当已使用的刷新令牌被再次提交时返回此错误。
    /// 调用方应立即撤销该令牌所属的整个家族。
    ReplayDetected(String),
    /// 刷新令牌已过期
    Expired(String),
    /// 家族已被撤销
    FamilyRevoked(String),
}

impl std::fmt::Display for TokenFamilyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenFamilyError::NotFound(msg) => write!(f, "Token not found: {}", msg),
            TokenFamilyError::ReplayDetected(msg) => {
                write!(f, "Replay detected (token already used): {}", msg)
            }
            TokenFamilyError::Expired(msg) => write!(f, "Token expired: {}", msg),
            TokenFamilyError::FamilyRevoked(msg) => {
                write!(f, "Token family revoked: {}", msg)
            }
        }
    }
}

impl std::error::Error for TokenFamilyError {}

impl From<TokenFamilyError> for AuthError {
    fn from(e: TokenFamilyError) -> Self {
        match e {
            TokenFamilyError::NotFound(msg) => AuthError::TokenInvalid(msg),
            TokenFamilyError::ReplayDetected(msg) => AuthError::TokenInvalid(msg),
            TokenFamilyError::Expired(msg) => AuthError::TokenExpired(msg),
            TokenFamilyError::FamilyRevoked(msg) => AuthError::TokenInvalid(msg),
        }
    }
}

/// 存储的令牌元数据
#[derive(Debug, Clone)]
pub struct StoredToken {
    /// 令牌值（refresh token 字符串）
    pub token: String,
    /// 所属家族 ID
    pub family_id: String,
    /// 关联的用户 ID
    pub user_id: i64,
    /// 创建时间（Unix 秒）
    pub created_at: i64,
    /// 过期时间（Unix 秒）
    pub expires_at: i64,
    /// 是否已被使用（轮换后标记为 true）
    pub used: bool,
    /// 是否已被主动撤销
    pub revoked: bool,
}

impl StoredToken {
    /// 创建新的存储令牌
    pub fn new(
        token: impl Into<String>,
        family_id: impl Into<String>,
        user_id: i64,
        expires_at: i64,
    ) -> Self {
        Self {
            token: token.into(),
            family_id: family_id.into(),
            user_id,
            created_at: current_secs(),
            expires_at,
            used: false,
            revoked: false,
        }
    }

    /// 是否已过期
    pub fn is_expired(&self) -> bool {
        current_secs() > self.expires_at
    }

    /// 是否有效（未使用、未撤销、未过期）
    pub fn is_valid(&self) -> bool {
        !self.used && !self.revoked && !self.is_expired()
    }
}

/// 家族元数据
#[derive(Debug)]
struct FamilyInfo {
    /// 家族是否已被撤销
    revoked: bool,
    /// 家族中的所有令牌值
    tokens: Vec<String>,
}

/// 刷新令牌存储
///
/// 管理刷新令牌的生命周期，支持轮换、撤销和重放检测。
/// 使用 `Mutex<HashMap>` 进行线程安全存储。
pub struct TokenStore {
    /// 令牌值 -> 存储的令牌元数据
    tokens: Mutex<HashMap<String, StoredToken>>,
    /// 家族 ID -> 家族信息
    families: Mutex<HashMap<String, FamilyInfo>>,
    /// 刷新令牌默认有效期（秒）
    default_refresh_lifetime: i64,
}

impl TokenStore {
    /// 创建新的令牌存储，刷新令牌默认有效期 7 天
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
            families: Mutex::new(HashMap::new()),
            default_refresh_lifetime: 7 * 24 * 3600,
        }
    }

    /// 配置刷新令牌默认有效期（秒）
    pub fn with_refresh_lifetime(mut self, seconds: i64) -> Self {
        self.default_refresh_lifetime = seconds;
        self
    }

    /// 签发新的令牌家族（用户登录时调用）
    ///
    /// 创建一个新的令牌家族，并存储初始刷新令牌。
    /// 返回创建的 `StoredToken` 供调用方返回给客户端。
    pub fn issue_family(
        &self,
        refresh_token: impl Into<String>,
        user_id: i64,
    ) -> Result<StoredToken, AuthError> {
        let token_value = refresh_token.into();
        let family_id = generate_family_id();
        let expires_at = current_secs() + self.default_refresh_lifetime;

        let stored = StoredToken::new(token_value.clone(), family_id.clone(), user_id, expires_at);

        self.tokens
            .lock()
            .unwrap()
            .insert(token_value.clone(), stored.clone());

        self.families.lock().unwrap().insert(
            family_id.clone(),
            FamilyInfo {
                revoked: false,
                tokens: vec![token_value],
            },
        );

        Ok(stored)
    }

    /// 刷新令牌（轮换）
    ///
    /// 验证旧刷新令牌有效后，标记其为已使用，并签发新的刷新令牌（同一家族）。
    ///
    /// # 安全机制
    ///
    /// 1. 如果旧令牌已被使用 -> 返回 `ReplayDetected`，撤销整个家族
    /// 2. 如果旧令牌已被撤销 -> 返回 `NotFound`
    /// 3. 如果旧令牌已过期 -> 返回 `Expired`
    /// 4. 如果家族已被撤销 -> 返回 `FamilyRevoked`
    pub fn refresh(
        &self,
        old_refresh_token: &str,
        new_refresh_token: impl Into<String>,
    ) -> Result<StoredToken, TokenFamilyError> {
        let new_token_value = new_refresh_token.into();
        let now = current_secs();
        let expires_at = now + self.default_refresh_lifetime;

        // 第一阶段：读取并验证旧令牌状态（不加写锁，避免与 revoke 冲突）
        let (family_id, user_id, is_used, is_revoked, is_expired, family_revoked) = {
            let tokens = self.tokens.lock().unwrap();
            let old_stored = match tokens.get(old_refresh_token) {
                Some(t) => t,
                None => {
                    return Err(TokenFamilyError::NotFound(
                        "Refresh token not found".to_string(),
                    ))
                }
            };

            let family_id = old_stored.family_id.clone();
            let user_id = old_stored.user_id;
            let is_used = old_stored.used;
            let is_revoked = old_stored.revoked;
            let is_expired = old_stored.is_expired();

            let family_revoked = {
                let families = self.families.lock().unwrap();
                families
                    .get(&family_id)
                    .map(|f| f.revoked)
                    .unwrap_or(false)
            };

            (
                family_id,
                user_id,
                is_used,
                is_revoked,
                is_expired,
                family_revoked,
            )
        };

        // 检查家族是否已被撤销
        if family_revoked {
            return Err(TokenFamilyError::FamilyRevoked(format!(
                "Family {} has been revoked",
                family_id
            )));
        }

        // 检查令牌是否已被撤销
        if is_revoked {
            return Err(TokenFamilyError::NotFound(
                "Refresh token has been revoked".to_string(),
            ));
        }

        // 检查令牌是否已过期
        if is_expired {
            return Err(TokenFamilyError::Expired(
                "Refresh token has expired".to_string(),
            ));
        }

        // 重放检测：令牌已被使用 -> 撤销整个家族
        // 注意：此时未持有 tokens 锁，revoke_family_internal 可以安全获取锁
        if is_used {
            self.revoke_family_internal(&family_id);
            return Err(TokenFamilyError::ReplayDetected(format!(
                "Refresh token already used (family {} revoked)",
                family_id
            )));
        }

        // 第二阶段：标记旧令牌为已使用，并创建新令牌
        let new_stored =
            StoredToken::new(new_token_value.clone(), family_id.clone(), user_id, expires_at);

        {
            let mut tokens = self.tokens.lock().unwrap();
            // 再次检查令牌状态（防止 TOCTOU：在两次加锁之间令牌可能被撤销或使用）
            let old = match tokens.get_mut(old_refresh_token) {
                Some(t) => t,
                None => {
                    return Err(TokenFamilyError::NotFound(
                        "Refresh token not found".to_string(),
                    ))
                }
            };

            if old.used {
                // 在释放锁的窗口内被使用 -> 重放
                drop(tokens);
                self.revoke_family_internal(&family_id);
                return Err(TokenFamilyError::ReplayDetected(format!(
                    "Refresh token already used (family {} revoked)",
                    family_id
                )));
            }
            if old.revoked {
                return Err(TokenFamilyError::NotFound(
                    "Refresh token has been revoked".to_string(),
                ));
            }

            old.used = true;
            tokens.insert(new_token_value.clone(), new_stored.clone());
        }

        // 将新令牌添加到家族
        {
            let mut families = self.families.lock().unwrap();
            if let Some(family) = families.get_mut(&family_id) {
                family.tokens.push(new_token_value);
            }
        }

        Ok(new_stored)
    }

    /// 撤销单个令牌
    ///
    /// 标记令牌为已撤销，但不影响家族中的其他令牌。
    /// 适用于用户登出单个设备的场景。
    pub fn revoke_token(&self, token: &str) -> Result<(), TokenFamilyError> {
        let mut tokens = self.tokens.lock().unwrap();
        let stored = tokens
            .get_mut(token)
            .ok_or_else(|| TokenFamilyError::NotFound("Token not found".to_string()))?;
        stored.revoked = true;
        Ok(())
    }

    /// 撤销整个令牌家族
    ///
    /// 撤销家族中的所有令牌。适用于：
    /// - 用户修改密码
    /// - 检测到重放攻击
    /// - 管理员强制下线
    pub fn revoke_family(&self, family_id: &str) -> Result<usize, TokenFamilyError> {
        // 先验证家族存在
        {
            let families = self.families.lock().unwrap();
            if !families.contains_key(family_id) {
                return Err(TokenFamilyError::NotFound("Family not found".to_string()));
            }
        }
        // 实际撤销由内部方法处理
        Ok(self.revoke_family_internal(family_id))
    }

    /// 撤销家族的内部实现（不加锁冲突）
    ///
    /// 返回撤销的令牌数量。
    fn revoke_family_internal(&self, family_id: &str) -> usize {
        let token_values: Vec<String> = {
            let mut families = self.families.lock().unwrap();
            if let Some(family) = families.get_mut(family_id) {
                family.revoked = true;
                family.tokens.clone()
            } else {
                return 0;
            }
        };

        let mut tokens = self.tokens.lock().unwrap();
        let mut count = 0;
        for tv in &token_values {
            if let Some(stored) = tokens.get_mut(tv) {
                stored.revoked = true;
                count += 1;
            }
        }
        count
    }

    /// 撤销用户的所有令牌
    ///
    /// 撤销属于指定用户的所有令牌家族。
    /// 适用于用户修改密码、账户被禁用等场景。
    pub fn revoke_user(&self, user_id: i64) -> usize {
        let family_ids: Vec<String> = {
            let tokens = self.tokens.lock().unwrap();
            tokens
                .values()
                .filter(|t| t.user_id == user_id)
                .map(|t| t.family_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect()
        };

        let mut total = 0;
        for fid in family_ids {
            total += self.revoke_family_internal(&fid);
        }
        total
    }

    /// 验证令牌是否有效
    pub fn is_valid(&self, token: &str) -> bool {
        let tokens = self.tokens.lock().unwrap();
        tokens.get(token).map(|t| t.is_valid()).unwrap_or(false)
    }

    /// 获取令牌信息
    pub fn get_token(&self, token: &str) -> Option<StoredToken> {
        self.tokens.lock().unwrap().get(token).cloned()
    }

    /// 获取家族中的所有令牌
    pub fn family_tokens(&self, family_id: &str) -> Vec<StoredToken> {
        let token_values: Vec<String> = {
            let families = self.families.lock().unwrap();
            families
                .get(family_id)
                .map(|f| f.tokens.clone())
                .unwrap_or_default()
        };

        let tokens = self.tokens.lock().unwrap();
        token_values
            .iter()
            .filter_map(|tv| tokens.get(tv).cloned())
            .collect()
    }

    /// 检查家族是否已被撤销
    pub fn is_family_revoked(&self, family_id: &str) -> bool {
        self.families
            .lock()
            .unwrap()
            .get(family_id)
            .map(|f| f.revoked)
            .unwrap_or(false)
    }

    /// 清理已过期和已撤销的令牌
    ///
    /// 返回清理的令牌数量。
    pub fn cleanup(&self) -> usize {
        let mut tokens = self.tokens.lock().unwrap();
        let before = tokens.len();
        tokens.retain(|_, t| !t.is_expired() && !t.revoked);
        before - tokens.len()
    }

    /// 返回当前存储的令牌数量
    pub fn token_count(&self) -> usize {
        self.tokens.lock().unwrap().len()
    }

    /// 返回当前存储的家族数量
    pub fn family_count(&self) -> usize {
        self.families.lock().unwrap().len()
    }
}

impl Default for TokenStore {
    fn default() -> Self {
        Self::new()
    }
}

/// 生成随机家族 ID（32 字节十六进制）
fn generate_family_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    current_nanos().hash(&mut hasher);
    let seed = hasher.finish();
    format!("fam_{:016x}", seed)
}

fn current_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn current_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_token_new() {
        let token = StoredToken::new("tok", "fam1", 42, current_secs() + 3600);
        assert_eq!(token.token, "tok");
        assert_eq!(token.family_id, "fam1");
        assert_eq!(token.user_id, 42);
        assert!(!token.used);
        assert!(!token.revoked);
        assert!(token.is_valid());
    }

    #[test]
    fn test_stored_token_is_expired() {
        let mut token = StoredToken::new("tok", "fam1", 1, current_secs() + 3600);
        assert!(!token.is_expired());
        token.expires_at = current_secs() - 100;
        assert!(token.is_expired());
    }

    #[test]
    fn test_stored_token_is_valid() {
        let mut token = StoredToken::new("tok", "fam1", 1, current_secs() + 3600);
        assert!(token.is_valid());

        token.used = true;
        assert!(!token.is_valid());

        token.used = false;
        token.revoked = true;
        assert!(!token.is_valid());

        token.revoked = false;
        token.expires_at = current_secs() - 100;
        assert!(!token.is_valid());
    }

    #[test]
    fn test_token_store_issue_family() {
        let store = TokenStore::new();
        let stored = store.issue_family("refresh1", 100).unwrap();
        assert_eq!(stored.user_id, 100);
        assert!(!stored.family_id.is_empty());
        assert!(stored.is_valid());
        assert_eq!(store.token_count(), 1);
        assert_eq!(store.family_count(), 1);
    }

    #[test]
    fn test_token_store_refresh_success() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();

        let new_token = store.refresh("refresh1", "refresh2").unwrap();
        assert_eq!(new_token.user_id, 100);
        assert_eq!(new_token.family_id, store.get_token("refresh1").unwrap().family_id);
        assert!(new_token.is_valid());

        // 旧令牌应标记为已使用
        let old = store.get_token("refresh1").unwrap();
        assert!(old.used);
        assert!(!old.is_valid());
    }

    #[test]
    fn test_token_store_refresh_not_found() {
        let store = TokenStore::new();
        let result = store.refresh("nonexistent", "new");
        assert!(matches!(result, Err(TokenFamilyError::NotFound(_))));
    }

    #[test]
    fn test_token_store_refresh_replay_detected() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();

        // 第一次刷新成功
        store.refresh("refresh1", "refresh2").unwrap();

        // 第二次使用同一个旧令牌 -> 重放检测
        let result = store.refresh("refresh1", "refresh3");
        assert!(matches!(result, Err(TokenFamilyError::ReplayDetected(_))));

        // 整个家族应被撤销
        let family_id = store.get_token("refresh1").unwrap().family_id;
        assert!(store.is_family_revoked(&family_id));

        // refresh2 也应被撤销
        let r2 = store.get_token("refresh2").unwrap();
        assert!(r2.revoked);
        assert!(!r2.is_valid());
    }

    #[test]
    fn test_token_store_refresh_expired() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();

        // 手动将令牌设为过期
        {
            let mut tokens = store.tokens.lock().unwrap();
            tokens.get_mut("refresh1").unwrap().expires_at = current_secs() - 100;
        }

        let result = store.refresh("refresh1", "refresh2");
        assert!(matches!(result, Err(TokenFamilyError::Expired(_))));
    }

    #[test]
    fn test_token_store_refresh_revoked_token() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        store.revoke_token("refresh1").unwrap();

        let result = store.refresh("refresh1", "refresh2");
        assert!(matches!(result, Err(TokenFamilyError::NotFound(_))));
    }

    #[test]
    fn test_token_store_revoke_token() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        assert!(store.is_valid("refresh1"));

        store.revoke_token("refresh1").unwrap();
        assert!(!store.is_valid("refresh1"));
    }

    #[test]
    fn test_token_store_revoke_token_not_found() {
        let store = TokenStore::new();
        let result = store.revoke_token("nonexistent");
        assert!(matches!(result, Err(TokenFamilyError::NotFound(_))));
    }

    #[test]
    fn test_token_store_revoke_family() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        let family_id = store.get_token("refresh1").unwrap().family_id;

        // 刷新生成 refresh2（同家族）
        store.refresh("refresh1", "refresh2").unwrap();
        assert!(store.is_valid("refresh2"));

        // 撤销整个家族
        let count = store.revoke_family(&family_id).unwrap();
        assert!(count >= 2);

        // refresh2 也应被撤销
        assert!(!store.is_valid("refresh2"));
        assert!(store.is_family_revoked(&family_id));
    }

    #[test]
    fn test_token_store_revoke_family_not_found() {
        let store = TokenStore::new();
        let result = store.revoke_family("nonexistent");
        assert!(matches!(result, Err(TokenFamilyError::NotFound(_))));
    }

    #[test]
    fn test_token_store_revoke_user() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        store.issue_family("refresh3", 100).unwrap();
        store.issue_family("refresh5", 200).unwrap();

        let count = store.revoke_user(100);
        assert!(count >= 2);

        assert!(!store.is_valid("refresh1"));
        assert!(!store.is_valid("refresh3"));
        // user 200 的令牌不受影响
        assert!(store.is_valid("refresh5"));
    }

    #[test]
    fn test_token_store_revoke_user_no_tokens() {
        let store = TokenStore::new();
        let count = store.revoke_user(999);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_token_store_is_valid() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        assert!(store.is_valid("refresh1"));
        assert!(!store.is_valid("nonexistent"));
    }

    #[test]
    fn test_token_store_get_token() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        let stored = store.get_token("refresh1").unwrap();
        assert_eq!(stored.user_id, 100);
        assert!(store.get_token("nonexistent").is_none());
    }

    #[test]
    fn test_token_store_family_tokens() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        let family_id = store.get_token("refresh1").unwrap().family_id;

        store.refresh("refresh1", "refresh2").unwrap();
        store.refresh("refresh2", "refresh3").unwrap();

        let tokens = store.family_tokens(&family_id);
        assert_eq!(tokens.len(), 3);
    }

    #[test]
    fn test_token_store_family_tokens_nonexistent() {
        let store = TokenStore::new();
        let tokens = store.family_tokens("nonexistent");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_token_store_is_family_revoked() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        let family_id = store.get_token("refresh1").unwrap().family_id;

        assert!(!store.is_family_revoked(&family_id));
        store.revoke_family(&family_id).unwrap();
        assert!(store.is_family_revoked(&family_id));
        assert!(!store.is_family_revoked("nonexistent"));
    }

    #[test]
    fn test_token_store_cleanup() {
        let store = TokenStore::new();
        store.issue_family("refresh1", 100).unwrap();
        store.issue_family("refresh2", 200).unwrap();

        // 手动将 refresh1 设为过期
        {
            let mut tokens = store.tokens.lock().unwrap();
            tokens.get_mut("refresh1").unwrap().expires_at = current_secs() - 100;
        }

        let removed = store.cleanup();
        assert_eq!(removed, 1);
        assert_eq!(store.token_count(), 1);
    }

    #[test]
    fn test_token_store_with_refresh_lifetime() {
        let store = TokenStore::new().with_refresh_lifetime(3600);
        let stored = store.issue_family("refresh1", 100).unwrap();
        // 过期时间应在 3600 秒左右
        let now = current_secs();
        assert!(stored.expires_at > now + 3500);
        assert!(stored.expires_at < now + 3700);
    }

    #[test]
    fn test_token_store_multi_refresh_chain() {
        // 模拟多次刷新的链式场景
        let store = TokenStore::new();
        store.issue_family("r1", 1).unwrap();

        let r2 = store.refresh("r1", "r2").unwrap();
        let r3 = store.refresh("r2", "r3").unwrap();
        let r4 = store.refresh("r3", "r4").unwrap();

        // 所有令牌属于同一家族
        assert_eq!(r2.family_id, r3.family_id);
        assert_eq!(r3.family_id, r4.family_id);

        // r1, r2, r3 应已使用
        assert!(store.get_token("r1").unwrap().used);
        assert!(store.get_token("r2").unwrap().used);
        assert!(store.get_token("r3").unwrap().used);
        // r4 应未使用且有效
        assert!(!store.get_token("r4").unwrap().used);
        assert!(store.is_valid("r4"));
    }

    #[test]
    fn test_token_store_replay_after_chain() {
        // 在链式刷新后，重放中间的令牌
        let store = TokenStore::new();
        store.issue_family("r1", 1).unwrap();
        store.refresh("r1", "r2").unwrap();
        store.refresh("r2", "r3").unwrap();

        // 重放 r2（已使用）
        let result = store.refresh("r2", "r4");
        assert!(matches!(result, Err(TokenFamilyError::ReplayDetected(_))));

        // 整个家族被撤销
        let family_id = store.get_token("r1").unwrap().family_id;
        assert!(store.is_family_revoked(&family_id));
        // r3 也应被撤销
        assert!(!store.is_valid("r3"));
    }

    #[test]
    fn test_token_store_default() {
        let store = TokenStore::default();
        assert_eq!(store.token_count(), 0);
        assert_eq!(store.family_count(), 0);
    }

    #[test]
    fn test_token_store_token_count() {
        let store = TokenStore::new();
        assert_eq!(store.token_count(), 0);
        store.issue_family("r1", 1).unwrap();
        assert_eq!(store.token_count(), 1);
        store.refresh("r1", "r2").unwrap();
        assert_eq!(store.token_count(), 2);
    }

    #[test]
    fn test_token_store_family_count() {
        let store = TokenStore::new();
        assert_eq!(store.family_count(), 0);
        store.issue_family("r1", 1).unwrap();
        assert_eq!(store.family_count(), 1);
        store.issue_family("r2", 2).unwrap();
        assert_eq!(store.family_count(), 2);
        // 刷新不增加家族数
        store.refresh("r1", "r3").unwrap();
        assert_eq!(store.family_count(), 2);
    }

    #[test]
    fn test_token_family_error_display() {
        let e1 = TokenFamilyError::NotFound("test".to_string());
        assert!(e1.to_string().contains("Token not found"));

        let e2 = TokenFamilyError::ReplayDetected("test".to_string());
        assert!(e2.to_string().contains("Replay detected"));

        let e3 = TokenFamilyError::Expired("test".to_string());
        assert!(e3.to_string().contains("Token expired"));

        let e4 = TokenFamilyError::FamilyRevoked("test".to_string());
        assert!(e4.to_string().contains("Token family revoked"));
    }

    #[test]
    fn test_token_family_error_to_auth_error() {
        let e: AuthError = TokenFamilyError::NotFound("test".to_string()).into();
        assert!(matches!(e, AuthError::TokenInvalid(_)));

        let e: AuthError = TokenFamilyError::ReplayDetected("test".to_string()).into();
        assert!(matches!(e, AuthError::TokenInvalid(_)));

        let e: AuthError = TokenFamilyError::Expired("test".to_string()).into();
        assert!(matches!(e, AuthError::TokenExpired(_)));

        let e: AuthError = TokenFamilyError::FamilyRevoked("test".to_string()).into();
        assert!(matches!(e, AuthError::TokenInvalid(_)));
    }

    #[test]
    fn test_generate_family_id_format() {
        let id = generate_family_id();
        assert!(id.starts_with("fam_"));
        assert!(id.len() > 4);
    }

    #[test]
    fn test_generate_family_id_different() {
        let id1 = generate_family_id();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let id2 = generate_family_id();
        assert_ne!(id1, id2);
    }
}
