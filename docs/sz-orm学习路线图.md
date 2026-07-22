# SZ-ORM 学习教程（面向 PHP/ThinkPHP 工程师）

> **读者画像**：PHP 工程师，擅长 ThinkPHP，没接触过 Rust，现在用 AI 驱动开发 sz-orm 项目
> **目标**：从零基础到能用 AI 协作维护 sz-orm 代码
> **更新日期**：2026-07-22 · 适用版本：v1.0.0+

---

## 这份教程怎么用

这份教程**不是**"看哪些文档"的索引，而是把 ThinkPHP 你已经熟悉的概念**逐一对照**到 sz-orm 的 Rust 实现，告诉你：

- ThinkPHP 里 XX 怎么写 → sz-orm 里对应 YY 怎么写
- Rust 哪些语法是 PHP 没有的，必须先理解
- 用 AI 协作开发 sz-orm 的正确姿势（怎么提问、怎么验证）

每一章都配**可运行代码**，代码取自 [examples/src/bin/](../examples/src/bin/) 8 个示例，全部能 `cargo run` 跑起来。

---

## 第 0 章 · Rust 速通（30 分钟，只看够用的部分）

你不需要先读完《The Rust Book》再来。本章只讲 sz-orm 代码里**必然遇到**的 Rust 概念，每个都用 PHP 类比。

### 0.1 心智模型转换

| PHP / ThinkPHP | Rust / sz-orm |
|----------------|---------------|
| `class User extends Model` | `struct User` + `impl Model for User`（组合优于继承） |
| `interface Model` | `trait Model`（几乎等价） |
| `function find($id): ?User` | `fn find(id: i64) -> Option<User>`（`Option` 替代 nullable） |
| `throw new Exception()` | `return Err(...)`（`Result<T, E>` 替代异常） |
| `array`（万能数组） | `Vec<T>`（列表）、`HashMap<K, V>`（字典）、`[T; N]`（定长数组） |
| `$obj->prop`（可空） | `Option<T>` + `match` / `?` 强制处理 |
| `use Foo\Bar` | `use foo::Bar;`（路径分隔符从 `\` 变 `::`） |
| Composer | Cargo（`Cargo.toml` = `composer.json`） |
| `vendor/` | `target/`（不提交） |
| `composer require x/y` | `cargo add x-y`（或在 Cargo.toml 加 `dependencies`） |
| `composer dump-autoload` | `cargo check`（自动） |
| `namespace App\Model` | `mod model;` + 文件路径决定模块 |

**核心差异**：Rust 没有 GC，靠**所有权（ownership）**管理内存。下一节详解。

### 0.2 所有权与借用（PHP 没有的概念，必须理解）

PHP 用引用计数 + GC，对象随便传。Rust 不行，每个值在任意时刻**只有一个所有者**。

```rust
// PHP: $a = [1,2,3]; $b = $a; → $a 和 $b 都是 [1,2,3]
// Rust:
let a = vec![1, 2, 3];
let b = a;                  // a 的所有权"移动"给 b
// println!("{:?}", a);     // ❌ 编译错误：a 已被 move
println!("{:?}", b);        // ✅ b 拥有数据

// 想保留 a，就"借用"：
let a = vec![1, 2, 3];
let b = &a;                 // 借用，a 仍拥有数据
println!("{:?} {:?}", a, b);// ✅ 都能用（只读）
```

sz-orm 里你会反复看到：
- `&str` / `&String` — 借用字符串，不拷贝
- `&mut T` — 可变借用（独占）
- `Arc<T>` — 跨线程共享所有权的智能指针（类似 PHP 的对象引用，但要显式 clone）
- `Box<dyn Dialect>` — 把对象放到堆上 + 动态派发（类似 PHP 的多态）

### 0.3 trait（≈ PHP interface，但更强）

```php
// PHP
interface SoftDelete {
    public function isDeleted(): bool;
}
class User implements SoftDelete {
    public function isDeleted(): bool { return $this->deleted_at !== null; }
}
```

```rust
// Rust
trait SoftDelete {
    fn is_deleted(&self) -> bool;
}
struct User { deleted_at: Option<String> }
impl SoftDelete for User {
    fn is_deleted(&self) -> bool { self.deleted_at.is_some() }
}
```

差异：
- Rust trait 可以有**默认实现**（PHP 也能，但 Rust 更常用）
- Rust trait 可以有**关联类型**（PHP 没有），如 `type PrimaryKey;`
- Rust 一个 trait 可以"继承"多个 trait：`trait ActiveRecord: Model + ModelExt {}`
- Rust trait 对象 `Box<dyn Dialect>` 类似 PHP 的 `Dialect $d` 类型提示

### 0.4 泛型（PHP 没有，必须学）

PHP 没有真正的泛型，靠 PHPDoc `@template T`。Rust 是真泛型，编译期单态化。

```rust
// sz-orm 的 QueryBuilder<M: Model> 表示 M 必须实现 Model trait
let qb: QueryBuilder<User> = QueryBuilder::new(dialect);
let qb2: QueryBuilder<Order> = QueryBuilder::new(dialect);
// 编译后变成两个不同的具体类型，零运行时开销
```

你不需要会写复杂泛型，但读 sz-orm 代码时看到 `<M: Model>` 要知道这是"任意实现了 Model 的类型"。

### 0.5 async/await（Swoole / ReactPHP 类比）

```php
// PHP Swoole
go(function() {
    $result = Co::system('ls');
    echo $result;
});
```

```rust
// Rust + Tokio
#[tokio::main]
async fn main() {
    let result = sqlx::query("SELECT 1").fetch_one(&pool).await?;
    println!("{:?}", result);
}
```

差异：
- Rust async 函数返回 `impl Future<Output = T>`，**必须** `.await` 才执行
- Rust 没有"绿色线程"，所有 async 都在 Tokio runtime 上调度
- sz-orm 所有数据库操作都是 async，`main` 必须加 `#[tokio::main]`

### 0.6 错误处理：`Result<T, E>` + `?` 运算符

```php
// PHP：try/catch
try {
    $user = User::find($id);
    if (!$user) throw new Exception('not found');
} catch (Exception $e) {
    // ...
}
```

```rust
// Rust：Result 枚举 + ? 运算符
fn get_user(id: i64) -> Result<User, DbError> {
    let user = User::find(id)?;          // 出错自动 return Err
    Ok(user)
}

// 调用方决定怎么处理
match get_user(42) {
    Ok(u) => println!("{}", u.name),
    Err(e) => eprintln!("error: {}", e),
}
```

`?` 是 Rust 的"失败早返回"语法糖，等价于：
```rust
let user = match User::find(id) {
    Ok(u) => u,
    Err(e) => return Err(e),
};
```

### 0.7 工具链速查

