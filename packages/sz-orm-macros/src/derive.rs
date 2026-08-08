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
use quote::{format_ident, quote, ToTokens};
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
    let table_name =
        parse_table_attr(&input.attrs).unwrap_or_else(|| to_snake_case(&struct_name.to_string()));

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
// `#[derive(GraphQLModel)]` — auto-generate `impl GraphQLModelInfo`
// ---------------------------------------------------------------------------

/// `#[derive(GraphQLModel)]` 派生宏入口
///
/// 从结构体字段提取元数据，生成 `sz_orm_graphql::schema_gen::GraphQLModelInfo` 实现。
/// 复用 `#[table(name = "...")]` 和 `#[column(skip)]` 属性。
pub fn derive_graphql_model_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag(
        "derive(GraphQLModel)",
        &format!("target struct: {}", input.ident),
    );

    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn_error_to_compile_error(syn::Error::new_spanned(
                    struct_name,
                    "GraphQLModel 仅支持命名字段结构体",
                ))
            }
        },
        _ => {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                struct_name,
                "GraphQLModel 仅支持 struct，不支持 enum / union",
            ))
        }
    };

    let table_name =
        parse_table_attr(&input.attrs).unwrap_or_else(|| to_snake_case(&struct_name.to_string()));

    let mut column_entries = Vec::new();
    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap().to_string();
        let col_attr = parse_column_attr(&field.attrs);
        if col_attr.skip {
            continue;
        }
        let col_name = col_attr.name.clone().unwrap_or_else(|| field_name.clone());
        let rust_type_str = quote!(#field.ty).to_string().replace(" ", "");
        let nullable = col_attr.nullable || is_option_type(&field.ty).is_some();

        column_entries.push(quote! {
            sz_orm_graphql::schema_gen::ColumnMeta {
                name: #col_name.to_string(),
                rust_type: #rust_type_str.to_string(),
                nullable: #nullable,
            }
        });
    }

    let expanded = quote! {
        impl sz_orm_graphql::schema_gen::GraphQLModelInfo for #struct_name {
            fn table_name() -> &'static str {
                #table_name
            }
            fn columns() -> Vec<sz_orm_graphql::schema_gen::ColumnMeta> {
                vec![#(#column_entries),*]
            }
        }
    };

    expanded
}

// ---------------------------------------------------------------------------
// `#[derive(FromQueryResult)]` — auto-generate `impl FromQueryResult for Struct`
// ---------------------------------------------------------------------------

/// 判断类型是否为 `Option<T>`，若是则返回内层类型 `T`
fn extract_option_inner(ty: &syn::Type) -> Option<&syn::Type> {
    if let syn::Type::Path(type_path) = ty {
        let seg = type_path.path.segments.last()?;
        if seg.ident == "Option" {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner);
                }
            }
        }
    }
    None
}

