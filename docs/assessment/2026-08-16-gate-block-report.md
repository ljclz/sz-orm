# 门禁阻断报告（2026-08-16）

- 分支 / commit：`main` @ `a9db540`
- 模式：`/sz-orm-review fast`（G1~G3）
- 失败门禁：**G1 格式检查（cargo fmt --all -- --check）**
- 状态机：`G1 → FAILED`（红牌即停，G2/G3 未执行）
- 失败命令：`cargo fmt --all -- --check`
- 退出码：1

## 失败输出摘要

12 处格式 diff，全部为 rustfmt 换行展开/折叠（assert_eq! / matches! 长行展开、链式调用折叠、`.schedule(...)` 调用链），无逻辑差异：

| 文件 | 行号（Diff 起点） |
|------|-------------------|
| `packages/sz-orm-designer/src/design_ir.rs` | 464, 475, 482, 488 |
| `packages/sz-orm-designer/src/exporter.rs` | 129, 154 |
| `packages/sz-orm-designer/src/web_ui.rs` | 223 |
| `packages/sz-orm-scheduler/src/lib.rs` | 505, 694, 787, 1226, 1468, 1480 |

## 证据（file:line 均真实存在）

- `packages/sz-orm-designer/src/design_ir.rs:464` — `assert_eq!(ColumnType::DateTime.to_rust_type(), ...)` 需按 rustfmt 展开为多行
- `packages/sz-orm-designer/src/exporter.rs:129` — `assert!(matches!(result.unwrap_err(), DesignerError::DdlGenerationPartial { .. }))` 需展开
- `packages/sz-orm-scheduler/src/lib.rs:505` — `self.tasks.read().map(|tasks| tasks.len()).unwrap_or(0)` 需折叠为单行

## 建议修复

```bash
cargo fmt --all          # rustfmt 自动修复（gate.ps1 官方提示）
```

## 复跑要求

修复后从 **G1** 重跑（`cargo fmt --all -- --check` 通过后再进 G2/G3），禁止跳过失败关。

---
*生成：sz-orm-review skill 钩子 on_gate_failed（2026-08-16）*
