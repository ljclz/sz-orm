#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""CI 阻断语义审计门禁（AGENTS.md 门禁 31，2026-10-27 SDET 评测新增）
================================================================
背景：2026-10-27 SDET 评测发现 `.github/workflows/ci.yml` 有 6 个 job 使用
`continue-on-error: true`（失败不阻断 PR）——fuzz / benchmark / integration /
mq-integration / feature-matrix / unused-deps。这些 job 失败只记录 artifact，
形成"门禁形同虚设"。本脚本强制：任何 `continue-on-error` job 必须登记在
豁免表中并给出理由，否则红牌（fail-closed）。

豁免表语义：登记即决策留痕。团队若评审后认为某 job 应保持非阻断，只需在
EXEMPT_JOBS 中登记；新增非阻断 job 必须显式登记，否则被本门禁拦截。

用法：
  python scripts/check-ci-blocking.py
退出码：0 = 所有 continue-on-error job 均在豁免表内；1 = 存在未登记的非阻断 job
"""

import os
import sys
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover
    yaml = None

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"

# continue-on-error job 豁免登记表（job 名 → 理由）
# 2026-10-27 SDET 评测盘点：以下 6 个 job 为存量非阻断，登记保留现状。
# 后续新增非阻断 job 必须在此登记，否则门禁 31 红牌。
EXEMPT_JOBS = {
    "benchmark": "ADR-005：性能波动不阻断 PR（基准结果作为 artifact 上传）",
    "fuzz": "cargo-fuzz 为探索性测试（60s/target），崩溃不阻断合并",
    "integration": "真实云服务集成（RabbitMQ/Mosquitto/MinIO）受环境可用性影响",
    "mq-integration": "5 种 MQ broker 服务容器依赖较重，偶发环境问题不阻断",
    "feature-matrix": "cargo-hack 全 feature 编译矩阵，存在已知原生依赖环境问题",
    "unused-deps": "cargo-udeps 需 nightly，结果为参考性警告",
}


def audit_workflows(workflows_dir):
    """审计一个 workflow 目录，返回 (problems, total_non_blocking_jobs)。

    problems 元素为字符串（如 "ci.yml: job 'fuzz' 未登记豁免"）。

    continue-on-error 两种形态都会统计并要求登记：
    - 无条件（true）：job 在 PR/main 均不阻断，必须登记豁免表
    - 条件式（如 `${{ github.ref != 'refs/heads/main' }}`）：PR 分支不阻断、
      main 分支阻断（已缓解），仍建议登记豁免表（2026-10-27 并发会话改造）
    """
    problems = []
    total = 0
    wf_dir = Path(workflows_dir)
    if not wf_dir.is_dir():
        return [f"workflow 目录不存在: {wf_dir}"], 0
    wf_files = sorted(wf_dir.glob("*.yml")) + sorted(wf_dir.glob("*.yaml"))
    for wf in wf_files:
        try:
            doc = yaml.safe_load(wf.read_text(encoding="utf-8", errors="replace"))
        except Exception as e:  # noqa: BLE001
            problems.append(f"{wf.name}: YAML 解析失败 ({e})")
            continue
        if not isinstance(doc, dict):
            continue
        jobs = doc.get("jobs")
        if not isinstance(jobs, dict):
            continue
        for job_name, cfg in jobs.items():
            if not isinstance(cfg, dict):
                continue
            coe = cfg.get("continue-on-error")
            if coe in (True, "true", "True"):
                total += 1
                if job_name not in EXEMPT_JOBS:
                    problems.append(
                        f"{wf.name}: job '{job_name}' 使用无条件 continue-on-error 但未登记豁免"
                    )
            elif isinstance(coe, str) and "${{" in coe:
                total += 1
                if job_name not in EXEMPT_JOBS:
                    problems.append(
                        f"{wf.name}: job '{job_name}' 使用条件式 continue-on-error（{coe}）"
                        "但未登记豁免（条件式 = PR 分支不阻断，建议登记理由）"
                    )
    return problems, total


def main():
    print("=" * 60)
    print("  CI 阻断语义审计门禁（门禁 31）")
    print("=" * 60)
    if yaml is None:
        print("  ❌ PyYAML 未安装（pip install pyyaml）")
        print("=" * 60)
        return 1
    problems, total = audit_workflows(WORKFLOWS)
    print(f"  扫描目录: {WORKFLOWS}")
    print(f"  continue-on-error job 总数: {total}")
    print()
    if problems:
        for p in problems:
            print(f"  ❌ {p}")
        print()
        print("=" * 60)
        print(f"  结果: ❌ 存在 {len(problems)} 个未登记豁免的非阻断 job")
        print("  （如需保持非阻断，请在 scripts/check-ci-blocking.py EXEMPT_JOBS 登记理由）")
        print("=" * 60)
        return 1
    print("  ✅ 所有 continue-on-error job 均已登记豁免")
    print("=" * 60)
    print(f"  结果: ✅ 通过")
    print("=" * 60)
    return 0


if __name__ == "__main__":
    sys.exit(main())