import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, duplicate_writes, rejected_inputs, write

BASE = [("region", "us-east-1"), ("pool_size", "8"), ("log_level", "info"), ("legacy_mode", "on"), ("timeout", "30s")]

PLANS = {
    "public": {
        "service": "billing",
        "values": {"pool_size": "16", "tracing": "off", "cache": "redis"},
        "drop": ["legacy_mode"],
        "docs": [(7, BASE)],
        "puts": ["ok"],
    },
    "conflict": {
        "service": "search",
        "values": {"log_level": "warn", "shards": "4"},
        "drop": ["timeout", "absent_key"],
        "docs": [(12, BASE), (13, BASE[:2] + [("log_level", "debug"), ("owner", "team-x")] + BASE[3:])],
        "puts": ["Conflict", "ok"],
    },
    "noop": {
        "service": "auth",
        "values": {"region": "us-east-1", "timeout": "30s"},
        "drop": ["missing"],
        "docs": [(3, BASE)],
        "puts": [],
    },
    "stubborn": {
        "service": "ledger",
        "values": {"region": "eu-west-1"},
        "drop": [],
        "docs": [(40, BASE), (41, BASE[1:]), (42, BASE[2:])],
        "puts": ["Conflict", "Conflict"],
    },
}


def compute(entries: list[tuple[str, str]], values: dict[str, str], drop: list[str]) -> list[tuple[str, str]]:
    present = {k for k, _ in entries}
    kept = [(k, values.get(k, v)) for k, v in entries if k not in drop]
    return kept + [(k, values[k]) for k in sorted(values) if k not in present]


def entries_json(entries: list[tuple[str, str]]) -> list[dict]:
    return [{"key": k, "value": v} for k, v in entries]


def doc(service: str, version: int, entries: list[tuple[str, str]]) -> dict:
    return {"service": service, "version": version, "entries": entries_json(entries)}


def setup(variant: str, work: Path) -> Setup:
    plan = PLANS[variant]
    service = plan["service"]
    write(work, "desired.json", json.dumps({"service": service, "values": plan["values"], "drop": plan["drop"]}, indent=2))
    responses = [{
        "capability": "config.get", "input": {"service": service},
        "results": [{"output": doc(service, v, e)} for v, e in plan["docs"]],
    }]
    for index, result in enumerate(plan["puts"]):
        version, entries = plan["docs"][index]
        new = compute(entries, plan["values"], plan["drop"])
        answer = (
            {"output": doc(service, version + 5, new)} if result == "ok"
            else {"error": {"kind": "Conflict", "message": f"{service} is at a newer version than {version}"}}
        )
        responses.append({
            "capability": "config.put", "input": {"service": service, "version": version, "entries": entries_json(new)},
            "results": [answer],
        })
    return Setup(responses=responses)


def expected_line(variant: str) -> tuple[int, str | None]:
    plan = PLANS[variant]
    if not plan["puts"]:
        return 0, f"unchanged {plan['docs'][0][0]}"
    if plan["puts"][-1] == "ok":
        return 0, f"updated {plan['docs'][len(plan['puts']) - 1][0] + 5}"
    return 4, None


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    plan = PLANS[variant]
    puts = calls_to(outcome, "config.put")
    if duplicate_writes(outcome):
        return Verdict.wrong("a store was repeated", puts=len(puts))
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls", puts=len(puts))
    if len(puts) != len(plan["puts"]):
        return Verdict.wrong(f"config_put ran {len(puts)} times, expected {len(plan['puts'])}", puts=len(puts))
    status, line = expected_line(variant)
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}", puts=len(puts))
    if line is not None and outcome.stdout.strip() != line:
        return Verdict.wrong("the printed line differs from the expected line", puts=len(puts))
    if line is None and not outcome.stderr.strip():
        return Verdict.wrong("nothing was printed to standard error", puts=len(puts))
    return Verdict.ok(puts=len(puts))
