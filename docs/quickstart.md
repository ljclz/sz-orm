# sz-orm 快速开始

> 目标：从零到跑通第一个数据库查询。提供两条路径：
> - **路径 A（零数据库依赖）**：SQLite 内存库，无需安装任何数据库。首次编译依机器性能约 1~10 分钟。
> - **路径 B（真实数据库，约 5~15 分钟，含首次编译）**：MySQL/PostgreSQL + 连接池 CRUD。

## 路径 A：零依赖快速体验

```bash
# 1. 克隆仓库并进入
git clone <repo-url> sz-orm && cd sz-orm

# 2. 运行真实数据库端到端示例（SQLite 内存库，含连接池 + CRUD + 事务）
cargo run -p sz-orm-examples --bin real_db_crud
```

`real_db_crud`（[examples/src/bin/real_db_crud.rs](../examples/src/bin/real_db_crud.rs)）演示：
连接池创建 → DDL 建表 → INSERT → SELECT → UPDATE → DELETE → 事务提交/回滚，全程使用 SQLite 内存数据库，无外部依赖。

## 路径 B：真实数据库（MySQL/PostgreSQL）

### 步骤 1：启动数据库

已安装 Docker 时，一键启动 MySQL + PostgreSQL：

```bash
docker compose up -d mysql postgres
```

- MySQL：`127.0.0.1:8802`（docker-compose.yml 将容器 3306 映射到宿主机 8802，避免与本机已有 MySQL 冲突），用户 `shop` / 密码 `shop_pwd`，库 `shop`
- PostgreSQL：`127.0.0.1:5432`，用户 `szorm` / 密码 `szorm_pwd`，库 `szorm`

无 Docker 时，使用本机已有的 MySQL/PostgreSQL 实例即可，连接串按实际修改；并提前建好库与用户：

```sql
-- MySQL（root 或有权限的账号执行）
CREATE DATABASE shop;
CREATE USER 'shop'@'%' IDENTIFIED BY 'shop_pwd';
GRANT ALL ON shop.* TO 'shop'@'%';

-- PostgreSQL
CREATE USER szorm WITH PASSWORD 'szorm_pwd';
CREATE DATABASE szorm OWNER szorm;
```

### 步骤 2：新建项目并添加依赖（path 依赖，始终使用当前版本）

新建你自己的项目（与克隆的 `sz-orm` 目录同级，例如都在 `D:\code\` 下）：

```bash
cargo new my_app
cd my_app
```

在 `my_app/Cargo.toml` 中添加依赖（path 指向克隆仓库内的包；下例假设 `my_app` 与 `sz-orm` 同级，相对路径按你的目录关系调整）：

```toml
[dependencies]
sz-orm-core = { path = "../sz-orm/packages/sz-orm-core" }
sz-orm-sqlx = { path = "../sz-orm/packages/sz-orm-sqlx" }
tokio = { version = "1.40", features = ["full"] }
```

> crates.io 上的 sz-orm-core 版本可能滞后于仓库，最新能力请使用 path 依赖（生产试点 sz-pay 同样使用 path 依赖）。

### 步骤 3：生成 Model（使用 CLI，不必手写）

安装 CLI：

```bash
cargo install --path cli
```

生成 Model 骨架（**在你的项目根目录**执行，生成到 `./src/models` 下；首次 `cargo install` 需编译 CLI，约 1~3 分钟）：

```bash
sz-orm make:model User --pk-type i64
```

CLI 会在 `./src/models` 下生成 `user.rs`（含字段、主键、表名、时间戳），比手写 `impl Model` 更快。生成后按需补充业务字段即可。

### 步骤 4：连接数据库并执行查询

```rust,no_run
use std::sync::Arc;
use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{MySqlPoolHandle, SqlxMySqlConnectionFactory};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // MySQL 连接（路径 B 步骤 1 的 docker 实例，端口 8802；无 Docker 时按本机实例修改）
    let handle = MySqlPoolHandle::connect("mysql://shop:<your-password>@127.0.0.1:8802/shop").await?;
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(Arc::new(handle)));
    let config = PoolConfigBuilder::new().max_size(10).build()?;
    let pool = Pool::new(config, factory)?;

    let mut conn = pool.acquire().await?;
    let rows = conn.query("SELECT 1 AS one").await?;
    println!("rows = {}", rows.len());
    Ok(())
}
```

PostgreSQL 替换为 `PgPoolHandle` + `SqlxPgConnectionFactory`，连接串换成 `postgres://szorm:<your-password>@127.0.0.1:5432/szorm`。

### 步骤 5：运行完整 CRUD 示例

以下命令需回到**克隆的 sz-orm 仓库根目录**执行（运行仓库内置示例，与你的 `my_app` 是两个独立项目）：

```bash
# SQLite 内存版（零依赖）
cargo run -p sz-orm-examples --bin real_db_crud

# 生产模式示例（电商订单系统，集成 6 个扩展包）
cargo run -p sz-orm-examples --bin production_app
```

## 下一步

- 完整使用指南：[docs/sz-orm使用指南.md](sz-orm使用指南.md)
- API 参考：[docs/sz-ormAPI参考.md](sz-ormAPI参考.md)
- 全部 38 个可运行示例清单见根 [README.md](../README.md) 的 Examples 章节
- 竞品对比（SQLx/Diesel/SeaORM）：[docs/sz-orm与同类产品对比分析.md](sz-orm与同类产品对比分析.md)