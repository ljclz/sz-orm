# SZ-ORM MSSQL

> Microsoft SQL Server 适配器 — Tiberius 驱动

## 概述

`sz-orm-mssql` 基于 `tiberius` crate（纯 Rust TDS 协议实现）实现 `Connection` trait，支持 SQL Server 2008+（TDS 7.3+）。

## 特性

- **纯 Rust TDS 协议**：无外部依赖
- **占位符转换**：`?` → `@P1` 格式自动转换
- **完整类型映射**：SQL Server 数据类型 ↔ Rust 类型
- **阻塞池隔离**：同步操作隔离，避免阻塞异步运行时

## 安装

```toml
[dependencies]
sz-orm-mssql = "2.0.0-alpha.1"
```

## License

MIT
