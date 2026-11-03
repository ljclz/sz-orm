#!/bin/bash

# 确保容器运行
echo "=== Checking container ==="
status=$(docker ps --filter "name=oracle-test" --format "{{.Status}}")
echo "Container status: $status"

if ! echo "$status" | grep -q "^Up"; then
  echo "Container not running, starting..."
  docker start oracle-test 2>&1
  echo "Waiting for database ready..."
  for i in $(seq 1 60); do
    if docker logs oracle-test 2>&1 | grep -q "DATABASE IS READY TO USE"; then
      echo "Database ready! (attempt $i)"
      break
    fi
    sleep 2
  done
  sleep 3
fi

# 重置密码（容器可能重启过）
echo "=== Reset password ==="
docker exec oracle-test resetPassword OracleTest123 2>&1 | tail -3
sleep 2

# 验证连接
echo "=== Verify connection ==="
result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
echo "Connection result: $result"

if ! echo "$result" | grep -q "1"; then
  echo "ERROR: Cannot connect to Oracle as sz_orm_test"
  echo "Trying to recreate user..."
  printf "CREATE USER sz_orm_test IDENTIFIED BY OracleTest123;\nGRANT DBA TO sz_orm_test;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1
fi

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