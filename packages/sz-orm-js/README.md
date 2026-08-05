# @sz-orm/core — JavaScript/Node.js 绑定

JavaScript/Node.js bindings for sz-orm-core: async ORM with Model, QueryBuilder, Pool, Transaction.

## 安装

```bash
npm install @sz-orm/core
```

## 基本用法

```javascript
const { QueryBuilder, Model, Pool } = require('@sz-orm/core');

// 构建 SQL（参数化查询）
const qb = new QueryBuilder('mysql');
qb.setTable('users');
qb.whereEqI64('id', 1);
const { sql, params } = qb.buildSelect();
// sql = "SELECT * FROM `users` WHERE `id` = ?"
// params = ["1"]  (JSON 字符串，JS 侧用 JSON.parse 还原)

// 模型
const user = new Model('users');
user.setStr('name', 'alice');
user.setI64('age', 30);
const json = user.toJsonString();

// 连接池
const pool = new Pool('mysql', 100);
console.log(pool.status());
```

## 异步模型

异步方法（connect/acquire/execute）通过 napi-rs 自动映射为 JS Promise，在 libuv 线程池执行 Rust Future。

## 类型映射表

| Rust Value | JS 类型 |
|------------|---------|
| Null | null |
| Bool | boolean |
| I8/I16/I32/I64 | number |
| U8/U16/U32 | number |
| U64 | number (≤ 2^53) / BigInt (> 2^53) |
| F32/F64 | number |
| Decimal | string (保留精度) |
| String | string |
| Bytes | Uint8Array |
| Uuid/Date/DateTime/Time | string (ISO 8601) |
| Json | object (JSON.parse) |
| Array | Array |
| Object | object |

### U64 大值说明

JS number 仅支持到 2^53，U64 > 2^53 需用 BigInt。当前实现通过 JSON 字符串中间格式传递，JS 侧用 `JSON.parse` 还原。

### Decimal 精度说明

Decimal 用字符串传递，避免 JS number 精度丢失。JS 侧用 `decimal.js` 或类似库处理。