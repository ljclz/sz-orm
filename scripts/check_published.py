"""Check sz-orm-* packages published to crates.io."""
import json
import os
import socket
import urllib.request
from urllib.parse import urlparse

# 固定查询 crates.io 官方 API；仅允许 https 且目标主机为 crates.io
URL = "https://crates.io/api/v1/crates?q=sz-orm&per_page=100"
ALLOWED_HOST = "crates.io"


def _safe_fetch(url: str, timeout: int = 30) -> bytes:
    """SSRF 防护：仅允许 HTTPS + 白名单主机，解析 IP 后阻断私网/环回/链路本地地址。"""
    parsed = urlparse(url)
    if parsed.scheme != "https":
        raise ValueError(f"只允许 HTTPS 请求: {parsed.scheme}")
    host = (parsed.hostname or "").lower()
    if host != ALLOWED_HOST:
        raise ValueError(f"不允许的目标主机: {host}")

    # 解析 IP 并阻断内网/环回/链路本地（防 DNS rebinding）
    try:
        infos = socket.getaddrinfo(host, None)
    except OSError as exc:
        raise ValueError(f"DNS 解析失败: {host}: {exc}") from exc
    for info in infos:
        ip = info[4][0]
        if ip.startswith(("127.", "10.", "192.168.", "172.16.", "169.254.", "0.", "::1", "fe80::")):
            raise ValueError(f"目标解析到内网/链路本地地址: {ip}")

    req = urllib.request.Request(url, headers={"User-Agent": "sz-orm-check"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return resp.read()


data = json.loads(_safe_fetch(URL).decode("utf-8"))

total = data["meta"]["total"]
print(f"Total sz-orm-* packages on crates.io: {total}")
print()
for c in data["crates"]:
    name = c["id"]
    version = c["max_version"]
    downloads = c["downloads"]
    print(f"  - {name} v{version} (downloads: {downloads})")

# Check workspace packages（相对脚本定位，避免硬编码旧路径）
workspace = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "packages")
if os.path.isdir(workspace):
    local_pkgs = sorted(d for d in os.listdir(workspace) if d.startswith("sz-orm-"))
    published_names = {c["id"] for c in data["crates"]}
    print()
    print(f"Local packages: {len(local_pkgs)}")
    missing = [p for p in local_pkgs if p not in published_names]
    print(f"Missing on crates.io: {len(missing)}")
    for m in missing:
        print(f"  ! {m}")