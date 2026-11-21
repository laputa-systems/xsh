import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, peak_in_flight, rejected_inputs, stdout_json, write

QUICK_MS = 250
SLOW_MS = 4000

PLANS = {
    "public": [
        ("web-1", "ok", "2.4.1", QUICK_MS),
        ("web-2", "ok", "2.4.1", SLOW_MS),
        ("web-3", "degraded", "2.4.0", QUICK_MS),
        ("db-1", "Unavailable", "", QUICK_MS),
        ("db-2", "ok", "9.1", QUICK_MS),
        ("cache-1", "ok", "7.0.15", QUICK_MS),
        ("cache-2", "ok", "7.0.15", QUICK_MS),
    ],
    "wide": [
        ("edge-a", "ok", "1.0", QUICK_MS),
        ("edge-b", "draining", "1.0", QUICK_MS),
        ("edge-c", "ok", "1.1", SLOW_MS),
        ("edge-d", "NotFound", "", QUICK_MS),
        ("edge-e", "ok", "1.1", QUICK_MS),
        ("edge-f", "ok", "1.2", SLOW_MS),
        ("edge-g", "Timeout", "", QUICK_MS),
        ("edge-h", "ok", "1.2", QUICK_MS),
        ("edge-i", "ok", "1.2", QUICK_MS),
        ("edge-j", "Ok", "1.2", QUICK_MS),
        ("edge-k", "ok", "1.3", QUICK_MS),
        ("edge-l", "Failed", "", SLOW_MS),
    ],
    "healthy": [
        ("api-1", "ok", "5.0", QUICK_MS),
        ("api-2", "ok", "5.0", QUICK_MS),
        ("api-3", "ok", "5.1", QUICK_MS),
        ("api-4", "ok", "5.1", QUICK_MS),
    ],
}


def setup(variant: str, work: Path) -> Setup:
    plan = PLANS[variant]
    write(work, "hosts.json", json.dumps([h for h, *_ in plan]))
    responses = []
    for host, status, version, delay in plan:
        if status[0].isupper() and status != "Ok":
            result = {"error": {"kind": status, "message": f"{host}: {status}"}, "delay_ms": delay}
        else:
            result = {"output": {"host": host, "status": status, "version": version}, "delay_ms": delay}
        responses.append({"capability": "health.probe", "input": {"host": host}, "results": [result]})
    return Setup(responses=responses)


def expected(variant: str) -> dict:
    up, down = [], []
    for host, status, version, delay in PLANS[variant]:
        if status == "ok" and delay < 1000:
            up.append({"host": host, "version": version})
        else:
            down.append(host)
    return {"up": up, "down": down}


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    calls = calls_to(outcome, "health.probe")
    peak = peak_in_flight(calls)
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls", peak=peak)
    hosts = [h for h, *_ in PLANS[variant]]
    counts = {h: sum(1 for c in calls if c["input"].get("host") == h) for h in hosts}
    if any(n != 1 for n in counts.values()):
        return Verdict.wrong(f"hosts were not probed exactly once: {counts}", peak=peak)
    if peak > 3:
        return Verdict.wrong(f"{peak} probes ran at once, more than 3", peak=peak)
    if peak < 2:
        return Verdict.wrong("the probes ran one at a time", peak=peak)
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0", peak=peak)
    result, problem = stdout_json(outcome)
    if problem:
        return Verdict.wrong(problem, peak=peak)
    if result != expected(variant):
        return Verdict.wrong("the printed object differs from the expected result", peak=peak)
    return Verdict.ok(peak=peak)
