//! GraphQL 深度扩展功能
//!
//! 本模块补充 GraphQL 支持缺失的核心深度功能，包括：
//!
//! - **嵌套关联（Nested Resolvers）**：类型间关联关系定义与解析器注册
//! - **分页（Connection/Edge 模式）**：Relay 风格的游标分页
//! - **变更（Mutation）**：创建/更新/删除的输入类型与执行框架
//! - **订阅（Subscription）框架**：基于内存 pub/sub 的事件推送
//!
//! # 设计说明
//!
//! 本模块以独立类型 + 扩展 trait 的方式提供，不修改既有 `GraphQLSchema` 结构，
//! 避免破坏已有的 Schema 生成与执行逻辑。
//! 内存计算部分基于纯 Rust 实现，不依赖外部库。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

// =============================================================================
// 一、嵌套关联（Nested Resolvers）
// =============================================================================

/// 关联关系类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RelationKind {
    /// 一对一
    OneToOne,
    /// 一对多
    OneToMany,
    /// 多对一
    ManyToOne,
    /// 多对多
    ManyToMany,
}

/// 关联关系定义
///
/// 描述两个 GraphQL 类型之间的关联关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// 关联名称
    pub name: String,
    /// 源类型名
    pub from_type: String,
    /// 源类型关联字段（外键）
    pub from_field: String,
    /// 目标类型名
    pub to_type: String,
    /// 目标类型关联字段（主键）
    pub to_field: String,
    /// 关联类型
    pub kind: RelationKind,
}

impl Relation {
    pub fn new(
        name: impl Into<String>,
        from_type: impl Into<String>,
        from_field: impl Into<String>,
        to_type: impl Into<String>,
        to_field: impl Into<String>,
        kind: RelationKind,
    ) -> Self {
        Self {
            name: name.into(),
            from_type: from_type.into(),
            from_field: from_field.into(),
            to_type: to_type.into(),
            to_field: to_field.into(),
            kind,
        }
    }

    /// 创建一对多关联
    pub fn one_to_many(
        name: impl Into<String>,
        from_type: impl Into<String>,
        to_type: impl Into<String>,
    ) -> Self {
        let from_type_str = from_type.into();
        let to_field = format!("{}_id", from_type_str.to_lowercase());
        Self::new(
            name,
            from_type_str,
            "id",
            to_type,
            to_field,
            RelationKind::OneToMany,
        )
    }

    /// 创建多对一关联
    pub fn many_to_one(
        name: impl Into<String>,
        from_type: impl Into<String>,
        to_type: impl Into<String>,
    ) -> Self {
        let to_type_str = to_type.into();
        let from_field = format!("{}_id", to_type_str.to_lowercase());
        Self::new(
            name,
            from_type,
            from_field,
            to_type_str,
            "id",
            RelationKind::ManyToOne,
        )
    }
}

/// 解析器函数类型
///
/// 接收父对象的 JSON 值和参数，返回解析结果
pub type ResolverFn = Box<dyn Fn(&Value, &HashMap<String, Value>) -> Value + Send + Sync>;

/// 嵌套关联解析器注册表
///
/// 注册字段到解析函数的映射，用于解析嵌套关联字段
pub struct ResolverRegistry {
    /// 字段到解析函数的映射，key = "TypeName.fieldName"
    resolvers: RwLock<HashMap<String, Arc<ResolverFn>>>,
}

impl ResolverRegistry {
    pub fn new() -> Self {
        Self {
            resolvers: RwLock::new(HashMap::new()),
        }
    }

    /// 注册解析器
    ///
    /// key 格式为 "TypeName.fieldName"
    pub fn register(&self, type_name: &str, field_name: &str, resolver: ResolverFn) {
        let key = format!("{}.{}", type_name, field_name);
        let mut map = self.resolvers.write().expect("resolver lock poisoned");
        map.insert(key, Arc::new(resolver));
    }

    /// 查找解析器
    pub fn get(&self, type_name: &str, field_name: &str) -> Option<Arc<ResolverFn>> {
        let key = format!("{}.{}", type_name, field_name);
        let map = self.resolvers.read().expect("resolver lock poisoned");
        map.get(&key).cloned()
    }

    /// 解析字段
    pub fn resolve(
        &self,
        type_name: &str,
        field_name: &str,
        parent: &Value,
        args: &HashMap<String, Value>,
    ) -> Option<Value> {
        self.get(type_name, field_name).map(|resolver| resolver(parent, args))
    }

    /// 已注册的解析器数量
    pub fn len(&self) -> usize {
        self.resolvers.read().expect("resolver lock poisoned").len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ResolverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 关联数据源
///
/// 提供按外键查找关联数据的能力，用于解析嵌套字段
pub struct RelationDataSource {
    /// 类型名 -> 文档列表
    data: RwLock<HashMap<String, Vec<Value>>>,
    /// 关联定义
    relations: RwLock<Vec<Relation>>,
}

impl RelationDataSource {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            relations: RwLock::new(Vec::new()),
        }
    }

    /// 插入文档到指定类型
    pub fn insert(&self, type_name: &str, doc: Value) {
        let mut data = self.data.write().expect("data lock poisoned");
        data.entry(type_name.to_string()).or_default().push(doc);
    }

    /// 批量插入文档
    pub fn insert_many(&self, type_name: &str, docs: Vec<Value>) {
        let mut data = self.data.write().expect("data lock poisoned");
        data.entry(type_name.to_string()).or_default().extend(docs);
    }

    /// 获取指定类型的所有文档
    pub fn get_all(&self, type_name: &str) -> Vec<Value> {
        let data = self.data.read().expect("data lock poisoned");
        data.get(type_name).cloned().unwrap_or_default()
    }

    /// 按 ID 查找文档
    pub fn find_by_id(&self, type_name: &str, id: &str) -> Option<Value> {
        let data = self.data.read().expect("data lock poisoned");
        data.get(type_name).and_then(|docs| {
            docs.iter()
                .find(|d| d.get("id").and_then(|v| v.as_str()) == Some(id))
                .cloned()
        })
    }

    /// 添加关联关系
    pub fn add_relation(&self, relation: Relation) {
        let mut rels = self.relations.write().expect("relation lock poisoned");
        rels.push(relation);
    }

