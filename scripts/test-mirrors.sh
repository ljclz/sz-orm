#!/bin/bash
for mirror in "docker.1ms.run" "dockerpull.com" "dockerproxy.cn" "docker.1panel.live" "hub.atomgit.com" "docker.nju.edu.cn" "docker.m.daocloud.io" "docker.mirrors.ustc.edu.cn" "hub-mirror.c.163.com"; do
  echo "Testing $mirror..."
  result=$(curl -sI -m 8 "https://$mirror/v2/" 2>&1 | head -1)
  echo "  -> $result"
done