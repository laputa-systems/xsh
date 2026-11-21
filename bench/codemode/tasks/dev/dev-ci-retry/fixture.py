from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, rejected_inputs, stdout_jsonl, write

TRANSIENT = ("Timeout", "Unavailable", "RateLimited")
DELAYS_MS = (100, 200, 400)
TOLERANCE_MS = 15
SERVICE_MS = 60

PLANS = {
    "public": {
        "file": "acme/api\nacme/old\n\nacme/web\nacme/docs\n",
        "repos": {
            "acme/api": ["Unavailable", "Unavailable", "passing"],
            "acme/old": ["NotFound"],
            "acme/web": ["Timeout", "Timeout", "Timeout", "Timeout"],
            "acme/docs": ["failing"],
        },
    },
    "limits": {
        "file": "  tools/lint  \n\n\nlib/core\nlib/edge\r\nsvc/batch\n",
        "repos": {
            "tools/lint": ["RateLimited", "Unavailable", "Timeout", "pending"],
            "lib/core": ["Invalid"],
            "lib/edge": ["RateLimited", "Failed"],
            "svc/batch": ["RateLimited", "RateLimited", "RateLimited", "RateLimited", "passing"],
        },
    },
    "plain": {
        "file": "one\ntwo\nthree",
        "repos": {"one": ["passing"], "two": ["failing"], "three": ["passing"]},
    },
}


def order(variant: str) -> list[str]:
    return [line.strip() for line in PLANS[variant]["file"].splitlines() if line.strip()]


def result(step: str, repo: str) -> dict:
    if step[0].isupper():
        return {"error": {"kind": step, "message": f"{step} while reading {repo}"}, "delay_ms": SERVICE_MS}
    return {"output": {"repo": repo, "state": step}, "delay_ms": SERVICE_MS}


def expected_calls(steps: list[str]) -> int:
    calls = 0
    for step in steps[:4]:
        calls += 1
        if step not in TRANSIENT:
            return calls
    return calls


def expected_state(steps: list[str]) -> str:
    calls = expected_calls(steps)
    last = steps[calls - 1]
    return "missing" if last[0].isupper() else last


def setup(variant: str, work: Path) -> Setup:
    plan = PLANS[variant]
    write(work, "repos.txt", plan["file"])
    responses = [
        {"capability": "ci.latest", "input": {"repo": repo}, "results": [result(s, repo) for s in steps]}
        for repo, steps in plan["repos"].items()
    ]
    return Setup(responses=responses)


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls")
    plan = PLANS[variant]["repos"]
    calls = calls_to(outcome, "ci.latest")
    for repo in order(variant):
        mine = sorted((c for c in calls if c["input"].get("repo") == repo), key=lambda c: c["seq"])
        wanted = expected_calls(plan[repo])
        if len(mine) != wanted:
            return Verdict.wrong(f"{repo} was looked up {len(mine)} times, expected {wanted}")
        for index, (before, after) in enumerate(zip(mine, mine[1:])):
            gap = after["start_ms"] - before["end_ms"]
            if gap + TOLERANCE_MS < DELAYS_MS[index]:
                return Verdict.wrong(f"retry {index + 1} of {repo} waited {gap} ms, expected at least {DELAYS_MS[index]}")
    sequence = [c["input"].get("repo") for c in sorted(calls, key=lambda c: c["seq"])]
    firsts = list(dict.fromkeys(sequence))
    if firsts != order(variant):
        return Verdict.wrong("repositories were not looked up in file order")
    if any(c["in_flight"] > 1 for c in calls):
        return Verdict.wrong("lookups overlapped")
    rows, problem = stdout_jsonl(outcome)
    if problem:
        return Verdict.wrong(problem)
    want = [{"repo": repo, "state": expected_state(plan[repo])} for repo in order(variant)]
    if rows != want:
        return Verdict.wrong("the printed lines differ from the expected result")
    return Verdict.ok(lookups=len(calls))