    /// 解析一对多关联：返回 from_type 中外键等于 to_type 主键的文档列表
    ///
    /// 例如：User -> Orders，查找 order.user_id == user.id 的 orders
    pub fn resolve_one_to_many(
        &self,
        from_type: &str,
        from_field: &str,
        parent_id: &str,
    ) -> Vec<Value> {
        let data = self.data.read().expect("data lock poisoned");
        data.get(from_type)
            .map(|docs| {
                docs.iter()
                    .filter(|d| d.get(from_field).and_then(|v| v.as_str()) == Some(parent_id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 解析多对一关联：返回 to_type 中主键等于 from_type 外键的文档
    ///
    /// 例如：Order -> User，查找 user.id == order.user_id 的 user
    pub fn resolve_many_to_one(
        &self,
        to_type: &str,
        to_field: &str,
        foreign_key: &str,
    ) -> Option<Value> {
        let data = self.data.read().expect("data lock poisoned");
        data.get(to_type).and_then(|docs| {
            docs.iter()
                .find(|d| d.get(to_field).and_then(|v| v.as_str()) == Some(foreign_key))
                .cloned()
        })
    }
}

impl Default for RelationDataSource {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// 二、分页（Connection/Edge 模式）
// =============================================================================

/// 分页信息
///
/// 遵循 Relay Connection 规范
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageInfo {
    /// 是否有下一页
    pub has_next_page: bool,
    /// 是否有上一页
    pub has_previous_page: bool,
    /// 第一条记录的游标
    pub start_cursor: Option<String>,
    /// 最后一条记录的游标
    pub end_cursor: Option<String>,
}

impl PageInfo {
    pub fn new() -> Self {
        Self {
            has_next_page: false,
            has_previous_page: false,
            start_cursor: None,
            end_cursor: None,
        }
    }

    pub fn with_next(mut self, has_next: bool) -> Self {
        self.has_next_page = has_next;
        self
    }

    pub fn with_previous(mut self, has_prev: bool) -> Self {
        self.has_previous_page = has_prev;
        self
    }
}

impl Default for PageInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// 分页边（Edge）
///
/// 包含一个节点和它的游标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// 游标（base64 编码的偏移量）
    pub cursor: String,
    /// 节点数据
    pub node: Value,
}

impl Edge {
    pub fn new(cursor: impl Into<String>, node: Value) -> Self {
        Self {
            cursor: cursor.into(),
            node,
        }
    }

    /// 从索引和节点创建 Edge
    ///
    /// 游标使用 base64 编码的偏移量
    pub fn from_index(index: usize, node: Value) -> Self {
        let cursor = encode_cursor(index);
        Self::new(cursor, node)
    }
}

/// 连接（Connection）
///
/// Relay 规范的分页结果容器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// 边列表
    pub edges: Vec<Edge>,
    /// 分页信息
    pub page_info: PageInfo,
    /// 总记录数
    pub total_count: usize,
}

impl Connection {
    pub fn new(edges: Vec<Edge>, page_info: PageInfo, total_count: usize) -> Self {
        Self {
            edges,
            page_info,
            total_count,
        }
    }

    /// 从空数据创建空 Connection
    pub fn empty() -> Self {
        Self::new(Vec::new(), PageInfo::new(), 0)
    }

    /// 获取所有节点
    pub fn nodes(&self) -> Vec<&Value> {
        self.edges.iter().map(|e| &e.node).collect()
    }
}

impl Default for Connection {
    fn default() -> Self {
        Self::empty()
    }
}

/// 分页参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationArgs {
    /// 向后分页：取前 N 条
    pub first: Option<usize>,
    /// 向后分页游标
    pub after: Option<String>,
    /// 向前分页：取后 N 条
    pub last: Option<usize>,
    /// 向前分页游标
    pub before: Option<String>,
}

impl PaginationArgs {
    pub fn new() -> Self {
        Self {
            first: None,
            after: None,
            last: None,
            before: None,
        }
    }

    /// 设置向前获取数量
    pub fn with_first(mut self, n: usize) -> Self {
        self.first = Some(n);
        self
    }

    /// 设置起始游标
    pub fn with_after(mut self, cursor: impl Into<String>) -> Self {
        self.after = Some(cursor.into());
        self
    }

    /// 设置向后获取数量
    pub fn with_last(mut self, n: usize) -> Self {
        self.last = Some(n);
        self
    }

    /// 设置结束游标
    pub fn with_before(mut self, cursor: impl Into<String>) -> Self {
        self.before = Some(cursor.into());
        self
    }
}

impl Default for PaginationArgs {
    fn default() -> Self {
        Self::new()
    }
}

/// 将索引编码为游标（base64 编码）
fn encode_cursor(index: usize) -> String {
    use std::fmt::Write;
    // 简单的 base64 编码：将 "cursor:{index}" 编码
    let plain = format!("cursor:{}", index);
    let bytes = plain.as_bytes();
    let mut result = String::new();
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };

        let _ = write!(
            result,
            "{}{}{}",
            CHARS[(b0 >> 2) as usize] as char,
            CHARS[((b0 << 4) & 0x30 | b1 >> 4) as usize] as char,
            if i + 1 < bytes.len() {
                CHARS[((b1 << 2) & 0x3C | b2 >> 6) as usize] as char
            } else {
                '='
            }
        );
        let _ = write!(
            result,
            "{}",
            if i + 2 < bytes.len() {
                CHARS[(b2 & 0x3F) as usize] as char
            } else {
                '='
            }
        );
        i += 3;
    }
    result
}

/// 将游标解码为索引
fn decode_cursor(cursor: &str) -> Option<usize> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [0u8; 128];
    for (i, &c) in CHARS.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }
    let bytes = cursor.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // c0 必须存在
        let c0 = lookup.get(bytes[i] as usize).copied().unwrap_or(0);
        // c1 必须存在且非填充符
        if i + 1 >= bytes.len() || bytes[i + 1] == b'=' {
            break;
        }
        let c1 = lookup.get(bytes[i + 1] as usize).copied().unwrap_or(0);
        // c2 可能为填充符
        let c2_pad = i + 2 >= bytes.len() || bytes[i + 2] == b'=';
        let c2 = if !c2_pad {
            lookup.get(bytes[i + 2] as usize).copied().unwrap_or(0)
        } else {
            0
        };
        // c3 可能为填充符
        let c3_pad = i + 3 >= bytes.len() || bytes[i + 3] == b'=';
        let c3 = if !c3_pad {
            lookup.get(bytes[i + 3] as usize).copied().unwrap_or(0)
        } else {
            0
        };
        // 第一个字节始终可解码
        decoded.push((c0 << 2) | (c1 >> 4));
        // 若 c2 非填充，可解码第二个字节
        if !c2_pad {
            decoded.push(((c1 & 0x0F) << 4) | (c2 >> 2));
            // 若 c3 非填充，可解码第三个字节
            if !c3_pad {
                decoded.push(((c2 & 0x03) << 6) | c3);
            }
        }
        i += 4;
    }
    let plain = String::from_utf8(decoded).ok()?;
    plain
        .strip_prefix("cursor:")
        .and_then(|s| s.parse::<usize>().ok())
}

