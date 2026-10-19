#!/bin/bash
# Batch coverage collection using cargo-llvm-cov.
#
# Usage:
#   bash scripts/collect-coverage-batch.sh                    # collect and report
#   bash scripts/collect-coverage-batch.sh --fail-under 90    # fail if < 90%
#   bash scripts/collect-coverage-batch.sh --help             # show help
#
# Prerequisites: cargo-llvm-cov installed (cargo install cargo-llvm-cov)
# Exit codes: 0 = pass, 1 = fail

set -uo pipefail

FAIL_UNDER="0"
HELP=false

while [ $# -gt 0 ]; do
    case "$1" in
        --fail-under) FAIL_UNDER="$2"; shift 2 ;;
        --help) HELP=true; shift ;;
        *) shift ;;
    esac
done

if [ "$HELP" = true ]; then
    echo "Usage: bash scripts/collect-coverage-batch.sh [--fail-under N] [--help]"
    echo ""
    echo "Options:"
    echo "  --fail-under N  Fail if coverage < N% (default: 0)"
    echo "  --help          Show this help message"
    exit 0
fi

export PATH="/c/Users/Administrator/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/f/cargo-target}"
export RUSTC_WRAPPER=""
export RUST_MIN_STACK=67108864

TOOL_VERSION=$(cargo llvm-cov --version 2>&1)
DATE_UTC=$(date -u '+%Y-%m-%d %H:%M:%S UTC')

echo "=== Batch Coverage Collection ==="
echo "Tool: $TOOL_VERSION"
echo "Date: $DATE_UTC"
echo "Fail-under: ${FAIL_UNDER}%"
echo ""

# Batches by dependency layer (excluding examples, js, python which are independent/private)
BATCH_0="sz-orm-macros"
BATCH_1="sz-orm-core"
BATCH_2="sz-orm-tracing sz-orm-logger sz-orm-config sz-orm-limit sz-orm-sql-validator sz-orm-health sz-orm-audit sz-orm-masking"
BATCH_3="sz-orm-sqlx sz-orm-es sz-orm-oracle sz-orm-mssql sz-orm-postgis sz-orm-timeseries sz-orm-search sz-orm-vector"
BATCH_4="sz-orm-crypto sz-orm-auth sz-orm-mig sz-orm-back"
BATCH_5="sz-orm-queue sz-orm-batch sz-orm-storage sz-orm-dtx sz-orm-rw sz-orm-sharding sz-orm-observability"
BATCH_6="sz-orm-ai sz-orm-explain sz-orm-adaptive sz-orm-anomaly sz-orm-advisor sz-orm-diagnosis sz-orm-fusion sz-orm-n1-lint"
BATCH_7="sz-orm-ai-designer sz-orm-ai-migration sz-orm-nl-query sz-orm-agent sz-orm-governance sz-orm-multimodal sz-orm-mcp sz-orm-model-ops"
BATCH_8="sz-orm-axum sz-orm-actix sz-orm-grpc sz-orm-graphql sz-orm-websocket sz-orm-mqtt sz-orm-swagger sz-orm-stream"
BATCH_9="sz-orm-parallel sz-orm-bench sz-orm-studio sz-orm-lsp sz-orm-flamegraph sz-orm-wasm sz-orm-lc sz-orm-designer"
BATCH_10="sz-orm-cabi sz-orm-java sz-orm-go sz-orm-cpp sz-orm-cli sz-orm-query-builder sz-orm-scheduler sz-orm-graph"

BATCHES=(
    "$BATCH_0" "$BATCH_1" "$BATCH_2" "$BATCH_3" "$BATCH_4"
    "$BATCH_5" "$BATCH_6" "$BATCH_7" "$BATCH_8" "$BATCH_9" "$BATCH_10"
)

OUTPUT_DIR="coverage-batch"
mkdir -p "$OUTPUT_DIR"

TOTAL_PACKAGES=0
BATCH_NUM=0
ALL_LCOV_FILES=""

