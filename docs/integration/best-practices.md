# 通用集成最佳实践

> 版本：v9.0.0 | 更新日期：2026-09-26

## 1. Feature Gate 隔离原则

所有 sz-orm 扩展能力通过 feature gate 隔离，下游项目按需启用：

```toml
[dependencies]
sz-orm-core = "9.0.0"
sz-orm-sqlx = "9.0.0"

[features]
ai-nl2sql = ["dep:sz-orm-ai", "sz-orm-ai/ai-nl2sql-enhanced"]
ai-index-advisor = ["dep:sz-orm-ai", "sz-orm-ai/ai-index-advisor"]
```

**规则**：
- 每个扩展能力一个 feature gate
- feature gate 同时控制依赖引入（`dep:`）和上游 feature 传递
- 默认不启用任何扩展 feature

## 2. 接线层模式

每个 sz-orm 包的接线应封装在独立的 `sz_orm_*.rs` 文件中：

```
src/
├── sz_orm_ai.rs     # AI 接线
├── sz_orm_dtx.rs    # 分布式事务接线
├── sz_orm_obs.rs    # 可观测接线
└── ...
```

**规则**：
- 接线文件只做 API 转发，不包含业务逻辑
- 错误类型转换为下游项目的统一错误类型
- 每个公共函数附 `// 生产调用点：file:line` 注释

## 3. 错误转换模式

```rust
use sz_orm_ai::nl2sql::Nl2SqlError;

// sz-orm 错误 → 下游统一错误
sz_orm_error.map_err(|e: Nl2SqlError| AppError::Validation {
    message: format!("NL2SQL 转换失败: {e}"),
    data: None,
})
```

## 4. 全局状态模式（可观测）

```rust
use std::sync::OnceLock;

static METRICS: OnceLock<MetricsHandles> = OnceLock::new();

fn metrics() -> &'static MetricsHandles {
    METRICS.get_or_init(|| {
        // 懒初始化
    })
}
```

## 5. Saga 分布式事务模式

```rust
// 1. 定义前向操作和补偿操作
let saga = Saga::new("tx-123")
    .with_step(SagaStep::new("step1")
        .with_action(|| do_something())
        .with_compensation(|| undo_something()));

// 2. 注册并执行
let manager = SagaManager::new();
manager.register(saga)?;
let result = manager.execute("tx-123")?;

// 3. 根据结果处理
match result {
    SagaResult::Success => { /* 提交 */ },
    SagaResult::Compensated { .. } => { /* 已回滚 */ },
    SagaResult::CompensationFailed { .. } => { /* 人工介入 */ },
}
```

## 6. 测试策略

- **单元测试**：接线函数的输入输出映射
- **边界测试**：空输入、非法输入、资源不存在
- **集成测试**：真实 sz-orm API 调用链路
- **补偿测试**：Saga 失败后补偿正确执行