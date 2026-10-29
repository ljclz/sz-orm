#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-phantom-delivery.py — 幻影交付扫描（AGENTS.md 门禁 15）
================================================================
幻影交付 = 文档宣称"已实现/已生效"（自动/强制/默认/集成语义），但生产路径零调用
或 feature gate 无任何启用点（默认构建不编译）。

两种幻影：
  PHANTOM-1 零调用符号  : 符号在定义文件之外、非测试/示例/benches 的 src 生产代码中
                         无任何调用方（排除 lib.rs 导出声明与注释）
  PHANTOM-2 门控未启用  : feature gate 定义存在，但无任何工作空间成员启用
                         （不在 default，也不在任何依赖声明的 features=[] 中）
                         可通过 PHANTOM2_EXEMPT 登记豁免（P2-2 2026-10-28）：
                         面向最终用户/测试的按需开关由下游在自身 Cargo.toml 启用，
                         workspace 成员不启用属正常形态；未登记的仍按警告（--strict 失败）。

用法:
  python scripts/check-phantom-delivery.py                # 全量扫描（内置宣称符号表）
  python scripts/check-phantom-delivery.py --symbols A B  # 自定义符号表
  python scripts/check-phantom-delivery.py --strict       # 未登记的 PHANTOM-2 也按失败计（exit 1）
  python scripts/check-phantom-delivery.py --skip-matrix  # 只查零调用符号
  python scripts/check-phantom-delivery.py --show-exempt  # 逐条列出已登记豁免的 feature

退出码: 0 = 无 PHANTOM-1（PHANTOM-2 未登记警告默认仅提示）
        1 = 存在 PHANTOM-1，或 --strict 下存在未登记的 PHANTOM-2

依据: docs/assessment/2026-08-13-production-zero-call-audit.md（审计方法论固化）
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGES = os.path.join(ROOT, "packages")
CLI = os.path.join(ROOT, "cli", "Cargo.toml")
EXAMPLES = os.path.join(ROOT, "examples", "Cargo.toml")

# 内置宣称符号表：源自 2026-08-13 生产零调用审计报告 §二/§三
# （v4.6.0/v4.7.0 新模块 + 核心宣称组件；新增 feature 交付时应同步补充）
DECLARED_SYMBOLS = [
    # v4.7.0（delayed-priority-queue / forward-compat-sandbox / copy-parallel-shard /
    #         anomaly-remediation-rca / multicloud-cost-forecast / tenant-quota-rls-enhanced /
    #         cache-warmup-protection）
    "DelayScheduler", "PriorityQueue", "ScheduledMessage", "DelayedMessage",
    "ForwardCompatChecker", "SandboxDryRunner", "MigrationDependencyGraph",
    "CopyProtocolAdapter", "ParallelShardExecutor", "ConflictResolution",
    "AutoRemediator", "RootCauseAnalyzer", "AnomalyCorrelator",
    "MultiCloudCostComparator", "CapacityForecaster", "AutoOptimizer",
    "TenantResourceQuota", "QuotaEnforcer", "RlsPolicyEnhancer", "TenantAuditLogger",
    "CacheWarmer", "BloomFilter", "PenetrationGuard", "SingleFlight",
    # v4.6.0
    "RedeliveryScheduler", "RollbackExecutor", "BatchTransactionCoordinator",
    "AnomalyDetector", "CostAnalyzer", "ConnectionTenantBinder", "ProcessL1Cache",
    # 核心宣称组件（README/AGENTS.md 宣称的集成能力）
    "N1QueryDetector", "HookRegistry", "HookDispatcher",
    "GossipInvalidationBus", "RedisPubSubInvalidationBus", "WriteBehindQueue",
    "BehaviorRegistry",
    # v4.7.0 宣称的缓存穿透/击穿防护
    "PenetrationGuard", "SingleFlight",
]

# C 类保留组件登记表（2026-08-13 生产零调用审计分类："提供 X 组件（需手动接入）"的独立库组件）：
# 零调用但按设计保留。登记条件（fail-closed，全部满足才豁免）：
#   ① 符号必须仍在 DECLARED_SYMBOLS 中（登记表过期即失败）；
#   ② 审计报告文件必须存在（出处可追溯）；
#   ③ 当前文档（README.md / 工程实践）中符号所在行不得含集成语义措辞
#      （自动注入/自动拦截/强制执行/默认生效/启动预热/自动接线/自动应用/默认启用），
#      追溯表行（| P-N |）除外——它们描述历史违规而非当前宣称。
C_CLASS_SYMBOLS = [
    "AnomalyCorrelator", "AnomalyDetector", "AutoOptimizer", "AutoRemediator",
    "BatchTransactionCoordinator", "BehaviorRegistry", "CapacityForecaster",
    "ConflictResolution", "ConnectionTenantBinder", "CopyProtocolAdapter",
    "CostAnalyzer", "DelayScheduler", "DelayedMessage", "ForwardCompatChecker",
    "GossipInvalidationBus", "HookDispatcher", "HookRegistry",
    "MigrationDependencyGraph", "MultiCloudCostComparator", "N1QueryDetector",
    "ParallelShardExecutor", "PenetrationGuard", "PriorityQueue",
    "RedeliveryScheduler", "RedisPubSubInvalidationBus", "RollbackExecutor",
    "RootCauseAnalyzer", "SandboxDryRunner", "ScheduledMessage", "SingleFlight",
    "TenantAuditLogger", "TenantResourceQuota", "WriteBehindQueue",
]

