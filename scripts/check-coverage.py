#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-coverage.py — 覆盖率门禁（AGENTS.md 门禁 22）
================================================================
解决"测试数量 ≠ 测试质量"（szrsql 审查根因 2/阶段 3）：
8,927 个 #[test] 标注无覆盖率度量——本脚本用 cargo-llvm-cov 统计
关键模块的行覆盖率，低于阈值即失败。

用法:
  python scripts/check-coverage.py                        # 默认关键模块子集
  python scripts/check-coverage.py --package sz-orm-core  # 指定包
  python scripts/check-coverage.py --threshold 0.6        # 自定义阈值（默认 60%）
  python scripts/check-coverage.py --features "tenant-quota-rls-enhanced"

退出码: 0 = 覆盖率达标；1 = 低于阈值或运行失败
"""

import argparse
import json
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# 内置关键模块子集（v8.2.0 扩展：4→70 个，含 v8.0.0 新增 33 + v8.1.0 新增 33）
# 语义敏感/变更高频，覆盖率优先保障
DEFAULT_MODULES = [
    # 既有 4 个核心模块
    "packages/sz-orm-core/src/tenant_quota_rls.rs",
    "packages/sz-orm-core/src/cache_warmup_protection.rs",
    "packages/sz-orm-core/src/bloom.rs",
    "packages/sz-orm-core/src/process_l1_cache.rs",
    # v8.0.0 新增 33 个组件
    "packages/sz-orm-core/src/perf_extreme/zero_copy_acquire.rs",
    "packages/sz-orm-core/src/perf_extreme/batch_acquire.rs",
    "packages/sz-orm-core/src/perf_extreme/simd_full_pipeline.rs",
    "packages/sz-orm-core/src/perf_extreme/zero_copy_deserializer.rs",
    "packages/sz-orm-core/src/perf_extreme/benchmark_comparator.rs",
    "packages/sz-orm-core/src/perf_extreme/regression_detector.rs",
    "packages/sz-orm-ai/src/ai_deep/equivalence_verifier.rs",
    "packages/sz-orm-ai/src/ai_deep/nl_rewrite_pipeline.rs",
    "packages/sz-orm-ai/src/ai_deep/schema_design_advisor.rs",
    "packages/sz-orm-ai/src/ai_deep/anomaly_prediction_engine.rs",
    "packages/sz-orm-fusion/src/bi_sync/hlc_clock.rs",
    "packages/sz-orm-fusion/src/bi_sync/bi_sync_coordinator.rs",
    "packages/sz-orm-dtx/src/saga_coordinator/coordinator.rs",
    "packages/sz-orm-core/src/cache_coherent/strong_consistency_cache.rs",
    "packages/sz-orm-crypto/src/tde_mgmt/dek_rotation_manager.rs",
    "packages/sz-orm-crypto/src/tde_mgmt/kms_ha_manager.rs",
    "packages/sz-orm-crypto/src/tde_mgmt/column_policy_hot_updater.rs",
    "packages/sz-orm-audit/src/evidence_chain/evidence_exporter.rs",
    "packages/sz-orm-masking/src/policy_engine/masking_policy_engine.rs",
    "packages/sz-orm-auth/src/abac_prod/abac_production_engine.rs",
    "packages/sz-orm-python/src/async_stream/async_stream_binding.rs",
    "packages/sz-orm-lsp/src/enhanced/lsp_enhanced_completion.rs",
    "packages/sz-orm-wasm/src/sidecar/k8s_sidecar_adapter.rs",
    "packages/sz-orm-core/src/plugin_marketplace/plugin_sandbox.rs",
    "packages/sz-orm-core/src/plugin_marketplace/plugin_marketplace_verifier.rs",
    "packages/sz-orm-governance/src/lifecycle/cross_storage/pyramid.rs",
    "packages/sz-orm-governance/src/lifecycle/cross_storage/cost_simulator.rs",
    "packages/sz-orm-governance/src/lifecycle/cross_storage/evidence_chain.rs",
    "packages/sz-orm-governance/src/lifecycle/cross_storage/federated_query.rs",
    "packages/sz-orm-mig/src/safety_net/freeze_window.rs",
    "packages/sz-orm-mig/src/safety_net/impact_analyzer.rs",
    "packages/sz-orm-mig/src/safety_net/shadow_verifier.rs",
    "packages/sz-orm-mig/src/safety_net/rollback_sandbox.rs",
    # v8.1.0 新增 33 个组件
    "packages/sz-orm-bench/src/prod_db_guard.rs",
    "packages/sz-orm-bench/src/repeatability_guard.rs",
    "packages/sz-orm-bench/src/result_exporter.rs",
    "packages/sz-orm-bench/src/ci_integration.rs",
    "packages/sz-orm-observability/src/perf_budget_alert.rs",
    "packages/sz-orm-ai/src/autonomous/closed_loop_scheduler.rs",
    "packages/sz-orm-ai/src/autonomous/takeover.rs",
    "packages/sz-orm-ai/src/autonomous/loop_break_detector.rs",
    "packages/sz-orm-ai/src/autonomous/xai/ab_significance.rs",
    "packages/sz-orm-ai/src/llm_provider/version_registry.rs",
    "packages/sz-orm-ai/src/llm_provider/version_canary.rs",
    "packages/sz-orm-dtx/src/coordination/raft_optimize.rs",
    "packages/sz-orm-dtx/src/coordination/split_brain_detector.rs",
    "packages/sz-orm-dtx/src/consistency_level_config.rs",
    "packages/sz-orm-fusion/src/split_brain_recovery.rs",
    "packages/sz-orm-fusion/src/cross_region_replicate_config.rs",
    "packages/sz-orm-audit/src/compliance_scan_engine.rs",
    "packages/sz-orm-audit/src/compliance_scan_desensitizer.rs",
    "packages/sz-orm-audit/src/evidence_auto_archive_scheduler.rs",
    "packages/sz-orm-audit/src/compliance_report_auto_generator.rs",
    "packages/sz-orm-crypto/src/tde_mgmt/key_auto_rotate_scheduler.rs",
    "packages/sz-orm-core/src/plugin_marketplace/marketplace_ops.rs",
    "packages/sz-orm-core/src/plugin_marketplace/billing_engine.rs",
    "packages/sz-orm-core/src/sdk_auto_gen.rs",
    "packages/sz-orm-wasm/src/sidecar/template_generator.rs",
    "packages/sz-orm-studio/src/developer_portal.rs",
    "packages/sz-orm-tracing/src/end_to_end.rs",
    "packages/sz-orm-observability/src/unified_collector.rs",
    "packages/sz-orm-logger/src/aggregator.rs",
    "packages/sz-orm-observability/src/alert_rule_engine.rs",
    "packages/sz-orm-observability/src/alert_storm_suppressor.rs",
    "packages/sz-orm-observability/src/grafana_dashboard_exporter.rs",
    "packages/sz-orm-observability/src/self_health.rs",
]
# v8.2.0 扩展：复用 v8.0.0 既有 22 + v8.1.0 既有 29 + 4 核心 feature gate
# 注意：全量 feature 组合在 Windows 下可能触发 rustc 栈溢出（90+ features），
# 建议分批测量（ADR-002），按 --package + --features 指定子集
DEFAULT_FEATURES = "tenant-quota-rls-enhanced,auto-prewarm,l1-cache,multi-tenant-enhanced,perf-extreme,pool-zero-copy,query-simd,serde-zero-copy,dist-cache-coherent,plugin-marketplace,ai-deep,ai-nl-rewrite,ai-schema-design,ai-anomaly-predict,dist-enhance,dtx-saga-coordinator,dist-sync-bi,sec-compliance,tde-key-mgmt,audit-evidence-chain,masking-policy-engine,auth-abac,eco-extend,binding-async-stream,toolchain-lsp-enhance,cloudnative-sidecar,perf-bench-real,bench-real-db,perf-regression-ci,perf-budget-alert,ai-autonomous,ai-closed-loop,ai-ab-testing,ai-model-versioning,dist-consensus,raft-optimize,split-brain-detect,consistency-tunable,cross-region-replicate,sec-auto,compliance-auto-scan,evidence-auto-archive,compliance-report-auto,key-auto-rotate,eco-deep,plugin-marketplace-ops,sdk-auto-gen,cloudnative-template,developer-portal,observability,dist-tracing,metrics-collect,log-aggregate,alert-rules,dashboard-export"


def find_cargo_bin(name):
    exe = shutil.which(name)
    if exe:
        return exe
    home = os.path.expanduser("~")
    for cand in (os.path.join(home, ".cargo", "bin", name),
                 os.path.join(home, ".cargo", "bin", name + ".exe")):
        if os.path.isfile(cand):
            return cand
    return name


def get_crate_features(crate_name):
    """从 Cargo.toml 读取 crate 的 feature gate 列表（排除 default）。"""
    cargo_toml = os.path.join(ROOT, "packages", crate_name, "Cargo.toml")
    if not os.path.isfile(cargo_toml):
        return []
    import re
    with open(cargo_toml, "r", encoding="utf-8") as f:
        content = f.read()
    match = re.search(r'\[features\](.*?)(?=\n\[|\Z)', content, re.DOTALL)
    if not match:
        return []
    result = []
    for line in match.group(1).strip().split("\n"):
        line = line.strip()
        if line and "=" in line:
            name = line.split("=")[0].strip()
            if name != "default":
                result.append(name)
    return result


# DEFAULT_FEATURES 中各 crate 的 feature 映射（手动维护，v8.2.0）
# 按 crate 分组，避免对 sz-orm-core 启用其他 crate 的 feature 导致编译失败
CRATE_FEATURE_MAP = {
    "sz-orm-core": "tenant-quota-rls-enhanced,auto-prewarm,l1-cache,multi-tenant-enhanced,perf-extreme,pool-zero-copy,query-simd,serde-zero-copy,dist-cache-coherent,plugin-marketplace,plugin-marketplace-ops,sdk-auto-gen,eco-deep,perf-accel",
    "sz-orm-ai": "ai-deep,ai-nl-rewrite,ai-schema-design,ai-anomaly-predict,ai-autonomous,ai-closed-loop,ai-ab-testing,ai-model-versioning",
    "sz-orm-fusion": "dist-sync-bi,split-brain-detect,cross-region-replicate",
    "sz-orm-dtx": "dtx-saga-coordinator,dist-consensus,raft-optimize,split-brain-detect,consistency-tunable",
    "sz-orm-crypto": "sec-compliance,tde-key-mgmt,key-auto-rotate",
    "sz-orm-audit": "audit-evidence-chain,compliance-auto-scan,evidence-auto-archive,compliance-report-auto",
    "sz-orm-masking": "masking-policy-engine",
    "sz-orm-auth": "auth-abac",
    "sz-orm-python": "binding-async-stream",
    "sz-orm-lsp": "toolchain-lsp-enhance",
    "sz-orm-wasm": "cloudnative-sidecar,cloudnative-template",
    "sz-orm-governance": "governance,cross-storage-lifecycle,federated-query,cost-governance",
    "sz-orm-mig": "zero-downtime-mig,evolution-safety-net,shadow-traffic-verify,rollback-sandbox",
    "sz-orm-bench": "perf-bench-real,bench-real-db,perf-regression-ci,perf-budget-alert",
    "sz-orm-observability": "observability,metrics-collect,alert-rules,dashboard-export,perf-budget-alert",
    "sz-orm-tracing": "dist-tracing",
    "sz-orm-logger": "log-aggregate",
    "sz-orm-studio": "developer-portal",
}


def main():
    ap = argparse.ArgumentParser(description="覆盖率门禁（门禁 22）")
    ap.add_argument("--package", default=None, help="目标包（默认按 crate 自动分组）")
    ap.add_argument("--modules", action="append", default=None, help="目标文件（可多次），默认内置子集")
    ap.add_argument("--features", default="", help="cargo features（默认按 crate 自动匹配）")
    ap.add_argument("--threshold", type=float, default=0.8, help="行覆盖率阈值（默认 80%%）")
    ap.add_argument("--branch-threshold", type=float, default=0.7, help="分支覆盖率阈值（默认 70%%）")
    args = ap.parse_args()

    modules = args.modules or DEFAULT_MODULES
    print("=" * 60)
    print("  覆盖率门禁（门禁 22）")
    print("=" * 60)

    # 按 crate 分组模块（从路径 packages/<crate>/src/... 提取 crate 名）
    crate_modules = {}
    for m in modules:
        parts = m.replace("\\", "/").split("/")
        if len(parts) >= 2 and parts[0] == "packages":
            crate = parts[1]
            crate_modules.setdefault(crate, []).append(m)

    # 如果指定了 --package，只运行该 crate
    if args.package:
        crate_modules = {k: v for k, v in crate_modules.items() if k == args.package}

    llvm_cov = find_cargo_bin("cargo-llvm-cov")
    all_missing = []
    total_covered = 0
    total_regions = 0
    total_branches = 0
    total_branches_covered = 0

    for crate, crate_mods in sorted(crate_modules.items()):
        features = args.features or CRATE_FEATURE_MAP.get(crate, "")
        # Windows 下禁用自动生成的超长 --ignore-filename-regex（72 包 workspace 路径
        # 使 llvm-cov 子进程命令行超过 CreateProcess 32767 字符限制触发 os error 206，
        # 见 2026-10-05 G22 审计修复）。
        cmd = [llvm_cov, "llvm-cov", "--package", crate, "--json", "--quiet", "--no-default-ignore-filename-regex"]
        if features:
            cmd += ["--features", features]
        print(f"\n  [{crate}] {len(crate_mods)} 模块, features: {features[:80]}...")
        print("  $ " + " ".join(cmd[:6]) + " ...")
        proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=1800)
        if proc.returncode != 0:
            print(f"  ❌ [{crate}] cargo-llvm-cov 运行失败")
            print("  " + (proc.stderr[-800:] or proc.stdout[-800:]))
            continue
        try:
            data = json.loads(proc.stdout)
        except json.JSONDecodeError:
            print(f"  ❌ [{crate}] 无法解析覆盖率 JSON")
            continue

        files = []
        for entry in data.get("data", [data]):
            files.extend(entry.get("files", []))

        # 按完整相对路径匹配目标模块（避免跨 crate 同名文件误统计，如
        # fusion 的 bi_sync_coordinator.rs 在 sz-orm-dtx 运行中被后缀误匹配）。
        # Windows 下 --no-default-ignore-filename-regex 使 JSON 包含所有编译单元，
        # 精确匹配尤为重要（2026-10-05 G22 审计修复）。
        for f in files:
            path = f.get("filename", "").replace("\\", "/")
            if not any(path.endswith(m) for m in crate_mods):
                continue
            summary = f.get("summary", {}).get("lines", {})
            covered = summary.get("covered", 0)
            regions = summary.get("count", 0)
            total_covered += covered
            total_regions += regions
            branch_summary = f.get("summary", {}).get("branches", {})
            branches_covered = branch_summary.get("covered", 0)
            branches_total = branch_summary.get("count", 0)
            total_branches_covered += branches_covered
            total_branches += branches_total
            if regions > 0:
                pct = covered / regions
                branch_pct = (branches_covered / branches_total) if branches_total > 0 else 1.0
                all_missing.append((path.split("/")[-1], pct, covered, regions, branch_pct, branches_covered, branches_total, crate))

    if total_regions == 0:
        print("\n  ❌ 目标模块无覆盖率数据（模块未编译或路径不匹配——请检查 --features / --modules）")
        return 1

    rate = total_covered / total_regions
    branch_rate = (total_branches_covered / total_branches) if total_branches > 0 else 1.0
    print("\n  模块行覆盖率 / 分支覆盖率:")
    for name, pct, cov, reg, bpct, bcov, btot, crate in sorted(all_missing):
        flag = "✅" if pct >= args.threshold and bpct >= args.branch_threshold else "❌"
        print(f"    {flag} {crate}/{name:36s} 行 {pct:.1%} ({cov}/{reg})  分支 {bpct:.1%} ({bcov}/{btot})")
    print(f"\n  合计: 行 {rate:.1%}（阈值 {args.threshold:.0%}）  分支 {branch_rate:.1%}（阈值 {args.branch_threshold:.0%}）")
    print("\n" + "=" * 60)
    try:
        if rate < args.threshold:
            print(f"❌ 门禁 22 未通过 — 行覆盖率 {rate:.1%} < {args.threshold:.0%}（请补测试）")
            return 1
        if branch_rate < args.branch_threshold:
            print(f"❌ 门禁 22 未通过 — 分支覆盖率 {branch_rate:.1%} < {args.branch_threshold:.0%}（请补测试）")
            return 1
        print(f"✅ 门禁 22 通过 — 行 {rate:.1%} ≥ {args.threshold:.0%}  分支 {branch_rate:.1%} ≥ {args.branch_threshold:.0%}")
        return 0
    finally:
        cov_dir = os.path.join(ROOT, "target", "llvm-cov-target")
        if os.path.isdir(cov_dir):
            shutil.rmtree(cov_dir, ignore_errors=True)
            print("  🧹 已清理 target/llvm-cov-target/")


if __name__ == "__main__":
    sys.exit(main())
