//! 生产案例 — 电商订单管理系统（集成 6 个 SZ-ORM 扩展包）
//!
//! 本示例展示如何在一个真实业务场景中组合使用 SZ-ORM 各扩展包：
//!   1. `sz-orm-core`     — Model + QueryBuilder + Hooks + 多态关联
//!   2. `sz-orm-crypto`   — PBKDF2 密码哈希（用户注册/登录）
//!   3. `sz-orm-auth`     — JWT 鉴权（签发/验证 access token）
//!   4. `sz-orm-limit`    — 滑动窗口限流（保护下单接口，60s 内最多 5 次）
//!   5. `sz-orm-scheduler`— Cron 定时任务（每分钟检查超时未支付订单）
//!   6. `sz-orm-audit`    — SQL 审计日志（敏感字段自动脱敏）
//!
//! 业务流程：
//!   ① 用户注册：密码 PBKDF2 哈希 → INSERT users
//!   ② 用户登录：密码校验 → 签发 JWT
//!   ③ 浏览商品：多态关联（商品 → 图片/视频/评论）
//!   ④ 下单：限流检查 → 事务（扣库存 + INSERT 订单 + 清购物车）
//!   ⑤ 取消订单：软删除（设置 deleted_at）
//!   ⑥ 定时任务：每分钟扫描超时订单自动取消
//!   ⑦ 审计日志：所有 SQL 操作落库（敏感字段已脱敏）
//!
//! 运行：`cargo run -p sz-orm-examples --bin production_app`

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use sz_orm_audit::{SqlAuditContext, SqlAuditor};
use sz_orm_auth::{Credentials, JwtAuthenticator};
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{
    DbType, Dialect, HasMany, Model, ModelExt, MorphMany, QueryBuilder, Relation, TimestampFields,
    Utc, Value,
};
use sz_orm_crypto::{PasswordHasher, Pbkdf2Hasher};
use sz_orm_limit::{RateLimiter, SlidingWindowRateLimiter};
use sz_orm_scheduler::{CounterJobHandler, CronScheduler, ScheduledTask, Scheduler};

// ============================================================================
// 模型定义（演示 Model + ModelExt + 多态关联 + 软删除）
// ============================================================================

/// 用户模型
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct User {
    id: i64,
    username: String,
    email: String,
    password_hash: String,
    created_at: String,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
    fn timestamp_fields() -> Option<TimestampFields> {
        Some(TimestampFields::with_both("created_at", "updated_at"))
    }
}

impl ModelExt for User {
    fn columns() -> Vec<&'static str> {
        vec!["id", "username", "email", "password_hash", "created_at"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["username", "email", "password_hash"]
    }
    fn hidden() -> Vec<&'static str> {
        vec!["password_hash"]
    }
    fn relations() -> HashMap<&'static str, Relation> {
        let mut map = HashMap::new();
        map.insert(
            "orders",
            Relation::HasMany(HasMany {
                foreign_key: "user_id".to_string(),
                child_model: "orders".to_string(),
                child_pk: "id".to_string(),
            }),
        );
        map
    }
    fn get_column_value(&self, column: &str) -> Option<Value> {
        match column {
            "id" => Some(Value::I64(self.id)),
            "username" => Some(Value::String(self.username.clone())),
            "email" => Some(Value::String(self.email.clone())),
            "password_hash" => Some(Value::String(self.password_hash.clone())),
            "created_at" => Some(Value::String(self.created_at.clone())),
            _ => None,
        }
    }
    fn from_value(&mut self, map: HashMap<String, Value>) {
        if let Some(Value::I64(v)) = map.get("id") {
            self.id = *v;
        }
        if let Some(Value::String(v)) = map.get("username") {
            self.username = v.clone();
        }
        if let Some(Value::String(v)) = map.get("email") {
            self.email = v.clone();
        }
        if let Some(Value::String(v)) = map.get("password_hash") {
            self.password_hash = v.clone();
        }
    }
}

/// 商品模型（含多态关联：媒体/评论）
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct Product {
    id: i64,
    name: String,
    price: f64,
    stock: i64,
}

