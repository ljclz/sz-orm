use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Expr, Fields, Lit, Meta};

pub fn derive_validate_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            Fields::Unnamed(_) => {
                return syn::Error::new_spanned(
                    &input,
                    "Validate only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
            Fields::Unit => {
                return syn::Error::new_spanned(
                    &input,
                    "Validate only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Validate only supports structs")
                .to_compile_error()
                .into();
        }
    };

    let mut validations = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();

        for attr in &field.attrs {
            if !attr.path().is_ident("validate") {
                continue;
            }

            let validation_code = match parse_validate_attr(attr, &field_name_str, field_name) {
                Ok(code) => code,
                Err(e) => return e.to_compile_error().into(),
            };
            validations.push(validation_code);
        }
    }

    let expanded = quote! {
        impl sz_orm_core::validation::Validate for #name {
            fn validate(&self) -> Result<(), sz_orm_core::validation::ValidationError> {
                let mut results: Vec<Result<(), sz_orm_core::validation::ValidationError>> = Vec::new();
                #(#validations)*
                sz_orm_core::validation::aggregate(results)
            }
        }
    };

    expanded.into()
}

fn parse_validate_attr(
    attr: &Attribute,
    field_str: &str,
    field_ident: &syn::Ident,
) -> syn::Result<proc_macro2::TokenStream> {
    let meta = attr.meta.require_list()?;
    let nested = meta
        .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)?;

    let mut conditions = Vec::new();
    let mut rule_code = Vec::new();

    for m in nested {
        match &m {
            Meta::Path(path) => {
                let rule_name = path.get_ident().map(|i| i.to_string()).unwrap_or_default();
                match rule_name.as_str() {
                    "email" => {
                        rule_code.push(quote! {
                            results.push(sz_orm_core::validation::rules::validate_email(#field_str, &self.#field_ident));
                        });
                    }
                    "required" => {
                        rule_code.push(quote! {
                            results.push(sz_orm_core::validation::rules::validate_required(#field_str, &self.#field_ident));
                        });
                    }
                    _ => {
                        return Err(syn::Error::new_spanned(
                            path,
                            format!("unknown validation rule: {}", rule_name),
                        ));
                    }
                }
            }
            Meta::NameValue(nv) => {
                let key = nv
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                if key == "when" {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let Lit::Str(lit_str) = &expr_lit.lit {
                            let cond: Expr = syn::parse_str(&lit_str.value())?;
                            conditions.push(cond);
                        }
                    }
                }
            }
            Meta::List(list) => {
                let rule_name = list
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                match rule_name.as_str() {
                    "length" => {
                        let (min, max) = parse_min_max(list)?;
                        rule_code.push(quote! {
                            results.push(sz_orm_core::validation::rules::validate_length(#field_str, &self.#field_ident, #min, #max));
                        });
                    }
                    "range" => {
                        let (min, max) = parse_min_max(list)?;
                        rule_code.push(quote! {
                            results.push(sz_orm_core::validation::rules::validate_range(#field_str, self.#field_ident, #min, #max));
                        });
                    }
                    "regex" => {
                        let pattern = parse_string_arg(list, "pattern")?;
                        rule_code.push(quote! {
                            results.push(sz_orm_core::validation::rules::validate_regex(#field_str, &self.#field_ident, #pattern));
                        });
                    }
                    "contains" => {
                        let substring = parse_string_arg(list, "value")?;
                        rule_code.push(quote! {
                            results.push(sz_orm_core::validation::rules::validate_contains(#field_str, &self.#field_ident, #substring));
                        });
                    }
                    "does_not_contain" => {
                        let substring = parse_string_arg(list, "value")?;
                        rule_code.push(quote! {
                            results.push(sz_orm_core::validation::rules::validate_does_not_contain(#field_str, &self.#field_ident, #substring));
                        });
                    }
                    _ => {
                        return Err(syn::Error::new_spanned(
                            &list.path,
                            format!("unknown validation rule: {}", rule_name),
                        ));
                    }
                }
            }
        }
    }

    if conditions.is_empty() {
        Ok(quote! { #(#rule_code)* })
    } else {
        let combined: Expr = if conditions.len() == 1 {
            conditions.into_iter().next().unwrap()
        } else {
            let mut iter = conditions.into_iter();
            let first = iter.next().unwrap();
            let mut tokens = quote! { #first };
            for cond in iter {
                tokens = quote! { #tokens && #cond };
            }
            syn::parse2(tokens).unwrap()
        };
        Ok(quote! {
            if #combined {
                #(#rule_code)*
            }
        })
    }
}

fn parse_min_max(list: &syn::MetaList) -> syn::Result<(Expr, Expr)> {
    let nested = list
        .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)?;
    let mut min = None;
    let mut max = None;

    for m in nested {
        if let Meta::NameValue(nv) = m {
            let key = nv
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            match key.as_str() {
                "min" => min = Some(nv.value),
                "max" => max = Some(nv.value),
                _ => {}
            }
        }
    }

    match (min, max) {
        (Some(min), Some(max)) => Ok((min, max)),
        _ => Err(syn::Error::new_spanned(
            list,
            "expected min and max arguments",
        )),
    }
}

fn parse_string_arg(list: &syn::MetaList, arg_name: &str) -> syn::Result<String> {
    let nested = list
        .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)?;

    for m in nested {
        if let Meta::NameValue(nv) = m {
            let key = nv
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            if key == arg_name {
                if let Expr::Lit(expr_lit) = &nv.value {
                    if let Lit::Str(lit_str) = &expr_lit.lit {
                        return Ok(lit_str.value());
                    }
                }
            }
        }
    }

    Err(syn::Error::new_spanned(
        list,
        format!("expected {} argument", arg_name),
    ))
}
