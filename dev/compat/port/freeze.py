#!/usr/bin/env python3
"""Freeze the set of upstream tests XSH passes, for the native port.

    freeze.py --uutils-junit FILE --gnu-json FILE --busybox-logs DIR --out freeze.json

The output lists, per suite and per utility, every test identifier that passed
at the freeze point and is not an accepted exclusion. The native port is
complete when `check_port.py` finds a native test carrying an origin tag for
each of them (or a recorded reason).
"""

from __future__ import annotations

import argparse
import json
import re
import xml.etree.ElementTree as ET
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]


def uutils(junit: Path) -> dict[str, list[str]]:
    excluded = {t["id"] for t in json.loads((REPO / "dev/compat/exclusions.json").read_text())["tests"]}
    out: dict[str, set[str]] = defaultdict(set)
    for case in ET.parse(junit).iter("testcase"):
        name = case.get("name", "").split()[-1]
        if not name.startswith("test_") or "::" not in name:
            continue
        failed = case.find("failure") is not None or case.find("error") is not None
        skipped = case.find("skipped") is not None
        if failed or skipped or name in excluded:
            continue
        out[name.split("::", 1)[0][len("test_"):]].add(name)
    return {util: sorted(ids) for util, ids in sorted(out.items())}


def gnu(report: Path) -> dict[str, list[str]]:
    data = json.loads(report.read_text())
    out: dict[str, list[str]] = {}
    for util, tests in sorted(data.items()):
        if not isinstance(tests, dict):
            continue
        ids = sorted(f"{util}/{name}" for name, status in tests.items() if status == "PASS")
        if ids:
            out[util] = ids
    return out


def busybox(logs: Path) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for log in sorted(logs.glob("*.log")):
        ids = sorted({f"{log.stem}/{m.group(1).strip()}"
                      for m in re.finditer(r"^PASS: (.*)$", log.read_text(errors="replace"), re.M)})
        if ids:
            out[log.stem] = ids
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--uutils-junit", type=Path, required=True)
    parser.add_argument("--gnu-json", type=Path, required=True)
    parser.add_argument("--busybox-logs", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    freeze = {
        "uutils": uutils(args.uutils_junit),
        "gnu": gnu(args.gnu_json),
        "busybox": busybox(args.busybox_logs),
    }
    args.out.write_text(json.dumps(freeze, indent=1, sort_keys=True) + "\n")
    for suite, utils in freeze.items():
        print(f"{suite}: {sum(len(v) for v in utils.values())} tests in {len(utils)} utilities")


if __name__ == "__main__":
    main()