# 集成语义措辞（AGENTS.md 门禁 15 明令禁止的"宣称集成"表述）
INTEGRATION_WORDING_RE = re.compile(
    r"自动注入|自动拦截|强制执行|默认生效|启动预热|自动接线|自动应用|默认启用"
)
# 追溯表行（| P-1 | 等）：描述历史违规与整改，不代表当前文档宣称
TRACE_ROW_RE = re.compile(r"^\s*\| P-\d+ \|")
AUDIT_DOC = "docs/assessment/2026-08-13-production-zero-call-audit.md"
C_CLASS_DOCS = ["README.md", "docs/sz-orm-engineering-practices.md"]

# ============================================================================
# PHANTOM-2 豁免登记表（P2-2 2026-10-28）：feature 从未被任何 workspace 成员启用
# 的绝大多数是面向最终用户/测试的按需开关（如 sz-orm-search 的 real-es、
# sz-orm-queue 的 kafka、sz-orm-wasm 的 wasi-support），由下游在自身 Cargo.toml
# 启用——成员不启用属正常形态，不是缺陷。
# 键=feature 名，值=豁免理由（通用分类常量）。同名 feature 跨包共用一条登记
# （283 个唯一名覆盖 308 个 pkg-feature 组合，分类已核对无跨包冲突）。
# fail-closed：未登记且无成员启用的 feature 仍按 PHANTOM-2 警告（--strict 失败）。
# 新增 feature 默认必须登记豁免理由或由成员启用；注销登记直接删除对应行。
# ============================================================================
_P2_DRIVER = "用户侧驱动开关：由下游在自身 Cargo.toml 启用（real-* / dialect-* / server / client 类）"
_P2_INTEG = "集成能力开关：启用后接入外部系统（kafka/nats/pulsar/otlp/s3-sdk 等）"
_P2_PERF = "性能/实验特性：按需启用（perf-* / simd 相关）"
_P2_TEST = "测试专用：仅集成测试/示例演示启用（owasp-pentest-suite / data-seeding 等）"
_P2_AI = "AI/智能增强开关：按需启用（llm/nl2sql/agent/anomaly 等智能特性）"
_P2_GOV = "治理/合规/安全能力开关：按需启用（governance/compliance/lineage/security 等增强）"
_P2_CAP = "能力扩展开关：按需启用（由下游按场景选择，默认不编译属正常形态）"

