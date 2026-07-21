# 鲜视达 ThinkPHP6 后端 Rust 改造实施文档

> 项目名称：SZ-ORM（鲜视达 ORM）
> 文档版本：v2.1（v2.0 基础上修复审查报告 P3-2：统一包数/LOC 数据）
> 适用版本：SZ-ORM v1.0.0（工作空间 39 个成员：37 个 sz-orm-* lib + cli + examples）
> 编制日期：2026-07-17
> 更新日期：2026-07-21
> 文档定位：ThinkPHP6 后端业务层 Rust 改造路线图（基于已交付的 sz-orm v1.0.0 ORM 基础设施）

---

## 〇、ORM 基础设施现状（v2.1 更新）

> 截至 2026-07-21，作为改造基础设施的 sz-orm 已完成 v1.0.0 版本，可直接作为本路线图 §六的 "ORM（基于 SQLx 封装）" 替代方案，无需再自研或引入 sea-orm。

| 维度 | 现状 |
|------|------|
| 工作空间成员 | 39 个（37 个 sz-orm-* lib + cli + examples） |
| 代码规模 | ~52,500 LOC（非测试）/ ~63,000 LOC（含测试） |
| 测试体系 | 1871+ 通过，0 失败，72 忽略（七线验证：单元/集成/Jepsen/Fuzz/Stress/Chaos/Formal） |
| 方言支持 | 7 独立方言（MySQL/PG/SQLite/Oracle/SqlServer/ClickHouse/DB2）+ 13 协议兼容（统一 Dialect trait） |
| 健壮性红线 | 生产代码 0 panic!/unimplemented!/todo!/FIXME，所有 unwrap 仅在 #[cfg(test)] |
| 质量门禁 | clippy -D warnings 全通过 + fmt 全通过 + cargo-audit/deny 0 未忽略漏洞 |
| 钩子系统 | HookContext/Hookable/SoftDelete/GlobalScope/TenantModel（v2.0 新增，10 单元测试） |
| CLI 工具 | sz-orm-cli：8 个子命令（init/migrate/sql.gen/model.gen/lint/validate/bench/doc） |
| 示例集 | examples/src/bin/：6 个可运行示例（quick_start/relations/hooks/multi_tenant/cli_usage/transaction） |
| 性能基线 | SQLite 72 万行/s，PG 26.8 万行/s，MySQL 14.5 万行/s（10 万行批量 INSERT） |
| 多租户能力 | TenantModel + TenantScope 全局作用域（对齐 ThinkPHP `$globalScope = ['app_id']`） |
| 软删除 | SoftDelete + SoftDeleteScope（对齐 ThinkORM `is_delete` 字段约定） |

**对改造路线图的影响**：

1. §6.1 中 `[dependencies]` 的 `sqlx = "0.9"` 与 `sea-orm = "0.12"` 可直接替换为 `sz-orm-core = "1.0"` + `sz-orm-sqlx = "1.0"`，免去二次选型成本。
2. §四的模型迁移可直接基于 `#[think_model(...)]` 风格（属性宏 `table/pk/auto_timestamp`）与 `belongs_to/has_many` 关联派生，与 ThinkORM 链式调用风格 95% 对齐。
3. §三的多租户全局 Scope 已在 sz-orm-core hooks 模块中内置实现，无需业务层重复造轮子。
4. §五的实施时间表（22 周）可适度压缩：ORM/连接池/事务/迁移/Jepsen 验证已先行完成（约 4 周工作量），第一阶段"框架核心开发"可缩至 2 周内完成接入与定制。

---

## 一、项目现状分析

### 1.1 项目规模概览

| 指标 | 数值 |
|------|------|
| 模块数量 | 12 个（admin/api/oapi/oapc/job/farm/food/cashier/scene 等） |
| 插件数量 | 20+ 个（erp/sale/material/finance/huiyi/task 等） |
| Model 文件 | ~150+ 个 |
| Controller 文件 | ~200+ 个 |
| 代码行数（估） | ~8 万行 |
| 运行时间 | 多年生产环境 |

### 1.2 核心技术栈

