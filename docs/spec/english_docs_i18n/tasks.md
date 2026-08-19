# sz-orm 英文文档翻译编码任务分解

> 任务编号：TASK-005
> 对应需求规格：`docs/spec/english_docs_i18n/spec.md`（REQ-I18N-001 ~ REQ-I18N-012）
> 对应技术设计：`docs/spec/english_docs_i18n/design.md`
> 版本基线：v4.9.0
> 日期：2026-08-19
> 目标：将 sz-orm 关键文档从中文翻译为英文，消除"文档语言 ⚠️ 中文"竞品劣势，中文文档保留（双语并存）

---

## 1. 术语对照表建立

### 1.1 扫描提取技术术语
- [ ] 扫描全部中文文档（README.md + docs/*.md + packages/*/src/lib.rs 的 /// 注释）提取技术术语
- [ ] 统计术语词频，高频术语优先纳入对照表
- **依赖**：无
- **验证方法**：术语列表生成，含词频统计
- **预估工作量**：1h

### 1.2 建立中英术语对照表
- [ ] 生成 `docs/glossary-zh-en.md`，含 ≥ 50 个术语映射（REQ-I18N-001）
- [ ] 每条映射含：中文术语 / 英文译法（唯一）/ 备注（可选，如"保留中文"/"品牌名"）
- [ ] 覆盖核心术语：连接池 → connection pool / 方言 → dialect / 查询构造器 → query builder / 派生宏 → derive macro / 连接池耗尽 → pool exhaustion / 异常检测 → anomaly detection / 事务 → transaction / 滑动窗口 → sliding window / 基线 → baseline / 突增 → spike 等
- [ ] 品牌名"鲜视达"保留原文 + 英文注释（REQ-I18N-004）
- **依赖**：1.1
- **验证方法**：`grep -c "" docs/glossary-zh-en.md` ≥ 50 行术语映射；含核心术语
- **预估工作量**：1.5h

### 1.3 术语一致性校验机制
- [ ] 建立术语一致性校验脚本：grep 英文文档，校验同一术语统一译法（REQ-I18N-002/003）
- [ ] 发现混用则标记并统一为对照表标准译法
- **依赖**：1.2
- **验证方法**：校验脚本可执行；对测试文档混用术语能检出
- **预估工作量**：0.5h

---

## 2. README 翻译（P0 最高优先级）

### 2.1 备份中文 README
- [ ] 将现有 `README.md`（中文）复制为 `README.zh.md`（保留中文版，REQ-I18N-006）
- **依赖**：无
- **验证方法**：`README.zh.md` 存在，内容与原中文 README 一致
- **预估工作量**：0.2h

### 2.2 翻译 README 为英文
- [ ] 将 `README.md` 翻译为英文（查阅术语对照表，REQ-I18N-005）
- [ ] 保留代码示例不变（仅注释翻译，REQ-I18N-007）
- [ ] 保留链接路径不变（REQ-I18N-008）
- [ ] 保留 Markdown 结构（章节对应，便于对照维护）
- [ ] crates.io 主页渲染 README.md 显示英文（DFX 4.5）
- **依赖**：1.2, 2.1
- **验证方法**：`README.md` 全英文；`grep "[一-龥]" README.md` 无中文字符（除品牌名/代码示例注释）
- **预估工作量**：2h

### 2.3 添加双语互链
- [ ] 在 `README.md`（英文）顶部添加链接 `[中文版](README.zh.md)`（REQ-I18N-006）
- [ ] 在 `README.zh.md`（中文）顶部添加链接 `[English](README.md)`（REQ-I18N-006）
- **依赖**：2.2
- **验证方法**：`grep "README.zh.md" README.md` 命中；`grep "README.md" README.zh.md` 命中
- **预估工作量**：0.2h

### 2.4 代码示例可运行性验证
- [ ] 提取英文 README 中所有代码示例，执行 `cargo run` 验证可运行（REQ-I18N-007）
- [ ] 与中文 README 示例行为一致
- **依赖**：2.2
- **验证方法**：英文示例 cargo run 成功；与中文示例行为一致
- **预估工作量**：0.5h

### 2.5 链接有效性验证
- [ ] 提取英文 README 中所有链接，校验目标存在（REQ-I18N-008）
- [ ] 修正死链
- **依赖**：2.2
- **验证方法**：链接检查脚本全部通过；无死链
- **预估工作量**：0.3h

---

## 3. API 文档翻译（rustdoc 注释，P0）

