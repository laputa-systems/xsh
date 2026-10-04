#!/usr/bin/env python3
"""Convert suite output into per-utility result JSON for dev/coreutils-parity.json.

    results.py uutils JUNIT.xml OUT.json [UTILITY...]
    results.py gnu UUTILS.json XSH.json OUT.json

uutils mode reads a nextest JUnit report. Test IDs are `test_<util>::<name>`;
IDs listed in dev/compat/exclusions.json count as `excluded`, not `fail`.
When utilities are named, only their entries are replaced, so partial runs
update the existing report instead of erasing other utilities.

gnu mode merges two reports from uutils' `util/gnu-json-result.py` (one run
against pinned uutils, one against XSH) into the four-cell differential. The
`uutils_pass_xsh_fail` cell is the Gate 4 blocker list.
"""

from __future__ import annotations

import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
EXCLUSIONS = REPO / "dev" / "compat" / "exclusions.json"


def load(path: Path, default):
    return json.loads(path.read_text()) if path.exists() else default


def uutils_mode(junit: Path, out: Path, only: list[str]) -> None:
    excluded = {e["id"] for e in load(EXCLUSIONS, {"tests": []})["tests"]}
    utilities: dict[str, dict] = {}
    for case in ET.parse(junit).iter("testcase"):
        test_id = case.get("name", "")
        module, _, _ = test_id.partition("::")
        if not module.startswith("test_"):
            continue
        util = module.removeprefix("test_")
        entry = utilities.setdefault(util, {"pass": 0, "fail": 0, "skip": 0, "excluded": 0, "failing": []})
        if test_id in excluded:
            entry["excluded"] += 1
        elif case.find("failure") is not None or case.find("error") is not None:
            entry["fail"] += 1
            entry["failing"].append(test_id)
        elif case.find("skipped") is not None:
            entry["skip"] += 1
        else:
            entry["pass"] += 1
    for entry in utilities.values():
        entry["failing"].sort()

    report = load(out, {"suite": "uutils-integration", "utilities": {}})
    if only:
        for util in only:
            report["utilities"].pop(util, None)
        report["utilities"].update({u: utilities[u] for u in only if u in utilities})
    else:
        report["utilities"] = utilities
    report["utilities"] = dict(sorted(report["utilities"].items()))
    totals = {k: sum(e[k] for e in report["utilities"].values()) for k in ("pass", "fail", "skip", "excluded")}
    report["totals"] = totals
    out.write_text(json.dumps(report, indent=2) + "\n")
    ran = totals["pass"] + totals["fail"]
    rate = f"{100 * totals['pass'] / ran:.1f}%" if ran else "n/a"
    print(f"uutils integration: {totals['pass']} pass, {totals['fail']} fail, "
          f"{totals['skip']} skip, {totals['excluded']} excluded ({rate} of run tests pass)")


def gnu_status(report: dict, prefix: str = "") -> dict[str, str]:
    """Flatten gnu-json-result.py output (nested {dir: {test.log: STATUS}}) to {dir/test.log: STATUS}."""
    flat = {}
    for key, value in report.items():
        path = f"{prefix}{key}"
        if isinstance(value, dict):
            flat.update(gnu_status(value, f"{path}/"))
        else:
            flat[path] = value
    return flat


def gnu_mode(uu_path: Path, xsh_path: Path, out: Path) -> None:
    uu = gnu_status(json.loads(uu_path.read_text()))
    xsh = gnu_status(json.loads(xsh_path.read_text()))
    cells = {"uutils_pass_xsh_pass": [], "uutils_pass_xsh_fail": [], "uutils_fail_xsh_pass": [], "uutils_fail_xsh_fail": []}
    utilities: dict[str, dict] = {}
    for test in sorted(set(uu) | set(xsh)):
        u_ok = uu.get(test) == "PASS"
        x_ok = xsh.get(test) == "PASS"
        if uu.get(test) == "SKIP" and xsh.get(test) == "SKIP":
            continue
        cell = f"uutils_{'pass' if u_ok else 'fail'}_xsh_{'pass' if x_ok else 'fail'}"
        cells[cell].append(test)
        util = test.split("/")[0]
        entry = utilities.setdefault(util, {"pass": 0, "fail": 0, "skip": 0, "excluded": 0, "blockers": []})
        entry["pass" if x_ok else "fail"] += 1
        if cell == "uutils_pass_xsh_fail":
            entry["blockers"].append(test)
    report = {
        "suite": "gnu-differential",
        "counts": {k: len(v) for k, v in cells.items()},
        "cells": cells,
        "utilities": utilities,
    }
    out.write_text(json.dumps(report, indent=2) + "\n")
    print("GNU differential:", json.dumps(report["counts"]))


def main() -> int:
    if len(sys.argv) >= 4 and sys.argv[1] == "uutils":
        uutils_mode(Path(sys.argv[2]), Path(sys.argv[3]), sys.argv[4:])
        return 0
    if len(sys.argv) == 5 and sys.argv[1] == "gnu":
        gnu_mode(Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4]))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