PHANTOM2_EXEMPT: dict[str, str] = {
    # —— 用户侧驱动开关（real-* / dialect-* / server / client 类）（24）——
    "cockroachdb": _P2_DRIVER,
    "db-backend-compat": _P2_DRIVER,
    "dialect-saphana-driver": _P2_DRIVER,
    "js": _P2_DRIVER,
    "lsp-server": _P2_DRIVER,
    "mssql": _P2_DRIVER,
    "oracle": _P2_DRIVER,
    "postgis": _P2_DRIVER,
    "real": _P2_DRIVER,
    "real-broker": _P2_DRIVER,
    "real-consul": _P2_DRIVER,
    "real-es": _P2_DRIVER,
    "real-meilisearch": _P2_DRIVER,
    "real-nacos": _P2_DRIVER,
    "real-opensearch": _P2_DRIVER,
    "real-pg": _P2_DRIVER,
    "real-postgis": _P2_DRIVER,
    "real-timescale": _P2_DRIVER,
    "server": _P2_DRIVER,
    "wasi-socket": _P2_DRIVER,
    "wasi-support": _P2_DRIVER,
    "wasm-build": _P2_DRIVER,
    "wasm-real-db": _P2_DRIVER,
    "yugabytedb": _P2_DRIVER,
    # —— 集成能力开关（kafka/nats/pulsar/otlp/s3-sdk 等）（31）——
    "activemq": _P2_INTEG,
    "all-real": _P2_INTEG,
    "all-real-no-native": _P2_INTEG,
    "async-graphql-integration": _P2_INTEG,
    "cdc-mysql": _P2_INTEG,
    "cdc-postgres": _P2_INTEG,
    "cdc-sqlite": _P2_INTEG,
    "config-center": _P2_INTEG,
    "dist-cache-cluster": _P2_INTEG,
    "dist-tracing": _P2_INTEG,
    "distributed-tracing": _P2_INTEG,
    "eco-config": _P2_INTEG,
    "eco-extend": _P2_INTEG,
    "embedding-store": _P2_INTEG,
    "integration": _P2_INTEG,
    "k8s-operator": _P2_INTEG,
    "kafka": _P2_INTEG,
    "log-aggregate": _P2_INTEG,
    "mcp": _P2_INTEG,
    "model-ops-llamacpp": _P2_INTEG,
    "model-ops-vllm": _P2_INTEG,
    "nats": _P2_INTEG,
    "neo4j-driver": _P2_INTEG,
    "otlp-export": _P2_INTEG,
    "prometheus": _P2_INTEG,
    "prometheus-exporter": _P2_INTEG,
    "pulsar": _P2_INTEG,
    "push-gateway": _P2_INTEG,
    "rabbitmq": _P2_INTEG,
    "s3-sdk": _P2_INTEG,
    "service-mesh": _P2_INTEG,
    # —— 性能/实验特性（perf-* / simd 相关）（30）——
    "adaptive-pool-tuning": _P2_PERF,
    "adaptive-query": _P2_PERF,
    "binding-perf-align": _P2_PERF,
    "cache-hit-optimize": _P2_PERF,
    "dynamic-pool-tuning": _P2_PERF,
    "executor-opt": _P2_PERF,
    "explain-analyzer": _P2_PERF,
    "flamegraph": _P2_PERF,
    "io-uring": _P2_PERF,
    "parallel-query": _P2_PERF,
    "perf-baseline": _P2_PERF,
    "perf-diff": _P2_PERF,
    "perf-extreme": _P2_PERF,
    "perf-metrics": _P2_PERF,
    "perf-smallstring": _P2_PERF,
    "plan-cache": _P2_PERF,
    "pool-elastic": _P2_PERF,
    "pool-io-reuse": _P2_PERF,
    "pool-zero-copy": _P2_PERF,
    "prod-rate-limit-tuning": _P2_PERF,
    "query-flamegraph": _P2_PERF,
    "query-simd": _P2_PERF,
    "raft-optimize": _P2_PERF,
    "serde-zero-copy": _P2_PERF,
    "simd-deep": _P2_PERF,
    "slo-driven-arbitration": _P2_PERF,
    "slo-driven-scheduling": _P2_PERF,
    "slow-query-capture": _P2_PERF,
    "stream-resultset": _P2_PERF,
    "zero-copy-deep": _P2_PERF,
    # —— 测试专用（owasp-pentest-suite / data-seeding / 示例演示等）（30）——
    "ai-nl-query": _P2_TEST,
    "bench-real-db": _P2_TEST,
    "bench-seaorm": _P2_TEST,
    "benchmark-suite": _P2_TEST,
    "cdc-realtime-sync": _P2_TEST,
    "composable-plugin": _P2_TEST,
    "data-seeding": _P2_TEST,
    "db-verify": _P2_TEST,
    "dist-coordination": _P2_TEST,
    "e2e-real-db": _P2_TEST,
    "graph-traversal": _P2_TEST,
    "ha-failover": _P2_TEST,
    "multi-region": _P2_TEST,
    "olap-vectorized": _P2_TEST,
    "owasp-pentest-suite": _P2_TEST,
    "perf-bench-real": _P2_TEST,
    "perf-budget-alert": _P2_TEST,
    "perf-regression-ci": _P2_TEST,
    "query-auto-tuning": _P2_TEST,
    "rbac-abac-enhanced": _P2_TEST,
    "real-bench": _P2_TEST,
    "real-bench-diesel": _P2_TEST,
    "real-db": _P2_TEST,
    "serverless-adapt": _P2_TEST,
    "sql-verify-proc": _P2_TEST,
    "stability-matrix": _P2_TEST,
    "stream-processing": _P2_TEST,
    "sz-pay-example": _P2_TEST,
    "tde-interceptor": _P2_TEST,
    "testing": _P2_TEST,
    # —— AI/智能增强（llm/nl2sql/agent/anomaly 等）（43）——
    "agent-plan-execute": _P2_AI,
    "agent-react": _P2_AI,
    "agent-workflow": _P2_AI,
    "ai-ab-testing": _P2_AI,
    "ai-anomaly-predict": _P2_AI,
    "ai-auto-remediation": _P2_AI,
    "ai-auto-scaling": _P2_AI,
    "ai-auto-tuning": _P2_AI,
    "ai-auto-tuning-exec": _P2_AI,
    "ai-autonomous": _P2_AI,
    "ai-capability": _P2_AI,
    "ai-closed-loop": _P2_AI,
    "ai-deep": _P2_AI,
    "ai-index-apply": _P2_AI,
    "ai-index-lifecycle": _P2_AI,
    "ai-migration-gen": _P2_AI,
    "ai-model-versioning": _P2_AI,
    "ai-native-query": _P2_AI,
    "ai-nl-rewrite": _P2_AI,
    "ai-nl2sql-complex": _P2_AI,
    "ai-nl2sql-deepen": _P2_AI,
    "ai-nl2sql-enhanced": _P2_AI,
    "ai-plan-predict": _P2_AI,
    "ai-query-optimize": _P2_AI,
    "ai-schema-design": _P2_AI,
    "ai-schema-extract": _P2_AI,
    "anomaly-detection": _P2_AI,
    "auto-diagnose": _P2_AI,
    "autonomous-ab-test": _P2_AI,
    "autonomous-xai": _P2_AI,
    "failure-prediction": _P2_AI,
    "index-advisor-deep": _P2_AI,
    "llm-diagnosis": _P2_AI,
    "llm-optimizer": _P2_AI,
    "llm-tuning": _P2_AI,
    "model-ops": _P2_AI,
    "model-ops-router": _P2_AI,
    "nl-query-sandbox": _P2_AI,
    "nl-query-visualizer": _P2_AI,
    "nl2sql-deep": _P2_AI,
    "plan-auto-optimize": _P2_AI,
    "query-intelligence-loop": _P2_AI,
    "trend-prediction": _P2_AI,
    # —— 治理/合规/安全（governance/compliance/lineage/security 等）（41）——
    "carbon-neutrality": _P2_GOV,
    "compliance-auto-check": _P2_GOV,
    "compliance-auto-scan": _P2_GOV,
    "compliance-report": _P2_GOV,
    "compliance-report-auto": _P2_GOV,
    "context-aware-masking": _P2_GOV,
    "cost-governance": _P2_GOV,
    "cross-node-audit": _P2_GOV,
    "cross-storage-lifecycle": _P2_GOV,
    "data-auto-archive": _P2_GOV,
    "data-cold-hot-split": _P2_GOV,
    "data-lifecycle-mgmt": _P2_GOV,
    "data-lineage": _P2_GOV,
    "data-quality": _P2_GOV,
    "data-validation": _P2_GOV,
    "dynamic-masking": _P2_GOV,
    "end-to-end-lineage": _P2_GOV,
    "evidence-auto-archive": _P2_GOV,
    "federated-query": _P2_GOV,
    "field-encryption": _P2_GOV,
    "governance": _P2_GOV,
    "governance-compliance": _P2_GOV,
    "governance-lineage": _P2_GOV,
    "green-computing": _P2_GOV,
    "hash-chain-enhanced": _P2_GOV,
    "lineage-viz": _P2_GOV,
    "masking-audit-link": _P2_GOV,
    "pqc-hybrid-kex": _P2_GOV,
    "pqc-key-rotation": _P2_GOV,
    "pqc-migration-exec": _P2_GOV,
    "pqc-ready": _P2_GOV,
    "prod-config-masking": _P2_GOV,
    "prod-metrics-acl": _P2_GOV,
    "rbac-enhanced": _P2_GOV,
    "sec-auto": _P2_GOV,
    "sec-compliance": _P2_GOV,
    "sensitive-discover": _P2_GOV,
    "sla-monitor": _P2_GOV,
    "storage-lifecycle": _P2_GOV,
    "tde-key-mgmt": _P2_GOV,
    "validate-on-write": _P2_GOV,
    # —— 能力扩展开关（由下游按场景选择）（84）——
    "alert-rules": _P2_CAP,
    "auto-rollback": _P2_CAP,
    "backup": _P2_CAP,
    "backup-verify": _P2_CAP,
    "batch-stream": _P2_CAP,
    "binding-async-stream": _P2_CAP,
    "canary-release": _P2_CAP,
    "cdc-enhanced": _P2_CAP,
    "circuit-breaker": _P2_CAP,
    "cloud-native-enhanced": _P2_CAP,
    "cloudnative-sidecar": _P2_CAP,
    "cloudnative-template": _P2_CAP,
    "consistency-tunable": _P2_CAP,
    "cross-lang-dtx": _P2_CAP,
    "cross-tx-consistency": _P2_CAP,
    "custom-diagnostic": _P2_CAP,
    "dashboard-export": _P2_CAP,
    "db-fusion": _P2_CAP,
    "db-resolver": _P2_CAP,
    "designer": _P2_CAP,
    "developer-portal": _P2_CAP,
    "dist-cache-enhanced": _P2_CAP,
    "dist-consensus": _P2_CAP,
    "dist-enhance": _P2_CAP,
    "dtx-saga-coordinator": _P2_CAP,
    "dynamic-sharding": _P2_CAP,
    "eco-deep": _P2_CAP,
    "evolution-safety-net": _P2_CAP,
    "failover-enhanced": _P2_CAP,
    "graph": _P2_CAP,
    "graph-query-deep": _P2_CAP,
    "graphql": _P2_CAP,
    "graphql-complexity": _P2_CAP,
    "graphql-n1": _P2_CAP,
    "graphql-schema-gen": _P2_CAP,
    "hot-reload": _P2_CAP,
    "lc-bidirectional-sync": _P2_CAP,
    "message-tracing": _P2_CAP,
    "metrics-collect": _P2_CAP,
    "migrate": _P2_CAP,
    "migration-branch": _P2_CAP,
    "migration-dry-run": _P2_CAP,
    "multi-region-active": _P2_CAP,
    "multi-region-enhanced": _P2_CAP,
    "multimodal": _P2_CAP,
    "multimodal-er": _P2_CAP,
    "multimodal-vision": _P2_CAP,
    "multimodal-voice": _P2_CAP,
    "n1-lint": _P2_CAP,
    "obs-enhanced": _P2_CAP,
    "observability": _P2_CAP,
    "openapi-reverse": _P2_CAP,
    "persistence": _P2_CAP,
    "plugin-marketplace": _P2_CAP,
    "plugin-marketplace-ops": _P2_CAP,
    "prod-log-level": _P2_CAP,
    "qb-migration-tool": _P2_CAP,
    "query-advisor": _P2_CAP,
    "query-logging": _P2_CAP,
    "rate-limit": _P2_CAP,
    "read-write-splitting": _P2_CAP,
    "rollback-sandbox": _P2_CAP,
    "rw-split-enhanced": _P2_CAP,
    "saga-enhanced": _P2_CAP,
    "schema-diff-viz": _P2_CAP,
    "sdk-auto-gen": _P2_CAP,
    "search": _P2_CAP,
    "shadow-traffic-verify": _P2_CAP,
    "shard-rebalance": _P2_CAP,
    "split-brain-detect": _P2_CAP,
    "streaming-export": _P2_CAP,
    "structured-logging": _P2_CAP,
    "three-pc": _P2_CAP,
    "timeseries": _P2_CAP,
    "toolchain-lsp-enhance": _P2_CAP,
    "two-pc-enhanced": _P2_CAP,
    "type-safe-columns": _P2_CAP,
    "typed-dsl": _P2_CAP,
    "typed-relation": _P2_CAP,
    "typed-schema": _P2_CAP,
    "vector-search": _P2_CAP,
    "xa": _P2_CAP,
    "zero-downtime-evolve": _P2_CAP,
    "zero-downtime-mig": _P2_CAP,
}