/// `#[derive(FromQueryResult)]` 派生宏入口
///
/// 为结构体自动生成 `FromQueryResult` trait 实现，
/// 从查询结果行（`HashMap<String, Value>`）反序列化为结构体实例。
///
/// 支持字段级 `#[column(name = "...")]` 覆盖列名映射。
/// `Option<T>` 字段在列缺失或值为 NULL 时自动返回 `None`。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::FromQueryResult;
///
/// #[derive(FromQueryResult)]
/// struct User {
///     id: i64,
///     name: String,
///     #[column(name = "user_email")]
///     email: Option<String>,
/// }
/// ```
pub fn derive_from_query_result_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag(
        "derive(FromQueryResult)",
        &format!("target struct: {}", input.ident),
    );

    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn_error_to_compile_error(syn::Error::new_spanned(
                    struct_name,
                    "FromQueryResult 仅支持命名字段结构体（struct Foo { a: T }）",
                ))
            }
        },
        _ => {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                struct_name,
                "FromQueryResult 仅支持 struct，不支持 enum / union",
            ))
        }
    };

    let mut field_inits = Vec::new();
    let mut col_names: Vec<String> = Vec::new();
    let mut col_type_pairs: Vec<TokenStream2> = Vec::new();

    for field in fields.iter() {
        let field_ident = field.ident.as_ref().unwrap();
        let field_name = field_ident.to_string();
        let col_attr = parse_column_attr(&field.attrs);
        let col_name = col_attr.name.clone().unwrap_or(field_name.clone());
        col_names.push(col_name.clone());
        let field_ty = &field.ty;
        let sql_type = rust_type_to_sql_type(field_ty);
        col_type_pairs.push(quote! { (#col_name, #sql_type) });
        let is_option = extract_option_inner(field_ty).is_some();

        if is_option {
            // Option<T> 字段：列缺失或 NULL 时返回 None
            field_inits.push(quote! {
                #field_ident: {
                    match row.get(#col_name) {
                        Some(::sz_orm_core::Value::Null) | None => ::std::option::Option::None,
                        Some(v) => {
                            <#field_ty as ::sz_orm_core::FromQueryResult>::from_value(v)
                                .map_err(|e| format!("字段 `{}`: {}", #col_name, e))?
                        }
                    }
                }
            });
        } else {
            // 非 Option 字段：列缺失时直接报错
            field_inits.push(quote! {
                #field_ident: {
                    match row.get(#col_name) {
                        Some(v) => {
                            <#field_ty as ::sz_orm_core::FromQueryResult>::from_value(v)
                                .map_err(|e| format!("字段 `{}`: {}", #col_name, e))?
                        }
                        None => {
                            return ::std::result::Result::Err(
                                format!("字段 `{}` 在结果行中不存在", #col_name)
                            );
                        }
                    }
                }
            });
        }
    }

    let expanded = quote! {
        impl ::sz_orm_core::FromQueryResult for #struct_name {
            fn from_query_result(
                row: &std::collections::HashMap<String, ::sz_orm_core::Value>,
            ) -> ::std::result::Result<Self, ::std::string::String> {
                ::std::result::Result::Ok(#struct_name {
                    #(#field_inits,)*
                })
            }

            fn row_desc() -> Vec<&'static str> {
                vec![#(#col_names),*]
            }

            fn column_types() -> &'static [(&'static str, &'static str)] {
                &[#(#col_type_pairs),*]
            }
        }

        impl #struct_name {
            /// 编译期列类型元数据（const fn，供 `query_as!` 宏在 `db-verify` 模式下
            /// 做编译期列名/列类型交叉验证）。
            ///
            /// 与 trait 方法 `column_types()` 数据一致，但可在 const 上下文中调用，
            /// 使 `query_as!` 生成的 `const _: () = { ... }` 验证块能引用它：
            /// 验证失败时 const panic 直接导致编译失败（编译期拦截类型不匹配）。
            #[doc(hidden)]
            pub const fn __sz_orm_column_types() -> &'static [(&'static str, &'static str)] {
                &[#(#col_type_pairs),*]
            }
        }
    };

    expanded
}

// ---------------------------------------------------------------------------
// `#[derive(FromRow)]` — auto-generate `impl FromRow for Struct`
// ---------------------------------------------------------------------------

/// `#[derive(FromRow)]` 派生宏入口
///
/// 为结构体自动生成 `sz_orm_core::queryable::FromRow` trait 实现，
/// 从 `HashMap<String, Value>` 按列名反序列化为结构体实例。
///
/// 与 `#[derive(FromQueryResult)]` 的区别：
/// - `FromRow` 使用 `QueryError` 错误类型（含列索引/类型信息），适合底层使用
/// - `FromQueryResult` 使用 `String` 错误类型，适合业务层使用
///
/// 支持字段级 `#[column(name = "...")]` 覆盖列名映射。
/// `Option<T>` 字段在列缺失或值为 NULL 时自动返回 `None`。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::FromRow;
///
/// #[derive(FromRow)]
/// struct User {
///     id: i64,
///     name: String,
///     #[column(name = "user_email")]
///     email: Option<String>,
/// }
/// ```
pub fn derive_from_row_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag(
        "derive(FromRow)",
        &format!("target struct: {}", input.ident),
    );

    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn_error_to_compile_error(syn::Error::new_spanned(
                    struct_name,
                    "FromRow 仅支持命名字段结构体（struct Foo { a: T }）",
                ))
            }
        },
        _ => {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                struct_name,
                "FromRow 仅支持 struct，不支持 enum / union",
            ))
        }
    };

    let mut field_inits = Vec::new();

    for field in fields.iter() {
        let field_ident = field.ident.as_ref().unwrap();
        let field_name = field_ident.to_string();
        let col_attr = parse_column_attr(&field.attrs);
        let col_name = col_attr.name.clone().unwrap_or(field_name.clone());
        let field_ty = &field.ty;
        let is_option = extract_option_inner(field_ty).is_some();

        if is_option {
            field_inits.push(quote! {
                #field_ident: {
                    match row.get(#col_name) {
                        Some(::sz_orm_core::Value::Null) | None => ::std::option::Option::None,
                        Some(v) => {
                            <#field_ty as ::sz_orm_core::FromQueryResult>::from_value(v)
                                .map_err(|e| ::sz_orm_core::queryable::QueryError::TypeMismatch {
                                    column: ::std::borrow::Cow::Borrowed(#col_name),
                                    expected: ::std::stringify!(#field_ty),
                                })?
                        }
                    }
                }
            });
        } else {
            field_inits.push(quote! {
                #field_ident: {
                    match row.get(#col_name) {
                        Some(v) => {
                            <#field_ty as ::sz_orm_core::FromQueryResult>::from_value(v)
                                .map_err(|e| ::sz_orm_core::queryable::QueryError::TypeMismatch {
                                    column: ::std::borrow::Cow::Borrowed(#col_name),
                                    expected: ::std::stringify!(#field_ty),
                                })?
                        }
                        None => {
                            return ::std::result::Result::Err(
                                ::sz_orm_core::queryable::QueryError::MissingColumn {
                                    column: #col_name,
                                }
                            );
                        }
                    }
                }
            });
        }
    }

    let expanded = quote! {
        impl ::sz_orm_core::queryable::FromRow for #struct_name {
            fn from_row(
                row: std::collections::HashMap<String, ::sz_orm_core::Value>,
            ) -> ::std::result::Result<Self, ::sz_orm_core::queryable::QueryError> {
                ::std::result::Result::Ok(#struct_name {
                    #(#field_inits,)*
                })
            }
        }
    };

    expanded
}

// ---------------------------------------------------------------------------
// `#[derive(SqlType)]` — auto-generate `impl FromQueryResult + to_value()` for enums
// ---------------------------------------------------------------------------

/// `#[derive(SqlType)]` 派生宏入口
///
/// 为 Rust 枚举自动生成 `sz_orm_core::FromQueryResult` trait 实现（从 `Value` 反序列化）
/// 和 `to_value(&self) -> Value` 方法（序列化到 `Value`）。
///
/// 这是 sz-orm 对 SQLx `#[derive(Type)]` 的等效实现：
/// 让自定义枚举可以直接用于查询结果的字段映射和查询参数的绑定。
///
/// # 支持的属性
///
/// - `#[sql_type(rename_all = "snake_case")]` — 控制变体名的序列化格式
///   - `lowercase` / `UPPERCASE` / `PascalCase` / `camelCase` / `snake_case` / `SCREAMING_SNAKE_CASE`
///   - 默认为 `snake_case`（`Status::Active` → `"active"`）
/// - `#[sql_type(rename = "...")]`（变体级）— 覆盖单个变体的序列化名
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::SqlType;
///
/// #[derive(SqlType)]
/// enum Status {
///     Active,      // → "active"
///     Inactive,    // → "inactive"
///     Pending,     // → "pending"
/// }
///
/// let v = Status::Active.to_value();  // Value::String("active")
/// ```
pub fn derive_sql_type_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag("derive(SqlType)", &format!("target: {}", input.ident));

    let type_name = &input.ident;

    // 解析 #[sql_type(rename_all = "...")]
    let rename_all = parse_sql_type_attr(&input.attrs);

    match &input.data {
        Data::Enum(data) => generate_sql_type_for_enum(type_name, &data.variants, rename_all),
        _ => syn_error_to_compile_error(syn::Error::new_spanned(
            type_name,
            "SqlType 目前仅支持 enum（枚举类型映射到 Value::String）",
        )),
    }
}

/// 解析 `#[sql_type(rename_all = "...")]` 属性
fn parse_sql_type_attr(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("sql_type") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename_all") {
                    let value = meta.value()?;
                    let _lit: syn::LitStr = value.parse()?;
                    return Ok(());
                }
                Ok(())
            })
            .ok();
            // 简化：直接从 attr tokens 提取字符串
            let tokens = attr.meta.require_list().ok()?.to_token_stream().to_string();
            if let Some(start) = tokens.find('"') {
                if let Some(end) = tokens[start + 1..].find('"') {
                    return Some(tokens[start + 1..start + 1 + end].to_string());
                }
            }
        }
    }
    None
}

