#!/usr/bin/env python3
"""
sz-orm 待办项自动推送到 Jira

功能：
1. 解析 TODO.md 中的待办项
2. 按优先级排序（发布前检查 > 文档同步 > 依赖清理 > 架构债）
3. 通过 Jira REST API 创建任务

使用方法：
1. 复制 config 字典并填写你的 Jira 信息
2. 运行：python scripts/push_todos_to_jira.py

配置说明：
- JIRA_URL: Jira 实例地址
  - Cloud: https://your-domain.atlassian.net
  - Server: https://jira.your-company.com
- JIRA_USER: 邮箱（Cloud）或用户名（Server）
- JIRA_TOKEN: API Token（Cloud）或密码/PAT（Server）
- JIRA_PROJECT: 项目 Key（如 SZ、ORM）
- JIRA_ISSUE_TYPE: 任务类型（如 Task、Story）
"""

import re
import sys
import json
from pathlib import Path
from dataclasses import dataclass
from typing import Optional

# ============================================================
# 配置区域 - 请修改此处
# ============================================================
config = {
    # Jira 实例地址
    "JIRA_URL": "https://your-domain.atlassian.net",
    # 邮箱（Cloud）或用户名（Server）
    "JIRA_USER": "your-email@example.com",
    # API Token（Cloud）或密码/PAT（Server）
    "JIRA_TOKEN": "your-api-token",
    # 项目 Key
    "JIRA_PROJECT": "SZ",
    # 任务类型
    "JIRA_ISSUE_TYPE": "Task",
    # 默认经办人
    "JIRA_ASSIGNEE": "your-email@example.com",
    # 优先级映射
    "PRIORITY_MAP": {
        "高": "High",
        "中": "Medium",
        "低": "Low",
    },
    # 状态映射（用于识别已完成）
    "DONE_MARKERS": ["✅", "[x]", "[X]"],
}

# ============================================================
# 数据结构
# ============================================================
@dataclass
class TodoItem:
    """待办项数据结构"""
    title: str
    section: str
    priority: str  # 高/中/低
    source: str
    description: str
    status: str  # 待办/已完成/阻塞
    estimated_time: str = ""
    raw_text: str = ""


# ============================================================
# 优先级定义
# ============================================================
SECTION_PRIORITY = {
    "发布前检查": "高",
    "文档同步": "中",
    "依赖清理": "中",
    "架构债": "低",
}

PRIORITY_ORDER = {"高": 0, "中": 1, "低": 2}


# ============================================================
# TODO.md 解析器
# ============================================================
def parse_todo_md(file_path: Path) -> list[TodoItem]:
    """解析 TODO.md 文件，提取待办项"""
    if not file_path.exists():
        print(f"错误：文件不存在：{file_path}")
        sys.exit(1)

    content = file_path.read_text(encoding="utf-8")
    items = []
    current_section = ""
    current_item = None
    current_lines = []

    for line in content.split("\n"):
        # 检测章节标题（## 开头）
        if line.startswith("## "):
            current_section = line[3:].strip()
            continue

        # 检测待办项标题（### 开头）
        if line.startswith("### "):
            # 保存上一个待办项
            if current_item:
                current_item.raw_text = "\n".join(current_lines)
                items.append(current_item)

            title = line[4:].strip()
            # 提取状态标记
            status = "待办"
            if "✅" in title or "[已完成]" in title:
                status = "已完成"
            elif "⚠️" in title or "[阻塞]" in title:
                status = "阻塞"
            elif "❌" in title:
                status = "失败"

            # 清理标题中的状态标记
            clean_title = re.sub(r'[✅⚠️❌]\s*', '', title)
            clean_title = re.sub(r'\[已完成\]|\[阻塞\]|\[失败\]', '', clean_title).strip()

            current_item = TodoItem(
                title=clean_title,
                section=current_section,
                priority=SECTION_PRIORITY.get(current_section, "中"),
                source="TODO.md",
                description="",
                status=status,
            )
            current_lines = [line]
            continue

        # 收集描述信息
        if current_item:
            current_lines.append(line)
            # 提取关键信息
            if line.startswith("**来源**："):
                current_item.source = line[6:].strip()
            elif line.startswith("**预计处理时间**："):
                current_item.estimated_time = line[9:].strip()
            elif line.startswith("**状态**："):
                status_text = line[6:].strip()
                if "已完成" in status_text:
                    current_item.status = "已完成"
                elif "阻塞" in status_text:
                    current_item.status = "阻塞"

    # 保存最后一个待办项
    if current_item:
        current_item.raw_text = "\n".join(current_lines)
        items.append(current_item)

    return items


def filter_todos(items: list[TodoItem]) -> list[TodoItem]:
    """过滤出未完成的待办项"""
    return [item for item in items if item.status != "已完成"]


def sort_by_priority(items: list[TodoItem]) -> list[TodoItem]:
    """按优先级排序"""
    return sorted(items, key=lambda x: (PRIORITY_ORDER.get(x.priority, 1), x.title))


