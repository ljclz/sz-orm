#!/bin/bash
# Setup offline advisory database cache for cargo-deny.
#
# Clones RustSec advisory-db to local directory and configures deny.toml
# to use the local cache, enabling cargo-deny to run without network.
#
# Usage: bash scripts/setup-advisory-db-offline.sh --dest .cargo/advisory-dbs

set -euo pipefail

DEST="${1:-$HOME/.cargo/advisory-dbs}"
ADVISORY_DB_URL="https://github.com/rustsec/advisory-db"

if [ "${1:-}" = "--dest" ]; then
    DEST="$2"
fi

echo "=== Setup Offline Advisory Database ==="
echo "Destination: $DEST"

if [ -d "$DEST/.git" ]; then
    echo "Cache already exists, pulling latest..."
    git -C "$DEST" pull --ff-only
else
    echo "Cloning advisory-db..."
    git clone --depth 1 "$ADVISORY_DB_URL" "$DEST"
fi

COMMIT_HASH=$(git -C "$DEST" rev-parse HEAD)
COMMIT_DATE=$(git -C "$DEST" log -1 --format='%ci')
echo ""
echo "=== Cache Info ==="
echo "Path: $DEST"
echo "Commit: $COMMIT_HASH"
echo "Date:  $COMMIT_DATE"
echo ""
echo "To use offline, set in deny.toml:"
echo "  [advisories]"
echo "  db-path = \"$DEST\""
echo ""
echo "Setup complete."