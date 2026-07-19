//! ResultMap 高级映射 + Native Query + ResultSetMapping
//!
//! 对应文档 6.8 节改进项 30（ResultMap 高级映射）+ 42（Native Query + ResultSetMapping）。
//!
//! # 核心概念
//!
//! - **ResultMap**：声明式结果映射规则（id property + result property + association + collection + discriminator）
//! - **ResultSetMapping**：Hibernate `@SqlResultSetMapping` 风格，原生 SQL 的结果映射
//! - **NativeQuery**：原生 SQL + ResultSetMapping 引用
//! - **ResultMapRegistry**：注册中心，按 id 索引
//! - **RowData**：行数据（列名 -> Value）
//!
//! # 设计灵感
//!
//! - MyBatis `resultMap`（最强大的 resultMap 模型，支持 discriminator 多态、association/collection 嵌套）
//! - Hibernate `@SqlResultSetMapping`（JPA 标准）
//! - Doctrine `ResultSetMappingBuilder`
//!
//! # 优势
//!
//! 1. **多态鉴别器**：通过 discriminator 按列值分派到不同 ResultMap
//! 2. **嵌套映射**：association（一对一）+ collection（一对多）支持嵌套
//! 3. **列前缀**：JOIN 查询结果通过列前缀隔离不同实体的列
//! 4. **原生 SQL**：NativeQuery 可执行任意 SQL 并通过 ResultSetMapping 映射
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::result_map::{
//!     ResultMap, Mapping, NestedAssociation, ResultMapRegistry, RowData, apply_result_map,
//! };
//! use sz_orm_core::Value;
//! use std::collections::HashMap;
//!
//! // 1. 注册 ResultMap
//! let mut registry = ResultMapRegistry::new();
//!
//! let mut dept_map = ResultMap::new("deptMap", "Dept");
//! dept_map.add_id_mapping(Mapping::new("id", "dept_id"));
//! dept_map.add_result_mapping(Mapping::new("name", "dept_name"));
//! registry.register(dept_map);
//!
//! let mut user_map = ResultMap::new("userMap", "User");
//! user_map.add_id_mapping(Mapping::new("id", "user_id"));
//! user_map.add_result_mapping(Mapping::new("name", "user_name"));
//! user_map.add_association(NestedAssociation::new("dept", "deptMap"));
//! registry.register(user_map);
//!
//! // 2. 构造 JOIN 查询结果行
//! let mut columns = HashMap::new();
//! columns.insert("user_id".to_string(), Value::I64(1));
//! columns.insert("user_name".to_string(), Value::String("Alice".to_string()));
//! columns.insert("dept_id".to_string(), Value::I64(10));
//! columns.insert("dept_name".to_string(), Value::String("Engineering".to_string()));
//! let row = RowData::new(columns);
//!
//! // 3. 应用 ResultMap 映射
//! let result = apply_result_map(&registry, "userMap", &row).unwrap();
//! assert_eq!(result.get("id"), Some(&Value::I64(1)));
//! assert_eq!(result.get("name"), Some(&Value::String("Alice".to_string())));
//! ```

use crate::value::Value;
use std::collections::HashMap;
use std::sync::RwLock;

// ============================================================================
// Mapping — 单字段映射规则
// ============================================================================

/// 单字段映射规则（property <-> column）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// 目标属性名（Rust 字段名）
    pub property: String,
    /// 数据库列名
    pub column: String,
    /// 可选 TypeHandler 名称（用于自定义类型转换）
    pub type_handler: Option<String>,
}

impl Mapping {
    /// 创建字段映射
    pub fn new(property: impl Into<String>, column: impl Into<String>) -> Self {
        Self {
            property: property.into(),
            column: column.into(),
            type_handler: None,
        }
    }

    /// 创建带 TypeHandler 的字段映射
    pub fn with_handler(
        property: impl Into<String>,
        column: impl Into<String>,
        handler: impl Into<String>,
    ) -> Self {
        Self {
            property: property.into(),
            column: column.into(),
            type_handler: Some(handler.into()),
        }
    }
}

// ============================================================================
// NestedAssociation — 一对一嵌套映射
// ============================================================================

/// 一对一嵌套映射（association）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedAssociation {
    /// 目标属性名
    pub property: String,
    /// 引用的 ResultMap id
    pub result_map: String,
    /// 列前缀（用于 JOIN 场景隔离不同实体列）
    pub column_prefix: Option<String>,
    /// notNullColumn：仅当该列非 NULL 时才填充（避免 LEFT JOIN NULL 行被填充）
    pub not_null_column: Option<String>,
}

impl NestedAssociation {
    /// 创建嵌套 association
    pub fn new(property: impl Into<String>, result_map: impl Into<String>) -> Self {
        Self {
            property: property.into(),
            result_map: result_map.into(),
            column_prefix: None,
            not_null_column: None,
        }
    }

    /// 设置列前缀
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.column_prefix = Some(prefix.into());
        self
    }

    /// 设置 notNullColumn
    pub fn with_not_null_column(mut self, column: impl Into<String>) -> Self {
        self.not_null_column = Some(column.into());
        self
    }
}

// ============================================================================
// NestedCollection — 一对多嵌套映射
// ============================================================================

/// 一对多嵌套映射（collection）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedCollection {
    /// 目标属性名
    pub property: String,
    /// 引用的 ResultMap id
    pub result_map: String,
    /// 列前缀
    pub column_prefix: Option<String>,
    /// notNullColumn
    pub not_null_column: Option<String>,
}

impl NestedCollection {
    /// 创建嵌套 collection
    pub fn new(property: impl Into<String>, result_map: impl Into<String>) -> Self {
        Self {
            property: property.into(),
            result_map: result_map.into(),
            column_prefix: None,
            not_null_column: None,
        }
    }

    /// 设置列前缀
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.column_prefix = Some(prefix.into());
        self
    }

    /// 设置 notNullColumn
    pub fn with_not_null_column(mut self, column: impl Into<String>) -> Self {
        self.not_null_column = Some(column.into());
        self
    }
}

// ============================================================================
// Discriminator — 多态鉴别器
// ============================================================================

/// 多态鉴别器 case
#[derive(Debug, Clone, PartialEq)]
pub struct DiscriminatorCase {
    /// 触发值
    pub value: Value,
    /// 该 case 使用的 ResultMap id
    pub result_map: String,
}

impl DiscriminatorCase {
    /// 创建 case
    pub fn new(value: Value, result_map: impl Into<String>) -> Self {
        Self {
            value,
            result_map: result_map.into(),
        }
    }
}

/// 多态鉴别器
#[derive(Debug, Clone, PartialEq)]
pub struct Discriminator {
    /// 鉴别列
    pub column: String,
    /// case 列表
    pub cases: Vec<DiscriminatorCase>,
}

