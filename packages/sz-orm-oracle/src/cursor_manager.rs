//! Oracle 游标管理
//!
//! 提供 [`CursorManager`] 与 [`CursorConfig`] 用于管理 PL/SQL 显式游标
//! 的生命周期（声明、打开、获取、关闭）。

use std::collections::HashMap;
use std::fmt;

/// 游标状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorState {
    /// 已声明未打开
    Declared,
    /// 已打开
    Open,
    /// 正在获取
    Fetching,
    /// 已关闭
    Closed,
}

impl CursorState {
    /// 是否可打开
    #[must_use]
    pub fn can_open(&self) -> bool {
        matches!(self, CursorState::Declared)
    }

    /// 是否可获取
    #[must_use]
    pub fn can_fetch(&self) -> bool {
        matches!(self, CursorState::Open | CursorState::Fetching)
    }

    /// 是否可关闭
    #[must_use]
    pub fn can_close(&self) -> bool {
        matches!(self, CursorState::Open | CursorState::Fetching)
    }

    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            CursorState::Declared => "declared",
            CursorState::Open => "open",
            CursorState::Fetching => "fetching",
            CursorState::Closed => "closed",
        }
    }
}

impl fmt::Display for CursorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.description())
    }
}

/// 游标获取方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchDirection {
    /// 向前获取（NEXT）
    Next,
    /// 向后获取（PRIOR）
    Prior,
    /// 获取第一条（FIRST）
    First,
    /// 获取最后一条（LAST）
    Last,
    /// 跳转绝对位置（ABSOLUTE N）
    Absolute(i64),
    /// 跳转相对位置（RELATIVE N）
    Relative(i64),
}

impl FetchDirection {
    /// 生成 FETCH 子句
    #[must_use]
    pub fn as_fetch_clause(&self) -> String {
        match self {
            FetchDirection::Next => "NEXT".to_string(),
            FetchDirection::Prior => "PRIOR".to_string(),
            FetchDirection::First => "FIRST".to_string(),
            FetchDirection::Last => "LAST".to_string(),
            FetchDirection::Absolute(n) => format!("ABSOLUTE {n}"),
            FetchDirection::Relative(n) => format!("RELATIVE {n}"),
        }
    }
}

/// 游标配置
#[derive(Debug, Clone)]
pub struct CursorConfig {
    /// 游标名
    pub name: String,
    /// 查询 SQL
    pub query: String,
    /// 每次获取的行数（BULK COLLECT INTO LIMIT）
    pub fetch_batch_size: usize,
    /// 是否使用 FOR UPDATE 锁定
    pub for_update: bool,
    /// FOR UPDATE 列（None 表示锁定所有列）
    pub for_update_columns: Option<Vec<String>>,
    /// 是否使用 WITH HOLD（保持游标跨提交）
    pub with_hold: bool,
    /// 是否使用 SCROLL 游标
    pub scroll: bool,
    /// 是否使用 BULK COLLECT
    pub bulk_collect: bool,
}

impl CursorConfig {
    /// 创建新的游标配置
    #[must_use]
    pub fn new(name: &str, query: &str) -> Self {
        Self {
            name: name.to_string(),
            query: query.to_string(),
            fetch_batch_size: 1,
            for_update: false,
            for_update_columns: None,
            with_hold: false,
            scroll: false,
            bulk_collect: false,
        }
    }

