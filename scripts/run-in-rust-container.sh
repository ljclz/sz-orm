#!/bin/bash
# 此脚本在 rust:1.99 容器内执行
set -e

echo "=== Step 1: Verify OCI library ==="
ls /opt/oracle/instantclient/lib/libclntsh.so
ls /opt/oracle/instantclient/include/oci.h
echo "OCI library OK"

echo ""
echo "=== Step 2: Install libaio1 and libnuma1 ==="
# 尝试 apt-get，超时 120 秒
timeout 120 apt-get update -qq 2>&1 | tail -2 || echo "apt-get update failed/slow"
timeout 120 apt-get install -y -qq libaio1 libnuma1 2>&1 | tail -5 || echo "apt-get install failed/slow"

# 检查是否安装成功
if ! ldconfig -p | grep -q libaio; then
  echo "libaio not installed, trying to copy from workspace..."
  # 从 WSL 复制（通过挂载的 /workspace 不行，但可以检查 /lib）
  find / -name "libaio.so*" 2>/dev/null | head -3
fi

# 创建 libnuma symlink
mkdir -p /usr/lib64
ln -sf /usr/lib/x86_64-linux-gnu/libnuma.so.1 /usr/lib64/libnuma.so.1 2>/dev/null || true

echo ""
echo "=== Step 3: Set environment ==="
export LD_LIBRARY_PATH=/opt/oracle/instantclient/lib:${LD_LIBRARY_PATH:-}
export OCI_LIB_DIR=/opt/oracle/instantclient/lib
export OCI_INC_DIR=/opt/oracle/instantclient/include
export SZ_ORM_ORACLE_PASSWORD=OracleTest123
export SZ_ORM_ORACLE_USER=sz_orm_test
export SZ_ORM_ORACLE_CONNECT_STRING=host.docker.internal:1521/freepdb1

echo "LD_LIBRARY_PATH=$LD_LIBRARY_PATH"
echo "rustc: $(rustc --version)"
echo "glibc: $(ldd --version | head -1)"

echo ""
echo "=== Step 4: Compile test ==="
cd /workspace
cargo test -p sz-orm-core --test integration_oracle --no-run 2>&1

echo ""
echo "=== Step 5: Run test ==="
cargo test -p sz-orm-core --test integration_oracle -- --ignored --nocapture --test-threads=1 2>&1

echo ""
echo "=== All done ==="