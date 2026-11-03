#!/bin/bash

# 确保容器运行并完全就绪
echo "=== Ensuring container is running ==="
status=$(docker ps --filter "name=oracle-test" --format "{{.Status}}")
echo "Container status: $status"

if ! echo "$status" | grep -q "^Up"; then
  echo "Starting container..."
  docker start oracle-test 2>&1
fi

echo "Waiting for database fully ready (freepdb1 registered)..."
start=$(date +%s)
for i in $(seq 1 90); do
  # 检查 freepdb1 服务是否注册
  result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1)
  if echo "$result" | grep -q "^.*1$"; then
    end=$(date +%s)
    echo "Database fully ready! (took $((end-start)) seconds)"
    break
  fi
  echo "  waiting... ($i) - $(echo "$result" | head -2 | tail -1)"
  sleep 3
done

# 最终验证
result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1)
if ! echo "$result" | grep -q "1"; then
  echo "ERROR: Database still not ready"
  docker logs oracle-test 2>&1 | tail -20
  exit 1
fi

# 重置密码
echo ""
echo "=== Reset password ==="
docker exec oracle-test resetPassword OracleTest123 2>&1 | tail -3
sleep 2

# 验证 sz_orm_test 用户
echo "=== Verify sz_orm_test user ==="
result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
echo "sz_orm_test connection: $result"

if ! echo "$result" | grep -q "1"; then
  echo "sz_orm_test user not found, creating..."
  printf "CREATE USER sz_orm_test IDENTIFIED BY OracleTest123;\nGRANT DBA TO sz_orm_test;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1
  sleep 1
  result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
  echo "After create: $result"
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