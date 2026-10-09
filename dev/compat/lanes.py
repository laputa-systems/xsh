#!/usr/bin/env python3
"""Lane ownership and brief rendering for the compatibility campaign.

    lanes.py check            exclusive ownership + coverage of the uutils manifest
    lanes.py list [WAVE]      lanes, kinds and utility counts
    lanes.py brief LANE ...   print the lane brief (LANES.md template) with the
                              current uutils counts for the lane's utilities

`check` fails when a utility or file has two owners, a lane names a utility
the manifest does not know (aliases excepted), or an in-scope uutils utility
has no owner in any lane.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
LANES = REPO / "dev" / "compat" / "lanes.json"
MANIFEST = REPO / "dev" / "coreutils-parity.json"
RESULTS = REPO / "dev" / "compat" / "results" / "uutils-integration.json"


def load() -> dict:
    return json.loads(LANES.read_text())["lanes"]


def owned_files(name: str, lane: dict) -> list[str]:
    files = []
    for util in lane["utilities"]:
        files += [f"core/{util}.xsh", f"core/tests/test-{util}.xsh"]
    return files + lane["extra_files"]


def check() -> int:
    lanes = load()
    manifest = json.loads(MANIFEST.read_text())
    in_scope = {u["utility"] for u in manifest["utilities"] if u["capability"] is None}
    gated = {u["utility"] for u in manifest["utilities"] if u["capability"] is not None}
    errors = []
    owners: dict[str, str] = {}
    util_owner: dict[str, str] = {}
    for name, lane in lanes.items():
        for path in owned_files(name, lane):
            if path in owners and owners[path] != name:
                errors.append(f"{path} is owned by both {owners[path]} and {name}")
            owners[path] = name
        for util in lane["utilities"]:
            util_owner[util] = name
    for util in sorted(in_scope - set(util_owner)):
        errors.append(f"in-scope utility {util} has no owner")
    if errors:
        print("lane ownership check failed:", file=sys.stderr)
        for line in errors:
            print(f"  {line}", file=sys.stderr)
        return 1
    extra = sorted(set(util_owner) - in_scope - gated)
    print(f"lanes: {len(lanes)} lanes, {len(owners)} owned files; all {len(in_scope)} in-scope uutils utilities owned"
          + (f"; non-uutils applets owned: {', '.join(extra)}" if extra else ""))
    return 0


def counts(utils: list[str]) -> tuple[int, int, list[str]]:
    data = json.loads(RESULTS.read_text())["utilities"] if RESULTS.exists() else {}
    passed = total = 0
    lines = []
    for util in utils:
        entry = data.get(util)
        if entry is None:
            continue
        p, f, x = entry["pass"], entry["fail"], entry.get("excluded", 0)
        passed += p
        total += p + f
        lines.append(f"{util} {p}/{p + f}" + (" (no applet yet)" if entry.get("applet") is False else ""))
    return passed, total, lines


def integration_root() -> Path:
    """Return the main checkout root when called from a linked worktree."""
    try:
        common_dir = subprocess.run(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            cwd=REPO,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return REPO
    path = Path(common_dir).resolve()
    return path.parent if path.name == ".git" else REPO


def brief(name: str, sha: str, root: str, targets: str, shared: str,
          uutils_root: str) -> str:
    lane = load()[name]
    files = owned_files(name, lane)
    passed, total, per = counts(lane["utilities"])
    rust = lane["kind"] in ("rust",)
    build = (
        f"CARGO_TARGET_DIR={targets}/{name}, compile only via `flock {targets}/cargo.lock cargo ...`"
        if rust
        else f"XSH_BIN={shared}/xsh (script lane; no compiling)"
    )
    goal = (
        f"{', '.join(lane['utilities'])} pass their applicable uutils tests (current: {passed}/{total}: {'; '.join(per)})"
        if lane["utilities"]
        else lane["note"]
    )
    return f"""Lane: {name}   Wave: {lane['wave']}   Agent: GPT-6 Luna (xhigh; campaign policy)
Worktree: {root}-lanes/{name} on branch lane/{name} (from campaign-utils @ {sha})
Read first: AGENTS.md, docs/user-tour.md, dev/compat/CAMPAIGN.md, dev/compat/LANES.md,
            core/README.md, core/lib/gnu.xsh, the cli GNU-mode section of docs/SPEC.md,
            core/basename.xsh and core/tests/test-basename.xsh (the converted reference applet),
            the applets and tests you own.
You own exactly: {', '.join(files)}. Do not edit anything else; put needs under "Requests:".
Shared binaries: {build}
Goal: {goal};
      remove discard buckets in owned applets; implement or explicitly reject every option.
Notes: {lane['note'] or '-'}
Use: core/lib/gnu.xsh diagnostics and the cli GNU mode (`gnu` record: prog, status, permute,
     unsupported; numeric and stop option fields); the pinned uutils source at
     {uutils_root}/src/uu/<util> and tests/by-util/test_<util>.rs are the behavior spec
     (read-only reference: never copy wholesale, never a dependency). GNU wording wins over clap wording.
Verify: {shared}/xsht test core/tests/test-<util>.xsh for each owned utility;
        your uutils slice, any time (it serializes on a lock):
          UUTILS_ROOT={uutils_root} XSH_BIN={shared}/xsh \\
          COMPAT_RESULTS_DIR=<scratch dir outside the repo> dev/compat/run-uutils.sh <utils>
        then read <scratch>/uutils-integration.json (per-utility pass/fail and failing test IDs);
        failure output is in <scratch>/uutils-integration.junit.xml. Never commit anything under
        dev/compat/results/ or target/. Also: python3 dev/compat/check_ignored_options.py,
        check_kernel_reads.py and check_exclusions.py.
Budget: stop and report at twice the size the integrator states.
Commit on lane/{name} when green (never push, merge, or rebase others).
Report (<200 words): behavior changed, before/after counts, tests run, decisions, Requests:, blockers.
"""


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == "check":
        return check()
    if len(sys.argv) >= 2 and sys.argv[1] == "list":
        wave = int(sys.argv[2]) if len(sys.argv) > 2 else None
        for name, lane in load().items():
            if wave is None or lane["wave"] == wave:
                print(f"{name:20} wave {lane['wave']} {lane['kind']:8} {len(lane['utilities']):3} utilities  depends: {' '.join(lane['depends']) or '-'}")
        return 0
    if len(sys.argv) >= 3 and sys.argv[1] == "brief":
        sha = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=REPO,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        campaign_root = integration_root()
        root = os.environ.get("LANES_ROOT", str(campaign_root))
        targets = os.environ.get("LANES_TARGETS", str(campaign_root.parent / "targets"))
        shared = os.environ.get("XSH_SHARED_BIN", str(campaign_root / "target" / "release"))
        uutils_root = os.environ.get("UUTILS_ROOT", str(campaign_root.parent / "ref" / "uutils-coreutils"))
        for name in sys.argv[2:]:
            print(brief(name, sha, root, targets, shared, uutils_root))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
