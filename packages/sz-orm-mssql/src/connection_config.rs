//! SQL Server 连接配置
//!
//! 提供 [`MssqlConnectionConfig`] 用于以类型安全方式构建 SQL Server 连接字符串，
//! 支持认证方式、加密、连接超时、多子网故障转移等。

use std::fmt;
use std::time::Duration;

/// SQL Server 认证方式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticationMode {
    /// SQL Server 认证（用户名 + 密码）
    SqlServer { user: String, password: String },
    /// Windows 认证（Integrated Security）
    Windows,
    /// Azure Active Directory 密码认证
    AzurePassword { user: String, password: String },
    /// Azure Active Directory 集成认证
    AzureIntegrated,
    /// Azure Active Directory 服务主体认证
    AzureServicePrincipal {
        client_id: String,
        client_secret: String,
        tenant_id: String,
    },
}

impl AuthenticationMode {
    /// 生成连接字符串片段
    #[must_use]
    pub fn to_connection_string(&self) -> String {
        match self {
            AuthenticationMode::SqlServer { user, password } => {
                format!("User Id={user};Password={password}")
            }
            AuthenticationMode::Windows => "Integrated Security=SSPI".to_string(),
            AuthenticationMode::AzurePassword { user, password } => {
                format!(
                    "Authentication=Active Directory Password;User Id={user};Password={password}"
                )
            }
            AuthenticationMode::AzureIntegrated => {
                "Authentication=Active Directory Integrated".to_string()
            }
            AuthenticationMode::AzureServicePrincipal {
                client_id,
                client_secret,
                tenant_id,
            } => {
                format!(
                    "Authentication=Active Directory Service Principal;User Id={client_id};Password={client_secret};Tenant Id={tenant_id}"
                )
            }
        }
    }

    /// 是否使用 Windows 认证
    #[must_use]
    pub fn is_windows(&self) -> bool {
        matches!(self, AuthenticationMode::Windows)
    }

    /// 是否使用 Azure AD 认证
    #[must_use]
    pub fn is_azure(&self) -> bool {
        matches!(
            self,
            AuthenticationMode::AzurePassword { .. }
                | AuthenticationMode::AzureIntegrated
                | AuthenticationMode::AzureServicePrincipal { .. }
        )
    }
}

impl fmt::Display for AuthenticationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_connection_string())
    }
}

/// 加密配置
#[derive(Debug, Clone)]
pub struct EncryptionConfig {
    /// 是否启用加密
    pub encrypt: bool,
    /// 是否信任服务器证书
    pub trust_server_certificate: bool,
    /// 证书路径
    pub certificate: Option<String>,
    /// TLS 版本
    pub tls_version: Option<TlsVersion>,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            encrypt: true,
            trust_server_certificate: false,
            certificate: None,
            tls_version: None,
        }
    }
}

impl EncryptionConfig {
    /// 创建默认加密配置
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 启用加密
    #[must_use]
    pub fn with_encrypt(mut self) -> Self {
        self.encrypt = true;
        self
    }

    /// 禁用加密
    #[must_use]
    pub fn without_encrypt(mut self) -> Self {
        self.encrypt = false;
        self
    }

    /// 信任服务器证书
    #[must_use]
    pub fn with_trust_server_certificate(mut self) -> Self {
        self.trust_server_certificate = true;
        self
    }

    /// 设置证书路径
    #[must_use]
    pub fn with_certificate(mut self, path: &str) -> Self {
        self.certificate = Some(path.to_string());
        self
    }

    /// 设置 TLS 版本
    #[must_use]
    pub fn with_tls_version(mut self, version: TlsVersion) -> Self {
        self.tls_version = Some(version);
        self
    }

    /// 生成连接字符串片段
    #[must_use]
    pub fn to_connection_string(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!(
            "Encrypt={}",
            if self.encrypt { "Yes" } else { "No" }
        ));
        parts.push(format!(
            "TrustServerCertificate={}",
            if self.trust_server_certificate {
                "Yes"
            } else {
                "No"
            }
        ));
        if let Some(ref cert) = self.certificate {
            parts.push(format!("Certificate={cert}"));
        }
        if let Some(ref tls) = self.tls_version {
            parts.push(format!("TLS={tls}"));
        }
        parts.join(";")
    }
}

/// TLS 版本
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    /// TLS 1.0
    Tls10,
    /// TLS 1.1
    Tls11,
    /// TLS 1.2
    Tls12,
    /// TLS 1.3
    Tls13,
}

impl fmt::Display for TlsVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlsVersion::Tls10 => f.write_str("1.0"),
            TlsVersion::Tls11 => f.write_str("1.1"),
            TlsVersion::Tls12 => f.write_str("1.2"),
            TlsVersion::Tls13 => f.write_str("1.3"),
        }
    }
}

