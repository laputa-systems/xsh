#!/usr/bin/env python3
"""Decide which final-qualification stage receipts carry forward to a later commit.

Every stage executes xsh or consumes a stage that does, so every tracked path
is an input of every stage except the planning records below, which no gate,
test, build, or documentation check reads. A receipt from an ancestor commit is
reusable only when the diff from it to the target touches none of those inputs.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

sys.dont_write_bytecode = True

STAGES = (
    "preflight",
    "scheduler",
    "supplemental",
    "bootstrap",
    "replay",
    "route",
    "privileged-ssh",
    "bind",
    "functional",
    "profile-cost",
    "observers",
    "forced-frames",
    "record-binary",
    "comparative",
)
PLANNING_RECORDS = frozenset({
    "TODO.md",
    "docs/ROADMAP.md",
    "dev/consolidation/CAMPAIGN.md",
    "dev/consolidation/HANDOFF.md",
    "dev/consolidation/ITEMS.md",
    "dev/consolidation/winddown/README.md",
})
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")


class ReuseError(ValueError):
    pass


def git(repo: Path, *args: str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, timeout=60)


def full_commit(repo: Path, commit: str) -> str:
    result = git(repo, "rev-parse", "--verify", "--end-of-options", f"{commit}^{{commit}}")
    resolved = result.stdout.decode().strip()
    if result.returncode != 0 or not COMMIT_RE.fullmatch(resolved):
        raise ReuseError(f"not a commit: {commit}")
    return resolved


def diff_command(receipt_commit: str, target_commit: str) -> list[str]:
    return ["git", "diff", "--no-renames", "--name-only", "-z", receipt_commit, target_commit]


def changed_paths(repo: Path, receipt_commit: str, target_commit: str) -> list[str]:
    result = git(repo, *diff_command(receipt_commit, target_commit)[1:])
    if result.returncode != 0:
        raise ReuseError(f"cannot diff {receipt_commit}..{target_commit}: {result.stderr.decode().strip()}")
    return sorted(path for path in result.stdout.decode("utf-8").split("\0") if path)


def stage_reuse(repo: Path, stage: str, receipt_commit: str, target_commit: str) -> dict:
    if stage not in STAGES:
        raise ReuseError(f"unknown stage: {stage}")
    receipt_commit = full_commit(repo, receipt_commit)
    target_commit = full_commit(repo, target_commit)
    if receipt_commit == target_commit:
        return {"stage": stage, "receipt_commit": receipt_commit, "target_commit": target_commit,
                "reuse": "fresh", "reusable": True}
    if git(repo, "merge-base", "--is-ancestor", receipt_commit, target_commit).returncode != 0:
        raise ReuseError(f"{stage} receipt commit {receipt_commit} is not an ancestor of {target_commit}")
    changed = changed_paths(repo, receipt_commit, target_commit)
    touched = [path for path in changed if path not in PLANNING_RECORDS]
    return {
        "stage": stage,
        "receipt_commit": receipt_commit,
        "target_commit": target_commit,
        "reuse": "carried-forward" if not touched else "rejected",
        "reusable": not touched,
        "diff_check": {
            "command": diff_command(receipt_commit, target_commit),
            "changed_paths": changed,
            "touched_inputs": touched,
            "non_inputs": sorted(PLANNING_RECORDS),
        },
    }


def acceptance_record(repo: Path, target_commit: str, receipt_commits: dict[str, str]) -> dict:
    missing = [stage for stage in STAGES if stage not in receipt_commits]
    if missing:
        raise ReuseError(f"no receipt commit for stages: {', '.join(missing)}")
    stages = [stage_reuse(repo, stage, receipt_commits[stage], target_commit) for stage in STAGES]
    return {
        "schema": 1,
        "target_commit": full_commit(repo, target_commit),
        "accepted": all(row["reusable"] for row in stages),
        "stages": stages,
    }


def parse_stage(value: str) -> tuple[str, str]:
    stage, separator, commit = value.partition("=")
    if not separator or not stage or not commit:
        raise argparse.ArgumentTypeError(f"expected STAGE=COMMIT, got {value!r}")
    return stage, commit


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--stage", type=parse_stage, action="append", required=True,
                        metavar="STAGE=COMMIT", help="commit whose passing receipt the stage uses")
    args = parser.parse_args()
    commits: dict[str, str] = {}
    for stage, commit in args.stage:
        if stage in commits:
            parser.error(f"stage given twice: {stage}")
        commits[stage] = commit
    try:
        record = acceptance_record(args.repo, args.target, commits)
    except ReuseError as error:
        print(f"stage reuse rejected: {error}", file=sys.stderr)
        return 2
    print(json.dumps(record, indent=2, sort_keys=True))
    return 0 if record["accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
