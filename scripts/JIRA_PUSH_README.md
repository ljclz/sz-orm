# Jira 推送脚本使用说明

> 版本：v2.0
> 更新：2026-08-16

## 功能特性

| 功能 | 说明 |
|------|------|
| 自动解析 | 读取 `TODO.md` 提取待办项 |
| 优先级排序 | 高 → 中 → 低（基于章节分类） |
| 状态过滤 | 自动跳过已完成的待办项 |
| 批量创建 | 通过 Jira REST API 批量创建任务 |
| DEBUG 日志 | `--debug` 参数查看详细解析过程 |
| 自动重试 | 失败自动重试，支持指数退避 |
| 告警通知 | 支持邮件和 Webhook 告警 |
| 双模式支持 | Jira Cloud + Jira Server |

---

## 快速开始

### 1. 获取 Jira API Token

#### Jira Cloud
1. 访问 https://id.atlassian.com/manage/api-tokens
2. 点击 "Create API token"
3. 复制生成的 token

#### Jira Server/Data Center
1. 点击右上角头像 → Profile
2. 点击 "Personal Access Tokens"
3. 创建新 token

### 2. 配置脚本

打开 `scripts/push_todos_to_jira.py`，修改 `config` 字典：

```python
config = {
    # ============================================================
    # Jira 配置（必填）
    # ============================================================
    # Jira 实例地址
    "JIRA_URL": "https://your-domain.atlassian.net",  # Cloud
    # 或 "https://jira.your-company.com"  # Server

    # 邮箱（Cloud）或用户名（Server）
    "JIRA_USER": "your-email@example.com",

    # API Token（Cloud）或密码/PAT（Server）
    "JIRA_TOKEN": "your-api-token",

    # 项目 Key（你的 Jira 项目短代码）
    "JIRA_PROJECT": "SZ",

    # 任务类型
    "JIRA_ISSUE_TYPE": "Task",

    # 默认经办人
    "JIRA_ASSIGNEE": "your-email@example.com",

    # 优先级映射（中文 → 英文）
    "PRIORITY_MAP": {
        "高": "High",
        "中": "Medium",
        "低": "Low",
    },

    # ============================================================
    # 重试配置（可选）
    # ============================================================
    # 最大重试次数
    "MAX_RETRIES": 3,

    # 重试间隔（秒）
    "RETRY_DELAY": 2,

    # 是否启用指数退避（每次重试间隔翻倍）
    "EXPONENTIAL_BACKOFF": True,

    # ============================================================
    # 告警通知配置（可选）
    # ============================================================
    # 是否启用告警通知
    "ALERT_ENABLED": False,

    # 告警方式：email / webhook / both
    "ALERT_METHOD": "webhook",

    # 邮件告警配置
    "ALERT_EMAIL": {
        "smtp_server": "smtp.example.com",
        "smtp_port": 587,
        "sender": "alerts@example.com",
        "recipients": ["admin@example.com"],
        "password": "your-smtp-password",
    },

    # Webhook 告警配置（钉钉/企业微信/飞书等）
    "ALERT_WEBHOOK": {
        "url": "https://hooks.example.com/webhook",
        "secret": "your-webhook-secret",  # 可选，用于签名验证
    },
}
```

### 3. 运行脚本

```bash
# 基本运行
python scripts/push_todos_to_jira.py

# 启用 DEBUG 日志（查看详细解析过程）
python scripts/push_todos_to_jira.py --debug
```

---

## 配置详解

### Jira 配置

| 配置项 | 说明 | 示例 |
|--------|------|------|
| `JIRA_URL` | Jira 实例地址 | `https://company.atlassian.net` |
| `JIRA_USER` | 认证用户名 | `user@company.com` |
| `JIRA_TOKEN` | API Token 或密码 | `ATATT3xFfGF0...` |
| `JIRA_PROJECT` | 项目 Key | `SZ`, `ORM`, `DEV` |
| `JIRA_ISSUE_TYPE` | 任务类型 | `Task`, `Story`, `Bug` |
| `JIRA_ASSIGNEE` | 默认经办人 | `user@company.com` |

### 重试配置

| 配置项 | 说明 | 默认值 |
|--------|------|--------|
| `MAX_RETRIES` | 最大重试次数 | `3` |
| `RETRY_DELAY` | 初始重试间隔（秒） | `2` |
| `EXPONENTIAL_BACKOFF` | 是否启用指数退避 | `True` |

**指数退避示例**（`RETRY_DELAY=2`）：
- 第 1 次失败 → 等待 2 秒
- 第 2 次失败 → 等待 4 秒
- 第 3 次失败 → 等待 8 秒

### 告警配置

#### 邮件告警

```python
"ALERT_ENABLED": True,
"ALERT_METHOD": "email",
"ALERT_EMAIL": {
    "smtp_server": "smtp.qq.com",
    "smtp_port": 587,
    "sender": "alerts@qq.com",
    "recipients": ["admin@qq.com", "dev@qq.com"],
    "password": "your-smtp-auth-code",
}
```

#### Webhook 告警（钉钉）

1. 钉钉群设置 → 智能群助手 → 添加机器人 → 自定义
2. 复制 Webhook 地址
3. 配置脚本：

```python
"ALERT_ENABLED": True,
"ALERT_METHOD": "webhook",
"ALERT_WEBHOOK": {
    "url": "https://oapi.dingtalk.com/robot/send?access_token=YOUR_TOKEN",
}
```

#### Webhook 告警（企业微信）

1. 企业微信群设置 → 群机器人 → 添加
2. 复制 Webhook 地址
3. 配置脚本（同上格式）

---

## 输出示例

### 正常模式

