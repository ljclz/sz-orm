#!/bin/bash
set -e

echo "=== Create directory in container ==="
docker exec rust-oracle-test mkdir -p /opt/oracle/instantclient 2>&1

echo ""
echo "=== Copy OCI lib via tar pipe ==="
tar -cf - -C /opt/oracle/instantclient lib | docker exec -i rust-oracle-test tar -xf - -C /opt/oracle/instantclient 2>&1
echo "Lib copied"

echo ""
echo "=== Copy OCI include via tar pipe ==="
tar -cf - -C /opt/oracle/instantclient include | docker exec -i rust-oracle-test tar -xf - -C /opt/oracle/instantclient 2>&1
echo "Include copied"

echo ""
echo "=== Verify files ==="
docker exec rust-oracle-test bash -c "ls -la /opt/oracle/instantclient/lib/libclntsh.so && ls /opt/oracle/instantclient/include/oci.h && echo OK" 2>&1

echo ""
echo "=== Install libaio1 and libnuma1 ==="
docker exec rust-oracle-test bash -c "apt-get update -qq 2>&1 | tail -2 && apt-get install -y -qq libaio1 libnuma1 2>&1 | tail -5" 2>&1

echo ""
echo "=== Create libnuma symlink ==="
docker exec rust-oracle-test bash -c "mkdir -p /usr/lib64 && ln -sf /usr/lib/x86_64-linux-gnu/libnuma.so.1 /usr/lib64/libnuma.so.1 2>/dev/null; echo done" 2>&1

echo ""
echo "=== Verify Rust and glibc ==="
docker exec rust-oracle-test bash -c "rustc --version && ldd --version | head -1" 2>&1

echo ""
echo "=== Setup complete ==="