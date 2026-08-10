//! M4-T3: 类型安全列引用
//!
//! `Column<T>` 通过幻影类型 `T` 在编译期保证列引用属于指定表，
//! 防止跨表列引用错误。启用 `type-safe-columns` feature 后可用。
//!
//! # 示例
//!
//! ```ignore
//! use sz_orm_core::column::Column;
//!
//! let col = Column::<User>::new("id");
//! assert_eq!(col.name(), "id");
//! ```

#![cfg(feature = "type-safe-columns")]

use std::marker::PhantomData;

/// Schema trait — 标记一个结构体可作为表 schema 使用
///
/// `#[derive(Schema)]` 在 `type-safe-columns` feature 启用时自动实现此 trait。
pub trait Schema {
    /// 表名
    fn schema_table_name() -> &'static str;
}

/// 类型安全列引用
///
/// 通过幻影类型 `T` 在编译期将列绑定到特定表，防止跨表列引用。
/// 运行时零额外开销（仅一个 `&'static str` + 零大小 PhantomData）。
pub struct Column<T: Schema> {
    name: &'static str,
    _marker: PhantomData<T>,
}

impl<T: Schema> Clone for Column<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Schema> Copy for Column<T> {}

impl<T: Schema> std::fmt::Debug for Column<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Column")
            .field("name", &self.name)
            .field("table", &T::schema_table_name())
            .finish()
    }
}

impl<T: Schema> PartialEq for Column<T> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl<T: Schema> Eq for Column<T> {}

impl<T: Schema> std::hash::Hash for Column<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl<T: Schema> Column<T> {
    /// 创建一个关联到表 `T` 的列引用
    pub const fn new(name: &'static str) -> Self {
        Column {
            name,
            _marker: PhantomData,
        }
    }

    /// 返回列名
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// 返回关联的表名
    pub fn table_name() -> &'static str {
        T::schema_table_name()
    }
}

impl<T: Schema> std::fmt::Display for Column<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl<T: Schema> std::ops::Deref for Column<T> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.name
    }
}

impl<T: Schema> AsRef<str> for Column<T> {
    fn as_ref(&self) -> &str {
        self.name
    }
}

impl<T: Schema> From<&'static str> for Column<T> {
    fn from(name: &'static str) -> Self {
        Column::new(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestTable;

    impl Schema for TestTable {
        fn schema_table_name() -> &'static str {
            "test_table"
        }
    }

    #[test]
    fn test_column_basic() {
        let col = Column::<TestTable>::new("id");
        assert_eq!(col.name(), "id");
        assert_eq!(&*col, "id");
        assert_eq!(col.to_string(), "id");
        assert_eq!(col.as_ref(), "id");
    }

    #[test]
    fn test_column_table_name() {
        assert_eq!(Column::<TestTable>::table_name(), "test_table");
    }

    #[test]
    fn test_column_deref() {
        let col = Column::<TestTable>::new("name");
        let s: &str = &col;
        assert_eq!(s, "name");
    }

    #[test]
    fn test_column_from_str() {
        let col: Column<TestTable> = "email".into();
        assert_eq!(col.name(), "email");
    }

    #[test]
    fn test_column_copy() {
        let col = Column::<TestTable>::new("id");
        let col2 = col;
        assert_eq!(col.name(), col2.name());
    }

    #[test]
    fn test_column_eq() {
        let col1 = Column::<TestTable>::new("id");
        let col2 = Column::<TestTable>::new("id");
        let col3 = Column::<TestTable>::new("name");
        assert_eq!(col1, col2);
        assert_ne!(col1, col3);
    }
}
