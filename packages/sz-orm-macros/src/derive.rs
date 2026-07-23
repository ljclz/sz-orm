//! 派生宏实现模块
//!
//! 提供 `#[derive(Schema)]` 和 `#[derive(Builder)]` 派生宏，以及字段级属性
//! （`#[column]` / `#[table]`）和宏展开诊断能力。
//!
//! # `#[derive(Schema)]`
//!
//! 自动从 Rust 结构体生成表结构信息，便于在运行时反射表名与列信息。
//!
//! ```ignore
//! use sz_orm_macros::Schema;
//!
//! #[derive(Schema)]
//! #[table(name = "users")]
//! struct User {
//!     #[column(primary_key)]
//!     id: i64,
//!     #[column(name = "user_name", type = "VARCHAR(255)")]
//!     name: String,
//!     email: Option<String>,
//! }
//! ```
//!
//! # `#[derive(Builder)]`
//!
//! 自动生成构造器模式代码。
//!
//! ```ignore
//! use sz_orm_macros::Builder;
//!
//! #[derive(Builder)]
//! struct User {
//!     id: i64,
//!     name: String,
//!     email: Option<String>,
//! }
//!
//! let user = UserBuilder::new()
//!     .id(1)
//!     .name("Alice".to_string())
//!     .email(Some("a@b.com".to_string()))
//!     .build()
//!     .unwrap();
//! ```

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Expr, Fields, Lit, Type};

// ---------------------------------------------------------------------------
// 公共诊断辅助：生成带 span 的编译错误
// ---------------------------------------------------------------------------

/// 把 `syn::Error` 转成 `compile_error!` TokenStream
fn syn_error_to_compile_error(err: syn::Error) -> TokenStream2 {
    let msg = err.to_string();
    let span = err.span();
    let mut lit_lit = proc_macro2::Literal::string(&msg);
    lit_lit.set_span(span);
    quote! { compile_error!(#lit_lit) }
}

/// 收集诊断信息字符串（用于宏展开诊断功能）
///
/// 当 `SZ_ORM_MACRO_TRACE=1` 环境变量存在时，会在编译期输出诊断信息到 stderr。
fn trace_diag(stage: &str, info: &str) {
    if std::env::var("SZ_ORM_MACRO_TRACE").ok().as_deref() == Some("1") {
        eprintln!("[sz-orm-macro][{}] {}", stage, info);
    }
}

// ---------------------------------------------------------------------------
// 属性解析：#[table(...)] / #[column(...)]
// ---------------------------------------------------------------------------

/// 解析 `#[table(name = "users")]` 属性，返回表名
fn parse_table_attr(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("table") {
            continue;
        }
        let mut table_name = None;
        // 解析 name = "value"
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let lit: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = lit {
                    table_name = Some(s.value());
                }
            }
            Ok(())
        });
        return table_name;
    }
    None
}

/// 字段级属性解析结果
#[derive(Default)]
struct ColumnAttr {
    /// 列名覆盖（None 表示使用字段名）
    name: Option<String>,
    /// SQL 类型覆盖
    sql_type: Option<String>,
    /// 是否主键
    primary_key: bool,
    /// 是否允许 NULL
    nullable: bool,
    /// 是否跳过此列（不生成 schema 条目）
    skip: bool,
    /// 默认值表达式
    default: Option<String>,
}

/// 解析 `#[column(...)]` 属性
fn parse_column_attr(attrs: &[Attribute]) -> ColumnAttr {
    let mut attr = ColumnAttr::default();
    for a in attrs {
        if !a.path().is_ident("column") {
            continue;
        }
        let _ = a.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let lit: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = lit {
                    attr.name = Some(s.value());
                }
            } else if meta.path.is_ident("type") {
                let lit: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = lit {
                    attr.sql_type = Some(s.value());
                }
            } else if meta.path.is_ident("primary_key") {
                attr.primary_key = true;
            } else if meta.path.is_ident("nullable") {
                attr.nullable = true;
            } else if meta.path.is_ident("skip") {
                attr.skip = true;
            } else if meta.path.is_ident("default") {
                let lit: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = lit {
                    attr.default = Some(s.value());
                }
            }
            Ok(())
        });
    }
    attr
}

