#!/usr/bin/env python3
"""Register unused dependencies to cargo-machete ignored list."""

import re
from pathlib import Path

# 63 unused dependencies (package -> [deps])
UNUSED_DEPS = {
    "sz-orm-actix": ["futures"],
    "sz-orm-ai": ["base64"],
    "sz-orm-auth": ["async-trait", "thiserror", "tokio"],
    "sz-orm-axum": ["futures", "tokio", "tower"],
    "sz-orm-back": ["async-trait", "thiserror"],
    "sz-orm-crypto": ["serde_json"],
    "sz-orm-dtx": ["prost", "sz-orm-grpc", "tonic-prost"],
    "sz-orm-es": ["chrono"],
    "sz-orm-fusion": ["serde"],
    "sz-orm-graph": ["async-trait", "tracing"],
    "sz-orm-graphql": ["sz-orm-macros", "sz-orm-queue"],
    "sz-orm-grpc": ["prost", "tonic-prost"],
    "sz-orm-js": ["tokio"],
    "sz-orm-lc": ["parking_lot", "sz-orm-designer", "tokio", "tracing"],
    "sz-orm-mig": ["async-trait", "thiserror"],
    "sz-orm-mqtt": ["async-trait", "thiserror"],
    "sz-orm-mssql": ["tracing"],
    "sz-orm-observability": ["async-trait", "chrono", "sz-orm-core", "sz-orm-limit", "thiserror"],
    "sz-orm-oracle": ["tracing"],
    "sz-orm-parallel": ["sz-orm-adaptive", "sz-orm-core"],
    "sz-orm-postgis": ["serde_json", "thiserror"],
    "sz-orm-python": ["pyo3-asyncio"],
    "sz-orm-queue": ["chrono", "thiserror"],
    "sz-orm-rw": ["tokio"],
    "sz-orm-scheduler": ["async-trait", "tokio"],
    "sz-orm-search": ["chrono", "thiserror"],
    "sz-orm-sharding": ["serde_json"],
    "sz-orm-storage": ["rust-s3", "thiserror"],
    "sz-orm-swagger": ["sz-orm-sqlx"],
    "sz-orm-timeseries": ["thiserror"],
    "sz-orm-tracing": ["tokio"],
    "sz-orm-vector": ["serde", "sz-orm-ai"],
    "sz-orm-wasm": ["chrono", "tokio-tungstenite", "web-sys"],
    "sz-orm-websocket": ["thiserror"],
}

BASE_DIR = Path(__file__).parent.parent / "packages"

def register_ignored(pkg_name: str, deps: list[str]) -> bool:
    """Add [package.metadata.cargo-machete] ignored = [...] to Cargo.toml."""
    cargo_toml = BASE_DIR / pkg_name / "Cargo.toml"
    if not cargo_toml.exists():
        print(f"  SKIP {pkg_name}: Cargo.toml not found")
        return False

    content = cargo_toml.read_text(encoding="utf-8")

    # Check if already has cargo-machete section
    if "[package.metadata.cargo-machete]" in content:
        # Already has the section, check if deps are already listed
        print(f"  SKIP {pkg_name}: already has cargo-machete section")
        return False

    # Add the section before [dependencies] or at end of [package]
    deps_str = ", ".join(f"'{d}'" for d in deps)
    section = f'\n[package.metadata.cargo-machete]\nignored = [{deps_str}]\n'

    # Insert before [dependencies] if exists, else at end
    if "[dependencies]" in content:
        content = content.replace("[dependencies]", section + "[dependencies]")
    else:
        content += section

    cargo_toml.write_text(content, encoding="utf-8")
    print(f"  OK  {pkg_name}: registered {len(deps)} deps")
    return True

def main():
    print("=" * 60)
    print("  Registering unused dependencies to cargo-machete")
    print("=" * 60)

    ok_count = 0
    skip_count = 0

    for pkg_name, deps in sorted(UNUSED_DEPS.items()):
        if register_ignored(pkg_name, deps):
            ok_count += 1
        else:
            skip_count += 1

    print()
    print(f"Done: {ok_count} registered, {skip_count} skipped")

if __name__ == "__main__":
    main()
