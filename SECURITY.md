# Security Policy

## Supported Versions

SZ-ORM 当前处于 v1.0.x 阶段，仅对最新主版本提供安全更新。

| Version | Supported          |
|---------|--------------------|
| 1.0.x   | ✅ 安全更新         |
| < 1.0   | ❌ 不支持           |

## Reporting a Vulnerability

### 私密披露流程

**请勿通过公开 GitHub Issue 报告安全漏洞。**

如发现安全漏洞，请通过以下方式私密披露：

1. **首选**：使用 GitHub Security Advisory（Private vulnerability reporting）
   - 访问 https://github.com/ljclz/sz-orm/security/advisories/new
   - 点击 "Report a vulnerability"
   - 填写漏洞详情、复现步骤、影响评估

2. **备选**：发送邮件至 `ljclz@users.noreply.github.com`
   - 主题：`[SECURITY] SZ-ORM <简短描述>`
   - 正文：漏洞详情、复现步骤、影响评估、建议修复方案

### 响应时间

| 阶段 | SLA |
|------|-----|
| 确认收到报告 | 48 小时内 |
| 初步评估 | 7 天内 |
| 修复方案沟通 | 30 天内 |
| 修复发布 | 90 天内（Critical/High） |
| 公开披露 | 修复发布后 14 天 |

### 披露原则

- **协调披露**：修复发布后再公开漏洞详情
- **致谢**：在修复公告中致谢报告者（除非报告者要求匿名）
- **CVE**：Critical/High 级别漏洞将申请 CVE 编号

## Security Measures

### 已实施的安全措施

| 措施 | 状态 | 说明 |
|------|------|------|
| 依赖漏洞扫描 | ✅ | `cargo audit` + `cargo deny check advisories` |
| 许可证合规 | ✅ | `cargo deny check licenses` |
| 来源校验 | ✅ | `cargo deny check sources` |
| 禁用 crate | ✅ | `cargo deny check bans` |
| SQL 参数化 | ✅ | 所有用户输入通过参数化查询 |
| 标识符校验 | ✅ | MorphTo 加载校验表名为合法标识符 |
| 凭证脱敏 | ✅ | 文档中真实凭证已替换为占位符 |
| `.gitignore` 加固 | ✅ | 排除 `.env`/`*.key`/`*.pem` 等敏感文件 |
| 0 unsafe（生产代码） | ✅ | 仅测试代码注释中出现 unsafe |
| 0 panic!/todo! | ✅ | 生产代码无 panic!/unimplemented!/todo! |

### 待实施的安全措施

| 措施 | 优先级 | 计划 |
|------|--------|------|
| Semgrep 静态分析规则 | P1 | 2026 Q4 |
| CodeQL 集成 | P1 | 2026 Q4 |
| cargo-fuzz 覆盖率扩展 | P2 | 2027 Q1 |
| 第三方渗透测试 | P2 | 2027 Q1 |
| SOC 2 Type II 审计 | P3 | 2027 Q2 |
| 发布签名（sigstore） | P2 | 2026 Q4 |
| 密钥轮换策略 | P2 | 2026 Q4 |

## Threat Model

### STRIDE 分析

| 威胁类型 | 风险点 | 缓解措施 | 状态 |
|----------|--------|----------|------|
| **S**poofing | 数据库凭证伪造 | 连接字符串验证 + TLS | ✅ |
| **T**ampering | SQL 注入 | 参数化查询 + 标识符校验 | ✅（C-2 已修复） |
| **R**epudiation | 审计日志缺失 | OTLP tracing | ✅ |
| **I**nfo Disclosure | 凭证泄漏到日志 | 脱敏处理 | ✅ |
| **D**oS | 连接池耗尽 | 超时 + 回收（待完善） | ⚠️ |
| **E**oP | 越权查询 | 应用层 RBAC（不在 ORM 范围） | N/A |

### 信任边界

```
┌─────────────────────────────────────────────────┐
│  应用层（用户代码）                                │
│  ┌───────────────────────────────────────────┐  │
│  │  SZ-ORM API（公开接口）                    │  │
│  │  ┌─────────────────────────────────────┐  │  │
│  │  │  SZ-ORM 内部（受信任）              │  │  │
│  │  │  ┌───────────────────────────────┐  │  │  │
│  │  │  │  数据库驱动（sqlx/redis 等）   │  │  │  │
│  │  │  └───────────────────────────────┘  │  │  │
│  │  └─────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
                       │
                       ▼
              ┌────────────────┐
              │  数据库（不可信）│
              └────────────────┘
```

**关键假设**：
- 应用层调用方可能是恶意的（需校验所有输入）
- 数据库返回的数据可能是恶意的（需校验所有输出）
- SZ-ORM 内部代码是受信任的（无需内部校验）

## Known Security Issues

### 已修复

| CVE/ID | 严重级别 | 描述 | 修复版本 |
|--------|----------|------|----------|
| C-2 | Critical | MorphTo 关系加载 SQL 注入 | 1.0.1 |
| C-3 | Critical | 分页 SQL 方言不兼容 | 1.0.1 |

### 已知限制（非漏洞）

| 限制 | 说明 |
|------|------|
| Kafka ack() 为 no-op | 启用 `enable.auto.commit=true`，消息处理失败仍提交 offset |
| Pulsar ack() 为 no-op | 需扩展 message_id → consumer 映射 |
| NATS Core 无 ACK | NATS Core 为 at-most-once，需 JetStream 才能支持 ACK |
| 连接池无健康检查 | 未实现 heartbeat，长时间空闲连接可能失效 |
| 事务无死锁检测 | 未实现自动重试死锁的事务 |

## Security Contacts

- **Security Lead**: @ljclz
- **Backup**: 无（单作者项目）
- **PGP Key**: 暂无（待生成）

## Acknowledgments

感谢以下人员报告安全漏洞（如有）：

- （暂无）

---

**注意**：SZ-ORM 为单作者开源项目，无 SLA 保证。Critical/High 漏洞将尽力在 90 天内修复，但无法律约束力。
