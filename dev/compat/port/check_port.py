#!/usr/bin/env python3
"""Ratchet for the native port: every frozen upstream test has a native test.

    check_port.py [--suite uutils|gnu|busybox] [--util NAME] [--strict]

A native test claims an upstream test with a comment line immediately above the
`test` declaration, anywhere under core/tests or tests/xsh:

    # origin: uutils test_basename::test_help
    # origin: gnu tests/ls/color-norm.sh
    # origin: busybox awk/awk -F case 0

and a test that is deliberately not ported (it stays a Rust boundary test or
needs facilities XSH tests cannot have) is listed with a reason in
dev/compat/port/exceptions.json as {"<suite> <id>": "reason"}. The check lists
frozen tests with neither, and tags that name no frozen test. Without --strict
it only reports; with --strict it fails when anything is unmapped.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
TAG = re.compile(r"^\s*#\s*origin:\s*(uutils|gnu|busybox)\s+(.+?)\s*$")


def claims() -> dict[str, list[str]]:
    found: dict[str, list[str]] = defaultdict(list)
    for root in (REPO / "core/tests", REPO / "tests/xsh"):
        for path in sorted(root.rglob("*.xsh")):
            for line in path.read_text(errors="replace").splitlines():
                m = TAG.match(line)
                if m:
                    found[f"{m.group(1)} {m.group(2)}"].append(str(path.relative_to(REPO)))
    return found


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--suite", choices=["uutils", "gnu", "busybox"])
    parser.add_argument("--util")
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()
    freeze = json.loads((REPO / "dev/compat/port/freeze.json").read_text())
    exceptions_path = REPO / "dev/compat/port/exceptions.json"
    exceptions = json.loads(exceptions_path.read_text()) if exceptions_path.exists() else {}
    found = claims()
    missing: list[str] = []
    per_suite: Counter = Counter()
    mapped: Counter = Counter()
    frozen_keys: set[str] = set()
    for suite, utils in freeze.items():
        for util, ids in utils.items():
            for test_id in ids:
                key = f"{suite} {test_id}"
                frozen_keys.add(key)
                if (args.suite and suite != args.suite) or (args.util and util != args.util):
                    continue
                per_suite[suite] += 1
                if key in found or key in exceptions:
                    mapped[suite] += 1
                else:
                    missing.append(key)
    stray = sorted(k for k in found if k not in frozen_keys)
    for suite in sorted(per_suite):
        print(f"{suite}: {mapped[suite]}/{per_suite[suite]} frozen tests have a native test or a reason")
    if stray:
        print(f"{len(stray)} origin tags name no frozen test (first: {stray[0]})")
    duplicates = sorted(k for k, v in found.items() if len(v) > 1)
    if duplicates:
        print(f"{len(duplicates)} upstream tests are claimed twice (first: {duplicates[0]})")
    if missing:
        print(f"{len(missing)} frozen tests are unmapped (first: {missing[0]})")
    return 1 if args.strict and (missing or stray) else 0


if __name__ == "__main__":
    sys.exit(main())
