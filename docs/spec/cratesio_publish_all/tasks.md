# sz-orm crates.io 全量发布编码任务分解

> 任务编号：TASK-001
> 对应需求规格：`docs/spec/cratesio_publish_all/spec.md`（REQ-PUB-001 ~ REQ-PUB-012）
> 对应技术设计：`docs/spec/cratesio_publish_all/design.md`
> 版本基线：v4.9.0
> 日期：2026-08-19
> 目标：将 workspace 全部 58 个 lib 包发布到 crates.io，使外部项目可通过 `cargo add sz-orm-*` 拉取

---

## 1. 发布前置检查

### 1.1 crates.io 登录态验证
- [ ] 读取 `E:\vue\test\鲜视达\服务器信息.md` 获取 crates.io API token，执行 `cargo login <token>` 写入 `~/.cargo/credentials.toml`
- [ ] 验证登录态：执行 `cargo publish --dry-run -p sz-orm-core`，成功则 token 有效（REQ-PUB-001）
- [ ] 确认 `~/.cargo/credentials.toml` 不在 git 仓库内（.gitignore 含该路径或位于 home 目录）
- **依赖**：无
- **验证方法**：`cargo publish --dry-run -p sz-orm-core` 返回成功；`git status` 不显示 credentials.toml
- **预估工作量**：0.5h

### 1.2 生成可发布包清单
- [ ] 执行 `cargo metadata --format-version 1` 解析 workspace，输出 60 个成员
- [ ] 过滤出 58 个 lib 包（排除 cli/examples 二进制包），过滤 `publish != false` 的包
- [ ] 生成清单文件 `docs/spec/cratesio_publish_all/publish-list.txt`，每行一个包名（REQ-PUB-002）
- **依赖**：无
- **验证方法**：清单含 58 行；`grep -c "" publish-list.txt` = 58；不含 sz-orm-cli/sz-orm-examples
- **预估工作量**：0.5h

### 1.3 版本号一致性校验
- [ ] 检查 55 个继承版本包：`grep "version.workspace = true" packages/*/Cargo.toml` 确认继承 workspace.package.version = "4.9.0"（REQ-PUB-003）
- [ ] 检查 3 个独立版本包：sz-orm-graph/sz-orm-js/sz-orm-python 的 Cargo.toml version = "0.1.0"（REQ-PUB-004）
- [ ] 检查 sz-orm-core 当前版本：若为 1.0.0 则升级至 workspace 继承（version.workspace = true），保留 1.0.0 在 crates.io 不删
- **依赖**：1.2
- **验证方法**：`grep -r "version.workspace = true" packages/` 计数 = 55；`grep 'version = "0.1.0"' packages/sz-orm-graph/Cargo.toml packages/sz-orm-js/Cargo.toml packages/sz-orm-python/Cargo.toml` 命中 3 处
- **预估工作量**：1h

### 1.4 包元数据完整性校验
- [ ] 对 58 个包逐一检查 Cargo.toml [package] 段含 name/description/license/repository 字段且非空（REQ-PUB-005）
- [ ] 校验 license = "MIT"，repository 指向 github.com（REQ-PUB-005）
- [ ] 若某包缺字段，中止并报告缺失包名与字段（REQ-PUB-006）
- **依赖**：1.2
- **验证方法**：脚本扫描 58 个 Cargo.toml，输出缺失字段报告；缺失数为 0 则通过
- **预估工作量**：1h

### 1.5 安全审计门禁
- [ ] 执行 `cargo audit` 检查 RUSTSEC 漏洞（AGENTS.md 门禁 6）
- [ ] 执行 `cargo deny check` 检查许可证违规（需 deny.toml 配置）
- [ ] 发现漏洞或违规则中止发布，报告详情
- **依赖**：无
- **验证方法**：`cargo audit` 退出码 0；`cargo deny check` 退出码 0
- **预估工作量**：0.5h

---

## 2. 依赖拓扑排序

### 2.1 构建依赖图
- [ ] 解析 `cargo metadata --format-version 1` 获取 58 个包的 workspace 内依赖关系（REQ-PUB-007）
- [ ] 构建有向图：节点=包，边=被依赖包 → 依赖方（如 sz-orm-core → sz-orm-sqlx 表示 sqlx 依赖 core）
- **依赖**：1.2
- **验证方法**：输出依赖图 JSON，含 58 节点 + 边列表
- **预估工作量**：1h

