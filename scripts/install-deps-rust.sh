#!/bin/bash
set -e

echo "=== Switch to fast mirror ==="
docker exec rust-oracle-test bash -c 'cat > /etc/apt/sources.list.d/debian.sources <<EOF
Types: deb
URIs: http://deb.debian.org/debian
Suites: trixie trixie-updates
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
EOF
rm -f /etc/apt/sources.list 2>/dev/null
apt-get update -qq 2>&1 | tail -3' 2>&1

echo ""
echo "=== Install libaio1 and libnuma1 ==="
docker exec rust-oracle-test bash -c 'apt-get install -y -qq libaio1 libnuma1 2>&1 | tail -5' 2>&1

echo ""
echo "=== Create libnuma symlink ==="
docker exec rust-oracle-test bash -c 'mkdir -p /usr/lib64 && ln -sf /usr/lib/x86_64-linux-gnu/libnuma.so.1 /usr/lib64/libnuma.so.1 2>/dev/null; echo done' 2>&1

echo ""
echo "=== Verify ==="
docker exec rust-oracle-test bash -c 'rustc --version && ldd --version | head -1 && ldconfig -p | grep -E "libaio|libnuma"' 2>&1

echo ""
echo "=== Setup complete ==="