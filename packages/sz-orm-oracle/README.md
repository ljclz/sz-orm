# SZ-ORM Oracle

> Oracle 数据库适配器 — ODPI-C 驱动

## 概述

`sz-orm-oracle` 基于 `oracle` crate（ODPI-C 绑定）实现 `Connection` trait，支持 Oracle 12c/19c/21c/23ai（实测 Oracle 23ai Free）。

## 特性

- **ODPI-C 绑定**：高性能原生驱动
- **占位符转换**：`?` → `:N` 格式自动转换
- **完整类型映射**：Oracle 数据类型 ↔ Rust 类型
- **阻塞池隔离**：同步操作隔离，避免阻塞异步运行时

## 安装

```toml
[dependencies]
sz-orm-oracle = "2.0.0-alpha.1"
```

## License

MIT