impl Discriminator {
    /// 创建鉴别器
    pub fn new(column: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            cases: Vec::new(),
        }
    }

    /// 添加 case
    pub fn add_case(&mut self, case: DiscriminatorCase) -> &mut Self {
        self.cases.push(case);
        self
    }

    /// 根据值查找对应 ResultMap id
    pub fn resolve(&self, value: &Value) -> Option<&str> {
        for case in &self.cases {
            if case.value == *value {
                return Some(&case.result_map);
            }
        }
        None
    }
}

// ============================================================================
// ResultMap — 完整结果映射规则
// ============================================================================

/// 完整结果映射规则
#[derive(Debug, Clone, PartialEq)]
pub struct ResultMap {
    /// 唯一 id
    pub id: String,
    /// 目标类型名（如 "User"）
    pub type_name: String,
    /// 主键字段映射（用于唯一性判断、collection 聚合）
    pub id_mappings: Vec<Mapping>,
    /// 普通字段映射
    pub result_mappings: Vec<Mapping>,
    /// 一对一嵌套映射
    pub associations: Vec<NestedAssociation>,
    /// 一对多嵌套映射
    pub collections: Vec<NestedCollection>,
    /// 多态鉴别器
    pub discriminator: Option<Discriminator>,
}

impl ResultMap {
    /// 创建 ResultMap
    pub fn new(id: impl Into<String>, type_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            type_name: type_name.into(),
            id_mappings: Vec::new(),
            result_mappings: Vec::new(),
            associations: Vec::new(),
            collections: Vec::new(),
            discriminator: None,
        }
    }

    /// 添加主键映射
    pub fn add_id_mapping(&mut self, mapping: Mapping) -> &mut Self {
        self.id_mappings.push(mapping);
        self
    }

    /// 添加普通字段映射
    pub fn add_result_mapping(&mut self, mapping: Mapping) -> &mut Self {
        self.result_mappings.push(mapping);
        self
    }

    /// 添加 association 嵌套
    pub fn add_association(&mut self, assoc: NestedAssociation) -> &mut Self {
        self.associations.push(assoc);
        self
    }

    /// 添加 collection 嵌套
    pub fn add_collection(&mut self, coll: NestedCollection) -> &mut Self {
        self.collections.push(coll);
        self
    }

    /// 设置 discriminator
    pub fn set_discriminator(&mut self, disc: Discriminator) -> &mut Self {
        self.discriminator = Some(disc);
        self
    }

    /// 收集本 ResultMap 直接引用的所有 sub-ResultMap id
    pub fn sub_map_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for a in &self.associations {
            ids.push(a.result_map.clone());
        }
        for c in &self.collections {
            ids.push(c.result_map.clone());
        }
        if let Some(d) = &self.discriminator {
            for case in &d.cases {
                ids.push(case.result_map.clone());
            }
        }
        ids
    }
}

// ============================================================================
// ResultMapRegistry — 注册中心
// ============================================================================

/// ResultMap 注册中心（线程安全）
#[derive(Debug, Default)]
pub struct ResultMapRegistry {
    maps: RwLock<HashMap<String, ResultMap>>,
}

impl ResultMapRegistry {
    /// 创建空注册中心
    pub fn new() -> Self {
        Self {
            maps: RwLock::new(HashMap::new()),
        }
    }

    /// 注册 ResultMap（同名覆盖）
    pub fn register(&self, map: ResultMap) {
        let mut maps = self.maps.write().unwrap();
        maps.insert(map.id.clone(), map);
    }

    /// 按 id 查找 ResultMap
    pub fn get(&self, id: &str) -> Option<ResultMap> {
        let maps = self.maps.read().unwrap();
        maps.get(id).cloned()
    }

    /// 是否包含指定 id
    pub fn contains(&self, id: &str) -> bool {
        let maps = self.maps.read().unwrap();
        maps.contains_key(id)
    }

    /// 已注册的 ResultMap 数量
    pub fn len(&self) -> usize {
        let maps = self.maps.read().unwrap();
        maps.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 列出所有已注册的 id
    pub fn list_ids(&self) -> Vec<String> {
        let maps = self.maps.read().unwrap();
        maps.keys().cloned().collect()
    }

    /// 清空注册中心
    pub fn clear(&self) {
        let mut maps = self.maps.write().unwrap();
        maps.clear();
    }
}

// ============================================================================
// RowData — 行数据
// ============================================================================

/// 行数据（列名 -> Value）
#[derive(Debug, Clone, Default)]
pub struct RowData {
    columns: HashMap<String, Value>,
}

impl RowData {
    /// 创建空行
    pub fn new(columns: HashMap<String, Value>) -> Self {
        Self { columns }
    }

    /// 创建空行
    pub fn empty() -> Self {
        Self {
            columns: HashMap::new(),
        }
    }

    /// 插入/更新列
    pub fn set(&mut self, column: impl Into<String>, value: Value) {
        self.columns.insert(column.into(), value);
    }

    /// 按列名取值
    pub fn get(&self, column: &str) -> Option<&Value> {
        self.columns.get(column)
    }

    /// 按前缀 + 列名取值（用于 JOIN 列前缀隔离）
    ///
    /// 例如：prefix="dept_", column="id" 将查找 "dept_id"
    pub fn get_with_prefix(&self, prefix: &str, column: &str) -> Option<&Value> {
        let full = format!("{}{}", prefix, column);
        self.columns.get(&full)
    }

    /// 判断列是否存在且非 NULL
    pub fn is_not_null(&self, column: &str) -> bool {
        match self.columns.get(column) {
            Some(Value::Null) | None => false,
            Some(_) => true,
        }
    }

    /// 列数
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// 所有列名
    pub fn column_names(&self) -> Vec<String> {
        self.columns.keys().cloned().collect()
    }

    /// 获取所有列的引用（按列名排序）
    pub fn sorted_columns(&self) -> Vec<(&String, &Value)> {
        let mut entries: Vec<(&String, &Value)> = self.columns.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries
    }

    /// 获取所有列的迭代器
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.columns.iter()
    }
}

// ============================================================================
// ResultMapError — 错误类型
// ============================================================================

/// ResultMap 错误
#[derive(Debug, Clone, PartialEq)]
pub enum ResultMapError {
    /// ResultMap 未注册
    MapNotFound { id: String },
    /// 必需列缺失
    RequiredColumnMissing { column: String },
    /// 嵌套映射失败
    NestedMappingFailed { property: String, reason: String },
}

impl std::fmt::Display for ResultMapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResultMapError::MapNotFound { id } => {
                write!(f, "ResultMap '{}' not registered", id)
            }
            ResultMapError::RequiredColumnMissing { column } => {
                write!(f, "Required column '{}' missing in row", column)
            }
            ResultMapError::NestedMappingFailed { property, reason } => {
                write!(f, "Nested mapping failed for '{}': {}", property, reason)
            }
        }
    }
}