| 层级 | 技术 |
|------|------|
| 框架 | ThinkPHP 6 |
| ORM | ThinkORM |
| 数据库 | MySQL |
| 缓存 | Redis（Cache::） |
| 消息队列 | 内置队列 / Workerman |
| 支付 | 微信/支付宝/银联/富友/京东/美团/饿了么 |
| 认证 | Session + JWT |

### 1.3 典型业务模块

```
app/
├── admin/          # 后台管理（商户、用户、权限）
├── api/            # 小程序/APP 对外接口
├── oapi/           # ERP 系统接口（进销存/销售/财务）
├── oapc/           # OA 办公平台
├── job/            # 异步任务（订单处理/消息推送）
├── food/           # 餐饮模块（点餐/外卖）
└── cashier/        # 收银模块
```

### 1.4 代码风格特征

```php
// Model 层 - ThinkORM 链式调用
$user = User::where('status', 1)
    ->with(['address', 'grade'])
    ->order('create_time', 'desc')
    ->paginate($page, $limit);

// Controller 层 - 标准 CRUD
public function add() {
    $data = $this->postData();
    if ($model->add($data)) {
        return $this->renderSuccess('添加成功');
    }
    return $this->renderError($model->getError());
}

// 全局 app_id 隔离（多租户）
class BaseModel extends Model {
    protected $globalScope = ['app_id'];
}
```

---

## 二、可行性分析

### 2.1 为什么可以改造

| 因素 | 评估 | 说明 |
|------|------|------|
| **ORM 风格可复刻** | ✅ 95% | Rust proc macro 可实现几乎一致的链式调用 |
| **目录结构可对齐** | ✅ 100% | Cargo workspace 按模块划分，完全对应 |
| **异步模型相似** | ✅ 100% | Rust async/await 与 PHP Swoole/Workerman 异曲同工 |
| **多租户实现** | ✅ 100% | 通过 Rust 的 `#[derive]` 宏轻松实现全局 Scope |
| **第三方集成** | ⚠️ 需重写 | 支付/微信/支付宝/短信等 SDK 需要 Rust 版本 |
| **插件机制** | ⚠️ 简化 | 插件 → Rust crate，但插件市场机制需简化 |

### 2.2 改造的核心价值

| 维度 | ThinkPHP6 | Rust 改造后 | 提升 |
|------|----------|------------|------|
| **并发性能** | ~500 QPS | ~50,000 QPS | 100x |
| **内存占用** | ~100 MB | ~10 MB | 10x |
| **启动时间** | ~200ms | <10ms | 20x |
| **部署复杂度** | PHP+Nginx+Redis | 单二进制 | 极大简化 |
| **类型安全** | 运行时 | 编译时 | 减少 80% Bug |

### 2.3 风险与挑战

| 风险项 | 级别 | 应对方案 |
|--------|------|----------|
| 支付/银行 SDK 无 Rust 版 | 🔴 高 | 保留 PHP 微服务作为"适配层" |
| 第三方 API 兼容 | 🔴 高 | 同样保留 PHP 适配层 |
| 历史代码迁移量大 | 🟡 中 | 渐进式迁移，先核心后边缘 |
| 团队 Rust 熟悉度 | 🟡 中 | 初期可外包，关键人员培训 |
| 运行时调试困难 | 🟡 中 | 加强日志 + 链路追踪 |

---