| 操作 | Composer | Cargo |
|------|----------|-------|
| 安装依赖 | `composer install` | `cargo build`（自动） |
| 添加包 | `composer require x/y` | `cargo add x-y` |
| 运行测试 | `./vendor/bin/phpunit` | `cargo test` |
| 检查风格 | `php-cs-fixer fix` | `cargo fmt` |
| 静态检查 | `phpstan analyse` | `cargo clippy` |
| 运行脚本 | `composer run-script x` | `cargo run --bin x` |
| 文档生成 | `phpdoc` | `cargo doc --open` |

---

## 第 1 章 · 环境搭建（15 分钟）

### 1.1 安装 Rust 工具链

Windows（你已经用的环境）：
```powershell
# 下载 rustup-init.exe 后执行
rustup default stable
rustc --version    # 需要 1.94.0+（sqlx 0.9.0 要求）
cargo --version
```

### 1.2 克隆 sz-orm 项目

```powershell
cd e:\vue\test\鲜视达\rust\sz-orm
cargo check --workspace        # 首次编译约 5-10 分钟
```

### 1.3 验证编译

```powershell
cargo build --workspace
cargo test --workspace -q      # 全量测试
cargo fmt --all -- --check     # 格式检查
cargo clippy --workspace --all-targets -- -D warnings   # 静态检查
```

如果 `cargo clippy` 报 warning，说明代码不符合规范，必须修。

### 1.4 项目结构速览

```
sz-orm/
├── packages/             # 39 个工作空间成员（≈ Composer 包）
│   ├── sz-orm-core/      # 核心引擎（≈ think-orm）
│   ├── sz-orm-sqlx/      # 真实数据库适配（≈ think-orm 的 PDO driver）
│   └── ...
├── examples/src/bin/     # 8 个可运行示例（你的入门起点）
├── docs/                 # 6 份文档 + 5 个 ADR
├── Cargo.toml            # 工作空间清单（≈ 根 composer.json）
└── README.md
```

### 1.5 运行第一个示例

```powershell
cargo run -p sz-orm-examples --bin quick_start
```

预期输出：
```
SELECT:
SELECT id, name, email FROM `users` WHERE status = 'active' ORDER BY created_at ASC, id DESC LIMIT 10
...
```

源码：[examples/src/bin/quick_start.rs](../examples/src/bin/quick_start.rs)

---

## 第 2 章 · 第一个 Model（对照 ThinkPHP Model）

### 2.1 ThinkPHP Model 回顾

```php
// PHP: app/model/User.php
namespace app\model;
use think\Model;

class User extends Model {
    protected $table = 'users';
    protected $pk = 'id';
    protected $autoWriteTimestamp = true;
    protected $deleteTime = 'deleted_at';
}
```

ThinkPHP 靠**继承**获得 ORM 能力，配置靠**属性**。

### 2.2 sz-orm Model 定义

sz-orm 靠**实现 trait** 获得 ORM 能力，配置靠**trait 方法**。

```rust
// Rust: 来自 examples/src/bin/quick_start.rs
use sz_orm_core::{Model, TimestampFields};

#[derive(Debug, Clone, Default)]   // ← PHP 没有对应，派生宏自动生成 trait 实现
struct User {
    id: i64,
    name: String,
    email: String,
}

impl Model for User {
    type PrimaryKey = i64;                              // ← PHP 没有关联类型，这里声明主键类型
    fn table_name() -> &'static str { "users" }        // ← protected $table = 'users'
    fn pk(&self) -> Self::PrimaryKey { self.id }       // ← 获取主键值
    fn set_pk(&mut self, pk: Self::PrimaryKey) { self.id = pk; }
    fn timestamp_fields() -> Option<TimestampFields> {
        Some(TimestampFields::with_both("created_at", "updated_at"))  // ← $autoWriteTimestamp
    }
}
```

### 2.3 逐行对照

| ThinkPHP | sz-orm | 说明 |
|----------|--------|------|
| `class User extends Model` | `struct User` + `impl Model for User` | 组合优于继承 |
| `protected $table = 'users'` | `fn table_name() -> &'static str { "users" }` | 表名 |
| `protected $pk = 'id'` | `fn pk_name() -> &'static str { "id" }` | 主键列名（有默认值） |
| `$user->id` | `user.pk()` | 获取主键值 |
| `$autoWriteTimestamp = true` | `timestamp_fields()` 返回 `Some(...)` | 自动时间戳 |
| `protected $deleteTime = 'deleted_at'` | `soft_delete_field()` 返回 `Some("deleted_at")` | 软删除 |

### 2.4 字段类型映射

| PHP 类型 | Rust 类型 | 说明 |
|----------|-----------|------|
| `int` | `i64` | 64位整数 |
| `float` | `f64` | 双精度浮点 |
| `string` | `String` | 堆字符串 |
| `?string` (nullable) | `Option<String>` | Rust 用 `Option` 表达 nullable |
| `array` | `Vec<T>` 或 `HashMap<K,V>` | 必须明确元素类型 |
| `bool` | `bool` | 同 |
| `DateTime` | `Option<String>`（ISO 字符串） | sz-orm 用字符串存时间戳 |

### 2.5 可运行示例

```powershell
cargo run -p sz-orm-examples --bin model_definition
```

源码：[examples/src/bin/model_definition.rs](../examples/src/bin/model_definition.rs)

这个示例演示完整的 Model 定义，包含主键、时间戳、软删除、列定义、批量赋值、JSON 序列化、关联关系。

---

## 第 3 章 · CRUD 操作（对照 ThinkPHP Db/Model）

### 3.1 ThinkPHP 查询回顾

```php
// PHP
$users = Db::table('users')
    ->where('status', 'active')
    ->where('age', 'between', [18, 65])
    ->order('created_at')
    ->limit(10)
    ->select();

$count = Db::table('users')->where('status', 'active')->count();

Db::table('users')->insert(['name' => 'Alice', 'email' => 'a@b.com']);
Db::table('users')->where('id', 1)->update(['name' => 'Bob']);
Db::table('users')->where('id', 1)->delete();
```

### 3.2 sz-orm QueryBuilder

