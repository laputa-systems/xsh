from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, rejected_inputs, stdout_json

LOGS = {
    "public": [
        [("e41", "info"), ("e17", "error")],
        [],
        [("e17", "error"), ("e93", "error"), ("e08", "warn"), ("e25", "error")],
    ],
    "shifting": [
        [("k-90", "error"), ("k-12", "info")],
        [("k-12", "info"), ("k-33", "Error"), ("k-05", "error")],
        [("k-05", "error"), ("k-71", "error ")],
        [("k-90", "error"), ("k-48", "warn")],
        [],
        [("k-64", "error"), ("k-19", "error"), ("k-48", "warn")],
        [("k-19", "error"), ("k-02", "error")],
    ],
    "single": [[("s1", "info"), ("s2", "warn")]],
}


def cursor(variant: str, index: int) -> str:
    return f"{variant[:2]}-{index * 7919 % 1000:03d}-{index}"


def pages(variant: str) -> list[dict]:
    log = LOGS[variant]
    built = []
    for index, events in enumerate(log):
        following = cursor(variant, index + 1) if index + 1 < len(log) else None
        built.append({
            "input": {"cursor": None if index == 0 else cursor(variant, index)},
            "page": {"events": [{"id": i, "kind": k} for i, k in events], "next": following},
        })
    return built


def expected(variant: str) -> dict:
    seen: list[str] = []
    for events in LOGS[variant]:
        for event_id, kind in events:
            if kind == "error" and event_id not in seen:
                seen.append(event_id)
    return {"errors": seen, "pages": len(LOGS[variant])}


def setup(variant: str, work: Path) -> Setup:
    responses = [
        {"capability": "audit.page", "input": p["input"], "results": [{"output": p["page"]}]} for p in pages(variant)
    ]
    return Setup(responses=responses)


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls")
    wanted_cursors = {str(p["input"]["cursor"]) for p in pages(variant)}
    asked = {str(c["input"]["cursor"]) for c in calls_to(outcome, "audit.page")}
    if wanted_cursors - asked:
        return Verdict.wrong(f"{len(wanted_cursors - asked)} pages were never read")
    result, problem = stdout_json(outcome)
    if problem:
        return Verdict.wrong(problem)
    if result != expected(variant):
        return Verdict.wrong("the printed object differs from the expected result")
    return Verdict.ok(pages_read=len(calls_to(outcome, "audit.page")))
