import os, re
from collections import defaultdict, deque

root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
pkg_dir = os.path.join(root, "packages")

pkgs = set()
deps = defaultdict(set)

for name in os.listdir(pkg_dir):
    cargo = os.path.join(pkg_dir, name, "Cargo.toml")
    if not os.path.isfile(cargo):
        continue
    pkgs.add(name)
    with open(cargo, "r", encoding="utf-8-sig") as f:
        content = f.read()

    in_deps = False
    in_dev = False
    for line in content.splitlines():
        stripped = line.strip()
        if stripped.startswith("[dependencies"):
            in_deps = True
            in_dev = False
            continue
        if stripped.startswith("[dev-dependencies"):
            in_deps = False
            in_dev = True
            continue
        if stripped.startswith("[build-dependencies"):
            in_deps = False
            in_dev = True
            continue
        if stripped.startswith("[") and stripped.endswith("]"):
            in_deps = False
            in_dev = False
            continue
        if in_deps and not in_dev:
            if stripped.startswith("#"):
                continue
            for m in re.finditer(r'sz-orm-[a-z0-9-]+', stripped):
                dep = m.group()
                if dep != name:
                    deps[name].add(dep)

pkgs = sorted(pkgs)
indegree = defaultdict(int)
adj = defaultdict(set)
for p in pkgs:
    for d in deps[p]:
        if d in pkgs:
            adj[d].add(p)
            indegree[p] += 1

queue = deque(sorted([p for p in pkgs if indegree[p] == 0]))
result = []
while queue:
    node = queue.popleft()
    result.append(node)
    for dependent in sorted(adj[node]):
        indegree[dependent] -= 1
        if indegree[dependent] == 0:
            queue.append(dependent)

if len(result) != len(pkgs):
    remaining = set(pkgs) - set(result)
    print(f"CYCLE DETECTED: {len(remaining)} packages in cycle")
    for r in sorted(remaining):
        print(f"  {r} <- {sorted(deps[r])}")
else:
    for i, pkg in enumerate(result, 1):
        d = sorted(deps[pkg])
        print(f"{i:3d}. {pkg}" + (f"  <- {d}" if d else ""))
    print(f"\nTotal: {len(result)} packages")
    with open(os.path.join(os.path.dirname(__file__), "topo-order.txt"), "w") as f:
        for pkg in result:
            f.write(pkg + "\n")
