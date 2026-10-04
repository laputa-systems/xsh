#!/usr/bin/env python3
"""Gate 6 ratchet: no compatibility applet may accept-and-discard options.

Scans `core/*.xsh` for option-record fields named like a discard bucket
(`ignored`, `unused`, `compat`, `noop`) and compares them with the baseline in
`dev/compat/ignored-options-baseline.json`. New offenders fail; a baseline
entry that no longer appears also fails, so the baseline can only shrink and
must be edited in the same change that removes a bucket.

An option that is ignored *because ignoring it is itself compatible behavior*
(e.g. GNU accepts `-u` for `cat` and documents it as ignored) must not live in
a bucket: give it a named field and a comment citing the upstream behavior.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BASELINE = REPO / "dev" / "compat" / "ignored-options-baseline.json"
BUCKET = re.compile(r"^\s*(ignored|unused|compat|noop)[A-Za-z0-9_]*\s*:\s*\{", re.M)
FORM = re.compile(r'form:\s*"([^"]*)"')


def scan() -> dict[str, str]:
    found: dict[str, str] = {}
    for path in sorted((REPO / "core").glob("*.xsh")):
        text = path.read_text()
        for match in BUCKET.finditer(text):
            tail = text[match.end() : match.end() + 400]
            form = FORM.search(tail)
            found[f"{path.name}:{match.group(1)}"] = form.group(1) if form else ""
    return found


def main() -> int:
    found = scan()
    if "--write-baseline" in sys.argv:
        BASELINE.write_text(json.dumps({"buckets": found}, indent=2, sort_keys=True) + "\n")
        return 0
    baseline = json.loads(BASELINE.read_text())["buckets"] if BASELINE.exists() else {}
    status = 0
    for key, form in sorted(found.items()):
        if key not in baseline:
            print(f"new discard bucket {key} accepts `{form}`: implement or reject these options")
            status = 1
        elif form != baseline[key]:
            grew = set(form.split()) - set(baseline[key].split())
            if grew:
                print(f"{key} gained discarded options {sorted(grew)}")
                status = 1
            else:
                print(f"{key} shrank; update {BASELINE.relative_to(REPO)} in this change")
                status = 1
    for key in sorted(set(baseline) - set(found)):
        print(f"{key} is gone; remove it from {BASELINE.relative_to(REPO)} in this change")
        status = 1
    if status == 0:
        print(f"ignored-options ratchet: {len(found)} legacy buckets remain, none new")
    return status


if __name__ == "__main__":
    sys.exit(main())
