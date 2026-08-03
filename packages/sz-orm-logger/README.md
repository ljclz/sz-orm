# SZ-ORM Logger

> 结构化日志 — 多级别多后端

## 概述

`sz-orm-logger` 提供多级别（Debug/Info/Warn/Error）、多输出目标的日志记录，支持异步写入与结构化字段。

## 特性

- **多级别**：Debug / Info / Warn / Error
- **多后端**：stdout / 文件 / 网络
- **结构化字段**：支持 key-value 结构化日志
- **异步写入**：不阻塞业务线程

## 安装

```toml
[dependencies]
sz-orm-logger = "2.0.0-alpha.1"
```

## License

MIT
