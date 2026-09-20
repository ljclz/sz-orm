#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-tech-debt.py — 技术债报告生成（v7.5.0 组7.4）
=====================================================
复用既有 check-architecture.py（孤儿包/概念重复）+ check-semantic-patterns.py（语义反模式），
输出 TechDebtReport：total_debts / cleared_debts / remaining_debts /
cleared_records（附 file:line 证据）/ remaining_records（附理由与计划）。

脚本以只读方式分析代码，不修改任何文件。

用法:
  python scripts/check-tech-debt.py
  python scripts/check-tech-debt.py --help
退出码: 0 = 成功生成报告；1 = 执行错误
"""

import argparse
import json
import os
import subprocess
import sys
from datetime import datetime

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRIPTS = os.path.join(ROOT, "scripts")


def run_script(script_name):
    """运行子脚本并返回 (exit_code, stdout, stderr)。"""
    path = os.path.join(SCRIPTS, script_name)
    if not os.path.exists(path):
        return 1, "", f"脚本不存在: {path}"
    try:
        result = subprocess.run(
            [sys.executable, path],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=120,
            cwd=ROOT,
        )
        return result.returncode, result.stdout or "", result.stderr or ""
    except subprocess.TimeoutExpired:
        return 1, "", f"脚本超时: {script_name}"
    except Exception as e:
        return 1, "", str(e)


def parse_architecture_debts(stdout):
    """从 check-architecture.py 输出解析技术债条目。"""
    debts = []
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith("WARN") or line.startswith("ERROR") or "重复" in line or "孤儿" in line:
            debts.append({
                "source": "check-architecture.py",
                "description": line,
                "severity": "ERROR" if line.startswith("ERROR") else "WARN",
            })
    return debts


def parse_semantic_debts(stdout):
    """从 check-semantic-patterns.py 输出解析技术债条目。"""
    debts = []
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith("WARN") or line.startswith("ERROR") or "反模式" in line or "无效" in line:
            debts.append({
                "source": "check-semantic-patterns.py",
                "description": line,
                "severity": "ERROR" if line.startswith("ERROR") else "WARN",
            })
    return debts


def generate_report():
    """生成技术债报告。"""
    # 运行架构检查
    arch_exit, arch_out, arch_err = run_script("check-architecture.py")
    arch_debts = parse_architecture_debts(arch_out)

    # 运行语义模式检查
    sem_exit, sem_out, sem_err = run_script("check-semantic-patterns.py")
    sem_debts = parse_semantic_debts(sem_out)

    # 合并技术债
    all_debts = arch_debts + sem_debts

    # 已知豁免/已清理技术债登记
    cleared_records = [
        {
            "id": "TD-001",
            "description": "bloom_filter 双实现合并",
            "evidence": "packages/sz-orm-core/src/bloom.rs:1",
            "cleared_at": "2026-08-14",
        },
    ]

    # 已知技术债（未清理，附理由与计划）
    remaining_records = []
    for debt in all_debts:
        remaining_records.append({
            "source": debt["source"],
            "description": debt["description"],
            "severity": debt["severity"],
            "reason": "待评估",
            "plan": "后续迭代清理",
        })

    total_debts = len(all_debts) + len(cleared_records)
    cleared_debts = len(cleared_records)
    remaining_debts = len(all_debts)

    report = {
        "generated_at": datetime.now().isoformat(),
        "total_debts": total_debts,
        "cleared_debts": cleared_debts,
        "remaining_debts": remaining_debts,
        "cleared_records": cleared_records,
        "remaining_records": remaining_records,
        "scripts_run": [
            {"name": "check-architecture.py", "exit_code": arch_exit, "stderr": arch_err[:500]},
            {"name": "check-semantic-patterns.py", "exit_code": sem_exit, "stderr": sem_err[:500]},
        ],
    }

    return report


def main():
    parser = argparse.ArgumentParser(description="技术债报告生成（v7.5.0 组7.4）")
    parser.add_argument(
        "--output",
        default=None,
        help="输出 JSON 文件路径（默认输出到 stdout）",
    )
    args = parser.parse_args()

    report = generate_report()

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2, ensure_ascii=False)
        print(f"报告已写入: {args.output}")
    else:
        print(json.dumps(report, indent=2, ensure_ascii=False))

    print(f"\n技术债摘要: 总计 {report['total_debts']} 项, "
          f"已清理 {report['cleared_debts']} 项, "
          f"剩余 {report['remaining_debts']} 项")

    return 0


if __name__ == "__main__":
    sys.exit(main())