### 3.1 扫描 rustdoc 注释
- [ ] 扫描 `packages/*/src/lib.rs` 及各模块的 `///` 文档注释，统计中文注释数量
- [ ] 按包分组，生成翻译清单（包名/文件路径/中文注释行数）
- **依赖**：无
- **验证方法**：翻译清单生成，含各包注释统计
- **预估工作量**：0.5h

### 3.2 翻译 sz-orm-core rustdoc 注释
- [ ] 翻译 `packages/sz-orm-core/src/lib.rs` 及各模块（query.rs/model.rs/pool.rs/transaction.rs/dialect.rs/db_type.rs 等）的 `///` 注释为英文（REQ-I18N-009）
- [ ] 查阅术语对照表确保术语一致
- [ ] 保留 ```rust 代码块不变（仅注释翻译）
- [ ] API 签名不变（仅 /// 注释翻译，REQ-I18N-010）
- **依赖**：1.2, 3.1
- **验证方法**：`git diff` 仅注释变更，无 pub fn/struct/enum 签名变更；`cargo doc -p sz-orm-core --no-deps` 成功
- **预估工作量**：4h（sz-orm-core 注释量大）

### 3.3 翻译其他包 rustdoc 注释
- [ ] 翻译 `packages/sz-orm-sqlx/` / `sz-orm-cabi/` / `sz-orm-macros/` / `sz-orm-diagnosis/` / `sz-orm-observability/` / `sz-orm-health/` 等包的 `///` 注释为英文（REQ-I18N-009）
- [ ] 优先翻译 pub API 注释，内部辅助函数注释次之
- [ ] API 签名不变（REQ-I18N-010）
- **依赖**：1.2, 3.1
- **验证方法**：`git diff` 仅注释变更；`cargo doc --workspace --no-deps` 成功
- **预估工作量**：6h（多包注释翻译）

### 3.4 rustdoc 编译验证
- [ ] 执行 `cargo check --workspace --all-targets` 确认翻译未破坏编译（REQ-I18N-011）
- [ ] 执行 `cargo doc --workspace --no-deps` 确认英文 rustdoc 生成成功（REQ-I18N-009）
- [ ] 执行 `cargo test --doc` 确认文档测试全通过（代码示例未被误改）
- **依赖**：3.2, 3.3
- **验证方法**：cargo check 退出码 0；cargo doc 退出码 0；cargo test --doc 退出码 0
- **预估工作量**：1h

---

## 4. 对比分析文档翻译（P1）

### 4.1 翻译对比分析文档
- [ ] 将 `docs/sz-orm与同类产品对比分析.md` 翻译为 `docs/sz-orm-comparison-analysis.en.md`（REQ-I18N-005 对比文档）
- [ ] 查阅术语对照表翻译描述文字
- [ ] 保留 file:line 代码证据路径不变（如 `packages/sz-orm-core/src/query.rs:36`，REQ-I18N-005）
- [ ] 保留 Markdown 表格结构（行列数一致，仅单元格内容翻译）
- [ ] 保留中文原文档（双语并存）
- **依赖**：1.2
- **验证方法**：`docs/sz-orm-comparison-analysis.en.md` 存在；file:line 路径与中文版一致；表格行列数一致
- **预估工作量**：4h（对比文档量大）

### 4.2 file:line 证据保留验证
- [ ] 对比中英文档所有 file:line 引用，确认路径一致（REQ-I18N-005）
- [ ] 若有丢失则与中文版比对补齐
- **依赖**：4.1
- **验证方法**：脚本对比中英文档 file:line，一致率 100%
- **预估工作量**：0.5h

### 4.3 表格结构保留验证
- [ ] 对比中英文档所有 Markdown 表格，确认行列数一致（REQ-I18N-005）
- [ ] 若有错乱则与中文版比对修正
- **依赖**：4.1
- **验证方法**：脚本对比中英文档表格行列数，一致率 100%
- **预估工作量**：0.5h

---

## 5. 路线图翻译（P1）

### 5.1 翻译成熟化路线图
- [ ] 将 `docs/sz-orm-maturity-roadmap.md` 翻译为 `docs/sz-orm-maturity-roadmap.en.md`（REQ-I18N-005 路线图）
- [ ] 查阅术语对照表翻译
- [ ] 保留 Markdown 结构（章节对应）
- [ ] 保留中文原文档（双语并存）
- **依赖**：1.2
- **验证方法**：`docs/sz-orm-maturity-roadmap.en.md` 存在；全英文；结构与中文版一致
- **预估工作量**：2h