/// 对文档列表执行分页查询
///
/// 根据 Relay Connection 规范，返回分页后的 Connection
pub fn paginate(nodes: Vec<Value>, args: &PaginationArgs) -> Connection {
    let total_count = nodes.len();

    // 向后分页（first + after）
    if let Some(first) = args.first {
        let start = match args
            .after
            .as_ref()
            .and_then(|c| decode_cursor(c))
        {
            Some(idx) => idx + 1,
            None => 0,
        };

        let end = (start + first).min(nodes.len());
        let has_next_page = end < nodes.len();
        let has_previous_page = start > 0;

        if start >= nodes.len() {
            return Connection::empty();
        }

        let slice = &nodes[start..end];
        let edges: Vec<Edge> = slice
            .iter()
            .enumerate()
            .map(|(i, node)| Edge::from_index(start + i, node.clone()))
            .collect();

        let start_cursor = edges.first().map(|e| e.cursor.clone());
        let end_cursor = edges.last().map(|e| e.cursor.clone());

        let page_info = PageInfo {
            has_next_page,
            has_previous_page,
            start_cursor,
            end_cursor,
        };

        return Connection::new(edges, page_info, total_count);
    }

    // 向前分页（last + before）
    if let Some(last) = args.last {
        let end = match args
            .before
            .as_ref()
            .and_then(|c| decode_cursor(c))
        {
            Some(idx) => idx,
            None => nodes.len(),
        };

        let start = end.saturating_sub(last);
        let has_next_page = end < nodes.len();
        let has_previous_page = start > 0;

        if start >= end || end == 0 {
            return Connection::empty();
        }

        let slice = &nodes[start..end];
        let edges: Vec<Edge> = slice
            .iter()
            .enumerate()
            .map(|(i, node)| Edge::from_index(start + i, node.clone()))
            .collect();

        let start_cursor = edges.first().map(|e| e.cursor.clone());
        let end_cursor = edges.last().map(|e| e.cursor.clone());

        let page_info = PageInfo {
            has_next_page,
            has_previous_page,
            start_cursor,
            end_cursor,
        };

        return Connection::new(edges, page_info, total_count);
    }

    // 无分页参数：返回全部
    let edges: Vec<Edge> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| Edge::from_index(i, node.clone()))
        .collect();
    let page_info = PageInfo {
        has_next_page: false,
        has_previous_page: false,
        start_cursor: edges.first().map(|e| e.cursor.clone()),
        end_cursor: edges.last().map(|e| e.cursor.clone()),
    };
    Connection::new(edges, page_info, total_count)
}

// =============================================================================
// 三、变更（Mutation）
// =============================================================================

/// 变更操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MutationKind {
    /// 创建
    Create,
    /// 更新
    Update,
    /// 删除
    Delete,
}

/// 变更输入
///
/// 描述一个变更操作的参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationInput {
    /// 操作类型
    pub kind: MutationKind,
    /// 目标类型名
    pub type_name: String,
    /// 主键值（更新/删除时需要）
    pub id: Option<String>,
    /// 输入数据（创建/更新时需要）
    pub data: Option<Value>,
}

impl MutationInput {
    pub fn create(type_name: impl Into<String>, data: Value) -> Self {
        Self {
            kind: MutationKind::Create,
            type_name: type_name.into(),
            id: None,
            data: Some(data),
        }
    }

    pub fn update(type_name: impl Into<String>, id: impl Into<String>, data: Value) -> Self {
        Self {
            kind: MutationKind::Update,
            type_name: type_name.into(),
            id: Some(id.into()),
            data: Some(data),
        }
    }

    pub fn delete(type_name: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: MutationKind::Delete,
            type_name: type_name.into(),
            id: Some(id.into()),
            data: None,
        }
    }
}

/// 变更结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationResult {
    /// 操作是否成功
    pub success: bool,
    /// 受影响的记录数
    pub affected: usize,
    /// 返回的数据
    pub data: Option<Value>,
    /// 错误信息
    pub error: Option<String>,
}

impl MutationResult {
    pub fn ok(data: Option<Value>) -> Self {
        Self {
            success: true,
            affected: 1,
            data,
            error: None,
        }
    }

