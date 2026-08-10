# QueryBuilder 迁移路线图

> 版本：v3.6.0
> 日期：2026-08-10

## 1. 背景

sz-orm 存在两个 QueryBuilder 实现：
- **`sz-orm-query-builder`**（旧版）：独立包，已标注 `#[deprecated]`
- **`sz_orm_core::QueryBuilder`**（新版）：核心包内建，推荐使用

v3.6.0 提供迁移工具，计划 v3.7.0 正式移除旧版。

## 2. 三阶段计划

### 2.1 v3.6.0（当前版本）

- ✅ 提供 `qb_migration_lint` 模块：检测 `sz_orm_query_builder::Query` 使用并输出告警
- ✅ 提供 `qb_migration_fix` 模块：自动将旧版 API 转换为新版等价 API
- ✅ 保持 `sz-orm-query-builder` 可用（标注 deprecated 但不删除）
- ✅ 通过 `qb-migration-tool` feature gate 隔离迁移工具

### 2.2 v3.6.x（x ≥ 1）

- 收集用户迁移反馈
- 优化迁移工具转换规则覆盖率
- 修复差分测试发现的不等价场景
- 发布迁移指南 `docs/query-builder-guide.md`

### 2.3 v3.7.0

- 正式移除 `sz-orm-query-builder` 包
- 从 workspace `members` 移除
- crates.io yank 或保留标注 EOL
- `qb-migration-tool` feature 保留（支持尚未迁移的项目）

## 3. 用户通知计划

| 时间节点 | 通知渠道 | 内容 |
|----------|----------|------|
| v3.6.0 发布 | CHANGELOG.md | 迁移工具可用 + deprecated 告警 |
| v3.6.0 发布 | README.md | 迁移指南链接 |
| v3.6.0 发布 | docs/query-builder-guide.md | 详细迁移步骤 |
| v3.6.x 发布 | CHANGELOG.md | 迁移工具优化 + 已知问题修复 |
| v3.7.0 发布 | CHANGELOG.md | 正式移除 sz-orm-query-builder |

## 4. 迁移步骤

1. 启用 `qb-migration-tool` feature
2. 运行 `qb_migration_lint` 检测旧版 API 使用
3. 运行 `qb_migration_fix --dry-run` 预览转换
4. 确认无误后运行 `qb_migration_fix --fix` 执行转换
5. 运行测试验证功能不变
6. 复杂场景（UNION/CTE/窗口函数）人工审查

## 5. 已知不等价场景

详见 `docs/qb-migration-known-issues.md`（如有）。