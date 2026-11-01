#!/usr/bin/env python3
# Copyright 2026 zstdf contributors
# SPDX-License-Identifier: Apache-2.0
"""Write cargo-fuzz seeds from the golden hex fixtures and, when present, public samples.

Public sample bytes are named public-* and are gitignored. Golden seeds are
synthetic and may be committed.
"""
from pathlib import Path
import hashlib

ROOT = Path(__file__).resolve().parents[1]
GOLDEN = ROOT / "fixtures" / "stdf" / "golden"
PUBLIC = ROOT / "examples" / "public-stdf" / "downloads"
TARGETS = ("decode_all", "truncate_prefix")


def parse_hex(text):
    out = bytearray()
    for line in text.splitlines():
        token = line.split("#", 1)[0]
        for piece in token.split():
            out.append(int(piece, 16))
    return bytes(out)


def write(target, name, data):
    path = ROOT / "fuzz" / "corpus" / target / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    print(f"{target}/{name} {len(data)} bytes")


def main():
    seeds = []
    for hex_path in sorted(GOLDEN.glob("*.hex")):
        seeds.append((f"golden-{hex_path.stem}", parse_hex(hex_path.read_text(encoding="utf-8"))))
    for sample in ("pystdf/lot2.stdf", "pystdf/lot3.stdf", "stdfreader/a595.stdf"):
        path = PUBLIC / sample
        if path.is_file():
            digest = hashlib.sha256(path.read_bytes()).hexdigest()[:12]
            seeds.append((f"public-{path.stem}-{digest}", path.read_bytes()[:65536]))
    for target in TARGETS:
        for name, data in seeds:
            write(target, name, data)


if __name__ == "__main__":
    main()