    /// 设置批量获取大小
    #[must_use]
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.fetch_batch_size = size.max(1);
        self
    }

    /// 启用 FOR UPDATE 锁
    #[must_use]
    pub fn for_update(mut self) -> Self {
        self.for_update = true;
        self
    }

    /// 启用 FOR UPDATE 锁并指定列
    #[must_use]
    pub fn for_update_cols(mut self, cols: &[&str]) -> Self {
        self.for_update = true;
        self.for_update_columns = Some(cols.iter().map(|s| s.to_string()).collect());
        self
    }

    /// 启用 WITH HOLD
    #[must_use]
    pub fn with_hold(mut self) -> Self {
        self.with_hold = true;
        self
    }

    /// 启用 SCROLL 游标
    #[must_use]
    pub fn scroll(mut self) -> Self {
        self.scroll = true;
        self
    }

    /// 启用 BULK COLLECT
    #[must_use]
    pub fn bulk_collect(mut self) -> Self {
        self.bulk_collect = true;
        self
    }

    /// 生成 DECLARE 语句
    #[must_use]
    pub fn declare_sql(&self) -> String {
        format!(
            "CURSOR {} IS {}{}",
            self.name,
            self.query,
            self.for_update_clause()
        )
    }

    /// 生成 OPEN 语句
    #[must_use]
    pub fn open_sql(&self) -> String {
        format!("OPEN {};", self.name)
    }

    /// 生成 FETCH 语句
    #[must_use]
    pub fn fetch_sql(&self, into_var: &str) -> String {
        if self.bulk_collect && self.fetch_batch_size > 1 {
            format!(
                "FETCH {} BULK COLLECT INTO {} LIMIT {};",
                self.name, into_var, self.fetch_batch_size
            )
        } else {
            format!("FETCH {} INTO {};", self.name, into_var)
        }
    }

    /// 生成 SCROLL FETCH 语句
    #[must_use]
    pub fn fetch_scroll_sql(&self, direction: FetchDirection, into_var: &str) -> String {
        format!(
            "FETCH {} {} INTO {};",
            self.name,
            direction.as_fetch_clause(),
            into_var
        )
    }

    /// 生成 CLOSE 语句
    #[must_use]
    pub fn close_sql(&self) -> String {
        format!("CLOSE {};", self.name)
    }

    /// 生成 FOR UPDATE 子句
    fn for_update_clause(&self) -> String {
        if !self.for_update {
            return String::new();
        }
        match &self.for_update_columns {
            Some(cols) if !cols.is_empty() => {
                format!(" FOR UPDATE OF {}", cols.join(", "))
            }
            _ => " FOR UPDATE".to_string(),
        }
    }

    /// 生成 %ISOPEN 检查
    #[must_use]
    pub fn is_open_check(&self) -> String {
        format!("{}%ISOPEN", self.name)
    }

    /// 生成 %NOTFOUND 检查
    #[must_use]
    pub fn not_found_check(&self) -> String {
        format!("{}%NOTFOUND", self.name)
    }

    /// 生成 %FOUND 检查
    #[must_use]
    pub fn found_check(&self) -> String {
        format!("{}%FOUND", self.name)
    }

    /// 生成 %ROWCOUNT 检查
    #[must_use]
    pub fn rowcount_check(&self) -> String {
        format!("{}%ROWCOUNT", self.name)
    }
}

impl fmt::Display for CursorConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.declare_sql())
    }
}

/// 游标实例（运行时状态）
#[derive(Debug, Clone)]
pub struct CursorInstance {
    /// 配置
    config: CursorConfig,
    /// 当前状态
    state: CursorState,
    /// 已获取行数
    fetched_rows: u64,
    /// 是否到达末尾
    exhausted: bool,
}

impl CursorInstance {
    /// 创建新的游标实例
    #[must_use]
    pub fn new(config: CursorConfig) -> Self {
        Self {
            config,
            state: CursorState::Declared,
            fetched_rows: 0,
            exhausted: false,
        }
    }

    /// 打开游标
    ///
    /// # Errors
    ///
    /// 若游标状态不允许打开返回 `Err`。
    pub fn open(&mut self) -> Result<(), String> {
        if !self.state.can_open() {
            return Err(format!("cannot open cursor in state: {}", self.state));
        }
        self.state = CursorState::Open;
        Ok(())
    }