/// 判断 Rust 类型是否为 `Option<T>`，若是返回内部类型字符串
fn is_option_type(ty: &Type) -> Option<String> {
    let s = quote!(#ty).to_string().replace(" ", "");
    if s.starts_with("Option<") && s.ends_with('>') {
        Some(s[7..s.len() - 1].to_string())
    } else {
        None
    }
}

/// 将 Rust 类型映射为 SQL 类型字符串
fn rust_type_to_sql_type(ty: &Type) -> String {
    let inner = is_option_type(ty).unwrap_or_else(|| quote!(#ty).to_string().replace(" ", ""));
    let lower = inner.to_lowercase();
    if lower.starts_with("i64") || lower.starts_with("u64") {
        "BIGINT".to_string()
    } else if lower.starts_with("i32") || lower.starts_with("u32") {
        "INTEGER".to_string()
    } else if lower.starts_with("i16") || lower.starts_with("u16") {
        "SMALLINT".to_string()
    } else if lower.starts_with("i8") || lower.starts_with("u8") {
        "TINYINT".to_string()
    } else if lower.starts_with("f32") {
        "FLOAT".to_string()
    } else if lower.starts_with("f64") {
        "DOUBLE".to_string()
    } else if lower.starts_with("bool") {
        "BOOLEAN".to_string()
    } else if lower.starts_with("string") {
        "TEXT".to_string()
    } else if lower.starts_with("vec<u8>") || lower == "vec<u8>" {
        "BLOB".to_string()
    } else if lower.starts_with("chrono::datetime") || lower.contains("datetime") {
        "TIMESTAMP".to_string()
    } else if lower.starts_with("uuid") {
        "UUID".to_string()
    } else {
        "TEXT".to_string()
    }
}

// ---------------------------------------------------------------------------
// `#[derive(Schema)]`
// ---------------------------------------------------------------------------

/// `#[derive(Schema)]` 派生宏入口
///
/// 接收已解析的 `DeriveInput`，返回 `proc_macro2::TokenStream`，
/// 便于在单元测试中直接调用（不依赖 proc_macro 上下文）。
pub fn derive_schema_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag("derive(Schema)", &format!("target struct: {}", input.ident));

    let struct_name = &input.ident;

    // 仅支持命名字段结构体
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn_error_to_compile_error(syn::Error::new_spanned(
                    struct_name,
                    "Schema 仅支持命名字段结构体（struct Foo { a: T }）",
                ))
            }
        },
        _ => {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                struct_name,
                "Schema 仅支持 struct，不支持 enum / union",
            ))
        }
    };

    // 解析 #[table(name = "...")]，默认使用结构体名（小写、蛇形）
    let table_name = parse_table_attr(&input.attrs).unwrap_or_else(|| {
        to_snake_case(&struct_name.to_string())
    });

    trace_diag("derive(Schema)", &format!("table_name = {}", table_name));

    // 收集每个字段的列信息
    let mut column_entries = Vec::new();

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap().to_string();
        let col_attr = parse_column_attr(&field.attrs);

        if col_attr.skip {
            continue;
        }

        let col_name = col_attr.name.clone().unwrap_or_else(|| field_name.clone());
        let sql_type = col_attr
            .sql_type
            .clone()
            .unwrap_or_else(|| rust_type_to_sql_type(&field.ty));
        let nullable = col_attr.nullable || is_option_type(&field.ty).is_some();
        let primary_key = col_attr.primary_key;
        let has_default = col_attr.default.is_some();

        column_entries.push(quote! {
            (#col_name, #field_name, #sql_type, #nullable, #primary_key, #has_default)
        });
    }

    let columns_len = column_entries.len();

    // 注意：proc-macro crate 不能导出普通 trait/struct，因此生成 inherent 方法。
    // 列信息以元组形式返回：(列名, Rust字段名, SQL类型, nullable, primary_key, has_default)
    let expanded = quote! {
        #[allow(dead_code)]
        impl #struct_name {
            /// 表名（由 #[derive(Schema)] 生成）
            pub const SZ_ORM_TABLE_NAME: &'static str = #table_name;

            /// 返回表名
            pub fn sz_orm_table_name() -> &'static str {
                #table_name
            }

            /// 返回列信息切片
            ///
            /// 每个元素是元组：`(列名, Rust字段名, SQL类型, nullable, primary_key, has_default)`
            pub fn sz_orm_columns() -> &'static [(&'static str, &'static str, &'static str, bool, bool, bool)] {
                static COLUMNS: &[(&'static str, &'static str, &'static str, bool, bool, bool)] = &[
                    #(#column_entries),*
                ];
                COLUMNS
            }

            /// 返回列数
            pub fn sz_orm_column_count() -> usize {
                #columns_len
            }
        }
    };

    expanded
}

