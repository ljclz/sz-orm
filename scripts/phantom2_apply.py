#!/usr/bin/env python3
"""PHANTOM-2 决策应用脚本 — 修改 Cargo.toml default 数组。

用法：
    python scripts/phantom2_apply.py --workspace-root . --verified phantom2-verified.json --output phantom2-apply-log.json
"""
import argparse
import json
import re
import shutil
from pathlib import Path
from collections import defaultdict


def find_cargo_toml(workspace_root: Path, pkg: str) -> Path:
    return workspace_root / "packages" / pkg / "Cargo.toml"


def apply_decision_a(cargo_path: Path, feature: str) -> dict:
    """将 feature 添加到 Cargo.toml 的 default 数组。"""
    content = cargo_path.read_text(encoding="utf-8")
    lines = content.splitlines()

    in_features = False
    default_start = None
    default_end = None
    existing_features = set()

    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("[features]"):
            in_features = True
            continue
        if in_features:
            if stripped.startswith("[") and not stripped.startswith("[features"):
                break
            if stripped.startswith("default"):
                default_start = i
                m = re.match(r'^default\s*=\s*\[(.*)\]', stripped)
                if m:
                    deps_str = m.group(1).strip()
                    if deps_str:
                        existing_features = {d.strip().strip('"') for d in deps_str.split(",") if d.strip()}
                    if "]" in stripped:
                        default_end = i
                    else:
                        for j in range(i + 1, len(lines)):
                            if "]" in lines[j]:
                                default_end = j
                                break
                break

    if default_start is None:
        return {"success": False, "error": "default array not found"}

    if feature in existing_features:
        return {"success": True, "already_present": True, "line": default_start + 1}

    backup_path = cargo_path.parent / "Cargo.toml.bak"
    if not backup_path.exists():
        shutil.copy2(cargo_path, backup_path)

    if default_start == default_end:
        m = re.match(r'^(\s*)default\s*=\s*\[(.*)\]', lines[default_start])
        indent = m.group(1)
        existing_str = m.group(2).strip()
        if existing_str:
            new_content = f'{indent}default = [{existing_str}, "{feature}"]'
        else:
            new_content = f'{indent}default = ["{feature}"]'
        lines[default_start] = new_content
    else:
        indent = "    "
        last_feat_line = default_end - 1
        last_feat = lines[last_feat_line].strip()
        if last_feat.endswith(","):
            lines.insert(default_end, f'{indent}"{feature}",')
        elif last_feat.endswith('"'):
            lines[last_feat_line] = lines[last_feat_line].rstrip() + ","
            lines.insert(default_end, f'{indent}"{feature}",')
        else:
            lines.insert(default_end, f'{indent}"{feature}",')

    cargo_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return {"success": True, "already_present": False, "line": default_start + 1, "backup": str(backup_path)}


def main():
    parser = argparse.ArgumentParser(description="PHANTOM-2 决策应用")
    parser.add_argument("--workspace-root", default=".", help="工作空间根路径")
    parser.add_argument("--verified", default="phantom2-verified.json", help="验证结果 JSON")
    parser.add_argument("--output", default="phantom2-apply-log.json", help="输出变更记录")
    parser.add_argument("--dry-run", action="store_true", help="仅输出变更不实际修改")
    args = parser.parse_args()

    workspace_root = Path(args.workspace_root).resolve()

    with open(args.verified, "r", encoding="utf-8") as f:
        data = json.load(f)

    a_gates = [g for g in data["gates"] if g.get("final_decision") == "A"]
    print(f"决策 A：{len(a_gates)} 个，{'dry-run' if args.dry_run else '开始应用'}...")

    changes = []
    by_pkg = defaultdict(list)
    for g in a_gates:
        by_pkg[g["package"]].append(g["feature"])

    for pkg, features in sorted(by_pkg.items()):
        cargo_path = find_cargo_toml(workspace_root, pkg)
        if not cargo_path.exists():
            print(f"  {pkg}: Cargo.toml not found!")
            continue
        print(f"  {pkg}: {len(features)} features → {', '.join(features)}")
        if args.dry_run:
            for feat in features:
                changes.append({"package": pkg, "feature": feat, "cargo_path": str(cargo_path), "action": "add_to_default"})
            continue
        for feat in features:
            result = apply_decision_a(cargo_path, feat)
            changes.append({
                "package": pkg,
                "feature": feat,
                "cargo_path": str(cargo_path),
                "action": "add_to_default",
                "result": result,
            })

    output = {
        "total_changes": len(changes),
        "dry_run": args.dry_run,
        "changes": changes,
    }
    Path(args.output).write_text(json.dumps(output, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"\n应用完成：{len(changes)} 个变更")
    print(f"输出文件：{args.output}")


if __name__ == "__main__":
    main()