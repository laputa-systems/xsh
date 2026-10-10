#!/usr/bin/env python3
"""Per-utility lane tooling for the compatibility campaign (Claude Code workflow).

    lane.py plan  [--donor REV]        rank utilities by uutils tests the donor passes and master fails
    lane.py new   UTIL... [--donor REV] worktree + branch + brief for each utility
    lane.py brief UTIL [--donor REV]   print the lane brief
    lane.py gate  UTIL [--committed]   acceptance: ownership, native test, uutils slice
    lane.py drop  UTIL... [--force]    remove a merged lane's worktree and branch

A lane owns exactly core/UTIL.xsh and core/tests/test-UTIL.xsh in its own
worktree. `gate` is both the lane's milestone check and the integrator's
acceptance check, so a lane cannot pass a weaker gate than the one that merges it.
The baseline is always master's committed dev/compat/results/uutils-integration.json.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve()


def git(*args: str, cwd: Path | None = None, check: bool = True) -> str:
    done = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)
    if check and done.returncode != 0:
        sys.exit(f"git {' '.join(args)}: {done.stderr.strip()}")
    return done.stdout


# The primary checkout owns the shared binaries, the baseline and the lane
# worktrees, wherever this script is invoked from (a lane worktree has its own
# copy of the script).
COMMON = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir", cwd=HERE.parent).strip())
MASTER = COMMON.parent
LANES_ROOT = MASTER.parent / "xsh-claude-lanes"
SCRATCH = MASTER / ".work" / "claude-campaign" / "lanes"
UUTILS_ROOT = Path(os.environ.get("UUTILS_ROOT", MASTER.parent / "ref" / "uutils-coreutils"))
XSH_BIN = Path(os.environ.get("XSH_BIN", MASTER / "target/x86_64-unknown-linux-musl/release/xsh"))
XSHT = XSH_BIN.parent / "xsht"
DEFAULT_DONOR = "origin/campaign-utils"
RESULTS_PATH = "dev/compat/results/uutils-integration.json"
# uutils renders argument errors as framed source snippets. Matching them changes
# the user-visible diagnostic contract (master follows GNU wording), so those
# tests are not lane targets until that decision is made.
SNIPPET_TESTS = re.compile(r"snippet|diagnostic")


def lane_dir(util: str) -> Path:
    return LANES_ROOT / util


def owned(util: str) -> set[str]:
    return {f"core/{util}.xsh", f"core/tests/test-{util}.xsh"}


def results_at(rev: str) -> dict:
    return json.loads(git("show", f"{rev}:{RESULTS_PATH}", cwd=MASTER))["utilities"]


def excluded_at(rev: str) -> set[str]:
    data = json.loads(git("show", f"{rev}:dev/compat/exclusions.json", cwd=MASTER))
    return {t["id"] for t in data["tests"]}


def targets(util: str, donor: str | None) -> list[str]:
    """uutils test IDs the lane should fix: failing on master, passing on the donor when one is given."""
    base = results_at("master").get(util)
    if base is None:
        sys.exit(f"{util}: no master baseline entry")
    skip = excluded_at("master")
    failing = [t for t in base["failing"] if t not in skip and not SNIPPET_TESTS.search(t)]
    if donor:
        other = results_at(donor).get(util)
        if other is None:
            sys.exit(f"{util}: no entry at {donor}")
        failing = [t for t in failing if t not in set(other["failing"])]
    return sorted(failing)


def plan(donor: str) -> None:
    base, other = results_at("master"), results_at(donor)
    skip = excluded_at("master")
    rows = []
    for util in sorted(set(base) & set(other)):
        b, d = set(base[util]["failing"]) - skip, set(other[util]["failing"])
        wins, losses = sorted(b - d), sorted(d - b - skip)
        deferred = [t for t in wins if SNIPPET_TESTS.search(t)]
        wins = [t for t in wins if t not in deferred]
        if wins:
            rows.append((len(wins), len(losses), util, base[util]["pass"], other[util]["pass"], len(deferred)))
    print(f"{'util':14}{'wins':>5}{'master loses':>13}{'master pass':>12}{'donor pass':>11}{'snippet-deferred':>17}")
    for wins, losses, util, bp, dp, deferred in sorted(rows, reverse=True):
        print(f"{util:14}{wins:5}{losses:13}{bp:12}{dp:11}{deferred:17}")
    print(f"\nwins = failing on master, passing on {donor}, not excluded on master, not a snippet test.")
    print("A utility with many master-only passes needs hunk-level porting, never a file copy.")


def brief(util: str, donor: str | None) -> str:
    fix = targets(util, donor)
    scratch = SCRATCH / util
    scratch.mkdir(parents=True, exist_ok=True)
    (scratch / "targets.txt").write_text("\n".join(fix) + "\n")
    sha = git("rev-parse", "--short", "master", cwd=MASTER).strip()
    shown = "\n".join(f"    {t}" for t in fix[:40])
    more = f"\n    ... {len(fix) - 40} more in {scratch}/targets.txt" if len(fix) > 40 else ""
    wt = lane_dir(util)
    py = f"python3 {MASTER}/.claude/skills/xsh-compat-campaign/scripts/lane.py"
    donor_block = ""
    if donor:
        base = git("merge-base", "master", donor, cwd=MASTER).strip()
        donor_block = f"""