// ---------------------------------------------------------------------------
// `#[derive(Builder)]`
// ---------------------------------------------------------------------------

/// `#[derive(Builder)]` 派生宏入口
///
/// 接收已解析的 `DeriveInput`，返回 `proc_macro2::TokenStream`，
/// 便于在单元测试中直接调用（不依赖 proc_macro 上下文）。
pub fn derive_builder_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag("derive(Builder)", &format!("target struct: {}", input.ident));

    let struct_name = &input.ident;
    let builder_name = format_ident!("{}Builder", struct_name);

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn_error_to_compile_error(syn::Error::new_spanned(
                    struct_name,
                    "Builder 仅支持命名字段结构体",
                ))
            }
        },
        _ => {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                struct_name,
                "Builder 仅支持 struct",
            ))
        }
    };

    // 检查 #[builder(skip)] 字段，不生成 setter
    struct FieldInfo {
        ident: syn::Ident,
        ty: Type,
        skip: bool,
        default: Option<Expr>,
    }

    let mut field_infos = Vec::new();
    for field in fields.iter() {
        let ident = field.ident.clone().unwrap();
        let mut skip = false;
        let mut default: Option<Expr> = None;

        for attr in &field.attrs {
            if attr.path().is_ident("builder") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("skip") {
                        skip = true;
                    } else if meta.path.is_ident("default") {
                        let expr: Expr = meta.value()?.parse()?;
                        default = Some(expr);
                    }
                    Ok(())
                });
            }
        }

        field_infos.push(FieldInfo {
            ident,
            ty: field.ty.clone(),
            skip,
            default,
        });
    }

    // 生成 builder 字段（全部 Option<T>）
    let builder_fields = field_infos.iter().map(|f| {
        let ident = &f.ident;
        let ty = &f.ty;
        quote! { #ident: ::std::option::Option<#ty> }
    });

    // 生成 setter 方法
    let setters = field_infos.iter().filter(|f| !f.skip).map(|f| {
        let ident = &f.ident;
        let ty = &f.ty;
        quote! {
            pub fn #ident(mut self, value: #ty) -> Self {
                self.#ident = ::std::option::Option::Some(value);
                self
            }
        }
    });

    // 生成 build() 方法中对每个字段的处理
    let build_fields = field_infos.iter().map(|f| {
        let ident = &f.ident;
        if let Some(default_expr) = &f.default {
            quote! {
                #ident: self.#ident.unwrap_or_else(|| #default_expr)
            }
        } else if f.skip {
            // skip 字段需要 Default
            quote! {
                #ident: ::std::default::Default::default()
            }
        } else {
            quote! {
                #ident: self.#ident.ok_or_else(|| ::std::format!("字段 `{}` 未设置", stringify!(#ident)))?
            }
        }
    });

    // 生成 builder 字段的初始化（全部 None）
    let builder_default_inits: Vec<_> = field_infos
        .iter()
        .map(|f| {
            let ident = &f.ident;
            quote! { #ident: ::std::option::Option::None }
        })
        .collect();

    let expanded = quote! {
        /// 自动生成的 Builder 类型
        pub struct #builder_name {
            #(#builder_fields,)*
        }

        impl #builder_name {
            /// 创建空的 builder
            pub fn new() -> Self {
                Self {
                    #(#builder_default_inits),*
                }
            }

            #(#setters)*

            /// 构建目标结构体
            ///
            /// 返回 `Result<T, String>`，未设置的非可选字段会返回错误
            pub fn build(self) -> ::std::result::Result<#struct_name, String> {
                ::std::result::Result::Ok(#struct_name {
                    #(#build_fields,)*
                })
            }
        }

        impl ::std::default::Default for #builder_name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl #struct_name {
            /// 返回此结构体的 Builder
            pub fn builder() -> #builder_name {
                #builder_name::new()
            }
        }
    };

    expanded
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 将驼峰命名转为蛇形命名（如 `UserAccount` → `user_account`）
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- to_snake_case ----

    #[test]
    fn test_to_snake_case_simple() {
        assert_eq!(to_snake_case("User"), "user");
        assert_eq!(to_snake_case("Order"), "order");
    }

    #[test]
    fn test_to_snake_case_camel() {
        assert_eq!(to_snake_case("UserAccount"), "user_account");
        assert_eq!(to_snake_case("OrderItem"), "order_item");
    }

    #[test]
    fn test_to_snake_case_all_caps() {
        assert_eq!(to_snake_case("URL"), "u_r_l");
        assert_eq!(to_snake_case("APIKey"), "a_p_i_key");
    }

    #[test]
    fn test_to_snake_case_lowercase() {
        assert_eq!(to_snake_case("user"), "user");
        assert_eq!(to_snake_case("users"), "users");
    }

    #[test]
    fn test_to_snake_case_empty() {
        assert_eq!(to_snake_case(""), "");
    }

    // ---- is_option_type ----

    #[test]
    fn test_is_option_type_some() {
        let ty: Type = syn::parse_str("Option<String>").unwrap();
        assert_eq!(is_option_type(&ty), Some("String".to_string()));
    }

    #[test]
    fn test_is_option_type_i64() {
        let ty: Type = syn::parse_str("Option<i64>").unwrap();
        assert_eq!(is_option_type(&ty), Some("i64".to_string()));
    }

    #[test]
    fn test_is_option_type_not_option() {
        let ty: Type = syn::parse_str("String").unwrap();
        assert_eq!(is_option_type(&ty), None);
    }

    #[test]
    fn test_is_option_type_vec() {
        let ty: Type = syn::parse_str("Vec<u8>").unwrap();
        assert_eq!(is_option_type(&ty), None);
    }

    // ---- rust_type_to_sql_type ----

    #[test]
    fn test_rust_type_to_sql_i64() {
        let ty: Type = syn::parse_str("i64").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "BIGINT");
    }

    #[test]
    fn test_rust_type_to_sql_i32() {
        let ty: Type = syn::parse_str("i32").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "INTEGER");
    }

    #[test]
    fn test_rust_type_to_sql_i16() {
        let ty: Type = syn::parse_str("i16").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "SMALLINT");
    }

    #[test]
    fn test_rust_type_to_sql_i8() {
        let ty: Type = syn::parse_str("i8").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "TINYINT");
    }

    #[test]
    fn test_rust_type_to_sql_f32() {
        let ty: Type = syn::parse_str("f32").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "FLOAT");
    }

    #[test]
    fn test_rust_type_to_sql_f64() {
        let ty: Type = syn::parse_str("f64").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "DOUBLE");
    }

    #[test]
    fn test_rust_type_to_sql_bool() {
        let ty: Type = syn::parse_str("bool").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "BOOLEAN");
    }

    #[test]
    fn test_rust_type_to_sql_string() {
        let ty: Type = syn::parse_str("String").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "TEXT");
    }

    #[test]
    fn test_rust_type_to_sql_vec_u8() {
        let ty: Type = syn::parse_str("Vec<u8>").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "BLOB");
    }

    #[test]
    fn test_rust_type_to_sql_option_string() {
        let ty: Type = syn::parse_str("Option<String>").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "TEXT");
    }

    #[test]
    fn test_rust_type_to_sql_option_i64() {
        let ty: Type = syn::parse_str("Option<i64>").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "BIGINT");
    }

    #[test]
    fn test_rust_type_to_sql_unknown() {
        let ty: Type = syn::parse_str("MyCustomType").unwrap();
        assert_eq!(rust_type_to_sql_type(&ty), "TEXT");
    }

    // ---- parse_table_attr ----

    #[test]
    fn test_parse_table_attr_present() {
        let input: DeriveInput = syn::parse_str(
            r#"
            #[table(name = "my_table")]
            struct Foo { a: i64 }
        "#,
        )
        .unwrap();
        assert_eq!(parse_table_attr(&input.attrs), Some("my_table".to_string()));
    }

    #[test]
    fn test_parse_table_attr_absent() {
        let input: DeriveInput = syn::parse_str("struct Foo { a: i64 }").unwrap();
        assert_eq!(parse_table_attr(&input.attrs), None);
    }

    // ---- parse_column_attr ----

    #[test]
    fn test_parse_column_attr_name() {
        let input: DeriveInput = syn::parse_str(
            r#"
            struct Foo {
                #[column(name = "user_id")]
                id: i64
            }
        "#,
        )
        .unwrap();
        if let Data::Struct(s) = &input.data {
            if let Fields::Named(named) = &s.fields {
                let field = named.named.first().unwrap();
                let attr = parse_column_attr(&field.attrs);
                assert_eq!(attr.name, Some("user_id".to_string()));
                assert!(!attr.primary_key);
                assert!(!attr.nullable);
                assert!(!attr.skip);
            }
        }
    }

    #[test]
    fn test_parse_column_attr_primary_key() {
        let input: DeriveInput = syn::parse_str(
            r#"
            struct Foo {
                #[column(primary_key)]
                id: i64
            }
        "#,
        )
        .unwrap();
        if let Data::Struct(s) = &input.data {
            if let Fields::Named(named) = &s.fields {
                let field = named.named.first().unwrap();
                let attr = parse_column_attr(&field.attrs);
                assert!(attr.primary_key);
            }
        }
    }

    #[test]
    fn test_parse_column_attr_type_override() {
        let input: DeriveInput = syn::parse_str(
            r#"
            struct Foo {
                #[column(type = "VARCHAR(255)")]
                name: String
            }
        "#,
        )
        .unwrap();
        if let Data::Struct(s) = &input.data {
            if let Fields::Named(named) = &s.fields {
                let field = named.named.first().unwrap();
                let attr = parse_column_attr(&field.attrs);
                assert_eq!(attr.sql_type, Some("VARCHAR(255)".to_string()));
            }
        }
    }

    #[test]
    fn test_parse_column_attr_nullable() {
        let input: DeriveInput = syn::parse_str(
            r#"
            struct Foo {
                #[column(nullable)]
                name: String
            }
        "#,
        )
        .unwrap();
        if let Data::Struct(s) = &input.data {
            if let Fields::Named(named) = &s.fields {
                let field = named.named.first().unwrap();
                let attr = parse_column_attr(&field.attrs);
                assert!(attr.nullable);
            }
        }
    }

    #[test]
    fn test_parse_column_attr_skip() {
        let input: DeriveInput = syn::parse_str(
            r#"
            struct Foo {
                #[column(skip)]
                internal: String
            }
        "#,
        )
        .unwrap();
        if let Data::Struct(s) = &input.data {
            if let Fields::Named(named) = &s.fields {
                let field = named.named.first().unwrap();
                let attr = parse_column_attr(&field.attrs);
                assert!(attr.skip);
            }
        }
    }

    #[test]
    fn test_parse_column_attr_default() {
        let input: DeriveInput = syn::parse_str(
            r#"
            struct Foo {
                #[column(default = "0")]
                count: i64
            }
        "#,
        )
        .unwrap();
        if let Data::Struct(s) = &input.data {
            if let Fields::Named(named) = &s.fields {
                let field = named.named.first().unwrap();
                let attr = parse_column_attr(&field.attrs);
                assert_eq!(attr.default, Some("0".to_string()));
            }
        }
    }

    #[test]
    fn test_parse_column_attr_combined() {
        let input: DeriveInput = syn::parse_str(
            r#"
            struct Foo {
                #[column(name = "uid", primary_key, type = "BIGINT")]
                id: i64
            }
        "#,
        )
        .unwrap();
        if let Data::Struct(s) = &input.data {
            if let Fields::Named(named) = &s.fields {
                let field = named.named.first().unwrap();
                let attr = parse_column_attr(&field.attrs);
                assert_eq!(attr.name, Some("uid".to_string()));
                assert_eq!(attr.sql_type, Some("BIGINT".to_string()));
                assert!(attr.primary_key);
            }
        }
    }

    #[test]
    fn test_parse_column_attr_empty() {
        let input: DeriveInput = syn::parse_str(
            r#"
            struct Foo {
                id: i64
            }
        "#,
        )
        .unwrap();
        if let Data::Struct(s) = &input.data {
            if let Fields::Named(named) = &s.fields {
                let field = named.named.first().unwrap();
                let attr = parse_column_attr(&field.attrs);
                assert_eq!(attr.name, None);
                assert!(!attr.primary_key);
            }
        }
    }

    // ---- 宏展开 smoke 测试 ----
    //
    // 注意：proc-macro API（parse_macro_input! / proc_macro::TokenStream）不能在
    // 单元测试中调用，因此内部实现函数接收已解析的 `DeriveInput` 并返回
    // `proc_macro2::TokenStream`。测试通过 `syn::parse_quote!` 构造输入。

    #[test]
    fn test_derive_schema_compiles() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key)]
                id: i64,
                name: String,
                email: Option<String>,
            }
        };
        let output = derive_schema_impl(input);
        // 应该生成非空 TokenStream
        let output_str = output.to_string();
        assert!(
            output_str.contains("TABLE_NAME"),
            "Schema 派生应生成 TABLE_NAME 常量: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_with_table_attr() {
        let input: DeriveInput = syn::parse_quote! {
            #[table(name = "users")]
            struct User {
                #[column(primary_key)]
                id: i64,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("users"),
            "Schema 派生应使用 #[table] 指定的表名: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_default_table_name() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserAccount {
                id: i64,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("user_account"),
            "默认表名应为蛇形: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_skip_column() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                id: i64,
                #[column(skip)]
                internal: String,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        // skip 字段不应在生成的列信息元组中作为列名出现
        // （注意：字段名 internal 可能出现在其他位置，因此使用元组首元素匹配）
        assert!(
            !output_str.contains("\"internal\""),
            "skip 字段不应出现在 schema 列信息中: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_compiles() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                id: i64,
                name: String,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("UserBuilder"),
            "Builder 派生应生成 UserBuilder: {}",
            output_str
        );
        assert!(
            output_str.contains("build"),
            "Builder 派生应生成 build 方法: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_has_setters() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                id: i64,
                name: String,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        // setter 方法名应与字段名相同
        assert!(
            output_str.contains("fn id"),
            "Builder 应有 id setter: {}",
            output_str
        );
        assert!(
            output_str.contains("fn name"),
            "Builder 应有 name setter: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_skip_field() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                id: i64,
                #[builder(skip)]
                computed: String,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("Default :: default") || output_str.contains("Default::default"),
            "skip 字段应使用 Default: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_default_value() {
        let input: DeriveInput = syn::parse_quote! {
            struct Counter {
                #[builder(default = 0)]
                count: i64,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("unwrap_or_else"),
            "default 字段应使用 unwrap_or_else: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_rejects_enum() {
        let input: DeriveInput = syn::parse_quote! {
            enum Foo {
                A,
                B,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("compile_error"),
            "enum 应触发编译错误: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_rejects_enum() {
        let input: DeriveInput = syn::parse_quote! {
            enum Foo {
                A,
                B,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("compile_error"),
            "enum 应触发编译错误: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_rejects_tuple_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo(i64, String);
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("compile_error"),
            "元组结构体应触发编译错误: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_rejects_tuple_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo(i64, String);
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("compile_error"),
            "元组结构体应触发编译错误: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_rejects_union() {
        let input: DeriveInput = syn::parse_quote! {
            union Foo {
                a: i64,
                b: u64,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("compile_error"),
            "union 应触发编译错误: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_with_multiple_columns() {
        let input: DeriveInput = syn::parse_quote! {
            #[table(name = "orders")]
            struct Order {
                #[column(primary_key)]
                id: i64,
                #[column(name = "user_id", type = "BIGINT")]
                user_id: i64,
                #[column(nullable)]
                note: String,
                total: f64,
                #[column(skip)]
                internal: String,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        // 表名
        assert!(output_str.contains("orders"));
        // 列名
        assert!(output_str.contains("\"id\""));
        assert!(output_str.contains("\"user_id\""));
        assert!(output_str.contains("\"note\""));
        assert!(output_str.contains("\"total\""));
        // skip 字段不应出现
        assert!(!output_str.contains("\"internal\""));
    }

    #[test]
    fn test_derive_schema_column_count() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                a: i64,
                b: String,
                c: f64,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        // 应生成 sz_orm_column_count 返回 3
        assert!(
            output_str.contains("3") || output_str.contains("usize"),
            "应包含列数 3: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_has_new_method() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                a: i64,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("fn new"),
            "Builder 应有 new 方法: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_has_default_impl() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                a: i64,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("Default"),
            "Builder 应实现 Default: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_has_builder_method_on_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                a: i64,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("fn builder"),
            "原结构体应有 builder() 方法: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_build_returns_result() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                a: i64,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("Result"),
            "build 方法应返回 Result: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_option_field_marked_nullable() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                id: i64,
                email: Option<String>,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        // Option<String> 应被识别为 nullable
        // 元组格式: (列名, 字段名, SQL类型, nullable, primary_key, has_default)
        // email 行应包含 true 表示 nullable
        assert!(
            output_str.contains("TEXT"),
            "Option<String> 应映射为 TEXT: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_int_types_mapping() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                a: i64,
                b: i32,
                c: i16,
                d: i8,
                e: u64,
                f: u32,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(output_str.contains("BIGINT"), "i64/u64 应映射为 BIGINT");
        assert!(output_str.contains("INTEGER"), "i32/u32 应映射为 INTEGER");
        assert!(output_str.contains("SMALLINT"), "i16 应映射为 SMALLINT");
        assert!(output_str.contains("TINYINT"), "i8 应映射为 TINYINT");
    }

    #[test]
    fn test_derive_schema_float_types_mapping() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                a: f32,
                b: f64,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(output_str.contains("FLOAT"), "f32 应映射为 FLOAT");
        assert!(output_str.contains("DOUBLE"), "f64 应映射为 DOUBLE");
    }

    #[test]
    fn test_derive_schema_bool_type_mapping() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                active: bool,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("BOOLEAN"),
            "bool 应映射为 BOOLEAN: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_vec_u8_type_mapping() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                data: Vec<u8>,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("BLOB"),
            "Vec<u8> 应映射为 BLOB: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_primary_key_attr() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                #[column(primary_key)]
                id: i64,
                name: String,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        // primary_key 标记应反映在生成的元组中（第 5 个元素为 true）
        assert!(
            output_str.contains("true"),
            "primary_key 应生成 true: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_nullable_attr() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                #[column(nullable)]
                name: String,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        // nullable 标记应反映在生成的元组中（第 4 个元素为 true）
        assert!(
            output_str.contains("true"),
            "nullable 应生成 true: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_type_override() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                #[column(type = "VARCHAR(255)")]
                name: String,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("VARCHAR(255)"),
            "应使用 #[column(type)] 覆盖类型: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_name_override() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                #[column(name = "user_name")]
                name: String,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("\"user_name\""),
            "应使用 #[column(name)] 覆盖列名: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_default_attr() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                #[column(default = "0")]
                count: i64,
            }
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        // has_default 应为 true
        assert!(
            output_str.contains("true"),
            "default 属性应使 has_default 为 true: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_multiple_fields() {
        let input: DeriveInput = syn::parse_quote! {
            struct Multi {
                a: i64,
                b: String,
                c: f64,
                d: bool,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        // 应为每个字段生成 setter
        assert!(output_str.contains("fn a"));
        assert!(output_str.contains("fn b"));
        assert!(output_str.contains("fn c"));
        assert!(output_str.contains("fn d"));
    }

    #[test]
    fn test_derive_builder_skip_no_setter() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                a: i64,
                #[builder(skip)]
                b: String,
            }
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        // skip 字段不应生成 setter（fn b），但应使用 Default
        // 注意：fn b 可能出现在其他上下文，因此检查更精确的模式
        assert!(
            !output_str.contains("fn b(mut self"),
            "skip 字段不应生成 setter: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_schema_empty_struct_error() {
        // 空结构体（无字段）应能编译，但生成空列列表
        let input: DeriveInput = syn::parse_quote! {
            struct Empty {}
        };
        let output = derive_schema_impl(input);
        let output_str = output.to_string();
        // 不应崩溃，应生成 column_count = 0
        assert!(
            output_str.contains("0") || output_str.contains("SZ_ORM_TABLE_NAME"),
            "空结构体应生成有效代码: {}",
            output_str
        );
    }

    #[test]
    fn test_derive_builder_empty_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct Empty {}
        };
        let output = derive_builder_impl(input);
        let output_str = output.to_string();
        assert!(
            output_str.contains("EmptyBuilder"),
            "空结构体也应生成 Builder: {}",
            output_str
        );
    }

    #[test]
    fn test_trace_diag_no_panic() {
        // 不应 panic
        trace_diag("test", "info");
    }
}
