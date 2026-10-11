#!/usr/bin/env python3
"""Validate core/tests/origins/exclusions.json: per-test, categorized, explained.

Upstream test exclusions are exact uutils test IDs (`test_<util>::function`), never
whole modules, each with a utility, a category from the closed list below and a
non-empty reason. With UUTILS_ROOT set, every ID must also name a test that
exists in the pinned tree, so a stale exclusion cannot linger after upstream
renames or removes a test. Excluded tests still count in the totals as
`excluded`; the denominator never shrinks.
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
EXCLUSIONS = REPO / "core" / "tests" / "origins" / "exclusions.json"
CATEGORIES = {
    "selinux": "needs SELinux",
    "smack": "needs SMACK",
    "needs-root": "requires privileges the harness does not grant",
    "host-oracle": "compares against a host tool that the clean environment does not have",
    "help-text": "asserts uutils-specific help or version text",
    "uutils-internal": "exercises uutils itself, not command behavior",
    "platform": "tests another platform's behavior",
    "clap-wording": "asserts clap's diagnostic wording; XSH follows GNU getopt_long wording",
    "gnu-semantics": "asserts behavior that conflicts with the pinned GNU behavior XSH follows",
    "uutils-extension": "asserts a uutils extension GNU does not have; XSH follows GNU",
    "thread-model": "traces the process with strace without -f and expects a syscall on the main thread; XSH evaluates scripts on one worker thread",
}
ID = re.compile(r"^test_([a-z0-9_]+)::([A-Za-z0-9_:]+)$")
RSTEST_CASE = re.compile(r"^case_(\d+)_(\w+)$")


def test_id_exists(test_path: str, source: str) -> bool:
    parts = test_path.split("::")
    case = RSTEST_CASE.fullmatch(parts[-1])
    function_parts = parts[:-1] if case else parts
    functions = [
        part
        for part in function_parts
        if re.search(rf"\bfn {re.escape(part)}\b", source)
    ]
    if not functions:
        return False
    if not case:
        return True

    function = functions[-1]
    declaration = re.search(
        rf"(?m)((?:^[ \t]*#\[[^\n]*\][ \t]*\n)+)^[ \t]*fn\s+{re.escape(function)}\b",
        source,
    )
    if not declaration:
        return False
    cases = re.findall(
        r"(?m)^[ \t]*#\[case(?:::(\w+))?(?:\(|\])",
        declaration.group(1),
    )
    ordinal = int(case.group(1))
    return 1 <= ordinal <= len(cases) and cases[ordinal - 1] == case.group(2)


def main() -> int:
    data = json.loads(EXCLUSIONS.read_text())
    errors: list[str] = []
    seen: set[str] = set()
    root = os.environ.get("UUTILS_ROOT")
    sources: dict[str, str] = {}
    for entry in data.get("tests", []):
        test_id = entry.get("id", "")
        match = ID.match(test_id)
        if not match:
            errors.append(f"{test_id or '<missing id>'}: not an exact `test_<util>::function` ID (module-wide exclusions are rejected)")
            continue
        if test_id in seen:
            errors.append(f"{test_id}: listed twice")
        seen.add(test_id)
        util = match.group(1)
        if entry.get("utility") != util:
            errors.append(f"{test_id}: utility must be '{util}', found {entry.get('utility')!r}")
        if entry.get("category") not in CATEGORIES:
            errors.append(f"{test_id}: category {entry.get('category')!r} is not one of {', '.join(sorted(CATEGORIES))}")
        if len(entry.get("reason", "").strip()) < 20:
            errors.append(f"{test_id}: needs a reason that explains the exclusion")
        if root:
            path = Path(root) / "tests" / "by-util" / f"test_{util}.rs"
            if util not in sources:
                sources[util] = path.read_text(errors="replace") if path.exists() else ""
            if not test_id_exists(match.group(2), sources[util]):
                errors.append(f"{test_id}: no such test in the pinned uutils tree")
    if errors:
        print("exclusions check failed:", file=sys.stderr)
        for line in errors:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"exclusions: {len(seen)} per-test exclusion(s), all categorized and explained")
    return 0


if __name__ == "__main__":
    sys.exit(main())
