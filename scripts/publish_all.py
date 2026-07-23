"""SZ-ORM crates.io auto-publish script (Python version).

Publishes each sz-orm-* package one at a time, handling rate limits by parsing
the `try again after <RFC1123 date>` hint from the 429 response and waiting.

Usage:
    python scripts/publish_all.py

Requires:
    - cargo login already done (token stored in ~/.cargo/credentials)
    - All package Cargo.toml have proper metadata (workspace inheritance)
"""

from __future__ import annotations

import datetime as dt
import os
import re
import subprocess
import sys
import time
from pathlib import Path

WORKSPACE = Path(r"e:\vue\test\鲜视达\rust\sz-orm")

# Publish order following dependency graph (dependees before dependents).
# Already published packages are detected via "already exists" and skipped.
PACKAGES = [
    # Foundation (already published - kept for skip detection)
    "sz-orm-macros",
    "sz-orm-sql-validator",
    "sz-orm-ai",
    "sz-orm-core",
    "sz-orm-query-builder",
    "sz-orm-observability",
    "sz-orm-crypto",
    "sz-orm-sqlx",
    "sz-orm-vector",
    # Remaining 28 packages (sorted by dependency depth)
    "sz-orm-auth",
    "sz-orm-batch",
    "sz-orm-config",
    "sz-orm-logger",
    "sz-orm-tracing",
    "sz-orm-health",
    "sz-orm-audit",
    "sz-orm-masking",
    "sz-orm-limit",
    "sz-orm-scheduler",
    "sz-orm-storage",
    "sz-orm-mig",
    "sz-orm-back",
    "sz-orm-postgis",
    "sz-orm-timeseries",
    "sz-orm-search",
    "sz-orm-queue",
    "sz-orm-mqtt",
    "sz-orm-websocket",
    "sz-orm-swagger",
    "sz-orm-grpc",
    "sz-orm-graphql",
    "sz-orm-dtx",
    "sz-orm-rw",
    "sz-orm-sharding",
    "sz-orm-lc",
    "sz-orm-wasm",
    "sz-orm-es",
]

MONTHS = {
    "Jan": 1, "Feb": 2, "Mar": 3, "Apr": 4, "May": 5, "Jun": 6,
    "Jul": 7, "Aug": 8, "Sep": 9, "Oct": 10, "Nov": 11, "Dec": 12,
}

# RFC 1123 date pattern: "Tue, 23 Jul 2026 13:45:00 GMT"
RFC1123_RE = re.compile(
    r"try again after \w+,\s*(\d{1,2})\s+(\w{3})\s+(\d{4})\s+(\d{2}):(\d{2}):(\d{2})\s+GMT",
    re.IGNORECASE,
)


def log(msg: str) -> None:
    """Print with timestamp and flush immediately (unbuffered)."""
    ts = dt.datetime.now().strftime("%H:%M:%S")
    print(f"[{ts}] {msg}", flush=True)


def parse_retry_seconds(stderr_text: str) -> int | None:
    """Parse `try again after <date>` from cargo 429 stderr. Returns seconds to wait."""
    m = RFC1123_RE.search(stderr_text)
    if not m:
        return None
    day, mon_str, year, hour, minute, second = m.groups()
    month = MONTHS.get(mon_str)
    if month is None:
        return None
    try:
        retry_utc = dt.datetime(
            int(year), month, int(day),
            int(hour), int(minute), int(second),
            tzinfo=dt.timezone.utc,
        )
    except ValueError:
        return None
    now_utc = dt.datetime.now(dt.timezone.utc)
    delta = (retry_utc - now_utc).total_seconds()
    if delta <= 0:
        return 30
    return int(delta) + 10  # 10s safety margin


def run_cargo_publish(pkg: str) -> tuple[int, str, str]:
    """Run `cargo publish -p <pkg>` and capture combined output."""
    proc = subprocess.run(
        ["cargo", "publish", "-p", pkg],
        cwd=WORKSPACE,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=300,
    )
    return proc.returncode, proc.stdout, proc.stderr


def classify(pkg: str, rc: int, stdout: str, stderr: str) -> str:
    """Classify the result: 'ok', 'exists', 'rate_limit', 'fail'."""
    combined = stdout + "\n" + stderr
    if f"Published {pkg} v" in combined or f"Uploaded {pkg} v" in combined:
        return "ok"
    if "already exists" in combined.lower() or "already been published" in combined.lower():
        return "exists"
    if "429" in combined and "Too Many Requests" in combined:
        return "rate_limit"
    if rc == 0 and ("Published" in combined or "published" in combined):
        return "ok"
    return "fail"


def publish_one(pkg: str, max_retries: int = 5) -> str:
    """Try to publish a package. Returns final status: ok/exists/fail."""
    for attempt in range(1, max_retries + 1):
        log(f"Publish {pkg} (try {attempt}/{max_retries})...")
        rc, stdout, stderr = run_cargo_publish(pkg)
        status = classify(pkg, rc, stdout, stderr)

        if status == "ok":
            log(f"OK {pkg} published")
            time.sleep(3)
            return "ok"
        if status == "exists":
            log(f"SKIP {pkg} already exists")
            return "exists"
        if status == "rate_limit":
            wait = parse_retry_seconds(stderr) or 300
            log(f"RATE_LIMIT {pkg}, waiting {wait}s...")
            # Sleep in chunks so we can show progress
            slept = 0
            while slept < wait:
                chunk = min(30, wait - slept)
                time.sleep(chunk)
                slept += chunk
                if wait > 60 and slept % 60 == 0:
                    log(f"  ... {slept}/{wait}s")
            continue
        # Real failure
        snippet = (stdout + stderr).strip()[-800:]
        log(f"FAIL {pkg} (rc={rc}):")
        for line in snippet.splitlines()[-15:]:
            print(f"    {line}", flush=True)
        time.sleep(15)
    return "fail"


def main() -> int:
    os.chdir(WORKSPACE)

    log(f"=== SZ-ORM crates.io publisher ===")
    log(f"Workspace: {WORKSPACE}")
    log(f"Packages to process: {len(PACKAGES)}")

    results = {"ok": [], "exists": [], "fail": []}
    for i, pkg in enumerate(PACKAGES, 1):
        log(f"--- [{i}/{len(PACKAGES)}] {pkg} ---")
        status = publish_one(pkg)
        results[status].append(pkg)

    log("=== DONE ===")
    log(f"Published: {len(results['ok'])}")
    log(f"Already existed: {len(results['exists'])}")
    log(f"Failed: {len(results['fail'])}")
    if results["fail"]:
        log(f"Failed packages: {', '.join(results['fail'])}")

    return 0 if not results["fail"] else 1


if __name__ == "__main__":
    sys.exit(main())
