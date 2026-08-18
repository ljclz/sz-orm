#!/usr/bin/env python3
"""
sz-orm 待办项自动推送到 Jira

功能：
1. 解析 TODO.md 中的待办项
2. 按优先级排序（发布前检查 > 文档同步 > 依赖清理 > 架构债）
3. 通过 Jira REST API 创建任务
4. 支持自动重试和告警通知

使用方法：
1. 复制 config 字典并填写你的 Jira 信息
2. 运行：python scripts/push_todos_to_jira.py
3. 添加 --debug 参数查看详细日志

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
import time
import logging
import argparse
from pathlib import Path
from dataclasses import dataclass, field
from typing import Optional

# ============================================================
# 日志配置
# ============================================================
logger = logging.getLogger("jira_push")

def setup_logging(debug: bool = False):
    """配置日志级别"""
    level = logging.DEBUG if debug else logging.INFO
    logging.basicConfig(
        level=level,
        format='%(asctime)s [%(levelname)s] %(message)s',
        datefmt='%Y-%m-%d %H:%M:%S'
    )


# ============================================================
# 配置区域 - 请修改此处
# ============================================================
config = {
    # Jira 实例地址
    "JIRA_URL": "https://ljclz.atlassian.net",
    # 邮箱（Cloud）或用户名（Server）
    "JIRA_USER": "zhangmingjie@ljclz.vip",
    # API Token（Cloud）或密码/PAT（Server）
    "JIRA_TOKEN": "ATATT3xFfGF0NTl3rx3-woADTK_DQtlHIQkLg4Li70lxBse3Wb4eAwfc49YVZzlbhUvwPF39xEl775cLu68jY3NvcKcDocHWYvcspxhurWiilDz3RztveoxzukOGXgHHXsP4F0JV84G-eX2kHGctBnWi0aUlMeu6TSH_1X9d3AJjqy14BVyLD6w=4FB4CFA6",
    # 项目 Key
    "JIRA_PROJECT": "SZ",
    # 任务类型
    "JIRA_ISSUE_TYPE": "Task",
    # 默认经办人
    "JIRA_ASSIGNEE": "zhangmingjie@ljclz.vip",
    # 优先级映射
    "PRIORITY_MAP": {
        "高": "High",
        "中": "Medium",
        "低": "Low",
    },
    # 状态映射（用于识别已完成）
    "DONE_MARKERS": ["✅", "[x]", "[X]"],

    # ============================================================
    # 重试配置
    # ============================================================
    # 最大重试次数
    "MAX_RETRIES": 3,
    # 重试间隔（秒）
    "RETRY_DELAY": 2,
    # 是否启用指数退避（每次重试间隔翻倍）
    "EXPONENTIAL_BACKOFF": True,

    # ============================================================
    # 告警通知配置
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
    debug_info: dict = field(default_factory=dict)


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
# 告警通知器
# ============================================================
class AlertNotifier:
    """告警通知器，支持邮件和 Webhook"""

    def __init__(self, config: dict):
        self.enabled = config.get("ALERT_ENABLED", False)
        self.method = config.get("ALERT_METHOD", "webhook")
        self.email_config = config.get("ALERT_EMAIL", {})
        self.webhook_config = config.get("ALERT_WEBHOOK", {})

    def send_failure_alert(self, item_title: str, error_msg: str, retry_count: int):
        """发送失败告警"""
        if not self.enabled:
            logger.debug("告警未启用，跳过通知")
            return

        subject = f"[sz-orm] Jira 推送失败：{item_title}"
        message = f"""任务推送失败通知

任务标题：{item_title}
错误信息：{error_msg}
重试次数：{retry_count}/{config['MAX_RETRIES']}
时间：{time.strftime('%Y-%m-%d %H:%M:%S')}

