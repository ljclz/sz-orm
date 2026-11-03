#!/bin/bash
set -e

echo "=== Starting container ==="
docker start oracle-test 2>&1
echo "Container started, waiting for ready..."

# 等待就绪
for i in $(seq 1 60); do
  log=$(docker logs oracle-test 2>&1 | tail -3)
  if echo "$log" | grep -q "DATABASE IS READY TO USE"; then
    echo "Database ready! (attempt $i)"
    break
  fi
  sleep 2
done

if ! echo "$log" | grep -q "DATABASE IS READY TO USE"; then
  echo "ERROR: Database not ready"
  docker logs oracle-test 2>&1 | tail -20
  exit 1
fi

sleep 3  # 额外等待确保完全就绪

echo ""
echo "=== Reset password ==="
printf "resetPassword OracleTest123\n" | docker exec -i oracle-test bash 2>&1 | tail -5

sleep 2

echo ""
echo "=== Creating sz_orm_test user ==="
printf "CREATE USER sz_orm_test IDENTIFIED BY OracleTest123;\nGRANT DBA TO sz_orm_test;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1

echo ""
echo "=== Verifying sz_orm_test user ==="
result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
echo "$result"

if echo "$result" | grep -q "1"; then
  echo ""
  echo "SUCCESS: sz_orm_test user created and verified!"
else
  echo ""
  echo "ERROR: User verification failed"
  exit 1
fi