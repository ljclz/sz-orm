#!/bin/bash

# 等待数据库完全就绪
echo "=== Waiting for database ready ==="
for i in $(seq 1 90); do
  result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
  if echo "$result" | grep -q "1"; then
    echo "Database ready, sz_orm_test connected! (attempt $i)"
    break
  fi
  # 如果 sz_orm_test 连不上，尝试 sys
  sys_result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1)
  if echo "$sys_result" | grep -q "1"; then
    echo "sys connected, resetting password and recreating user... (attempt $i)"
    docker exec oracle-test resetPassword OracleTest123 2>&1 | tail -1
    sleep 1
    # 尝试创建用户（可能已存在）
    printf "BEGIN\nEXECUTE IMMEDIATE 'CREATE USER sz_orm_test IDENTIFIED BY OracleTest123';\nEXCEPTION WHEN OTHERS THEN NULL;\nEND;\n/\nGRANT DBA TO sz_orm_test;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1 | tail -2
    sleep 1
  fi
  sleep 3
done

# 最终验证
result=$(printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1)
echo "Final connection check: $result"
if ! echo "$result" | grep -q "1"; then
  echo "ERROR: Cannot connect as sz_orm_test"
  exit 1
fi

echo ""
echo "=== Running test with strace (last 50 lines) ==="
export LD_LIBRARY_PATH=/opt/oracle/instantclient/lib:${LD_LIBRARY_PATH:-}
export OCI_LIB_DIR=/opt/oracle/instantclient/lib
export OCI_INC_DIR=/opt/oracle/instantclient/include
export SZ_ORM_ORACLE_PASSWORD=OracleTest123
export SZ_ORM_ORACLE_USER=sz_orm_test
export SZ_ORM_ORACLE_CONNECT_STRING=127.0.0.1:1521/freepdb1

cd /root/sz-orm
# 用 strace 运行，捕获信号
timeout 60 strace -f -e trace=mmap,open,openat,connect,signal,kill -o /tmp/oracle-strace.log /root/sz-orm/target/debug/deps/integration_oracle-4280ecb41a2b8e62 --ignored --nocapture --test-threads=1 test_oracle_create_insert_select 2>&1
exit_code=$?
echo "Exit code: $exit_code"
echo ""
echo "=== Last strace lines ==="
tail -30 /tmp/oracle-strace.log 2>/dev/null