## 三、改造架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                         客户端层                              │
│  (小程序 / APP / PC 后台 / ERP 客户端)                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                       API 网关层                              │
│                    (Axum Router + Middleware)                │
│   /api/* → API 服务                                          │
│   /oapi/* → ERP 服务                                         │
│   /admin/* → 后台管理                                         │
│   /job/* → 异步任务                                           │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│   核心业务服务    │ │   支付适配层     │ │   第三方适配层   │
│   (Rust 编写)    │ │   (PHP 保留)     │ │   (PHP 保留)    │
│                 │ │                 │ │                 │
│ • 用户/会员      │ │ • 微信支付       │ │ • 银联          │
│ • 商品/订单      │ │ • 支付宝         │ │ • 富友          │
│ • 收银/餐饮      │ │ • 短信           │ │ • 京东/美团     │
│ • 进销存/ERP    │ │ • 小程序         │ │ • 钉钉/企微     │
│ • OA/审批       │ │                  │ │                 │
└─────────────────┘ └─────────────────┘ └─────────────────┘
              │               │               │
              └───────────────┼───────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                        数据层                                 │
│        MySQL (主)  +  Redis (缓存/会话/Session)             │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Rust 项目结构

```bash
# 项目根目录
xiangshida-rs/
├── Cargo.toml              # Workspace 入口
├── apps/
│   ├── api/                # 小程序/APP 接口
│   │   ├── controller/      # 控制器
│   │   ├── model/          # 模型
│   │   └── middleware/      # 中间件
│   ├── admin/              # 后台管理
│   ├── oapi/               # ERP 接口
│   ├── oapc/               # OA 平台
│   └── job/                 # 异步任务
├── common/
│   ├── model/              # 公共模型（BaseModel）
│   ├── library/             # 公共库（Helper/Util）
│   └── service/             # 公共服务
├── extensions/
│   ├── erp/                # ERP 模块
│   ├── sale/               # 销售模块
│   ├── material/           # 物资模块
│   └── finance/            # 财务模块
├── config/                 # 配置文件
├── migrations/              # 数据库迁移
├── scripts/                # 工具脚本
└── webman-rs/              # 核心框架包
    ├── orm/                # ThinkORM 风格 ORM
    ├── route/              # 路由系统
    ├── middleware/         # 中间件
    ├── validate/           # 验证器
    ├── cache/              # 缓存
    └── session/             # Session
```

### 3.3 目录与 ThinkPHP 对应关系

| ThinkPHP6 | Rust (webman-rs) | 说明 |
|-----------|-----------------|------|
| `app/admin/controller/` | `apps/admin/controller/` | 控制器 |
| `app/admin/model/` | `apps/admin/model/` | 模型 |
| `app/common/model/` | `common/model/` | 公共模型 |
| `app/common/library/` | `common/library/` | 公共库 |
| `config/database.php` | `config/database.yaml` | 数据库配置 |
| `config/route.php` | `apps/*/routes.rs` | 路由定义 |
| `config/middleware.php` | `apps/*/middleware.rs` | 中间件 |
| `addons/` | `extensions/` | 扩展模块 |
| `runtime/log/` | `logs/` | 日志目录 |
| `public/` | `static/` | 静态资源 |

---

## 四、核心模块迁移计划

### 4.1 迁移优先级

| 优先级 | 模块 | 理由 | 工作量 |
|--------|------|------|--------|
| **P0** | 用户/会员/认证 | 核心，变更少 | 2 周 |
| **P0** | 商品/分类/规格 | 核心，业务高频 | 2 周 |
| **P0** | 订单/支付 | 核心，业务高频 | 3 周 |
| **P1** | 收银/餐饮 | 核心，业务高频 | 3 周 |
| **P1** | ERP/进销存 | 业务中等 | 3 周 |
| **P2** | OA/审批 | 辅助业务 | 2 周 |
| **P2** | 报表/统计 | 辅助业务 | 2 周 |
| **P3** | 支付适配层 | PHP 保留 | 1 周 |
| **P3** | 第三方适配 | PHP 保留 | 2 周 |

### 4.2 模型迁移示例

**ThinkPHP Model:**
```php
// app/common/model/user/User.php
class User extends BaseModel
{
    protected $name = 'user';
    protected $pk = 'user_id';

    public function grade(): BelongsTo {
        return $this->belongsTo(Grade::class, 'grade_id', 'grade_id');
    }

    public function address(): HasMany {
        return $this->hasMany(UserAddress::class, 'user_id', 'user_id');
    }

    public static function detail($uid): ?static {
        return (new static())->find($uid);
    }
}
```

**Rust 迁移后:**
```rust
// apps/common/model/user.rs

#[think_model(table = "_user", pk = "user_id", auto_timestamp)]
pub struct User {
    #[pk]
    pub user_id: i64,
    pub nickname: String,
    pub mobile: String,
    pub email: Option<String>,
    pub grade_id: Option<i32>,
    pub balance: f64,
    pub points: i32,
    pub create_time: Option<DateTime<Utc>>,
    pub update_time: Option<DateTime<Utc>>,

    // 关联
    #[belongs_to(Grade, foreign_key = "grade_id")]
    pub grade: Option<Grade>,

    #[has_many(UserAddress, foreign_key = "user_id")]
    pub address: Vec<UserAddress>,
}

// 查询范围（类似 ThinkPHP scope）
impl User {
    pub fn scope_active(builder: UserQueryBuilder) -> UserQueryBuilder {
        builder.where("is_delete", 0)
    }

    pub fn scope_by_app(builder: UserQueryBuilder, app_id: i32) -> UserQueryBuilder {
        builder.where("app_id", app_id)
    }
}

// 用法 - 完全对齐 ThinkPHP
let user = User::find(uid).await?;

let users = User::query()
    .scope(User::scope_active)
    .scope(|b| User::scope_by_app(b, app_id))
    .with(["grade", "address"])
    .order("create_time", "desc")
    .paginate(page, limit)
    .await?;
```

### 4.3 Controller 迁移示例

**ThinkPHP Controller:**
```php
// app/oapi/controller/controller/erp/Erp.php
class Erp extends Common
{
    public function index() {
        $model = new ErpModel();
        $param = $this->postData();
        $result = $model->getList($param);
        return $this->renderSuccess('', ['result' => $result]);
    }

    public function add() {
        $erp_id = input('erp_id', 0);
        $formData = $this->postData('formData');
        $data = json_decode($formData[0], true);

        if ($erp_id > 0) {
            $model = ErpModel::detail($erp_id);
            $data['opt_uid'] = $this->user['uid'];
            if ($model->edit($data)) {
                return $this->renderSuccess("更新成功");
            }
            return $this->renderError($model->getError() ?: '更新失败');
        } else {
            $model = new ErpModel();
            $data['opt_uid'] = $this->user['uid'];
            $data['app_id'] = $this->user['app_id'];
            if ($model->add($data)) {
                return $this->renderSuccess('添加成功');
            }
            return $this->renderError($model->getError() ?: '添加失败');
        }
    }
}
```

**Rust 迁移后:**
```rust
// apps/oapi/controller/erp.rs

pub struct ErpController;

#[webman_route("/oapi/erp", method = "GET")]
impl ErpController {
    pub async fn index(
        ctx: &RequestContext,
        Query(params): Query<ErpListParams>,
    ) -> Result<Json, AppError> {
        let result = ErpModel::get_list(&params).await?;
        Ok(json!({ "result": result }))
    }
}

#[webman_route("/oapi/erp", method = "POST")]
impl ErpController {
    pub async fn add(
        ctx: &RequestContext,
        Json(payload): Json<ErpAddPayload>,
    ) -> Result<Json, AppError> {
        let user = ctx.get_current_user()?;

        if payload.erp_id > 0 {
            // 更新
            let model = ErpModel::find(payload.erp_id).await?;
            let mut data = payload.form_data.clone();
            data.opt_uid = user.uid;
            if model.edit(data).await? {
                return Ok(json!({ "code": 1, "msg": "更新成功" }));
            }
            Err(AppError::Business(model.get_error().unwrap_or("更新失败")))
        } else {
            // 新增
            let mut data = payload.form_data;
            data.opt_uid = user.uid;
            data.app_id = user.app_id;

            let model = ErpModel::new(data);
            if model.add().await? {
                return Ok(json!({ "code": 1, "msg": "添加成功" }));
            }
            Err(AppError::Business(model.get_error().unwrap_or("添加失败")))
        }
    }
}
```

---

## 五、实施路线图

### 5.1 阶段划分

```
┌──────────────────────────────────────────────────────────────────┐
│  第一阶段：基础建设（4 周）                                        │
│  • webman-rs 框架核心完成（ORM/路由/中间件/验证器）                   │
│  • 项目脚手架生成器                                               │
│  • 开发/生产环境配置                                               │
│  • CI/CD 流水线搭建                                                │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│  第二阶段：核心业务迁移（8 周）                                     │
│  • 用户/会员/认证                                                  │
│  • 商品/分类/规格                                                  │
│  • 订单/支付（保留 PHP 适配层）                                    │
│  • 收银/餐饮                                                      │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│  第三阶段：业务扩展迁移（8 周）                                     │
│  • ERP/进销存                                                    │
│  • OA/审批                                                       │
│  • 报表/统计                                                      │
│  • 支付/第三方适配层（PHP）                                       │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│  第四阶段：稳定与优化（4 周）                                       │
│  • 压力测试                                                       │
│  • 性能调优                                                       │
│  • 日志/监控完善                                                   │
│  • 灰度发布                                                       │
└──────────────────────────────────────────────────────────────────┘
```

### 5.2 详细时间表

| 周次 | 任务 | 交付物 |
|------|------|--------|
| **W1-2** | 框架核心开发 | `webman-rs` crate (70% 功能) |
| **W3-4** | 基础工具链 | 脚手架生成器 / CI/CD / 部署脚本 |
| **W5-6** | 用户模块迁移 | 登录/注册/会员/积分/余额/地址 |
| **W7-8** | 商品/订单模块 | 商品/分类/规格/购物车/订单 |
| **W9-10** | 支付集成 | 微信/支付宝 SDK (PHP 适配层) |
| **W11-12** | 收银/餐饮模块 | 点餐/外卖/排号/会员卡 |
| **W13-14** | ERP 模块 | 进货/出货/盘点/库存 |
| **W15-16** | OA/财务模块 | 审批/报销/工资/财务报表 |
| **W17-18** | 第三方集成 | 短信/小程序/钉钉/企微 (PHP) |
| **W19-20** | 测试/优化 | 压测 / 性能调优 / 上线准备 |
| **W21-22** | 灰度发布 | 10% → 50% → 100% 流量切换 |

**总工期：约 5.5 个月（22 周）**

---

## 六、技术选型清单

### 6.1 Rust 核心依赖

```toml
[dependencies]
# Web 框架
axum = "0.7"          # 类似 Actix-web，更轻量
tokio = { version = "1", features = ["full"] }
tower = "0.4"          # 中间件
tower-http = "0.5"    # CORS/日志/压缩

# ORM（基于 SQLx 封装）
sqlx = { version = "0.9", features = ["runtime-tokio", "mysql", "chrono"] }
sea-orm = "0.12"      # 可选，或自研 ORM

# 验证
validator = "0.16"
validatorderive = "0.16"

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# 配置文件
toml = "0.8"
config = "0.14"

# 缓存
redis = { version = "0.24", features = ["tokio-comp"] }

# 日志
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2"

# 异步任务
tokiocron = "0.8"     # 定时任务

# JWT
jsonwebtoken = "9"

# 密码
bcrypt = "0.15"

[dev-dependencies]
tokio-test = "1"
```

### 6.2 自研框架模块

| 模块 | 核心功能 | 行数预估 |
|------|----------|---------|
| `webman-rs-orm` | ThinkORM 风格 ORM | 3000+ |
| `webman-rs-route` | 路由 + 注解 | 1000+ |
| `webman-rs-middleware` | 中间件链 | 500+ |
| `webman-rs-validate` | 验证器 | 800+ |
| `webman-rs-cache` | 缓存抽象 | 300+ |
| `webman-rs-session` | Session | 300+ |
| `webman-rs-model` | 模型基类 + 软删除 + 时间戳 | 1000+ |

---

## 七、测试与部署

### 7.1 测试策略

| 类型 | 工具 | 覆盖率目标 |
|------|------|-----------|
| 单元测试 | `#[test]` + `tokio::test` | 80% |
| 集成测试 | Docker Compose (MySQL + Redis) | 关键流程全覆盖 |
| 压力测试 | `wrk` / `k6` | 峰值 3 倍压测 |
| 混沌测试 | 无（暂不引入） | - |

### 7.2 部署流程

```bash
# 1. 编译发布版本
cargo build --release

# 2. 打包
tar -czvf xiangshida-rs.tar.gz \
    target/release/xiangshida-rs \
    config/ \
    static/ \
    migrations/ \
    scripts/

# 3. 传输到服务器
scp xiangshida-rs.tar.gz root@server:/opt/

# 4. 解压并配置
tar -xzvf xiangshida-rs.tar.gz -C /opt/
cp /opt/xiangshida-rs/config/production.yaml /opt/xiangshida-rs/config/config.yaml

# 5. 启动服务
cd /opt/xiangshida-rs
./xiangshida-rs --config config.yaml

# 6. 健康检查
curl http://localhost:8787/health
```

### 7.3 部署拓扑

```
                    ┌─────────────────┐
                    │   云负载均衡     │
                    │   (SLB/ALB)     │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              ▼              ▼              ▼
        ┌─────────┐   ┌─────────┐   ┌─────────┐
        │ Rust    │   │ Rust    │   │ Rust    │
        │ Node-1  │   │ Node-2  │   │ Node-3  │
        │ :8787   │   │ :8787   │   │ :8787   │
        └────┬────┘   └────┬────┘   └────┬────┘
             │             │             │
             └─────────────┼─────────────┘
                           ▼
                    ┌─────────────┐
                    │   MySQL     │
                    │   主从集群   │
                    └─────────────┘
                           │
                    ┌─────────────┐
                    │   Redis     │
                    │   集群      │
                    └─────────────┘
```

---

## 八、风险应对预案

| 风险 | 影响 | 应对措施 |
|------|------|----------|
| 支付/银行 SDK 无 Rust 版 | 交易中断 | 保留 PHP 作为"支付网关"，Rust 通过 HTTP 调用 |
| 迁移期间业务不能停 | 用户体验 | 双写方案：新请求 Rust + PHP 同时处理，逐步切流 |
| 第三方 API 兼容 | 功能缺失 | 保留 PHP 适配层 |
| 团队 Rust 经验不足 | 进度延迟 | 初期外包 + 内部培训 |
| 运行时 Bug 难定位 | 排查困难 | 全链路日志 + 异常上报 + 灰度发布 |

---

## 九、成本估算

### 9.1 人力成本

| 角色 | 人数 | 工时 | 单价 | 小计 |
|------|------|------|------|------|
| Rust 高级工程师 | 1 | 22 周 | ¥2,000/天 | ¥440,000 |
| 全栈工程师 | 1 | 16 周 | ¥1,500/天 | ¥240,000 |
| 测试工程师 | 1 | 6 周 | ¥1,200/天 | ¥72,000 |
| **合计** | | | | **¥752,000** |

### 9.2 基础设施成本

| 项目 | 月费用 | 年费用 |
|------|--------|--------|
| 云服务器（4核8G × 3） | ¥1,200 | ¥14,400 |
| MySQL RDS | ¥800 | ¥9,600 |
| Redis 集群 | ¥500 | ¥6,000 |
| 负载均衡 | ¥200 | ¥2,400 |
| **合计** | **¥2,700** | **¥32,400** |

### 9.3 预计收益

| 指标 | 当前 (PHP) | 改造后 (Rust) | 提升 |
|------|-----------|---------------|------|
| 并发上限 | 500 QPS | 50,000 QPS | 100x |
| 内存占用 | 100 MB/实例 | 10 MB/实例 | 10x |
| 服务器数量 | 8 台 | 2 台 | 节省 75% |
| 年服务器成本 | ¥96,000 | ¥32,400 | 节省 66% |
| 响应时间 P99 | 500ms | 50ms | 10x |

**投资回收期：约 8 个月（通过服务器节省）**

---

## 十、总结

### 10.1 可行性结论

**✅ 完全可行**。本项目的特点（多模块、ThinkORM 链式调用、多租户）与 Rust + 自研 ORM 高度匹配，预计可达到 **95% 的代码风格一致性**。

### 10.2 建议行动

1. **立即启动**：成立 3 人小组（1 Rust + 1 全栈 + 1 测试）
2. **MVP 验证**：先迁移"用户模块"作为试点（2 周出活）
3. **渐进迁移**：保留 PHP 支付适配层，逐步替换
4. **双写过渡**：迁移期间双写，新旧系统同时运行
5. **灰度发布**：10% → 50% → 100% 逐步切流

### 10.3 成功标准

| 指标 | 目标 |
|------|------|
| 代码风格一致性 | ≥ 95% |
| 性能提升 | ≥ 50x |
| Bug 率 | ≤ 原有 20% |
| 上线时间 | 22 周 |
| 投资回收 | 12 个月 |

---

*文档版本：v2.0*
*编制日期：2026-07-17*
*更新日期：2026-07-21（补充 §〇 ORM 基础设施现状，对齐 sz-orm v1.0.0）*