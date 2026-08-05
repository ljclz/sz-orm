# sz-orm — Python 绑定

Python bindings for sz-orm-core: async ORM with Model, QueryBuilder, Pool, Transaction.

## 安装

```bash
pip install sz-orm
```

## 基本用法

```python
from sz_orm import QueryBuilder, Model, Pool

# 构建 SQL（参数化查询）
qb = QueryBuilder("mysql")
qb.set_table("users")
qb.where_eq("id", 1)
sql, params = qb.build_select()
# sql = "SELECT * FROM `users` WHERE `id` = ?"
# params = [1]

# 模型
user = Model("users")
user.set("name", "alice")
user.set("age", 30)
data = user.to_dict()

# 连接池
pool = Pool("mysql", max_size=100)
print(pool.status())
```

## 异步模型

异步方法（connect/acquire/execute）通过 pyo3-asyncio 桥接到 asyncio，需在 asyncio 事件循环内调用。

## 类型映射表

| Rust Value | Python 类型 |
|------------|-------------|
| Null | None |
| Bool | bool |
| I8/I16/I32/I64 | int |
| U8/U16/U32/U64 | int |
| F32/F64 | float |
| Decimal | str |
| String | str |
| Bytes | bytes |
| Uuid/Date/DateTime/Time/Json | str (ISO 8601) |
| Array | list |
| Object | dict |

### U64 大值说明

Python int 无精度限制，U64 大值（> 2^53）无精度丢失。