/// 连接池配置
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// 是否启用连接池
    pub enabled: bool,
    /// 最大池大小
    pub max_pool_size: usize,
    /// 最小池大小
    pub min_pool_size: usize,
    /// 连接生命周期（秒）
    pub connection_lifetime: Option<u64>,
    /// 连接空闲超时（秒）
    pub idle_timeout: Option<u64>,
    /// 负载均衡超时（秒）
    pub load_balance_timeout: Option<u64>,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_pool_size: 100,
            min_pool_size: 0,
            connection_lifetime: None,
            idle_timeout: None,
            load_balance_timeout: None,
        }
    }
}

impl ConnectionPoolConfig {
    /// 创建默认配置
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置最大池大小
    #[must_use]
    pub fn with_max_pool_size(mut self, size: usize) -> Self {
        self.max_pool_size = size.max(1);
        self
    }

    /// 设置最小池大小
    #[must_use]
    pub fn with_min_pool_size(mut self, size: usize) -> Self {
        self.min_pool_size = size;
        self
    }

    /// 设置连接生命周期
    #[must_use]
    pub fn with_connection_lifetime(mut self, seconds: u64) -> Self {
        self.connection_lifetime = Some(seconds);
        self
    }

    /// 禁用连接池
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 生成连接字符串片段
    #[must_use]
    pub fn to_connection_string(&self) -> String {
        if !self.enabled {
            return "Pooling=False".to_string();
        }
        let mut parts = Vec::new();
        parts.push(format!("Max Pool Size={}", self.max_pool_size));
        parts.push(format!("Min Pool Size={}", self.min_pool_size));
        if let Some(lt) = self.connection_lifetime {
            parts.push(format!("Connection Lifetime={lt}"));
        }
        if let Some(it) = self.idle_timeout {
            parts.push(format!("Idle Timeout={it}"));
        }
        if let Some(lbt) = self.load_balance_timeout {
            parts.push(format!("Load Balance Timeout={lbt}"));
        }
        parts.join(";")
    }
}

/// SQL Server 连接配置
#[derive(Debug, Clone)]
pub struct MssqlConnectionConfig {
    /// 服务器地址
    pub server: String,
    /// 端口（默认 1433）
    pub port: u16,
    /// 数据库名
    pub database: String,
    /// 认证方式
    pub authentication: AuthenticationMode,
    /// 加密配置
    pub encryption: EncryptionConfig,
    /// 连接池配置
    pub pool: ConnectionPoolConfig,
    /// 连接超时
    pub connect_timeout: Option<Duration>,
    /// 命令超时
    pub command_timeout: Option<Duration>,
    /// 应用名
    pub application_name: Option<String>,
    /// 工作站 ID
    pub workstation_id: Option<String>,
    /// 是否启用多子网故障转移
    pub multi_subnet_failover: bool,
    /// 是否启用故障转移伙伴
    pub failover_partner: Option<String>,
    /// 是否启用 Always Encrypted
    pub always_encrypted: bool,
}

impl MssqlConnectionConfig {
    /// 创建新的连接配置（SQL Server 认证）
    #[must_use]
    pub fn new(server: &str, database: &str, user: &str, password: &str) -> Self {
        Self {
            server: server.to_string(),
            port: 1433,
            database: database.to_string(),
            authentication: AuthenticationMode::SqlServer {
                user: user.to_string(),
                password: password.to_string(),
            },
            encryption: EncryptionConfig::default(),
            pool: ConnectionPoolConfig::default(),
            connect_timeout: None,
            command_timeout: None,
            application_name: None,
            workstation_id: None,
            multi_subnet_failover: false,
            failover_partner: None,
            always_encrypted: false,
        }
    }

    /// 创建使用 Windows 认证的连接配置
    #[must_use]
    pub fn windows_auth(server: &str, database: &str) -> Self {
        Self {
            server: server.to_string(),
            port: 1433,
            database: database.to_string(),
            authentication: AuthenticationMode::Windows,
            encryption: EncryptionConfig::default(),
            pool: ConnectionPoolConfig::default(),
            connect_timeout: None,
            command_timeout: None,
            application_name: None,
            workstation_id: None,
            multi_subnet_failover: false,
            failover_partner: None,
            always_encrypted: false,
        }
    }

    /// 设置端口
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// 设置认证方式
    #[must_use]
    pub fn with_authentication(mut self, auth: AuthenticationMode) -> Self {
        self.authentication = auth;
        self
    }

    /// 设置加密配置
    #[must_use]
    pub fn with_encryption(mut self, encryption: EncryptionConfig) -> Self {
        self.encryption = encryption;
        self
    }