impl std::error::Error for ResultMapError {}

// ============================================================================
// 映射函数
// ============================================================================

/// 应用 ResultMap 规则到单行，返回属性 HashMap
///
/// # 行为
///
/// 1. 检查 discriminator，若命中 case 则改用 case 指定的 ResultMap
/// 2. 应用 id_mappings 和 result_mappings，将列值填入属性
/// 3. 递归处理 associations（一对一）
/// 4. 单行模式下 collections 仅返回当前行解析出的单个子实体（多次行合并需用 `apply_result_map_many`）
pub fn apply_result_map(
    registry: &ResultMapRegistry,
    map_id: &str,
    row: &RowData,
) -> Result<HashMap<String, Value>, ResultMapError> {
    let map = registry
        .get(map_id)
        .ok_or_else(|| ResultMapError::MapNotFound {
            id: map_id.to_string(),
        })?;

    // 1. discriminator 多态分派
    let effective_map = if let Some(disc) = &map.discriminator {
        if let Some(disc_value) = row.get(&disc.column) {
            if let Some(case_map_id) = disc.resolve(disc_value) {
                registry.get(case_map_id).unwrap_or(map)
            } else {
                map
            }
        } else {
            map
        }
    } else {
        map
    };

    let mut attrs: HashMap<String, Value> = HashMap::new();

    // 2. id + result 映射
    for m in &effective_map.id_mappings {
        if let Some(v) = row.get(&m.column) {
            attrs.insert(m.property.clone(), v.clone());
        }
    }
    for m in &effective_map.result_mappings {
        if let Some(v) = row.get(&m.column) {
            attrs.insert(m.property.clone(), v.clone());
        }
    }

    // 3. associations（一对一，递归）
    for assoc in &effective_map.associations {
        // notNullColumn 检查
        if let Some(not_null_col) = &assoc.not_null_column {
            if !row.is_not_null(not_null_col) {
                continue; // 跳过，不填充该 association
            }
        }

        let nested = apply_result_map(registry, &assoc.result_map, row).map_err(|e| {
            ResultMapError::NestedMappingFailed {
                property: assoc.property.clone(),
                reason: e.to_string(),
            }
        })?;

        // column_prefix 处理：重新映射列
        let nested_value = if let Some(_prefix) = &assoc.column_prefix {
            // prefix 模式下，apply_result_map 已经使用原始列名
            // 这里需要构造带前缀的 RowData 再调用
            let mut prefixed_row = RowData::empty();
            for (col, v) in &row.columns {
                if let Some(stripped) = col.strip_prefix(_prefix) {
                    prefixed_row.set(stripped.to_string(), v.clone());
                }
            }
            apply_result_map(registry, &assoc.result_map, &prefixed_row).map_err(|e| {
                ResultMapError::NestedMappingFailed {
                    property: assoc.property.clone(),
                    reason: e.to_string(),
                }
            })?
        } else {
            nested
        };

        // 将嵌套 HashMap 转为 Value::Object 存储
        attrs.insert(assoc.property.clone(), Value::Object(nested_value));
    }

    // 4. collections（一对多，单行模式下仅返回当前行解析的单个元素）
    for coll in &effective_map.collections {
        if let Some(not_null_col) = &coll.not_null_column {
            if !row.is_not_null(not_null_col) {
                continue;
            }
        }

        let nested = if let Some(prefix) = &coll.column_prefix {
            let mut prefixed_row = RowData::empty();
            for (col, v) in &row.columns {
                if let Some(stripped) = col.strip_prefix(prefix) {
                    prefixed_row.set(stripped.to_string(), v.clone());
                }
            }
            apply_result_map(registry, &coll.result_map, &prefixed_row).map_err(|e| {
                ResultMapError::NestedMappingFailed {
                    property: coll.property.clone(),
                    reason: e.to_string(),
                }
            })?
        } else {
            apply_result_map(registry, &coll.result_map, row).map_err(|e| {
                ResultMapError::NestedMappingFailed {
                    property: coll.property.clone(),
                    reason: e.to_string(),
                }
            })?
        };

        // collection 在单行模式下以单元素 Array 形式返回
        // 完整合并需调用 apply_result_map_many
        attrs.insert(
            coll.property.clone(),
            Value::Array(vec![Value::Object(nested)]),
        );
    }

    Ok(attrs)
}

/// 应用 ResultMap 到多行，处理 collection 聚合
///
/// # 行为
///
/// 1. 按主键（id_mappings 的属性值）分组：同一主键的多行合并为一个实体
/// 2. associations 取第一行解析结果
/// 3. collections 跨行聚合：每行解析出的子实体追加到数组
pub fn apply_result_map_many(
    registry: &ResultMapRegistry,
    map_id: &str,
    rows: &[RowData],
) -> Result<Vec<HashMap<String, Value>>, ResultMapError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let map = registry
        .get(map_id)
        .ok_or_else(|| ResultMapError::MapNotFound {
            id: map_id.to_string(),
        })?;

    // 用主键属性值的字符串形式作为分组的 key
    fn pk_key(attrs: &HashMap<String, Value>, id_mappings: &[Mapping]) -> String {
        if id_mappings.is_empty() {
            // 无主键映射时，按行号分组（每行独立）
            // 这里返回空字符串，调用方需另行处理
            return String::new();
        }
        let mut parts = Vec::new();
        for m in id_mappings {
            if let Some(v) = attrs.get(&m.property) {
                parts.push(format!("{:?}", v));
            } else {
                parts.push("null".to_string());
            }
        }
        parts.join("|")
    }

    // 保持插入顺序
    let mut ordered_keys: Vec<String> = Vec::new();
    let mut groups: HashMap<String, HashMap<String, Value>> = HashMap::new();
    let mut collection_acc: HashMap<String, HashMap<String, Vec<Value>>> = HashMap::new();

    for row in rows {
        let attrs = apply_result_map(registry, map_id, row)?;
        let key = pk_key(&attrs, &map.id_mappings);

        if !groups.contains_key(&key) {
            ordered_keys.push(key.clone());
            groups.insert(key.clone(), attrs.clone());
            collection_acc.insert(key.clone(), HashMap::new());
        }

        // 聚合 collections
        for coll in &map.collections {
            if let Some(Value::Array(items)) = attrs.get(&coll.property) {
                if !items.is_empty() {
                    let acc = collection_acc.get_mut(&key).unwrap();
                    let entry = acc.entry(coll.property.clone()).or_default();
                    for item in items {
                        entry.push(item.clone());
                    }
                }
            }
        }
    }

    // 合并 collection 聚合结果到主属性
    let mut result = Vec::new();
    for key in ordered_keys {
        let mut attrs = groups.remove(&key).unwrap();
        if let Some(coll_acc) = collection_acc.remove(&key) {
            for (prop, items) in coll_acc {
                attrs.insert(prop, Value::Array(items));
            }
        }
        result.push(attrs);
    }

    Ok(result)
}