/// 将变体名按 rename_all 规则转换
fn apply_rename(name: &str, rule: Option<&str>) -> String {
    match rule.unwrap_or("snake_case") {
        "snake_case" => to_snake_case(name),
        "SCREAMING_SNAKE_CASE" => to_snake_case(name).to_uppercase(),
        "camelCase" => {
            let s = to_snake_case(name);
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => {
                    let mut result = first.to_lowercase().to_string();
                    let mut prev_underscore = false;
                    for c in chars {
                        if c == '_' {
                            prev_underscore = true;
                            continue;
                        }
                        result.push(if prev_underscore {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        });
                        prev_underscore = false;
                    }
                    result
                }
                None => s,
            }
        }
        "PascalCase" => {
            let mut result = String::new();
            let mut upper_next = true;
            for c in name.chars() {
                if c == '_' {
                    upper_next = true;
                    continue;
                }
                result.push(if upper_next {
                    c.to_ascii_uppercase()
                } else {
                    c
                });
                upper_next = false;
            }
            result
        }
        "lowercase" => name.to_lowercase(),
        "UPPERCASE" => name.to_uppercase(),
        _ => name.to_string(),
    }
}

/// 为枚举生成 SqlType 实现
fn generate_sql_type_for_enum(
    type_name: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    rename_all: Option<String>,
) -> TokenStream2 {
    // 仅支持单元变体（无字段）
    let mut match_arms_from: Vec<TokenStream2> = Vec::new();
    let mut match_arms_to: Vec<TokenStream2> = Vec::new();

    for variant in variants.iter() {
        // 只支持单元变体（无字段）
        if !matches!(variant.fields, syn::Fields::Unit) {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                &variant.fields,
                "SqlType 枚举仅支持单元变体（如 Active，不支持 Active(String)）",
            ));
        }

        let var_ident = &variant.ident;
        let var_name = var_ident.to_string();

        // 检查变体级 #[sql_type(rename = "...")]
        let serialized_name = parse_sql_type_variant_rename(&variant.attrs)
            .unwrap_or_else(|| apply_rename(&var_name, rename_all.as_deref()));

        match_arms_from.push(quote! {
            #serialized_name => ::std::result::Result::Ok(#type_name::#var_ident),
        });

        match_arms_to.push(quote! {
            #type_name::#var_ident => ::sz_orm_core::Value::String(#serialized_name.to_string()),
        });
    }

    let expanded = quote! {
        impl ::sz_orm_core::FromQueryResult for #type_name {
            fn from_value(value: &::sz_orm_core::Value) -> ::std::result::Result<Self, ::std::string::String> {
                match value {
                    ::sz_orm_core::Value::String(s) => {
                        match s.as_str() {
                            #(#match_arms_from)*
                            other => ::std::result::Result::Err(
                                ::std::format!("无法将 '{}' 反序列化为 {}", other, ::std::stringify!(#type_name))
                            ),
                        }
                    }
                    ::sz_orm_core::Value::Null => ::std::result::Result::Err(
                        "NULL 值不能反序列化为非 Option 枚举类型".to_string()
                    ),
                    other => ::std::result::Result::Err(
                        ::std::format!("期望 String 类型，实际得到 {:?}", other)
                    ),
                }
            }
        }

        impl #type_name {
            /// 将枚举值转换为 `Value`，用于查询参数绑定
            pub fn to_value(&self) -> ::sz_orm_core::Value {
                match self {
                    #(#match_arms_to)*
                }
            }
        }
    };

    expanded
}

/// 解析变体级 `#[sql_type(rename = "...")]`
fn parse_sql_type_variant_rename(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("sql_type") {
            let tokens = attr.meta.require_list().ok()?.to_token_stream().to_string();
            if let Some(start) = tokens.find('"') {
                if let Some(end) = tokens[start + 1..].find('"') {
                    return Some(tokens[start + 1..start + 1 + end].to_string());
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// `#[derive(Entity)]` — auto-generate `impl Model for Struct`
// ---------------------------------------------------------------------------

/// `#[derive(Entity)]` 派生宏入口
///
/// 为目标结构体自动生成 `sz_orm_core::Model` trait 实现：
/// - `type PrimaryKey` — 由 `#[column(primary_key)]` 字段的类型决定
/// - `fn table_name()` — 由 `#[table(name = "...")]` 决定，默认蛇形结构体名
/// - `fn pk_name()` — 主键列名，默认 `"id"`
/// - `fn pk(&self)` / `fn set_pk(&mut self, pk)` — 主键读写
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::Entity;
///
/// #[derive(Entity)]
/// #[table(name = "users")]
/// struct User {
///     #[column(primary_key)]
///     id: i64,
///     name: String,
/// }
/// ```
pub fn derive_entity_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag("derive(Entity)", &format!("target struct: {}", input.ident));

    let struct_name = &input.ident;

    // 仅支持命名字段结构体
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn_error_to_compile_error(syn::Error::new_spanned(
                    struct_name,
                    "Entity 仅支持命名字段结构体（struct Foo { a: T }）",
                ))
            }
        },
        _ => {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                struct_name,
                "Entity 仅支持 struct，不支持 enum / union",
            ))
        }
    };

    // 表名
    let table_name =
        parse_table_attr(&input.attrs).unwrap_or_else(|| to_snake_case(&struct_name.to_string()));

    // 找主键字段
    let mut pk_field_ident = None;
    let mut pk_field_col_name = None;
    let mut pk_field_ty = None;

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap().to_string();
        let col_attr = parse_column_attr(&field.attrs);
        if col_attr.primary_key {
            pk_field_ident = Some(field.ident.as_ref().unwrap().clone());
            pk_field_col_name = Some(col_attr.name.clone().unwrap_or(field_name));
            pk_field_ty = Some(field.ty.clone());
        }
    }

    // 未指定主键时的错误
    let (pk_ident, pk_col_name, pk_ty) = match (pk_field_ident, pk_field_col_name, pk_field_ty) {
        (Some(ident), Some(col), Some(ty)) => (ident, col, ty),
        _ => {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                struct_name,
                "Entity 需要恰好一个 #[column(primary_key)] 字段",
            ))
        }
    };

    trace_diag(
        "derive(Entity)",
        &format!("pk = {} ({})", pk_col_name, quote!(#pk_ty)),
    );

    let pk_col_name_lit = proc_macro2::Literal::string(&pk_col_name);

    // G-SO-2：自动为 #[derive(Entity)] 生成 <StructName>Column 枚举（ColumnTrait），
    // 无需用户额外添加 #[derive(ColumnEnum)]。跳过 #[column(skip)] 字段。
    let mut col_variants: Vec<syn::Ident> = Vec::new();
    let mut col_as_str_arms: Vec<TokenStream2> = Vec::new();
    for field in fields.iter() {
        let field_ident = field.ident.as_ref().unwrap();
        let col_attr = parse_column_attr(&field.attrs);
        if col_attr.skip {
            continue;
        }
        let col_name = col_attr
            .name
            .clone()
            .unwrap_or_else(|| field_ident.to_string());
        let variant = syn::Ident::new(
            &snake_to_camel(&field_ident.to_string()),
            field_ident.span(),
        );
        col_variants.push(variant.clone());
        col_as_str_arms.push(quote! { Self::#variant => #col_name, });
    }
    let enum_name = syn::Ident::new(&format!("{}Column", struct_name), struct_name.span());
    let column_enum_impl = if col_variants.is_empty() {
        quote! {}
    } else {
        quote! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            #[allow(dead_code)]
            pub enum #enum_name {
                #(#col_variants),*
            }

            impl ::sz_orm_core::ColumnTrait for #enum_name {
                fn as_str(&self) -> &'static str {
                    match self { #(#col_as_str_arms)* }
                }
                fn all() -> Vec<Self> { vec![#(Self::#col_variants),*] }
            }

            impl std::fmt::Display for #enum_name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", ::sz_orm_core::ColumnTrait::as_str(self))
                }
            }
        }
    };

    let expanded = quote! {
        #[allow(clippy::wrong_self_convention)]
        impl ::sz_orm_core::Model for #struct_name {
            type PrimaryKey = #pk_ty;

            fn table_name() -> &'static str {
                #table_name
            }

            fn pk_name() -> &'static str {
                #pk_col_name_lit
            }

            fn pk(&self) -> Self::PrimaryKey {
                self.#pk_ident.clone()
            }

            fn set_pk(&mut self, pk: Self::PrimaryKey) {
                self.#pk_ident = pk;
            }
        }

        #column_enum_impl
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
    trace_diag(
        "derive(Builder)",
        &format!("target struct: {}", input.ident),
    );

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
// `#[derive(Relation)]` — auto-generate `impl ModelExt` with relations()
// ---------------------------------------------------------------------------