    /// 设置连接池配置
    #[must_use]
    pub fn with_pool(mut self, pool: ConnectionPoolConfig) -> Self {
        self.pool = pool;
        self
    }

    /// 设置连接超时
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// 设置命令超时
    #[must_use]
    pub fn with_command_timeout(mut self, timeout: Duration) -> Self {
        self.command_timeout = Some(timeout);
        self
    }

    /// 设置应用名
    #[must_use]
    pub fn with_application_name(mut self, name: &str) -> Self {
        self.application_name = Some(name.to_string());
        self
    }

    /// 启用多子网故障转移
    #[must_use]
    pub fn with_multi_subnet_failover(mut self) -> Self {
        self.multi_subnet_failover = true;
        self
    }

    /// 设置故障转移伙伴
    #[must_use]
    pub fn with_failover_partner(mut self, partner: &str) -> Self {
        self.failover_partner = Some(partner.to_string());
        self
    }

    /// 启用 Always Encrypted
    #[must_use]
    pub fn with_always_encrypted(mut self) -> Self {
        self.always_encrypted = true;
        self
    }

    /// 生成连接字符串
    #[must_use]
    pub fn to_connection_string(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("Server={},{},1433", self.server, self.port));
        parts.push(format!("Database={}", self.database));
        parts.push(self.authentication.to_connection_string());
        parts.push(self.encryption.to_connection_string());
        parts.push(self.pool.to_connection_string());
        if let Some(t) = self.connect_timeout {
            parts.push(format!("Connect Timeout={}", t.as_secs()));
        }
        if let Some(t) = self.command_timeout {
            parts.push(format!("Command Timeout={}", t.as_secs()));
        }
        if let Some(ref name) = self.application_name {
            parts.push(format!("Application Name={name}"));
        }
        if let Some(ref wid) = self.workstation_id {
            parts.push(format!("Workstation Id={wid}"));
        }
        if self.multi_subnet_failover {
            parts.push("MultiSubnetFailover=True".to_string());
        }
        if let Some(ref partner) = self.failover_partner {
            parts.push(format!("Failover Partner={partner}"));
        }
        if self.always_encrypted {
            parts.push("Column Encryption Setting=Enabled".to_string());
        }
        parts.join(";")
    }

    /// 生成 ADO.NET 连接字符串
    #[must_use]
    pub fn to_adonet_string(&self) -> String {
        self.to_connection_string()
    }

    /// 生成 JDBC 连接字符串
    #[must_use]
    pub fn to_jdbc_url(&self) -> String {
        let auth = match &self.authentication {
            AuthenticationMode::SqlServer { user, password } => {
                format!(";user={user};password={password}")
            }
            AuthenticationMode::Windows => ";integratedSecurity=true".to_string(),
            _ => String::new(),
        };
        format!(
            "jdbc:sqlserver://{}:{};databaseName={}{}",
            self.server, self.port, self.database, auth
        )
    }

    /// 验证配置
    ///
    /// # Errors
    ///
    /// 若配置无效返回 `Err`。
    pub fn validate(&self) -> Result<(), String> {
        if self.server.is_empty() {
            return Err("server cannot be empty".to_string());
        }
        if self.database.is_empty() {
            return Err("database cannot be empty".to_string());
        }
        if self.port == 0 {
            return Err("port cannot be 0".to_string());
        }
        Ok(())
    }
}

