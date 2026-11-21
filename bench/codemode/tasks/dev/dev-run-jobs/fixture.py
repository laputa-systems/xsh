import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, write

PROBE = """#!/bin/sh
printf 'cwd=%s\\n' "$(pwd)"
for arg in "$@"; do printf '<%s>\\n' "$arg"; done
printf 'argc=%s\\n' "$#"
if [ -n "$PROBE_SHOW" ]; then
  for name in $PROBE_SHOW; do printf '%s=%s\\n' "$name" "$(printenv "$name")" >&2; done
fi
exit "${PROBE_EXIT:-0}"
"""

PLANS = {
    "public": [
        {"name": "list", "argv": ["/work/bin/probe", "two words", "", "--flag=1"], "cwd": ".", "env": {}},
        {"name": "inside", "argv": ["/work/bin/probe"], "cwd": "sub dir",
         "env": {"PROBE_SHOW": "GREETING INHERITED", "GREETING": "hello world"}},
        {"name": "failing", "argv": ["/work/bin/probe", "x"], "cwd": ".", "env": {"PROBE_EXIT": "3"}},
        {"name": "missing", "argv": ["/work/bin/not-installed", "a"], "cwd": ".", "env": {}},
    ],
    "quoting": [
        {"name": "glob", "argv": ["/work/bin/probe", "*.txt", "$HOME", "`id`", "a;b", "'q'"], "cwd": ".", "env": {}},
        {"name": "empties", "argv": ["/work/bin/probe", "", "", " "], "cwd": "deep/er", "env": {}},
        {"name": "env-only", "argv": ["/work/bin/probe"], "cwd": ".",
         "env": {"PROBE_SHOW": "A B", "A": "x y", "B": "$A"}},
        {"name": "fails", "argv": ["/work/bin/probe", "-"], "cwd": "deep", "env": {"PROBE_EXIT": "17"}},
    ],
    "clean": [
        {"name": "one", "argv": ["/work/bin/probe", "1"], "cwd": ".", "env": {}},
        {"name": "two", "argv": ["/work/bin/probe", "2"], "cwd": "sub dir", "env": {"PROBE_SHOW": "INHERITED"}},
    ],
}
INHERITED = "from-the-parent"


def run_probe(job: dict, base: dict[str, str]) -> dict:
    program = job["argv"][0]
    if program != "/work/bin/probe":
        return {"name": job["name"], "exit": None, "stdout": "", "stderr": ""}
    env = {**base, **job["env"]}
    cwd = "/work" if job["cwd"] == "." else f"/work/{job['cwd']}"
    args = job["argv"][1:]
    out = f"cwd={cwd}\n" + "".join(f"<{a}>\n" for a in args) + f"argc={len(args)}\n"
    err = "".join(f"{n}={env.get(n, '')}\n" for n in env.get("PROBE_SHOW", "").split()) if env.get("PROBE_SHOW") else ""
    return {"name": job["name"], "exit": int(env.get("PROBE_EXIT", "0")), "stdout": out, "stderr": err}


def setup(variant: str, work: Path) -> Setup:
    write(work, "bin/probe", PROBE, mode=0o755)
    for job in PLANS[variant]:
        if job["cwd"] != ".":
            (work / job["cwd"]).mkdir(parents=True, exist_ok=True)
    write(work, "jobs.json", json.dumps(PLANS[variant], indent=2))
    return Setup(env={"INHERITED": INHERITED})


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    want = [run_probe(job, {"INHERITED": INHERITED}) for job in PLANS[variant]]
    status = 0 if all(r["exit"] == 0 for r in want) else 4
    path = work / "results.json"
    if not path.is_file():
        return Verdict.wrong("results.json was not written")
    try:
        found = json.loads(path.read_text())
    except json.JSONDecodeError:
        return Verdict.wrong("results.json is not JSON")
    if not isinstance(found, list) or len(found) != len(want):
        return Verdict.wrong("results.json does not hold one result per job")
    for got, expected in zip(found, want):
        if got != expected:
            fields = sorted(k for k in expected if not isinstance(got, dict) or got.get(k) != expected[k])
            return Verdict.wrong(f"job {expected['name']}: {', '.join(fields)} differ")
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    return Verdict.ok()