/// 关系属性解析结果
#[derive(Default)]
struct RelationAttr {
    kind: Option<String>, // has_many / belongs_to / has_one / belongs_to_many / morph_many / morph_to
    model: Option<String>, // 关联模型名（运行时字符串值）
    fk: Option<String>,   // 外键列名
    pk: Option<String>,   // 关联主键列名（默认 "id"）
    other_key: Option<String>, // 多对多：中间表另一侧键
    junction: Option<String>, // 多对多：中间表名
    target: Option<String>, // 多对多：目标模型
    target_pk: Option<String>, // 多对多：目标主键
    morph_type: Option<String>, // 多态：类型列
    morph_id: Option<String>, // 多态：ID 列
    morph_type_value: Option<String>, // 多态：类型标识值
}

/// 解析 `#[relation(...)]` 属性
fn parse_relation_attr(attrs: &[Attribute]) -> Vec<RelationAttr> {
    let mut results = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("relation") {
            continue;
        }
        let tokens = match &attr.meta {
            syn::Meta::List(list) => &list.tokens,
            _ => continue,
        };
        // 手动解析逗号分隔的 key[ = value] 对，避免 parse_nested_meta 对
        // `key = "string"` 语法的限制（它把 = 右侧当作表达式，导致 "Post" 被当作路径）
        let mut attr_obj = RelationAttr::default();
        let mut found = false;
        let parse_result: Result<(), syn::Error> = (|| {
            let stream = tokens.clone();
            let mut cursor = stream.into_iter().peekable();
            while let Some(tok) = cursor.next() {
                let key = match tok {
                    proc_macro2::TokenTree::Ident(id) => id.to_string(),
                    proc_macro2::TokenTree::Punct(p) if p.as_char() == ',' => continue,
                    other => {
                        return Err(syn::Error::new(other.span(), "expected identifier or ','"))
                    }
                };
                // 跳过可选的 '='
                let val_tok = if let Some(proc_macro2::TokenTree::Punct(p)) = cursor.peek() {
                    if p.as_char() == '=' {
                        cursor.next(); // consume '='
                        cursor.next() // consume value
                    } else {
                        None
                    }
                } else {
                    None
                };
                match key.as_str() {
                    "has_many" | "belongs_to" | "has_one" | "belongs_to_many" | "morph_many"
                    | "morph_to" => {
                        attr_obj.kind = Some(key.to_string());
                        if let Some(v) = val_tok {
                            attr_obj.model = Some(match v {
                                proc_macro2::TokenTree::Ident(id) => id.to_string(),
                                proc_macro2::TokenTree::Literal(lit) => {
                                    let s = lit.to_string();
                                    if s.starts_with('"') && s.ends_with('"') {
                                        s[1..s.len() - 1].to_string()
                                    } else {
                                        return Err(syn::Error::new(
                                            lit.span(),
                                            "expected string literal or identifier",
                                        ));
                                    }
                                }
                                other => {
                                    return Err(syn::Error::new(
                                        other.span(),
                                        "expected string literal or identifier",
                                    ))
                                }
                            });
                        }
                        found = true;
                    }
                    "fk" | "foreign_key" => {
                        attr_obj.fk = val_tok.map(extract_str_lit).transpose()?;
                    }
                    "pk" | "parent_pk" | "child_pk" => {
                        attr_obj.pk = val_tok.map(extract_str_lit).transpose()?;
                    }
                    "other_key" => {
                        attr_obj.other_key = val_tok.map(extract_str_lit).transpose()?;
                    }
                    "junction" | "junction_table" => {
                        attr_obj.junction = val_tok.map(extract_str_lit).transpose()?;
                    }
                    "target" | "target_model" => {
                        attr_obj.target = val_tok.map(extract_str_lit).transpose()?;
                    }
                    "target_pk" => {
                        attr_obj.target_pk = val_tok.map(extract_str_lit).transpose()?;
                    }
                    "morph_type" => {
                        attr_obj.morph_type = val_tok.map(extract_str_lit).transpose()?;
                    }
                    "morph_id" => {
                        attr_obj.morph_id = val_tok.map(extract_str_lit).transpose()?;
                    }
                    "morph_type_value" => {
                        attr_obj.morph_type_value = val_tok.map(extract_str_lit).transpose()?;
                    }
                    _ => {}
                }
            }
            Ok(())
        })();
        if parse_result.is_ok() && found {
            results.push(attr_obj);
        }
    }
    results
}

