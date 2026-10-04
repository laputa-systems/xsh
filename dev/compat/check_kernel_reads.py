#!/usr/bin/env python3
"""Ratchet: compatibility applets read kernel state only through typed domain APIs.

CAMPAIGN.md "One model per kernel ABI": each kernel ABI (procfs, sysfs,
netlink, ioctl) has exactly one typed collector or controller, and commands are
renderers over it. A `/proc/` or `/sys/` path literal in a top-level applet
(`core/*.xsh`) means that applet grew a private reader. Collectors live in
native modules and in `core/lib/`, which this check does not scan.

Offenders already present are listed in `dev/compat/kernel-reads-baseline.json`
(applet -> number of literals). New offenders and grown counts fail, and so does
a baseline entry that no longer matches, so the baseline only shrinks and is
edited in the change that removes the reads.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BASELINE = REPO / "dev" / "compat" / "kernel-reads-baseline.json"
KERNEL_PATH = re.compile(r"/(?:proc|sys)/")


def scan() -> dict[str, int]:
    found: dict[str, int] = {}
    for path in sorted((REPO / "core").glob("*.xsh")):
        count = len(KERNEL_PATH.findall(path.read_text(errors="replace")))
        if count:
            found[f"core/{path.name}"] = count
    return found


def main() -> int:
    baseline = json.loads(BASELINE.read_text())["applets"] if BASELINE.exists() else {}
    found = scan()
    errors = []
    for applet, count in found.items():
        allowed = baseline.get(applet)
        if allowed is None:
            errors.append(f"{applet}: {count} direct /proc or /sys literal(s); use the domain API (or add it to the domain)")
        elif count > allowed:
            errors.append(f"{applet}: direct kernel reads grew from {allowed} to {count}")
        elif count < allowed:
            errors.append(f"{applet}: baseline lists {allowed} but only {count} remain; update {BASELINE.name}")
    for applet in baseline:
        if applet not in found:
            errors.append(f"{applet}: no direct kernel reads remain; remove it from {BASELINE.name}")
    if errors:
        print("kernel-reads ratchet failed:", file=sys.stderr)
        for line in errors:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"kernel-reads ratchet: {len(found)} legacy applet(s) with direct reads, none new")
    return 0


if __name__ == "__main__":
    sys.exit(main())