def c_class_exempt(sym):
    """C 类豁免校验（fail-closed）。返回 (True, 原因) 或 (False, 失败原因)。"""
    if sym not in DECLARED_SYMBOLS:
        return False, f"登记符号 {sym} 不在 DECLARED_SYMBOLS 中（登记表过期，请移除或恢复声明）"
    audit_path = os.path.join(ROOT, AUDIT_DOC)
    if not os.path.isfile(audit_path):
        return False, f"审计报告缺失 {AUDIT_DOC}（C 类出处不可追溯）"
    rx = sym_re(sym)
    for doc in C_CLASS_DOCS:
        path = os.path.join(ROOT, doc)
        if not os.path.isfile(path):
            continue
        for i, ln in enumerate(
            open(path, encoding="utf-8", errors="replace").read().splitlines(), 1
        ):
            if TRACE_ROW_RE.match(ln) or not rx.search(ln):
                continue
            if INTEGRATION_WORDING_RE.search(ln):
                return False, f"{doc}:{i} 对 {sym} 使用集成语义措辞：{ln.strip()[:80]}"
    return True, "C 类保留组件（审计出处存在 + 文档措辞已降级为'提供组件'）"

# 导出声明模式（lib.rs 的 pub mod / pub use，不算调用）
EXPORT_RE = re.compile(r"^\s*(pub\s+mod\s+\w+|pub\s+use\s+\S+)")
# 多行 pub use X::{ ... }; 块（续行符号不计调用）
PUB_USE_BLOCK_RE = re.compile(r"pub\s+use\s+[\w:]+::\{[^}]*\};", re.S)

