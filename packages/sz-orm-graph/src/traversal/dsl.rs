//! 图遍历 DSL — 点分路径解析
//!
//! 将 "user.posts.comments" 解析为遍历路径。

/// 路径段（单次关系跳转）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSegment {
    /// 关系名称
    pub relation: String,
    /// 内联过滤条件（可选）
    pub filter: Option<String>,
    /// 排序（可选）
    pub order_by: Option<String>,
    /// 限制数量（可选）
    pub limit: Option<usize>,
}

impl PathSegment {
    /// 创建路径段
    pub fn new(relation: &str) -> Self {
        Self {
            relation: relation.to_string(),
            filter: None,
            order_by: None,
            limit: None,
        }
    }

    /// 添加过滤条件
    pub fn with_filter(mut self, filter: &str) -> Self {
        self.filter = Some(filter.to_string());
        self
    }

    /// 添加排序
    pub fn with_order_by(mut self, order: &str) -> Self {
        self.order_by = Some(order.to_string());
        self
    }

    /// 添加限制
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// 遍历路径
#[derive(Debug, Clone)]
pub struct TraversalPath {
    /// 起始表
    pub root_table: String,
    /// 路径段
    pub segments: Vec<PathSegment>,
    /// 最大深度
    pub max_depth: usize,
}

impl TraversalPath {
    /// 创建遍历路径
    pub fn new(root_table: &str) -> Self {
        Self {
            root_table: root_table.to_string(),
            segments: Vec::new(),
            max_depth: 10,
        }
    }

    /// 添加路径段
    pub fn push(&mut self, segment: PathSegment) -> &mut Self {
        self.segments.push(segment);
        self
    }

    /// 遍历深度
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// 是否超出最大深度
    pub fn exceeds_max_depth(&self) -> bool {
        self.depth() > self.max_depth
    }

    /// 设置最大深度
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }
}

/// 图遍历 DSL 解析器
pub struct GraphTraversalDsl;

impl GraphTraversalDsl {
    /// 解析点分路径 "user.posts.comments" → TraversalPath
    pub fn parse(input: &str) -> Result<TraversalPath, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("空路径".into());
        }
        let parts: Vec<&str> = input.split('.').collect();
        let root_table = parts[0].to_string();
        let mut path = TraversalPath::new(&root_table);
        for &part in &parts[1..] {
            if part.is_empty() {
                return Err("空路径段".into());
            }
            path.push(PathSegment::new(part));
        }
        Ok(path)
    }

    /// 解析带过滤的路径 "user.posts[active=true].comments"
    pub fn parse_with_filters(input: &str) -> Result<TraversalPath, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("空路径".into());
        }
        let parts: Vec<&str> = input.split('.').collect();
        let root_table = parts[0].to_string();
        let mut path = TraversalPath::new(&root_table);
        for &part in &parts[1..] {
            if part.is_empty() {
                return Err("空路径段".into());
            }
            let segment = Self::parse_segment(part);
            path.push(segment);
        }
        Ok(path)
    }

    /// 解析单个路径段（支持 [filter] 语法）
    fn parse_segment(part: &str) -> PathSegment {
        if let Some(bracket_start) = part.find('[') {
            if let Some(bracket_end) = part.find(']') {
                if bracket_end > bracket_start {
                    let relation = &part[..bracket_start];
                    let filter = &part[bracket_start + 1..bracket_end];
                    return PathSegment::new(relation).with_filter(filter);
                }
            }
        }
        PathSegment::new(part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let path = GraphTraversalDsl::parse("user.posts.comments").unwrap();
        assert_eq!(path.root_table, "user");
        assert_eq!(path.depth(), 2);
        assert_eq!(path.segments[0].relation, "posts");
        assert_eq!(path.segments[1].relation, "comments");
    }

    #[test]
    fn test_parse_single() {
        let path = GraphTraversalDsl::parse("user").unwrap();
        assert_eq!(path.root_table, "user");
        assert_eq!(path.depth(), 0);
    }

    #[test]
    fn test_parse_with_filter() {
        let path =
            GraphTraversalDsl::parse_with_filters("user.posts[active=true].comments").unwrap();
        assert_eq!(path.segments[0].relation, "posts");
        assert_eq!(path.segments[0].filter.as_deref(), Some("active=true"));
        assert_eq!(path.segments[1].relation, "comments");
        assert!(path.segments[1].filter.is_none());
    }

    #[test]
    fn test_max_depth() {
        let path = TraversalPath::new("user").with_max_depth(3);
        assert_eq!(path.max_depth, 3);
    }

    #[test]
    fn test_exceeds_max_depth() {
        let mut path = TraversalPath::new("user").with_max_depth(2);
        path.push(PathSegment::new("a"));
        path.push(PathSegment::new("b"));
        path.push(PathSegment::new("c"));
        assert!(path.exceeds_max_depth());
    }

    #[test]
    fn test_path_segment_builders() {
        let seg = PathSegment::new("posts")
            .with_filter("active = true")
            .with_order_by("created_at DESC")
            .with_limit(10);
        assert_eq!(seg.filter.as_deref(), Some("active = true"));
        assert_eq!(seg.order_by.as_deref(), Some("created_at DESC"));
        assert_eq!(seg.limit, Some(10));
    }

    #[test]
    fn test_parse_empty() {
        assert!(GraphTraversalDsl::parse("").is_err());
    }
}