# ============================================================
# Jira API 客户端
# ============================================================
class JiraClient:
    """Jira REST API 客户端"""

    def __init__(self, config: dict):
        self.base_url = config["JIRA_URL"].rstrip("/")
        self.user = config["JIRA_USER"]
        self.token = config["JIRA_TOKEN"]
        self.project = config["JIRA_PROJECT"]
        self.issue_type = config["JIRA_ISSUE_TYPE"]
        self.assignee = config.get("JIRA_ASSIGNEE", "")

        # 验证配置
        if self.base_url == "https://your-domain.atlassian.net":
            print("⚠️  警告：JIRA_URL 未配置，请使用占位符模式")
            self.dry_run = True
        else:
            self.dry_run = False

    def _get_auth(self) -> tuple[str, str]:
        """获取认证信息"""
        return (self.user, self.token)

    def _get_headers(self) -> dict:
        """获取请求头"""
        return {
            "Content-Type": "application/json",
            "Accept": "application/json",
        }

    def create_issue(self, item: TodoItem) -> Optional[dict]:
        """创建 Jira 任务"""
        if self.dry_run:
            print(f"  [占位符模式] 将创建任务：{item.title}")
            return {"key": "PLACEHOLDER", "self": ""}

        url = f"{self.base_url}/rest/api/2/issue"

        # 构建任务描述
        description = self._build_description(item)

        payload = {
            "fields": {
                "project": {"key": self.project},
                "summary": item.title,
                "description": description,
                "issuetype": {"name": self.issue_type},
                "priority": {"name": config["PRIORITY_MAP"].get(item.priority, "Medium")},
            }
        }

        if self.assignee:
            payload["fields"]["assignee"] = {"name": self.assignee}

        try:
            import requests
            response = requests.post(
                url,
                headers=self._get_headers(),
                auth=self._get_auth(),
                json=payload,
                timeout=30,
            )
            response.raise_for_status()
            result = response.json()
            return {"key": result["key"], "self": result["self"]}
        except requests.exceptions.RequestException as e:
            print(f"  ❌ 创建失败：{e}")
            if hasattr(e, 'response') and e.response is not None:
                print(f"     响应：{e.response.text[:200]}")
            return None

    def _build_description(self, item: TodoItem) -> str:
        """构建任务描述"""
        desc = f"*来源：* {item.source}\n\n"
        desc += f"*优先级：* {item.priority}\n\n"
        desc += f"*所属章节：* {item.section}\n\n"

        if item.estimated_time:
            desc += f"*预计处理时间：* {item.estimated_time}\n\n"

        # 提取描述内容（**详情**：之后的内容）
        if "**详情**：" in item.raw_text:
            detail = item.raw_text.split("**详情**：")[1].split("\n---")[0].strip()
            desc += f"*详情：*\n{detail}\n\n"

        desc += "---\n"
        desc += f"*自动生成：* sz-orm TODO 推送脚本\n"
        desc += f"*原始文件：* TODO.md"

        return desc

    def batch_create(self, items: list[TodoItem]) -> list[dict]:
        """批量创建任务"""
        results = []
        for i, item in enumerate(items, 1):
            print(f"[{i}/{len(items)}] 创建任务：{item.title[:50]}...")
            result = self.create_issue(item)
            if result:
                results.append({
                    "title": item.title,
                    "key": result["key"],
                    "url": result["self"],
                    "priority": item.priority,
                })
                print(f"  ✅ 成功：{result['key']}")
            else:
                print(f"  ❌ 失败")
        return results


# ============================================================
# 报告生成
# ============================================================
def generate_report(items: list[TodoItem], results: list[dict]) -> str:
    """生成推送报告"""
    report = []
    report.append("=" * 60)
    report.append("  sz-orm 待办项 Jira 推送报告")
    report.append("=" * 60)
    report.append(f"  总待办项：{len(items)}")
    report.append(f"  成功创建：{len(results)}")
    report.append(f"  失败：{len(items) - len(results)}")
    report.append("")

    # 按优先级分组
    for priority in ["高", "中", "低"]:
        priority_items = [r for r in results if r["priority"] == priority]
        if priority_items:
            report.append(f"【{priority}优先级】({len(priority_items)} 项)")
            for r in priority_items:
                report.append(f"  - [{r['key']}] {r['title']}")
            report.append("")

    report.append("=" * 60)
    return "\n".join(report)


# ============================================================
# 主函数
# ============================================================
def main():
    """主函数"""
    print("=" * 60)
    print("  sz-orm 待办项 Jira 推送脚本")
    print("=" * 60)
    print()

    # 解析 TODO.md
    todo_path = Path(__file__).parent.parent / "TODO.md"
    print(f"解析文件：{todo_path}")
    items = parse_todo_md(todo_path)
    print(f"找到 {len(items)} 个待办项")

    # 过滤未完成的
    pending = filter_todos(items)
    print(f"未完成：{len(pending)} 个")

    # 按优先级排序
    sorted_items = sort_by_priority(pending)

    # 打印预览
    print("\n待推送任务（按优先级）：")
    for i, item in enumerate(sorted_items, 1):
        print(f"  [{i}] [{item.priority}] {item.title}")

    # 确认推送
    if len(sorted_items) == 0:
        print("\n✅ 没有待推送的任务")
        return

    print(f"\n准备推送 {len(sorted_items)} 个任务到 Jira...")

    # 创建 Jira 客户端
    jira = JiraClient(config)

    if jira.dry_run:
        print("\n⚠️  占位符模式：请先配置 Jira 信息")
        print("\n配置步骤：")
        print("  1. 打开 scripts/push_todos_to_jira.py")
        print("  2. 修改 config 字典中的以下字段：")
        print("     - JIRA_URL: 你的 Jira 实例地址")
        print("     - JIRA_USER: 你的邮箱或用户名")
        print("     - JIRA_TOKEN: API Token 或密码")
        print("     - JIRA_PROJECT: 项目 Key（如 SZ、ORM）")
        print("  3. 重新运行脚本")
        print("\n获取 API Token（Jira Cloud）：")
        print("  https://id.atlassian.com/manage/api-tokens")

    # 批量创建
    results = jira.batch_create(sorted_items)

    # 生成报告
    report = generate_report(sorted_items, results)
    print("\n" + report)

    # 保存报告
    report_path = Path(__file__).parent / "jira_push_report.txt"
    report_path.write_text(report, encoding="utf-8")
    print(f"\n报告已保存：{report_path}")


if __name__ == "__main__":
    main()