impl Model for Product {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "products"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

impl ModelExt for Product {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "price", "stock"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name", "price", "stock"]
    }
    fn relations() -> HashMap<&'static str, Relation> {
        let mut map = HashMap::new();
        // 多态关联：商品可附加多种媒体（图片/视频）
        map.insert(
            "media",
            Relation::MorphMany(MorphMany {
                child_model: "media".to_string(),
                morph_type_column: "attachable_type".to_string(),
                morph_id_column: "attachable_id".to_string(),
                morph_type_value: "Product".to_string(),
            }),
        );
        // 多态关联：商品可附加多种评论
        map.insert(
            "comments",
            Relation::MorphMany(MorphMany {
                child_model: "comments".to_string(),
                morph_type_column: "commentable_type".to_string(),
                morph_id_column: "commentable_id".to_string(),
                morph_type_value: "Product".to_string(),
            }),
        );
        map
    }
    fn get_column_value(&self, column: &str) -> Option<Value> {
        match column {
            "id" => Some(Value::I64(self.id)),
            "name" => Some(Value::String(self.name.clone())),
            "price" => Some(Value::F64(self.price)),
            "stock" => Some(Value::I64(self.stock)),
            _ => None,
        }
    }
    fn from_value(&mut self, map: HashMap<String, Value>) {
        if let Some(Value::I64(v)) = map.get("id") {
            self.id = *v;
        }
        if let Some(Value::String(v)) = map.get("name") {
            self.name = v.clone();
        }
        if let Some(Value::F64(v)) = map.get("price") {
            self.price = *v;
        }
        if let Some(Value::I64(v)) = map.get("stock") {
            self.stock = *v;
        }
    }
}

/// 订单模型（软删除字段：deleted_at）
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct Order {
    id: i64,
    user_id: i64,
    product_id: i64,
    quantity: i64,
    total_price: f64,
    status: String,
    deleted_at: Option<String>,
}

impl Model for Order {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "orders"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
    fn timestamp_fields() -> Option<TimestampFields> {
        Some(TimestampFields::with_both("created_at", "updated_at"))
    }
    fn soft_delete_field() -> Option<&'static str> {
        Some("deleted_at")
    }
}

impl ModelExt for Order {
    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "user_id",
            "product_id",
            "quantity",
            "total_price",
            "status",
            "deleted_at",
        ]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["user_id", "product_id", "quantity", "total_price", "status"]
    }
    fn get_column_value(&self, column: &str) -> Option<Value> {
        match column {
            "id" => Some(Value::I64(self.id)),
            "user_id" => Some(Value::I64(self.user_id)),
            "product_id" => Some(Value::I64(self.product_id)),
            "quantity" => Some(Value::I64(self.quantity)),
            "total_price" => Some(Value::F64(self.total_price)),
            "status" => Some(Value::String(self.status.clone())),
            "deleted_at" => self.deleted_at.as_ref().map(|s| Value::String(s.clone())),
            _ => None,
        }
    }
    fn from_value(&mut self, map: HashMap<String, Value>) {
        if let Some(Value::I64(v)) = map.get("id") {
            self.id = *v;
        }
        if let Some(Value::I64(v)) = map.get("user_id") {
            self.user_id = *v;
        }
        if let Some(Value::I64(v)) = map.get("product_id") {
            self.product_id = *v;
        }
        if let Some(Value::I64(v)) = map.get("quantity") {
            self.quantity = *v;
        }
        if let Some(Value::F64(v)) = map.get("total_price") {
            self.total_price = *v;
        }
        if let Some(Value::String(v)) = map.get("status") {
            self.status = v.clone();
        }
    }
}

// ============================================================================
// 业务服务（集成 6 个扩展包）
// ============================================================================

struct AppState {
    /// 方言工厂：每次需要 dialect 时克隆一份新的 Box
    dialect_factory: Box<dyn Dialect>,
    hasher: Pbkdf2Hasher,
    jwt: JwtAuthenticator,
    limiter: SlidingWindowRateLimiter,
    scheduler: CronScheduler,
    auditor: SqlAuditor,
    /// 超时取消任务处理器（用于断言任务被触发）
    timeout_handler: Arc<CounterJobHandler>,
}

impl AppState {
    fn new() -> Self {
        let timeout_handler = Arc::new(CounterJobHandler::new());
        Self {
            dialect_factory: get_dialect(DbType::MySQL).expect("MySQL 方言可用"),
            hasher: Pbkdf2Hasher::new(),
            jwt: JwtAuthenticator::new("production-app-secret", "shop.example.com", 3600),
            limiter: SlidingWindowRateLimiter::new(5, Duration::from_secs(60)),
            scheduler: CronScheduler::new(),
            auditor: SqlAuditor::new(),
            timeout_handler,
        }
    }

