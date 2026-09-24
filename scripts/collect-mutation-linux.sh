#!/bin/bash
# Collect mutation test results on Linux using cargo-mutants.
#
# Usage: bash scripts/collect-mutation-linux.sh --packages sz-orm-core --output <report.md> --fail-under 70
#
# Prerequisites: cargo-mutants installed (cargo install cargo-mutants)
# Note: cargo-mutants modifies source code in-place. After completion,
#       source is restored via git checkout and verified clean.
# Exit codes: 0 = pass (kill rate >= fail-under), 1 = fail (kill rate < fail-under or error)

set -euo pipefail

PACKAGES="sz-orm-core,sz-orm-sqlx,sz-orm-pool,sz-orm-query,sz-orm-model"
OUTPUT=""
BASELINE="70"
FAIL_UNDER="70"
JSON_OUTPUT=""

while [ $# -gt 0 ]; do
    case "$1" in
        --packages) PACKAGES="$2"; shift 2 ;;
        --output) OUTPUT="$2"; shift 2 ;;
        --baseline) BASELINE="$2"; shift 2 ;;
        --fail-under) FAIL_UNDER="$2"; shift 2 ;;
        --json) JSON_OUTPUT="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "=== Mutation Test Collection (Linux) ==="

if ! command -v cargo-mutants &>/dev/null; then
    echo "ERROR: cargo-mutants not installed"
    echo "Install: cargo install cargo-mutants"
    exit 1
fi

TOOL_VERSION=$(cargo mutants --version)
DATE_UTC=$(date -u '+%Y-%m-%d %H:%M:%S UTC')
OS_INFO=$(uname -a)

echo "Tool version: $TOOL_VERSION"
echo "Date: $DATE_UTC"
echo "OS: $OS_INFO"
echo "Packages: $PACKAGES"
echo "Fail-under: ${FAIL_UNDER}%"
echo ""

IFS=',' read -ra PKG_ARRAY <<< "$PACKAGES"

TOTAL_MUTANTS=0
KILLED_MUTANTS=0
SURVIVED_MUTANTS=0
TIMEOUT_MUTANTS=0
PKG_RESULTS=""

