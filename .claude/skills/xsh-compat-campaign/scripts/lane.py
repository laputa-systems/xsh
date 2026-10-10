#!/usr/bin/env python3
"""Per-utility lane tooling for the compatibility campaign (Claude Code workflow).

    lane.py plan  [--donor REV]        rank utilities by uutils tests the donor passes and master fails
    lane.py new   LANE... [--donor REV] worktree + branch + brief for each lane
    lane.py new   LANE --utils U... [--own PATH...] [--donor REV]
                                       a lane over several utilities and shared library files
    lane.py brief UTIL [--donor REV]   print the lane brief
    lane.py gate  UTIL [--committed]   acceptance: ownership, native test, uutils slice
    lane.py drop  UTIL... [--force]    remove a merged lane's worktree and branch

A lane owns exactly core/UTIL.xsh and core/tests/test-UTIL.xsh in its own
worktree. A library lane (`--utils`, `--own`) also owns the named library files
and gates every utility that consumes them; its spec is lane.json in the
lane's scratch directory. `gate` is both the lane's milestone check and the integrator's
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


def upstream_problems() -> list[str]:
    """Why the pinned uutils checkout is not exactly the pinned upstream, if it is not.

    The upstream tests are the oracle; editing, deleting or adding to them is cheating.
    """
    pin = json.loads((MASTER / "dev/compat/upstream.lock.json").read_text())["uutils"]["commit"]
    problems = []
    head = git("rev-parse", "HEAD", cwd=UUTILS_ROOT).strip()
    if head != pin:
        problems.append(f"upstream checkout is at {head[:12]}, not the pinned {pin[:12]}")
    # Untracked build output (target/, docs/tldr.zip) is not tampering; tests/ and src/ are.
    changed = git("status", "--porcelain", "--untracked-files=all", "--", "tests", "src", cwd=UUTILS_ROOT).split("\n")
    changed = [line for line in changed if line.strip()]
    if changed:
        problems.append("upstream tests or sources were modified: " + "; ".join(changed[:8]))
    return problems


def lane_dir(util: str) -> Path:
    return LANES_ROOT / util


def spec(lane: str) -> dict:
    path = SCRATCH / lane / "lane.json"
    if path.exists():
        return json.loads(path.read_text())
    return {"utils": [lane], "own": [], "donor": None}


def owned(lane: str) -> set[str]:
    info = spec(lane)
    files = set(info["own"])
    for util in info["utils"]:
        files |= {f"core/{util}.xsh", f"core/tests/test-{util}.xsh"}
    return files


def results_at(rev: str) -> dict:
    return json.loads(git("show", f"{rev}:{RESULTS_PATH}", cwd=MASTER))["utilities"]


def excluded_at(rev: str) -> set[str]:
    data = json.loads(git("show", f"{rev}:dev/compat/exclusions.json", cwd=MASTER))
    return {t["id"] for t in data["tests"]}


def targets(utils: list[str], donor: str | None) -> list[str]:
    """uutils test IDs the lane should fix: failing on master, passing on the donor when one is given."""
    skip = excluded_at("master")
    out: list[str] = []
    for util in utils:
        base = results_at("master").get(util)
        if base is None:
            sys.exit(f"{util}: no master baseline entry")
        failing = [t for t in base["failing"] if t not in skip and not SNIPPET_TESTS.search(t)]
        if donor:
            other = results_at(donor).get(util)
            if other is None:
                sys.exit(f"{util}: no entry at {donor}")
            failing = [t for t in failing if t not in set(other["failing"])]
        out += failing
    return sorted(out)


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
    info = spec(util)
    utils = info["utils"]
    fix = targets(utils, donor)
    files = sorted(owned(util))
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
Donor (read-only reference, a different implementation of the same utilities on `{donor}`):
    git show {donor}:<owned path>
    git diff {base[:8]} {donor} -- {" ".join(files)}   # what the donor changed
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
You own exactly (use absolute paths under the worktree):
{chr(10).join("    " + f for f in files)}
Never edit anything else (other core/lib/*, dev/*, src/*, crates/*, docs/*, results, gaps.json).
Anything you need elsewhere goes in your report under Requests: with the exact symbol or file.

Goal: make these uutils tests pass ({len(fix)} total; all currently fail on master):
{shown}{more}
Every uutils test that passes on master today must keep passing; the gate reports regressions by ID.
{donor_block}
Specification: {" and ".join(f"{UUTILS_ROOT}/src/uu/{u}/ with {UUTILS_ROOT}/tests/by-util/test_{u}.rs" for u in utils)}
(read-only; never copy wholesale). GNU wording wins over clap wording. Tests in
dev/compat/exclusions.json are deliberately out of scope.

Upstream tests are read-only. Never edit, delete, skip, add to, or special-case the pinned uutils or GNU
tests, fixtures, or their harness (the checkout under {UUTILS_ROOT} is read-only and the gate verifies it
is untouched). A test passes only when the unmodified upstream test passes against XSH.

XSH first. Implement behavior in XSH, in the owned files. Do not ask for Rust unless XSH cannot express a
reusable OS or byte boundary (a syscall, descriptor operation, or codec). If XSH makes something awkward,
that is a finding: describe it under "Language gaps:" in your report (what you wrote, what you wanted).

Wording rule: GNU's output wins. If the only way to pass a test is to print text that differs from
GNU (clap or uutils wording, a "(invalid value 'X')" suffix, a bare strerror where GNU prints
"write error: ...", a uutils version line), or to detect the test harness (XSH_EXECUTION_PHRASE,
"xsh-uutests"), change nothing for that test and list its ID under Requests: as `wording-conflict`.
Never edit an existing native test's expected text to make a uutils test pass.

Loop (keep it this tight):
  1. Edit the owned files.
  2. After EVERY edit:   {"; ".join(f"{XSHT} test core/tests/test-{u}.xsh" for u in utils)}      (seconds)
     Add or extend a `test NAME {{ ... }}` there for each behavior you change.
  3. Milestone only, at most 6 times: {py} gate {util}
     It takes minutes and queues behind other lanes. It prints GATE PASS or GATE FAIL
     with the exact test IDs that regressed or are still failing.
  4. On GATE PASS: git add {" ".join(files)} && git commit -m "{util}: <what changed>"
Stop and report when GATE PASS and committed, or after 3 failed attempts on one test ID
(list it as unresolved and move on), or after 6 gate runs.
Scope: pass tests. No formatting, linting, refactors, cleanup of unrelated code, or performance work.
Probes: run `xsh` ad hoc only under `perl -e 'alarm 60; exec @ARGV' ...`. Never run formatters, `xsht fmt`,
`xsht lint --fix`, cargo, or git push/merge/rebase. Leave no process running.
Report (under 150 words): gate result line, tests fixed vs unresolved, Requests:, Language gaps:, blockers.
"""


