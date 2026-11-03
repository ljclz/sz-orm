#!/bin/bash
# 尝试多个 Oracle 镜像
images=(
  "gvenzl/oracle-free:23ai-full"
  "gvenzl/oracle-free:latest"
  "gvenzl/oracle-xe:21"
  "gvenzl/oracle-xe:21-slim"
  "oracleinanutshell/oracle-19c:latest"
  "container-registry.oracle.com/database/free:23ai"
  "wnameless/oracle-xe-11g-r2:latest"
)

for img in "${images[@]}"; do
  echo "=== Trying: $img ==="
  result=$(docker pull "$img" 2>&1 | tail -3)
  echo "$result"
  if echo "$result" | grep -q "Downloaded\|Pull complete"; then
    echo "SUCCESS: $img"
    break
  fi
  echo ""
done