/// 从 TokenTree 中提取字符串字面量的内容（去引号）
fn extract_str_lit(tok: proc_macro2::TokenTree) -> Result<String, syn::Error> {
    match tok {
        proc_macro2::TokenTree::Literal(lit) => {
            let s = lit.to_string();
            if s.starts_with('"') && s.ends_with('"') {
                Ok(s[1..s.len() - 1].to_string())
            } else {
                Err(syn::Error::new(lit.span(), "expected string literal"))
            }
        }
        other => Err(syn::Error::new(other.span(), "expected string literal")),
    }
}

/// 从关系名（Rust 字段名）推断默认外键列名
///
/// 规则：`orders` → `order_id`，`user_profile` → `user_profile_id`
fn infer_fk_from_name(name: &str) -> String {
    // 去掉尾部 's'（复数形式），加 `_id`
    let stem = name.strip_suffix('s').unwrap_or(name);
    format!("{}_id", stem)
}

/// `#[derive(Relation)]` 派生宏入口
///
/// 自动生成 `impl ModelExt for Struct`，填充 `relations()` 映射。
/// 每个 `#[relation(...)]` 属性对应一条关系定义。
///
/// # 支持的属性格式
///
/// ```ignore
/// #[relation(has_many = "orders", fk = "user_id", pk = "id")]
/// #[relation(belongs_to = "users", fk = "user_id", pk = "id")]
/// #[relation(has_one = "profiles", fk = "user_id", pk = "id")]
/// #[relation(belongs_to_many = "roles", junction = "user_roles",
///            fk = "user_id", other_key = "role_id", target = "roles", target_pk = "id")]
/// #[relation(morph_many = "comments", morph_type = "commentable_type",
///            morph_id = "commentable_id", morph_type_value = "Post")]
/// #[relation(morph_to, morph_type = "commentable_type", morph_id = "commentable_id")]
/// ```
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::{Entity, Relation};
///
/// #[derive(Entity, Relation)]
/// #[table(name = "users")]
/// struct User {
///     #[column(primary_key)]
///     id: i64,
/// }
///
/// // 自动生成 relations() 包含 "orders" → HasMany { ... }
/// ```
pub fn derive_relation_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag(
        "derive(Relation)",
        &format!("target struct: {}", input.ident),
    );

    let struct_name = &input.ident;
    let rel_attrs = parse_relation_attr(&input.attrs);

    let table_name =
        parse_table_attr(&input.attrs).unwrap_or_else(|| to_snake_case(&struct_name.to_string()));

    // 从结构体字段提取列名（用于 columns()/fillable()）
    let fields = match &input.data {
        syn::Data::Struct(s) => &s.fields,
        _ => &syn::Fields::Named(syn::FieldsNamed {
            brace_token: Default::default(),
            named: Default::default(),
        }),
    };
    let col_idents: Vec<&syn::Ident> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
    let col_strs: Vec<String> = col_idents.iter().map(|id| id.to_string()).collect();
    // fillable = 所有非主键字段
    let pk_name = fields
        .iter()
        .find(|f| parse_column_attr(&f.attrs).primary_key)
        .and_then(|f| f.ident.as_ref())
        .map(|id| id.to_string());
    let fillable_strs: Vec<String> = col_strs
        .iter()
        .filter(|s| pk_name.as_ref().map(|pk| *s != pk).unwrap_or(true))
        .cloned()
        .collect();

    let mut map_inserts = Vec::new();
    for (i, attr) in rel_attrs.iter().enumerate() {
        // rel_name 用作 HashMap<&'static str, Relation> 的 key；
        // 必须用 LitStr 生成字符串字面量（Ident 会产生变量引用导致 E0425）
        let rel_name_str = attr
            .model
            .clone()
            .unwrap_or_else(|| format!("relation_{}", i));
        let rel_name_lit = syn::LitStr::new(&rel_name_str, proc_macro2::Span::call_site());
        let kind = attr.kind.as_deref().unwrap_or("has_many");
        let fk_default = infer_fk_from_name(match kind {
            "has_many" => "orders",
            "has_one" => "profile",
            _ => "fk",
        });
        let fk = attr.fk.clone().unwrap_or_else(|| fk_default.clone());
        let pk = attr.pk.clone().unwrap_or_else(|| "id".to_string());
        let rel_expr = match kind {
            "has_many" => {
                let child = attr.model.clone().unwrap_or_else(|| table_name.clone());
                quote! {
                    ::sz_orm_core::Relation::HasMany(::sz_orm_core::HasMany {
                        foreign_key: #fk.to_string(),
                        child_model: #child.to_string(),
                        child_pk: #pk.to_string(),
                    })
                }
            }
            "belongs_to" => {
                let parent = attr.model.clone().unwrap_or_else(|| "parent".to_string());
                let fk_bt = attr.fk.clone().unwrap_or_else(|| "parent_id".to_string());
                quote! {
                    ::sz_orm_core::Relation::BelongsTo(::sz_orm_core::BelongsTo {
                        foreign_key: #fk_bt.to_string(),
                        parent_model: #parent.to_string(),
                        parent_pk: #pk.to_string(),
                    })
                }
            }
            "has_one" => {
                let child = attr.model.clone().unwrap_or_else(|| "child".to_string());
                quote! {
                    ::sz_orm_core::Relation::HasOne(::sz_orm_core::HasOne {
                        foreign_key: #fk.to_string(),
                        child_model: #child.to_string(),
                        child_pk: #pk.to_string(),
                    })
                }
            }
            "belongs_to_many" => {
                let junction = attr
                    .junction
                    .clone()
                    .unwrap_or_else(|| "junction".to_string());
                let other = attr
                    .other_key
                    .clone()
                    .unwrap_or_else(|| "other_key".to_string());
                let target = attr.target.clone().unwrap_or_else(|| "target".to_string());
                let target_pk = attr.target_pk.clone().unwrap_or_else(|| "id".to_string());
                quote! {
                    ::sz_orm_core::Relation::BelongsToMany(::sz_orm_core::BelongsToMany {
                        junction_table: #junction.to_string(),
                        foreign_key: #fk.to_string(),
                        other_key: #other.to_string(),
                        target_model: #target.to_string(),
                        target_pk: #target_pk.to_string(),
                    })
                }
            }
            "morph_many" => {
                let child = attr.model.clone().unwrap_or_else(|| "child".to_string());
                let mt = attr
                    .morph_type
                    .clone()
                    .unwrap_or_else(|| "morph_type".to_string());
                let mi = attr
                    .morph_id
                    .clone()
                    .unwrap_or_else(|| "morph_id".to_string());
                let mtv = attr
                    .morph_type_value
                    .clone()
                    .unwrap_or_else(|| "Parent".to_string());
                quote! {
                    ::sz_orm_core::Relation::MorphMany(::sz_orm_core::MorphMany {
                        child_model: #child.to_string(),
                        morph_type_column: #mt.to_string(),
                        morph_id_column: #mi.to_string(),
                        morph_type_value: #mtv.to_string(),
                    })
                }
            }
            "morph_to" => {
                let mt = attr
                    .morph_type
                    .clone()
                    .unwrap_or_else(|| "morph_type".to_string());
                let mi = attr
                    .morph_id
                    .clone()
                    .unwrap_or_else(|| "morph_id".to_string());
                quote! {
                    ::sz_orm_core::Relation::MorphTo(::sz_orm_core::MorphTo {
                        morph_type_column: #mt.to_string(),
                        morph_id_column: #mi.to_string(),
                    })
                }
            }
            _ => continue,
        };
        map_inserts.push(quote! {
            map.insert(#rel_name_lit, #rel_expr);
        });
    }

    let expanded = quote! {
        impl ::sz_orm_core::ModelExt for #struct_name {
            fn columns() -> Vec<&'static str> {
                vec![#(#col_strs),*]
            }

            fn fillable() -> Vec<&'static str> {
                vec![#(#fillable_strs),*]
            }

            fn relations() -> std::collections::HashMap<&'static str, ::sz_orm_core::Relation> {
                let mut map = std::collections::HashMap::new();
                #(#map_inserts)*
                map
            }
        }
    };

    expanded
}

