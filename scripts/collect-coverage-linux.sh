#!/bin/bash
# Collect code coverage on Linux using cargo-llvm-cov.
#
# Usage: bash scripts/collect-coverage-linux.sh --output <report.md>
#
# Prerequisites: cargo-llvm-cov installed (cargo install cargo-llvm-cov)

set -euo pipefail

OUTPUT=""
BASELINE="87.4"

while [ $# -gt 0 ]; do
    case "$1" in
        --output) OUTPUT="$2"; shift 2 ;;
        --baseline) BASELINE="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "=== Coverage Collection (Linux) ==="

if ! command -v cargo-llvm-cov &>/dev/null; then
    echo "ERROR: cargo-llvm-cov not installed"
    echo "Install: cargo install cargo-llvm-cov"
    exit 1
fi

echo "Tool version: $(cargo-llvm-cov --version)"
echo "Date: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo "OS: $(uname -a)"
echo ""

echo "Running cargo-llvm-cov..."
cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info 2>&1 || true

if [ ! -f lcov.info ]; then
    echo "ERROR: lcov.info not generated"
    exit 1
fi

echo "Parsing lcov.info..."
LINES_TOTAL=$(grep -c '^L' lcov.info || echo 0)
LINES_HIT=$(grep '^L' lcov.info | awk -F: '{split($2,a,","); if(a[1]>0) print}' | wc -l)

echo "Coverage summary:"
echo "  Total lines: $LINES_TOTAL"
echo "  Hit lines:   $LINES_HIT"

if [ -n "$OUTPUT" ]; then
    echo "Writing report to $OUTPUT..."
    {
        echo "# Coverage Report (Linux)"
        echo ""
        echo "- Date: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
        echo "- Tool: $(cargo-llvm-cov --version)"
        echo "- OS: $(uname -a)"
        echo "- Baseline: ${BASELINE}%"
        echo ""
        echo "## Summary"
        echo ""
        echo "- Total lines: $LINES_TOTAL"
        echo "- Hit lines:   $LINES_HIT"
        echo ""
    } > "$OUTPUT"
    echo "Report written to $OUTPUT"
fi

echo ""
echo "Coverage collection complete."