// ============================================================================
// NativeQuery + ResultSetMapping
// ============================================================================

/// 标量结果列
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarResult {
    /// 列名
    pub column: String,
    /// 类型名（如 "i64"、"string"）
    pub type_name: String,
}

impl ScalarResult {
    pub fn new(column: impl Into<String>, type_name: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            type_name: type_name.into(),
        }
    }
}

/// 实体字段结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldResult {
    pub name: String,
    pub column: String,
}

impl FieldResult {
    pub fn new(name: impl Into<String>, column: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            column: column.into(),
        }
    }
}

/// Entity 结果（用于 ResultSetMapping）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityResult {
    pub entity_class: String,
    pub fields: Vec<FieldResult>,
    pub discriminator_column: Option<String>,
}

impl EntityResult {
    pub fn new(entity_class: impl Into<String>) -> Self {
        Self {
            entity_class: entity_class.into(),
            fields: Vec::new(),
            discriminator_column: None,
        }
    }

    pub fn add_field(&mut self, field: FieldResult) -> &mut Self {
        self.fields.push(field);
        self
    }

    pub fn with_discriminator_column(mut self, col: impl Into<String>) -> Self {
        self.discriminator_column = Some(col.into());
        self
    }
}

/// Hibernate `@SqlResultSetMapping` 风格的结果集映射
#[derive(Debug, Clone, PartialEq)]
pub struct ResultSetMapping {
    pub name: String,
    pub entities: Vec<EntityResult>,
    pub scalars: Vec<ScalarResult>,
}

impl ResultSetMapping {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entities: Vec::new(),
            scalars: Vec::new(),
        }
    }

    pub fn add_entity(&mut self, entity: EntityResult) -> &mut Self {
        self.entities.push(entity);
        self
    }

    pub fn add_scalar(&mut self, scalar: ScalarResult) -> &mut Self {
        self.scalars.push(scalar);
        self
    }
}

/// ResultSetMapping 注册中心
#[derive(Debug, Default)]
pub struct ResultSetMappingRegistry {
    mappings: RwLock<HashMap<String, ResultSetMapping>>,
}

