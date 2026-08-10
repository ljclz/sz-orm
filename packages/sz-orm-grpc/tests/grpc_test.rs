use sz_orm_grpc::*;

#[test]
fn test_grpc_service_def() {
    let def = GrpcServiceDef {
        name: "UserService".to_string(),
        methods: vec![],
    };
    assert_eq!(def.name, "UserService");
    assert!(def.methods.is_empty());
}

#[test]
fn test_grpc_method() {
    let method = GrpcMethod {
        name: "GetUser".to_string(),
        input_type: "GetUserRequest".to_string(),
        output_type: "GetUserResponse".to_string(),
        client_streaming: false,
        server_streaming: false,
    };
    assert_eq!(method.name, "GetUser");
    assert!(!method.client_streaming);
    assert!(!method.server_streaming);
}

#[test]
fn test_grpc_method_streaming() {
    let method = GrpcMethod {
        name: "StreamUsers".to_string(),
        input_type: "StreamRequest".to_string(),
        output_type: "User".to_string(),
        client_streaming: false,
        server_streaming: true,
    };
    assert!(method.server_streaming);
}

#[test]
fn test_grpc_server_new() {
    let def = GrpcServiceDef {
        name: "PingService".to_string(),
        methods: vec![],
    };
    let server = GrpcServer::new("127.0.0.1", 8080).register_service(def);
    let handle = server.start().unwrap();
    assert_eq!(handle.address(), "127.0.0.1:8080");
}

#[test]
fn test_grpc_server_register_service() {
    let def = GrpcServiceDef {
        name: "TestService".to_string(),
        methods: vec![GrpcMethod {
            name: "Ping".to_string(),
            input_type: "Empty".to_string(),
            output_type: "Pong".to_string(),
            client_streaming: false,
            server_streaming: false,
        }],
    };
    let server = GrpcServer::new("0.0.0.0", 9090).register_service(def);
    assert_eq!(server.services.len(), 1);
}

#[test]
fn test_grpc_service_def_serialization() {
    let def = GrpcServiceDef {
        name: "Svc".to_string(),
        methods: vec![],
    };
    let json = serde_json::to_string(&def).unwrap();
    let deserialized: GrpcServiceDef = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "Svc");
}