### 2.2 Kahn 算法拓扑排序
- [ ] 实现 Kahn 算法（BFS）：计算每包入度，入度 0 入队，出队减依赖方入度，归 0 入队（REQ-PUB-007）
- [ ] 同层级按包名字典序排序，保证可重现
- [ ] 检测循环依赖：若出队包数 < 58 则存在循环，中止并报告循环链
- [ ] 输出拓扑序列到 `docs/spec/cratesio_publish_all/topo-order.txt`
- **依赖**：2.1
- **验证方法**：拓扑序列含 58 行；对每条依赖边，被依赖包在依赖方之前；`python -c "校验脚本"` 通过
- **预估工作量**：1.5h

---

## 3. dry-run 全量预演

### 3.1 逐包 dry-run
- [ ] 按拓扑序逐包执行 `cargo publish --dry-run -p <pkg>`，超时 5 分钟/包（REQ-PUB-007 预演）
- [ ] 记录每包 dry-run 结果（成功/失败 + 错误输出）到 `docs/spec/cratesio_publish_all/dry-run-report.md`
- [ ] 任一包 dry-run 失败则中止，报告失败包 + 错误输出
- **依赖**：2.2, 1.5
- **验证方法**：dry-run-report.md 含 58 行 SUCCESS；无 FAILED 行
- **预估工作量**：2h（含编译耗时）

---

## 4. 逐包发布执行

### 4.1 查询 crates.io 已存在版本
- [ ] 对每个包调用 crates.io API `GET https://crates.io/api/v1/crates/<pkg>` 查询已发布版本（REQ-PUB-009）
- [ ] 若目标版本已存在则标记 SKIPPED，记录"已存在"
- **依赖**：2.2
- **验证方法**：API 返回 JSON 含 versions 数组；sz-orm-core 1.0.0 标记 SKIPPED
- **预估工作量**：1h

### 4.2 逐包发布
- [ ] 按拓扑序逐包执行：已存在版本跳过（SKIPPED），否则执行 `cargo publish -p <pkg>`，超时 5 分钟（REQ-PUB-008）
- [ ] 成功记录 SUCCESS + crates.io URL + 时间戳（ISO 8601）
- [ ] 失败记录 FAILED + 错误输出 + 失败原因分类（编译错误/依赖缺失/token 失效/网络错误），询问是否继续后续包（REQ-PUB-010）
- [ ] 网络错误重试最多 3 次
- **依赖**：3.1, 4.1
- **验证方法**：publish-manifest.md 含 58 行记录；SUCCESS + SKIPPED + FAILED = 58
- **预估工作量**：3h（含编译 + 上传耗时）

### 4.3 生成发布清单
- [ ] 生成 `docs/spec/cratesio_publish_all/publish-manifest.md`，含表格：包名/版本号/发布时间/crates.io URL/发布结果/失败原因
- [ ] 统计 total/success_count/skipped_count/failed_count
- **依赖**：4.2
- **验证方法**：publish-manifest.md 存在；表格 58 行；统计字段完整
- **预估工作量**：0.5h

---

## 5. 发布后验证

### 5.1 cargo add 可用性验证
- [ ] 对每个 SUCCESS 包创建临时目录 `tmp/verify-<pkg>/`，执行 `cargo init` + `cargo add <pkg>@<version>` + `cargo check`（REQ-PUB-011）
- [ ] 成功记录验证通过，失败记录 FAIL + 错误输出
- [ ] 验证完成后删除临时目录（session rules：测试文件及时清理）
- **依赖**：4.2
- **验证方法**：verify-report.md 含 58 行验证记录；无 FAIL 行；`ls tmp/verify-*` 无残留
- **预估工作量**：2h

### 5.2 sz-pay 兼容性验证
- [ ] 创建临时项目，执行 `cargo add sz-orm-core@4.7.0` + `cargo add sz-orm-sqlx@4.7.0` + ...（sz-pay 依赖的 6 个包 @ 4.7.0）
- [ ] 验证 4.7.0 版本仍可从 crates.io 拉取（DFX 4.5.2 兼容性）
- [ ] 若 4.7.0 不可拉取则警告"sz-pay 依赖可能受影响"
- [ ] 删除临时目录
- **依赖**：4.2
- **验证方法**：`cargo add sz-orm-core@4.7.0` 成功；临时目录已删除
- **预估工作量**：0.5h

