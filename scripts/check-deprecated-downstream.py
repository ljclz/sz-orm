#!/usr/bin/env python3
"""检查下游项目对 deprecated API 的调用，输出迁移状态 JSON。

用法：
    python scripts/check-deprecated-downstream.py \
        --downstream sz-pay:E:/vue/test/sz-pay \
        --downstream new-wxapp:E:/vue/test/new-wxapp/rust \
        --deprecated-list scripts/deprecated_apis.json \
        --output docs/deprecated-removal-report-v7.2.0.json
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


def extract_symbol_names(api_path: str) -> list[str]:
    """从 api_path 提取用于 grep 的符号名列表。

    例如 "sz_orm_query_builder::Query::select" → ["Query::select", "Query", "select"]
    """
    parts = api_path.split("::")
    symbols = []
    # 最后两个部分组合（如 Query::select）
    if len(parts) >= 2:
        symbols.append(f"{parts[-2]}::{parts[-1]}")
    # 最后一个部分（如 select）
    if parts[-1]:
        symbols.append(parts[-1])
    # 倒数第二个部分（如 Query）
    if len(parts) >= 2 and parts[-2]:
        symbols.append(parts[-2])
    return symbols


def scan_downstream(project_dir: str, symbols: list[str]) -> tuple[bool, str | None]:
    """在下游项目目录递归搜索符号调用。

    返回 (found, evidence_file_line)
    - found=True: 找到调用，evidence 为 file:line
    - found=False: 未找到调用，evidence 为 None
    """
    project_path = Path(project_dir)
    if not project_path.exists():
        return False, None

    # 搜索 .rs 文件中的符号引用
    for symbol in symbols:
        try:
            result = subprocess.run(
                ["grep", "-rn", "--include=*.rs", symbol, str(project_path)],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="ignore",
                timeout=30,
            )
            stdout = result.stdout or ""
            if result.returncode == 0 and stdout.strip():
                # 过滤掉注释行和 deprecated 标注本身
                for line in stdout.strip().split("\n"):
                    stripped = line.strip()
                    # 跳过注释行
                    if stripped.startswith("#") or "//" in stripped.split(":")[-1][:2]:
                        continue
                    # 跳过 #[deprecated 和 #[allow(deprecated 标注
                    if "#[deprecated" in stripped or "#[allow(deprecated" in stripped:
                        continue
                    # 找到真实调用
                    return True, stripped.split(":")[0] + ":" + stripped.split(":")[1]
        except (subprocess.TimeoutExpired, FileNotFoundError):
            # grep 不可用，用 Python 递归搜索
            for rs_file in project_path.rglob("*.rs"):
                try:
                    content = rs_file.read_text(encoding="utf-8", errors="ignore")
                    for i, line in enumerate(content.split("\n"), 1):
                        if symbol in line and "#[deprecated" not in line and "#[allow(deprecated" not in line:
                            if not line.strip().startswith("//"):
                                return True, f"{rs_file}:{i}"
                except Exception:
                    continue

    return False, None


def main():
    parser = argparse.ArgumentParser(description="检查下游 deprecated API 迁移状态")
    parser.add_argument(
        "--downstream",
        action="append",
        required=True,
        help="下游项目，格式 name:path（可多次指定）",
    )
    parser.add_argument(
        "--deprecated-list",
        required=True,
        help="deprecated API 清单 JSON 文件路径",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="输出报告 JSON 文件路径",
    )
    args = parser.parse_args()

    # 加载 deprecated API 清单
    with open(args.deprecated_list, "r", encoding="utf-8") as f:
        deprecated_data = json.load(f)

    apis = deprecated_data.get("apis", [])

    # 解析下游项目
    downstreams = []
    for ds in args.downstream:
        if ":" in ds:
            name, path = ds.split(":", 1)
            downstreams.append((name, path))
        else:
            print(f"警告: 跳过格式错误的 --downstream 参数: {ds}", file=sys.stderr)

    # 评估每个 API
    report = []
    for api in apis:
        api_path = api["api_path"]
        symbols = extract_symbol_names(api_path)

        downstream_status = []
        all_migrated = True

        for project_name, project_path in downstreams:
            project_dir = Path(project_path)
            if not project_dir.exists():
                downstream_status.append({
                    "project_name": project_name,
                    "migrated": None,
                    "evidence_file_line": None,
                    "warning": "DOWNSTREAM_UNEVALUABLE: 路径不可达",
                })
                all_migrated = False
                continue

            found, evidence = scan_downstream(project_path, symbols)
            if found:
                downstream_status.append({
                    "project_name": project_name,
                    "migrated": False,
                    "evidence_file_line": evidence,
                })
                all_migrated = False
            else:
                downstream_status.append({
                    "project_name": project_name,
                    "migrated": True,
                    "evidence_file_line": None,
                })

        report.append({
            "api_path": api_path,
            "code_location": api.get("code_location", ""),
            "replacement_path": api.get("replacement_path", ""),
            "since": api.get("since", ""),
            "note": api.get("note", ""),
            "downstream_status": downstream_status,
            "removable": all_migrated,
        })

    # 输出报告
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)

    # 打印摘要
    removable_count = sum(1 for r in report if r["removable"])
    print(f"评估完成: {len(report)} 个 API, {removable_count} 个可移除, {len(report) - removable_count} 个保留")
    for r in report:
        status = "可移除" if r["removable"] else "保留"
        print(f"  {status}: {r['api_path']}")
        for ds in r["downstream_status"]:
            if ds["migrated"] is True:
                print(f"    {ds['project_name']}: 已迁移")
            elif ds["migrated"] is False:
                print(f"    {ds['project_name']}: 未迁移 (调用点: {ds['evidence_file_line']})")
            else:
                print(f"    {ds['project_name']}: 不可评估 ({ds.get('warning', '')})")


if __name__ == "__main__":
    main()