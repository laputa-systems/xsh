import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, duplicate_writes, rejected_inputs, stdout_json, write

# Each host: (delay_ms, outcome). An int outcome is the revision; a str is an error kind.
PLANS = {
    "public": {
        "release": "2026.11.3",
        "waves": [["web-1", "web-2", "web-3"], ["api-1", "api-2"], ["db-1"]],
        "hosts": {
            "web-1": (150, 7101), "web-2": (250, 7102), "web-3": (100, 7103),
            "api-1": (200, 7104), "api-2": (120, 7105), "db-1": (80, 7106),
        },
    },
    "failing": {
        "release": "r-884",
        "waves": [["a1", "a2"], ["b1", "b2", "b3", "b4"], ["c1", "c2"], ["d1"]],
        "hosts": {
            "a1": (120, 11), "a2": (90, 12),
            "b1": (150, 13), "b2": (60, "Failed"), "b3": (200, 14), "b4": (100, "PermissionDenied"),
            "c1": (50, 15), "c2": (50, 16), "d1": (50, 17),
        },
    },
    "slow": {
        "release": "hotfix-2",
        "waves": [["s1", "s2", "s3"], ["t1", "t2"]],
        "hosts": {
            "s1": (200, 501), "s2": (1600, 502), "s3": (300, 503),
            "t1": (50, 504), "t2": (50, 505),
        },
    },
    "repeats": {
        "release": "2026.12.0-rc1",
        "waves": [["m1", "m2"], [], ["m2", "m3", "m1", "m4"], ["m3"], ["m5"]],
        "hosts": {
            "m1": (150, 901), "m2": (100, 902), "m3": (180, 903), "m4": (120, 904), "m5": (60, 905),
        },
    },
}


def effective_waves(variant: str) -> list[list[str]]:
    seen: set[str] = set()
    waves = []
    for wave in PLANS[variant]["waves"]:
        kept = []
        for host in wave:
            if host not in seen:
                seen.add(host)
                kept.append(host)
        waves.append(kept)
    return waves


TIMEOUT_MS = 1000


def host_result(variant: str, host: str) -> tuple[bool, object]:
    delay, outcome = PLANS[variant]["hosts"][host]
    if delay > TIMEOUT_MS:
        return False, "Timeout"
    if isinstance(outcome, int):
        return True, outcome
    return False, outcome


def expected(variant: str) -> tuple[dict, int, list[list[str]]]:
    deployed, failed, skipped, started = [], [], [], []
    stopped = False
    for wave in effective_waves(variant):
        if stopped:
            skipped += wave
            continue
        started.append(wave)
        for host in wave:
            ok, value = host_result(variant, host)
            if ok:
                deployed.append({"host": host, "revision": value})
            else:
                failed.append({"host": host, "kind": value})
                stopped = True
    return {"deployed": deployed, "failed": failed, "skipped": skipped}, (4 if failed else 0), started


def setup(variant: str, work: Path) -> Setup:
    plan = PLANS[variant]
    write(work, "rollout.json", json.dumps({"release": plan["release"], "waves": plan["waves"]}, indent=2) + "\n")
    responses = []
    for host, (delay, outcome) in plan["hosts"].items():
        if isinstance(outcome, int):
            result = {"output": {"host": host, "revision": outcome}, "delay_ms": delay}
        else:
            result = {"error": {"kind": outcome, "message": f"{host}: deploy refused", "performed": False},
                      "delay_ms": delay}
        responses.append({"capability": "deploy.start", "input": {"host": host, "release": plan["release"]},
                          "results": [result]})
    return Setup(responses=responses)


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    wanted, status, started = expected(variant)
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls")
    if duplicate_writes(outcome):
        return Verdict.wrong("a deploy was started twice")
    calls = calls_to(outcome, "deploy.start")
    by_host: dict[str, list[dict]] = {}
    for call in calls:
        by_host.setdefault(call["input"]["host"], []).append(call)
    should_start = [h for wave in started for h in wave]
    for host in should_start:
        if len(by_host.get(host, [])) != 1:
            return Verdict.wrong(f"{host} was started {len(by_host.get(host, []))} times, expected once")
    extra = sorted(set(by_host) - set(should_start))
    if extra:
        return Verdict.wrong(f"hosts of waves that must not start were started: {extra}")
    for wave in started:
        records = [by_host[h][0] for h in wave]
        if any(r["in_flight"] > 2 for r in records):
            return Verdict.wrong("more than two deploys were in flight")
        if len(wave) >= 2 and max(r["in_flight"] for r in records) < 2:
            return Verdict.wrong("a wave with several hosts never had two deploys in flight")
    nonempty = [w for w in started if w]
    for previous, current in zip(nonempty, nonempty[1:]):
        finished = max(by_host[h][0]["end_ms"] for h in previous)
        begun = min(by_host[h][0]["start_ms"] for h in current)
        if begun < finished:
            return Verdict.wrong("a wave started before the previous wave had answered")
    slow = [c for c in calls if c.get("outcome") == "abandoned"]
    if variant == "slow" and not slow:
        return Verdict.wrong("the slow deploy was not abandoned at the 1 second timeout")
    result, problem = stdout_json(outcome)
    if problem:
        return Verdict.wrong(problem)
    if result != wanted:
        return Verdict.wrong("the printed object differs from the expected result")
    return Verdict.ok(calls=len(calls))