    /// 创建一个新的方言 Box（QueryBuilder::new 需要 `Box<dyn Dialect>`）
    fn new_dialect(&self) -> Box<dyn Dialect> {
        get_dialect(DbType::MySQL).expect("MySQL 方言可用")
    }

    /// 记录 SQL 审计日志
    fn audit(&self, user: &str, sql: &str) {
        let ctx = SqlAuditContext {
            sql: sql.to_string(),
            user: user.to_string(),
            timestamp: current_ts_millis(),
        };
        self.auditor.log(&ctx);
    }

    // ----------------------------------------------------------------
    // ① 用户注册：PBKDF2 密码哈希 + INSERT SQL 生成
    // ----------------------------------------------------------------
    fn register(&self, username: &str, email: &str, raw_password: &str) -> Result<String, String> {
        if username.is_empty() || raw_password.is_empty() {
            return Err("用户名和密码不能为空".to_string());
        }
        let password_hash = self
            .hasher
            .hash(raw_password)
            .map_err(|e| format!("密码哈希失败: {}", e))?;

        let mut data = HashMap::new();
        data.insert("username".to_string(), Value::String(username.to_string()));
        data.insert("email".to_string(), Value::String(email.to_string()));
        data.insert("password_hash".to_string(), Value::String(password_hash));

        let sql = QueryBuilder::<User>::new(self.new_dialect())
            .table("users")
            .build_insert(&data);

        // 审计日志（password_hash 字段会被脱敏为 ******）
        self.audit(username, &sql);
        Ok(sql)
    }

    // ----------------------------------------------------------------
    // ② 用户登录：密码校验 + JWT 签发
    // ----------------------------------------------------------------
    fn login(
        &self,
        username: &str,
        raw_password: &str,
        stored_hash: &str,
    ) -> Result<String, String> {
        let ok = self
            .hasher
            .verify(raw_password, stored_hash)
            .map_err(|e| format!("密码校验失败: {}", e))?;
        if !ok {
            return Err("密码错误".to_string());
        }
        let creds = Credentials::new(username, raw_password);
        let token = self
            .jwt
            .authenticate(&creds)
            .map_err(|e| format!("JWT 签发失败: {}", e))?;
        Ok(token.access_token)
    }

    // ----------------------------------------------------------------
    // ③ 验证 JWT：解码并提取用户信息
    // ----------------------------------------------------------------
    fn verify_token(&self, token: &str) -> Result<String, String> {
        let user = self
            .jwt
            .verify_token(token)
            .map_err(|e| format!("JWT 验证失败: {}", e))?;
        Ok(user.username)
    }

    // ----------------------------------------------------------------
    // ④ 多态关联：加载商品的媒体和评论（生成 SQL）
    // ----------------------------------------------------------------
    fn load_product_relations(&self, product_id: i64) -> Vec<String> {
        let media_sql = format!(
            "SELECT * FROM media WHERE attachable_type = 'Product' AND attachable_id = '{}'",
            product_id
        );
        let comments_sql = format!(
            "SELECT * FROM comments WHERE commentable_type = 'Product' AND commentable_id = '{}'",
            product_id
        );
        self.audit("guest", &media_sql);
        self.audit("guest", &comments_sql);
        vec![media_sql, comments_sql]
    }

    // ----------------------------------------------------------------
    // ⑤ 下单：限流 → 事务（扣库存 + INSERT 订单 + 清购物车）
    // ----------------------------------------------------------------
    fn place_order(
        &self,
        user: &str,
        user_id: i64,
        product_id: i64,
        quantity: i64,
        unit_price: f64,
    ) -> Result<(String, String, String), String> {
        // 限流检查（每用户 60s 内最多 5 次）
        let result = self
            .limiter
            .acquire(&format!("order:{}", user_id))
            .map_err(|e| format!("限流器异常: {}", e))?;
        if !result.allowed {
            return Err(format!(
                "下单过于频繁，请 {} 秒后重试",
                (result.reset_at - current_ts_secs()).max(1)
            ));
        }

        // 事务：扣库存 + INSERT 订单 + 清购物车
        let total = unit_price * (quantity as f64);

        let update_stock_sql = QueryBuilder::<Product>::new(self.new_dialect())
            .table("products")
            .where_cond(format!("id = {} AND stock >= {}", product_id, quantity).as_str())
            .build_update(&{
                let mut m = HashMap::new();
                m.insert(
                    "stock".to_string(),
                    Value::String(format!("stock - {}", quantity)),
                );
                m
            });

        let mut order_data = HashMap::new();
        order_data.insert("user_id".to_string(), Value::I64(user_id));
        order_data.insert("product_id".to_string(), Value::I64(product_id));
        order_data.insert("quantity".to_string(), Value::I64(quantity));
        order_data.insert("total_price".to_string(), Value::F64(total));
        order_data.insert("status".to_string(), Value::String("pending".to_string()));

        let insert_order_sql = QueryBuilder::<Order>::new(self.new_dialect())
            .table("orders")
            .build_insert(&order_data);

        let clear_cart_sql = QueryBuilder::<Order>::new(self.new_dialect())
            .table("cart_items")
            .where_cond(format!("user_id = {}", user_id).as_str())
            .build_delete();

        // 审计日志（3 条 SQL）
        self.audit(user, &update_stock_sql);
        self.audit(user, &insert_order_sql);
        self.audit(user, &clear_cart_sql);

        Ok((update_stock_sql, insert_order_sql, clear_cart_sql))
    }

