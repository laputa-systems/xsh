import hashlib
import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, stdout_jsonl, write

SLOWSUM = """#!/bin/sh
printf 'start %s\\n' "$1" >> /work/.slowsum/runs.log
sleep 0.3
if [ ! -r "$1" ] || [ -d "$1" ]; then
  printf 'end %s\\n' "$1" >> /work/.slowsum/runs.log
  printf 'slowsum: cannot read %s\\nsee slowsum(1)\\n' "$1" >&2
  exit 2
fi
sum=$(sha256sum < "$1" | cut -d ' ' -f 1)
printf 'end %s\\n' "$1" >> /work/.slowsum/runs.log
printf '%s\\n' "$sum"
"""

PLANS = {
    "public": {
        "files": {"data/a.txt": "alpha\n", "data/b.txt": "bravo\n", "data/c.txt": "charlie\n", "data/d.txt": "delta\n",
                  "data/e.txt": "echo\n"},
        "list": ["data/a.txt", "data/b.txt", "data/missing.txt", "data/c.txt", "/work/data/d.txt", "data/e.txt"],
    },
    "spaces": {
        "files": {f"in put/file {n}.dat": f"payload {n}\n" * (n + 1) for n in range(9)},
        "list": [f"  in put/file {n}.dat " for n in range(9)] + ["", "in put", "in put/file 9.dat"],
    },
    "healthy": {
        "files": {"x": "1", "y": "22", "z": "333", "w": ""},
        "list": ["x", "y", "z", "w"],
    },
}


def listed(variant: str) -> list[str]:
    return [line.strip() for line in PLANS[variant]["list"] if line.strip()]


def setup(variant: str, work: Path) -> Setup:
    write(work, "bin/slowsum", SLOWSUM, mode=0o755)
    (work / ".slowsum").mkdir()
    write(work, ".slowsum/runs.log", "")
    for name, content in PLANS[variant]["files"].items():
        write(work, name, content)
    write(work, "files.txt", "\n".join(PLANS[variant]["list"]) + "\n")
    return Setup()


def expected_row(variant: str, path: str) -> dict:
    files = PLANS[variant]["files"]
    key = path.removeprefix("/work/")
    if key in files:
        return {"file": path, "sha256": hashlib.sha256(files[key].encode()).hexdigest()}
    return {"file": path, "error": f"slowsum: cannot read {path}"}


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    events = (work / ".slowsum" / "runs.log").read_text().splitlines()
    running, peak = 0, 0
    starts: dict[str, int] = {}
    for event in events:
        kind, _, path = event.partition(" ")
        if kind == "start":
            running += 1
            peak = max(peak, running)
            starts[path] = starts.get(path, 0) + 1
        else:
            running -= 1
    paths = listed(variant)
    if any(starts.get(p, 0) != 1 for p in paths) or set(starts) - set(paths):
        return Verdict.wrong("slowsum did not run exactly once per listed path", peak=peak)
    if peak > 3:
        return Verdict.wrong(f"{peak} runs overlapped, more than 3", peak=peak)
    if peak < 2:
        return Verdict.wrong("the runs never overlapped", peak=peak)
    rows, problem = stdout_jsonl(outcome)
    if problem:
        return Verdict.wrong(problem, peak=peak)
    want = [expected_row(variant, p) for p in paths]
    if rows != want:
        return Verdict.wrong("the printed lines differ from the expected result", peak=peak)
    status = 0 if all("sha256" in r for r in want) else 4
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}", peak=peak)
    return Verdict.ok(peak=peak)
