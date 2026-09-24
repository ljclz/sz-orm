#!/bin/bash
# Collect code coverage on Linux using cargo-llvm-cov.
#
# Usage: bash scripts/collect-coverage-linux.sh --output <report.md> --fail-under 60
#
# Prerequisites: cargo-llvm-cov installed (cargo install cargo-llvm-cov)
# Exit codes: 0 = pass (coverage >= fail-under), 1 = fail (coverage < fail-under or error)

set -euo pipefail

OUTPUT=""
BASELINE="87.4"
FAIL_UNDER="60"
JSON_OUTPUT=""

while [ $# -gt 0 ]; do
    case "$1" in
        --output) OUTPUT="$2"; shift 2 ;;
        --baseline) BASELINE="$2"; shift 2 ;;
        --fail-under) FAIL_UNDER="$2"; shift 2 ;;
        --json) JSON_OUTPUT="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "=== Coverage Collection (Linux) ==="

if ! command -v cargo-llvm-cov &>/dev/null; then
    echo "ERROR: cargo-llvm-cov not installed"
    echo "Install: cargo install cargo-llvm-cov"
    exit 1
fi

TOOL_VERSION=$(cargo-llvm-cov --version)
DATE_UTC=$(date -u '+%Y-%m-%d %H:%M:%S UTC')
OS_INFO=$(uname -a)

echo "Tool version: $TOOL_VERSION"
echo "Date: $DATE_UTC"
echo "OS: $OS_INFO"
echo "Fail-under: ${FAIL_UNDER}%"
echo ""

echo "Running cargo-llvm-cov..."
# 移除 || true，cargo-llvm-cov 失败时 exit 1
cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info 2>&1

if [ ! -f lcov.info ]; then
    echo "ERROR: lcov.info not generated"
    exit 1
fi

echo "Parsing lcov.info..."

# 解析 lcov.info：LF = line total, LH = line hit
# lcov 格式每条 DA:<line>,<count> 表示该行执行次数
# 使用 awk 汇总所有文件的 LF 和 LH
LINES_TOTAL=$(awk '/^LF:/ { total += $2 } END { print total + 0 }' lcov.info)
LINES_HIT=$(awk '/^LH:/ { hit += $2 } END { print hit + 0 }' lcov.info)

if [ "$LINES_TOTAL" -eq 0 ]; then
    echo "ERROR: LINES_TOTAL is 0, lcov.info may be empty or malformed"
    exit 1
fi

# 计算覆盖率百分比（保留 2 位小数）
COVERAGE_PERCENT=$(awk -v hit="$LINES_HIT" -v total="$LINES_TOTAL" \
    'BEGIN { printf "%.2f", (hit / total) * 100 }')

echo "Coverage summary:"
echo "  Total lines: $LINES_TOTAL"
echo "  Hit lines:   $LINES_HIT"
echo "  Coverage:    ${COVERAGE_PERCENT}%"
echo "  Threshold:   ${FAIL_UNDER}%"

# 门限判定
PASS=$(awk -v cov="$COVERAGE_PERCENT" -v threshold="$FAIL_UNDER" \
    'BEGIN { print (cov >= threshold) ? "true" : "false" }')

if [ "$PASS" = "false" ]; then
    echo ""
    echo "FAIL: Coverage ${COVERAGE_PERCENT}% < threshold ${FAIL_UNDER}%"
    echo ""
    echo "Uncovered modules (top 20):"
    # 列出覆盖率最低的模块
    awk '
        /^SF:/ { file = substr($0, 4) }
        /^LF:/ { lf = $2 }
        /^LH:/ { lh = $2; if (lf > 0) printf "%.2f%% %s (%d/%d)\n", (lh/lf)*100, file, lh, lf }
    ' lcov.info | sort -n | head -20
    exit 1
fi

echo "PASS: Coverage ${COVERAGE_PERCENT}% >= threshold ${FAIL_UNDER}%"

# 生成 Markdown 报告
if [ -n "$OUTPUT" ]; then
    echo "Writing report to $OUTPUT..."
    {
        echo "# Coverage Report (Linux)"
        echo ""
        echo "- Date: $DATE_UTC"
        echo "- Tool: $TOOL_VERSION"
        echo "- OS: $OS_INFO"
        echo "- Baseline: ${BASELINE}%"
        echo "- Threshold: ${FAIL_UNDER}%"
        echo ""
        echo "## Summary"
        echo ""
        echo "- Total lines: $LINES_TOTAL"
        echo "- Hit lines:   $LINES_HIT"
        echo "- Coverage:    ${COVERAGE_PERCENT}%"
        echo "- Pass:        $PASS"
        echo ""
        echo "## Per-module Coverage"
        echo ""
        echo "| Coverage | File | Hit/Total |"
        echo "|----------|------|-----------|"
        awk '
            /^SF:/ { file = substr($0, 4) }
            /^LF:/ { lf = $2 }
            /^LH:/ { lh = $2; if (lf > 0) printf "| %.2f%% | %s | %d/%d |\n", (lh/lf)*100, file, lh, lf }
        ' lcov.info | sort -n | head -50
    } > "$OUTPUT"
    echo "Report written to $OUTPUT"
fi

# 生成结构化 JSON 输出
if [ -n "$JSON_OUTPUT" ]; then
    echo "Writing JSON to $JSON_OUTPUT..."
    cat > "$JSON_OUTPUT" <<EOF
{
  "date": "$DATE_UTC",
  "tool_version": "$TOOL_VERSION",
  "os": "$OS_INFO",
  "total_lines": $LINES_TOTAL,
  "hit_lines": $LINES_HIT,
  "coverage_percent": $COVERAGE_PERCENT,
  "fail_under": $FAIL_UNDER,
  "pass": $PASS
}
EOF
    echo "JSON written to $JSON_OUTPUT"
fi

echo ""
echo "Coverage collection complete."
