"""Check sz-orm-* packages published to crates.io."""
import json
import urllib.request

URL = "https://crates.io/api/v1/crates?q=sz-orm&per_page=100"
req = urllib.request.Request(URL, headers={"User-Agent": "sz-orm-check"})
with urllib.request.urlopen(req, timeout=30) as resp:
    data = json.loads(resp.read().decode("utf-8"))

total = data["meta"]["total"]
print(f"Total sz-orm-* packages on crates.io: {total}")
print()
for c in data["crates"]:
    name = c["id"]
    version = c["max_version"]
    downloads = c["downloads"]
    print(f"  - {name} v{version} (downloads: {downloads})")

# Check workspace packages
import os
workspace = r"e:\vue\test\鲜视达\rust\sz-orm\packages"
if os.path.isdir(workspace):
    local_pkgs = sorted(d for d in os.listdir(workspace) if d.startswith("sz-orm-"))
    published_names = {c["id"] for c in data["crates"]}
    print()
    print(f"Local packages: {len(local_pkgs)}")
    missing = [p for p in local_pkgs if p not in published_names]
    print(f"Missing on crates.io: {len(missing)}")
    for m in missing:
        print(f"  ! {m}")