    /// 获取一批行
    ///
    /// # Errors
    ///
    /// 若游标状态不允许获取返回 `Err`。
    pub fn fetch(&mut self, rows: usize) -> Result<usize, String> {
        if !self.state.can_fetch() {
            return Err(format!("cannot fetch cursor in state: {}", self.state));
        }
        if self.exhausted {
            return Ok(0);
        }
        self.state = CursorState::Fetching;
        let actual = rows.min(self.config.fetch_batch_size);
        self.fetched_rows += actual as u64;
        if rows < self.config.fetch_batch_size {
            self.exhausted = true;
            self.state = CursorState::Open;
        } else {
            self.state = CursorState::Open;
        }
        Ok(actual)
    }

    /// 关闭游标
    ///
    /// # Errors
    ///
    /// 若游标状态不允许关闭返回 `Err`。
    pub fn close(&mut self) -> Result<(), String> {
        if !self.state.can_close() {
            return Err(format!("cannot close cursor in state: {}", self.state));
        }
        self.state = CursorState::Closed;
        Ok(())
    }

    /// 获取当前状态
    #[must_use]
    pub fn state(&self) -> CursorState {
        self.state
    }

    /// 获取已获取行数
    #[must_use]
    pub fn fetched_rows(&self) -> u64 {
        self.fetched_rows
    }

    /// 是否已耗尽
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// 获取配置引用
    #[must_use]
    pub fn config(&self) -> &CursorConfig {
        &self.config
    }
}

/// 游标管理器
///
/// 管理多个命名游标的生命周期。
#[derive(Debug, Default)]
pub struct CursorManager {
    /// 游标实例映射
    cursors: HashMap<String, CursorInstance>,
}

impl CursorManager {
    /// 创建新的游标管理器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册游标
    pub fn register(&mut self, config: CursorConfig) {
        let name = config.name.clone();
        self.cursors.insert(name, CursorInstance::new(config));
    }

    /// 打开游标
    ///
    /// # Errors
    ///
    /// 若游标不存在或状态不允许打开返回 `Err`。
    pub fn open(&mut self, name: &str) -> Result<(), String> {
        let cursor = self
            .cursors
            .get_mut(name)
            .ok_or_else(|| format!("cursor not found: {name}"))?;
        cursor.open()
    }

    /// 获取行
    ///
    /// # Errors
    ///
    /// 若游标不存在或状态不允许获取返回 `Err`。
    pub fn fetch(&mut self, name: &str, rows: usize) -> Result<usize, String> {
        let cursor = self
            .cursors
            .get_mut(name)
            .ok_or_else(|| format!("cursor not found: {name}"))?;
        cursor.fetch(rows)
    }

    /// 关闭游标
    ///
    /// # Errors
    ///
    /// 若游标不存在或状态不允许关闭返回 `Err`。
    pub fn close(&mut self, name: &str) -> Result<(), String> {
        let cursor = self
            .cursors
            .get_mut(name)
            .ok_or_else(|| format!("cursor not found: {name}"))?;
        cursor.close()
    }

    /// 获取游标状态
    ///
    /// # Errors
    ///
    /// 若游标不存在返回 `Err`。
    pub fn state(&self, name: &str) -> Result<CursorState, String> {
        let cursor = self
            .cursors
            .get(name)
            .ok_or_else(|| format!("cursor not found: {name}"))?;
        Ok(cursor.state())
    }

    /// 获取已获取行数
    ///
    /// # Errors
    ///
    /// 若游标不存在返回 `Err`。
    pub fn fetched_rows(&self, name: &str) -> Result<u64, String> {
        let cursor = self
            .cursors
            .get(name)
            .ok_or_else(|| format!("cursor not found: {name}"))?;
        Ok(cursor.fetched_rows())
    }

    /// 关闭所有游标
    pub fn close_all(&mut self) {
        for cursor in self.cursors.values_mut() {
            if cursor.state.can_close() {
                let _ = cursor.close();
            }
        }
    }

    /// 游标数量
    #[must_use]
    pub fn count(&self) -> usize {
        self.cursors.len()
    }

    /// 获取所有游标名
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.cursors.keys().cloned().collect()
    }
}

