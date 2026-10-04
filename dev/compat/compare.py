#!/usr/bin/env python3
"""Compare two uutils-integration result files: the integrator's no-regression gate.

    compare.py BEFORE.json AFTER.json

Prints the before/after totals (for commit messages) and the per-utility
change. Exits 1 when any test that passed before fails now: a test is a
regression when it appears in AFTER's `failing` list for a utility but not in
BEFORE's. Both files come from the same pinned uutils commit, so the test set
is identical and "not failing before" means "passed before" (or excluded).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def load(path: str) -> dict:
    return json.loads(Path(path).read_text())


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    before, after = load(sys.argv[1]), load(sys.argv[2])
    b, a = before["totals"], after["totals"]
    print(f"suite totals: {b['pass']}/{b['pass'] + b['fail']} -> {a['pass']}/{a['pass'] + a['fail']} passing "
          f"({a['pass'] - b['pass']:+d})")

    regressions: list[str] = []
    for util in sorted(set(before["utilities"]) | set(after["utilities"])):
        old = before["utilities"].get(util, {"pass": 0, "fail": 0, "failing": []})
        new = after["utilities"].get(util, {"pass": 0, "fail": 0, "failing": []})
        regressed = sorted(set(new["failing"]) - set(old["failing"]))
        fixed = sorted(set(old["failing"]) - set(new["failing"]))
        if regressed or fixed or old["pass"] != new["pass"]:
            print(f"  {util}: {old['pass']} -> {new['pass']} pass ({len(fixed)} fixed, {len(regressed)} regressed)")
        regressions.extend(regressed)
    if regressions:
        print(f"REGRESSIONS: {len(regressions)} test(s) passed before and fail now:", file=sys.stderr)
        for test in regressions:
            print(f"  {test}", file=sys.stderr)
        return 1
    print("no regressions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
