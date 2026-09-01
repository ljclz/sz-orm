//! LLM 安全审计模块
//!
//! 规则引擎无法识别的注入模式，调用 LLM 二次判断。
//! 发现新注入模式时持久化到本地文件（JSON 格式）。
//!
//! 启用 `ai-security-audit` feature 后可用。
//! 使用 `LlmSecurityAuditor::new` 启用 LLM 安全审计。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ==================== 错误类型 ====================

/// 安全审计错误
#[derive(Debug, Error)]
pub enum SecurityAuditError {
    /// IO 错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// 序列化错误
    #[error("Serialize error: {0}")]
    Serialize(#[from] serde_json::Error),
    /// LLM 调用错误
    #[error("LLM error: {0}")]
    Llm(String),
}

// ==================== 安全审计结果 ====================

/// 风险等级
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// 安全
    Safe,
    /// 低风险
    Low,
    /// 中风险
    Medium,
    /// 高风险（注入）
    High,
    /// 严重风险（确认注入）
    Critical,
}

impl RiskLevel {
    /// 是否为危险级别
    pub fn is_dangerous(&self) -> bool {
        matches!(self, RiskLevel::High | RiskLevel::Critical)
    }
}

/// 安全审计结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditResult {
    /// 风险等级
    pub risk_level: RiskLevel,
    /// 检测到的模式
    pub detected_patterns: Vec<String>,
    /// 是否为新发现的模式
    pub is_new_pattern: bool,
    /// 修正建议
    pub fix_suggestion: Option<String>,
    /// 审计来源（rule / llm）
    pub source: AuditSource,
}

/// 审计来源
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditSource {
    /// 规则引擎
    Rule,
    /// LLM 判断
    Llm,
}

// ==================== 注入模式存储 ====================

/// 注入模式条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionPattern {
    /// 模式名称
    pub name: String,
    /// 正则表达式或匹配模式
    pub pattern: String,
    /// 风险等级
    pub risk_level: RiskLevel,
    /// 发现时间（Unix 时间戳）
    pub discovered_at: i64,
    /// 命中次数
    pub hit_count: u64,
}

/// 注入模式持久化存储
///
/// 将注入模式持久化到本地 JSON 文件，启动时加载，发现新模式时追加。
pub struct InjectionPatternStore {
    /// 已知模式列表
    patterns: Vec<InjectionPattern>,
    /// 持久化文件路径
    store_path: Option<PathBuf>,
}

impl InjectionPatternStore {
    /// 创建内存中的模式存储（不持久化）
    pub fn in_memory() -> Self {
        Self {
            patterns: Self::builtin_patterns(),
            store_path: None,
        }
    }

    /// 创建持久化模式存储
    pub fn new(store_path: PathBuf) -> Result<Self, SecurityAuditError> {
        let patterns = if store_path.exists() {
            let content = std::fs::read_to_string(&store_path)?;
            serde_json::from_str(&content)?
        } else {
            Self::builtin_patterns()
        };
        Ok(Self {
            patterns,
            store_path: Some(store_path),
        })
    }

    /// 内置注入模式
    fn builtin_patterns() -> Vec<InjectionPattern> {
        vec![
            InjectionPattern {
                name: "or_1_eq_1".to_string(),
                pattern: "' OR 1=1".to_string(),
                risk_level: RiskLevel::Critical,
                discovered_at: 0,
                hit_count: 0,
            },
            InjectionPattern {
                name: "union_injection".to_string(),
                pattern: "' UNION SELECT".to_string(),
                risk_level: RiskLevel::Critical,
                discovered_at: 0,
                hit_count: 0,
            },
            InjectionPattern {
                name: "stacked_injection".to_string(),
                pattern: "; DROP TABLE".to_string(),
                risk_level: RiskLevel::Critical,
                discovered_at: 0,
                hit_count: 0,
            },
            InjectionPattern {
                name: "comment_injection".to_string(),
                pattern: "--".to_string(),
                risk_level: RiskLevel::Medium,
                discovered_at: 0,
                hit_count: 0,
            },
        ]
    }

    /// 检查输入是否匹配已知模式
    pub fn check(&mut self, input: &str) -> Option<&InjectionPattern> {
        let lower = input.to_lowercase();
        for pattern in &mut self.patterns {
            if lower.contains(&pattern.pattern.to_lowercase()) {
                pattern.hit_count += 1;
                return Some(pattern);
            }
        }
        None
    }

    /// 添加新注入模式
    pub fn add_pattern(&mut self, pattern: InjectionPattern) -> Result<bool, SecurityAuditError> {
        // 检查是否已存在
        let exists = self
            .patterns
            .iter()
            .any(|p| p.pattern.eq_ignore_ascii_case(&pattern.pattern));
        if exists {
            return Ok(false);
        }

        self.patterns.push(pattern);

        // 持久化
        if let Some(path) = &self.store_path {
            let content = serde_json::to_string_pretty(&self.patterns)?;
            std::fs::write(path, content)?;
        }

        Ok(true)
    }

    /// 获取所有模式
    pub fn patterns(&self) -> &[InjectionPattern] {
        &self.patterns
    }

