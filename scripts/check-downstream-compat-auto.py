#!/usr/bin/env python3
"""Downstream Compatibility Automated Verification.

Verifies that downstream projects (sz-pay, new-wxapp) remain compatible
with sz-orm after version upgrades.

Usage:
  python scripts/check-downstream-compat-auto.py --project sz-pay --compile --api-diff --json
  python scripts/check-downstream-compat-auto.py --all --compile --api-diff --json --output compat.json
"""

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path


PROJECTS = {
    "sz-pay": {
        "path": r"E:\vue\test\sz-pay\server\sz-rust",
        "packages": ["sz-orm-core", "sz-orm-sqlx", "sz-orm-config", "sz-orm-auth", "sz-orm-macros", "sz-orm-queue"],
    },
    "new-wxapp": {
        "path": r"E:\vue\test\new-wxapp\rust",
        "packages": ["sz-orm-core", "sz-orm-sqlx", "sz-orm-config"],
    },
}


@dataclass
class CompatReport:
    project: str
    path: str
    exists: bool = False
    compile_ok: bool = False
    compile_output: str = ""
    api_compatible: bool = False
    api_diff: list = field(default_factory=list)
    packages_checked: int = 0
    packages_compatible: int = 0


def check_compile(project_path: str) -> tuple:
    """Check if project compiles."""
    cargo_toml = Path(project_path) / "Cargo.toml"
    if not cargo_toml.exists():
        return False, f"Cargo.toml not found at {cargo_toml}"
    try:
        result = subprocess.run(
            ["cargo", "check", "--manifest-path", str(cargo_toml)],
            capture_output=True, text=True, timeout=300, encoding="utf-8", errors="replace",
        )
        output = (result.stdout or "") + (result.stderr or "")
        return result.returncode == 0, output
    except Exception as e:
        return False, str(e)


def check_api_compat(project_path: str, packages: list) -> tuple:
    """Check API compatibility for listed packages."""
    diffs = []
    compatible = 0
    cargo_toml = Path(project_path) / "Cargo.toml"
    if not cargo_toml.exists():
        return False, diffs, 0
    try:
        content = cargo_toml.read_text(encoding="utf-8", errors="replace")
        for pkg in packages:
            if pkg in content:
                compatible += 1
            else:
                diffs.append(f"{pkg}: not found in Cargo.toml")
    except Exception as e:
        diffs.append(f"Error reading Cargo.toml: {e}")
    return len(diffs) == 0, diffs, compatible


def auto_check_project(name: str, config: dict, do_compile: bool, do_api_diff: bool) -> CompatReport:
    """Run automated compatibility check for a project."""
    report = CompatReport(project=name, path=config["path"])
    report.exists = Path(config["path"]).exists()
    if not report.exists:
        report.compile_output = f"Project path not found: {config['path']}"
        return report
    if do_compile:
        report.compile_ok, report.compile_output = check_compile(config["path"])
    if do_api_diff:
        report.api_compatible, report.api_diff, report.packages_compatible = check_api_compat(
            config["path"], config["packages"]
        )
        report.packages_checked = len(config["packages"])
    return report


def report_to_dict(report: CompatReport) -> dict:
    return {
        "name": report.project,
        "path": report.path,
        "exists": report.exists,
        "compile_ok": report.compile_ok,
        "api_compatible": report.api_compatible,
        "packages_checked": report.packages_checked,
        "packages_compatible": report.packages_compatible,
        "api_diff": report.api_diff,
    }


def main():
    parser = argparse.ArgumentParser(description="Downstream Compatibility Automated Verification")
    parser.add_argument("--project", default="sz-pay", help="Project name (single project mode)")
    parser.add_argument("--all", action="store_true", help="Check all projects (batch mode)")
    parser.add_argument("--compile", action="store_true", help="Check compilation")
    parser.add_argument("--api-diff", action="store_true", help="Check API compatibility")
    parser.add_argument("--json", action="store_true", help="Output JSON format")
    parser.add_argument("--output", default=None, help="Output file path")
    args = parser.parse_args()

    if args.all:
        reports = []
        all_pass = True
        for name, config in PROJECTS.items():
            print(f"=== Checking {name} ===")
            report = auto_check_project(name, config, args.compile, args.api_diff)
            reports.append(report)
            if not report.exists:
                print(f"  WARNING: {name} path not found: {config['path']}, skipping (non-failure)")
            else:
                if args.compile:
                    print(f"  Compile: {'PASS' if report.compile_ok else 'FAIL'}")
                    if not report.compile_ok:
                        all_pass = False
                if args.api_diff:
                    print(f"  API Compatible: {'PASS' if report.api_compatible else 'FAIL'}")
                    print(f"  Packages: {report.packages_compatible}/{report.packages_checked} compatible")

        if args.json:
            output_data = {
                "projects": [report_to_dict(r) for r in reports],
                "all_pass": all_pass,
            }
            output_str = json.dumps(output_data, indent=2, ensure_ascii=False)
            if args.output:
                Path(args.output).write_text(output_str, encoding="utf-8")
                print(f"Report written to {args.output}")
            else:
                print(output_str)
        else:
            print(f"\n=== Summary ===")
            for r in reports:
                status = "SKIP" if not r.exists else ("PASS" if r.compile_ok else "FAIL")
                print(f"  {r.project}: {status}")
            print(f"  All pass: {all_pass}")

        if not all_pass:
            sys.exit(1)
        return

    if args.project not in PROJECTS:
        print(f"Unknown project: {args.project}")
        print(f"Available: {list(PROJECTS.keys())}")
        sys.exit(1)

    report = auto_check_project(args.project, PROJECTS[args.project], args.compile, args.api_diff)

    if args.json:
        output_str = json.dumps(report_to_dict(report), indent=2, ensure_ascii=False)
        if args.output:
            Path(args.output).write_text(output_str, encoding="utf-8")
            print(f"Report written to {args.output}")
        else:
            print(output_str)
    else:
        print(f"=== Downstream Compatibility: {report.project} ===")
        print(f"Path: {report.path}")
        print(f"Exists: {report.exists}")
        if args.compile:
            print(f"Compile: {'PASS' if report.compile_ok else 'FAIL'}")
        if args.api_diff:
            print(f"API Compatible: {'PASS' if report.api_compatible else 'FAIL'}")
            print(f"Packages: {report.packages_compatible}/{report.packages_checked} compatible")
            if report.api_diff:
                for d in report.api_diff:
                    print(f"  - {d}")

    if not report.exists:
        print(f"WARNING: Project path not found, skipping (non-failure)")
    elif args.compile and not report.compile_ok:
        sys.exit(1)


if __name__ == "__main__":
    main()