# ============================================================================
# 接线断言表（门禁 15 双模式之"接线断言"）：
# 覆盖 PHANTOM-1 符号级检测看不见的"模块内接线"（符号与调用方同文件）。
# 验证方式：caller 函数体（括号匹配提取）内必须引用 symbol 的定义。
# 新增接线时：跨文件接线 → 符号断言自动变绿；同文件接线 → 在此登记断言。
# ============================================================================
WIRING_ASSERTIONS = [
    {
        "id": "W1",
        "name": "cache_ttl → execute_with_cache（query.rs 内部接线）",
        "symbol": "get_cache_ttl",
        "file": "packages/sz-orm-core/src/query.rs",
        "caller": "execute_with_cache",
        "hint": "QueryBuilder::cache_ttl 的 TTL 被 execute_with_cache 消费（v4.7.0 接线，测试 test_execute_with_cache_ttl_hit）",
    },
    {
        "id": "W2",
        "name": "N1QueryDetector → BatchLoader（entity_graph.rs 内部接线）",
        "symbol": "N1QueryDetector",
        "file": "packages/sz-orm-core/src/entity_graph.rs",
        "caller": "with_detector",
        "hint": "批量加载器通过 with_detector 挂载 N+1 检测器（v4.7.0 接线，测试 test_batch_loader_with_detector_*）",
    },
    {
        "id": "W3",
        "name": "PenetrationGuard → CacheProtection（cache_warmup_protection.rs 内部接线）",
        "symbol": "self.penetration",
        "file": "packages/sz-orm-core/src/cache_warmup_protection.rs",
        "caller": "get_or_load",
        "hint": "组合防护门面：穿透防护进入缓存加载路径（字段类型 PenetrationGuard，构造点 CacheProtection::new；v4.7.0 真剩余接入，测试 test_cache_protection_*）",
    },
    {
        "id": "W4",
        "name": "SingleFlight → CacheProtection（cache_warmup_protection.rs 内部接线）",
        "symbol": "single_flight",
        "file": "packages/sz-orm-core/src/cache_warmup_protection.rs",
        "caller": "get_or_load",
        "hint": "组合防护门面：击穿防护进入缓存加载路径（字段类型 SingleFlight，构造点 CacheProtection::new；v4.7.0 真剩余接入，测试 test_cache_protection_single_flight_dedup）",
    },
]


