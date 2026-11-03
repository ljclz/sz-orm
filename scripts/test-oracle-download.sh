#!/bin/bash
echo "=== Test Oracle download URLs ==="
urls=(
  "https://download.oracle.com/otn_software/linux/instantclient/1930000/instantclient-basic-linux.x64-19.3.0.0.0.zip"
  "https://download.oracle.com/otn_software/linux/instantclient/2113000/instantclient-basic-linux.x64-21.13.0.0.0.zip"
  "https://download.oracle.com/otn_software/linux/instantclient/2390000/instantclient-basic-linux.x64-23.9.0.0.0.zip"
)
for url in "${urls[@]}"; do
  echo "Testing: $url"
  result=$(curl -sI -m 15 "$url" 2>&1 | head -3)
  echo "  -> $result"
  echo ""
done