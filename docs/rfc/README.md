# RFC (Request for Comments)

> SZ-ORM 的重大变更提案流程

## 目录说明

| 路径 | 用途 |
|------|------|
| `docs/rfc/` | RFC 草案（draft 状态） |
| `docs/rfc/accepted/` | 已接受的 RFC |
| `docs/rfc/rejected/` | 已拒绝的 RFC |
| `docs/rfc/template.md` | RFC 模板 |

## 编号规则

- 格式：`RFC-XXXX`（XXXX 为零填充序号，从 0001 开始）
- 序号不回收，拒绝的 RFC 占用序号

## 生命周期

```
draft → discussion (PR open) → accepted (PR merged) → implemented
                                ↘ rejected (PR closed)
```

## 何时需要 RFC

| 变更类型 | 需要 RFC？ |
|----------|-----------|
| 新增工作空间包 | ✅ |
| Breaking API 变更 | ✅ |
| 新增 feature gate | ✅ |
| 架构决策（ADR 级别） | ✅ |
| Bug 修复 | ❌（PR + 回归测试） |
| 文档改进 | ❌（直接 PR） |
| 性能优化 | ❌（PR + benchmark） |

## 现有 RFC

（暂无）

## 现有 ADR

参见 `docs/adr/` 目录，已有 ADR-0001 至 ADR-0011。