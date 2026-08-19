#!/usr/bin/env python3
"""术语一致性校验脚本（REQ-I18N-002/003）

校验英文文档中同一中文术语的译法是否统一。
用法：python scripts/check-glossary-consistency.py
"""

import re
import sys
from pathlib import Path
from collections import defaultdict

GLOSSARY_PATH = Path("docs/glossary-zh-en.md")
SEARCH_DIRS = [
    Path("README.md"),
    Path("docs"),
    Path("packages"),
]
INCLUDE_EXTS = {".md", ".rs"}

KNOWN_MIXES = {
    "connection pool": ["conn pool", "connection_pool", "pool"],
    "dialect": ["sql_dialect", "sql dialect"],
    "query builder": ["querybuilder", "query_builder"],
    "derive macro": ["derive_macro", "derivemacro"],
    "anomaly detection": ["anomaly_detection", "anomalydetection"],
    "sliding window": ["sliding_window", "slidingwindow"],
    "soft delete": ["softdelete", "soft_delete"],
    "multi-tenant": ["multitenant", "multi_tenant"],
    "zero-copy": ["zerocopy", "zero_copy"],
    "type-safe": ["typesafe", "type_safe"],
    "production ready": ["productionready", "production_ready"],
    "feature gate": ["featuregate", "feature_gate"],
    "cache coherence": ["cachecoherence", "cache_coherence"],
    "auto failover": ["autofailover", "auto_failover"],
    "row-level security": ["rowlevelsecurity", "row_level_security"],
}


def load_glossary() -> list[tuple[str, str]]:
    entries = []
    if not GLOSSARY_PATH.exists():
        return entries
    text = GLOSSARY_PATH.read_text(encoding="utf-8")
    for line in text.splitlines():
        m = re.match(r"\|\s*(.+?)\s*\|\s*(.+?)\s*\|", line)
        if m and not m.group(1).startswith("中文") and not m.group(1).startswith("-"):
            zh = m.group(1).strip()
            en = m.group(2).strip()
            if zh and en and not zh.startswith("#"):
                entries.append((zh, en))
    return entries


def find_english_files() -> list[Path]:
    files = []
    for target in SEARCH_DIRS:
        if target.is_file() and target.suffix in INCLUDE_EXTS:
            files.append(target)
        elif target.is_dir():
            for ext in INCLUDE_EXTS:
                files.extend(target.rglob(f"*{ext}"))
    return [f for f in files if ".zh." not in f.name and "glossary" not in f.name]


def check_consistency(files: list[Path]) -> list[str]:
    issues = []
    for standard, variants in KNOWN_MIXES.items():
        standard_count = 0
        variant_hits = defaultdict(list)
        for f in files:
            try:
                text = f.read_text(encoding="utf-8", errors="ignore")
            except Exception:
                continue
            lower = text.lower()
            standard_count += lower.count(standard.lower())
            for v in variants:
                if v.lower() in lower:
                    variant_hits[v].append(str(f))
        if variant_hits:
            for v, hits in variant_hits.items():
                issues.append(
                    f"TERM MIX: standard='{standard}' variant='{v}' "
                    f"in {len(hits)} files (e.g. {hits[0]})"
                )
    return issues


def main() -> int:
    glossary = load_glossary()
    print(f"Glossary entries: {len(glossary)}")
    if len(glossary) < 50:
        print(f"FAIL: glossary has {len(glossary)} entries, need >= 50")
        return 1

    files = find_english_files()
    print(f"Scanning {len(files)} English files...")

    issues = check_consistency(files)
    if issues:
        print(f"FAIL: {len(issues)} consistency issues found:")
        for i in issues:
            print(f"  {i}")
        return 1

    print("PASS: terminology consistent across all English files")
    return 0


if __name__ == "__main__":
    sys.exit(main())