### 5.3 生成验证报告
- [ ] 生成 `docs/spec/cratesio_publish_all/verify-report.md`，含 58 行验证记录 + sz-pay 兼容性结果
- **依赖**：5.1, 5.2
- **验证方法**：verify-report.md 存在；含 58 行验证记录 + sz-pay 兼容性段落
- **预估工作量**：0.5h

---

## 6. 交付记录与文档

### 6.1 生成交付记录
- [ ] 生成 `docs/spec/cratesio_publish_all/delivery-record.md`，含：发布时间、包数量（58）、成功/跳过/失败统计、crates.io URL 列表、sz-pay 兼容性结论（REQ-PUB-012）
- [ ] 确认 token 未出现在任何文档/日志中（脱敏检查）
- **依赖**：4.3, 5.3
- **验证方法**：delivery-record.md 存在且内容完整；`grep -r "token" docs/spec/cratesio_publish_all/` 无明文 token
- **预估工作量**：0.5h

### 6.2 更新对比分析文档
- [ ] 更新 `docs/sz-orm与同类产品对比分析.md` 中 crates.io 发布项：从"⚠️ 仅 sz-orm-core"改为"✅ 全量 58 包"
- **依赖**：4.2
- **验证方法**：grep 对比文档含"✅ 全量 58 包"
- **预估工作量**：0.5h

---

## 7. 集成验证

### 7.1 全量发布回归验证
- [ ] 执行 `cargo test --workspace -j 2 --no-fail-fast` 确认发布过程未破坏既有测试
- [ ] 执行 `cargo check --workspace --all-targets` 确认编译通过
- [ ] 验证 sz-orm-core 1.0.0 仍可在 crates.io 拉取（不回退）
- **依赖**：6.1
- **验证方法**：cargo test 全通过；cargo check 退出码 0；`cargo add sz-orm-core@1.0.0` 成功
- **预估工作量**：1h

### 7.2 临时文件清理验证
- [ ] 扫描 `tmp/verify-*` 目录，确认全部删除
- [ ] 扫描发布过程产生的临时文件，确认无残留
- **依赖**：5.1, 5.2
- **验证方法**：`ls tmp/` 无 verify-* 目录；无残留临时文件
- **预估工作量**：0.2h

---

## 8. 审查与确认

### 8.1 五维审查
- [ ] 正确性：58 包全部 SUCCESS/SKIPPED，无 FAILED
- [ ] 可读性：publish-manifest.md / verify-report.md / delivery-record.md 结构清晰
- [ ] 架构：拓扑排序正确，被依赖包先发布
- [ ] 安全性：token 未泄露；安全审计通过
- [ ] 性能：总耗时 < 2 小时；单包 < 5 分钟
- **依赖**：7.1, 7.2
- **验证方法**：审查清单逐项确认，附 file:line 证据
- **预估工作量**：0.5h

### 8.2 变更范围确认
- [ ] 确认仅修改 sz-orm-core Cargo.toml version（1.0.0 → workspace 继承）+ publish-manifest/verify-report/delivery-record 文档
- [ ] 确认未修改任何包源码功能逻辑
- [ ] 确认未新增 workspace 成员
- **依赖**：8.1
- **验证方法**：`git diff --name-only` 仅含 sz-orm-core/Cargo.toml + docs/spec/cratesio_publish_all/*.md
- **预估工作量**：0.2h

---

## 任务依赖关系

```
1.1 → 1.3 → 2.1 → 2.2 → 3.1 → 4.1 → 4.2 → 4.3 → 5.1 → 5.3 → 6.1 → 7.1 → 8.1 → 8.2
1.2 → 1.3
1.2 → 1.4
1.5 → 3.1
4.2 → 5.2 → 5.3
4.2 → 6.2
5.1 → 7.2
5.2 → 7.2
```

## 任务统计

- 主任务：8 组
- 子任务：22 个
- 需求覆盖：REQ-PUB-001 ~ REQ-PUB-012 全部 12 项
- 预估总工作量：约 18h（含编译 + 上传耗时）