---

## 6. 一致性校验与编译验证

### 6.1 术语一致性校验
- [ ] 执行术语一致性校验脚本，grep 全部英文文档，校验同一术语统一译法（REQ-I18N-002/003）
- [ ] 发现混用则修正为对照表标准译法
- **依赖**：1.3, 2.2, 3.3, 4.1, 5.1
- **验证方法**：一致性校验脚本输出"无混用"；`grep` 各术语译法统一
- **预估工作量**：1h

### 6.2 编译与文档测试验证
- [ ] 执行 `cargo check --workspace --all-targets` 确认翻译未破坏编译（REQ-I18N-011）
- [ ] 执行 `cargo doc --workspace --no-deps` 确认英文 rustdoc 生成成功（REQ-I18N-009）
- [ ] 执行 `cargo test --doc` 确认文档测试全通过（REQ-I18N-007）
- **依赖**：3.4, 6.1
- **验证方法**：cargo check 退出码 0；cargo doc 退出码 0；cargo test --doc 退出码 0
- **预估工作量**：1h

### 6.3 API 签名不变验证
- [ ] `git diff` 确认仅 /// 注释变更，无 pub fn/struct/enum 签名变更（REQ-I18N-010）
- [ ] 确认未修改任何代码功能逻辑
- **依赖**：6.2
- **验证方法**：`git diff` 仅含注释变更行，无签名变更
- **预估工作量**：0.5h

---

## 7. 交付记录与文档

### 7.1 生成交付记录
- [ ] 生成 `docs/spec/english_docs_i18n/delivery-record.md`，含：翻译文件清单（英文路径/中文路径/状态/行数）、术语表（glossary-zh-en.md）、验证结果（cargo doc / cargo test --doc / cargo check 通过证据，file:line）（REQ-I18N-012）
- **依赖**：6.2, 6.3
- **验证方法**：delivery-record.md 存在且内容完整；含验证证据
- **预估工作量**：0.5h

### 7.2 更新对比分析文档
- [ ] 更新 `docs/sz-orm与同类产品对比分析.md` 综合对比矩阵"文档语言"项：从"⚠️ 中文"改为"✅ 中英双语"
- **依赖**：6.2
- **验证方法**：grep 对比文档含"✅ 中英双语"
- **预估工作量**：0.3h

---

## 8. 审查与确认

### 8.1 五维审查
- [ ] 正确性：英文与中文技术含义一致（语义等价，无歧义）；代码示例可运行；链接有效
- [ ] 可读性：术语一致；英文表达自然；结构清晰
- [ ] 架构：双语文档并存；章节对应便于维护
- [ ] 安全性：翻译未引入敏感信息；未移除安全警告
- [ ] 性能：翻译不增加编译时间；cargo doc 耗时不变
- **依赖**：7.1, 7.2
- **验证方法**：审查清单逐项确认，附 file:line 证据
- **预估工作量**：0.5h

### 8.2 变更范围确认
- [ ] 确认仅修改 README.md（改英文）+ 新增 README.zh.md + packages/*/src/lib.rs 的 /// 注释 + 新增英文对比/路线图文档 + glossary-zh-en.md + delivery-record.md
- [ ] 确认未修改任何代码功能逻辑（仅注释翻译）
- [ ] 确认未翻译测试代码注释 / git commit / AGENTS.md / spec 文档（边界声明）
- [ ] 确认中文文档 100% 保留（双语并存）
- **依赖**：8.1
- **验证方法**：`git diff --name-only` 仅含上述文件；`git diff` 无代码逻辑变更
- **预估工作量**：0.3h

---

## 任务依赖关系

```
1.1 → 1.2 → 1.3 → 2.2 → 2.3 → 2.4 → 2.5 → 6.1 → 6.2 → 6.3 → 7.1 → 8.1 → 8.2
1.2 → 2.1 → 2.2
1.2 → 3.1 → 3.2 → 3.4 → 6.2
1.2 → 3.3 → 3.4
1.2 → 4.1 → 4.2 → 4.3 → 6.1
1.2 → 5.1 → 6.1
6.2 → 7.2
```

## 任务统计

- 主任务：8 组
- 子任务：22 个
- 需求覆盖：REQ-I18N-001 ~ REQ-I18N-012 全部 12 项
- 预估总工作量：约 26h（含大量文档翻译）