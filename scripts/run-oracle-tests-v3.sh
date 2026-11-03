#!/bin/bash

# 等待数据库完全就绪
echo "=== Waiting for database ready ==="
for i in $(seq 1 90); do
  result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
  if echo "$result" | grep -q "1"; then
    echo "Database ready, sz_orm_test connected! (attempt $i)"
    break
  fi
  # 尝试 sys 连接
  sys_result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1)
  if echo "$sys_result" | grep -q "1"; then
    echo "sys connected, ensuring sz_orm_test user... (attempt $i)"
    docker exec oracle-test resetPassword OracleTest123 2>&1 | tail -1
    sleep 1
    printf "BEGIN\nEXECUTE IMMEDIATE 'CREATE USER sz_orm_test IDENTIFIED BY OracleTest123';\nEXCEPTION WHEN OTHERS THEN NULL;\nEND;\n/\nGRANT DBA TO sz_orm_test;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1 | tail -2
    sleep 1
  fi
  sleep 3
done

# 最终验证
result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
if ! echo "$result" | grep -q "1"; then
  echo "ERROR: Cannot connect as sz_orm_test"
  exit 1
fi
echo "Connection verified: $result"

echo ""
echo "=== Running Oracle integration tests ==="
export LD_LIBRARY_PATH=/opt/oracle/instantclient/lib:${LD_LIBRARY_PATH:-}
export OCI_LIB_DIR=/opt/oracle/instantclient/lib
export OCI_INC_DIR=/opt/oracle/instantclient/include
export SZ_ORM_ORACLE_PASSWORD=OracleTest123
export SZ_ORM_ORACLE_USER=sz_orm_test
export SZ_ORM_ORACLE_CONNECT_STRING=127.0.0.1:1521/freepdb1

cd /root/sz-orm
cargo test -p sz-orm-core --test integration_oracle -- --ignored --nocapture --test-threads=1 2>&1

echo ""
echo "=== Tests complete ==="