def extract_fn_body(lines, fn_name):
    """提取 `pub fn fn_name` / `pub async fn fn_name` 的函数体（括号匹配），返回行区间或 None。"""
    start = None
    for i, ln in enumerate(lines):
        if re.search(rf"^\s*pub\s+(?:async\s+)?fn\s+{re.escape(fn_name)}\b", ln):
            start = i
            break
    if start is None:
        return None
    # 找函数体起始 {（可能跨行，如泛型参数）
    depth = 0
    j = start
    while j < len(lines):
        depth += lines[j].count("{") - lines[j].count("}")
        if depth >= 1:
            # 已进入函数体，继续到 depth 归零
            k = j
            while k < len(lines):
                depth += lines[k].count("{") - lines[k].count("}")
                if depth <= 0:
                    return start, k
                k += 1
            return None
        j += 1
    return None


def check_wiring(assertions):
    """验证接线断言：caller 函数体内引用 symbol。"""
    results = []
    for a in assertions:
        path = os.path.join(ROOT, a["file"])
        if not os.path.isfile(path):
            results.append((a, "FAIL", "文件不存在"))
            continue
        lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
        span = extract_fn_body(lines, a["caller"])
        if span is None:
            results.append((a, "FAIL", f"未找到函数 {a['caller']}"))
            continue
        body = "\n".join(lines[span[0]:span[1] + 1])
        # 剥注释（函数体 doc 注释里的符号引用不算接线）
        body = "\n".join(l for l in body.splitlines()
                         if not l.strip().startswith("//"))
        if re.search(r"\b" + re.escape(a["symbol"]) + r"\b", body):
            results.append((a, "PASS", f"函数体 {span[0]+1}-{span[1]+1} 引用 {a['symbol']}"))
        else:
            results.append((a, "FAIL", f"函数 {a['caller']} 体内未引用 {a['symbol']}"))
    return results
# 非生产目录（测试/示例/基准/fuzz）
NON_PROD_DIR_RE = re.compile(r"(^|/)(tests|benches|fuzz|examples)(/|$)")


def sym_re(symbol):
    """词边界匹配，避免子串误报（如 BloomFilterGuard 含 BloomFilter）。"""
    return re.compile(r"\b" + re.escape(symbol) + r"\b")


def iter_rs_files():
    for root, dirs, files in os.walk(PACKAGES):
        dirs[:] = [d for d in dirs if d not in ("target", "node_modules", ".git")]
        for f in files:
            if f.endswith(".rs"):
                yield os.path.join(root, f)
    for d in (CLI, EXAMPLES):
        pass  # cli/examples 视为生产入口（脚本入口），src 位于各自目录
    for base in (os.path.join(ROOT, "cli", "src"), os.path.join(ROOT, "examples", "src")):
        if os.path.isdir(base):
            for root, dirs, files in os.walk(base):
                dirs[:] = [d for d in dirs if d not in ("target", "node_modules")]
                for f in files:
                    if f.endswith(".rs"):
                        yield os.path.join(root, f)


def strip_tests(txt):
    """去掉 #[cfg(test)] 与顶层 mod tests 块（括号匹配，支持嵌套花括号——
    正则 [^{}]* 会在测试体内嵌套 { } 处失败导致剥离不完整，2026-08-14 修复）。"""

    def strip_block(src, marker):
        out = []
        i = 0
        n = len(src)
        while True:
            m = re.search(marker, src[i:])
            if not m:
                out.append(src[i:])
                break
            start = i + m.start()
            out.append(src[i:start])
            j = src.find('{', start)
            if j == -1:
                out.append(src[start:])
                break
            depth = 0
            k = j
            while k < n:
                if src[k] == '{':
                    depth += 1
                elif src[k] == '}':
                    depth -= 1
                    if depth == 0:
                        break
                k += 1
            i = k + 1 if k < n else n
        return ''.join(out)

    txt = strip_block(txt, r'#\[cfg\(test\)\]')
    return strip_block(txt, r'\bmod\s+tests\s*\{')


