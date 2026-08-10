# Proc-Macro SQL 编译期验证指南

> 版本：v3.7.0 | 稳定性：stable | feature gate：`sql-verify-proc`

## 1. 概述

`sql-verify-proc` feature 提供 `query!` 宏的编译期 SQL 验证能力。默认仅语法校验，启用 `db-verify` feature + `SZ_ORM_QUERY_VERIFY=1` 环境变量后，编译时连真实 DB 执行 `EXPLAIN` 验证。

## 2. 启用方式

### 2.1 仅语法校验（默认）

```toml
[dependencies]
sz-orm-core = { version = "3.7.0", features = ["sql-verify-proc"] }
```

### 2.2 连真实 DB 验证

```bash
export DATABASE_URL="mysql://root:test123@127.0.0.1:3306/sz_orm_test"
export SZ_ORM_QUERY_VERIFY=1
cargo build --features sz-orm-macros/db-verify
```

## 3. 覆盖路径

| 路径 | 说明 | EXPLAIN 支持 |
|------|------|-------------|
| SELECT | 基础查询 | ✅ |
| INSERT | 插入数据 | ✅ |
| UPDATE | 更新数据 | ✅ |
| DELETE | 删除数据 | ✅ |
| JOIN（INNER/LEFT/RIGHT/FULL） | 表连接 | ✅ |
| 子查询（WHERE/SELECT/FROM） | 嵌套查询 | ✅ |
| CTE（WITH/WITH RECURSIVE） | 公用表表达式 | ✅ |
| 窗口函数（OVER/PARTITION BY/FRAME） | 窗口计算 | ✅ |

## 4. 降级模式

当 `DATABASE_URL` 未设置或 DB 不可达时，自动回退到仅语法校验：

```
warning: sql-verify-proc degraded to syntax-only (DATABASE_URL not set)
```

## 5. 缓存机制

验证结果按 SQL 哈希缓存，仅 SQL 变更时重新验证，避免重复编译时重复连 DB。

## 6. 多方言支持

| 方言 | EXPLAIN 语法 |
|------|-------------|
| MySQL | `EXPLAIN <sql>` |
| PostgreSQL | `EXPLAIN <sql>` |
| SQLite | `EXPLAIN QUERY PLAN <sql>` |

## 7. 稳定性

- **v3.6.0**：首次引入，experimental
- **v3.7.0**：stable，覆盖所有 QueryBuilder 路径，测试 ≥10 用例