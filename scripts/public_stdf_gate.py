#!/usr/bin/env python3
# Copyright 2026 zstdf contributors
# SPDX-License-Identifier: Apache-2.0
"""Run the pinned public-sample test with downloads required.

The improvement runner does not forward environment variables, so this gate
sets ZSTDF_REQUIRE_PUBLIC itself.
"""
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    env = os.environ.copy()
    env["ZSTDF_REQUIRE_PUBLIC"] = "1"
    result = subprocess.run(
        [
            "cargo",
            "test",
            "-p",
            "stdf-cli",
            "--locked",
            "pinned_public_files_match_expected_counts",
        ],
        cwd=ROOT,
        env=env,
    )
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
