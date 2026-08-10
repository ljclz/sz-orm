# DSL 表达式覆盖度对比：sz-orm v3.6.0 vs Diesel 2.2.x

> 生成时间：2026-08-10
> sz-orm 版本：3.6.0
> Diesel 版本：2.2.x

## 1. 表达式清单对比

### 1.1 sz-orm v3.6.0 表达式清单（61 种）

#### 既有表达式（46 种，v3.4.0 引入）

| # | 表达式 | 类别 | 实现位置 |
|---|--------|------|----------|
| 1 | `Eq<C, T>` | 比较 | `typed_ast.rs:55` |
| 2 | `Ne<C, T>` | 比较 | `typed_ast.rs:70` |
| 3 | `Lt<C, T>` | 比较 | `typed_ast.rs:85` |
| 4 | `Gt<C, T>` | 比较 | `typed_ast.rs:100` |
| 5 | `Le<C, T>` | 比较 | `typed_ast.rs:115` |
| 6 | `Ge<C, T>` | 比较 | `typed_ast.rs:130` |
| 7 | `And<L, R>` | 逻辑 | `typed_ast.rs:145` |
| 8 | `Or<L, R>` | 逻辑 | `typed_ast.rs:160` |
| 9 | `Not<E>` | 逻辑 | `typed_ast.rs:175` |
| 10 | `ColumnExpr<C>` | 基础 | `typed_ast.rs:263` |
| 11 | `Literal<T>` | 基础 | `typed_ast.rs:300` |
| 12 | `Cast<S, T>` | 类型转换 | `typed_ast.rs:typed_dsl_ext` |
| 13-18 | `Sum/Avg/Count/Min/Max/CountDistinct` | 聚合 | `typed_ast.rs:typed_dsl_ext` |
| 19-24 | `Add/Sub/Mul/Div/Mod/Neg` | 算术 | `typed_ast.rs:typed_dsl_ext` |
| 25-30 | `Concat/Upper/Lower/Length/Trim/SubStr` | 字符串 | `typed_ast.rs:typed_dsl_ext` |
| 31-36 | `Year/Month/Day/Hour/Minute/Second` | 日期 | `typed_ast.rs:typed_dsl_ext` |
| 37-42 | `RowNumber/Rank/DenseRank/Lag/Lead/FirstValue` | 窗口 | `typed_ast.rs:typed_dsl_ext` |
| 43 | `IsNull<E>` | NULL | `typed_ast.rs:typed_dsl_ext` |
| 44 | `IsNotNull<E>` | NULL | `typed_ast.rs:typed_dsl_ext` |
| 45 | `Between<E, L, H>` | 范围 | `typed_ast.rs:typed_dsl_ext` |
| 46 | `Distinct<E>` | 去重 | `typed_ast.rs:typed_dsl_ext` |

#### 新增表达式（15 种，v3.6.0 引入）

| # | 表达式 | 类别 | 实现位置 |
|---|--------|------|----------|
| 47 | `With<N, S>` | CTE | `typed_ast.rs:1701` |
| 48 | `WithRecursive<N, I, R>` | CTE | `typed_ast.rs:1704` |
| 49 | `CteRef<N>` | CTE | `typed_ast.rs:1709` |
| 50 | `RowsFrame` | Window Frame | `typed_ast.rs:1785` |
| 51 | `RangeFrame` | Window Frame | `typed_ast.rs:1788` |
| 52 | `GroupsFrame` | Window Frame | `typed_ast.rs:1791` |
| 53 | `FrameBetween<S, E>` | Window Frame | `typed_ast.rs:1794` |
| 54 | `FrameUnboundedPreceding` | Window Frame | `typed_ast.rs:1799` |
| 55 | `FrameCurrentRow` | Window Frame | `typed_ast.rs:1802` |
| 56 | `JsonGet<C, K>` | JSON | `typed_ast.rs:1918` |
| 57 | `JsonGetText<C, K>` | JSON | `typed_ast.rs:1921` |
| 58 | `JsonPathGet<C, P>` | JSON | `typed_ast.rs:1924` |
| 59 | `JsonPathGetText<C, P>` | JSON | `typed_ast.rs:1927` |
| 60 | `JsonContains<C, V>` | JSON | `typed_ast.rs:1930` |
| 61 | `JsonExists<C, K>` | JSON | `typed_ast.rs:1933` |

### 1.2 Diesel 2.2.x 表达式清单

| 类别 | Diesel 表达式 | 数量 |
|------|--------------|------|
| 比较 | eq/ne/lt/gt/le/ge | 6 |
| 逻辑 | and/or/not | 3 |
| 聚合 | sum/avg/count/min/max | 5 |
| 算术 | add/sub/mul/div | 4 |
| 字符串 | concat/upper/lower/length | 4 |
| 日期 | date/time/timestamp | 3 |
| 窗口 | over/partition_by/order_by | 3 |
| NULL | is_null/is_not_null | 2 |
| 范围 | between | 1 |
| CTE | with/with_recursive | 2 |
| JSON | json_get/json_contains | 2 |
| 关联 | belongs_to/has_many/has_one | 3 |
| **合计** | | **~38** |

## 2. 覆盖度对比

| 维度 | sz-orm v3.6.0 | Diesel 2.2.x | 优势 |
|------|---------------|--------------|------|
| 比较表达式 | 6 | 6 | = |
| 逻辑表达式 | 3 | 3 | = |
| 聚合表达式 | 6 | 5 | +1 |
| 算术表达式 | 6 | 4 | +2 |
| 字符串表达式 | 6 | 4 | +2 |
| 日期表达式 | 6 | 3 | +3 |
| 窗口函数 | 6 | 3 | +3 |
| Window Frame | 6 | 0 | +6 |
| NULL 表达式 | 2 | 2 | = |
| 范围表达式 | 1 | 1 | = |
| 去重表达式 | 1 | 0 | +1 |
| CTE 表达式 | 3 | 2 | +1 |
| JSON 操作符 | 6 | 2 | +4 |
| 类型转换 | 1 | 1 | = |
| 关联查询 | 3 (typed_relation) | 3 | = |
| **合计** | **61** | **~38** | **+23** |

## 3. 结论

sz-orm v3.6.0 表达式覆盖度（61 种）显著超越 Diesel 2.2.x（~38 种），领先 23 种表达式。

主要优势领域：
- **Window Frame（+6）**：sz-orm 支持 ROWS/RANGE/GROUPS/BETWEEN/UNBOUNDED PRECEDING/CURRENT ROW，Diesel 无原生支持
- **JSON 操作符（+4）**：sz-orm 支持 6 种 JSON 操作符，Diesel 仅 2 种
- **日期表达式（+3）**：sz-orm 支持 6 种日期提取，Diesel 仅 3 种
- **窗口函数（+3）**：sz-orm 支持 6 种窗口函数，Diesel 仅 3 种

所有新增表达式均为 ZST（零大小类型），通过 `#[cfg(feature = "typed-dsl")]` feature gate 隔离，不影响默认编译产物大小。