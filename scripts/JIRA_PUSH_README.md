# Jira 推送脚本使用说明

## 功能

自动解析 `TODO.md` 中的待办项，按优先级排序后推送到 Jira 创建任务。

## 配置步骤

### 1. 获取 Jira API Token

#### Jira Cloud
1. 访问 https://id.atlassian.com/manage/api-tokens
2. 点击 "Create API token"
3. 复制生成的 token

#### Jira Server/Data Center
1. 点击右上角头像 → Profile
2. 点击 "Personal Access Tokens"
3. 创建新 token

### 2. 编辑脚本配置

打开 `scripts/push_todos_to_jira.py`，修改 `config` 字典：

```python
config = {
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
}
```

### 3. 运行脚本

```bash
cd sz-orm
python scripts/push_todos_to_jira.py
```

## 输出示例

```
============================================================
  sz-orm 待办项 Jira 推送脚本
============================================================

解析文件：TODO.md
找到 6 个待办项
未完成：3 个

待推送任务（按优先级）：
  [1] [高] 待办：变异测试补跑
  [2] [中] 待办：对比文档同步 cabi 新 API
  [3] [低] 待办：布隆过滤器双实现合并

[1/3] 创建任务：待办：变异测试补跑...
  ✅ 成功：SZ-123

[2/3] 创建任务：待办：对比文档同步 cabi 新 API...
  ✅ 成功：SZ-124

[3/3] 创建任务：待办：布隆过滤器双实现合并...
  ✅ 成功：SZ-125

============================================================
  sz-orm 待办项 Jira 推送报告
============================================================
  总待办项：3
  成功创建：3
  失败：0

【高优先级】(1 项)
  - [SZ-123] 待办：变异测试补跑

【中优先级】(1 项)
  - [SZ-124] 待办：对比文档同步 cabi 新 API

【低优先级】(1 项)
  - [SZ-125] 待办：布隆过滤器双实现合并

============================================================
报告已保存：scripts/jira_push_report.txt
```

## 优先级规则

| 章节 | 优先级 |
|------|--------|
| 发布前检查 | 高 |
| 文档同步 | 中 |
| 依赖清理 | 中 |
| 架构债 | 低 |

## 常见问题

### Q: 报错 "401 Unauthorized"
A: 检查 `JIRA_USER` 和 `JIRA_TOKEN` 是否正确

### Q: 报错 "Project key does not exist"
A: 确认 `JIRA_PROJECT` 是你的项目 Key（可在 Jira 项目列表查看）

### Q: 报错 "Issue type does not exist"
A: 确认你的项目中有该任务类型（Task/Story/Bug 等）

### Q: 如何跳过已完成的待办项？
A: 脚本自动识别 `✅` 标记，已完成的不会推送

## 依赖

脚本使用 Python 标准库 + `requests` 包。如未安装：

```bash
pip install requests
```
