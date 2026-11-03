#!/bin/bash
set -e

export LD_LIBRARY_PATH=/opt/oracle/instantclient/lib:${LD_LIBRARY_PATH:-}
export OCI_LIB_DIR=/opt/oracle/instantclient/lib
export OCI_INC_DIR=/opt/oracle/instantclient/include

echo "=== Environment ==="
echo "LD_LIBRARY_PATH=$LD_LIBRARY_PATH"
echo "OCI_LIB_DIR=$OCI_LIB_DIR"
echo "OCI_INC_DIR=$OCI_INC_DIR"
echo "libclntsh.so: $(ls $OCI_LIB_DIR/libclntsh.so 2>&1)"
echo "oci.h: $(ls $OCI_INC_DIR/oci.h 2>&1)"
echo ""

echo "=== Starting compile ==="
cd /root/sz-orm
cargo test -p sz-orm-core --test integration_oracle --no-run 2>&1

echo ""
echo "=== Compile complete ==="