def new(lanes: list[str], donor: str | None, utils: list[str] | None, own: list[str]) -> None:
    if utils and len(lanes) != 1:
        sys.exit("--utils describes exactly one lane")
    for util in lanes:
        wt = lane_dir(util)
        if wt.exists():
            sys.exit(f"{wt} already exists")
        (SCRATCH / util).mkdir(parents=True, exist_ok=True)
        (SCRATCH / util / "lane.json").write_text(json.dumps(
            {"utils": utils or [util], "own": own, "donor": donor}))
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

    # Against the fork point, not master's tip: master advances as other lanes merge.
    fork = git("merge-base", "master", "HEAD", cwd=wt).strip()
    changed = set(git("diff", "--name-only", fork, cwd=wt).split())
    status = git("status", "--porcelain", cwd=wt).splitlines()
    changed |= {line[3:] for line in status}
    stray = sorted(changed - owned(util))
    if stray:
        fail.append(f"ownership: files outside the lane: {', '.join(stray)}")
    if committed and status:
        fail.append("worktree has uncommitted changes")
    fail += upstream_problems()

    env = {**os.environ, "UUTILS_ROOT": str(UUTILS_ROOT), "XSH_BIN": str(XSH_BIN),
           "COMPAT_RESULTS_DIR": str(scratch), "TMPDIR": str(scratch / "tmp"),
           "PATH": f"{Path.home()}/.cargo/bin:{os.environ['PATH']}"}
    (scratch / "tmp").mkdir(exist_ok=True)

    utils = spec(util)["utils"]
    for name in utils:
        native = subprocess.run([str(XSHT), "test", f"core/tests/test-{name}.xsh"], cwd=wt, env=env,
                                capture_output=True, text=True, timeout=600)
        if native.returncode != 0:
            fail.append(f"native test {name} failed:\n" + "\n".join(native.stdout.splitlines()[-15:]))

    # A report from an earlier run must never stand in for this one.
    (scratch / "uutils-integration.json").unlink(missing_ok=True)
    slice_run = subprocess.run(["dev/compat/run-uutils.sh", *utils], cwd=wt, env=env,
                               capture_output=True, text=True)
    (scratch / "run.log").write_text(slice_run.stdout + slice_run.stderr)
    if slice_run.returncode != 0:
        fail.append(f"uutils slice harness failed ({slice_run.returncode}); see {scratch}/run.log")
    try:
        report = json.loads((scratch / "uutils-integration.json").read_text())["utilities"]
    except FileNotFoundError:
        fail.append(f"uutils slice produced no report; see {scratch}/run.log")
        report = {}

    lines = []
    gained = False
    skip = excluded_at("master")
    targets_file = SCRATCH / util / "targets.txt"
    target_ids = targets_file.read_text().split() if targets_file.exists() else []
    for name in utils:
        base = results_at("master")[name]
        new_entry = report.get(name)
        if new_entry is None:
            fail.append(f"no report entry for {name}")
            continue
        regressed = sorted(set(new_entry["failing"]) - set(base["failing"]) - skip)
        fixed = sorted(set(base["failing"]) - set(new_entry["failing"]))
        gained = gained or bool(fixed)
        lines.append(f"uutils {name}: {base['pass']} -> {new_entry['pass']} pass, "
                     f"{len(fixed)} fixed, {len(regressed)} regressed")
        if regressed:
            fail.append(f"regressed in {name}: " + ", ".join(regressed))
        remaining = [t for t in target_ids if t in new_entry["failing"]]
        if remaining:
            lines.append(f"still failing from the target list ({len(remaining)}): " + ", ".join(remaining[:20]))
    summary = "\n".join(lines)
    if not gained and not fail:
        summary += "\n(no gain)"
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
        if name == "new":
            p.add_argument("--utils", dest="members", nargs="+")
            p.add_argument("--own", nargs="+", default=[])
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
        new(args.utils, args.donor, args.members, args.own)
    elif args.cmd == "brief":
        print(brief(args.utils[0], args.donor))
    elif args.cmd == "gate":
        return gate(args.util, args.committed)
    else:
        drop(args.utils, args.force)
    return 0


if __name__ == "__main__":
    sys.exit(main())
