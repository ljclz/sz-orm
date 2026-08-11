//! Relay 分页规范：Connection/Edge/PageInfo，cursor-based 分页

use super::error::TicketError;

/// Relay Connection
#[derive(Debug, Clone)]
pub struct RelayConnection<T> {
    pub edges: Vec<RelayEdge<T>>,
    pub page_info: PageInfo,
}

/// Relay Edge
#[derive(Debug, Clone)]
pub struct RelayEdge<T> {
    pub node: T,
    pub cursor: String,
}

/// PageInfo
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageInfo {
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
}

impl<T> RelayConnection<T> {
    pub fn new(edges: Vec<RelayEdge<T>>, page_info: PageInfo) -> Self {
        Self { edges, page_info }
    }

    pub fn empty() -> Self {
        Self {
            edges: vec![],
            page_info: PageInfo {
                has_next_page: false,
                has_previous_page: false,
                start_cursor: None,
                end_cursor: None,
            },
        }
    }
}

/// Relay 分页：从一组数据生成 Connection
pub fn relay_paginate<T: Clone>(
    items: &[T],
    first: usize,
    after: Option<&str>,
    cursor_fn: impl Fn(&T) -> String,
) -> Result<RelayConnection<T>, TicketError> {
    if first == 0 {
        return Ok(RelayConnection::empty());
    }

    let start_idx = match after {
        Some(cursor) => items
            .iter()
            .position(|item| cursor_fn(item) == cursor)
            .map(|idx| idx + 1)
            .unwrap_or(0),
        None => 0,
    };

    let available = if start_idx < items.len() {
        &items[start_idx..]
    } else {
        &items[0..0]
    };

    let has_next_page = available.len() > first;
    let page_items = if has_next_page {
        &available[..first]
    } else {
        available
    };

    let edges: Vec<RelayEdge<T>> = page_items
        .iter()
        .map(|item| RelayEdge {
            node: item.clone(),
            cursor: cursor_fn(item),
        })
        .collect();

    let start_cursor = edges.first().map(|e| e.cursor.clone());
    let end_cursor = edges.last().map(|e| e.cursor.clone());

    Ok(RelayConnection {
        edges,
        page_info: PageInfo {
            has_next_page,
            has_previous_page: after.is_some() && start_idx > 0,
            start_cursor,
            end_cursor,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor_fn(item: &u32) -> String {
        format!("cursor-{item}")
    }

    #[test]
    fn test_relay_paginate_basic() {
        let items = vec![1, 2, 3, 4, 5];
        let conn = relay_paginate(&items, 3, None, cursor_fn).unwrap();

        assert_eq!(conn.edges.len(), 3);
        assert!(conn.page_info.has_next_page);
        assert!(!conn.page_info.has_previous_page);
        assert_eq!(conn.page_info.start_cursor, Some("cursor-1".to_string()));
        assert_eq!(conn.page_info.end_cursor, Some("cursor-3".to_string()));
    }

    #[test]
    fn test_relay_paginate_with_after() {
        let items = vec![1, 2, 3, 4, 5];
        let conn = relay_paginate(&items, 2, Some("cursor-2"), cursor_fn).unwrap();

        assert_eq!(conn.edges.len(), 2);
        assert_eq!(conn.edges[0].node, 3);
        assert!(conn.page_info.has_next_page);
        assert!(conn.page_info.has_previous_page);
    }

    #[test]
    fn test_relay_paginate_no_more_pages() {
        let items = vec![1, 2, 3];
        let conn = relay_paginate(&items, 10, None, cursor_fn).unwrap();

        assert_eq!(conn.edges.len(), 3);
        assert!(!conn.page_info.has_next_page);
    }

    #[test]
    fn test_relay_paginate_empty() {
        let items: Vec<u32> = vec![];
        let conn = relay_paginate(&items, 10, None, cursor_fn).unwrap();

        assert!(conn.edges.is_empty());
        assert!(!conn.page_info.has_next_page);
        assert!(conn.page_info.start_cursor.is_none());
        assert!(conn.page_info.end_cursor.is_none());
    }

    #[test]
    fn test_relay_paginate_first_zero() {
        let items = vec![1, 2, 3];
        let conn = relay_paginate(&items, 0, None, cursor_fn).unwrap();
        assert!(conn.edges.is_empty());
    }

    #[test]
    fn test_relay_paginate_after_last() {
        let items = vec![1, 2, 3];
        let conn = relay_paginate(&items, 10, Some("cursor-3"), cursor_fn).unwrap();
        assert!(conn.edges.is_empty());
        assert!(!conn.page_info.has_next_page);
    }

    #[test]
    fn test_relay_paginate_after_not_found() {
        let items = vec![1, 2, 3];
        let conn = relay_paginate(&items, 2, Some("nonexistent"), cursor_fn).unwrap();
        assert_eq!(conn.edges.len(), 2);
    }

    #[test]
    fn test_relay_connection_empty() {
        let conn: RelayConnection<u32> = RelayConnection::empty();
        assert!(conn.edges.is_empty());
        assert!(!conn.page_info.has_next_page);
    }

    #[test]
    fn test_page_info_eq() {
        let pi1 = PageInfo {
            has_next_page: true,
            has_previous_page: false,
            start_cursor: Some("a".to_string()),
            end_cursor: Some("b".to_string()),
        };
        let pi2 = pi1.clone();
        assert_eq!(pi1, pi2);
    }
}
