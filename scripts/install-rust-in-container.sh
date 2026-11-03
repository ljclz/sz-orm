#!/bin/bash
set -e

echo "=== Installing build tools in container ==="
docker exec oracle-test bash -c "dnf install -y gcc make curl tar 2>&1 | tail -5"

echo ""
echo "=== Installing Rust in container ==="
docker exec oracle-test bash -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.99.1 2>&1 | tail -10"

echo ""
echo "=== Verify Rust ==="
docker exec oracle-test bash -c "source ~/.cargo/env && rustc --version && cargo --version" 2>&1

echo ""
echo "=== Rust installation complete ==="