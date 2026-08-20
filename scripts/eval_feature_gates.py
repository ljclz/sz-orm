import re, os

with open('packages/sz-orm-core/Cargo.toml', 'r', encoding='utf-8', errors='replace') as f:
    content = f.read()
match = re.search(r'\[features\](.*?)(?:\n\[|\Z)', content, re.DOTALL)
features = {}
for line in match.group(1).strip().split('\n'):
    line = line.strip()
    if '=' in line and not line.startswith('#'):
        name = line.split('=')[0].strip()
        deps = line.split('=', 1)[1].strip().strip('[]').strip()
        features[name] = deps

src_dir = 'packages'
feature_usage = {f: 0 for f in features}

for root, dirs, files in os.walk(src_dir):
    if 'target' in root:
        continue
    for fname in files:
        if not fname.endswith('.rs'):
            continue
        fpath = os.path.join(root, fname)
        try:
            with open(fpath, 'r', encoding='utf-8', errors='replace') as f:
                text = f.read()
            for feat in features:
                pattern = r'cfg\(feature\s*=\s*"' + re.escape(feat) + r'"\)'
                count = len(re.findall(pattern, text))
                feature_usage[feat] += count
        except:
            pass

print(f"{'Feature':<35} {'cfg refs':<10} {'Status'}")
print('-' * 60)
no_refs = []
for name in sorted(features.keys()):
    refs = feature_usage[name]
    deps = features[name]
    if name == 'default':
        status = 'default'
    elif refs > 0:
        status = f'USED({refs})'
    elif deps and deps != '[]':
        status = 'composite-alias'
    else:
        status = 'NO-REFS'
        no_refs.append(name)
    print(f'{name:<35} {refs:<10} {status}')

print(f'\nTotal: {len(features)}')
print(f'With cfg refs: {sum(1 for v in feature_usage.values() if v > 0)}')
print(f'NO cfg refs (potential phantom): {len(no_refs)}')
if no_refs:
    print(f'\nNo-ref features: {no_refs}')