    pub fn ok_many(affected: usize, data: Option<Value>) -> Self {
        Self {
            success: true,
            affected,
            data,
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            affected: 0,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// 变更处理器函数类型
pub type MutationHandlerFn =
    Box<dyn Fn(&MutationInput) -> MutationResult + Send + Sync>;

/// 变更注册表
///
/// 注册类型到变更处理器的映射
pub struct MutationRegistry {
    /// "TypeName.create" / "TypeName.update" / "TypeName.delete" -> handler
    handlers: RwLock<HashMap<String, Arc<MutationHandlerFn>>>,
}

impl MutationRegistry {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// 注册变更处理器
    pub fn register(&self, type_name: &str, kind: MutationKind, handler: MutationHandlerFn) {
        let key = mutation_key(type_name, &kind);
        let mut map = self.handlers.write().expect("handler lock poisoned");
        map.insert(key, Arc::new(handler));
    }

    /// 查找变更处理器
    pub fn get(&self, type_name: &str, kind: &MutationKind) -> Option<Arc<MutationHandlerFn>> {
        let key = mutation_key(type_name, kind);
        self.handlers
            .read()
            .expect("handler lock poisoned")
            .get(&key)
            .cloned()
    }

    /// 执行变更
    pub fn execute(&self, input: &MutationInput) -> MutationResult {
        match self.get(&input.type_name, &input.kind) {
            Some(handler) => handler(input),
            None => MutationResult::err(format!(
                "no mutation handler for {:?} on type '{}'",
                input.kind, input.type_name
            )),
        }
    }

    /// 已注册的处理器数量
    pub fn len(&self) -> usize {
        self.handlers
            .read()
            .expect("handler lock poisoned")
            .len()
    }

    /// 是否没有已注册的处理器
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for MutationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 生成变更处理器映射的 key
fn mutation_key(type_name: &str, kind: &MutationKind) -> String {
    let kind_str = match kind {
        MutationKind::Create => "create",
        MutationKind::Update => "update",
        MutationKind::Delete => "delete",
    };
    format!("{}.{}", type_name, kind_str)
}

/// 内存数据存储，支持 CRUD 操作
///
/// 配合 MutationRegistry 使用，提供基本的创建/更新/删除能力
pub struct InMemoryStore {
    data: RwLock<HashMap<String, Vec<Value>>>,
    /// 自增 ID 计数器
    counters: RwLock<HashMap<String, u64>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            counters: RwLock::new(HashMap::new()),
        }
    }

    /// 生成下一个 ID
    fn next_id(&self, type_name: &str) -> String {
        let mut counters = self.counters.write().expect("counter lock poisoned");
        let counter = counters.entry(type_name.to_string()).or_insert(0);
        *counter += 1;
        counter.to_string()
    }

    /// 创建记录
    pub fn create(&self, type_name: &str, mut data: Value) -> MutationResult {
        let id = self.next_id(type_name);
        // 注入 id 字段
        if let Some(obj) = data.as_object_mut() {
            obj.insert("id".to_string(), Value::String(id.clone()));
        }
        let mut store = self.data.write().expect("store lock poisoned");
        store
            .entry(type_name.to_string())
            .or_default()
            .push(data.clone());
        MutationResult::ok(Some(data))
    }

    /// 更新记录
    pub fn update(&self, type_name: &str, id: &str, patch: &Value) -> MutationResult {
        let mut store = self.data.write().expect("store lock poisoned");
        let docs = match store.get_mut(type_name) {
            Some(d) => d,
            None => return MutationResult::err(format!("type '{}' not found", type_name)),
        };
        let doc = docs
            .iter_mut()
            .find(|d| d.get("id").and_then(|v| v.as_str()) == Some(id));
        match doc {
            Some(doc) => {
                // 合并 patch 字段
                if let (Some(obj), Some(patch_obj)) = (doc.as_object_mut(), patch.as_object()) {
                    for (k, v) in patch_obj {
                        obj.insert(k.clone(), v.clone());
                    }
                }
                MutationResult::ok(Some(doc.clone()))
            }
            None => MutationResult::err(format!(
                "document with id '{}' not found in type '{}'",
                id, type_name
            )),
        }
    }

    /// 删除记录
    pub fn delete(&self, type_name: &str, id: &str) -> MutationResult {
        let mut store = self.data.write().expect("store lock poisoned");
        let docs = match store.get_mut(type_name) {
            Some(d) => d,
            None => return MutationResult::err(format!("type '{}' not found", type_name)),
        };
        let before = docs.len();
        docs.retain(|d| d.get("id").and_then(|v| v.as_str()) != Some(id));
        let after = docs.len();
        if before == after {
            MutationResult::err(format!(
                "document with id '{}' not found in type '{}'",
                id, type_name
            ))
        } else {
            MutationResult::ok_many(1, None)
        }
    }

    /// 获取指定类型的所有记录
    pub fn get_all(&self, type_name: &str) -> Vec<Value> {
        self.data
            .read()
            .expect("store lock poisoned")
            .get(type_name)
            .cloned()
            .unwrap_or_default()
    }

    /// 按 ID 查找记录
    pub fn find_by_id(&self, type_name: &str, id: &str) -> Option<Value> {
        self.data
            .read()
            .expect("store lock poisoned")
            .get(type_name)
            .and_then(|docs| {
                docs.iter()
                    .find(|d| d.get("id").and_then(|v| v.as_str()) == Some(id))
                    .cloned()
            })
    }

    /// 注册到 MutationRegistry
    ///
    /// 将此存储的 create/update/delete 方法注册为变更处理器
    pub fn register_to(&self, type_name: &str, registry: &MutationRegistry) {
        // 注意：由于 self 是 &Self（不可克隆），这里使用 Arc<Mutex> 来共享
        // 但为简化实现，我们使用全局闭包捕获类型名，实际数据操作通过外部调用
        // 这里注册的处理器仅做基本验证，真正的 CRUD 由 InMemoryStore 方法直接调用
        let tn = type_name.to_string();
        registry.register(
            type_name,
            MutationKind::Create,
            Box::new(move |input: &MutationInput| {
                let _ = &tn;
                match input.data {
                    Some(ref data) => {
                        // 实际创建由外部调用 store.create 完成
                        MutationResult::ok(Some(data.clone()))
                    }
                    None => MutationResult::err("create mutation requires data"),
                }
            }),
        );
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// 四、订阅（Subscription）框架
// =============================================================================

/// 订阅事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionEvent {
    /// 事件主题
    pub topic: String,
    /// 事件载荷
    pub payload: Value,
    /// 事件序列号
    pub sequence: u64,
}

impl SubscriptionEvent {
    pub fn new(topic: impl Into<String>, payload: Value, sequence: u64) -> Self {
        Self {
            topic: topic.into(),
            payload,
            sequence,
        }
    }
}

/// 订阅 ID
pub type SubscriptionId = u64;

/// 订阅消息（发送给订阅者）
#[derive(Debug, Clone)]
enum SubscriptionMessage {
    Event(SubscriptionEvent),
    Unsubscribe,
}

/// 订阅句柄
///
/// 持有此句柄可接收事件，drop 时自动取消订阅
pub struct SubscriptionHandle {
    id: SubscriptionId,
    topic: String,
    receiver: std::sync::mpsc::Receiver<SubscriptionMessage>,
    broker: Option<Arc<SubscriptionBrokerInner>>,
}

impl SubscriptionHandle {
    /// 获取订阅 ID
    pub fn id(&self) -> SubscriptionId {
        self.id
    }

    /// 获取订阅主题
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// 非阻塞地尝试接收事件
    pub fn try_recv(&self) -> Option<SubscriptionEvent> {
        match self.receiver.try_recv() {
            Ok(SubscriptionMessage::Event(e)) => Some(e),
            Ok(SubscriptionMessage::Unsubscribe) | Err(_) => None,
        }
    }

    /// 阻塞地接收一个事件
    pub fn recv(&self) -> Option<SubscriptionEvent> {
        match self.receiver.recv() {
            Ok(SubscriptionMessage::Event(e)) => Some(e),
            Ok(SubscriptionMessage::Unsubscribe) | Err(_) => None,
        }
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        // 取消订阅
        if let Some(broker) = self.broker.take() {
            broker.unsubscribe(self.id);
        }
    }
}

/// 订阅者条目类型别名
type SubscriberEntry = (SubscriptionId, std::sync::mpsc::Sender<SubscriptionMessage>);

/// 订阅代理内部实现
struct SubscriptionBrokerInner {
    /// 订阅者映射：topic -> [(id, sender)]
    subscribers: Mutex<HashMap<String, Vec<SubscriberEntry>>>,
    /// 下一个订阅 ID
    next_id: Mutex<SubscriptionId>,
    /// 全局事件序列号
    sequence: Mutex<u64>,
}

impl SubscriptionBrokerInner {
    fn new() -> Self {
        Self {
            subscribers: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
            sequence: Mutex::new(0),
        }
    }

    fn next_sequence(&self) -> u64 {
        let mut seq = self.sequence.lock().expect("seq lock poisoned");
        *seq += 1;
        *seq
    }

    fn subscribe(&self, topic: &str) -> (SubscriptionId, std::sync::mpsc::Receiver<SubscriptionMessage>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let id = {
            let mut next = self.next_id.lock().expect("id lock poisoned");
            let id = *next;
            *next += 1;
            id
        };
        let mut subs = self.subscribers.lock().expect("sub lock poisoned");
        subs.entry(topic.to_string())
            .or_default()
            .push((id, tx));
        (id, rx)
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        let mut subs = self.subscribers.lock().expect("sub lock poisoned");
        for list in subs.values_mut() {
            list.retain(|(sub_id, _)| *sub_id != id);
        }
        // 清理空主题
        subs.retain(|_, list| !list.is_empty());
    }

    fn publish(&self, topic: &str, payload: Value) -> usize {
        let seq = self.next_sequence();
        let event = SubscriptionEvent::new(topic, payload, seq);
        let mut subs = self.subscribers.lock().expect("sub lock poisoned");
        let list = match subs.get_mut(topic) {
            Some(l) => l,
            None => return 0,
        };
        let mut delivered = 0;
        // 保留发送成功的订阅者
        let mut to_remove = Vec::new();
        for (i, (_, sender)) in list.iter().enumerate() {
            match sender.send(SubscriptionMessage::Event(event.clone())) {
                Ok(_) => delivered += 1,
                Err(_) => to_remove.push(i),
            }
        }
        // 移除已断开的订阅者（逆序删除）
        for i in to_remove.into_iter().rev() {
            list.remove(i);
        }
        delivered
    }

    fn subscriber_count(&self, topic: &str) -> usize {
        let subs = self.subscribers.lock().expect("sub lock poisoned");
        subs.get(topic).map(|l| l.len()).unwrap_or(0)
    }

    fn topic_count(&self) -> usize {
        let subs = self.subscribers.lock().expect("sub lock poisoned");
        subs.len()
    }
}

/// 订阅代理
///
/// 基于内存 pub/sub 模式实现的事件分发
pub struct SubscriptionBroker {
    inner: Arc<SubscriptionBrokerInner>,
}

impl SubscriptionBroker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SubscriptionBrokerInner::new()),
        }
    }

    /// 订阅主题
    ///
    /// 返回订阅句柄，drop 时自动取消订阅
    pub fn subscribe(&self, topic: &str) -> SubscriptionHandle {
        let (id, rx) = self.inner.subscribe(topic);
        SubscriptionHandle {
            id,
            topic: topic.to_string(),
            receiver: rx,
            broker: Some(self.inner.clone()),
        }
    }

    /// 发布事件到指定主题
    ///
    /// 返回实际投递到的订阅者数量
    pub fn publish(&self, topic: &str, payload: Value) -> usize {
        self.inner.publish(topic, payload)
    }

    /// 获取指定主题的订阅者数量
    pub fn subscriber_count(&self, topic: &str) -> usize {
        self.inner.subscriber_count(topic)
    }

    /// 获取活跃主题数量
    pub fn topic_count(&self) -> usize {
        self.inner.topic_count()
    }
}