for BATCH in "${BATCHES[@]}"; do
    BATCH_NUM=$((BATCH_NUM + 1))
    PKG_COUNT=$(echo "$BATCH" | wc -w)
    TOTAL_PACKAGES=$((TOTAL_PACKAGES + PKG_COUNT))
    LCOV_FILE="$OUTPUT_DIR/batch_${BATCH_NUM}.lcov"

    echo "--- Batch $BATCH_NUM ($PKG_COUNT packages) ---"
    echo "Packages: $BATCH"
    START_TIME=$(date +%s)

    PKG_ARGS=""
    for PKG in $BATCH; do
        PKG_ARGS="$PKG_ARGS -p $PKG"
    done

    timeout 300 cargo llvm-cov $PKG_ARGS --lcov --output-path "$LCOV_FILE" 2>&1 | tail -3
    EXIT_CODE=$?

    END_TIME=$(date +%s)
    ELAPSED=$((END_TIME - START_TIME))

    if [ $EXIT_CODE -eq 0 ] && [ -f "$LCOV_FILE" ]; then
        echo "  ✅ Batch $BATCH_NUM completed in ${ELAPSED}s"
        if [ -n "$ALL_LCOV_FILES" ]; then
            ALL_LCOV_FILES="$ALL_LCOV_FILES $LCOV_FILE"
        else
            ALL_LCOV_FILES="$LCOV_FILE"
        fi
    elif [ $EXIT_CODE -eq 124 ]; then
        echo "  ⏰ Batch $BATCH_NUM timed out after 300s, skipping"
    else
        echo "  ❌ Batch $BATCH_NUM failed (exit $EXIT_CODE), skipping"
    fi
    echo ""
done

echo "=== Merging lcov reports ==="
MERGED_LCOV="$OUTPUT_DIR/merged.lcov"
> "$MERGED_LCOV"
for LCOV_FILE in $ALL_LCOV_FILES; do
    if [ -f "$LCOV_FILE" ]; then
        cat "$LCOV_FILE" >> "$MERGED_LCOV"
    fi
done

if [ ! -s "$MERGED_LCOV" ]; then
    echo "ERROR: Merged lcov is empty"
    exit 1
fi

echo "Parsing merged lcov..."
LINES_TOTAL=$(awk '/^LF:/ { total += $2 } END { print total + 0 }' "$MERGED_LCOV")
LINES_HIT=$(awk '/^LH:/ { hit += $2 } END { print hit + 0 }' "$MERGED_LCOV")

if [ "$LINES_TOTAL" -eq 0 ]; then
    echo "ERROR: LINES_TOTAL is 0"
    exit 1
fi

COVERAGE_PERCENT=$(awk -v hit="$LINES_HIT" -v total="$LINES_TOTAL" \
    'BEGIN { printf "%.2f", (hit / total) * 100 }')

echo ""
echo "=== Coverage Summary ==="
echo "  Total packages: $TOTAL_PACKAGES"
echo "  Total lines:    $LINES_TOTAL"
echo "  Hit lines:      $LINES_HIT"
echo "  Coverage:       ${COVERAGE_PERCENT}%"
echo "  Threshold:      ${FAIL_UNDER}%"
echo "  Date:           $DATE_UTC"
echo "  Tool:           $TOOL_VERSION"

PASS=$(awk -v cov="$COVERAGE_PERCENT" -v threshold="$FAIL_UNDER" \
    'BEGIN { print (cov >= threshold) ? "true" : "false" }')

if [ "$PASS" = "false" ] && [ "$FAIL_UNDER" -gt 0 ]; then
    echo ""
    echo "FAIL: Coverage ${COVERAGE_PERCENT}% < threshold ${FAIL_UNDER}%"
    echo ""
    echo "Top 20 least covered files:"
    awk '
        /^SF:/ { file = substr($0, 4) }
        /^LF:/ { lf = $2 }
        /^LH:/ { lh = $2; if (lf > 0) printf "%.2f%% %s (%d/%d)\n", (lh/lf)*100, file, lh, lf }
    ' "$MERGED_LCOV" | sort -n | head -20
    exit 1
fi

if [ "$FAIL_UNDER" -gt 0 ]; then
    echo "PASS: Coverage ${COVERAGE_PERCENT}% >= threshold ${FAIL_UNDER}%"
fi

echo ""
echo "Coverage collection complete. Reports in $OUTPUT_DIR/"