impl ResultSetMappingRegistry {
    pub fn new() -> Self {
        Self {
            mappings: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, mapping: ResultSetMapping) {
        let mut m = self.mappings.write().unwrap();
        m.insert(mapping.name.clone(), mapping);
    }

    pub fn get(&self, name: &str) -> Option<ResultSetMapping> {
        let m = self.mappings.read().unwrap();
        m.get(name).cloned()
    }

    pub fn contains(&self, name: &str) -> bool {
        let m = self.mappings.read().unwrap();
        m.contains_key(name)
    }

    pub fn len(&self) -> usize {
        let m = self.mappings.read().unwrap();
        m.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// NativeQuery — 原生 SQL + ResultSetMapping
#[derive(Debug, Clone)]
pub struct NativeQuery {
    /// SQL 语句（可含 `?` 占位符）
    pub sql: String,
    /// ResultSetMapping 名称
    pub result_set_mapping: String,
    /// 绑定参数
    pub parameters: Vec<Value>,
}

impl NativeQuery {
    pub fn new(sql: impl Into<String>, mapping_name: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            result_set_mapping: mapping_name.into(),
            parameters: Vec::new(),
        }
    }

    pub fn bind(&mut self, value: Value) -> &mut Self {
        self.parameters.push(value);
        self
    }

    pub fn bind_many(&mut self, values: Vec<Value>) -> &mut Self {
        self.parameters.extend(values);
        self
    }
}

/// 应用 ResultSetMapping 到单行，返回 (entity_attrs_vec, scalar_values_vec)
///
/// 返回：
/// - 第一个 Vec：每个 EntityResult 对应一个 HashMap<String, Value>
/// - 第二个 Vec：每个 ScalarResult 对应一个 Value
pub fn apply_result_set_mapping(
    mapping: &ResultSetMapping,
    row: &RowData,
) -> ResultSetMappingResult {
    let mut entities = Vec::new();
    for ent in &mapping.entities {
        let mut attrs = HashMap::new();
        for f in &ent.fields {
            if let Some(v) = row.get(&f.column) {
                attrs.insert(f.name.clone(), v.clone());
            }
        }
        entities.push(attrs);
    }

    let mut scalars = Vec::new();
    for s in &mapping.scalars {
        if let Some(v) = row.get(&s.column) {
            scalars.push(v.clone());
        } else {
            scalars.push(Value::Null);
        }
    }

    (entities, scalars)
}

/// ResultSetMapping 应用结果类型（entities + scalars）
pub type ResultSetMappingResult = (Vec<HashMap<String, Value>>, Vec<Value>);

/// 应用 ResultSetMapping 到多行
pub fn apply_result_set_mapping_many(
    mapping: &ResultSetMapping,
    rows: &[RowData],
) -> Vec<ResultSetMappingResult> {
    rows.iter()
        .map(|row| apply_result_set_mapping(mapping, row))
        .collect()
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Mapping =====

    #[test]
    fn test_mapping_new() {
        let m = Mapping::new("id", "user_id");
        assert_eq!(m.property, "id");
        assert_eq!(m.column, "user_id");
        assert_eq!(m.type_handler, None);
    }

    #[test]
    fn test_mapping_with_handler() {
        let m = Mapping::with_handler("amount", "amount", "money_handler");
        assert_eq!(m.property, "amount");
        assert_eq!(m.column, "amount");
        assert_eq!(m.type_handler.as_deref(), Some("money_handler"));
    }

    // ===== NestedAssociation =====

    #[test]
    fn test_association_new() {
        let a = NestedAssociation::new("dept", "deptMap");
        assert_eq!(a.property, "dept");
        assert_eq!(a.result_map, "deptMap");
        assert_eq!(a.column_prefix, None);
        assert_eq!(a.not_null_column, None);
    }

    #[test]
    fn test_association_with_prefix() {
        let a = NestedAssociation::new("dept", "deptMap").with_prefix("d_");
        assert_eq!(a.column_prefix.as_deref(), Some("d_"));
    }

    #[test]
    fn test_association_with_not_null_column() {
        let a = NestedAssociation::new("dept", "deptMap").with_not_null_column("dept_id");
        assert_eq!(a.not_null_column.as_deref(), Some("dept_id"));
    }

    // ===== NestedCollection =====

    #[test]
    fn test_collection_new() {
        let c = NestedCollection::new("roles", "roleMap");
        assert_eq!(c.property, "roles");
        assert_eq!(c.result_map, "roleMap");
        assert_eq!(c.column_prefix, None);
    }

    #[test]
    fn test_collection_with_prefix() {
        let c = NestedCollection::new("roles", "roleMap").with_prefix("r_");
        assert_eq!(c.column_prefix.as_deref(), Some("r_"));
    }

    // ===== Discriminator =====

    #[test]
    fn test_discriminator_new() {
        let d = Discriminator::new("user_type");
        assert_eq!(d.column, "user_type");
        assert!(d.cases.is_empty());
    }

    #[test]
    fn test_discriminator_add_case() {
        let mut d = Discriminator::new("user_type");
        d.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"))
            .add_case(DiscriminatorCase::new(Value::I64(2), "userMap"));
        assert_eq!(d.cases.len(), 2);
    }

    #[test]
    fn test_discriminator_resolve_hit() {
        let mut d = Discriminator::new("user_type");
        d.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"))
            .add_case(DiscriminatorCase::new(Value::I64(2), "userMap"));

        assert_eq!(d.resolve(&Value::I64(1)), Some("adminMap"));
        assert_eq!(d.resolve(&Value::I64(2)), Some("userMap"));
    }

    #[test]
    fn test_discriminator_resolve_miss() {
        let mut d = Discriminator::new("user_type");
        d.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"));

        assert_eq!(d.resolve(&Value::I64(99)), None);
    }

    // ===== ResultMap =====

    #[test]
    fn test_result_map_new() {
        let rm = ResultMap::new("userMap", "User");
        assert_eq!(rm.id, "userMap");
        assert_eq!(rm.type_name, "User");
        assert!(rm.id_mappings.is_empty());
        assert!(rm.result_mappings.is_empty());
        assert!(rm.associations.is_empty());
        assert!(rm.collections.is_empty());
        assert!(rm.discriminator.is_none());
    }

    #[test]
    fn test_result_map_add_mappings() {
        let mut rm = ResultMap::new("userMap", "User");
        rm.add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"))
            .add_association(NestedAssociation::new("dept", "deptMap"))
            .add_collection(NestedCollection::new("roles", "roleMap"));

        assert_eq!(rm.id_mappings.len(), 1);
        assert_eq!(rm.result_mappings.len(), 1);
        assert_eq!(rm.associations.len(), 1);
        assert_eq!(rm.collections.len(), 1);
    }

    #[test]
    fn test_result_map_set_discriminator() {
        let mut rm = ResultMap::new("userMap", "User");
        rm.set_discriminator(Discriminator::new("user_type"));
        assert!(rm.discriminator.is_some());
        assert_eq!(rm.discriminator.as_ref().unwrap().column, "user_type");
    }

    #[test]
    fn test_sub_map_ids() {
        let mut rm = ResultMap::new("userMap", "User");
        rm.add_association(NestedAssociation::new("dept", "deptMap"))
            .add_collection(NestedCollection::new("roles", "roleMap"))
            .set_discriminator({
                let mut d = Discriminator::new("type");
                d.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"));
                d
            });

        let ids = rm.sub_map_ids();
        assert!(ids.contains(&"deptMap".to_string()));
        assert!(ids.contains(&"roleMap".to_string()));
        assert!(ids.contains(&"adminMap".to_string()));
    }

    // ===== ResultMapRegistry =====

    #[test]
    fn test_registry_register_and_get() {
        let registry = ResultMapRegistry::new();
        let rm = ResultMap::new("userMap", "User");
        registry.register(rm);

        assert!(registry.contains("userMap"));
        assert!(!registry.contains("missing"));
        assert_eq!(registry.len(), 1);
        assert!(registry.get("userMap").is_some());
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn test_registry_list_ids() {
        let registry = ResultMapRegistry::new();
        registry.register(ResultMap::new("userMap", "User"));
        registry.register(ResultMap::new("deptMap", "Dept"));

        let ids = registry.list_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"userMap".to_string()));
        assert!(ids.contains(&"deptMap".to_string()));
    }

    #[test]
    fn test_registry_clear() {
        let registry = ResultMapRegistry::new();
        registry.register(ResultMap::new("userMap", "User"));
        assert_eq!(registry.len(), 1);
        registry.clear();
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_overwrite() {
        let registry = ResultMapRegistry::new();
        registry.register(ResultMap::new("userMap", "User"));
        registry.register(ResultMap::new("userMap", "AdminUser"));

        let rm = registry.get("userMap").unwrap();
        assert_eq!(rm.type_name, "AdminUser");
    }

    // ===== RowData =====

    #[test]
    fn test_row_data_new() {
        let mut cols = HashMap::new();
        cols.insert("id".to_string(), Value::I64(1));
        let row = RowData::new(cols);

        assert_eq!(row.get("id"), Some(&Value::I64(1)));
        assert_eq!(row.get("missing"), None);
        assert_eq!(row.len(), 1);
    }

    #[test]
    fn test_row_data_set_and_get() {
        let mut row = RowData::empty();
        row.set("name", Value::String("Alice".to_string()));

        assert_eq!(row.get("name"), Some(&Value::String("Alice".to_string())));
    }

    #[test]
    fn test_row_data_get_with_prefix() {
        let mut row = RowData::empty();
        row.set("dept_id", Value::I64(10));
        row.set("dept_name", Value::String("Engineering".to_string()));

        assert_eq!(row.get_with_prefix("dept_", "id"), Some(&Value::I64(10)));
        assert_eq!(
            row.get_with_prefix("dept_", "name"),
            Some(&Value::String("Engineering".to_string()))
        );
        assert_eq!(row.get_with_prefix("dept_", "missing"), None);
    }

    #[test]
    fn test_row_data_is_not_null() {
        let mut row = RowData::empty();
        row.set("a", Value::I64(1));
        row.set("b", Value::Null);

        assert!(row.is_not_null("a"));
        assert!(!row.is_not_null("b"));
        assert!(!row.is_not_null("missing"));
    }

    #[test]
    fn test_row_data_column_names() {
        let mut row = RowData::empty();
        row.set("id", Value::I64(1));
        row.set("name", Value::String("Alice".to_string()));

        let names = row.column_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"id".to_string()));
        assert!(names.contains(&"name".to_string()));
    }

    // ===== apply_result_map 基础 =====

    #[test]
    fn test_apply_result_map_basic() {
        let registry = ResultMapRegistry::new();
        let mut rm = ResultMap::new("userMap", "User");
        rm.add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"));
        registry.register(rm);

        let mut row = RowData::empty();
        row.set("user_id", Value::I64(1));
        row.set("user_name", Value::String("Alice".to_string()));

        let attrs = apply_result_map(&registry, "userMap", &row).unwrap();
        assert_eq!(attrs.get("id"), Some(&Value::I64(1)));
        assert_eq!(attrs.get("name"), Some(&Value::String("Alice".to_string())));
    }

