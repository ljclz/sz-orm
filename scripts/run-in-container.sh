#!/bin/bash

# 确保数据库就绪
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
echo "=== Copy test binary to container ==="
docker cp /root/sz-orm/target/debug/deps/integration_oracle-4280ecb41a2b8e62 oracle-test:/tmp/integration_oracle 2>&1
docker exec oracle-test chmod +x /tmp/integration_oracle 2>&1

echo ""
echo "=== Run test in container ==="
docker exec -e SZ_ORM_ORACLE_PASSWORD=OracleTest123 \
           -e SZ_ORM_ORACLE_USER=sz_orm_test \
           -e SZ_ORM_ORACLE_CONNECT_STRING=127.0.0.1:1521/freepdb1 \
           -e LD_LIBRARY_PATH=/opt/oracle/product/26ai/dbhomeFree/lib \
           oracle-test /tmp/integration_oracle --ignored --nocapture --test-threads=1 2>&1

echo ""
echo "Exit code: $?"