#!/usr/bin/env python3
"""Create native-port worktrees and briefs for frozen upstream tests.

    lanes.py uutils UTIL [UTIL...] [--chunk N]      create lanes (not spawned)
    lanes.py list [--suite S]                       utilities with unmapped tests

Lane worktrees and their scratch files live beside this checkout, under
`<checkout>-lanes`; the integration checkout is never used as a lane.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
LANES_ROOT = REPO.parent / f"{REPO.name}-lanes"
SCRATCH_ROOT = LANES_ROOT / "_scratch"
SHARED_XSH_BIN = Path(
    "/home/josh/d/laputa-systems/xsh/target/x86_64-unknown-linux-musl/release/xsh",
)

if str(REPO) not in sys.path:
    sys.path.insert(0, str(REPO))

from dev.compat.port.suite_sources import reference_paths, test_file
from dev.compat.port import check_port

FILE_PREFIX = {"uutils": "uu", "gnu": "gnu", "busybox": "bb"}


def freeze() -> dict:
    return json.loads((REPO / "dev/compat/port/freeze.json").read_text())


def tagged() -> set[str]:
    found, _detached = check_port.claims()
    out = set(found)
    exceptions = REPO / "dev/compat/port/exceptions.json"
    if exceptions.exists():
        out |= set(json.loads(exceptions.read_text()))
    return out


def git(*args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(REPO), *args], capture_output=True, text=True,
    )
    if result.returncode:
        raise RuntimeError(f"git {' '.join(args)}: {result.stderr.strip()}")
    return result.stdout.strip()


def native_test_name(suite: str, util: str, test_id: str) -> str:
    if suite == "uutils":
        prefix = f"test_{util}::"
        if not test_id.startswith(prefix):
            raise ValueError(f"uutils origin {test_id!r} does not belong to {util!r}")
        parts = test_id[len(prefix):].split("::")
        parts[-1] = parts[-1].removeprefix("test_")
        name = "_".join(parts)
    else:
        prefix = f"{util}/"
        if not test_id.startswith(prefix) or len(test_id) == len(prefix):
            raise ValueError(f"{suite} origin {test_id!r} does not belong to {util!r}")
        name = test_id[len(prefix):]
        name = name.removeprefix("test_")
    name = re.sub(r"[^A-Za-z0-9_]+", "_", name).strip("_")
    digest = f"_{hashlib.sha256(test_id.encode()).hexdigest()[:8]}" if suite == "busybox" else ""
    return f"test_{FILE_PREFIX[suite]}_{util}_{name}{digest}"


def native_test_names(suite: str, util: str, ids: list[str]) -> dict[str, str]:
    if len(set(ids)) != len(ids):
        raise ValueError(f"{suite} {util} has duplicate frozen test IDs")
    names = {test_id: native_test_name(suite, util, test_id) for test_id in ids}
    counts: dict[str, int] = {}
    for name in names.values():
        counts[name] = counts.get(name, 0) + 1
    for test_id, name in list(names.items()):
        if counts[name] > 1:
            names[test_id] = f"{name}_{hashlib.sha256(test_id.encode()).hexdigest()[:8]}"
    if len(set(names.values())) != len(names):
        raise ValueError(f"{suite} {util} frozen IDs do not produce unique native test names")
    return names


def rules(suite: str, util: str, fixture_owner: bool, has_fixtures: bool) -> str:
    common = f"""Transcribe each listed upstream test as an independent native XSH test.
Keep its observable arguments, input data, expected output and status. Put the
exact `# origin: {suite} <full frozen id>` immediately above each `test` line.
Use the exact native test name listed beside each ID below.
Use `support.uu` for applet launches in every suite so the integrator can run
the same assertions against the reference tool with `port/oracle_port.py`.
Request exact stream or process boundary support instead of bypassing the oracle.

Propose an exception only when the exact observable boundary cannot be owned by
an XSH test. In your lane's scratch `exceptions.json`, state the boundary and
either the retained Rust coverage path or the exact coverage request; timing or
complexity alone is not a reason. Use `{{"{suite} <full frozen id>": "boundary: ...; retained Rust coverage: ..."}}`;
do not edit the shared exceptions file. The integrator reviews and merges it.
Use the x86_64 Linux musl branch of platform-gated tests. Directory input,
non-UTF-8 paths, and byte-exact streams are native-testable, not automatic
exceptions. Keep stable timing assertions. Keep a faithful native test when it
shows an applet difference, and report the ID and output under `Findings`;
integration remains unaccepted until that behavior passes.

Run the owned native test in the current test image from your worktree:

    XSH_BIN={SHARED_XSH_BIN} dev/compat/docker-xsht.sh -- test -j 1 <owned test file>

The shared `xsh` and sibling `xsht` binaries are read-only. Do not build them
from a lane. Commit only your owned test and fixture files when the focused test
passes; never push, merge, or rebase another lane. Report tests run, decisions,
`Requests:`, findings, and blockers in under 200 words."""
    if suite == "uutils":
        fixtures = (
            f"Only this first chunk owns `core/tests/data/uutils/{util}/`; other chunks must not write there. "
            "If another chunk needs a fixture, list its exact name in `Requests:` so the integrator can add it, "
            "then rebase after that change merges. "
            if has_fixtures and fixture_owner else
            "This chunk does not own the shared fixture directory. Request any needed fixture names from the integrator. "
            if has_fixtures else ""
        )
        return f"""{common}
