#!/usr/bin/env python3
# Copyright 2026 zstdf contributors
# SPDX-License-Identifier: Apache-2.0
"""Check project SPDX metadata and exact package notice copies (Python 3.11+)."""
import json
from pathlib import Path
import subprocess
import tomllib


def check():
    root = Path(__file__).resolve().parents[1]
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    expected = "Apache-2.0"
    assert workspace["workspace"]["package"]["license"] == expected
    license_text = (root / "LICENSE").read_text(encoding="utf-8")
    assert "Version 2.0, January 2004" in license_text
    assert "END OF TERMS AND CONDITIONS" in license_text
    for member in workspace["workspace"]["members"]:
        package = tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf-8"))
        assert package["package"]["license"] == {"workspace": True}, member
        for notice in ("LICENSE", "NOTICE"):
            assert (root / member / notice).read_bytes() == (root / notice).read_bytes(), (member, notice)
    for base in (root, root / "stdf-py"):
        pyproject = tomllib.loads((base / "pyproject.toml").read_text(encoding="utf-8"))
        assert pyproject["project"]["license"] == expected, base
        assert set(pyproject["project"]["license-files"]) == {"LICENSE", "NOTICE"}, base
        for notice in pyproject["project"]["license-files"]:
            assert (base / notice).is_file(), (base, notice)
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"],
        cwd=root, check=True, capture_output=True, text=True,
    )
    metadata = json.loads(result.stdout)
    packages = [p for p in metadata["packages"] if p["id"] in metadata["workspace_members"]]
    assert len(packages) == len(workspace["workspace"]["members"])
    assert all(p["license"] == expected for p in packages)
    print(f"PASS: {len(packages)} Rust crates, both Python metadata entries, and all LICENSE/NOTICE copies")


if __name__ == "__main__":
    check()