    #[test]
    fn test_apply_result_map_missing_column() {
        let registry = ResultMapRegistry::new();
        let mut rm = ResultMap::new("userMap", "User");
        rm.add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"));
        registry.register(rm);

        let row = RowData::empty();
        let attrs = apply_result_map(&registry, "userMap", &row).unwrap();
        // 缺失列不报错，对应属性不出现
        assert!(!attrs.contains_key("id"));
        assert!(!attrs.contains_key("name"));
    }

    #[test]
    fn test_apply_result_map_not_found() {
        let registry = ResultMapRegistry::new();
        let row = RowData::empty();
        let err = apply_result_map(&registry, "missingMap", &row).unwrap_err();
        match err {
            ResultMapError::MapNotFound { id } => assert_eq!(id, "missingMap"),
            _ => panic!("expected MapNotFound"),
        }
    }

    // ===== apply_result_map association =====

    #[test]
    fn test_apply_result_map_with_association() {
        let registry = ResultMapRegistry::new();

        let mut dept_map = ResultMap::new("deptMap", "Dept");
        dept_map
            .add_id_mapping(Mapping::new("id", "dept_id"))
            .add_result_mapping(Mapping::new("name", "dept_name"));
        registry.register(dept_map);

        let mut user_map = ResultMap::new("userMap", "User");
        user_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"))
            .add_association(NestedAssociation::new("dept", "deptMap"));
        registry.register(user_map);

        let mut row = RowData::empty();
        row.set("user_id", Value::I64(1));
        row.set("user_name", Value::String("Alice".to_string()));
        row.set("dept_id", Value::I64(10));
        row.set("dept_name", Value::String("Engineering".to_string()));

        let attrs = apply_result_map(&registry, "userMap", &row).unwrap();
        assert_eq!(attrs.get("id"), Some(&Value::I64(1)));
        let dept = attrs.get("dept");
        assert!(dept.is_some());
        if let Some(Value::Object(dept_attrs)) = dept {
            assert_eq!(dept_attrs.get("id"), Some(&Value::I64(10)));
            assert_eq!(
                dept_attrs.get("name"),
                Some(&Value::String("Engineering".to_string()))
            );
        }
    }

    #[test]
    fn test_apply_result_map_association_not_null_column_skip() {
        let registry = ResultMapRegistry::new();

        let mut dept_map = ResultMap::new("deptMap", "Dept");
        dept_map
            .add_id_mapping(Mapping::new("id", "dept_id"))
            .add_result_mapping(Mapping::new("name", "dept_name"));
        registry.register(dept_map);

        let mut user_map = ResultMap::new("userMap", "User");
        user_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"))
            .add_association(
                NestedAssociation::new("dept", "deptMap").with_not_null_column("dept_id"),
            );
        registry.register(user_map);

        // dept_id 为 NULL（LEFT JOIN 缺失行）
        let mut row = RowData::empty();
        row.set("user_id", Value::I64(1));
        row.set("user_name", Value::String("Alice".to_string()));
        row.set("dept_id", Value::Null);

        let attrs = apply_result_map(&registry, "userMap", &row).unwrap();
        // dept 应该被跳过（不出现）
        assert!(!attrs.contains_key("dept"));
    }

    #[test]
    fn test_apply_result_map_association_with_prefix() {
        let registry = ResultMapRegistry::new();

        let mut dept_map = ResultMap::new("deptMap", "Dept");
        dept_map
            .add_id_mapping(Mapping::new("id", "id"))
            .add_result_mapping(Mapping::new("name", "name"));
        registry.register(dept_map);

        let mut user_map = ResultMap::new("userMap", "User");
        user_map
            .add_id_mapping(Mapping::new("id", "id"))
            .add_result_mapping(Mapping::new("name", "name"))
            .add_association(NestedAssociation::new("dept", "deptMap").with_prefix("d_"));
        registry.register(user_map);

        // 列名带 d_ 前缀
        let mut row = RowData::empty();
        row.set("id", Value::I64(1));
        row.set("name", Value::String("Alice".to_string()));
        row.set("d_id", Value::I64(10));
        row.set("d_name", Value::String("Engineering".to_string()));

        let attrs = apply_result_map(&registry, "userMap", &row).unwrap();
        assert_eq!(attrs.get("id"), Some(&Value::I64(1)));
        assert_eq!(attrs.get("name"), Some(&Value::String("Alice".to_string())));

        if let Some(Value::Object(dept_attrs)) = attrs.get("dept") {
            assert_eq!(dept_attrs.get("id"), Some(&Value::I64(10)));
            assert_eq!(
                dept_attrs.get("name"),
                Some(&Value::String("Engineering".to_string()))
            );
        } else {
            panic!("dept should be an Object");
        }
    }

    // ===== apply_result_map discriminator =====

    #[test]
    fn test_apply_result_map_discriminator() {
        let registry = ResultMapRegistry::new();

        // adminMap
        let mut admin_map = ResultMap::new("adminMap", "AdminUser");
        admin_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"))
            .add_result_mapping(Mapping::new("admin_level", "extra_level"));
        registry.register(admin_map);

        // normalMap
        let mut normal_map = ResultMap::new("normalMap", "NormalUser");
        normal_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"));
        registry.register(normal_map);

        // baseMap with discriminator
        let mut base_map = ResultMap::new("baseMap", "User");
        base_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"))
            .set_discriminator({
                let mut d = Discriminator::new("user_type");
                d.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"));
                d.add_case(DiscriminatorCase::new(Value::I64(2), "normalMap"));
                d
            });
        registry.register(base_map);

        // user_type=1 → adminMap
        let mut row = RowData::empty();
        row.set("user_id", Value::I64(1));
        row.set("user_name", Value::String("Alice".to_string()));
        row.set("user_type", Value::I64(1));
        row.set("extra_level", Value::I64(5));

        let attrs = apply_result_map(&registry, "baseMap", &row).unwrap();
        assert_eq!(attrs.get("id"), Some(&Value::I64(1)));
        assert_eq!(attrs.get("admin_level"), Some(&Value::I64(5)));

        // user_type=2 → normalMap
        let mut row2 = RowData::empty();
        row2.set("user_id", Value::I64(2));
        row2.set("user_name", Value::String("Bob".to_string()));
        row2.set("user_type", Value::I64(2));

        let attrs2 = apply_result_map(&registry, "baseMap", &row2).unwrap();
        assert_eq!(attrs2.get("id"), Some(&Value::I64(2)));
        assert!(!attrs2.contains_key("admin_level")); // normalMap 没有 admin_level
    }