Use `use support.uu as uu` and follow `core/tests/support/uu.xsh` and
`core/tests/test-uu-basename.xsh`. Convert Rust scene setup and command builders
to matching `uu` helpers. Use `uu.invoke_from_path` for path-backed stdin,
`uu.invoke_paths` for non-UTF-8 path arguments, `uu.at_bytes` for byte-named
paths, and `uu.stdout_only_bytes` or `uu.stderr_is_bytes` for byte-exact output.
If a needed helper is missing, write a small local proc in your owned test file
and report it; do not edit shared support code.
{fixtures}The uutils suite is MIT licensed. Port its test behavior and approved
fixtures; do not copy applet implementation code.

Finish with `python3 dev/compat/port/check_port.py --suite uutils --util {util}`
after every chunk for this utility has landed."""
    license_name = "GNU Coreutils" if suite == "gnu" else "BusyBox"
    return f"""{common}
The {license_name} reference tests are GPL licensed. Use them only to learn the
behavior; write original XSH tests and do not copy test scripts, comments, or
literal script text. Construct inputs in the test or request a fixture through
the integrator. Finish with `python3 dev/compat/port/check_port.py --suite
{suite} --util {util}` after every chunk for this utility has landed."""


def lane_brief(
    suite: str,
    lane: str,
    util: str,
    test_file: str,
    fixture_owner: bool,
    ids: list[str],
    head: str,
    references: dict[str, Path],
    source_files: dict[str, str],
    test_names: dict[str, str],
) -> str:
    worktree = LANES_ROOT / lane
    scratch = SCRATCH_ROOT / lane
    has_fixtures = "fixtures" in references
    fixture_path = f"core/tests/data/uutils/{util}/"
    owned = [test_file]
    if suite == "uutils" and has_fixtures and fixture_owner:
        owned.append(fixture_path)
    fixture_note = ""
    if suite == "uutils" and has_fixtures:
        fixture_note = (
            f"\nThis lane owns `{fixture_path}` for this utility's chunks."
            if fixture_owner else f"\nThis lane does not own `{fixture_path}`."
        )
    source_paths = sorted({str(references["source"] / source_files[test_id]) for test_id in ids})
    mappings = "\n".join(
        f"- `{test_id}` → `{test_names[test_id]}` — `{references['source'] / source_files[test_id]}`"
        for test_id in ids
    )
    return f"""Lane: {lane}
Worktree: {worktree} on branch lane/{lane} (from integration HEAD {head})
Scratch: {scratch}
You own exactly: {', '.join(owned)}{fixture_note}
Proposed exceptions: {scratch}/exceptions.json (lane-local; integrator merges).

Upstream checkout (read-only): {references['source']}
Upstream test source(s):
{chr(10).join(f"- {path}" for path in source_paths)}
{f"Upstream fixtures (read-only): {references['fixtures']}" if has_fixtures else ""}

Goal: transcribe these {len(ids)} frozen {suite} tests to native XSH tests:
{mappings}

{rules(suite, util, fixture_owner, has_fixtures)}"""


def create(suite: str, util: str, chunk: int, head: str | None = None) -> list[str]:
    if chunk < 1:
        raise ValueError("--chunk must be at least 1")
    ids = freeze()[suite][util]
    done = tagged()
    ids = [test_id for test_id in ids if f"{suite} {test_id}" not in done]
    parts = [ids[index:index + chunk] for index in range(0, len(ids), chunk)]
    if not parts:
        return []

    if suite not in FILE_PREFIX:
        raise ValueError(f"unsupported suite {suite!r}")
    references = reference_paths(suite, util)
    source_files = {test_id: test_file(suite, util, test_id) for test_id in ids}
    test_names = native_test_names(suite, util, ids)
    base = head or git("rev-parse", "HEAD")
    LANES_ROOT.mkdir(parents=True, exist_ok=True)
    SCRATCH_ROOT.mkdir(parents=True, exist_ok=True)
    names = []
    for number, part in enumerate(parts, 1):
        suffix = f"-{number}" if len(parts) > 1 else ""
        lane = f"port-{suite}-{util}{suffix}"
        test_path = f"core/tests/test-{FILE_PREFIX[suite]}-{util}{suffix}.xsh"
        worktree = LANES_ROOT / lane
        scratch = SCRATCH_ROOT / lane
        if worktree.exists() or scratch.exists():
            raise FileExistsError(f"lane path already exists: {worktree} or {scratch}")

        scratch.mkdir()
        (scratch / "tests.txt").write_text("\n".join(part) + "\n")
        (scratch / "exceptions.json").write_text("{}\n")
        (scratch / "brief.md").write_text(
            lane_brief(
                suite, lane, util, test_path, number == 1, part, base, references, source_files,
                test_names,
            ) + "\n",
        )
        try:
            git("worktree", "add", "-b", f"lane/{lane}", str(worktree), base)
        except Exception:
            shutil.rmtree(scratch)
            raise
        names.append(lane)
    return names


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action")
    parser.add_argument("utils", nargs="*")
    parser.add_argument("--chunk", type=int, default=80)
    parser.add_argument("--suite", default="uutils")
    args = parser.parse_args()
    if args.action == "list":
        done = tagged()
        for util, ids in freeze()[args.suite].items():
            left = [test_id for test_id in ids if f"{args.suite} {test_id}" not in done]
            if left:
                print(f"{util}: {len(left)}/{len(ids)}")
        return
    if args.action not in FILE_PREFIX:
        sys.exit("suite must be one of: uutils, gnu, busybox")
    try:
        head = git("rev-parse", "HEAD")
        for util in args.utils:
            for name in create(args.action, util, args.chunk, head):
                print(name)
    except (FileExistsError, RuntimeError, ValueError) as error:
        sys.exit(str(error))


if __name__ == "__main__":
    main()
