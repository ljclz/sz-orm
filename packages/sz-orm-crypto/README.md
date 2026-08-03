# SZ-ORM Crypto

> 加密工具 — AES-256-GCM / HMAC-SHA256 / PBKDF2

## 概述

`sz-orm-crypto` 提供常用密码学原语，基于 RustCrypto 实现，保证常数时间比较。

## 特性

- **AES-256-GCM**：认证加密，防篡改
- **HMAC-SHA256**：消息认证码
- **PBKDF2**：密钥派生，可配置迭代次数
- **SHA-256**：哈希摘要
- **常数时间比较**：防时序攻击

## 安装

```toml
[dependencies]
sz-orm-crypto = "2.0.0-alpha.1"
```

## License

MIT
