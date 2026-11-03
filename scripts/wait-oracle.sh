#!/bin/bash
echo "Waiting for Oracle database to be ready..."
start=$(date +%s)

for i in $(seq 1 120); do
  # 检查容器日志中是否有就绪标志
  log=$(docker logs oracle-test 2>&1 | tail -5)
  if echo "$log" | grep -q "DATABASE IS READY TO USE\|database is ready\|ready to use"; then
    end=$(date +%s)
    echo "Oracle database is ready! (took $((end-start)) seconds)"
    docker logs oracle-test 2>&1 | tail -10
    exit 0
  fi
  
  # 也尝试用 sqlplus 检查
  result=$(docker exec oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba <<< "SELECT 1 FROM dual;" 2>/dev/null)
  if echo "$result" | grep -q "^1"; then
    end=$(date +%s)
    echo "Oracle database is ready (sqlplus check)! (took $((end-start)) seconds)"
    exit 0
  fi
  
  echo "waiting... ($i) - $(echo "$log" | tail -1)"
  sleep 3
done

end=$(date +%s)
echo "TIMEOUT after $((end-start)) seconds"
echo "Last logs:"
docker logs oracle-test 2>&1 | tail -20
exit 1