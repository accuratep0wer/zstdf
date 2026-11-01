#!/usr/bin/env python3
# Copyright 2026 zstdf contributors
# SPDX-License-Identifier: Apache-2.0
"""Emit decode throughput JSON for the improvement runner.

Verifies the pinned public samples against sources.json, builds
`decode_throughput` if needed, and prints one JSON object on stdout.
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SAMPLES = ("pystdf/lot2.stdf", "pystdf/lot3.stdf", "stdfreader/a595.stdf")


def verify():
    sources = json.loads((ROOT / "examples/public-stdf/sources.json").read_text(encoding="utf-8"))
    by_file = {entry["file"]: entry for entry in sources}
    for name in SAMPLES:
        entry = by_file[name]
        path = ROOT / "examples/public-stdf/downloads" / name
        if not path.is_file():
            raise SystemExit(f"missing {path}; run examples/public-stdf/download.py")
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if digest != entry["sha256"] or len(data) != entry["bytes"]:
            raise SystemExit(f"checksum mismatch for {name}")


def main():
    verify()
    binary = ROOT / "target/release/decode_throughput"
    build = subprocess.run(
        ["cargo", "build", "--release", "-p", "stdf-core", "--bin", "decode_throughput", "--locked"],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
    )
    if build.returncode != 0 or not binary.is_file():
        raise SystemExit("decode_throughput build failed")
    measured = subprocess.run([binary], cwd=ROOT, capture_output=True, text=True)
    if measured.returncode != 0:
        sys.stderr.write(measured.stderr)
        raise SystemExit(measured.returncode)
    payload = json.loads(measured.stdout)
    rate = payload["records_per_sec"]
    if not isinstance(rate, (int, float)) or rate <= 0:
        raise SystemExit("records_per_sec is not a positive number")
    print(json.dumps({"records_per_sec": rate}))


if __name__ == "__main__":
    main()