impl fmt::Display for CursorManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CursorManager(count={})", self.count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_state_can_open() {
        assert!(CursorState::Declared.can_open());
        assert!(!CursorState::Open.can_open());
    }

    #[test]
    fn test_cursor_state_can_fetch() {
        assert!(CursorState::Open.can_fetch());
        assert!(CursorState::Fetching.can_fetch());
        assert!(!CursorState::Declared.can_fetch());
    }

    #[test]
    fn test_cursor_state_can_close() {
        assert!(CursorState::Open.can_close());
        assert!(CursorState::Fetching.can_close());
        assert!(!CursorState::Declared.can_close());
    }

    #[test]
    fn test_cursor_state_description() {
        assert_eq!(CursorState::Declared.description(), "declared");
        assert_eq!(CursorState::Open.description(), "open");
    }

    #[test]
    fn test_fetch_direction_next() {
        assert_eq!(FetchDirection::Next.as_fetch_clause(), "NEXT");
    }

    #[test]
    fn test_fetch_direction_absolute() {
        assert_eq!(
            FetchDirection::Absolute(10).as_fetch_clause(),
            "ABSOLUTE 10"
        );
    }

    #[test]
    fn test_fetch_direction_relative() {
        assert_eq!(
            FetchDirection::Relative(-5).as_fetch_clause(),
            "RELATIVE -5"
        );
    }

    #[test]
    fn test_cursor_config_declare_sql() {
        let cfg = CursorConfig::new("emp_cursor", "SELECT * FROM employees");
        let sql = cfg.declare_sql();
        assert!(sql.contains("CURSOR emp_cursor IS"));
        assert!(sql.contains("SELECT * FROM employees"));
    }

    #[test]
    fn test_cursor_config_open_sql() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        assert_eq!(cfg.open_sql(), "OPEN c1;");
    }

    #[test]
    fn test_cursor_config_fetch_sql() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        assert_eq!(cfg.fetch_sql("v1"), "FETCH c1 INTO v1;");
    }

    #[test]
    fn test_cursor_config_fetch_bulk_sql() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual")
            .bulk_collect()
            .with_batch_size(100);
        let sql = cfg.fetch_sql("v1");
        assert!(sql.contains("BULK COLLECT INTO"));
        assert!(sql.contains("LIMIT 100"));
    }

    #[test]
    fn test_cursor_config_close_sql() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        assert_eq!(cfg.close_sql(), "CLOSE c1;");
    }

    #[test]
    fn test_cursor_config_for_update() {
        let cfg = CursorConfig::new("c1", "SELECT * FROM t").for_update();
        let sql = cfg.declare_sql();
        assert!(sql.contains("FOR UPDATE"));
    }

    #[test]
    fn test_cursor_config_for_update_cols() {
        let cfg = CursorConfig::new("c1", "SELECT * FROM t").for_update_cols(&["id", "name"]);
        let sql = cfg.declare_sql();
        assert!(sql.contains("FOR UPDATE OF id, name"));
    }

    #[test]
    fn test_cursor_config_scroll_fetch() {
        let cfg = CursorConfig::new("c1", "SELECT * FROM t").scroll();
        let sql = cfg.fetch_scroll_sql(FetchDirection::Prior, "v1");
        assert_eq!(sql, "FETCH c1 PRIOR INTO v1;");
    }

    #[test]
    fn test_cursor_config_is_open_check() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        assert_eq!(cfg.is_open_check(), "c1%ISOPEN");
    }

    #[test]
    fn test_cursor_config_not_found_check() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        assert_eq!(cfg.not_found_check(), "c1%NOTFOUND");
    }

    #[test]
    fn test_cursor_config_rowcount_check() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        assert_eq!(cfg.rowcount_check(), "c1%ROWCOUNT");
    }

    #[test]
    fn test_cursor_config_display() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        let s = format!("{}", cfg);
        assert!(s.contains("CURSOR c1"));
    }

    #[test]
    fn test_cursor_instance_open() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        let mut cursor = CursorInstance::new(cfg);
        assert_eq!(cursor.state(), CursorState::Declared);
        cursor.open().unwrap();
        assert_eq!(cursor.state(), CursorState::Open);
    }

    #[test]
    fn test_cursor_instance_open_twice_fails() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        let mut cursor = CursorInstance::new(cfg);
        cursor.open().unwrap();
        assert!(cursor.open().is_err());
    }

    #[test]
    fn test_cursor_instance_fetch() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual").with_batch_size(10);
        let mut cursor = CursorInstance::new(cfg);
        cursor.open().unwrap();
        let n = cursor.fetch(5).unwrap();
        assert_eq!(n, 5);
        assert_eq!(cursor.fetched_rows(), 5);
    }

    #[test]
    fn test_cursor_instance_fetch_before_open() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        let mut cursor = CursorInstance::new(cfg);
        assert!(cursor.fetch(1).is_err());
    }

    #[test]
    fn test_cursor_instance_close() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        let mut cursor = CursorInstance::new(cfg);
        cursor.open().unwrap();
        cursor.close().unwrap();
        assert_eq!(cursor.state(), CursorState::Closed);
    }

    #[test]
    fn test_cursor_instance_close_before_open() {
        let cfg = CursorConfig::new("c1", "SELECT 1 FROM dual");
        let mut cursor = CursorInstance::new(cfg);
        assert!(cursor.close().is_err());
    }

    #[test]
    fn test_cursor_manager_register() {
        let mut mgr = CursorManager::new();
        mgr.register(CursorConfig::new("c1", "SELECT 1 FROM dual"));
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn test_cursor_manager_open() {
        let mut mgr = CursorManager::new();
        mgr.register(CursorConfig::new("c1", "SELECT 1 FROM dual"));
        mgr.open("c1").unwrap();
        assert_eq!(mgr.state("c1").unwrap(), CursorState::Open);
    }

    #[test]
    fn test_cursor_manager_open_nonexistent() {
        let mut mgr = CursorManager::new();
        assert!(mgr.open("c1").is_err());
    }

    #[test]
    fn test_cursor_manager_fetch() {
        let mut mgr = CursorManager::new();
        mgr.register(CursorConfig::new("c1", "SELECT 1 FROM dual").with_batch_size(10));
        mgr.open("c1").unwrap();
        let n = mgr.fetch("c1", 5).unwrap();
        assert_eq!(n, 5);
        assert_eq!(mgr.fetched_rows("c1").unwrap(), 5);
    }

    #[test]
    fn test_cursor_manager_close() {
        let mut mgr = CursorManager::new();
        mgr.register(CursorConfig::new("c1", "SELECT 1 FROM dual"));
        mgr.open("c1").unwrap();
        mgr.close("c1").unwrap();
        assert_eq!(mgr.state("c1").unwrap(), CursorState::Closed);
    }

    #[test]
    fn test_cursor_manager_close_all() {
        let mut mgr = CursorManager::new();
        mgr.register(CursorConfig::new("c1", "SELECT 1 FROM dual"));
        mgr.register(CursorConfig::new("c2", "SELECT 2 FROM dual"));
        mgr.open("c1").unwrap();
        mgr.open("c2").unwrap();
        mgr.close_all();
        assert_eq!(mgr.state("c1").unwrap(), CursorState::Closed);
        assert_eq!(mgr.state("c2").unwrap(), CursorState::Closed);
    }

    #[test]
    fn test_cursor_manager_names() {
        let mut mgr = CursorManager::new();
        mgr.register(CursorConfig::new("c1", "SELECT 1 FROM dual"));
        mgr.register(CursorConfig::new("c2", "SELECT 2 FROM dual"));
        let names = mgr.names();
        assert!(names.contains(&"c1".to_string()));
        assert!(names.contains(&"c2".to_string()));
    }

    #[test]
    fn test_cursor_manager_display() {
        let mgr = CursorManager::new();
        let s = format!("{}", mgr);
        assert!(s.contains("count=0"));
    }
}
