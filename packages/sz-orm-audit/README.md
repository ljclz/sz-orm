# SZ-ORM Audit

> SQL 审计日志 — 执行记录与敏感信息脱敏

## 概述

`sz-orm-audit` 提供 SQL 执行审计记录能力，对 `password`/`token`/`credit_card` 等敏感关键词进行大小写不敏感脱敏，确保审计日志不泄露敏感信息。

## 特性

- **执行审计**：记录每条 SQL 的执行时间、影响行数、调用方
- **敏感词脱敏**：自动识别并脱敏 password/token/credit_card 等字段
- **异步写入**：不阻塞主查询路径

## 安装

```toml
[dependencies]
sz-orm-audit = "2.0.0-alpha.1"
```

## License

MIT
