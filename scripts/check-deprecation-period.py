#!/usr/bin/env python3
"""废弃保留期检查脚本。

扫描所有 #[deprecated(since = "x.y.z")] 标注，验证废弃保留期（≥2 个 MINOR 版本）已满。
未满则 CI 失败（exit(1)）。

用法：python3 scripts/check-deprecation-period.py
"""

import re
import sys
import json
from pathlib import Path

DEPRECATED_RE = re.compile(r'#\[deprecated\([^)]*since\s*=\s*"(\d+\.\d+\.\d+)"[^)]*\)\]')
VERSION_RE = re.compile(r'^version\s*=\s*"(\d+\.\d+\.\d+)"', re.MULTILINE)
MINOR_PERIOD = 2
EXCLUDE_DIRS = {"target", "node_modules", ".git"}


def parse_version(v: str) -> tuple[int, int, int]:
    major, minor, patch = v.split(".")
    return (int(major), int(minor), int(patch))


def find_deprecated_apis(root: Path) -> list[dict]:
    results = []
    for path in root.rglob("*.rs"):
        if any(part in EXCLUDE_DIRS for part in path.parts):
            continue
        try:
            content = path.read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        for match in DEPRECATED_RE.finditer(content):
            since = match.group(1)
            results.append({
                "file": str(path.relative_to(root)),
                "deprecated_since": since,
            })
    return results


def check(current_version: str, apis: list[dict]) -> list[dict]:
    cur = parse_version(current_version)
    for api in apis:
        since = parse_version(api["deprecated_since"])
        minor_diff = (cur[0] - since[0]) * 1000 + (cur[1] - since[1])
        api["minor_diff"] = minor_diff
        api["status"] = "OK" if minor_diff >= MINOR_PERIOD else "VIOLATION"
    return apis


def main():
    root = Path(__file__).resolve().parent.parent
    cargo_toml = root / "Cargo.toml"
    content = cargo_toml.read_text(encoding="utf-8")
    m = VERSION_RE.search(content)
    if not m:
        print("ERROR: cannot find version in Cargo.toml", file=sys.stderr)
        sys.exit(1)
    current_version = m.group(1)

    apis = find_deprecated_apis(root)
    apis = check(current_version, apis)

    violations = [a for a in apis if a["status"] == "VIOLATION"]
    result = {
        "current_version": current_version,
        "total_deprecated": len(apis),
        "violations": len(violations),
        "details": apis,
    }
    print(json.dumps(result, indent=2, ensure_ascii=False))

    if violations:
        print(f"\nERROR: {len(violations)} deprecation(s) with period < {MINOR_PERIOD} minor versions", file=sys.stderr)
        sys.exit(1)
    else:
        print(f"\nOK: all {len(apis)} deprecation(s) satisfy period >= {MINOR_PERIOD} minor versions")


if __name__ == "__main__":
    main()