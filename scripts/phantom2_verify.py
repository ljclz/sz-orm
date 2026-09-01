#!/usr/bin/env python3
"""PHANTOM-2 编译/测试验证脚本 — 对候选 A 逐个验证。

用法：
    python scripts/phantom2_verify.py --workspace-root . --preliminary phantom2-preliminary.json --output phantom2-verified.json
"""
import argparse
import json
import subprocess
import sys
import os
from pathlib import Path


def run_cargo(pkg: str, features: str, mode: str, timeout: int = 120) -> dict:
    """执行 cargo check 或 cargo test。"""
    if mode == "check":
        cmd = ["cargo", "check", "-p", pkg, "--features", features, "-j", "2"]
    elif mode == "test":
        cmd = ["cargo", "test", "-p", pkg, "--features", features, "-j", "2", "--no-fail-fast"]
    else:
        return {"exit_code": -1, "error": f"unknown mode: {mode}"}

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=os.getcwd(),
        )
        return {
            "exit_code": result.returncode,
            "stdout_tail": result.stdout[-500:] if result.stdout else "",
            "stderr_tail": result.stderr[-500:] if result.stderr else "",
        }
    except subprocess.TimeoutExpired:
        return {"exit_code": -1, "error": f"timeout after {timeout}s"}
    except Exception as e:
        return {"exit_code": -1, "error": str(e)}


def main():
    parser = argparse.ArgumentParser(description="PHANTOM-2 编译/测试验证")
    parser.add_argument("--workspace-root", default=".", help="工作空间根路径")
    parser.add_argument("--preliminary", default="phantom2-preliminary.json", help="初步决策 JSON")
    parser.add_argument("--output", default="phantom2-verified.json", help="输出 JSON")
    parser.add_argument("--skip-test", action="store_true", help="跳过测试验证")
    args = parser.parse_args()

    with open(args.preliminary, "r", encoding="utf-8") as f:
        data = json.load(f)

    a_gates = [g for g in data["gates"] if g["decision"] == "A"]
    print(f"候选 A：{len(a_gates)} 个，开始编译验证...")

    results = []
    for i, gate in enumerate(a_gates, 1):
        pkg = gate["package"]
        feat = gate["feature"]
        print(f"[{i}/{len(a_gates)}] cargo check -p {pkg} --features {feat} ...", end=" ", flush=True)

        check_result = run_cargo(pkg, feat, "check", timeout=120)
        if check_result["exit_code"] != 0:
            print(f"FAIL (exit={check_result['exit_code']})")
            gate_copy = dict(gate)
            gate_copy["final_decision"] = "B"
            gate_copy["check_result"] = check_result
            gate_copy["downgrade_reason"] = "编译失败"
            results.append(gate_copy)
            continue

        print("OK", end="")

        if not args.skip_test:
            print(" → test ...", end=" ", flush=True)
            test_result = run_cargo(pkg, feat, "test", timeout=180)
            if test_result["exit_code"] != 0:
                print(f"FAIL (exit={test_result['exit_code']})")
                gate_copy = dict(gate)
                gate_copy["final_decision"] = "B"
                gate_copy["check_result"] = check_result
                gate_copy["test_result"] = test_result
                gate_copy["downgrade_reason"] = "测试失败"
                results.append(gate_copy)
                continue
            print("OK")
            gate_copy = dict(gate)
            gate_copy["final_decision"] = "A"
            gate_copy["check_result"] = check_result
            gate_copy["test_result"] = test_result
            results.append(gate_copy)
        else:
            print()
            gate_copy = dict(gate)
            gate_copy["final_decision"] = "A"
            gate_copy["check_result"] = check_result
            results.append(gate_copy)

    b_gates = [g for g in data["gates"] if g["decision"] == "B"]
    for g in b_gates:
        gate_copy = dict(g)
        gate_copy["final_decision"] = "B"
        results.append(gate_copy)

    final_a = sum(1 for r in results if r["final_decision"] == "A")
    final_b = sum(1 for r in results if r["final_decision"] == "B")
    final_c = sum(1 for r in results if r["final_decision"] == "C")

    output = {
        "total": len(results),
        "summary": {
            "final_A": final_a,
            "final_B": final_b,
            "final_C": final_c,
        },
        "gates": results,
    }

    Path(args.output).write_text(json.dumps(output, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"\n验证完成：A={final_a}, B={final_b}, C={final_c}")
    print(f"输出文件：{args.output}")


if __name__ == "__main__":
    main()