// ---------------------------------------------------------------------------
// `#[derive(RelationTrait)]` — 自动生成 RelationTrait 实现（P-F-2, v2.1.0）
// ---------------------------------------------------------------------------

/// 从 `#[derive(Relation)]` 的 `#[relation(...)]` 属性生成 `RelationTrait` 实现
///
/// 与 `derive_relation_impl` 共享 `parse_relation_attr`，但生成 `RelationDef` 静量表
/// 而非 `HashMap<Relation>`，零分配、编译期常量。
pub fn derive_relation_trait_impl(input: DeriveInput) -> TokenStream2 {
    trace_diag(
        "derive(RelationTrait)",
        &format!("target struct: {}", input.ident),
    );

    let struct_name = &input.ident;
    let rel_attrs = parse_relation_attr(&input.attrs);

    let table_name =
        parse_table_attr(&input.attrs).unwrap_or_else(|| to_snake_case(&struct_name.to_string()));

    let mut relation_defs = Vec::new();
    for (i, attr) in rel_attrs.iter().enumerate() {
        let rel_name_str = attr
            .model
            .clone()
            .unwrap_or_else(|| format!("relation_{}", i));
        let kind = attr.kind.as_deref().unwrap_or("has_many");
        let pk = attr.pk.clone().unwrap_or_else(|| "id".to_string());

        let (relation_kind, to_entity, from_key, to_key) = match kind {
            "has_many" => {
                let child = attr.model.clone().unwrap_or_else(|| table_name.clone());
                let fk = attr.fk.clone().unwrap_or_else(|| "fk".to_string());
                (
                    quote! { ::sz_orm_core::relation_trait::RelationKind::HasMany },
                    child,
                    pk,
                    fk,
                )
            }
            "belongs_to" => {
                let parent = attr.model.clone().unwrap_or_else(|| "parent".to_string());
                let fk_bt = attr.fk.clone().unwrap_or_else(|| "parent_id".to_string());
                (
                    quote! { ::sz_orm_core::relation_trait::RelationKind::BelongsTo },
                    parent,
                    fk_bt,
                    pk,
                )
            }
            "has_one" => {
                let child = attr.model.clone().unwrap_or_else(|| "child".to_string());
                let fk = attr.fk.clone().unwrap_or_else(|| "fk".to_string());
                (
                    quote! { ::sz_orm_core::relation_trait::RelationKind::HasOne },
                    child,
                    pk,
                    fk,
                )
            }
            "belongs_to_many" => {
                let target = attr.target.clone().unwrap_or_else(|| "target".to_string());
                let other = attr
                    .other_key
                    .clone()
                    .unwrap_or_else(|| "other_key".to_string());
                (
                    quote! { ::sz_orm_core::relation_trait::RelationKind::ManyToMany },
                    target,
                    pk,
                    other,
                )
            }
            _ => continue,
        };

        relation_defs.push(quote! {
            ::sz_orm_core::relation_trait::RelationDef::new(
                #rel_name_str,
                #table_name,
                #to_entity,
                #from_key,
                #to_key,
                #relation_kind,
            )
        });
    }

    let relation_count = relation_defs.len();

    let relations_ident = syn::Ident::new(
        &format!(
            "__RELATIONS_{}",
            struct_name.to_string().to_ascii_uppercase()
        ),
        proc_macro2::Span::call_site(),
    );

    let expanded = if relation_count == 0 {
        quote! {}
    } else {
        quote! {
            #[allow(dead_code)]
            static #relations_ident: &[::sz_orm_core::relation_trait::RelationDef] = &[
                #(#relation_defs),*
            ];

            impl ::sz_orm_core::relation_trait::RelationTrait for #struct_name {
                fn def(&self) -> &'static ::sz_orm_core::relation_trait::RelationDef {
                    &#relations_ident[0]
                }

                fn all_relations() -> &'static [::sz_orm_core::relation_trait::RelationDef] {
                    #relations_ident
                }
            }
        }
    };

    expanded
}

// ---------------------------------------------------------------------------

