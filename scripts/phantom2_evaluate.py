#!/usr/bin/env python3
"""PHANTOM-2 Feature Gate 评估脚本 — 扫描、分类、初步决策。

用法：
    python scripts/phantom2_evaluate.py --workspace-root . --output phantom2-preliminary.json
"""
import argparse
import json
import re
import os
from pathlib import Path
from collections import defaultdict

# ── 分类规则 ──
CATEGORY_RULES = {
    "性能优化": {"perf-box-str", "perf-enum-dispatch", "perf-smallstring", "perf-zero-copy-l2", "performance", "auto-prewarm", "cache-coherence", "benchmark-suite"},
    "生产调优": {"prod-pool-tuning", "prod-n1-tuning", "prod-rate-limit-tuning", "prod-redis-tls", "prod-circuit-tuning", "prod-config-masking", "prod-health-endpoint", "prod-jwt-key-rotation", "prod-leak-detection", "prod-log-level", "prod-metrics-acl", "prod-probe-endpoint", "prod-shutdown-timeout", "prod-dialect-security"},
    "方言扩展": {"dialect-cockroachdb", "dialect-firebird", "dialect-informix", "dialect-redshift", "dialect-saphana", "dialect-snowflake", "dialect-yugabytedb", "dialect-saphana-driver"},
    "安全测试": {"owasp-pentest-suite"},
    "AI": {"ai-auto-tuning", "ai-nl2sql-enhanced", "llm-optimizer", "multi-llm", "plan-cache", "real"},
    "队列": {"activemq", "all-real", "all-real-no-native", "kafka", "message-tracing", "nats", "pulsar", "rabbitmq"},
    "WASM": {"js", "persistence", "wasi-socket", "wasm-real-db"},
    "真实驱动": {"real-es", "real-meilisearch", "real-opensearch", "real-postgis", "real-timescale", "real-pg", "real-consul", "real-nacos", "real-broker", "neo4j-driver", "neo4j", "s3-sdk", "real-db", "real-cloud"},
    "测试基础设施": {"testing", "db-verify", "e2e-real-db", "sql-verify-proc"},
}

# 固定决策 B 的分类
FIXED_B_CATEGORIES = {"安全测试", "AI", "队列", "WASM", "真实驱动", "测试基础设施"}

# CLI 包名（所有 CLI feature 固定 B）
CLI_PACKAGES = {"sz-orm-cli"}


def parse_cargo_toml_features(cargo_path: Path) -> dict:
    """解析 Cargo.toml 的 [features] 段，返回 {feature_name: deps_list}。"""
    content = cargo_path.read_text(encoding="utf-8", errors="replace")
    features = {}

    in_features = False
    for line in content.splitlines():
        stripped = line.strip()
        if stripped.startswith("[features]"):
            in_features = True
            continue
        if in_features:
            if stripped.startswith("[") and not stripped.startswith("[features"):
                break
            if not stripped or stripped.startswith("#"):
                continue
            m = re.match(r'^(\S+)\s*=\s*\[(.*)\]', stripped)
            if m:
                feat_name = m.group(1).strip().strip('"')
                deps_str = m.group(2).strip()
                deps = [d.strip().strip('"') for d in deps_str.split(",") if d.strip()] if deps_str else []
                features[feat_name] = deps
    return features


def get_default_features(features: dict) -> set:
    """获取 default 数组中的 feature 名。"""
    return set(features.get("default", []))


def classify_feature(pkg: str, feat: str, deps: list) -> str:
    """分类 feature gate。"""
    for cat, feats in CATEGORY_RULES.items():
        if feat in feats:
            return cat
    if pkg in CLI_PACKAGES:
        return "CLI"
    if "real" in feat.lower() or "neo4j" in feat.lower() or "s3" in feat.lower():
        return "真实驱动"
    if "prod-" in feat:
        return "生产调优"
    if "dialect-" in feat:
        return "方言扩展"
    if "perf-" in feat:
        return "性能优化"
    return "功能扩展"


