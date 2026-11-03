#!/bin/bash
# 外层脚本：确保环境就绪并执行测试

echo "=== Step 0: Ensure oracle-test database ready ==="
for i in $(seq 1 60); do
  result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
  if echo "$result" | grep -q "1"; then
    echo "Database ready! (attempt $i)"
    break
  fi
  # 尝试 sys 重置
  sys_result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1)
  if echo "$sys_result" | grep -q "1"; then
    docker exec oracle-test resetPassword OracleTest123 2>&1 | tail -1
    sleep 1
    printf "BEGIN\nEXECUTE IMMEDIATE 'CREATE USER sz_orm_test IDENTIFIED BY OracleTest123';\nEXCEPTION WHEN OTHERS THEN NULL;\nEND;\n/\nGRANT DBA TO sz_orm_test;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1 | tail -1
    sleep 1
  fi
  sleep 3
done

echo ""
echo "=== Step 1: Ensure rust-oracle-test container running ==="
if ! docker ps --filter "name=rust-oracle-test" --format "{{.Status}}" | grep -q "^Up"; then
  echo "Starting rust-oracle-test..."
  docker start rust-oracle-test 2>&1
  sleep 3
fi
docker ps --filter "name=rust-oracle-test" --format "rust-oracle-test: {{.Status}}"

echo ""
echo "=== Step 2: Execute test script in rust container ==="
docker exec rust-oracle-test bash /workspace/scripts/run-in-rust-container.sh 2>&1

echo ""
echo "=== Done ==="