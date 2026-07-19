//! Real GraphQL execution and serving backed by async-graphql + axum.
//!
//! This module is only compiled with the `real` feature enabled. It builds a
//! dynamic async-graphql schema from the declarative [`GraphQLSchema`],
//! executes queries against it and exposes it over HTTP via axum.

use async_graphql::dynamic::{
    Field, FieldFuture, InputValue, Object, ResolverContext, Schema, TypeRef,
};
use async_graphql::Value;

use crate::{GraphQLField, GraphQLSchema, GraphQLType};

/// Parse a GraphQL type reference like `ID!`, `User` or `[User!]!` into a
/// dynamic [`TypeRef`]. List nesting beyond one level is rejected.
fn parse_type_ref(type_name: &str) -> Result<TypeRef, String> {
    let unsupported = || format!("Unsupported type reference '{type_name}'");
    let trimmed = type_name.trim();
    let (inner, non_null) = match trimmed.strip_suffix('!') {
        Some(rest) => (rest.trim(), true),
        None => (trimmed, false),
    };
    if let Some(list_inner) = inner.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let list_inner = list_inner.trim();
        let (item, item_non_null) = match list_inner.strip_suffix('!') {
            Some(rest) => (rest.trim(), true),
            None => (list_inner, false),
        };
        if item.is_empty() || item.starts_with('[') {
            return Err(unsupported());
        }
        Ok(match (item_non_null, non_null) {
            (false, false) => TypeRef::named_list(item),
            (true, false) => TypeRef::named_nn_list(item),
            (false, true) => TypeRef::named_list_nn(item),
            (true, true) => TypeRef::named_nn_list_nn(item),
        })
    } else if inner.is_empty() {
        Err(unsupported())
    } else if non_null {
        Ok(TypeRef::named_nn(inner))
    } else {
        Ok(TypeRef::named(inner))
    }
}

/// Build the mock JSON payload for a root field, mirroring the data shape of
/// the in-memory implementation.
fn mock_payload(field_name: &str, is_list: bool) -> Value {
    let object = |id: &str| {
        serde_json::json!({
            "id": id,
            "name": format!("{field_name}_{id}"),
            "createdAt": "2024-01-01T00:00:00Z",
            "updatedAt": "2024-01-01T00:00:00Z",
        })
    };
    let json = if is_list {
        serde_json::json!([object("1"), object("2")])
    } else {
        object("1")
    };
    // The payload contains only strings, arrays and objects, which always
    // convert into a GraphQL value.
    Value::from_json(json).unwrap_or(Value::Null)
}

/// Create a root (query/mutation) field that resolves to mock data.
fn mock_root_field(field: &GraphQLField) -> Result<Field, String> {
    let type_ref = parse_type_ref(&field.type_name)?;
    let is_list = field.type_name.trim_start().starts_with('[');
    let value = mock_payload(&field.name, is_list);
    let mut root = Field::new(field.name.clone(), type_ref, move |_ctx| {
        FieldFuture::from_value(Some(value.clone()))
    });
    if !is_list {
        // Single-object lookups conventionally accept an optional `id`.
        root = root.argument(InputValue::new("id", TypeRef::named(TypeRef::ID)));
    }
    Ok(root)
}

/// Create a dynamic object type whose fields read from the parent value
/// resolved by the root field.
fn object_type(t: &GraphQLType) -> Result<Object, String> {
    let mut obj = Object::new(t.name.clone());
    for field in &t.fields {
        let type_ref = parse_type_ref(&field.type_name)?;
        let field_name = field.name.clone();
        obj = obj.field(Field::new(
            field.name.clone(),
            type_ref,
            move |ctx: ResolverContext<'_>| {
                let value = ctx
                    .parent_value
                    .try_to_value()
                    .ok()
                    .and_then(|parent| match parent {
                        Value::Object(map) => map.get(field_name.as_str()).cloned(),
                        _ => None,
                    });
                FieldFuture::from_value(value)
            },
        ));
    }
    Ok(obj)
}

/// Build a real executable async-graphql [`Schema`] from the declarative
/// [`GraphQLSchema`].
pub fn build_dynamic_schema(schema: &GraphQLSchema) -> Result<Schema, String> {
    let mutation_name = if schema.mutations.is_empty() {
        None
    } else {
        Some("Mutation")
    };
    let mut builder = Schema::build("Query", mutation_name, None);
    for t in &schema.types {
        builder = builder.register(object_type(t)?);
    }
    let mut query = Object::new("Query");
    for field in &schema.queries {
        query = query.field(mock_root_field(field)?);
    }
    builder = builder.register(query);
    if mutation_name.is_some() {
        let mut mutation = Object::new("Mutation");
        for field in &schema.mutations {
            mutation = mutation.field(mock_root_field(field)?);
        }
        builder = builder.register(mutation);
    }
    builder.finish().map_err(|e| e.to_string())
}

/// Build an axum router serving GraphQL POST requests at `/graphql`.
pub fn router(schema: Schema) -> axum::Router {
    async fn graphql_handler(
        axum::extract::State(schema): axum::extract::State<Schema>,
        request: async_graphql_axum::GraphQLRequest,
    ) -> async_graphql_axum::GraphQLResponse {
        schema.execute(request.into_inner()).await.into()
    }

    axum::Router::new()
        .route("/graphql", axum::routing::post(graphql_handler))
        .with_state(schema)
}

/// Execute a GraphQL query against the dynamic schema and return the resolved
/// value of the first root field as JSON.
pub fn execute(schema: &Schema, query: &str) -> Result<serde_json::Value, String> {
    // Run the async executor on a dedicated scoped thread so this synchronous
    // API works both inside and outside of an existing tokio runtime.
    let response = std::thread::scope(|scope| {
        scope
            .spawn(|| -> Result<async_graphql::Response, String> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("Failed to build tokio runtime: {e}"))?;
                Ok(runtime.block_on(schema.execute(query)))
            })
            .join()
            .map_err(|_| "GraphQL executor thread panicked".to_string())?
    })?;
    if !response.errors.is_empty() {
        return Err(response
            .errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("; "));
    }
    let data = response.data.into_json().map_err(|e| e.to_string())?;
    match data {
        serde_json::Value::Object(map) => Ok(map
            .into_iter()
            .next()
            .map(|(_, value)| value)
            .unwrap_or(serde_json::Value::Null)),
        other => Ok(other),
    }
}