impl fmt::Display for MssqlConnectionConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_connection_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_sql_server() {
        let auth = AuthenticationMode::SqlServer {
            user: "sa".to_string(),
            password: "pwd".to_string(),
        };
        let cs = auth.to_connection_string();
        assert_eq!(cs, "User Id=sa;Password=pwd");
    }

    #[test]
    fn test_auth_windows() {
        let auth = AuthenticationMode::Windows;
        assert_eq!(auth.to_connection_string(), "Integrated Security=SSPI");
        assert!(auth.is_windows());
    }

    #[test]
    fn test_auth_azure_password() {
        let auth = AuthenticationMode::AzurePassword {
            user: "user@tenant".to_string(),
            password: "pwd".to_string(),
        };
        let cs = auth.to_connection_string();
        assert!(cs.contains("Active Directory Password"));
        assert!(auth.is_azure());
    }

    #[test]
    fn test_auth_azure_sp() {
        let auth = AuthenticationMode::AzureServicePrincipal {
            client_id: "cid".to_string(),
            client_secret: "secret".to_string(),
            tenant_id: "tid".to_string(),
        };
        let cs = auth.to_connection_string();
        assert!(cs.contains("Service Principal"));
        assert!(auth.is_azure());
    }

    #[test]
    fn test_encryption_default() {
        let enc = EncryptionConfig::default();
        assert!(enc.encrypt);
        assert!(!enc.trust_server_certificate);
    }

    #[test]
    fn test_encryption_to_cs() {
        let enc = EncryptionConfig::new()
            .with_encrypt()
            .with_trust_server_certificate();
        let cs = enc.to_connection_string();
        assert!(cs.contains("Encrypt=Yes"));
        assert!(cs.contains("TrustServerCertificate=Yes"));
    }

    #[test]
    fn test_encryption_with_tls() {
        let enc = EncryptionConfig::new().with_tls_version(TlsVersion::Tls12);
        let cs = enc.to_connection_string();
        assert!(cs.contains("TLS=1.2"));
    }

    #[test]
    fn test_tls_version_display() {
        assert_eq!(format!("{}", TlsVersion::Tls13), "1.3");
    }

    #[test]
    fn test_pool_config_default() {
        let pool = ConnectionPoolConfig::default();
        assert!(pool.enabled);
        assert_eq!(pool.max_pool_size, 100);
    }

    #[test]
    fn test_pool_config_to_cs() {
        let pool = ConnectionPoolConfig::new()
            .with_max_pool_size(50)
            .with_min_pool_size(5);
        let cs = pool.to_connection_string();
        assert!(cs.contains("Max Pool Size=50"));
        assert!(cs.contains("Min Pool Size=5"));
    }

    #[test]
    fn test_pool_config_disabled() {
        let pool = ConnectionPoolConfig::new().disabled();
        let cs = pool.to_connection_string();
        assert_eq!(cs, "Pooling=False");
    }

    #[test]
    fn test_connection_config_new() {
        let cfg = MssqlConnectionConfig::new("localhost", "testdb", "sa", "pwd");
        assert_eq!(cfg.server, "localhost");
        assert_eq!(cfg.port, 1433);
        assert_eq!(cfg.database, "testdb");
    }

    #[test]
    fn test_connection_config_windows_auth() {
        let cfg = MssqlConnectionConfig::windows_auth("localhost", "testdb");
        assert!(cfg.authentication.is_windows());
    }

    #[test]
    fn test_connection_config_builder() {
        let cfg = MssqlConnectionConfig::new("localhost", "testdb", "sa", "pwd")
            .with_port(1434)
            .with_connect_timeout(Duration::from_secs(30))
            .with_command_timeout(Duration::from_secs(60))
            .with_application_name("my_app")
            .with_multi_subnet_failover();
        assert_eq!(cfg.port, 1434);
        assert_eq!(cfg.application_name.as_deref(), Some("my_app"));
        assert!(cfg.multi_subnet_failover);
    }

    #[test]
    fn test_connection_config_to_cs() {
        let cfg = MssqlConnectionConfig::new("localhost", "testdb", "sa", "pwd");
        let cs = cfg.to_connection_string();
        assert!(cs.contains("Server=localhost"));
        assert!(cs.contains("Database=testdb"));
        assert!(cs.contains("User Id=sa"));
    }

    #[test]
    fn test_connection_config_jdbc_url() {
        let cfg = MssqlConnectionConfig::new("localhost", "testdb", "sa", "pwd");
        let url = cfg.to_jdbc_url();
        assert!(url.starts_with("jdbc:sqlserver://"));
        assert!(url.contains("localhost:1433"));
    }

    #[test]
    fn test_connection_config_jdbc_url_windows() {
        let cfg = MssqlConnectionConfig::windows_auth("localhost", "testdb");
        let url = cfg.to_jdbc_url();
        assert!(url.contains("integratedSecurity=true"));
    }

    #[test]
    fn test_connection_config_validate_ok() {
        let cfg = MssqlConnectionConfig::new("localhost", "testdb", "sa", "pwd");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_connection_config_validate_empty_server() {
        let cfg = MssqlConnectionConfig::new("", "testdb", "sa", "pwd");
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_connection_config_display() {
        let cfg = MssqlConnectionConfig::new("localhost", "testdb", "sa", "pwd");
        let s = format!("{}", cfg);
        assert!(s.contains("Server=localhost"));
    }

    #[test]
    fn test_connection_config_with_failover() {
        let cfg = MssqlConnectionConfig::new("localhost", "testdb", "sa", "pwd")
            .with_failover_partner("partner_host");
        assert_eq!(cfg.failover_partner.as_deref(), Some("partner_host"));
    }

    #[test]
    fn test_connection_config_with_always_encrypted() {
        let cfg =
            MssqlConnectionConfig::new("localhost", "testdb", "sa", "pwd").with_always_encrypted();
        assert!(cfg.always_encrypted);
    }

    #[test]
    fn test_pool_config_with_connection_lifetime() {
        let pool = ConnectionPoolConfig::new().with_connection_lifetime(3600);
        let cs = pool.to_connection_string();
        assert!(cs.contains("Connection Lifetime=3600"));
    }
}
