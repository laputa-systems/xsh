#!/usr/bin/env python3
"""Per-utility lane tooling for the compatibility campaign (Claude Code workflow).

    lane.py plan  [--donor REV]        rank utilities by uutils tests the donor passes and master fails
    lane.py new   LANE... [--donor REV] worktree + branch + brief for each lane
    lane.py new   LANE --utils U... [--own PATH...] [--donor REV]
                                       a lane over several utilities and shared library files
    lane.py brief UTIL [--donor REV]   print the lane brief
    lane.py gate  UTIL [--committed]   acceptance: ownership, native test, uutils slice
    lane.py accept LANE... [-m TEXT]   after reviewing the diff: committed gate, merge --no-ff, drop
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
# Each suite has a runner, the report file the runner writes into COMPAT_RESULTS_DIR, and the
# committed baseline. A lane is gated on every suite that knows its utilities, so a fix for one
# suite cannot regress another.
SUITES = {
    "uutils": {"runner": "dev/compat/run-uutils.sh", "report": "uutils-integration.json",
               "baseline": "dev/compat/results/uutils-integration.json"},
    "busybox": {"runner": "dev/compat/run-busybox.sh", "report": "busybox.json",
                "baseline": "dev/compat/results/busybox.json"},
}
BUSYBOX_SOURCE_ROOT = MASTER / ".work" / "upstream" / "busybox"
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


def is_owned(lane: str, path: str) -> bool:
    """A path is owned when it is listed, or lies under an owned directory (an entry ending in /)."""
    files = owned(lane)
    return path in files or any(f.endswith("/") and path.startswith(f) for f in files)


def results_at(rev: str, suite: str = "uutils") -> dict:
    """Per-utility results committed at REV, or {} when REV has none (a branch that never ran the suite)."""
    text = git("show", f"{rev}:{SUITES[suite]['baseline']}", cwd=MASTER, check=False)
    return json.loads(text)["utilities"] if text else {}


def excluded_at(rev: str) -> set[str]:
    data = json.loads(git("show", f"{rev}:dev/compat/exclusions.json", cwd=MASTER))
    return {t["id"] for t in data["tests"]}


def targets(utils: list[str], donor: str | None, suite: str = "uutils") -> list[str]:
    """Test IDs the lane should fix: failing on master, passing on the donor when one is given."""
    skip = excluded_at("master") if suite == "uutils" else set()
    out: list[str] = []
    for util in utils:
        base = results_at("master", suite).get(util)
        if base is None:
            sys.exit(f"{util}: no master baseline entry in {suite}")
        failing = [t for t in base["failing"] if t not in skip and not SNIPPET_TESTS.search(t)]
        if donor:
            other = results_at(donor, suite).get(util)
            if other is None:
                continue
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
    suite = info.get("suite", "uutils")
    fix = targets(utils, None, suite)
    proven = targets(utils, donor, suite) if donor else []
    fix = proven + [t for t in fix if t not in set(proven)]
    files = sorted(owned(util))
    if suite == "busybox":
        spec_text = (f"the BusyBox tests under {BUSYBOX_SOURCE_ROOT}/ (find the testsuite directory for each applet: "
                     f"testsuite/<applet>.tests or testsuite/<applet>/); each test is a name, a command and its expected "
                     f"output. They are read-only. Also the GNU manual behavior for the applet.")
    else:
        spec_text = " and ".join(f"{UUTILS_ROOT}/src/uu/{u}/ with {UUTILS_ROOT}/tests/by-util/test_{u}.rs" for u in utils)
    native = bool(info.get("native"))
    target_dir = LANES_ROOT / "_targets" / util
    cargo_rule = "" if native else "cargo, "
    native_block = f"""
Native lane (Rust under the owned directories is allowed; XSH-first still applies: the smallest primitive only):
  build: flock {LANES_ROOT}/_targets/cargo.lock env LD_PRELOAD=/usr/lib/libjemalloc.so.2 CARGO_TARGET_DIR={target_dir} \\
         cargo build --release --target x86_64-unknown-linux-musl -p xsh --bins -p xsht --bin xsht
  then use XSH_BIN={target_dir}/x86_64-unknown-linux-musl/release/xsh for `xsht test` (its sibling) and for the gate
  (export XSH_BIN before running lane.py gate). Run the focused Rust tests with `cargo test --release --test NAME`
  in the same target directory; never a workspace-wide test run. Register every new native function in the
  registry the way its neighbours are (signature, docs) and add a native XSH test for it.""" if native else ""
    watch = info.get("watch", [])
    watch_note = (f"\nThe gate also runs these consumers of your library files and fails on any regression there: "
                  f"{', '.join(watch)}.") if watch else ""
    scratch = SCRATCH / util
    scratch.mkdir(parents=True, exist_ok=True)
    (scratch / "targets.txt").write_text("\n".join(fix) + "\n")
    sha = git("rev-parse", "--short", "master", cwd=MASTER).strip()
    shown = "\n".join(f"    {t}" + ("   (donor passes: port first)" if t in set(proven) else "")
                      for t in fix[:40])
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

Goal: make these {suite} tests pass ({len(fix)} total; all currently fail on master).
Some tests may be impossible for a host reason (a permission or group the unprivileged test user lacks):
if a test fails identically on an untouched copy, say so under Requests: as `host-conflict` and move on.
{shown}{more}
Every uutils test that passes on master today must keep passing; the gate reports regressions by ID.{watch_note}
{donor_block}
Specification: {spec_text}
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
  4. On GATE PASS: git add -A && git commit -m "{util}: <what changed>"   (the gate already proved
     that only owned files changed; a test file that does not exist yet may be created)
Stop and report when GATE PASS and committed, or after 3 failed attempts on one test ID
(list it as unresolved and move on), or after 6 gate runs.
Scope: pass tests. No formatting, linting, refactors, cleanup of unrelated code, or performance work.
Probes: run `xsh` ad hoc only under `perl -e 'alarm 60; exec @ARGV' ...`. Never run formatters, `xsht fmt`,
`xsht lint --fix`, {cargo_rule}or git push/merge/rebase. Leave no process running.{native_block}
Report (under 150 words): gate result line, tests fixed vs unresolved, Requests:, Language gaps:, blockers.
"""