echo "Running cargo-mutants..."
for pkg in "${PKG_ARRAY[@]}"; do
    echo "  Testing $pkg..."
    # 移除 || true，cargo-mutants 失败时仍继续测试其他包但记录失败
    cargo mutants --package "$pkg" --in-place 2>&1 || echo "  WARNING: $pkg had mutants errors"

    # 解析 mutants.out/ 目录
    MUTANTS_DIR="mutants.out"
    if [ -d "$MUTANTS_DIR" ]; then
        # cargo-mutants 输出 mutants.json 含每个变异体状态
        if [ -f "$MUTANTS_DIR/mutants.json" ]; then
            PKG_TOTAL=$(python3 -c "
import json
with open('$MUTANTS_DIR/mutants.json') as f:
    data = json.load(f)
print(len(data))
" 2>/dev/null || echo 0)
            PKG_KILLED=$(python3 -c "
import json
with open('$MUTANTS_DIR/mutants.json') as f:
    data = json.load(f)
print(sum(1 for m in data if m.get('status') == 'Killed'))
" 2>/dev/null || echo 0)
            PKG_SURVIVED=$(python3 -c "
import json
with open('$MUTANTS_DIR/mutants.json') as f:
    data = json.load(f)
print(sum(1 for m in data if m.get('status') == 'Survived'))
" 2>/dev/null || echo 0)
            PKG_TIMEOUT=$(python3 -c "
import json
with open('$MUTANTS_DIR/mutants.json') as f:
    data = json.load(f)
print(sum(1 for m in data if m.get('status') == 'Timeout'))
" 2>/dev/null || echo 0)
        else
            # 回退到文本解析
            PKG_TOTAL=$(find "$MUTANTS_DIR" -name "*.log" 2>/dev/null | wc -l)
            PKG_KILLED=$(grep -rl "Killed" "$MUTANTS_DIR" 2>/dev/null | wc -l)
            PKG_SURVIVED=$(grep -rl "Survived" "$MUTANTS_DIR" 2>/dev/null | wc -l)
            PKG_TIMEOUT=0
        fi
    else
        PKG_TOTAL=0
        PKG_KILLED=0
        PKG_SURVIVED=0
        PKG_TIMEOUT=0
    fi

    TOTAL_MUTANTS=$((TOTAL_MUTANTS + PKG_TOTAL))
    KILLED_MUTANTS=$((KILLED_MUTANTS + PKG_KILLED))
    SURVIVED_MUTANTS=$((SURVIVED_MUTANTS + PKG_SURVIVED))
    TIMEOUT_MUTANTS=$((TIMEOUT_MUTANTS + PKG_TIMEOUT))

    echo "    $pkg: total=$PKG_TOTAL killed=$PKG_KILLED survived=$PKG_SURVIVED timeout=$PKG_TIMEOUT"

    PKG_RESULTS="${PKG_RESULTS}  {\"package\": \"$pkg\", \"total\": $PKG_TOTAL, \"killed\": $PKG_KILLED, \"survived\": $PKG_SURVIVED, \"timeout\": $PKG_TIMEOUT},\n"

    # 每个包测试后恢复源码
    git checkout -- packages/ 2>/dev/null || true
    git clean -fd packages/ 2>/dev/null || true
done

echo ""
echo "Mutation summary:"
echo "  Total mutants:   $TOTAL_MUTANTS"
echo "  Killed mutants:   $KILLED_MUTANTS"
echo "  Survived mutants: $SURVIVED_MUTANTS"
echo "  Timeout mutants:  $TIMEOUT_MUTANTS"

# 计算杀率
if [ "$TOTAL_MUTANTS" -eq 0 ]; then
    echo "WARNING: No mutants generated, setting kill rate to 0"
    KILL_RATE="0.00"
else
    KILL_RATE=$(awk -v killed="$KILLED_MUTANTS" -v total="$TOTAL_MUTANTS" \
        'BEGIN { printf "%.2f", (killed / total) * 100 }')
fi

echo "  Kill rate:        ${KILL_RATE}%"
echo "  Threshold:        ${FAIL_UNDER}%"

# 门限判定
PASS=$(awk -v rate="$KILL_RATE" -v threshold="$FAIL_UNDER" \
    'BEGIN { print (rate >= threshold) ? "true" : "false" }')

if [ "$PASS" = "false" ]; then
    echo ""
    echo "FAIL: Kill rate ${KILL_RATE}% < threshold ${FAIL_UNDER}%"
    echo ""
    echo "Survived mutants:"
    for pkg in "${PKG_ARRAY[@]}"; do
        MUTANTS_DIR="mutants.out"
        if [ -d "$MUTANTS_DIR" ]; then
            if [ -f "$MUTANTS_DIR/mutants.json" ]; then
                python3 -c "
import json
with open('$MUTANTS_DIR/mutants.json') as f:
    data = json.load(f)
for m in data:
    if m.get('status') == 'Survived':
        print(f'  {m.get(\"file\",\"?\")}:{m.get(\"line\",\"?\")} {m.get(\"mutation\",\"?\")}')
" 2>/dev/null || true
            fi
        fi
    done
fi

# 源码残留校验
echo ""
echo "Restoring source code..."
git checkout -- packages/ 2>/dev/null || true
git clean -fd packages/ 2>/dev/null || true

# 验证源码无残留
if ! git diff --quiet -- packages/ 2>/dev/null; then
    echo "ERROR: Source code has residual changes after git checkout"
    echo "Residual diff:"
    git diff -- packages/ 2>/dev/null | head -50
    exit 1
fi

echo "Source code restored (no residual)."

if [ "$PASS" = "false" ]; then
    exit 1
fi

echo "PASS: Kill rate ${KILL_RATE}% >= threshold ${FAIL_UNDER}%"

# 生成 Markdown 报告
if [ -n "$OUTPUT" ]; then
    echo "Writing report to $OUTPUT..."
    {
        echo "# Mutation Test Report (Linux)"
        echo ""
        echo "- Date: $DATE_UTC"
        echo "- Tool: $TOOL_VERSION"
        echo "- OS: $OS_INFO"
        echo "- Packages: $PACKAGES"
        echo "- Baseline: ${BASELINE}%"
        echo "- Threshold: ${FAIL_UNDER}%"
        echo ""
        echo "## Summary"
        echo ""
        echo "- Total mutants:   $TOTAL_MUTANTS"
        echo "- Killed mutants:   $KILLED_MUTANTS"
        echo "- Survived mutants: $SURVIVED_MUTANTS"
        echo "- Timeout mutants:  $TIMEOUT_MUTANTS"
        echo "- Kill rate:        ${KILL_RATE}%"
        echo "- Pass:             $PASS"
        echo ""
        echo "## Per-package Results"
        echo ""
        echo "| Package | Total | Killed | Survived | Timeout |"
        echo "|---------|-------|--------|----------|---------|"
        for pkg in "${PKG_ARRAY[@]}"; do
            echo "| $pkg | - | - | - | - |"
        done
    } > "$OUTPUT"
    echo "Report written to $OUTPUT"
fi

# 生成结构化 JSON 输出
if [ -n "$JSON_OUTPUT" ]; then
    echo "Writing JSON to $JSON_OUTPUT..."
    # 构建 packages JSON 数组
    PACKAGES_JSON=$(printf "%s" "$PKG_RESULTS" | sed 's/,$//')
    cat > "$JSON_OUTPUT" <<EOF
{
  "date": "$DATE_UTC",
  "tool_version": "$TOOL_VERSION",
  "os": "$OS_INFO",
  "packages": [$PACKAGES_JSON],
  "total_mutants": $TOTAL_MUTANTS,
  "killed_mutants": $KILLED_MUTANTS,
  "survived_mutants": $SURVIVED_MUTANTS,
  "timeout_mutants": $TIMEOUT_MUTANTS,
  "kill_rate_percent": $KILL_RATE,
  "fail_under": $FAIL_UNDER,
  "pass": $PASS
}
EOF
    echo "JSON written to $JSON_OUTPUT"
fi

echo ""
echo "Mutation test collection complete."