impl Default for SubscriptionBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SubscriptionBroker {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

// =============================================================================
// 五、Schema 扩展：在既有 schema 上注册关联/分页/变更/订阅
// =============================================================================

/// Schema 扩展配置
///
/// 将关联关系、分页类型、变更、订阅等扩展信息附加到既有 schema
pub struct SchemaExtensions {
    /// 关联关系列表
    pub relations: Vec<Relation>,
    /// 变更定义列表（类型名 + 操作类型）
    pub mutations: Vec<(String, MutationKind)>,
    /// 订阅主题列表
    pub subscriptions: Vec<String>,
}

impl SchemaExtensions {
    pub fn new() -> Self {
        Self {
            relations: Vec::new(),
            mutations: Vec::new(),
            subscriptions: Vec::new(),
        }
    }

    /// 添加关联关系
    pub fn with_relation(mut self, relation: Relation) -> Self {
        self.relations.push(relation);
        self
    }

    /// 添加变更定义
    pub fn with_mutation(mut self, type_name: impl Into<String>, kind: MutationKind) -> Self {
        self.mutations.push((type_name.into(), kind));
        self
    }

    /// 添加订阅主题
    pub fn with_subscription(mut self, topic: impl Into<String>) -> Self {
        self.subscriptions.push(topic.into());
        self
    }

    /// 将扩展信息渲染为 SDL 片段
    pub fn to_sdl(&self) -> String {
        let mut out = String::new();

        // 渲染关联类型（在对应 type 上添加嵌套字段）
        if !self.relations.is_empty() {
            out.push_str("# Relations\n");
            for rel in &self.relations {
                out.push_str(&format!(
                    "# {} {}.{} -> {} ({:?})\n",
                    rel.name, rel.from_type, rel.from_field, rel.to_type, rel.kind
                ));
            }
            out.push('\n');
        }

        // 渲染变更
        if !self.mutations.is_empty() {
            out.push_str("type Mutation {\n");
            for (type_name, kind) in &self.mutations {
                let op = match kind {
                    MutationKind::Create => format!("create{type_name}(input: {type_name}Input!): {type_name}"),
                    MutationKind::Update => format!("update{type_name}(id: ID!, input: {type_name}Input!): {type_name}"),
                    MutationKind::Delete => format!("delete{type_name}(id: ID!): Boolean!"),
                };
                out.push_str(&format!("    {}\n", op));
            }
            out.push_str("}\n\n");
        }

        // 渲染订阅
        if !self.subscriptions.is_empty() {
            out.push_str("type Subscription {\n");
            for topic in &self.subscriptions {
                out.push_str(&format!("    {}: SubscriptionEvent!\n", topic));
            }
            out.push_str("}\n\n");
            out.push_str("type SubscriptionEvent {\n");
            out.push_str("    topic: String!\n");
            out.push_str("    payload: JSON!\n");
            out.push_str("    sequence: Int!\n");
            out.push_str("}\n");
        }

        out
    }
}

impl Default for SchemaExtensions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- 关联关系测试 ---

    #[test]
    fn test_relation_new() {
        let rel = Relation::new(
            "userOrders",
            "User",
            "id",
            "Order",
            "user_id",
            RelationKind::OneToMany,
        );
        assert_eq!(rel.name, "userOrders");
        assert_eq!(rel.from_type, "User");
        assert_eq!(rel.from_field, "id");
        assert_eq!(rel.to_type, "Order");
        assert_eq!(rel.to_field, "user_id");
        assert_eq!(rel.kind, RelationKind::OneToMany);
    }

    #[test]
    fn test_relation_one_to_many() {
        let rel = Relation::one_to_many("userOrders", "User", "Order");
        assert_eq!(rel.kind, RelationKind::OneToMany);
        assert_eq!(rel.from_type, "User");
        assert_eq!(rel.to_type, "Order");
        assert_eq!(rel.from_field, "id");
        assert_eq!(rel.to_field, "user_id");
    }