请检查 Jira 配置或网络连接后重新运行脚本。
"""
        if self.method in ("email", "both"):
            self._send_email_alert(subject, message)
        if self.method in ("webhook", "both"):
            self._send_webhook_alert(subject, message)

    def _send_email_alert(self, subject: str, message: str):
        """发送邮件告警"""
        try:
            import smtplib
            from email.mime.text import MIMEText

            msg = MIMEText(message, "plain", "utf-8")
            msg["Subject"] = subject
            msg["From"] = self.email_config["sender"]
            msg["To"] = ", ".join(self.email_config["recipients"])

            server = smtplib.SMTP(
                self.email_config["smtp_server"],
                self.email_config["smtp_port"]
            )
            server.starttls()
            server.login(
                self.email_config["sender"],
                self.email_config["password"]
            )
            server.sendmail(
                self.email_config["sender"],
                self.email_config["recipients"],
                msg.as_string()
            )
            server.quit()
            logger.info("邮件告警发送成功")
        except Exception as e:
            logger.error(f"邮件告警发送失败：{e}")

    def _send_webhook_alert(self, subject: str, message: str):
        """发送 Webhook 告警（钉钉/企业微信/飞书格式）"""
        try:
            import requests

            # 钉钉格式
            payload = {
                "msgtype": "text",
                "text": {
                    "content": f"{subject}\n\n{message}"
                }
            }

            headers = {"Content-Type": "application/json"}
            response = requests.post(
                self.webhook_config["url"],
                json=payload,
                headers=headers,
                timeout=10
            )
            response.raise_for_status()
            logger.info("Webhook 告警发送成功")
        except Exception as e:
            logger.error(f"Webhook 告警发送失败：{e}")


# ============================================================
# TODO.md 解析器
# ============================================================
def parse_todo_md(file_path: Path) -> list[TodoItem]:
    """解析 TODO.md 文件，提取待办项"""
    logger.debug(f"开始解析文件：{file_path}")

    if not file_path.exists():
        logger.error(f"文件不存在：{file_path}")
        sys.exit(1)

    content = file_path.read_text(encoding="utf-8")
    lines = content.split("\n")
    logger.debug(f"文件总行数：{len(lines)}")

    items = []
    current_section = ""
    current_item = None
    current_lines = []
    line_num = 0

    for line in lines:
        line_num += 1
        stripped = line.strip()

        # 跳过空行
        if not stripped:
            if current_item:
                current_lines.append(line)
            continue

        logger.debug(f"L{line_num}: {stripped[:60]}...")

        # 检测章节标题（## 开头）
        if stripped.startswith("## "):
            current_section = stripped[3:].strip()
            logger.debug(f"  → 检测到章节：{current_section}")
            continue

        # 检测待办项标题（### 开头）
        if stripped.startswith("### "):
            # 保存上一个待办项
            if current_item:
                current_item.raw_text = "\n".join(current_lines)
                items.append(current_item)
                logger.debug(f"  → 保存待办项：{current_item.title[:40]}...")

            title = stripped[4:].strip()
            logger.debug(f"  → 检测到待办项标题：{title[:50]}...")

            # 提取状态标记
            status = "待办"
            if "✅" in title or "[已完成]" in title:
                status = "已完成"
            elif "⚠️" in title or "[阻塞]" in title:
                status = "阻塞"
            elif "❌" in title:
                status = "失败"

            logger.debug(f"    状态：{status}")

            # 清理标题中的状态标记
            clean_title = re.sub(r'[✅⚠️❌]\s*', '', title)
            clean_title = re.sub(r'\[已完成\]|\[阻塞\]|\[失败\]', '', clean_title).strip()

            priority = SECTION_PRIORITY.get(current_section, "中")
            logger.debug(f"    优先级：{priority}（来自章节：{current_section}）")

            current_item = TodoItem(
                title=clean_title,
                section=current_section,
                priority=priority,
                source="TODO.md",
                description="",
                status=status,
                debug_info={"line": line_num, "raw_title": title}
            )
            current_lines = [line]
            continue

        # 收集描述信息
        if current_item:
            current_lines.append(line)

            # 提取关键信息
            if stripped.startswith("**来源**："):
                current_item.source = stripped[6:].strip()
                logger.debug(f"    来源：{current_item.source}")
            elif stripped.startswith("**预计处理时间**："):
                current_item.estimated_time = stripped[9:].strip()
                logger.debug(f"    预计时间：{current_item.estimated_time}")
            elif stripped.startswith("**状态**："):
                status_text = stripped[6:].strip()
                if "已完成" in status_text:
                    current_item.status = "已完成"
                elif "阻塞" in status_text:
                    current_item.status = "阻塞"

    # 保存最后一个待办项
    if current_item:
        current_item.raw_text = "\n".join(current_lines)
        items.append(current_item)
        logger.debug(f"  → 保存最后一个待办项：{current_item.title[:40]}...")

    logger.debug(f"解析完成，共找到 {len(items)} 个待办项")
    return items


def filter_todos(items: list[TodoItem]) -> list[TodoItem]:
    """过滤出未完成的待办项"""
    pending = [item for item in items if item.status != "已完成"]
    logger.debug(f"过滤后剩余 {len(pending)} 个未完成待办项")
    for item in pending:
        logger.debug(f"  - [{item.priority}] {item.title[:40]}...")
    return pending


def sort_by_priority(items: list[TodoItem]) -> list[TodoItem]:
    """按优先级排序"""
    sorted_items = sorted(items, key=lambda x: (PRIORITY_ORDER.get(x.priority, 1), x.title))
    logger.debug(f"排序完成")
    return sorted_items


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
        self.max_retries = config.get("MAX_RETRIES", 3)
        self.retry_delay = config.get("RETRY_DELAY", 2)
        self.exponential_backoff = config.get("EXPONENTIAL_BACKOFF", True)

        # 告警通知器
        self.notifier = AlertNotifier(config)

        # 验证配置
        if self.base_url == "https://your-domain.atlassian.net":
            logger.warning("JIRA_URL 未配置，使用占位符模式")
            self.dry_run = True
        else:
            self.dry_run = False
            logger.info(f"Jira 地址：{self.base_url}")
            logger.info(f"项目：{self.project}")
            logger.info(f"任务类型：{self.issue_type}")

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
        """创建 Jira 任务（带重试）"""
        if self.dry_run:
            logger.info(f"[占位符模式] 将创建任务：{item.title}")
            return {"key": "PLACEHOLDER", "self": ""}

        url = f"{self.base_url}/rest/api/2/issue"
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

        # 重试逻辑
        last_error = None
        for attempt in range(1, self.max_retries + 1):
            logger.debug(f"尝试创建任务（第 {attempt}/{self.max_retries} 次）")

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
                logger.info(f"任务创建成功：{result['key']}")
                return {"key": result["key"], "self": result["self"]}

            except requests.exceptions.RequestException as e:
                last_error = e
                logger.warning(f"第 {attempt} 次尝试失败：{e}")

                if hasattr(e, 'response') and e.response is not None:
                    error_body = e.response.text[:200]
                    logger.debug(f"响应内容：{error_body}")

                # 如果还有重试机会，等待后重试
                if attempt < self.max_retries:
                    if self.exponential_backoff:
                        delay = self.retry_delay * (2 ** (attempt - 1))
                    else:
                        delay = self.retry_delay
                    logger.info(f"等待 {delay} 秒后重试...")
                    time.sleep(delay)

        # 所有重试失败，发送告警
        logger.error(f"任务创建失败（已重试 {self.max_retries} 次）：{last_error}")
        self.notifier.send_failure_alert(
            item.title,
            str(last_error),
            self.max_retries
        )
        return None

    def _build_description(self, item: TodoItem) -> str:
        """构建任务描述"""
        desc = f"*来源：* {item.source}\n\n"
        desc += f"*优先级：* {item.priority}\n\n"
        desc += f"*所属章节：* {item.section}\n\n"

        if item.estimated_time:
            desc += f"*预计处理时间：* {item.estimated_time}\n\n"

        # 提取描述内容
        if "**详情**：" in item.raw_text:
            detail = item.raw_text.split("**详情**：")[1].split("\n---")[0].strip()
            desc += f"*详情：*\n{detail}\n\n"

        desc += "---\n"
        desc += f"*自动生成：* sz-orm TODO 推送脚本\n"
        desc += f"*原始文件：* TODO.md"

        logger.debug(f"任务描述长度：{len(desc)} 字符")
        return desc

    def batch_create(self, items: list[TodoItem]) -> list[dict]:
        """批量创建任务"""
        results = []
        success_count = 0
        fail_count = 0

        for i, item in enumerate(items, 1):
            logger.info(f"[{i}/{len(items)}] 创建任务：{item.title[:50]}...")
            result = self.create_issue(item)

            if result:
                results.append({
                    "title": item.title,
                    "key": result["key"],
                    "url": result["self"],
                    "priority": item.priority,
                })
                success_count += 1
            else:
                fail_count += 1

        logger.info(f"批量创建完成：成功 {success_count}，失败 {fail_count}")
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
    parser = argparse.ArgumentParser(description="sz-orm 待办项 Jira 推送脚本")
    parser.add_argument("--debug", action="store_true", help="启用 DEBUG 日志")
    args = parser.parse_args()

    # 配置日志
    setup_logging(args.debug)

    logger.info("=" * 60)
    logger.info("  sz-orm 待办项 Jira 推送脚本")
    logger.info("=" * 60)

    # 解析 TODO.md
    todo_path = Path(__file__).parent.parent / "TODO.md"
    logger.info(f"解析文件：{todo_path}")
    items = parse_todo_md(todo_path)
    logger.info(f"找到 {len(items)} 个待办项")

    # 统计各状态数量
    status_count = {}
    for item in items:
        status_count[item.status] = status_count.get(item.status, 0) + 1
    logger.info(f"状态分布：{status_count}")

    # 过滤未完成的
    pending = filter_todos(items)
    logger.info(f"未完成：{len(pending)} 个")

    # 按优先级排序
    sorted_items = sort_by_priority(pending)

    # 打印预览
    logger.info("\n待推送任务（按优先级）：")
    for i, item in enumerate(sorted_items, 1):
        logger.info(f"  [{i}] [{item.priority}] {item.title}")

    # 确认推送
    if len(sorted_items) == 0:
        logger.info("\n✅ 没有待推送的任务")
        return

    logger.info(f"\n准备推送 {len(sorted_items)} 个任务到 Jira...")

    # 创建 Jira 客户端
    jira = JiraClient(config)

    if jira.dry_run:
        logger.warning("\n占位符模式：请先配置 Jira 信息")
        logger.info("\n配置步骤：")
        logger.info("  1. 打开 scripts/push_todos_to_jira.py")
        logger.info("  2. 修改 config 字典中的以下字段：")
        logger.info("     - JIRA_URL: 你的 Jira 实例地址")
        logger.info("     - JIRA_USER: 你的邮箱或用户名")
        logger.info("     - JIRA_TOKEN: API Token 或密码")
        logger.info("     - JIRA_PROJECT: 项目 Key（如 SZ、ORM）")
        logger.info("  3. 重新运行脚本")
        logger.info("\n获取 API Token（Jira Cloud）：")
        logger.info("  https://id.atlassian.com/manage/api-tokens")

    # 批量创建
    results = jira.batch_create(sorted_items)

    # 生成报告
    report = generate_report(sorted_items, results)
    logger.info("\n" + report)

    # 保存报告
    report_path = Path(__file__).parent / "jira_push_report.txt"
    report_path.write_text(report, encoding="utf-8")
    logger.info(f"\n报告已保存：{report_path}")

    # 退出码
    if len(results) == len(sorted_items):
        sys.exit(0)
    else:
        sys.exit(1)


if __name__ == "__main__":
    main()
