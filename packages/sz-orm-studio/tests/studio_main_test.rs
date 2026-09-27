use sz_orm_studio::parse_args;

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_parse_args_default_addr_port() {
    let config = parse_args(&argv(&["sz-orm-studio"]));
    assert_eq!(config.addr, "127.0.0.1");
    assert_eq!(config.port, 8080);
}

#[test]
fn test_parse_args_custom_addr() {
    let config = parse_args(&argv(&["sz-orm-studio", "--addr=0.0.0.0"]));
    assert_eq!(config.addr, "0.0.0.0");
    assert_eq!(config.port, 8080);
}

#[test]
fn test_parse_args_custom_port() {
    let config = parse_args(&argv(&["sz-orm-studio", "--port=9999"]));
    assert_eq!(config.addr, "127.0.0.1");
    assert_eq!(config.port, 9999);
}

#[test]
fn test_parse_args_invalid_port_falls_back_to_default() {
    let config = parse_args(&argv(&["sz-orm-studio", "--port=abc"]));
    assert_eq!(config.port, 8080);
}

#[test]
fn test_parse_args_both_custom() {
    let config = parse_args(&argv(&["sz-orm-studio", "--addr=0.0.0.0", "--port=3000"]));
    assert_eq!(config.addr, "0.0.0.0");
    assert_eq!(config.port, 3000);
}

#[test]
fn test_parse_args_empty_argv() {
    let config = parse_args(&[]);
    assert_eq!(config.addr, "127.0.0.1");
    assert_eq!(config.port, 8080);
}
