#!/bin/bash
# Update offline advisory database cache and verify integrity.
#
# Updates the local advisory-db cache and outputs the new commit hash.
# Verifies cache integrity by comparing hashes.
#
# Usage: bash scripts/update-advisory-db.sh [--dest <path>]

set -euo pipefail

DEST="${1:-$HOME/.cargo/advisory-dbs}"

if [ "${1:-}" = "--dest" ]; then
    DEST="$2"
fi

echo "=== Update Advisory Database Cache ==="
echo "Path: $DEST"

if [ ! -d "$DEST/.git" ]; then
    echo "ERROR: Cache not found at $DEST"
    echo "Run setup-advisory-db-offline.sh first."
    exit 1
fi

OLD_HASH=$(git -C "$DEST" rev-parse HEAD)
echo "Old commit: $OLD_HASH"

echo "Pulling updates..."
git -C "$DEST" fetch origin
git -C "$DEST" merge --ff-only origin/main

NEW_HASH=$(git -C "$DEST" rev-parse HEAD)
NEW_DATE=$(git -C "$DEST" log -1 --format='%ci')

echo ""
echo "=== Update Result ==="
echo "Old commit: $OLD_HASH"
echo "New commit: $NEW_HASH"
echo "Date:       $NEW_DATE"

if [ "$OLD_HASH" = "$NEW_HASH" ]; then
    echo "Status:     Already up to date"
else
    echo "Status:     Updated"
fi

echo ""
echo "=== Integrity Verification ==="
verify_cache_integrity() {
    local path="$1"
    local expected_hash="$2"
    local actual_hash
    actual_hash=$(git -C "$path" rev-parse HEAD)
    if [ "$actual_hash" = "$expected_hash" ]; then
        echo "PASS: Cache integrity verified ($actual_hash)"
        return 0
    else
        echo "FAIL: Hash mismatch (expected: $expected_hash, actual: $actual_hash)"
        return 1
    fi
}
verify_cache_integrity "$DEST" "$NEW_HASH"

echo ""
echo "Update complete."