/// snake_case 字段名 → CamelCase 变体名（`user_id` → `UserId`，`id` → `Id`）
fn snake_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for c in s.chars() {
        if c == '_' {
            upper_next = true;
        } else if upper_next {
            out.push(c.to_ascii_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `#[derive(ColumnEnum)]` 派生宏入口（P2-2）
///
/// 从结构体命名字段自动生成 `<StructName>Column` 枚举：
/// - 每个字段一个变体（snake_case → CamelCase）；
/// - `#[column(name = "...")]` 覆盖列名（与 `#[derive(FromQueryResult)]` 一致）；
/// - 实现 `ColumnTrait`（`as_str` / `all`）与 `Display`。
///
/// # 示例
///
/// ```rust,ignore
/// use sz_orm_macros::ColumnEnum;
/// use sz_orm_core::ColumnTrait;
///
/// #[derive(ColumnEnum)]
/// struct User {
///     id: i64,
///     #[column(name = "user_name")]
///     name: String,
/// }
///
/// assert_eq!(UserColumn::Id.as_str(), "id");
/// assert_eq!(UserColumn::Name.as_str(), "user_name");
/// ```
pub fn derive_column_enum_impl(input: DeriveInput) -> TokenStream2 {
    let struct_name = &input.ident;
    let enum_name = syn::Ident::new(&format!("{}Column", struct_name), struct_name.span());

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn_error_to_compile_error(syn::Error::new_spanned(
                    struct_name,
                    "ColumnEnum 仅支持命名字段结构体（struct Foo { a: T }）",
                ))
            }
        },
        _ => {
            return syn_error_to_compile_error(syn::Error::new_spanned(
                struct_name,
                "ColumnEnum 仅支持 struct，不支持 enum / union",
            ))
        }
    };

    let mut variants: Vec<syn::Ident> = Vec::new();
    let mut as_str_arms: Vec<TokenStream2> = Vec::new();
    for field in fields {
        let field_ident = field.ident.as_ref().unwrap();
        let variant = syn::Ident::new(
            &snake_to_camel(&field_ident.to_string()),
            field_ident.span(),
        );
        let col_attr = parse_column_attr(&field.attrs);
        let col_name = col_attr
            .name
            .clone()
            .unwrap_or_else(|| field_ident.to_string());
        variants.push(variant.clone());
        as_str_arms.push(quote! {
            Self::#variant => #col_name,
        });
    }

    let expanded = quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(dead_code)]
        pub enum #enum_name {
            #(#variants),*
        }

        impl ::sz_orm_core::ColumnTrait for #enum_name {
            fn as_str(&self) -> &'static str {
                match self {
                    #(#as_str_arms)*
                }
            }

            fn all() -> Vec<Self> {
                vec![#(Self::#variants),*]
            }
        }

        impl std::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", ::sz_orm_core::ColumnTrait::as_str(self))
            }
        }
    };

    expanded
}
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

    // ---- derive_entity_impl ----

    #[test]
    fn test_derive_entity_basic() {
        let input: DeriveInput = syn::parse_quote! {
            #[table(name = "users")]
            struct User {
                #[column(primary_key)]
                id: i64,
                name: String,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(s.contains("Model"), "应生成 Model trait impl: {}", s);
        assert!(s.contains("PrimaryKey"), "应生成 PrimaryKey 类型: {}", s);
        assert!(s.contains("table_name"), "应生成 table_name: {}", s);
        assert!(s.contains("pk_name"), "应生成 pk_name: {}", s);
        assert!(s.contains("fn pk"), "应生成 pk 方法: {}", s);
        assert!(s.contains("fn set_pk"), "应生成 set_pk 方法: {}", s);
    }

    #[test]
    fn test_derive_entity_default_table_name() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserAccount {
                #[column(primary_key)]
                id: i64,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(s.contains("user_account"), "默认表名应为蛇形: {}", s);
    }

    #[test]
    fn test_derive_entity_pk_column_name() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key, name = "user_id")]
                id: i64,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(s.contains("user_id"), "pk_name 应使用覆盖列名: {}", s);
    }

    #[test]
    fn test_derive_entity_pk_type_i64() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key)]
                id: i64,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(s.contains("i64"), "PrimaryKey 应为 i64: {}", s);
    }

    #[test]
    fn test_derive_entity_pk_type_string() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key)]
                id: String,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(s.contains("String"), "PrimaryKey 应为 String: {}", s);
    }

    #[test]
    fn test_derive_entity_rejects_enum() {
        let input: DeriveInput = syn::parse_quote! {
            enum Foo { A, B }
        };
        let output = derive_entity_impl(input);
        assert!(
            output.to_string().contains("compile_error"),
            "enum 应触发编译错误: {}",
            output
        );
    }

    #[test]
    fn test_derive_entity_rejects_tuple_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo(i64);
        };
        let output = derive_entity_impl(input);
        assert!(
            output.to_string().contains("compile_error"),
            "元组结构体应触发编译错误: {}",
            output
        );
    }

    #[test]
    fn test_derive_entity_rejects_no_pk() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo {
                name: String,
            }
        };
        let output = derive_entity_impl(input);
        assert!(
            output.to_string().contains("compile_error"),
            "无主键字段应触发编译错误: {}",
            output
        );
    }

    #[test]
    fn test_derive_entity_pk_setter_getter() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key)]
                id: i64,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("self . id . clone ()") || s.contains("self.id.clone()"),
            "pk 应返回 self.id.clone(): {}",
            s
        );
        assert!(
            s.contains("self . id = pk") || s.contains("self.id = pk"),
            "set_pk 应设置 self.id: {}",
            s
        );
    }

    // ---- G-SO-2: Entity 自动生成 ColumnEnum ----

    #[test]
    fn test_derive_entity_auto_column_enum() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key)]
                id: i64,
                name: String,
                email: String,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        // ColumnEnum 部分
        assert!(s.contains("UserColumn"), "应生成 UserColumn 枚举: {}", s);
        assert!(s.contains("ColumnTrait"), "应实现 ColumnTrait: {}", s);
        assert!(s.contains("fn as_str"), "应生成 as_str 方法: {}", s);
        assert!(s.contains("fn all"), "应生成 all 方法: {}", s);
        // 变体名（snake → CamelCase）
        assert!(
            s.contains("Id") && s.contains("Name") && s.contains("Email"),
            "变体应为 CamelCase: {}",
            s
        );
    }

    #[test]
    fn test_derive_entity_column_enum_name_override() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key, name = "user_id")]
                id: i64,
                #[column(name = "user_name")]
                name: String,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("user_id"),
            "as_str 应使用覆盖列名 user_id: {}",
            s
        );
        assert!(
            s.contains("user_name"),
            "as_str 应使用覆盖列名 user_name: {}",
            s
        );
    }

    #[test]
    fn test_derive_entity_column_enum_skip() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key)]
                id: i64,
                #[column(skip)]
                password: String,
                name: String,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(!s.contains("Password"), "skip 字段不应出现在枚举: {}", s);
        assert!(s.contains("Name"), "非 skip 字段应出现在枚举: {}", s);
    }

    #[test]
    fn test_derive_entity_column_enum_display() {
        let input: DeriveInput = syn::parse_quote! {
            struct User {
                #[column(primary_key)]
                id: i64,
            }
        };
        let output = derive_entity_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("impl std :: fmt :: Display for UserColumn"),
            "应生成 Display impl: {}",
            s
        );
    }

    // ---- FromQueryResult derive ----

    #[test]
    fn test_derive_from_query_result_basic() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserRow {
                id: i64,
                name: String,
            }
        };
        let output = derive_from_query_result_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("impl :: sz_orm_core :: FromQueryResult for UserRow"),
            "应生成 FromQueryResult impl: {}",
            s
        );
        assert!(s.contains("id"), "应包含 id 字段: {}", s);
        assert!(s.contains("name"), "应包含 name 字段: {}", s);
    }

    #[test]
    fn test_derive_from_query_result_option_field() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserRow {
                id: i64,
                email: Option<String>,
            }
        };
        let output = derive_from_query_result_impl(input);
        let s = output.to_string();
        // Option 字段应匹配 Null 和 None
        assert!(
            s.contains("Value :: Null") || s.contains("Value::Null"),
            "Option 字段应处理 Null: {}",
            s
        );
    }

    #[test]
    fn test_derive_from_query_result_column_name_override() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserRow {
                #[column(name = "user_id")]
                id: i64,
            }
        };
        let output = derive_from_query_result_impl(input);
        let s = output.to_string();
        assert!(s.contains("user_id"), "应使用覆盖列名 user_id: {}", s);
    }

    #[test]
    fn test_derive_from_query_result_rejects_enum() {
        let input: DeriveInput = syn::parse_quote! {
            enum Foo { A, B }
        };
        let output = derive_from_query_result_impl(input);
        let s = output.to_string();
        assert!(s.contains("compile_error !"), "enum 应报错: {}", s);
    }

    #[test]
    fn test_derive_from_query_result_rejects_tuple_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo(i64, String);
        };
        let output = derive_from_query_result_impl(input);
        let s = output.to_string();
        assert!(s.contains("compile_error !"), "tuple struct 应报错: {}", s);
    }

    // ---- FromRow derive ----

    #[test]
    fn test_derive_from_row_basic() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserRow {
                id: i64,
                name: String,
            }
        };
        let output = derive_from_row_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("impl :: sz_orm_core :: queryable :: FromRow for UserRow"),
            "应生成 FromRow impl: {}",
            s
        );
        assert!(s.contains("QueryError"), "应使用 QueryError: {}", s);
        assert!(s.contains("id"), "应包含 id 字段: {}", s);
        assert!(s.contains("name"), "应包含 name 字段: {}", s);
    }

    #[test]
    fn test_derive_from_row_option_field() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserRow {
                id: i64,
                email: Option<String>,
            }
        };
        let output = derive_from_row_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("Value :: Null") || s.contains("Value::Null"),
            "Option 字段应处理 Null: {}",
            s
        );
    }

    #[test]
    fn test_derive_from_row_column_name_override() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserRow {
                id: i64,
                #[column(name = "user_email")]
                email: Option<String>,
            }
        };
        let output = derive_from_row_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("user_email"),
            "应使用覆盖的列名 user_email: {}",
            s
        );
    }

    #[test]
    fn test_derive_from_row_rejects_enum() {
        let input: DeriveInput = syn::parse_quote! {
            enum Foo { Bar, Baz }
        };
        let output = derive_from_row_impl(input);
        let s = output.to_string();
        assert!(s.contains("compile_error !"), "enum 应报错: {}", s);
    }

    #[test]
    fn test_derive_from_row_rejects_tuple_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo(i64, String);
        };
        let output = derive_from_row_impl(input);
        let s = output.to_string();
        assert!(s.contains("compile_error !"), "tuple struct 应报错: {}", s);
    }

    // ---- SqlType derive ----

    #[test]
    fn test_derive_sql_type_basic() {
        let input: DeriveInput = syn::parse_quote! {
            enum Status {
                Active,
                Inactive,
                Pending,
            }
        };
        let output = derive_sql_type_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("impl :: sz_orm_core :: FromQueryResult for Status"),
            "应生成 FromQueryResult impl: {}",
            s
        );
        assert!(s.contains("to_value"), "应生成 to_value 方法: {}", s);
        assert!(
            s.contains("active"),
            "应包含 snake_case 变体名 active: {}",
            s
        );
    }

    #[test]
    fn test_derive_sql_type_rename_all() {
        let input: DeriveInput = syn::parse_quote! {
            #[sql_type(rename_all = "UPPERCASE")]
            enum Priority {
                Low,
                High,
            }
        };
        let output = derive_sql_type_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("LOW") && s.contains("HIGH"),
            "rename_all=UPPERCASE 应生成大写变体名: {}",
            s
        );
    }

    #[test]
    fn test_derive_sql_type_variant_rename() {
        let input: DeriveInput = syn::parse_quote! {
            enum Role {
                #[sql_type(rename = "admin_user")]
                Admin,
                User,
            }
        };
        let output = derive_sql_type_impl(input);
        let s = output.to_string();
        assert!(
            s.contains("admin_user"),
            "变体级 rename 应覆盖为 admin_user: {}",
            s
        );
    }

    #[test]
    fn test_derive_sql_type_rejects_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct Foo { id: i64 }
        };
        let output = derive_sql_type_impl(input);
        let s = output.to_string();
        assert!(s.contains("compile_error !"), "struct 应报错: {}", s);
    }

    #[test]
    fn test_derive_sql_type_rejects_tuple_variant() {
        let input: DeriveInput = syn::parse_quote! {
            enum Foo {
                Bar(i64),
            }
        };
        let output = derive_sql_type_impl(input);
        let s = output.to_string();
        assert!(s.contains("compile_error !"), "带字段变体应报错: {}", s);
    }

    #[test]
    fn test_extract_option_inner_some() {
        let ty: syn::Type = syn::parse_str("Option<String>").unwrap();
        assert!(extract_option_inner(&ty).is_some());
    }

    #[test]
    fn test_extract_option_inner_none() {
        let ty: syn::Type = syn::parse_str("i64").unwrap();
        assert!(extract_option_inner(&ty).is_none());

        let ty2: syn::Type = syn::parse_str("String").unwrap();
        assert!(extract_option_inner(&ty2).is_none());
    }
}