```rust
// Rust: 来自 examples/src/bin/quick_start.rs
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, Model, QueryBuilder, Value};

let dialect = get_dialect(DbType::MySQL).expect("MySQL 方言可用");

// SELECT
let select_sql = QueryBuilder::<User>::new(dialect.clone())
    .table("users")
    .select(vec!["id", "name", "email"])
    .where_cond("status = 'active'")
    .order_by("created_at")
    .order_desc("id")
    .limit(10)
    .build_select();
// → SELECT id, name, email FROM `users` WHERE status = 'active'
//   ORDER BY created_at ASC, id DESC LIMIT 10

// WHERE 复合条件
let complex_sql = QueryBuilder::<User>::new(dialect.clone())
    .table("users")
    .where_cond("status = 'active'")
    .or_where("role = 'admin'")
    .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
    .where_between("age", Value::I64(18), Value::I64(65))
    .where_null("deleted_at")
    .page(3, 20)                    // 第 3 页，每页 20 条
    .build_select();

// 聚合
let count_sql = QueryBuilder::<User>::new(dialect.clone())
    .table("users")
    .where_cond("status = 'active'")
    .build_count();

// INSERT
use std::collections::HashMap;
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Alice".to_string()));
data.insert("email".to_string(), Value::String("alice@example.com".to_string()));
let insert_sql = QueryBuilder::<User>::new(dialect.clone())
    .table("users")
    .build_insert(&data);

// UPDATE
let mut update_data = HashMap::new();
update_data.insert("name".to_string(), Value::String("Bob".to_string()));
let update_sql = QueryBuilder::<User>::new(dialect.clone())
    .table("users")
    .where_cond("id = 1")
    .build_update(&update_data);

// DELETE
let delete_sql = QueryBuilder::<User>::new(dialect.clone())
    .table("users")
    .where_cond("id = 1")
    .build_delete();
```

### 3.3 API 对照表

| ThinkPHP | sz-orm QueryBuilder | 说明 |
|----------|---------------------|------|
| `->where('status', 'active')` | `.where_cond("status = 'active'")` | **注意**：sz-orm 直接传完整条件字符串 |
| `->whereOr('role', 'admin')` | `.or_where("role = 'admin'")` | OR 条件 |
| `->whereIn('id', [1,2,3])` | `.where_in("id", vec![Value::I64(1), ...])` | IN |
| `->whereBetween('age', [18,65])` | `.where_between("age", Value::I64(18), Value::I64(65))` | BETWEEN |
| `->whereNull('deleted_at')` | `.where_null("deleted_at")` | IS NULL |
| `->order('created_at')` | `.order_by("created_at")` | ASC |
| `->order('id', 'desc')` | `.order_desc("id")` | DESC |
| `->limit(10)` | `.limit(10)` | 限制 |
| `->page(3, 20)` | `.page(3, 20)` | 分页 |
| `->join('posts p', 'p.user_id=u.id')` | `.join_inner("posts", "users.id", "posts.user_id")` | INNER JOIN |
| `->leftJoin('profiles p', ...)` | `.join_left("profiles", "users.id", "profiles.user_id")` | LEFT JOIN |
| `->count()` | `.build_count()` | 返回 SQL 字符串 |
| `->select()` | `.build_select()` | 返回 SQL 字符串 |
| `->insert($data)` | `.build_insert(&data)` | 返回 SQL 字符串 |
| `->update($data)` | `.build_update(&data)` | 返回 SQL 字符串 |
| `->delete()` | `.build_delete()` | 返回 SQL 字符串 |

### 3.4 关键差异（必须理解）

1. **sz-orm 的 QueryBuilder 只生成 SQL，不执行**。要执行 SQL，需要 sz-orm-sqlx（第 5 章）。
2. **where_cond 接收完整条件字符串**，不像 ThinkPHP 拆成字段+操作符+值。这是因为 sz-orm 假设你用参数化绑定（`?` 占位符）传值，条件里只写 SQL 片段。
3. **Value 是强类型枚举**（第 4 章），不是 PHP 的弱类型。

### 3.5 SQL 校验（ThinkPHP 没有）

sz-orm 提供运行时校验，防止 SQL 注入：

```rust
let result = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .where_cond("id = 1")
    .validate();   // 返回 Result<(), DbError>
```

更强大的是**编译期校验** `sql_string!` 宏（见第 12 章），在编译时就能拦截 SQL 注入。

### 3.6 可运行示例

```powershell
cargo run -p sz-orm-examples --bin quick_start
```

---

## 第 4 章 · Value 类型系统（PHP 没有的概念）

### 4.1 为什么需要 Value 枚举

PHP 是弱类型，`$data['age'] = 18` 和 `$data['name'] = 'Alice'` 可以混在一个 array 里。Rust 的 `HashMap<K, V>` 要求所有值同类型，所以 sz-orm 设计了 `Value` 枚举来统一表达"任意数据库列值"。

### 4.2 20 种 Value 变体

```rust
use sz_orm_core::Value;

// 数值类
Value::I8(1)         // 8位有符号整数
Value::I16(1)
Value::I32(1)
Value::I64(1)        // 最常用，对应 MySQL BIGINT/INT
Value::U8(1)         // 无符号
Value::U16(1)
Value::U32(1)
Value::U64(1)
Value::F32(1.5)      // 单精度浮点
Value::F64(1.5)      // 双精度浮点，对应 MySQL FLOAT/DOUBLE

// 文本类
Value::String("hello".to_string())   // 对应 VARCHAR/TEXT
Value::Bytes(vec![0x01, 0x02])       // BLOB

// 时间类（用 ISO 字符串表示）
Value::Date("2026-07-22".to_string())
Value::DateTime("2026-07-22 10:00:00".to_string())
Value::Time("10:00:00".to_string())

// 特殊类
Value::Null                      // NULL
Value::Bool(true)                // BOOLEAN
Value::Uuid("...".to_string())   // UUID
Value::Json("{...}".to_string()) // JSON
Value::Array(vec![...])          // 数组
Value::Object(HashMap::new())    // 对象
```

### 4.3 类型转换

```rust
let v = Value::I64(42);
let n: Option<i64> = v.as_i64();     // Some(42)
let s: Option<&str> = v.as_str();    // None（I64 不是字符串）

let v2 = Value::String("hello".to_string());
let s2: Option<&str> = v2.as_str();  // Some("hello")
```

### 4.4 PHP 对照

| PHP | sz-orm Value |
|-----|--------------|
| `18` (int) | `Value::I64(18)` |
| `3.14` (float) | `Value::F64(3.14)` |
| `'Alice'` (string) | `Value::String("Alice".to_string())` |
| `true` (bool) | `Value::Bool(true)` |
| `null` | `Value::Null` |
| `[1, 2, 3]` | `Value::Array(vec![Value::I64(1), ...])` |
| `['a' => 1]` | `Value::Object(HashMap::new())` |

**繁琐但安全**：Rust 编译器会强制你处理每个可能的类型，避免 PHP 那种"隐式类型转换"导致的 bug。

### 4.5 详细 API

