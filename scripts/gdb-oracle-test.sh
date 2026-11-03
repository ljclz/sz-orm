#!/bin/bash

# 等待数据库就绪
echo "=== Waiting for database ready ==="
for i in $(seq 1 60); do
  result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
  if echo "$result" | grep -q "1"; then
    echo "Database ready! (attempt $i)"
    break
  fi
  sleep 3
done

echo ""
echo "=== Running test with gdb ==="
export LD_LIBRARY_PATH=/opt/oracle/instantclient/lib:${LD_LIBRARY_PATH:-}
export OCI_LIB_DIR=/opt/oracle/instantclient/lib
export OCI_INC_DIR=/opt/oracle/instantclient/include
export SZ_ORM_ORACLE_PASSWORD=OracleTest123
export SZ_ORM_ORACLE_USER=sz_orm_test
export SZ_ORM_ORACLE_CONNECT_STRING=127.0.0.1:1521/freepdb1

cd /root/sz-orm

# 用 gdb 运行，捕获 SIGSEGV backtrace
gdb -batch -ex "set env LD_LIBRARY_PATH=$LD_LIBRARY_PATH" \
    -ex "set env SZ_ORM_ORACLE_PASSWORD=$SZ_ORM_ORACLE_PASSWORD" \
    -ex "set env SZ_ORM_ORACLE_USER=$SZ_ORM_ORACLE_USER" \
    -ex "set env SZ_ORM_ORACLE_CONNECT_STRING=$SZ_ORM_ORACLE_CONNECT_STRING" \
    -ex "run --ignored --nocapture --test-threads=1 test_oracle_create_insert_select" \
    -ex "bt full" \
    -ex "info sharedlibrary" \
    /root/sz-orm/target/debug/deps/integration_oracle-4280ecb41a2b8e62 2>&1 | tail -80