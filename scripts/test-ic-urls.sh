#!/bin/bash
echo "=== Test Oracle Instant Client URLs ==="
urls=(
  "https://download.oracle.com/otn_software/linux/instantclient/193000/instantclient-basic-linux.x64-19.3.0.0.0.zip"
  "https://download.oracle.com/otn_software/linux/instantclient/1922000/instantclient-basic-linux.x64-19.22.0.0.0.zip"
  "https://download.oracle.com/otn_software/linux/instantclient/2110000/instantclient-basic-linux.x64-21.10.0.0.0.zip"
  "https://download.oracle.com/otn_software/linux/instantclient/2130000/instantclient-basic-linux.x64-21.13.0.0.0.zip"
  "https://download.oracle.com/otn_software/linux/instantclient/2340000/instantclient-basic-linux.x64-23.4.0.0.0.zip"
  "https://download.oracle.com/otn_software/linux/instantclient/2300000/instantclient-basic-linux.x64-23.0.0.0.0.zip"
)
for url in "${urls[@]}"; do
  echo "Testing: $url"
  code=$(curl -sI -m 15 -o /dev/null -w "%{http_code}" "$url" 2>&1)
  echo "  HTTP code: $code"
  if [ "$code" = "200" ]; then
    echo "  FOUND: $url"
  fi
  echo ""
done