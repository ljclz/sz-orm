# SZ-ORM Limit

> 限流器 — 令牌桶 / 滑动窗口

## 概述

`sz-orm-limit` 提供令牌桶与滑动窗口限流算法，内置 OOM 防护（默认 max_keys=10000）。

## 特性

- **令牌桶**：支持突发流量
- **滑动窗口**：精确限流，无边界突变
- **OOM 防护**：自动淘汰最久未使用 key
- **分布式**：可与 Redis 配合实现分布式限流

## 安装

```toml
[dependencies]
sz-orm-limit = "2.0.0-alpha.1"
```

## License

MIT
