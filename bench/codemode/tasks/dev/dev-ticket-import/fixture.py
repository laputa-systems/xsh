import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, duplicate_writes, rejected_inputs, stdout_json, write

FILES = {
    "public": [
        '{"id": "T-1", "title": "Login fails on Safari", "priority": 1}',
        '{"id": "T-2", "title": "Typo on pricing page", "priority": 4, "labels": ["web"]}',
        "",
        '{"id": "T-3", "title": "Export times out", "priority": "2"}',
        '{"id": "T-2", "title": "Typo on pricing page (again)", "priority": 3}',
        '{"id": "T-4", "title": "Dark mode contrast", "priority": 3}',
        "   ",
        '{"id": "T-5", "priority": 2}',
    ],
    "messy": [
        '{"id": "B-10", "title": "Crash on start", "priority": 1',
        '["B-11", "Not an object", 2]',
        '{"id": "B-12", "title": "Slow search", "priority": 0}',
        '{"id": "B-13", "title": "Audit log gap", "priority": 2}',
        '{"id": "", "title": "Empty id", "priority": 2}',
        '{"id": "B-14", "title": null, "priority": 2}',
        '{"id": "B-15", "title": "Refund rounding", "priority": 5}',
        '{"id": "B-16", "title": "Webhook retries", "priority": 2, "owner": {"name": "kim"}}',
        '{"id": "B-13", "title": "Audit log gap", "priority": 2}',
        '{"id": "B-17", "title": "Quota display", "priority": 4}',
        "",
        '{"id": "B-18", "title": "Locale fallback", "priority": 3}',
        "null",
    ],
    "clean": [
        '{"id": "C-1", "title": "One", "priority": 1}',
        '{"id": "C-2", "title": "Two", "priority": 2}',
        '{"id": "C-3", "title": "Three", "priority": 3}',
    ],
}
FAILS = {"messy": {"B-16": "Conflict"}, "public": {}, "clean": {}}


def classify(variant: str) -> tuple[list[dict], list[dict]]:
    accepted, rejects, seen = [], [], set()
    for number, line in enumerate(FILES[variant], start=1):
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            value = None
        valid = (
            isinstance(value, dict) and isinstance(value.get("id"), str) and value["id"] != ""
            and isinstance(value.get("title"), str) and isinstance(value.get("priority"), int)
            and not isinstance(value.get("priority"), bool) and 1 <= value["priority"] <= 4
        )
        if not valid:
            rejects.append({"line": number, "reason": "invalid"})
        elif value["id"] in seen:
            rejects.append({"line": number, "reason": "duplicate"})
        else:
            seen.add(value["id"])
            if value["id"] in FAILS[variant]:
                rejects.append({"line": number, "reason": "failed"})
            accepted.append({"id": value["id"], "title": value["title"], "priority": value["priority"]})
    return accepted, sorted(rejects, key=lambda r: r["line"])


def setup(variant: str, work: Path) -> Setup:
    write(work, "tickets.jsonl", "\n".join(FILES[variant]) + "\n")
    accepted, _ = classify(variant)
    responses = []
    for ticket in accepted:
        kind = FAILS[variant].get(ticket["id"])
        result = (
            {"error": {"kind": kind, "message": f"{ticket['id']} conflicts with an existing ticket"}}
            if kind else {"output": {"id": ticket["id"], "url": f"https://tracker.test/{ticket['id']}"}}
        )
        responses.append({"capability": "tracker.create", "input": ticket, "results": [result]})
    return Setup(responses=responses)


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    creates = sorted(calls_to(outcome, "tracker.create"), key=lambda c: c["seq"])
    if duplicate_writes(outcome):
        return Verdict.wrong("a ticket was created twice", creates=len(creates))
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls", creates=len(creates))
    accepted, rejects = classify(variant)
    if [c["input"] for c in creates] != accepted:
        return Verdict.wrong("the created tickets differ from the accepted lines in file order", creates=len(creates))
    if any(c["in_flight"] > 1 for c in creates):
        return Verdict.wrong("creations overlapped")
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    path = work / "rejects.jsonl"
    if not path.is_file():
        return Verdict.wrong("rejects.jsonl was not written")
    try:
        rows = [json.loads(line) for line in path.read_text().splitlines()]
    except json.JSONDecodeError:
        return Verdict.wrong("rejects.jsonl has a line that is not JSON")
    if rows != rejects:
        return Verdict.wrong("rejects.jsonl differs from the expected rejections")
    result, problem = stdout_json(outcome)
    if problem:
        return Verdict.wrong(problem)
    created = len(accepted) - sum(1 for r in rejects if r["reason"] == "failed")
    if result != {"created": created, "rejected": len(rejects)}:
        return Verdict.wrong("the printed counts differ from the expected counts")
    return Verdict.ok(creates=len(creates))
