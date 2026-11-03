#!/bin/bash
set -e

echo "=== Creating directories ==="
mkdir -p /opt/oracle/instantclient/lib
mkdir -p /opt/oracle/instantclient/include

echo "=== Copying lib from container ==="
docker cp oracle-test:/opt/oracle/product/26ai/dbhomeFree/lib/. /opt/oracle/instantclient/lib/ 2>&1
echo "Lib copied: $(ls /opt/oracle/instantclient/lib/libclntsh.so* 2>/dev/null | wc -l) clntsh files"

echo "=== Copying include from container ==="
docker cp oracle-test:/opt/oracle/product/26ai/dbhomeFree/rdbms/public/. /opt/oracle/instantclient/include/ 2>&1
echo "Include copied: $(ls /opt/oracle/instantclient/include/oci.h 2>/dev/null && echo 'oci.h present')"

echo "=== Checking libclntsh.so symlink ==="
cd /opt/oracle/instantclient/lib
if [ -f libclntsh.so ] || [ -L libclntsh.so ]; then
  echo "libclntsh.so exists"
else
  echo "Creating libclntsh.so symlink..."
  ln -s libclntsh.so.23.1 libclntsh.so
fi
ls -la libclntsh.so 2>&1

echo "=== Installing libaio1 ==="
apt-get update -qq 2>&1 | tail -2
apt-get install -y -qq libaio1 2>&1 | tail -3

echo "=== Verifying ==="
echo "libclntsh.so: $(ls /opt/oracle/instantclient/lib/libclntsh.so 2>&1)"
echo "oci.h: $(ls /opt/oracle/instantclient/include/oci.h 2>&1)"
echo "libaio: $(ldconfig -p | grep libaio | head -1)"

echo ""
echo "=== OCI library installation complete ==="