```
2026-08-16 17:00:00 [INFO] ============================================================
2026-08-16 17:00:00 [INFO]   sz-orm 待办项 Jira 推送脚本
2026-08-16 17:00:00 [INFO] ============================================================
2026-08-16 17:00:00 [INFO] 解析文件：E:\sz-orm\TODO.md
2026-08-16 17:00:00 [INFO] 找到 6 个待办项
2026-08-16 17:00:00 [INFO] 状态分布：{'待办': 3, '已完成': 3}
2026-08-16 17:00:00 [INFO] 未完成：3 个
2026-08-16 17:00:00 [INFO]
待推送任务（按优先级）：
2026-08-16 17:00:00 [INFO]   [1] [高] 待办：变异测试补跑
2026-08-16 17:00:00 [INFO]   [2] [中] 待办：对比文档同步 cabi 新 API
2026-08-16 17:00:00 [INFO]   [3] [低] 待办：布隆过滤器双实现合并
2026-08-16 17:00:00 [INFO]
准备推送 3 个任务到 Jira...
2026-08-16 17:00:00 [INFO] Jira 地址：https://company.atlassian.net
2026-08-16 17:00:00 [INFO] 项目：SZ
2026-08-16 17:00:00 [INFO] 任务类型：Task
2026-08-16 17:00:01 [INFO] [1/3] 创建任务：待办：变异测试补跑...
2026-08-16 17:00:02 [INFO] 任务创建成功：SZ-123
2026-08-16 17:00:02 [INFO] [2/3] 创建任务：待办：对比文档同步 cabi 新 API...
2026-08-16 17:00:03 [INFO] 任务创建成功：SZ-124
2026-08-16 17:00:03 [INFO] [3/3] 创建任务：待办：布隆过滤器双实现合并...
2026-08-16 17:00:04 [INFO] 任务创建成功：SZ-125
2026-08-16 17:00:04 [INFO] 批量创建完成：成功 3，失败 0
```

### DEBUG 模式

```bash
python scripts/push_todos_to_jira.py --debug
```

```
2026-08-16 17:00:00 [DEBUG] 开始解析文件：E:\sz-orm\TODO.md
2026-08-16 17:00:00 [DEBUG] 文件总行数：85
2026-08-16 17:00:00 [DEBUG] L1: # sz-orm 待办清单
2026-08-16 17:00:00 [DEBUG] L2:
2026-08-16 17:00:00 [DEBUG] L3: > 自动生成：2026-08-16
...
2026-08-16 17:00:00 [DEBUG] L8: ## 发布前检查
2026-08-16 17:00:00 [DEBUG]   → 检测到章节：发布前检查
2026-08-16 17:00:00 [DEBUG] L10: ### 待办：变异测试补跑
2026-08-16 17:00:00 [DEBUG]   → 检测到待办项标题：待办：变异测试补跑
2026-08-16 17:00:00 [DEBUG]     状态：待办
2026-08-16 17:00:00 [DEBUG]     优先级：高（来自章节：发布前检查）
2026-08-16 17:00:00 [DEBUG]     来源：门禁 20
2026-08-16 17:00:00 [DEBUG]     预计时间：发布后 1 个工作日内
...
2026-08-16 17:00:00 [DEBUG] 解析完成，共找到 6 个待办项
2026-08-16 17:00:00 [DEBUG] 过滤后剩余 3 个未完成待办项
2026-08-16 17:00:00 [DEBUG]   - [高] 待办：变异测试补跑
2026-08-16 17:00:00 [DEBUG]   - [中] 待办：对比文档同步 cabi 新 API
2026-08-16 17:00:00 [DEBUG]   - [低] 待办：布隆过滤器双实现合并
2026-08-16 17:00:00 [DEBUG] 排序完成
```

### 重试过程

```
2026-08-16 17:00:01 [DEBUG] 尝试创建任务（第 1/3 次）
2026-08-16 17:00:02 [WARNING] 第 1 次尝试失败：Connection refused
2026-08-16 17:00:02 [INFO] 等待 2 秒后重试...
2026-08-16 17:00:04 [DEBUG] 尝试创建任务（第 2/3 次）
2026-08-16 17:00:05 [WARNING] 第 2 次尝试失败：Connection refused
2026-08-16 17:00:05 [INFO] 等待 4 秒后重试...
2026-08-16 17:00:09 [DEBUG] 尝试创建任务（第 3/3 次）
2026-08-16 17:00:10 [INFO] 任务创建成功：SZ-126
```

---

## 优先级规则

| 章节 | 优先级 |
|------|--------|
| 发布前检查 | 高 |
| 文档同步 | 中 |
| 依赖清理 | 中 |
| 架构债 | 低 |

---

## 常见问题

### Q: 报错 "401 Unauthorized"
**A**: 检查 `JIRA_USER` 和 `JIRA_TOKEN` 是否正确

### Q: 报错 "Project key does not exist"
**A**: 确认 `JIRA_PROJECT` 是你的项目 Key（可在 Jira 项目列表查看）

### Q: 报错 "Issue type does not exist"
**A**: 确认你的项目中有该任务类型（Task/Story/Bug 等）

### Q: 如何跳过已完成的待办项？
**A**: 脚本自动识别 `✅` 标记，已完成的不会推送

### Q: 如何禁用重试？
**A**: 设置 `"MAX_RETRIES": 0`

### Q: 如何禁用告警？
**A**: 设置 `"ALERT_ENABLED": False`

---

## 依赖

脚本使用 Python 标准库 + `requests` 包。如未安装：

```bash
pip install requests
```

---

## 退出码

| 退出码 | 说明 |
|--------|------|
| `0` | 全部任务创建成功 |
| `1` | 部分或全部任务创建失败 |

---

**文档版本**：v2.0
**最后更新**：2026-08-16
