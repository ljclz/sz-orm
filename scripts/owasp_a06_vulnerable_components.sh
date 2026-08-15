#!/usr/bin/env bash
#
# OWASP A06: 易受攻击和过时组件深化渗透测试（Bash）
#
# 对应 REQ-V49-006（OWASP A06 深化）
# 运行 cargo audit / cargo deny check 验证依赖安全性。
#

set -euo pipefail
EXIT_CODE=0
SKIP_SBOM=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-sbom) SKIP_SBOM=true; shift ;;
        *) shift ;;
    esac
done

invoke_cve_audit() {
    echo "[A06-1] CVE 审计 (cargo audit)..."
    if cargo audit 2>&1; then
        echo "  PASS: 无未忽略公告"
    else
        echo "  FAIL: cargo audit 发现未忽略的 RUSTSEC 公告"
        EXIT_CODE=1
    fi
}

invoke_license_check() {
    echo "[A06-2] 许可证检查 (cargo deny check licenses)..."
    if cargo deny check licenses 2>&1; then
        echo "  PASS: 全部许可证在白名单"
    else
        echo "  FAIL: 发现 copyleft 或不在白名单的许可证"
        EXIT_CODE=1
    fi
}

invoke_yanked_check() {
    echo "[A06-3] Yanked 检查 (cargo deny check)..."
    if cargo deny check 2>&1; then
        echo "  PASS: 无 yanked 依赖"
    else
        echo "  WARN: 发现 yanked 依赖或其它问题"
    fi
}

invoke_duplicate_check() {
    echo "[A06-4] 重复依赖检查 (cargo deny check bans)..."
    if cargo deny check bans 2>&1; then
        echo "  PASS: 无重复依赖"
    else
        echo "  WARN: 发现重复依赖（版本碎片化）"
    fi
}

invoke_source_check() {
    echo "[A06-5] 依赖来源检查 (cargo deny check sources)..."
    if cargo deny check sources 2>&1; then
        echo "  PASS: 全部依赖来自 crates.io"
    else
        echo "  FAIL: 发现非 crates.io 来源"
        EXIT_CODE=1
    fi
}

invoke_sbom_generation() {
    if [ "$SKIP_SBOM" = true ]; then
        echo "[A06-6] SBOM 生成跳过（--skip-sbom）"
        return
    fi
    echo "[A06-6] SBOM 生成 (cargo cyclonedx)..."
    if ! command -v cargo-cyclonedx &>/dev/null; then
        echo "  SKIP: cargo cyclonedx 未安装"
        return
    fi
    cargo cyclonedx 2>&1 || true
    if [ -f "sbom.json" ]; then
        echo "  PASS: sbom.json 已生成"
    else
        echo "  WARN: sbom.json 未生成"
    fi
}

echo "=== OWASP A06: 易受攻击和过时组件深化渗透测试 ==="
echo ""

invoke_cve_audit
invoke_license_check
invoke_yanked_check
invoke_duplicate_check
invoke_source_check
invoke_sbom_generation

echo ""
if [ "$EXIT_CODE" -eq 0 ]; then
    echo "=== A06 审计完成: 全部通过 ==="
else
    echo "=== A06 审计完成: 存在失败项 ==="
fi
exit "$EXIT_CODE"