def find_definition_file(symbol):
    """返回定义符号的文件（含 pub struct/enum/trait/fn 的行）。"""
    for path in iter_rs_files():
        try:
            lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
        except OSError:
            continue
        for i, ln in enumerate(lines):
            if re.search(rf"^\s*pub\s+(struct|enum|trait|fn)\s+{re.escape(symbol)}\b", ln):
                return path, i + 1
    return None, None


def production_callers(symbol, def_file):
    """统计定义文件之外的生产 src 引用（排除注释、导出声明、测试）。"""
    rx = sym_re(symbol)
    callers = []
    for path in iter_rs_files():
        if path == def_file:
            continue
        rel = os.path.relpath(path, ROOT).replace("\\", "/")
        if NON_PROD_DIR_RE.search(rel):
            continue
        try:
            txt = open(path, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        if not rx.search(txt):
            continue
        body = PUB_USE_BLOCK_RE.sub("", strip_tests(txt))
        hits = 0
        for ln in body.splitlines():
            if not rx.search(ln):
                continue
            s = ln.strip()
            if s.startswith("//") or s.startswith("///") or s.startswith("//!"):
                continue
            if EXPORT_RE.match(s):
                continue
            hits += 1
        if hits:
            callers.append((rel, hits))
    return callers


def check_symbols(symbols, verbose=False):
    phantoms = []
    ok = []
    for sym in sorted(set(symbols)):
        def_file, def_line = find_definition_file(sym)
        if def_file is None:
            # 未定义（可能是外部 crate 符号或已被移除）
            phantoms.append((sym, "未找到定义", []))
            continue
        callers = production_callers(sym, def_file)
        def_rel = os.path.relpath(def_file, ROOT).replace("\\", "/")
        if callers:
            ok.append((sym, def_rel, callers))
        else:
            phantoms.append((sym, def_rel, []))
    return ok, phantoms


def collect_features():
    """{pkg: {feature: [依赖的 feature]}}，含 default。"""
    result = {}
    for toml in sorted(glob.glob(os.path.join(PACKAGES, "*", "Cargo.toml"))) + [CLI, EXAMPLES]:
        txt = open(toml, encoding="utf-8", errors="replace").read()
        m = re.search(r"\[features\]", txt)
        if not m:
            continue
        pkg = re.search(r'^name\s*=\s*"([^"]+)"', txt, re.M).group(1)
        section = txt[m.end():].split("\n[")[0]
        feats = {}
        for ln in section.splitlines():
            mm = re.match(r'^\s*([\w-]+)\s*=\s*\[([^\]]*)\]', ln)
            if mm:
                feats[mm.group(1)] = [x.strip().strip('"') for x in mm.group(2).split(",") if x.strip()]
        result[pkg] = feats
    return result


def collect_enable_points():
    """所有启用点：依赖声明 features=[...] + feature 定义中的 dep/feature 语法。"""
    enabled = {}
    for toml in sorted(glob.glob(os.path.join(PACKAGES, "*", "Cargo.toml"))) + [CLI, EXAMPLES]:
        txt = open(toml, encoding="utf-8", errors="replace").read()
        # 1) 依赖声明 features=[...]
        for m in re.finditer(r"([\w-]+)\s*=\s*\{([^}]*)\}", txt):
            dep, body = m.group(1), m.group(2)
            if not dep.startswith("sz-orm"):
                continue
            fm = re.search(r'features\s*=\s*\[([^\]]*)\]', body)
            if fm:
                for ff in re.findall(r'"([\w-]+)"', fm.group(1)):
                    enabled.setdefault(dep, set()).add(ff)
        # 2) feature 定义内的 dep/feature 语法（如 cli: n1-lint = ["dep:sz-orm-n1-lint", "sz-orm-n1-lint/n1-lint"]）
        for m in re.finditer(r'([\w-]+)/([\w-]+)', txt):
            dep, feat = m.group(1), m.group(2)
            if dep.startswith("sz-orm"):
                enabled.setdefault(dep, set()).add(feat)
    return enabled


def check_feature_matrix():
    """PHANTOM-2：定义存在但无任何跨包启用点且不在 default。"""
    feats = collect_features()
    enabled = collect_enable_points()
    phantoms = []
    for pkg, fs in sorted(feats.items()):
        for f in sorted(fs):
            if f == "default":
                continue
            # 跨包启用
            if f in enabled.get(pkg, set()):
                continue
            # default 链：被 default 直接或间接引用
            if f in fs.get("default", []):
                continue
            # 内部 feature 链不算启用（默认不编译）
            phantoms.append((pkg, f))
    return phantoms


def main():
    ap = argparse.ArgumentParser(description="幻影交付扫描（AGENTS.md 门禁 15，双模式）")
    ap.add_argument("--symbols", nargs="*", default=DECLARED_SYMBOLS,
                    help="符号表（默认内置宣称符号表）")
    ap.add_argument("--strict", action="store_true", help="PHANTOM-2 也按失败计")
    ap.add_argument("--skip-matrix", action="store_true", help="跳过 feature 矩阵检查")
    ap.add_argument("--skip-wiring", action="store_true", help="跳过接线断言检查")
    ap.add_argument("--show-exempt", action="store_true", help="逐条列出已登记豁免的 PHANTOM-2 feature")
    args = ap.parse_args()

    print("=" * 60)
    print("  幻影交付扫描（门禁 15）: 符号断言 + 接线断言 + 门控矩阵")
    print("=" * 60)

    ok, phantoms = check_symbols(args.symbols)
    # C 类豁免：零调用但按"提供 X 组件（需手动接入）"定位保留的独立库组件
    # （fail-closed：登记条件由 c_class_exempt 逐项校验，不满足仍按 PHANTOM-1 判红）
    exempt = {}
    for sym, def_rel, _ in phantoms:
        if def_rel != "未找到定义" and sym in C_CLASS_SYMBOLS:
            ok_exempt, reason = c_class_exempt(sym)
            if ok_exempt:
                exempt[sym] = reason
    print(f"\n[1/3] 符号断言（宣称符号 {len(ok) + len(phantoms)} 个，C 类豁免 {len(exempt)} 个）")
    for sym, def_rel, callers in ok:
        print(f"  ✅ {sym:28s} 定义 {def_rel}  生产调用方 {len(callers)} 处")
    for sym, def_rel, _ in phantoms:
        if def_rel == "未找到定义":
            print(f"  ⚠️  {sym:28s} 未找到定义（符号已移除或非本仓库定义）")
        elif sym in exempt:
            print(f"  ⓒ  C 类保留 {sym:16s} 定义 {def_rel} — 零调用（{exempt[sym]}）")
        else:
            print(f"  ❌ PHANTOM-1 {sym:18s} 定义 {def_rel} — 生产路径零调用")

    wiring = [] if args.skip_wiring else check_wiring(WIRING_ASSERTIONS)
    print(f"\n[2/3] 接线断言（模块内接线 {len(wiring)} 条）")
    wiring_fail = 0
    for a, status, detail in wiring:
        if status == "PASS":
            print(f"  ✅ {a['id']} {a['name']} — {detail}")
        else:
            wiring_fail += 1
            print(f"  ❌ {a['id']} {a['name']} — {detail}（{a['hint']}）")

    if not args.skip_matrix:
        p2 = check_feature_matrix()
        # PHANTOM-2 豁免：命中 PHANTOM2_EXEMPT 登记表的归入"已登记豁免"单独计数
        p2_warn = [(pkg, f) for pkg, f in p2 if f not in PHANTOM2_EXEMPT]
        p2_exempt = [(pkg, f) for pkg, f in p2 if f in PHANTOM2_EXEMPT]
        print(f"\n[3/3] feature 启用矩阵（未启用 gate {len(p2)} 个：已登记豁免 {len(p2_exempt)}，未登记警告 {len(p2_warn)}）")
        for pkg, f in p2_warn:
            print(f"  ⚠️  PHANTOM-2 {pkg:26s} {f} — 无任何成员启用，默认构建不编译（如属正常形态请登记 PHANTOM2_EXEMPT）")
        if args.show_exempt:
            for pkg, f in p2_exempt:
                print(f"  ⓔ  PHANTOM-2 豁免 {pkg:20s} {f} — {PHANTOM2_EXEMPT[f]}")
        matrix_fail = args.strict and bool(p2_warn)
    else:
        p2 = []
        p2_warn = []
        p2_exempt = []
        matrix_fail = False

    n1 = len([p for p in phantoms if p[1] != "未找到定义"])
    n1_real = n1 - len(exempt)
    print("\n" + "=" * 60)
    print(f"  结果: PHANTOM-1 {n1_real} 个（C 类豁免 {len(exempt)}）| PHANTOM-2 {len(p2_warn)}（豁免 {len(p2_exempt)}）| 符号通过 {len(ok)} | 接线断言 {len(wiring) - wiring_fail}/{len(wiring)}")
    print("=" * 60)
    if n1_real > 0 or matrix_fail or wiring_fail > 0:
        print("❌ 门禁 15 未通过 — 存在幻影交付或接线断言失败（请接线、登记断言或修正文档措辞）")
        return 1
    print("✅ 门禁 15 通过（PHANTOM-2 未登记警告 0；豁免登记见 PHANTOM2_EXEMPT，--strict 升级为失败）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