Donor (read-only reference, a different implementation of the same utility on `{donor}`):
    git show {donor}:core/{util}.xsh
    git show {donor}:core/tests/test-{util}.xsh
    git diff {base[:8]} {donor} -- core/{util}.xsh core/tests/test-{util}.xsh   # what the donor changed
Port by hunk: copy only the behavior needed for a target test into master's file, in
master's style. Never replace master's file with the donor's. The donor may call
helpers or native functions master lacks (an unknown-name checker error shows it):
do not copy library code; list the missing symbol under Requests: and skip that test.
Known differences: the donor reads raw argv bytes with `cli.argv_bytes()`, which master does
not have; master's applets take `proc main(...argv: List[Bytes])`. Keep master's entry shape.
Donor-only native APIs: git diff {base[:8]} {donor} -- crates/xsh-registry/src/signature/modules.rs
"""
    return f"""Lane: {util}   Agent: xsh-compat-lane (haiku, high)   Base: master @ {sha}
Worktree: {wt}   Branch: claude/{util}
First command, and the only place you work:
    cd {wt} && git rev-parse --abbrev-ref HEAD      # must print claude/{util}
You own exactly: core/{util}.xsh and core/tests/test-{util}.xsh (use absolute paths under the worktree).
Never edit anything else (core/lib/*, dev/*, src/*, crates/*, docs/*, results, gaps.json).
Anything you need elsewhere goes in your report under Requests: with the exact symbol or file.

Goal: make these uutils tests pass ({len(fix)} total; all currently fail on master):
{shown}{more}
Every uutils test that passes on master today must keep passing; the gate reports regressions by ID.
{donor_block}
Specification: {UUTILS_ROOT}/src/uu/{util}/ and {UUTILS_ROOT}/tests/by-util/test_{util}.rs
(read-only; never copy wholesale). GNU wording wins over clap wording. Tests in
dev/compat/exclusions.json are deliberately out of scope.

Loop (keep it this tight):
  1. Edit the owned files.
  2. After EVERY edit:   {XSHT} test core/tests/test-{util}.xsh      (about a second)
     Add or extend a `test NAME {{ ... }}` there for each behavior you change.
  3. Milestone only, at most 6 times: {py} gate {util}
     It takes minutes and queues behind other lanes. It prints GATE PASS or GATE FAIL
     with the exact test IDs that regressed or are still failing.
  4. On GATE PASS: git add core/{util}.xsh core/tests/test-{util}.xsh && git commit -m "{util}: <what changed>"
Stop and report when GATE PASS and committed, or after 3 failed attempts on one test ID
(list it as unresolved and move on), or after 6 gate runs.
Scope: pass tests. No formatting, linting, refactors, cleanup of unrelated code, or performance work.
Probes: run `xsh` ad hoc only under `perl -e 'alarm 60; exec @ARGV' ...`. Never run formatters, `xsht fmt`,
`xsht lint --fix`, cargo, or git push/merge/rebase. Leave no process running.
Report (under 150 words): gate result line, tests fixed vs unresolved, Requests:, blockers.
"""


def new(utils: list[str], donor: str | None) -> None:
    for util in utils:
        wt = lane_dir(util)
        if wt.exists():
            sys.exit(f"{wt} already exists")
        git("worktree", "add", "-b", f"claude/{util}", str(wt), "master", cwd=MASTER)
        text = brief(util, donor)
        (SCRATCH / util / "brief.txt").write_text(text)
        print(f"{util}: {wt}  brief: {SCRATCH / util / 'brief.txt'}")


def gate(util: str, committed: bool) -> int:
    wt = lane_dir(util)
    if not wt.is_dir():
        sys.exit(f"no lane worktree {wt}")
    scratch = SCRATCH / util / "gate"
    scratch.mkdir(parents=True, exist_ok=True)
    fail: list[str] = []

    changed = set(git("diff", "--name-only", "master", cwd=wt).split())
    status = git("status", "--porcelain", cwd=wt).splitlines()
    changed |= {line[3:] for line in status}
    stray = sorted(changed - owned(util))
    if stray:
        fail.append(f"ownership: files outside the lane: {', '.join(stray)}")
    if committed and status:
        fail.append("worktree has uncommitted changes")

    env = {**os.environ, "UUTILS_ROOT": str(UUTILS_ROOT), "XSH_BIN": str(XSH_BIN),
           "COMPAT_RESULTS_DIR": str(scratch), "TMPDIR": str(scratch / "tmp"),
           "PATH": f"{Path.home()}/.cargo/bin:{os.environ['PATH']}"}
    (scratch / "tmp").mkdir(exist_ok=True)

    native = subprocess.run([str(XSHT), "test", f"core/tests/test-{util}.xsh"], cwd=wt, env=env,
                            capture_output=True, text=True, timeout=600)
    if native.returncode != 0:
        fail.append("native test failed:\n" + "\n".join(native.stdout.splitlines()[-15:]))

    # A report from an earlier run must never stand in for this one.
    (scratch / "uutils-integration.json").unlink(missing_ok=True)
    slice_run = subprocess.run(["dev/compat/run-uutils.sh", util], cwd=wt, env=env,
                               capture_output=True, text=True)
    (scratch / "run.log").write_text(slice_run.stdout + slice_run.stderr)
    if slice_run.returncode != 0:
        fail.append(f"uutils slice harness failed ({slice_run.returncode}); see {scratch}/run.log")
    base = results_at("master")[util]
    try:
        new_entry = json.loads((scratch / "uutils-integration.json").read_text())["utilities"][util]
    except (FileNotFoundError, KeyError):
        fail.append(f"uutils slice produced no report; see {scratch}/run.log")
        new_entry = None

    summary = ""
    if new_entry is not None:
        skip = excluded_at("master")
        regressed = sorted(set(new_entry["failing"]) - set(base["failing"]) - skip)
        fixed = sorted(set(base["failing"]) - set(new_entry["failing"]))
        summary = (f"uutils {util}: {base['pass']} -> {new_entry['pass']} pass, "
                   f"{len(fixed)} fixed, {len(regressed)} regressed")
        if regressed:
            fail.append("regressed: " + ", ".join(regressed))
        elif not fixed and not fail:
            summary += " (no gain)"
        remaining = [t for t in (SCRATCH / util / "targets.txt").read_text().split() if t in new_entry["failing"]] \
            if (SCRATCH / util / "targets.txt").exists() else []
        if remaining:
            summary += f"\nstill failing from the target list ({len(remaining)}): " + ", ".join(remaining[:20])
    print(summary)
    if fail:
        print("GATE FAIL")
        for line in fail:
            print(f"  {line}")
        return 1
    print("GATE PASS" if "(no gain)" not in summary else "GATE NOOP")
    return 0


def drop(utils: list[str], force: bool) -> None:
    for util in utils:
        merged = git("branch", "--merged", "master", "--list", f"claude/{util}", cwd=MASTER).strip()
        if not merged and not force:
            sys.exit(f"claude/{util} is not merged into master; pass --force to discard it")
        git("worktree", "remove", "--force", str(lane_dir(util)), cwd=MASTER)
        git("branch", "-D", f"claude/{util}", cwd=MASTER)
        print(f"{util}: removed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("plan", "new", "brief"):
        p = sub.add_parser(name)
        p.add_argument("--donor", default=DEFAULT_DONOR if name == "plan" else None)
        if name != "plan":
            p.add_argument("utils", nargs="+" if name == "new" else 1)
    p = sub.add_parser("gate")
    p.add_argument("util")
    p.add_argument("--committed", action="store_true")
    p = sub.add_parser("drop")
    p.add_argument("utils", nargs="+")
    p.add_argument("--force", action="store_true")
    args = parser.parse_args()
    if args.cmd == "plan":
        plan(args.donor)
    elif args.cmd == "new":
        new(args.utils, args.donor)
    elif args.cmd == "brief":
        print(brief(args.utils[0], args.donor))
    elif args.cmd == "gate":
        return gate(args.util, args.committed)
    else:
        drop(args.utils, args.force)
    return 0


if __name__ == "__main__":
    sys.exit(main())