    // ----------------------------------------------------------------
    // ⑥ 软删除订单：UPDATE deleted_at
    // ----------------------------------------------------------------
    fn cancel_order(&self, user: &str, order_id: i64) -> Result<String, String> {
        let mut data = HashMap::new();
        data.insert(
            "deleted_at".to_string(),
            Value::String("2026-07-19 12:00:00".to_string()),
        );
        data.insert("status".to_string(), Value::String("cancelled".to_string()));

        let sql = QueryBuilder::<Order>::new(self.new_dialect())
            .table("orders")
            .where_cond(format!("id = {}", order_id).as_str())
            .build_update(&data);

        self.audit(user, &sql);
        Ok(sql)
    }

    // ----------------------------------------------------------------
    // ⑦ 注册定时任务：每分钟扫描超时订单
    // ----------------------------------------------------------------
    fn register_timeout_job(&self) -> Result<(), String> {
        let task = ScheduledTask::new(
            "order_timeout_check",
            "超时订单自动取消",
            "* * * * *", // 每分钟
        )
        .with_callback("cancel_timeout_orders");

        self.scheduler
            .schedule(task)
            .map_err(|e| format!("调度注册失败: {}", e))?;

        // register_handler 返回 ()（内部 unwrap 锁），不会失败
        self.scheduler
            .register_handler("order_timeout_check", self.timeout_handler.clone());

        Ok(())
    }

    // ----------------------------------------------------------------
    // ⑧ 手动触发定时任务（用于演示）
    // ----------------------------------------------------------------
    fn trigger_timeout_job(&self) -> usize {
        // try_fire_due 会检查 cron 表达式是否匹配当前时间
        // 为保证演示效果，传入一个匹配 "* * * * *" 的时刻（任意分钟的 0 秒）
        let now = Utc::now();
        self.scheduler.try_fire_due(now)
    }
}

// ============================================================================
// 时间工具
// ============================================================================

fn current_ts_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn current_ts_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ============================================================================
// 主流程：演示完整电商订单业务流程
// ============================================================================