参考 [sz-ormAPI参考.md §1.7 Value](sz-ormAPI参考.md#17-valuevalue-rs)

---

## 第 5 章 · 连接真实数据库

### 5.1 sz-orm-core vs sz-orm-sqlx

- `sz-orm-core`：提供 ORM 抽象层（Model、QueryBuilder、Value），但**不连接真实数据库**，只生成 SQL 字符串。适合测试、SQL 生成。
- `sz-orm-sqlx`：基于 [sqlx](https://crates.io/crates/sqlx) 提供**真实数据库连接**（MySQL/PostgreSQL/SQLite）。生产环境必须用这个。

### 5.2 MySQL 连接示例

```rust
use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{MySqlPoolHandle, SqlxMySqlConnectionFactory};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建 sqlx 连接池
    let handle = MySqlPoolHandle::connect("mysql://user:pass@localhost:3306/db").await?;
    // 2. 包装成 sz-orm 的 ConnectionFactory
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(Arc::new(handle)));
    // 3. 创建 sz-orm 连接池
    let config = PoolConfigBuilder::new().max_size(10).build()?;
    let pool = Pool::new(config, factory)?;
    // 4. 获取连接并查询
    let mut conn = pool.acquire().await?;
    let rows = conn.query("SELECT 1 AS one").await?;
    println!("rows = {}", rows.len());
    Ok(())
}
```

### 5.3 SQLite 内存数据库（测试首选）

```rust
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};

let handle = SqlitePoolHandle::connect("sqlite::memory:").await?;
// 其余同上
```

CI 环境用 `sqlite::memory:` 而非文件路径，避免磁盘 I/O 错误（见 [ADR 0005](adr/0005-Connection-trait-手动解糖-async.md)）。

### 5.4 PostgreSQL

```rust
use sz_orm_sqlx::{PgPoolHandle, SqlxPgConnectionFactory};

let handle = PgPoolHandle::connect("postgres://user:pass@localhost:5432/db").await?;
```

### 5.5 完整端到端示例

参考 [sz-orm使用指南.md §2.4 连接真实数据库](sz-orm使用指南.md#24-连接真实数据库sz-orm-sqlx)

---

## 第 6 章 · 连接池（对照 Swoole 连接池）

### 6.1 ThinkPHP / Swoole 连接池回顾

ThinkPHP 本身没有连接池，但 Swoole Runtime 提供：
```php
// Swoole 连接池
$pool = new Swoole\ConnectionPool(function() {
    return new PDO('mysql:host=localhost;dbname=test', 'root', '');
}, 100);  // 最多 100 个连接
$conn = $pool->get();
// 使用...
$pool->put($conn);
```

### 6.2 sz-orm 连接池

```rust
use sz_orm_core::{Pool, PoolConfigBuilder};
use std::time::Duration;

let config = PoolConfigBuilder::new()
    .max_size(100)                              // ← 最多 100 个连接
    .min_idle(10)                               // ← 最少保持 10 个空闲
    .acquire_timeout(30)                        // ← 获取超时 30 秒
    .idle_timeout(600)                          // ← 空闲超时 600 秒
    .max_lifetime(1800)                         // ← 最长生命周期 1800 秒
    .build()?;

let pool = Pool::new(config, factory)?;
let conn = pool.acquire().await?;              // ← $pool->get()
// 使用 conn...
pool.release(conn).await;                      // ← $pool->put()

pool.status().await;     // PoolStatus { idle, active, max, min }
pool.reap_idle().await;  // 清理空闲连接
pool.close_all().await;  // 关闭所有连接
```

### 6.3 配置参数对照

| Swoole 连接池 | sz-orm PoolConfigBuilder | 说明 |
|---------------|--------------------------|------|
| `$pool = new Pool(..., 100)` | `.max_size(100)` | 最大连接数 |
| - | `.min_idle(10)` | 最小空闲数（保活） |
| - | `.acquire_timeout(30)` | 获取超时 |
| - | `.idle_timeout(600)` | 空闲超时（自动回收） |
| - | `.max_lifetime(1800)` | 最长生命周期 |

### 6.4 详细 API

参考 [sz-ormAPI参考.md §1.4 Pool / Connection](sz-ormAPI参考.md#14-pool--connectionpool-rs)

---

## 第 7 章 · 事务（对照 ThinkPHP 事务）

### 7.1 ThinkPHP 事务回顾

```php
// PHP
Db::startTrans();
try {
    Db::table('accounts')->where('id', 1)->dec('balance', 100)->update();
    Db::table('accounts')->where('id', 2)->inc('balance', 100)->update();
    Db::commit();
} catch (\Exception $e) {
    Db::rollback();
    throw $e;
}

// 嵌套事务（ThinkPHP 用事务点）
Db::startTrans();
try {
    Db::table('users')->insert(['name' => 'Alice']);
    Db::startTrans();  // 实际是 SAVEPOINT
    try {
        Db::table('users')->insert(['name' => 'Bob']);
        Db::commitTrans();
    } catch (\Exception $e) {
        Db::rollbackTrans();
    }
    Db::commit();
} catch (\Exception $e) {
    Db::rollback();
}
```

### 7.2 sz-orm 事务

```rust
use sz_orm_core::{Transaction, TransactOptions, IsolationLevel};

// 1. 基本事务
let mut tx = Transaction::new(conn, TransactOptions::default());
tx.execute("UPDATE accounts SET balance = balance - 100 WHERE id = 1").await?;
tx.execute("UPDATE accounts SET balance = balance + 100 WHERE id = 2").await?;
tx.commit().await?;
// 出错时：
// tx.rollback().await?;

// 2. 保存点（嵌套事务）
let mut tx = Transaction::new(conn, TransactOptions::default());
tx.execute("INSERT INTO users (name) VALUES ('Alice')").await?;
let sp = tx.savepoint().await?;                    // ← SAVEPOINT sp_1
tx.execute("INSERT INTO users (name) VALUES ('Bob')").await?;
tx.rollback_to_savepoint(&sp).await?;              // ← 回滚 Bob，保留 Alice
tx.release_savepoint(&sp).await?;
tx.commit().await?;                                // ← 提交 Alice

// 3. 自定义隔离级别
let opts = TransactOptions::default()
    .with_isolation(IsolationLevel::Serializable)  // ← 隔离级别
    .read_only()                                    // ← 只读事务
    .with_timeout(Duration::from_secs(30));         // ← 超时
let mut tx = Transaction::new(conn, opts);
```

### 7.3 隔离级别对照

| MySQL 隔离级别 | sz-orm IsolationLevel |
|----------------|----------------------|
| READ UNCOMMITTED | `IsolationLevel::ReadUncommitted` |
| READ COMMITTED | `IsolationLevel::ReadCommitted` |
| REPEATABLE READ（默认） | `IsolationLevel::RepeatableRead` |
| SERIALIZABLE | `IsolationLevel::Serializable` |
| SNAPSHOT | `IsolationLevel::Snapshot` |

### 7.4 TransactionManager（多事务管理）

sz-orm 提供 `TransactionManager` 同时管理多个命名事务，类似分布式事务协调器：

```rust
let mgr = TransactionManager::new();
mgr.begin("tx1", conn1, opts).await?;
mgr.begin("tx2", conn2, opts).await?;
mgr.commit("tx1").await?;
mgr.rollback("tx2").await?;
mgr.list().await;   // 列出活跃事务
```

### 7.5 可运行示例

```powershell
cargo run -p sz-orm-examples --bin transaction
```

源码：[examples/src/bin/transaction.rs](../examples/src/bin/transaction.rs)

### 7.6 ADR 参考

事务嵌套设计的架构决策见 [ADR 0003](adr/0003-事务嵌套用-SAVEPOINT-加深度限制.md)

---

## 第 8 章 · 关联关系（对照 ThinkPHP 关联）

### 8.1 ThinkPHP 关联回顾

```php
// PHP: User 模型
class User extends Model {
    public function profile() { return $this->hasOne(Profile::class); }
    public function orders() { return $this->hasMany(Order::class); }
    public function role() { return $this->belongsTo(Role::class); }
    public function tags() { return $this->belongsToMany(Tag::class, 'user_tags'); }
}

// 使用
$user = User::with(['profile', 'orders'])->find(1);  // Eager Loading
```

### 8.2 sz-orm 关联定义

关联通过 `ModelExt::relations()` 返回的 HashMap 定义：

```rust
// Rust: 来自 examples/src/bin/model_definition.rs
use sz_orm_core::{BelongsTo, HasMany, Model, ModelExt, Relation};
use std::collections::HashMap;

impl ModelExt for Article {
    fn relations() -> HashMap<&'static str, Relation> {
        let mut map = HashMap::new();
        // BelongsTo: Article belongsTo User (author)
        map.insert(
            "author",
            Relation::BelongsTo(BelongsTo {
                foreign_key: "author_id".to_string(),       // 当前模型的外键
                parent_model: "users".to_string(),          // 父表名
                parent_pk: "id".to_string(),                // 父表主键
            }),
        );
        // HasMany: Article hasMany Comment
        map.insert(
            "comments",
            Relation::HasMany(HasMany {
                foreign_key: "article_id".to_string(),      // 子表外键
                child_model: "comments".to_string(),        // 子表名
                child_pk: "id".to_string(),                 // 子表主键
            }),
        );
        map
    }
}
```

### 8.3 四种关联类型对照

| ThinkPHP | sz-orm Relation 变体 | 关键字段 |
|----------|----------------------|----------|
| `belongsTo(Role::class)` | `Relation::BelongsTo(BelongsTo)` | foreign_key, parent_model, parent_pk |
| `hasMany(Order::class)` | `Relation::HasMany(HasMany)` | foreign_key, child_model, child_pk |
| `hasOne(Profile::class)` | `Relation::HasOne(HasOne)` | 同 HasMany |
| `belongsToMany(Tag::class, 'user_tags')` | `Relation::BelongsToMany(BelongsToMany)` | junction_table, foreign_key, other_key, target_model, target_pk |

### 8.4 多态关联（ThinkPHP 6+ 才有）

sz-orm 支持 MorphMany / MorphTo：

```rust
// 商品可附加多种媒体（图片/视频）— 来自 examples/src/bin/production_app.rs
Relation::MorphMany(MorphMany {
    child_model: "media".to_string(),
    morph_type_column: "attachable_type".to_string(),   // 存父类型名
    morph_id_column: "attachable_id".to_string(),       // 存父主键
    morph_type_value: "Product".to_string(),            // 当前父类型标识
})
// 生成的 SQL: SELECT * FROM media WHERE attachable_type = 'Product' AND attachable_id = ?
```

### 8.5 Eager Loading（预加载）

```rust
// 类似 ThinkPHP 的 User::with(['orders', 'profile'])->find(1)
let user = User { id: 1, name: "Alice".into(), /* ... */ }
    .with("orders")
    .with("profile")
    .load(&mut conn)
    .await?;
// load() 会执行 3 条 SQL：1 查 user + 1 查 orders + 1 查 profile
```

### 8.6 详细 API

参考 [sz-ormAPI参考.md §2.17 find_with_related](sz-ormAPI参考.md#217-find_with_relatedfind_with_related)

---

## 第 9 章 · 钩子系统（对照 ThinkPHP 模型事件）

### 9.1 ThinkPHP 模型事件回顾

```php
// PHP
class User extends Model {
    public static function onBeforeInsert($user) { $user->created_at = date('Y-m-d'); }
    public static function onAfterUpdate($user) { Log::info('user updated'); }
}
```

### 9.2 sz-orm 钩子：Hookable trait（编译期钩子）

```rust
// Rust: 来自 examples/src/bin/hooks_soft_delete.rs
use sz_orm_core::hooks::{HookContext, Hookable};
use sz_orm_core::{DbError, Model};

impl Hookable for Product {
    fn before_insert(_ctx: &mut HookContext) -> Result<(), DbError> {
        println!("[钩子] before_insert: 即将插入商品");
        Ok(())
    }
    fn after_insert(_ctx: &HookContext, _id: &Self::PrimaryKey) -> Result<(), DbError> {
        println!("[钩子] after_insert: 商品已插入, id={}", _id);
        Ok(())
    }
    fn before_delete(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> Result<(), DbError> {
        println!("[钩子] before_delete: 即将删除商品 id={}", _id);
        Ok(())
    }
}
```

### 9.3 16 种钩子事件

参考 [sz-ormAPI参考.md §4.2 HookEvent 16 种事件枚举](sz-ormAPI参考.md#42-hookevent-16-种事件枚举)

### 9.4 运行时钩子：HookRegistry

sz-orm 还支持运行时动态注册钩子（ThinkPHP 没有）：

```rust
use sz_orm_core::hooks::{HookEvent, HookRegistry};
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

let registry = HookRegistry::new();
let call_count = Arc::new(AtomicU32::new(0));
let counter = Arc::clone(&call_count);
registry.register(
    HookEvent::BeforeInsert,
    Arc::new(move |_ctx| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }),
);
registry.dispatch(HookEvent::BeforeInsert, &ctx).unwrap();
```

### 9.5 全局作用域：ScopeRegistry

```rust
use sz_orm_core::hooks::ScopeRegistry;

let scope_reg = ScopeRegistry::new();
scope_reg.disable("soft_delete");                  // 临时禁用软删除作用域
scope_reg.without_scope("tenant", || {             // 闭包内临时禁用
    // 这里查询不会附加 tenant_id 过滤
    42
});
```

### 9.6 可运行示例

```powershell
cargo run -p sz-orm-examples --bin hooks_soft_delete
```

源码：[examples/src/bin/hooks_soft_delete.rs](../examples/src/bin/hooks_soft_delete.rs)

---

## 第 10 章 · 软删除（对照 ThinkPHP 软删除）

### 10.1 ThinkPHP 软删除回顾

```php
class User extends Model {
    use SoftDelete;
    protected $deleteTime = 'delete_time';
}
// User::destroy(1);  → 实际执行 UPDATE SET delete_time = NOW()
// User::find(1);     → 自动 WHERE delete_time IS NULL
// User::withTrashed()->find(1);  → 查询包含已删除
```

### 10.2 sz-orm 软删除

```rust
// 1. 在 Model trait 里声明软删除字段
impl Model for Order {
    fn soft_delete_field() -> Option<&'static str> {
        Some("deleted_at")
    }
}

// 2. 实现 SoftDelete trait
use sz_orm_core::hooks::SoftDelete;
impl SoftDelete for Order {
    fn soft_delete_field() -> &'static str { "deleted_at" }
    fn is_deleted(&self) -> bool { self.deleted_at.is_some() }
}

// 3. SoftDeleteScope 全局作用域自动追加 WHERE deleted_at IS NULL
//    来自 examples/src/bin/hooks_soft_delete.rs
let scope_result = <(SoftDeleteScope, Product) as GlobalScope>::apply_scope(&ctx);
// → Some(("deleted_at IS NULL", []))

// 4. 临时禁用软删除（类似 withTrashed）
scope_reg.disable("soft_delete");
```

### 10.3 完整示例

`cargo run -p sz-orm-examples --bin production_app` 里的 `cancel_order` 方法演示了软删除：

```rust
// 来自 production_app.rs
fn cancel_order(&self, user: &str, order_id: i64) -> Result<String, String> {
    let mut data = HashMap::new();
    data.insert("deleted_at".to_string(), Value::String("2026-07-19 12:00:00".to_string()));
    data.insert("status".to_string(), Value::String("cancelled".to_string()));
    let sql = QueryBuilder::<Order>::new(self.new_dialect())
        .table("orders")
        .where_cond(format!("id = {}", order_id).as_str())
        .build_update(&data);
    Ok(sql)
}
```

---

## 第 11 章 · 迁移系统（对照 ThinkPHP migrate）

### 11.1 ThinkPHP migrate 回顾

```bash
php think migrate:create CreateUsersTable
php think migrate:run
php think migrate:rollback
```

```php
// PHP migration
public function up() {
    $this->table('users')
        ->addColumn('name', 'string')
        ->addColumn('email', 'string')
        ->create();
}
public function down() {
    $this->table('users')->drop()->save();
}
```

### 11.2 sz-orm 文件迁移

sz-orm 用**纯 SQL 文件**做迁移，不用 PHP/Rust 代码：

```
migrations/
├── 001_create_users_up.sql
├── 001_create_users_down.sql
├── 002_add_email_index_up.sql
└── 002_add_email_index_down.sql
```

```rust
use sz_orm_core::migration::{FileMigrationResolver, MigrationContext, Migrator};
use sz_orm_core::{MigrationResolver, DbType};

let resolver = FileMigrationResolver::new("./migrations".into());
let migrations = resolver.resolve(DbType::MySQL)?;

let mut migrator = Migrator::new(MigrationContext::default())
    .add_migrations(migrations);

migrator.migrate().await?;                // 应用所有待迁移
migrator.up(Some("003")).await?;           // 应用至 003
migrator.down(Some("001")).await?;         // 回滚至 001
migrator.rollback("002").await?;           // 回滚单个
migrator.reset().await?;                   // 回滚所有 + 重新应用
migrator.refresh().await?;                 // reset 别名
migrator.progress();                       // 迁移进度
```

### 11.3 SchemaBuilder（程序化 DDL）

类似 ThinkPHP 的 `$this->table()`：

```rust
use sz_orm_core::migration::{SchemaBuilder, ColumnDef, IndexDef, ForeignKeyDef};

let sql = SchemaBuilder::new("users")
    .add_column(ColumnDef::new("id", "BIGINT").not_null().auto_increment())
    .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null())
    .add_index(IndexDef::new("idx_email", vec!["email"]).unique())
    .add_foreign_key(
        ForeignKeyDef::new("fk_role", "role_id", "roles", "id").on_delete("CASCADE")
    )
    .build(DbType::MySQL);
```

### 11.4 可运行示例

```powershell
cargo run -p sz-orm-examples --bin migration
```

源码：[examples/src/bin/migration.rs](../examples/src/bin/migration.rs)

### 11.5 CLI 工具

sz-orm 提供 CLI 工具简化迁移：

```bash
cargo install --path cli    # 安装 sz-orm CLI
sz-orm migrate:up
sz-orm migrate:rollback
sz-orm model:gen            # 从数据库生成 Model 代码
```

---

## 第 12 章 · SQL 安全（ThinkPHP 没有的能力）

### 12.1 sz-orm 的三层安全防护

1. **编译期校验**：`sql_string!` 宏在编译时拦截 SQL 注入
2. **运行时校验**：`validate()` 方法检查生成的 SQL
3. **标识符白名单**：`sql_safety::validate_identifier` 校验表名/列名

### 12.2 编译期 SQL 校验（最强）

```rust
use sz_orm_core::sql_string;

// ✅ 正确 SQL，编译通过
let sql = sql_string!("SELECT * FROM users WHERE id = 1");

// ✅ 参数化查询，编译通过
let sql = sql_string!("SELECT * FROM users WHERE name = ?"; params: "Alice");

// ❌ 拼写错误，编译失败（注意 FORM 应为 FROM）
// let sql = sql_string!("SELECT * FORM users");

// ❌ SQL 注入模式，编译失败
// let sql = sql_string!("SELECT * FROM users WHERE name = 'x' OR '1'='1'");
```

宏会检测 12 种注入模式，包括：UNION 注入、注释注入、布尔盲注、时间盲注等。

### 12.3 运行时校验

```rust
let result = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .where_cond("id = 1")
    .validate()?;   // 返回 Result<(), DbError>
```

### 12.4 标识符白名单

防止表名/列名注入：

```rust
use sz_orm_core::sql_safety;

// ✅ 合法标识符
sql_safety::validate_identifier("users", "table name")?;
sql_safety::validate_identifier("email", "column name")?;

// ❌ 非法标识符
// sql_safety::validate_identifier("users; DROP TABLE", "table name")?;
// → 返回 Err(DbError::InvalidInput)
```

### 12.5 安全设计 ADR

标识符校验为什么用白名单而非 quote，见 [ADR 0002](adr/0002-SQL标识符校验用白名单而非-quote.md)

---

## 第 13 章 · 生产案例：电商订单系统

这是最值得学习的示例，整合了 sz-orm 的 6 个扩展包：

```powershell
cargo run -p sz-orm-examples --bin production_app
```

源码：[examples/src/bin/production_app.rs](../examples/src/bin/production_app.rs)（685 行，完整可运行）

### 13.1 业务流程

1. **用户注册**：PBKDF2 密码哈希 → INSERT users
2. **用户登录**：密码校验 → 签发 JWT
3. **浏览商品**：多态关联（商品 → 图片/视频/评论）
4. **下单**：限流检查 → 事务（扣库存 + INSERT 订单 + 清购物车）
5. **取消订单**：软删除（设置 deleted_at）
6. **定时任务**：每分钟扫描超时订单自动取消
7. **审计日志**：所有 SQL 操作落库（敏感字段自动脱敏）

### 13.2 集成的扩展包

| 扩展包 | 作用 | 对应 ThinkPHP 能力 |
|--------|------|---------------------|
| `sz-orm-core` | Model + QueryBuilder + 关联 + 软删除 | think-orm 核心 |
| `sz-orm-crypto` | PBKDF2 密码哈希 | `password_hash()` / `password_verify()` |
| `sz-orm-auth` | JWT 鉴权 | firebase/php-jwt |
| `sz-orm-limit` | 滑动窗口限流 | 限流中间件 |
| `sz-orm-scheduler` | Cron 定时任务 | crontab / think-queue |
| `sz-orm-audit` | SQL 审计日志（敏感字段脱敏） | 自定义日志中间件 |

### 13.3 阅读建议

1. 先跑一遍 `cargo run -p sz-orm-examples --bin production_app`，看输出
2. 打开 [production_app.rs](../examples/src/bin/production_app.rs) 从 `main()` 开始读
3. 重点看 `AppState` 结构体，它把 6 个扩展包组合起来
4. 每个 `register` / `login` / `place_order` / `cancel_order` 方法都对应一个业务场景

---

## 第 14 章 · 用 AI 驱动开发 sz-orm（核心章节）

你现在的角色是**用 AI 协作维护 sz-orm**。这一章讲怎么高效提问、怎么验证 AI 输出、怎么避免常见陷阱。

### 14.1 AI 协作的正确姿势

| 场景 | 错误提问 | 正确提问 |
|------|----------|----------|
| 加新 Model | "帮我加个订单模型" | "参照 examples/src/bin/model_definition.rs 的 Article 模型，新增 Order 模型，字段：id(i64)、user_id(i64)、product_id(i64)、quantity(i64)、total_price(f64)、status(String)、deleted_at(Option<String>)，实现 Model + ModelExt，包含软删除" |
| 修改 QueryBuilder | "改下查询逻辑" | "在 packages/sz-orm-core/src/query.rs 的 build_select 方法里，当 where_conditions 为空时不要输出 WHERE 关键字" |
| 加测试 | "加个测试" | "参照 packages/sz-orm-core/tests/contracts/query_contract.rs 的 test_select_basic，新增 test_select_with_having，验证 having 条件拼接正确" |
| 修 bug | "这代码有 bug" | "运行 `cargo test -p sz-orm-core test_pool_acquire` 失败，错误信息：`PoolError::Timeout`，期望行为：池满时应等待而非立即超时" |

### 14.2 验证 AI 输出的 5 步法

每次 AI 生成代码后，**必须**执行：

```powershell
# 1. 编译检查
cargo check --workspace

# 2. 格式检查（AI 经常漏空格）
cargo fmt --all -- --check

# 3. 静态检查（AI 经常写出低效代码）
cargo clippy --workspace --all-targets -- -D warnings

# 4. 测试（AI 经常漏边界情况）
cargo test --workspace

# 5. 文档检查（如果有公共 API 变更）
cargo doc --workspace --no-deps
```

如果任何一步失败，把错误贴回给 AI 让它修，**不要手动改**（AI 改 AI 的代码更快）。

### 14.3 阅读源码的顺序

sz-orm 有 39 个包，不要乱看。推荐顺序：

1. **[examples/src/bin/quick_start.rs](../examples/src/bin/quick_start.rs)** — 30 分钟，了解 QueryBuilder
2. **[examples/src/bin/model_definition.rs](../examples/src/bin/model_definition.rs)** — 1 小时，了解完整 Model
3. **[examples/src/bin/hooks_soft_delete.rs](../examples/src/bin/hooks_soft_delete.rs)** — 1 小时，了解钩子
4. **[examples/src/bin/transaction.rs](../examples/src/bin/transaction.rs)** — 30 分钟，了解事务
5. **[examples/src/bin/migration.rs](../examples/src/bin/migration.rs)** — 30 分钟，了解迁移
6. **[examples/src/bin/multi_tenant.rs](../examples/src/bin/multi_tenant.rs)** — 1 小时，了解多租户
7. **[examples/src/bin/production_app.rs](../examples/src/bin/production_app.rs)** — 2 小时，整合所有
8. **[examples/src/bin/production_dtx.rs](../examples/src/bin/production_dtx.rs)** — 2 小时，分布式事务

读完 examples 后再读 `packages/sz-orm-core/src/`，推荐顺序：
1. `lib.rs` — 公共 API 导出
2. `value.rs` — Value 类型系统
3. `model.rs` — Model trait
4. `query.rs` — QueryBuilder
5. `dialect.rs` — 方言抽象
6. `pool.rs` — 连接池
7. `transaction.rs` — 事务
8. `hooks.rs` — 钩子系统
9. `migration.rs` — 迁移
10. `sql_safety.rs` — SQL 安全

### 14.4 常见 AI 陷阱

1. **AI 不知道 sz-orm 的约定**：比如 controller 方法不要参数、主键从 `$data` 取。每次提问时附上相关规范。
2. **AI 会编造 API**：sz-orm 的 API 和 ThinkPHP 不同，AI 可能按 ThinkPHP 写。让它先读源码再写。
3. **AI 会忽略错误处理**：Rust 的 `Result` 必须处理，AI 经常写 `unwrap()`。要求它用 `?` 运算符。
4. **AI 会忽略所有权**：AI 经常写出 move 后还用变量的代码。`cargo check` 会抓到。
5. **AI 会写多余的抽象**：Rust 社区崇尚"零成本抽象"，不要过度设计。

### 14.5 提问模板

```
任务：[具体描述]
上下文：
- 文件：[文件路径]
- 参考实现：[相关文件路径]
- 约定：[项目规范，如"controller 不要方法参数"]

要求：
1. 先读取相关文件理解现有代码
2. 修改/新增后必须通过 cargo check + clippy + test
3. 不要改变现有 API 的返回格式
4. 修改后说明改了哪些文件、为什么改

验证：
- 运行 `cargo test -p [package] [test_name]` 应通过
```

---

## 第 15 章 · 多租户（对照 ThinkPHP 多租户）

### 15.1 sz-orm 多租户

```rust
// 实现 TenantModel trait
use sz_orm_core::TenantModel;

impl TenantModel for Order {
    fn tenant_field() -> &'static str { "tenant_id" }
}

// TenantScope 全局作用域自动追加 WHERE tenant_id = ?
// 类似 ThinkPHP 的多租户中间件，但在 ORM 层实现
```

### 15.2 可运行示例

```powershell
cargo run -p sz-orm-examples --bin multi_tenant
```

源码：[examples/src/bin/multi_tenant.rs](../examples/src/bin/multi_tenant.rs)

---

## 第 16 章 · 分布式事务（高级，ThinkPHP 无对应）

### 16.1 三种分布式事务模式

sz-orm-dtx 提供：
- **2PC**（两阶段提交）：强一致，性能低
- **TCC**（Try-Confirm-Cancel）：业务补偿，性能中
- **Saga**：长事务，最终一致，性能高

### 16.2 可运行示例

```powershell
cargo run -p sz-orm-examples --bin production_dtx
```

源码：[examples/src/bin/production_dtx.rs](../examples/src/bin/production_dtx.rs)

### 16.3 进一步阅读

参考 [sz-ormAPI参考.md §2.15 sz-orm-dtx](sz-ormAPI参考.md#215-sz-orm-dtx分布式事务扩展)

---

## 附录 A · ThinkPHP ↔ sz-orm 速查表

| ThinkPHP | sz-orm | 文档位置 |
|----------|--------|----------|
| `Model::find(1)` | `QueryBuilder::build_select()` + 执行 | [§3](#第-3-章--crud-操作对照-thinkphp-dbmodel) |
| `Model::where('id', 1)->find()` | `qb.where_cond("id = 1").build_select()` | [§3](#第-3-章--crud-操作对照-thinkphp-dbmodel) |
| `Model::create($data)` | `qb.build_insert(&data)` | [§3](#第-3-章--crud-操作对照-thinkphp-dbmodel) |
| `Model::update($data)` | `qb.build_update(&data)` | [§3](#第-3-章--crud-操作对照-thinkphp-dbmodel) |
| `Model::destroy(1)` | `qb.build_delete()` | [§3](#第-3-章--crud-操作对照-thinkphp-dbmodel) |
| `Model::with('rel')->find()` | `model.with("rel").load(&mut conn).await` | [§8](#第-8-章--关联关系对照-thinkphp-关联) |
| `Db::startTrans()` | `Transaction::new(conn, opts)` | [§7](#第-7-章--事务对照-thinkphp-事务) |
| `Db::commit()` | `tx.commit().await` | [§7](#第-7-章--事务对照-thinkphp-事务) |
| `use SoftDelete` | `impl SoftDelete for T` | [§10](#第-10-章--软删除对照-thinkphp-软删除) |
| 模型事件 `onBeforeInsert` | `impl Hookable` + `before_insert` | [§9](#第-9-章--钩子系统对照-thinkphp-模型事件) |
| `php think migrate:run` | `migrator.migrate().await` | [§11](#第-11-章--迁移系统对照-thinkphp-migrate) |
| `password_hash()` | `Pbkdf2Hasher::hash()` | [§13](#第-13-章--生产案例电商订单系统) |
| JWT | `JwtAuthenticator::authenticate()` | [§13](#第-13-章--生产案例电商订单系统) |

---

## 附录 B · 文档索引（已验证全部存在）

| 文档 | 用途 | 路径 |
|------|------|------|
| 本教程 | 入门学习 | `docs/sz-orm学习路线图.md` |
| 使用指南 | 端到端用法 | [sz-orm使用指南.md](sz-orm使用指南.md) |
| API 参考 | API 速查 | [sz-ormAPI参考.md](sz-ormAPI参考.md) |
| 架构设计 | 39 包架构 | [sz-orm架构设计.md](sz-orm架构设计.md) |
| 工程实践 | 测试金字塔/Soak | [sz-orm-engineering-practices.md](sz-orm-engineering-practices.md) |
| API 契约 | 公共 API 稳定性约束 | [api-contracts.md](api-contracts.md) |
| ADR 索引 | 5 条架构决策 | [adr/README.md](adr/README.md) |
| 安全策略 | 漏洞报告流程 | [../SECURITY.md](../SECURITY.md) |
| 贡献指南 | 代码规范/PR 流程 | [../CONTRIBUTING.md](../CONTRIBUTING.md) |
| 变更日志 | 版本变更 | [../CHANGELOG.md](../CHANGELOG.md) |
| 项目 README | 项目概览 | [../README.md](../README.md) |
| 示例代码 | 8 个可运行示例 | [../examples/src/bin/](../examples/src/bin/) |

---

## 附录 C · 常见问题

### Q: 我不会 Rust，能用 AI 维护 sz-orm 吗？

A: 能，但有前提：
1. 完成第 0 章 Rust 速通（30 分钟）
2. 能读懂 `cargo check` / `cargo clippy` 的错误信息
3. 知道何时该问 AI、何时该读源码（第 14 章详述）

### Q: sz-orm 和 ThinkORM 的 API 差异大吗？

A: 概念上一一对应（Model、QueryBuilder、关联、事务、软删除），但语法差异大（Rust vs PHP）。最大的区别：
1. sz-orm QueryBuilder 只生成 SQL，不执行（执行需 sz-orm-sqlx）
2. sz-orm 是强类型，所有值必须包装成 `Value` 枚举
3. sz-orm 所有 I/O 都是 async，必须 `.await`

### Q: 如何参与贡献？

A: 阅读 [../CONTRIBUTING.md](../CONTRIBUTING.md)，重点：
1. Fork → 分支 → PR
2. 必须通过 `cargo fmt` + `cargo clippy -D warnings` + `cargo test`
3. 遵循 [Conventional Commits](https://www.conventionalcommits.org/) 规范
4. 新增功能必须有测试

### Q: 遇到 bug 怎么办？

A: 
1. 先读 [sz-orm使用指南.md §7 故障排除](sz-orm使用指南.md#七故障排除)
2. 在 [GitHub Issues](https://github.com/ljclz/sz-orm/issues) 搜索
3. 按 [../SECURITY.md](../SECURITY.md) 流程报告（安全漏洞）或开 Issue（普通 bug）

### Q: 支持哪些数据库？

11 种：MySQL、PostgreSQL、SQLite、Oracle 23ai、OceanBase、SQL Server、ClickHouse、Redis、MongoDB、VectorDB、PureJsDb。详见 [../README.md#支持的数据库](../README.md#支持的数据库)

---

## 学习节奏建议

| 阶段 | 时间 | 内容 | 产出 |
|------|------|------|------|
| 第 1 天 | 4h | 第 0-3 章 + 跑通 quick_start | 能编译、能改 Model |
| 第 2 天 | 6h | 第 4-7 章 + 跑通 model_definition / transaction | 能写 CRUD + 事务 |
| 第 3 天 | 6h | 第 8-11 章 + 跑通 hooks_soft_delete / migration | 能用关联、钩子、迁移 |
| 第 4 天 | 4h | 第 12-13 章 + 读 production_app | 能整合扩展包 |
| 第 5 天 | 4h | 第 14 章 + 用 AI 改一个真实需求 | 能独立维护 |

**关键原则**：每章都要**跑通示例 + 读源码 + 改一点代码**，不要只看不动手。
