#!/bin/bash
set -e

echo "=== Verify OCI files ==="
docker exec rust-oracle-test bash -c "ls /opt/oracle/instantclient/lib/libclntsh.so && ls /opt/oracle/instantclient/include/oci.h && echo OK" 2>&1

echo ""
echo "=== Install libaio1 and libnuma1 ==="
docker exec rust-oracle-test bash -c "apt-get update -qq 2>&1 | tail -2 && apt-get install -y -qq libaio1 libnuma1 2>&1 | tail -5" 2>&1

echo ""
echo "=== Create libnuma symlink ==="
docker exec rust-oracle-test bash -c "mkdir -p /usr/lib64 && ln -sf /usr/lib/x86_64-linux-gnu/libnuma.so.1 /usr/lib64/libnuma.so.1 2>/dev/null; echo done" 2>&1

echo ""
echo "=== Verify Rust ==="
docker exec rust-oracle-test bash -c "rustc --version && cargo --version" 2>&1

echo ""
echo "=== Check glibc version ==="
docker exec rust-oracle-test bash -c "ldd --version | head -1" 2>&1

echo ""
echo "=== Setup complete ==="