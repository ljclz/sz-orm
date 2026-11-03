#!/bin/bash
echo "=== Creating sz_orm_test user ==="
printf "CREATE USER sz_orm_test IDENTIFIED BY OracleTest123;\nGRANT DBA TO sz_orm_test;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sys/OracleTest123@//localhost:1521/freepdb1 as sysdba 2>&1
echo ""
echo "=== Verifying sz_orm_test user ==="
printf "SET PAGESIZE 0;\nSELECT 1 FROM dual;\nEXIT;\n" | docker exec -i oracle-test sqlplus -s sz_orm_test/OracleTest123@//localhost:1521/freepdb1 2>&1