    #[test]
    fn test_apply_result_map_discriminator_no_match_falls_back_to_base() {
        let registry = ResultMapRegistry::new();

        let mut base_map = ResultMap::new("baseMap", "User");
        base_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .set_discriminator({
                let mut d = Discriminator::new("user_type");
                d.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"));
                d
            });
        registry.register(base_map);

        // user_type=99 → 无匹配，使用 baseMap
        let mut row = RowData::empty();
        row.set("user_id", Value::I64(1));
        row.set("user_type", Value::I64(99));

        let attrs = apply_result_map(&registry, "baseMap", &row).unwrap();
        assert_eq!(attrs.get("id"), Some(&Value::I64(1)));
    }

    // ===== apply_result_map_many collection 聚合 =====

    #[test]
    fn test_apply_result_map_many_collection_aggregation() {
        let registry = ResultMapRegistry::new();

        let mut role_map = ResultMap::new("roleMap", "Role");
        role_map
            .add_id_mapping(Mapping::new("id", "role_id"))
            .add_result_mapping(Mapping::new("name", "role_name"));
        registry.register(role_map);

        let mut user_map = ResultMap::new("userMap", "User");
        user_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"))
            .add_collection(NestedCollection::new("roles", "roleMap"));
        registry.register(user_map);

        // 用户 1 有 2 个角色
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(1));
                r.set("user_name", Value::String("Alice".to_string()));
                r.set("role_id", Value::I64(100));
                r.set("role_name", Value::String("admin".to_string()));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(1));
                r.set("user_name", Value::String("Alice".to_string()));
                r.set("role_id", Value::I64(101));
                r.set("role_name", Value::String("editor".to_string()));
                r
            },
        ];

        let result = apply_result_map_many(&registry, "userMap", &rows).unwrap();
        assert_eq!(result.len(), 1); // 合并为 1 个用户
        let user = &result[0];
        assert_eq!(user.get("id"), Some(&Value::I64(1)));
        let roles = user.get("roles");
        assert!(roles.is_some());
        if let Some(Value::Array(items)) = roles {
            assert_eq!(items.len(), 2);
        }
    }

    #[test]
    fn test_apply_result_map_many_multi_users() {
        let registry = ResultMapRegistry::new();

        let mut role_map = ResultMap::new("roleMap", "Role");
        role_map
            .add_id_mapping(Mapping::new("id", "role_id"))
            .add_result_mapping(Mapping::new("name", "role_name"));
        registry.register(role_map);

        let mut user_map = ResultMap::new("userMap", "User");
        user_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"))
            .add_collection(NestedCollection::new("roles", "roleMap"));
        registry.register(user_map);

        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(1));
                r.set("user_name", Value::String("Alice".to_string()));
                r.set("role_id", Value::I64(100));
                r.set("role_name", Value::String("admin".to_string()));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(2));
                r.set("user_name", Value::String("Bob".to_string()));
                r.set("role_id", Value::I64(101));
                r.set("role_name", Value::String("editor".to_string()));
                r
            },
        ];

        let result = apply_result_map_many(&registry, "userMap", &rows).unwrap();
        assert_eq!(result.len(), 2);
        // 保持插入顺序
        assert_eq!(result[0].get("id"), Some(&Value::I64(1)));
        assert_eq!(result[1].get("id"), Some(&Value::I64(2)));
    }

    #[test]
    fn test_apply_result_map_many_empty() {
        let registry = ResultMapRegistry::new();
        registry.register(ResultMap::new("userMap", "User"));

        let result = apply_result_map_many(&registry, "userMap", &[]).unwrap();
        assert!(result.is_empty());
    }

    // ===== ResultSetMapping =====

    #[test]
    fn test_entity_result_new() {
        let er = EntityResult::new("User");
        assert_eq!(er.entity_class, "User");
        assert!(er.fields.is_empty());
        assert_eq!(er.discriminator_column, None);
    }

    #[test]
    fn test_entity_result_add_field() {
        let mut er = EntityResult::new("User");
        er.add_field(FieldResult::new("id", "user_id"))
            .add_field(FieldResult::new("name", "user_name"));
        assert_eq!(er.fields.len(), 2);
    }

    #[test]
    fn test_entity_result_with_discriminator() {
        let er = EntityResult::new("User").with_discriminator_column("user_type");
        assert_eq!(er.discriminator_column.as_deref(), Some("user_type"));
    }

    #[test]
    fn test_scalar_result_new() {
        let s = ScalarResult::new("count", "i64");
        assert_eq!(s.column, "count");
        assert_eq!(s.type_name, "i64");
    }

    #[test]
    fn test_result_set_mapping_new() {
        let rsm = ResultSetMapping::new("userCount");
        assert_eq!(rsm.name, "userCount");
        assert!(rsm.entities.is_empty());
        assert!(rsm.scalars.is_empty());
    }

    #[test]
    fn test_result_set_mapping_add() {
        let mut rsm = ResultSetMapping::new("userWithCount");
        rsm.add_entity(EntityResult::new("User"))
            .add_scalar(ScalarResult::new("total", "i64"));
        assert_eq!(rsm.entities.len(), 1);
        assert_eq!(rsm.scalars.len(), 1);
    }

    // ===== ResultSetMappingRegistry =====

    #[test]
    fn test_rsm_registry() {
        let reg = ResultSetMappingRegistry::new();
        reg.register(ResultSetMapping::new("mapping1"));
        assert!(reg.contains("mapping1"));
        assert!(!reg.contains("missing"));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("mapping1").is_some());
        assert!(reg.get("missing").is_none());
    }

    // ===== NativeQuery =====

    #[test]
    fn test_native_query_new() {
        let nq = NativeQuery::new("SELECT * FROM users WHERE id = ?", "userMapping");
        assert_eq!(nq.sql, "SELECT * FROM users WHERE id = ?");
        assert_eq!(nq.result_set_mapping, "userMapping");
        assert!(nq.parameters.is_empty());
    }

    #[test]
    fn test_native_query_bind() {
        let mut nq = NativeQuery::new("SELECT * FROM users WHERE id = ?", "userMapping");
        nq.bind(Value::I64(1));
        assert_eq!(nq.parameters.len(), 1);
        assert_eq!(nq.parameters[0], Value::I64(1));
    }

    #[test]
    fn test_native_query_bind_many() {
        let mut nq = NativeQuery::new("SELECT * FROM users WHERE id IN (?, ?)", "userMapping");
        nq.bind_many(vec![Value::I64(1), Value::I64(2)]);
        assert_eq!(nq.parameters.len(), 2);
    }

    // ===== apply_result_set_mapping =====

    #[test]
    fn test_apply_result_set_mapping_entities_only() {
        let mut rsm = ResultSetMapping::new("userMapping");
        let mut er = EntityResult::new("User");
        er.add_field(FieldResult::new("id", "user_id"))
            .add_field(FieldResult::new("name", "user_name"));
        rsm.add_entity(er);

        let mut row = RowData::empty();
        row.set("user_id", Value::I64(1));
        row.set("user_name", Value::String("Alice".to_string()));

        let (entities, scalars) = apply_result_set_mapping(&rsm, &row);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].get("id"), Some(&Value::I64(1)));
        assert_eq!(
            entities[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
        assert!(scalars.is_empty());
    }

    #[test]
    fn test_apply_result_set_mapping_scalars_only() {
        let mut rsm = ResultSetMapping::new("countMapping");
        rsm.add_scalar(ScalarResult::new("total", "i64"))
            .add_scalar(ScalarResult::new("avg_age", "f64"));

        let mut row = RowData::empty();
        row.set("total", Value::I64(100));
        row.set("avg_age", Value::F64(25.5));

        let (entities, scalars) = apply_result_set_mapping(&rsm, &row);
        assert!(entities.is_empty());
        assert_eq!(scalars.len(), 2);
        assert_eq!(scalars[0], Value::I64(100));
        assert_eq!(scalars[1], Value::F64(25.5));
    }

    #[test]
    fn test_apply_result_set_mapping_mixed() {
        let mut rsm = ResultSetMapping::new("userWithCount");
        let mut er = EntityResult::new("User");
        er.add_field(FieldResult::new("id", "user_id"))
            .add_field(FieldResult::new("name", "user_name"));
        rsm.add_entity(er);
        rsm.add_scalar(ScalarResult::new("total_orders", "i64"));

        let mut row = RowData::empty();
        row.set("user_id", Value::I64(1));
        row.set("user_name", Value::String("Alice".to_string()));
        row.set("total_orders", Value::I64(42));

        let (entities, scalars) = apply_result_set_mapping(&rsm, &row);
        assert_eq!(entities.len(), 1);
        assert_eq!(scalars.len(), 1);
        assert_eq!(entities[0].get("id"), Some(&Value::I64(1)));
        assert_eq!(scalars[0], Value::I64(42));
    }

    #[test]
    fn test_apply_result_set_mapping_many() {
        let mut rsm = ResultSetMapping::new("userMapping");
        let mut er = EntityResult::new("User");
        er.add_field(FieldResult::new("id", "user_id"));
        rsm.add_entity(er);

        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(1));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(2));
                r
            },
        ];

        let results = apply_result_set_mapping_many(&rsm, &rows);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0[0].get("id"), Some(&Value::I64(1)));
        assert_eq!(results[1].0[0].get("id"), Some(&Value::I64(2)));
    }

    // ===== 端到端场景 =====

    #[test]
    fn test_e2e_user_with_dept_and_roles() {
        let registry = ResultMapRegistry::new();

        // roleMap
        let mut role_map = ResultMap::new("roleMap", "Role");
        role_map
            .add_id_mapping(Mapping::new("id", "role_id"))
            .add_result_mapping(Mapping::new("name", "role_name"));
        registry.register(role_map);

        // deptMap
        let mut dept_map = ResultMap::new("deptMap", "Dept");
        dept_map
            .add_id_mapping(Mapping::new("id", "dept_id"))
            .add_result_mapping(Mapping::new("name", "dept_name"));
        registry.register(dept_map);

        // userMap
        let mut user_map = ResultMap::new("userMap", "User");
        user_map
            .add_id_mapping(Mapping::new("id", "user_id"))
            .add_result_mapping(Mapping::new("name", "user_name"))
            .add_association(NestedAssociation::new("dept", "deptMap"))
            .add_collection(NestedCollection::new("roles", "roleMap"));
        registry.register(user_map);

        // 模拟 JOIN 查询结果：1 个用户 + 1 个部门 + 2 个角色 = 2 行
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(1));
                r.set("user_name", Value::String("Alice".to_string()));
                r.set("dept_id", Value::I64(10));
                r.set("dept_name", Value::String("Engineering".to_string()));
                r.set("role_id", Value::I64(100));
                r.set("role_name", Value::String("admin".to_string()));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(1));
                r.set("user_name", Value::String("Alice".to_string()));
                r.set("dept_id", Value::I64(10));
                r.set("dept_name", Value::String("Engineering".to_string()));
                r.set("role_id", Value::I64(101));
                r.set("role_name", Value::String("editor".to_string()));
                r
            },
        ];

        let result = apply_result_map_many(&registry, "userMap", &rows).unwrap();
        assert_eq!(result.len(), 1);
        let user = &result[0];
        assert_eq!(user.get("id"), Some(&Value::I64(1)));
        assert_eq!(user.get("name"), Some(&Value::String("Alice".to_string())));

        // dept association
        if let Some(Value::Object(dept_attrs)) = user.get("dept") {
            assert_eq!(dept_attrs.get("id"), Some(&Value::I64(10)));
            assert_eq!(
                dept_attrs.get("name"),
                Some(&Value::String("Engineering".to_string()))
            );
        } else {
            panic!("dept should be an Object");
        }

        // roles collection
        if let Some(Value::Array(roles)) = user.get("roles") {
            assert_eq!(roles.len(), 2);
        } else {
            panic!("roles should be an Array");
        }
    }

    #[test]
    fn test_e2e_native_query_with_rsm() {
        // 模拟：SELECT u.id AS user_id, u.name AS user_name, COUNT(o.id) AS order_count
        // FROM users u LEFT JOIN orders o ON o.user_id = u.id
        // GROUP BY u.id
        let mut rsm = ResultSetMapping::new("userOrderCount");
        let mut er = EntityResult::new("User");
        er.add_field(FieldResult::new("id", "user_id"))
            .add_field(FieldResult::new("name", "user_name"));
        rsm.add_entity(er);
        rsm.add_scalar(ScalarResult::new("order_count", "i64"));

        let mut nq = NativeQuery::new(
            "SELECT u.id AS user_id, u.name AS user_name, COUNT(o.id) AS order_count FROM users u LEFT JOIN orders o ON o.user_id = u.id GROUP BY u.id",
            "userOrderCount",
        );
        nq.bind(Value::Null); // 仅示意绑定参数

        // 模拟 ResultSet
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(1));
                r.set("user_name", Value::String("Alice".to_string()));
                r.set("order_count", Value::I64(5));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("user_id", Value::I64(2));
                r.set("user_name", Value::String("Bob".to_string()));
                r.set("order_count", Value::I64(3));
                r
            },
        ];

        let reg = ResultSetMappingRegistry::new();
        reg.register(rsm.clone());
        assert!(reg.contains("userOrderCount"));

        let results = apply_result_set_mapping_many(&rsm, &rows);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0[0].get("id"), Some(&Value::I64(1)));
        assert_eq!(results[0].1[0], Value::I64(5));
        assert_eq!(results[1].0[0].get("id"), Some(&Value::I64(2)));
        assert_eq!(results[1].1[0], Value::I64(3));

        // 验证 NativeQuery 字段
        assert_eq!(nq.result_set_mapping, "userOrderCount");
        assert_eq!(nq.parameters.len(), 1);
    }
}
