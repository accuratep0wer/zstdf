# Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
"""Synthetic gate and simulated score: this does not benchmark zstdf."""
import json
import sys
from pathlib import Path

data = json.loads(Path("workload.json").read_text(encoding="utf-8"))
if sys.argv[1] == "check":
    if data["results"] != [1, 4, 9]:
        raise SystemExit("Recorded results differ from the independent expected values")
    print("Exact results passed")
elif sys.argv[1] == "measure":
    print(json.dumps({"simulated_cost": data["simulated_cost"]}))
else:
    raise SystemExit("Use check or measure")