def is_empty_gate(deps: list) -> bool:
    """空门控：feature = []"""
    return len(deps) == 0


def has_dep引入(deps: list) -> bool:
    """依赖引入：feature = ["dep:xxx"]"""
    return any(d.startswith("dep:") for d in deps)


def is_cross_package(deps: list) -> bool:
    """跨包转发：feature = ["sz-orm-X/feature"]"""
    return any("/" in d and not d.startswith("dep:") for d in deps)


def make_preliminary_decision(pkg: str, feat: str, deps: list, category: str) -> tuple:
    """做出初步决策 (A/B/C, 依据)。"""
    if pkg in CLI_PACKAGES:
        return "B", f"CLI 转发类，编译时按需启用"
    if category in FIXED_B_CATEGORIES:
        reasons = {
            "安全测试": "渗透测试代码不应进入生产默认构建",
            "AI": "需 LLM API key 等运行时凭证",
            "队列": "需外部 broker + 部分需原生库",
            "WASM": "需 wasm32 目标环境",
            "真实驱动": "需外部服务运行时",
            "测试基础设施": "测试用 feature，不应进入生产默认构建",
        }
        return "B", reasons.get(category, "固定决策 B")
    if category in ("性能优化", "生产调优", "方言扩展"):
        if is_empty_gate(deps):
            return "A", f"空门控，{category}类，可安全启用"
        if has_dep引入(deps):
            return "B", f"{category}类但引入外部依赖"
        return "A", f"{category}类，候选默认启用"
    return "B", f"{category}类，需逐案研判，暂定 B"


def scan_workspace(workspace_root: Path) -> list:
    """扫描工作空间所有包的 Cargo.toml。"""
    results = []
    packages_dir = workspace_root / "packages"
    if not packages_dir.exists():
        return results
    for cargo_path in sorted(packages_dir.glob("*/Cargo.toml")):
        pkg_name = cargo_path.parent.name
        features = parse_cargo_toml_features(cargo_path)
        if not features:
            continue
        default_feats = get_default_features(features)
        for feat_name, deps in features.items():
            if feat_name == "default":
                continue
            if feat_name in default_feats:
                continue
            category = classify_feature(pkg_name, feat_name, deps)
            decision, reason = make_preliminary_decision(pkg_name, feat_name, deps, category)
            results.append({
                "package": pkg_name,
                "feature": feat_name,
                "category": category,
                "decision": decision,
                "reason": reason,
                "deps": deps,
                "is_empty_gate": is_empty_gate(deps),
                "has_dep_intro": has_dep引入(deps),
                "is_cross_package": is_cross_package(deps),
                "cargo_path": str(cargo_path),
            })
    return results


def main():
    parser = argparse.ArgumentParser(description="PHANTOM-2 Feature Gate 评估")
    parser.add_argument("--workspace-root", default=".", help="工作空间根路径")
    parser.add_argument("--output", default="phantom2-preliminary.json", help="输出 JSON 路径")
    args = parser.parse_args()

    workspace_root = Path(args.workspace_root).resolve()
    results = scan_workspace(workspace_root)

    by_decision = defaultdict(int)
    by_category = defaultdict(int)
    for r in results:
        by_decision[r["decision"]] += 1
        by_category[r["category"]] += 1

    output = {
        "total": len(results),
        "summary": {
            "by_decision": dict(by_decision),
            "by_category": dict(by_category),
        },
        "gates": results,
    }

    output_path = Path(args.output)
    output_path.write_text(json.dumps(output, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"评估完成：{len(results)} 个 feature gate")
    print(f"决策分布：A={by_decision['A']}, B={by_decision['B']}, C={by_decision.get('C', 0)}")
    print(f"分类分布：{dict(by_category)}")
    print(f"输出文件：{output_path}")


if __name__ == "__main__":
    main()