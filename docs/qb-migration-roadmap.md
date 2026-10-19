# QueryBuilder 迁移路线图

> 版本：v10.9.0
> 日期：2026-10-17

## 1. 背景

sz-orm 存在两个 QueryBuilder 实现：
- **`sz-orm-query-builder`**（旧版）：独立包，已标注 `#[deprecated]`（since 3.5.0）
- **`sz_orm_core::QueryBuilder`**（新版）：核心包内建，推荐使用

**v10.9.0 决策**：5 处 deprecated API（`Query` struct + `select`/`insert`/`update`/`delete`）全部保留，不移除。理由：
1. 向后兼容：下游项目（如 sz-pay）声明依赖，移除将导致编译失败
2. 复杂 SQL 场景：UNION/CTE/窗口函数等 `QueryBuilder<M>` 不支持，`Query` 仍有存在价值
3. 迁移指引已深化：`docs/query_builder_selection_guide.md` 第 6 节提供完整迁移步骤和示例

## 2. 当前状态（v10.9.0）

- ✅ `sz-orm-query-builder` 包保留，5 处 `#[deprecated]` 标记保留
- ✅ 迁移指引文档深化：`docs/query_builder_selection_guide.md` 第 6 节（v10.9.0 新增）
- ✅ `qb_migration_lint` 模块：检测 `sz_orm_query_builder::Query` 使用并输出告警
- ✅ `qb_migration_fix` 模块：自动将旧版 API 转换为新版等价 API
- ✅ `qb-migration-tool` feature gate 隔离迁移工具
- ⏸️ v3.7.0 计划的"正式移除"已撤销，改为长期保留（向后兼容）

## 3. 迁移步骤

1. 启用 `qb-migration-tool` feature
2. 运行 `qb_migration_lint` 检测旧版 API 使用
3. 运行 `qb_migration_fix --dry-run` 预览转换
4. 确认无误后运行 `qb_migration_fix --fix` 执行转换
5. 运行测试验证功能不变
6. 复杂场景（UNION/CTE/窗口函数）人工审查，保留 `Query` 使用

## 4. 已知不等价场景

详见 `docs/qb-migration-known-issues.md`（如有）。

## 5. deprecated API 清单

| API | file:line | 迁移目标 | 处置 |
|-----|-----------|---------|------|
| `Query` struct | `packages/sz-orm-query-builder/src/lib.rs:221` | `sz_orm_core::QueryBuilder<M>` | 保留+迁移指引 |
| `Query::select()` | `packages/sz-orm-query-builder/src/lib.rs:230` | `QueryBuilder<M>::select()` | 保留+迁移指引 |
| `Query::insert()` | `packages/sz-orm-query-builder/src/lib.rs:239` | `QueryBuilder<M>::insert()` | 保留+迁移指引 |
| `Query::update()` | `packages/sz-orm-query-builder/src/lib.rs:248` | `QueryBuilder<M>::update()` | 保留+迁移指引 |
| `Query::delete()` | `packages/sz-orm-query-builder/src/lib.rs:257` | `QueryBuilder<M>::delete()` | 保留+迁移指引 |