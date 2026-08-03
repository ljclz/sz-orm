# SZ-ORM Masking

> 数据脱敏 — 敏感字段自动脱敏

## 概述

`sz-orm-masking` 提供手机号、邮箱、身份证、银行卡、姓名、地址等敏感字段脱敏，支持自定义前缀/后缀保留规则。实现 Unicode 安全，对短输入有合理兜底，不会 panic。

## 特性

- **内置规则**：手机号/邮箱/身份证/银行卡/姓名/地址
- **自定义规则**：支持自定义前缀/后缀保留
- **Unicode 安全**：正确处理 emoji 和多字节字符
- **短输入兜底**：输入过短时合理降级，不 panic

## 安装

```toml
[dependencies]
sz-orm-masking = "2.0.0-alpha.1"
```

## License

MIT