    /// 模式数量
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// 从文件加载注入模式库
    ///
    /// 启动时调用此方法从本地 JSON 文件加载模式。
    /// 如果文件不存在或解析失败，保留当前内存中的模式（降级安全）。
    pub fn load_patterns(&mut self) -> Result<usize, SecurityAuditError> {
        let path = match &self.store_path {
            Some(p) => p,
            None => return Ok(self.patterns.len()),
        };
        if !path.exists() {
            return Ok(self.patterns.len());
        }
        let content = std::fs::read_to_string(path)?;
        let loaded: Vec<InjectionPattern> = serde_json::from_str(&content)?;
        let count = loaded.len();
        self.patterns = loaded;
        Ok(count)
    }

    /// 保存模式库到文件
    ///
    /// 将当前所有模式持久化到本地文件。
    pub fn save_patterns(&self) -> Result<(), SecurityAuditError> {
        let path = match &self.store_path {
            Some(p) => p,
            None => return Ok(()),
        };
        let content = serde_json::to_string_pretty(&self.patterns)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// 设置持久化路径
    pub fn with_store_path(mut self, path: PathBuf) -> Self {
        self.store_path = Some(path);
        self
    }

    /// 获取持久化路径
    pub fn store_path(&self) -> Option<&PathBuf> {
        self.store_path.as_ref()
    }
}

// ==================== LLM 审计 Provider trait ====================

/// LLM 审计 Provider trait
///
/// 抽象 LLM 调用，测试中用 Mock 实现。
#[async_trait::async_trait]
pub trait LlmAuditProvider: Send + Sync {
    /// 调用 LLM 判断注入风险
    ///
    /// 返回 (风险等级, 检测到的模式名称, 修正建议)
    async fn audit(
        &self,
        input: &str,
    ) -> Result<(RiskLevel, Vec<String>, Option<String>), SecurityAuditError>;
}

// ==================== LLM 安全审计器 ====================

/// LLM 安全审计器
///
/// 规则引擎无法识别时调用 LLM 判断注入风险，发现新模式时入库。
pub struct LlmSecurityAuditor {
    /// 注入模式存储
    pattern_store: InjectionPatternStore,
    /// LLM 审计 Provider
    llm_provider: Option<Box<dyn LlmAuditProvider>>,
}

impl LlmSecurityAuditor {
    /// 创建仅规则引擎的审计器
    pub fn rule_only() -> Self {
        Self {
            pattern_store: InjectionPatternStore::in_memory(),
            llm_provider: None,
        }
    }

    /// 创建带 LLM 的审计器
    pub fn with_llm(llm_provider: Box<dyn LlmAuditProvider>) -> Self {
        Self {
            pattern_store: InjectionPatternStore::in_memory(),
            llm_provider: Some(llm_provider),
        }
    }

    /// 创建带 LLM 和持久化存储的审计器
    pub fn with_llm_and_store(
        llm_provider: Box<dyn LlmAuditProvider>,
        store_path: PathBuf,
    ) -> Result<Self, SecurityAuditError> {
        Ok(Self {
            pattern_store: InjectionPatternStore::new(store_path)?,
            llm_provider: Some(llm_provider),
        })
    }

    /// 审计输入
    ///
    /// 1. 先用规则引擎检查
    /// 2. 规则引擎无法识别时调用 LLM
    /// 3. 发现新模式时入库
    pub async fn audit(&mut self, input: &str) -> Result<SecurityAuditResult, SecurityAuditError> {
        // 1. 规则引擎检查
        if let Some(pattern) = self.pattern_store.check(input) {
            return Ok(SecurityAuditResult {
                risk_level: pattern.risk_level.clone(),
                detected_patterns: vec![pattern.name.clone()],
                is_new_pattern: false,
                fix_suggestion: Some("使用参数化查询".to_string()),
                source: AuditSource::Rule,
            });
        }

        // 2. LLM 二次判断
        if let Some(llm) = &self.llm_provider {
            let (risk_level, patterns, fix) = llm.audit(input).await?;

            // 3. 发现新模式时入库
            let mut is_new = false;
            if !patterns.is_empty() && risk_level.is_dangerous() {
                for name in &patterns {
                    let pattern = InjectionPattern {
                        name: name.clone(),
                        pattern: input.to_string(),
                        risk_level: risk_level.clone(),
                        discovered_at: current_timestamp(),
                        hit_count: 1,
                    };
                    if self.pattern_store.add_pattern(pattern)? {
                        is_new = true;
                    }
                }
            }

            let fix_suggestion = if risk_level.is_dangerous() {
                fix.or(Some("LLM 判断为危险，建议人工复核".to_string()))
            } else {
                fix
            };

            return Ok(SecurityAuditResult {
                risk_level,
                detected_patterns: patterns,
                is_new_pattern: is_new,
                fix_suggestion,
                source: AuditSource::Llm,
            });
        }

        // 3. 无 LLM 且规则未识别 → 安全
        Ok(SecurityAuditResult {
            risk_level: RiskLevel::Safe,
            detected_patterns: Vec::new(),
            is_new_pattern: false,
            fix_suggestion: None,
            source: AuditSource::Rule,
        })
    }

    /// 获取模式存储引用
    pub fn pattern_store(&self) -> &InjectionPatternStore {
        &self.pattern_store
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
