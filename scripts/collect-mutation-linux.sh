#!/bin/bash
# Collect mutation test results on Linux using cargo-mutants.
#
# Usage: bash scripts/collect-mutation-linux.sh --packages sz-orm-core,sz-orm-sqlx
#
# Prerequisites: cargo-mutants installed (cargo install cargo-mutants)
# Note: cargo-mutants modifies source code in-place. After completion,
#       source is restored via git checkout.

set -euo pipefail

PACKAGES="sz-orm-core,sz-orm-sqlx,sz-orm-pool,sz-orm-query,sz-orm-model"
OUTPUT=""
BASELINE="70"

while [ $# -gt 0 ]; do
    case "$1" in
        --packages) PACKAGES="$2"; shift 2 ;;
        --output) OUTPUT="$2"; shift 2 ;;
        --baseline) BASELINE="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "=== Mutation Test Collection (Linux) ==="

if ! command -v cargo-mutants &>/dev/null; then
    echo "ERROR: cargo-mutants not installed"
    echo "Install: cargo install cargo-mutants"
    exit 1
fi

echo "Tool version: $(cargo mutants --version)"
echo "Date: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo "OS: $(uname -a)"
echo "Packages: $PACKAGES"
echo ""

IFS=',' read -ra PKG_ARRAY <<< "$PACKAGES"

echo "Running cargo-mutants..."
for pkg in "${PKG_ARRAY[@]}"; do
    echo "  Testing $pkg..."
    cargo mutants --package "$pkg" --in-place 2>&1 || true
done

echo ""
echo "Restoring source code..."
git checkout -- packages/ 2>/dev/null || true

echo ""
echo "Mutation test collection complete."
echo "Note: Check mutants.out/ for detailed results."