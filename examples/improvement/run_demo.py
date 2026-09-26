# Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
"""Run a bounded synthetic experiment without changing product source files."""
import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path,
                        default=Path("target/improvement-demo"))
    args = parser.parse_args()
    out = args.output_dir.resolve()
    # Refuse reuse so previous evidence and user files are never overwritten.
    out.mkdir(parents=True, exist_ok=False)
    here = Path(__file__).resolve().parent
    runner = here.parents[1] / "scripts" / "improvement_runner.py"
    for name, cost, results in [
        ("baseline", 100, [1, 4, 9]),
        ("better", 70, [1, 4, 9]),
        ("worse", 120, [1, 4, 9]),
        ("incorrect", 50, [1, 4, 8]),
    ]:
        folder = out / name
        folder.mkdir()
        shutil.copyfile(here / "evaluate.py", folder / "evaluate.py")
        (folder / "workload.json").write_text(
            json.dumps({"results": results, "simulated_cost": cost}) + "\n",
            encoding="utf-8")

    def run(*arguments, expected):
        result = subprocess.run([sys.executable, str(runner), *map(str, arguments)],
                                capture_output=True, text=True, encoding="utf-8")
        print(result.stdout, end="")
        if result.stderr:
            print(result.stderr, file=sys.stderr, end="")
        if result.returncode != expected:
            raise SystemExit(f"Unexpected exit code: {result.returncode}; expected {expected}")

    run("init", "--contract", here / "contract.json", "--baseline", out / "baseline",
        "--state", out / "experiment", expected=0)
    for name, code in [("better", 0), ("worse", 1), ("incorrect", 1)]:
        run("evaluate", "--state", out / "experiment", "--candidate", out / name,
            expected=code)
    run("status", "--state", out / "experiment", expected=0)
    print(f"Synthetic demonstration complete. Evidence: {out / 'experiment'}")


if __name__ == "__main__":
    main()
