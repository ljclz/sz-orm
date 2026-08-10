use sz_orm_swagger::*;

#[test]
fn test_openapi_spec_to_json() {
    let spec = OpenAPISpec {
        openapi: "3.0.0".to_string(),
        info: serde_json::json!({"title": "Test API", "version": "1.0"}),
        paths: std::collections::HashMap::new(),
        components: None,
        tags: vec![],
        servers: vec![],
        security: vec![],
    };
    let json = spec.to_json_string();
    assert!(json.contains("3.0.0"));
    assert!(json.contains("Test API"));
}

#[test]
fn test_openapi_spec_empty_paths() {
    let spec = OpenAPISpec {
        openapi: "3.0.0".to_string(),
        info: serde_json::json!({}),
        paths: std::collections::HashMap::new(),
        components: None,
        tags: vec![],
        servers: vec![],
        security: vec![],
    };
    let json = spec.to_json_string();
    assert!(json.contains("paths"));
}

#[test]
fn test_tag_new() {
    let tag = Tag::new("users").with_description("User operations");
    assert_eq!(tag.name, "users");
    assert_eq!(tag.description, Some("User operations".to_string()));
}

#[test]
fn test_server_new() {
    let server = Server::new("https://api.example.com").with_description("Production");
    assert_eq!(server.url, "https://api.example.com");
    assert_eq!(server.description, Some("Production".to_string()));
}

#[test]
fn test_openapi_spec_with_tags() {
    let spec = OpenAPISpec {
        openapi: "3.0.0".to_string(),
        info: serde_json::json!({"title": "API", "version": "1.0"}),
        paths: std::collections::HashMap::new(),
        components: None,
        tags: vec![Tag::new("users"), Tag::new("orders")],
        servers: vec![],
        security: vec![],
    };
    let json = spec.to_json_string();
    assert!(json.contains("users"));
    assert!(json.contains("orders"));
}

#[test]
fn test_openapi_spec_with_servers() {
    let spec = OpenAPISpec {
        openapi: "3.0.0".to_string(),
        info: serde_json::json!({"title": "API", "version": "1.0"}),
        paths: std::collections::HashMap::new(),
        components: None,
        tags: vec![],
        servers: vec![Server::new("https://api.example.com")],
        security: vec![],
    };
    let json = spec.to_json_string();
    assert!(json.contains("api.example.com"));
}