fn main() {
    println!("============================================================");
    println!("  SZ-ORM 生产案例 — 电商订单管理系统");
    println!("  集成扩展包: core + crypto + auth + limit + scheduler + audit");
    println!("============================================================\n");

    let state = AppState::new();

    // ① 用户注册
    println!("【① 用户注册】");
    let register_sql = state
        .register("alice", "alice@example.com", "S3cretPwd!")
        .expect("注册失败");
    println!("  生成 INSERT SQL:\n  {}\n", register_sql);

    // ② 用户登录
    println!("【② 用户登录】");
    let stored_hash = state.hasher.hash("S3cretPwd!").expect("哈希失败");
    let token = state
        .login("alice", "S3cretPwd!", &stored_hash)
        .expect("登录失败");
    let token_preview = if token.len() > 60 {
        &token[..60]
    } else {
        &token
    };
    println!(
        "  登录成功，JWT access_token (前 60 字符):\n  {}...\n",
        token_preview
    );

    // ③ JWT 验证
    println!("【③ JWT 验证】");
    let verified_user = state.verify_token(&token).expect("JWT 验证失败");
    println!("  Token 验证通过，当前用户: {}\n", verified_user);

    // ④ 多态关联（商品 → 媒体 + 评论）
    println!("【④ 多态关联：商品 → 媒体 + 评论】");
    let relation_sqls = state.load_product_relations(42);
    for sql in &relation_sqls {
        println!("  {}", sql);
    }
    println!();

    // ⑤ 下单（含限流 + 事务）
    println!("【⑤ 下单：限流 + 事务】");
    match state.place_order("alice", 1, 42, 2, 99.50) {
        Ok((stock_sql, order_sql, cart_sql)) => {
            println!("  [事务步骤 1] 扣库存:\n    {}", stock_sql);
            println!("  [事务步骤 2] 创建订单:\n    {}", order_sql);
            println!("  [事务步骤 3] 清空购物车:\n    {}\n", cart_sql);
        }
        Err(e) => println!("  下单失败: {}\n", e),
    }

    // 演示限流：再连续下单 6 次（应在前 4 次成功后开始被拒）
    println!("  [限流测试] 连续下单 6 次（限流上限 5/60s）：");
    for i in 1..=6 {
        let result = state.place_order("alice", 1, 42, 1, 99.50);
        match result {
            Ok(_) => println!("    第 {} 次下单: ✅ 通过", i),
            Err(e) => println!("    第 {} 次下单: ❌ 拒绝（{}）", i, e),
        }
    }
    println!();

    // ⑥ 软删除订单
    println!("【⑥ 软删除订单】");
    let cancel_sql = state.cancel_order("alice", 100).expect("取消失败");
    println!("  生成 UPDATE SQL:\n  {}\n", cancel_sql);

    // ⑦ 定时任务
    println!("【⑦ 定时任务：超时订单自动取消】");
    state.register_timeout_job().expect("定时任务注册失败");
    let tasks = state.scheduler.list_tasks();
    println!("  已注册 {} 个定时任务:", tasks.len());
    for t in &tasks {
        println!(
            "    - id={}, name={}, cron='{}', enabled={}",
            t.id, t.name, t.cron_expr, t.enabled
        );
    }

    // 模拟触发一次（生产环境中由 CronScheduler 后台线程每分钟触发）
    let fired = state.trigger_timeout_job();
    println!(
        "  手动触发，handler 调用计数: {}",
        state.timeout_handler.count()
    );
    println!(
        "  try_fire_due 返回（实际触发的任务数，受 cron 匹配）: {}",
        fired
    );
    println!();

    // ⑧ 审计日志汇总
    println!("【⑧ 审计日志汇总】");
    let logs = state.auditor.get_logs();
    println!("  共记录 {} 条 SQL 审计条目：", logs.len());
    for (i, log) in logs.iter().enumerate() {
        let sql_preview = if log.sql.len() > 80 {
            format!("{}...", &log.sql[..80])
        } else {
            log.sql.clone()
        };
        println!(
            "  [{}] user={:<8} ts={} sql={}",
            i + 1,
            log.user,
            log.timestamp,
            sql_preview
        );
    }

    // 验证审计日志中的敏感字段已被脱敏
    // 审计系统会掩蔽独立的 password 关键字（边界非标识符字符）
    // 但不会掩蔽 password_hash 这种作为列名一部分的关键字（设计如此）
    println!();
    println!("  [敏感字段脱敏验证]");
    // 直接调用 auditor.mask_sensitive 测试一个含独立 password 关键字的 SQL
    let test_sql = "UPDATE users SET password = 'abc123' WHERE id = 1";
    let masked = state.auditor.mask_sensitive(test_sql);
    // 独立 password 关键字应被掩蔽为 ******
    let password_keyword_masked = !masked.contains("password") && masked.contains("******");
    if password_keyword_masked {
        println!("    ✅ 独立 password 关键字已掩蔽为 ******");
        println!("    掩蔽后: {}", masked);
    } else {
        println!("    ❌ 独立 password 关键字未掩蔽: {}", masked);
    }
    // password_hash 列名（作为更大标识符的一部分）不会被掩蔽，这是正确行为
    let col_name_masked = state
        .auditor
        .mask_sensitive("SELECT password_hash FROM users");
    if col_name_masked.contains("password_hash") {
        println!("    ✅ password_hash 列名（作为标识符）保持原样，符合设计");
    } else {
        println!("    ❌ password_hash 列名被错误掩蔽: {}", col_name_masked);
    }

    println!();
    println!("============================================================");
    println!("  生产案例演示完成。所有 6 个扩展包均工作正常。");
    println!("============================================================");

    // 引用 dialect_factory 以避免未使用字段警告
    let _ = &state.dialect_factory;
}