def new(lanes: list[str], donor: str | None, utils: list[str] | None, own: list[str], watch: list[str],
        native: bool = False, suite: str = "uutils") -> None:
    if utils and len(lanes) != 1:
        sys.exit("--utils describes exactly one lane")
    for util in lanes:
        wt = lane_dir(util)
        if wt.exists():
            sys.exit(f"{wt} already exists")
        (SCRATCH / util).mkdir(parents=True, exist_ok=True)
        (SCRATCH / util / "lane.json").write_text(json.dumps(
            {"utils": utils or [util], "own": own, "watch": watch, "donor": donor, "native": native, "suite": suite}))
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
    stray = sorted(path for path in changed if not is_owned(util, path))
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
    watched = [w for w in spec(util).get("watch", []) if w not in utils]
    for name in utils + watched:
        if not (wt / f"core/tests/test-{name}.xsh").exists():
            continue
        native = subprocess.run([str(XSHT), "test", f"core/tests/test-{name}.xsh"], cwd=wt, env=env,
                                capture_output=True, text=True, timeout=600, stdin=subprocess.DEVNULL)
        if native.returncode != 0:
            fail.append(f"native test {name} failed:\n" + "\n".join(native.stdout.splitlines()[-15:]))

    skip = excluded_at("master")
    primary = spec(util).get("suite", "uutils")
    targets_file = SCRATCH / util / "targets.txt"
    target_ids = targets_file.read_text().splitlines() if targets_file.exists() else []
    lines = []
    gained = False
    for suite, info in SUITES.items():
        base_all = results_at("master", suite)
        names = [n for n in utils + watched if n in base_all]
        if not names:
            continue
        env_suite = dict(env)
        if suite == "busybox":
            env_suite["BUSYBOX_SOURCE_ROOT"] = str(BUSYBOX_SOURCE_ROOT)
        # A report from an earlier run must never stand in for this one.
        (scratch / info["report"]).unlink(missing_ok=True)
        run = subprocess.run([info["runner"], *names], cwd=wt, env=env_suite, capture_output=True, text=True,
                             stdin=subprocess.DEVNULL)
        (scratch / f"run-{suite}.log").write_text(run.stdout + run.stderr)
        # Runners exit nonzero when tests fail; only a missing report is a harness failure.
        try:
            report = json.loads((scratch / info["report"]).read_text())["utilities"]
        except FileNotFoundError:
            fail.append(f"{suite} slice produced no report; see {scratch}/run-{suite}.log")
            continue
        for name in names:
            base, new_entry = base_all[name], report.get(name)
            if new_entry is None:
                fail.append(f"no {suite} report entry for {name}")
                continue
            ignored = skip if suite == "uutils" else set()
            regressed = sorted(set(new_entry["failing"]) - set(base["failing"]) - ignored)
            fixed = sorted(set(base["failing"]) - set(new_entry["failing"]))
            gained = gained or bool(fixed)
            lines.append(f"{suite} {name}: {base['pass']} -> {new_entry['pass']} pass, "
                         f"{len(fixed)} fixed, {len(regressed)} regressed")
            if regressed:
                fail.append(f"regressed in {suite} {name}: " + ", ".join(regressed))
            remaining = [t for t in target_ids if t in new_entry["failing"]] \
                if name in utils and suite == primary else []
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


def accept(lanes: list[str], note: str | None) -> int:
    """The integrator's merge step. Review `git diff master...claude/LANE` before calling this."""
    for lane in lanes:
        gate_out = subprocess.run([sys.executable, str(HERE), "gate", lane, "--committed"],
                                  capture_output=True, text=True)
        if gate_out.returncode != 0:
            print(gate_out.stdout + gate_out.stderr)
            print(f"{lane}: gate failed, not merged")
            return 1
        summary = "; ".join(l for l in gate_out.stdout.splitlines()
                            if l.startswith(("uutils ", "busybox ")) and "regressed" in l)
        message = f"{lane}: {note + ' ' if note else ''}({summary})\n\nCo-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
        git("merge", "--no-ff", "-q", f"claude/{lane}", "-m", message, cwd=MASTER)
        drop([lane], False)
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
            p.add_argument("--watch", nargs="+", default=[])
            p.add_argument("--native", action="store_true")
            p.add_argument("--suite", choices=list(SUITES), default="uutils")
    p = sub.add_parser("gate")
    p.add_argument("util")
    p.add_argument("--committed", action="store_true")
    p = sub.add_parser("accept")
    p.add_argument("lanes", nargs="+")
    p.add_argument("-m", dest="note")
    p = sub.add_parser("drop")
    p.add_argument("utils", nargs="+")
    p.add_argument("--force", action="store_true")
    args = parser.parse_args()
    if args.cmd == "plan":
        plan(args.donor)
    elif args.cmd == "new":
        new(args.utils, args.donor, args.members, args.own, args.watch, args.native, args.suite)
    elif args.cmd == "brief":
        print(brief(args.utils[0], args.donor))
    elif args.cmd == "gate":
        return gate(args.util, args.committed)
    elif args.cmd == "accept":
        return accept(args.lanes, args.note)
    else:
        drop(args.utils, args.force)
    return 0


if __name__ == "__main__":
    sys.exit(main())
