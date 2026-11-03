#!/bin/bash
# 备份原配置
cp /etc/docker/daemon.json /etc/docker/daemon.json.bak 2>/dev/null

# 写入新配置
cat > /etc/docker/daemon.json <<'EOF'
{
  "registry-mirrors": [
    "https://docker.1panel.live",
    "https://docker.1ms.run",
    "https://docker.m.daocloud.io"
  ]
}
EOF

echo "New daemon.json:"
cat /etc/docker/daemon.json

# 重启 Docker
echo "Restarting docker..."
service docker restart 2>&1 || systemctl restart docker 2>&1

sleep 5
echo "Docker status:"
docker version --format "{{.Server.Version}}" 2>&1