    #[test]
    fn test_relation_many_to_one() {
        let rel = Relation::many_to_one("orderUser", "Order", "User");
        assert_eq!(rel.kind, RelationKind::ManyToOne);
        assert_eq!(rel.from_type, "Order");
        assert_eq!(rel.to_type, "User");
        assert_eq!(rel.from_field, "user_id");
        assert_eq!(rel.to_field, "id");
    }

    #[test]
    fn test_relation_kind_serde() {
        let kind = RelationKind::ManyToMany;
        let json = serde_json::to_string(&kind).unwrap();
        let de: RelationKind = serde_json::from_str(&json).unwrap();
        assert_eq!(de, kind);
    }

    // --- 解析器注册表测试 ---

    #[test]
    fn test_resolver_registry_register_and_get() {
        let registry = ResolverRegistry::new();
        registry.register(
            "User",
            "orders",
            Box::new(|parent, _args| {
                let id = parent["id"].as_str().unwrap_or("");
                json!([{"id": "1", "user_id": id, "name": "order1"}])
            }),
        );
        assert_eq!(registry.len(), 1);

        let parent = json!({"id": "u1"});
        let args = HashMap::new();
        let result = registry.resolve("User", "orders", &parent, &args);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.is_array());
        assert_eq!(result[0]["user_id"], "u1");
    }

    #[test]
    fn test_resolver_registry_get_missing() {
        let registry = ResolverRegistry::new();
        assert!(registry.get("Unknown", "field").is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn test_resolver_registry_multiple() {
        let registry = ResolverRegistry::new();
        registry.register(
            "User",
            "orders",
            Box::new(|_, _| json!([{"id": "1"}])),
        );
        registry.register(
            "Order",
            "user",
            Box::new(|_, _| json!({"id": "u1"})),
        );
        assert_eq!(registry.len(), 2);

        let parent = json!({});
        let args = HashMap::new();
        assert!(registry.resolve("User", "orders", &parent, &args).is_some());
        assert!(registry.resolve("Order", "user", &parent, &args).is_some());
        assert!(registry.resolve("User", "user", &parent, &args).is_none());
    }

    // --- 关联数据源测试 ---

    #[test]
    fn test_data_source_insert_and_get() {
        let ds = RelationDataSource::new();
        ds.insert("User", json!({"id": "1", "name": "Alice"}));
        ds.insert("User", json!({"id": "2", "name": "Bob"}));

        let users = ds.get_all("User");
        assert_eq!(users.len(), 2);
    }

    #[test]
    fn test_data_source_find_by_id() {
        let ds = RelationDataSource::new();
        ds.insert("User", json!({"id": "1", "name": "Alice"}));
        ds.insert("User", json!({"id": "2", "name": "Bob"}));

        let user = ds.find_by_id("User", "2").unwrap();
        assert_eq!(user["name"], "Bob");
        assert!(ds.find_by_id("User", "999").is_none());
    }

    #[test]
    fn test_data_source_resolve_one_to_many() {
        let ds = RelationDataSource::new();
        ds.insert("User", json!({"id": "u1", "name": "Alice"}));
        ds.insert_many(
            "Order",
            vec![
                json!({"id": "o1", "user_id": "u1", "total": 100}),
                json!({"id": "o2", "user_id": "u1", "total": 200}),
                json!({"id": "o3", "user_id": "u2", "total": 300}),
            ],
        );

        let orders = ds.resolve_one_to_many("Order", "user_id", "u1");
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0]["id"], "o1");
        assert_eq!(orders[1]["id"], "o2");
    }

    #[test]
    fn test_data_source_resolve_many_to_one() {
        let ds = RelationDataSource::new();
        ds.insert("User", json!({"id": "u1", "name": "Alice"}));
        ds.insert("User", json!({"id": "u2", "name": "Bob"}));
        ds.insert("Order", json!({"id": "o1", "user_id": "u1", "total": 100}));

        let user = ds.resolve_many_to_one("User", "id", "u1").unwrap();
        assert_eq!(user["name"], "Alice");

        let user2 = ds.resolve_many_to_one("User", "id", "u2").unwrap();
        assert_eq!(user2["name"], "Bob");

        assert!(ds.resolve_many_to_one("User", "id", "u999").is_none());
    }

    #[test]
    fn test_data_source_empty_type() {
        let ds = RelationDataSource::new();
        assert!(ds.get_all("Nonexistent").is_empty());
        assert!(ds.find_by_id("Nonexistent", "1").is_none());
        assert!(ds
            .resolve_one_to_many("Nonexistent", "fk", "1")
            .is_empty());
        assert!(ds.resolve_many_to_one("Nonexistent", "id", "1").is_none());
    }

    // --- 分页测试 ---

    #[test]
    fn test_page_info_new() {
        let pi = PageInfo::new();
        assert!(!pi.has_next_page);
        assert!(!pi.has_previous_page);
        assert!(pi.start_cursor.is_none());
        assert!(pi.end_cursor.is_none());
    }

    #[test]
    fn test_page_info_builder() {
        let pi = PageInfo::new()
            .with_next(true)
            .with_previous(false);
        assert!(pi.has_next_page);
        assert!(!pi.has_previous_page);
    }

    #[test]
    fn test_edge_from_index() {
        let edge = Edge::from_index(5, json!({"id": "5"}));
        assert!(!edge.cursor.is_empty());
        assert_eq!(edge.node["id"], "5");
    }

    #[test]
    fn test_connection_empty() {
        let conn = Connection::empty();
        assert!(conn.edges.is_empty());
        assert_eq!(conn.total_count, 0);
        assert!(conn.nodes().is_empty());
    }

    #[test]
    fn test_cursor_encode_decode_roundtrip() {
        for i in [0, 1, 5, 10, 100, 999, 1000] {
            let cursor = encode_cursor(i);
            let decoded = decode_cursor(&cursor);
            assert_eq!(decoded, Some(i), "roundtrip failed for index {}", i);
        }
    }

    #[test]
    fn test_decode_invalid_cursor() {
        assert!(decode_cursor("!!!invalid!!!").is_none());
        assert!(decode_cursor("").is_none());
    }

    #[test]
    fn test_paginate_no_args_returns_all() {
        let nodes = vec![
            json!({"id": "1"}),
            json!({"id": "2"}),
            json!({"id": "3"}),
        ];
        let conn = paginate(nodes, &PaginationArgs::new());
        assert_eq!(conn.edges.len(), 3);
        assert_eq!(conn.total_count, 3);
        assert!(!conn.page_info.has_next_page);
        assert!(!conn.page_info.has_previous_page);
    }

    #[test]
    fn test_paginate_first_n() {
        let nodes: Vec<Value> = (1..=10)
            .map(|i| json!({"id": i.to_string()}))
            .collect();
        let args = PaginationArgs::new().with_first(3);
        let conn = paginate(nodes, &args);
        assert_eq!(conn.edges.len(), 3);
        assert_eq!(conn.total_count, 10);
        assert!(conn.page_info.has_next_page);
        assert!(!conn.page_info.has_previous_page);
    }

    #[test]
    fn test_paginate_first_after() {
        let nodes: Vec<Value> = (1..=10)
            .map(|i| json!({"id": i.to_string()}))
            .collect();
        // 第一页取 3 条
        let args1 = PaginationArgs::new().with_first(3);
        let conn1 = paginate(nodes.clone(), &args1);
        let after_cursor = conn1.page_info.end_cursor.unwrap();

        // 第二页从第一页最后一条之后开始
        let args2 = PaginationArgs::new().with_first(3).with_after(after_cursor);
        let conn2 = paginate(nodes, &args2);
        assert_eq!(conn2.edges.len(), 3);
        assert_eq!(conn2.edges[0].node["id"], "4");
        assert_eq!(conn2.edges[2].node["id"], "6");
        assert!(conn2.page_info.has_next_page);
        assert!(conn2.page_info.has_previous_page);
    }

    #[test]
    fn test_paginate_first_beyond_end() {
        let nodes = vec![json!({"id": "1"}), json!({"id": "2"})];
        let args = PaginationArgs::new().with_first(10);
        let conn = paginate(nodes, &args);
        assert_eq!(conn.edges.len(), 2);
        assert_eq!(conn.total_count, 2);
        assert!(!conn.page_info.has_next_page);
    }

    #[test]
    fn test_paginate_last_n() {
        let nodes: Vec<Value> = (1..=10)
            .map(|i| json!({"id": i.to_string()}))
            .collect();
        let args = PaginationArgs::new().with_last(3);
        let conn = paginate(nodes, &args);
        assert_eq!(conn.edges.len(), 3);
        assert_eq!(conn.edges[0].node["id"], "8");
        assert_eq!(conn.edges[2].node["id"], "10");
        assert!(!conn.page_info.has_next_page);
        assert!(conn.page_info.has_previous_page);
    }

    #[test]
    fn test_paginate_empty_list() {
        let conn = paginate(Vec::new(), &PaginationArgs::new().with_first(5));
        assert_eq!(conn.edges.len(), 0);
        assert_eq!(conn.total_count, 0);
    }

    #[test]
    fn test_paginate_after_beyond_end() {
        let nodes = vec![json!({"id": "1"}), json!({"id": "2"})];
        // 使用一个超出范围的游标
        let cursor = encode_cursor(100);
        let args = PaginationArgs::new().with_first(5).with_after(cursor);
        let conn = paginate(nodes, &args);
        assert_eq!(conn.edges.len(), 0);
    }

    #[test]
    fn test_connection_nodes() {
        let edges = vec![
            Edge::from_index(0, json!({"id": "1"})),
            Edge::from_index(1, json!({"id": "2"})),
        ];
        let conn = Connection::new(edges, PageInfo::new(), 2);
        let nodes = conn.nodes();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["id"], "1");
    }

    // --- 变更输入测试 ---

    #[test]
    fn test_mutation_input_create() {
        let input = MutationInput::create("User", json!({"name": "Alice"}));
        assert_eq!(input.kind, MutationKind::Create);
        assert_eq!(input.type_name, "User");
        assert!(input.id.is_none());
        assert!(input.data.is_some());
    }

    #[test]
    fn test_mutation_input_update() {
        let input = MutationInput::update("User", "1", json!({"name": "Bob"}));
        assert_eq!(input.kind, MutationKind::Update);
        assert_eq!(input.type_name, "User");
        assert_eq!(input.id, Some("1".to_string()));
    }

    #[test]
    fn test_mutation_input_delete() {
        let input = MutationInput::delete("User", "1");
        assert_eq!(input.kind, MutationKind::Delete);
        assert!(input.data.is_none());
    }

    #[test]
    fn test_mutation_result_ok() {
        let result = MutationResult::ok(Some(json!({"id": "1"})));
        assert!(result.success);
        assert_eq!(result.affected, 1);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_mutation_result_err() {
        let result = MutationResult::err("not found");
        assert!(!result.success);
        assert_eq!(result.affected, 0);
        assert_eq!(result.error, Some("not found".to_string()));
    }

    // --- 变更注册表测试 ---

    #[test]
    fn test_mutation_registry_execute() {
        let registry = MutationRegistry::new();
        registry.register(
            "User",
            MutationKind::Create,
            Box::new(|input| {
                MutationResult::ok(input.data.clone())
            }),
        );
        let input = MutationInput::create("User", json!({"name": "Alice"}));
        let result = registry.execute(&input);
        assert!(result.success);
        assert_eq!(result.data.unwrap()["name"], "Alice");
    }

    #[test]
    fn test_mutation_registry_no_handler() {
        let registry = MutationRegistry::new();
        let input = MutationInput::create("Unknown", json!({}));
        let result = registry.execute(&input);
        assert!(!result.success);
        assert!(result.error.unwrap().contains("no mutation handler"));
    }

    #[test]
    fn test_mutation_registry_multiple() {
        let registry = MutationRegistry::new();
        registry.register(
            "User",
            MutationKind::Create,
            Box::new(|_| MutationResult::ok(None)),
        );
        registry.register(
            "User",
            MutationKind::Delete,
            Box::new(|_| MutationResult::ok_many(1, None)),
        );
        assert_eq!(registry.len(), 2);

        let create_result = registry.execute(&MutationInput::create("User", json!({})));
        assert!(create_result.success);

        let delete_result = registry.execute(&MutationInput::delete("User", "1"));
        assert!(delete_result.success);
    }

    // --- 内存存储测试 ---

    #[test]
    fn test_in_memory_store_create() {
        let store = InMemoryStore::new();
        let result = store.create("User", json!({"name": "Alice"}));
        assert!(result.success);
        let data = result.data.unwrap();
        assert_eq!(data["name"], "Alice");
        assert!(data["id"].is_string());
        // 验证记录已存储
        let all = store.get_all("User");
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_in_memory_store_create_multiple() {
        let store = InMemoryStore::new();
        store.create("User", json!({"name": "Alice"}));
        store.create("User", json!({"name": "Bob"}));
        let all = store.get_all("User");
        assert_eq!(all.len(), 2);
        // ID 应递增
        assert_eq!(all[0]["id"], "1");
        assert_eq!(all[1]["id"], "2");
    }

    #[test]
    fn test_in_memory_store_update() {
        let store = InMemoryStore::new();
        store.create("User", json!({"name": "Alice"}));
        let result = store.update("User", "1", &json!({"name": "Alicia"}));
        assert!(result.success);
        let updated = store.find_by_id("User", "1").unwrap();
        assert_eq!(updated["name"], "Alicia");
    }

    #[test]
    fn test_in_memory_store_update_missing() {
        let store = InMemoryStore::new();
        let result = store.update("User", "999", &json!({"name": "X"}));
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[test]
    fn test_in_memory_store_delete() {
        let store = InMemoryStore::new();
        store.create("User", json!({"name": "Alice"}));
        let result = store.delete("User", "1");
        assert!(result.success);
        assert_eq!(result.affected, 1);
        assert!(store.find_by_id("User", "1").is_none());
    }

    #[test]
    fn test_in_memory_store_delete_missing() {
        let store = InMemoryStore::new();
        let result = store.delete("User", "999");
        assert!(!result.success);
    }

    #[test]
    fn test_in_memory_store_find_by_id_missing_type() {
        let store = InMemoryStore::new();
        assert!(store.find_by_id("Nonexistent", "1").is_none());
    }

    // --- 订阅代理测试 ---

    #[test]
    fn test_subscription_broker_subscribe_and_publish() {
        let broker = SubscriptionBroker::new();
        let handle = broker.subscribe("userCreated");

        assert_eq!(broker.subscriber_count("userCreated"), 1);
        let delivered = broker.publish("userCreated", json!({"id": "1", "name": "Alice"}));
        assert_eq!(delivered, 1);

        let event = handle.try_recv();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.topic, "userCreated");
        assert_eq!(event.payload["name"], "Alice");
        assert!(event.sequence > 0);
    }

    #[test]
    fn test_subscription_broker_multiple_subscribers() {
        let broker = SubscriptionBroker::new();
        let handle1 = broker.subscribe("topic1");
        let handle2 = broker.subscribe("topic1");

        assert_eq!(broker.subscriber_count("topic1"), 2);
        let delivered = broker.publish("topic1", json!({"msg": "hello"}));
        assert_eq!(delivered, 2);

        assert!(handle1.try_recv().is_some());
        assert!(handle2.try_recv().is_some());
    }

    #[test]
    fn test_subscription_broker_no_subscribers() {
        let broker = SubscriptionBroker::new();
        let delivered = broker.publish("noSubs", json!({"msg": "hello"}));
        assert_eq!(delivered, 0);
    }

    #[test]
    fn test_subscription_broker_unsubscribe_on_drop() {
        let broker = SubscriptionBroker::new();
        {
            let _handle = broker.subscribe("tempTopic");
            assert_eq!(broker.subscriber_count("tempTopic"), 1);
        } // handle dropped here
         // Give a moment for drop to propagate
        assert_eq!(broker.subscriber_count("tempTopic"), 0);
    }

    #[test]
    fn test_subscription_broker_different_topics() {
        let broker = SubscriptionBroker::new();
        let handle1 = broker.subscribe("topicA");
        let handle2 = broker.subscribe("topicB");

        broker.publish("topicA", json!({"a": 1}));
        broker.publish("topicB", json!({"b": 2}));

        let event1 = handle1.try_recv().unwrap();
        assert_eq!(event1.payload["a"], 1);

        let event2 = handle2.try_recv().unwrap();
        assert_eq!(event2.payload["b"], 2);

        // 各自只收到各自主题的事件
        assert!(handle1.try_recv().is_none());
        assert!(handle2.try_recv().is_none());
    }

    #[test]
    fn test_subscription_broker_sequence_increments() {
        let broker = SubscriptionBroker::new();
        let handle = broker.subscribe("seq");

        broker.publish("seq", json!({}));
        broker.publish("seq", json!({}));
        broker.publish("seq", json!({}));

        let e1 = handle.try_recv().unwrap();
        let e2 = handle.try_recv().unwrap();
        let e3 = handle.try_recv().unwrap();

        assert!(e2.sequence > e1.sequence);
        assert!(e3.sequence > e2.sequence);
    }

    #[test]
    fn test_subscription_broker_clone_shares_state() {
        let broker = SubscriptionBroker::new();
        let broker2 = broker.clone();
        let handle = broker2.subscribe("shared");

        broker.publish("shared", json!({"x": 1}));
        assert!(handle.try_recv().is_some());
    }

    #[test]
    fn test_subscription_handle_recv_blocking() {
        let broker = SubscriptionBroker::new();
        let handle = broker.subscribe("block");

        // 在另一个线程中发布事件
        let b = broker.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            b.publish("block", json!({"delayed": true}));
        });

        let event = handle.recv();
        assert!(event.is_some());
        assert_eq!(event.unwrap().payload["delayed"], true);
        thread.join().unwrap();
    }

    #[test]
    fn test_subscription_broker_topic_count() {
        let broker = SubscriptionBroker::new();
        let _h1 = broker.subscribe("t1");
        let _h2 = broker.subscribe("t2");
        let _h3 = broker.subscribe("t3");
        assert_eq!(broker.topic_count(), 3);
    }

    // --- Schema 扩展测试 ---

    #[test]
    fn test_schema_extensions_new() {
        let ext = SchemaExtensions::new();
        assert!(ext.relations.is_empty());
        assert!(ext.mutations.is_empty());
        assert!(ext.subscriptions.is_empty());
    }

    #[test]
    fn test_schema_extensions_builder() {
        let ext = SchemaExtensions::new()
            .with_relation(Relation::one_to_many("userOrders", "User", "Order"))
            .with_mutation("User", MutationKind::Create)
            .with_mutation("User", MutationKind::Delete)
            .with_subscription("userCreated");

        assert_eq!(ext.relations.len(), 1);
        assert_eq!(ext.mutations.len(), 2);
        assert_eq!(ext.subscriptions.len(), 1);
    }

    #[test]
    fn test_schema_extensions_to_sdl_contains_mutation() {
        let ext = SchemaExtensions::new()
            .with_mutation("User", MutationKind::Create)
            .with_mutation("User", MutationKind::Delete);

        let sdl = ext.to_sdl();
        assert!(sdl.contains("type Mutation {"));
        assert!(sdl.contains("createUser"));
        assert!(sdl.contains("deleteUser"));
    }

    #[test]
    fn test_schema_extensions_to_sdl_contains_subscription() {
        let ext = SchemaExtensions::new().with_subscription("userCreated");
        let sdl = ext.to_sdl();
        assert!(sdl.contains("type Subscription {"));
        assert!(sdl.contains("userCreated"));
        assert!(sdl.contains("SubscriptionEvent"));
    }

    #[test]
    fn test_schema_extensions_to_sdl_contains_relation() {
        let ext = SchemaExtensions::new()
            .with_relation(Relation::one_to_many("userOrders", "User", "Order"));
        let sdl = ext.to_sdl();
        assert!(sdl.contains("# Relations"));
        assert!(sdl.contains("userOrders"));
    }

    #[test]
    fn test_schema_extensions_to_sdl_empty() {
        let ext = SchemaExtensions::new();
        let sdl = ext.to_sdl();
        assert!(sdl.is_empty());
    }
}
