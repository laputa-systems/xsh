import json
from pathlib import Path

from fixtures import (
    Outcome, Setup, Verdict, calls_to, duplicate_writes, rejected_inputs, tree_effects, write,
)

TRANSIENT = ("Timeout", "Unavailable", "RateLimited")
REPORT = "payout-report.json"
SEND_TIMEOUT_MS = 500
WAIT_MS = 200

# A send is ("ok", TRANSFER), ("slow", TRANSFER) for an answer that arrives after the
# send timeout, or (KIND, PERFORMED). A status lookup is a TRANSFER, None, or ("error", KIND).
PAYOUTS = {
    "public": [
        {"ref": "P-1001", "account": "DE44 5001 0517", "cents": 125000, "sends": [("ok", "tx-8a1")], "status": []},
        {"ref": "P-1002", "account": "FR76 3000 6000", "cents": 9900,
         "sends": [("Unavailable", False), ("ok", "tx-8a2")], "status": [None]},
        {"ref": "P-1003", "account": "NL91 ABNA 0417", "cents": 4550,
         "sends": [("Timeout", True)], "status": ["tx-8a3"]},
        {"ref": "P-1004", "account": "IT60 X054 2811", "cents": 30000, "sends": [("ok", "tx-8a4")], "status": []},
    ],
    "exhausted": [
        {"ref": "Q-1", "account": "acct-q1", "cents": 700,
         "sends": [("RateLimited", False), ("RateLimited", False), ("Unavailable", False)],
         "status": [None, None, None]},
        {"ref": "Q-2", "account": "acct-q2", "cents": 1800, "sends": [("ok", "tq-2")], "status": []},
        {"ref": "Q-3", "account": "acct-q3", "cents": 2500,
         "sends": [("Timeout", False), ("slow", "tq-3")], "status": [None, "tq-3"]},
        {"ref": "Q-4", "account": "acct-q4", "cents": 99,
         "sends": [("Unavailable", False), ("Unavailable", False), ("ok", "tq-4")], "status": [None, None]},
    ],
    "ambiguous": [
        {"ref": "R-1", "account": "acct r1", "cents": 5000, "sends": [("PermissionDenied", False)], "status": []},
        {"ref": "R-2", "account": "acct r2", "cents": 6100,
         "sends": [("Unavailable", True)], "status": [("error", "Unavailable")]},
        {"ref": "R-3", "account": "acct r3", "cents": 4200, "sends": [("Conflict", False)], "status": []},
        {"ref": "R-4", "account": "acct r4", "cents": 120, "sends": [("ok", "tr-4")], "status": []},
        {"ref": "R-5", "account": "acct r5", "cents": 330,
         "sends": [("RateLimited", False)], "status": ["tr-5"]},
    ],
    "empty": [],
}


def classify(payout: dict) -> tuple[str, object, int, int]:
    """Returns (bucket, value, sends, lookups) under the contract."""
    lookups = 0
    for index, send in enumerate(payout["sends"]):
        kind = send[0]
        if kind == "ok":
            return "sent", send[1], index + 1, lookups
        if kind == "slow":
            kind = "Timeout"
        if kind not in TRANSIENT:
            return "failed", kind, index + 1, lookups
        state = payout["status"][min(lookups, len(payout["status"]) - 1)]
        lookups += 1
        if isinstance(state, tuple):
            return "unknown", None, index + 1, lookups
        if state is not None:
            return "sent", state, index + 1, lookups
        if index + 1 == 3:
            return "failed", kind, 3, lookups
    raise AssertionError(f"script for {payout['ref']} ends early")


def expected(variant: str) -> dict:
    report = {"sent": [], "failed": [], "unknown": []}
    for payout in PAYOUTS[variant]:
        bucket, value, _, _ = classify(payout)
        if bucket == "sent":
            report["sent"].append({"ref": payout["ref"], "transfer": value})
        elif bucket == "failed":
            report["failed"].append({"ref": payout["ref"], "kind": value})
        else:
            report["unknown"].append(payout["ref"])
    return report


def send_result(payout: dict, send: tuple) -> dict:
    kind = send[0]
    if kind == "ok":
        return {"output": {"ref": payout["ref"], "transfer": send[1]}, "delay_ms": 40}
    if kind == "slow":
        return {"output": {"ref": payout["ref"], "transfer": send[1]}, "delay_ms": 900}
    return {"error": {"kind": kind, "message": f"payout {payout['ref']}: {kind.lower()}", "performed": send[1]},
            "delay_ms": 30}


def status_result(payout: dict, state: object) -> dict:
    if isinstance(state, tuple):
        return {"error": {"kind": state[1], "message": "status store unavailable", "performed": False}}
    return {"output": {"ref": payout["ref"], "transfer": state}}


def setup(variant: str, work: Path) -> Setup:
    payouts = PAYOUTS[variant]
    listed = [{"ref": p["ref"], "account": p["account"], "cents": p["cents"]} for p in payouts]
    write(work, "payouts.json", json.dumps(listed, indent=2) + "\n")
    responses = []
    for payout in payouts:
        key = {"ref": payout["ref"], "account": payout["account"], "cents": payout["cents"]}
        responses.append({"capability": "payouts.send", "input": key,
                          "results": [send_result(payout, s) for s in payout["sends"]]})
        states = payout["status"] or [None]
        responses.append({"capability": "payouts.status", "input": {"ref": payout["ref"]},
                          "results": [status_result(payout, s) for s in states]})
    return Setup(responses=responses)


def overlapping(calls: list[dict]) -> bool:
    # The service notices an abandoned call when the client closes it, which can be logged a
    # few milliseconds after the client's next call arrives; allow that much for those calls.
    busy_until = None
    for call in sorted(calls, key=lambda c: c["start_ms"]):
        if busy_until is not None and call["start_ms"] < busy_until:
            return True
        finished = call["end_ms"] - (30 if call.get("outcome") == "abandoned" else 0)
        busy_until = finished if busy_until is None else max(busy_until, finished)
    return False


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    wanted = expected(variant)
    status = 0 if not wanted["failed"] and not wanted["unknown"] else 4
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls")
    if duplicate_writes(outcome):
        return Verdict.wrong("a payout was sent again after it had been paid")
    if overlapping(outcome.calls):
        return Verdict.wrong("payouts were not processed one at a time")
    order = [p["ref"] for p in PAYOUTS[variant]]
    seen_refs = [c["input"]["ref"] for c in sorted(outcome.calls, key=lambda c: c["seq"])]
    collapsed = [r for i, r in enumerate(seen_refs) if i == 0 or seen_refs[i - 1] != r]
    if collapsed != [r for r in order if r in collapsed] or len(collapsed) != len(set(collapsed)):
        return Verdict.wrong("payouts were not processed in file order, one after another")
    for payout in PAYOUTS[variant]:
        _, _, sends_wanted, lookups_wanted = classify(payout)
        mine = sorted((c for c in outcome.calls if c["input"]["ref"] == payout["ref"]), key=lambda c: c["seq"])
        sends = [c for c in mine if c["capability"] == "payouts.send"]
        lookups = [c for c in mine if c["capability"] == "payouts.status"]
        if len(sends) != sends_wanted:
            return Verdict.wrong(f"{payout['ref']} was sent {len(sends)} times, expected {sends_wanted}")
        if len(lookups) < lookups_wanted:
            return Verdict.wrong(f"{payout['ref']}: a transient send failure was not followed by a status lookup")
        for previous, resend in zip(sends, sends[1:]):
            between = [c for c in lookups if previous["seq"] < c["seq"] < resend["seq"]]
            if not between:
                return Verdict.wrong(f"{payout['ref']} was sent again without a status lookup")
            if resend["start_ms"] - between[-1]["end_ms"] < WAIT_MS - 10:
                return Verdict.wrong(f"{payout['ref']} was sent again without waiting 200 ms")
        slow = [c for c in sends if c.get("outcome") == "abandoned"]
        if any(s[0] == "slow" for s in payout["sends"]) and not slow:
            return Verdict.wrong(f"{payout['ref']}: the slow send was not abandoned at the 500 ms timeout")
    effects = tree_effects(outcome.before, outcome.after)
    if effects["created"] != [REPORT] or effects["deleted"] or effects["modified"]:
        return Verdict.wrong(f"unexpected file changes: {effects}")
    try:
        report = json.loads((work / REPORT).read_text())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        return Verdict.wrong(f"the report is not JSON: {error}")
    if report != wanted:
        return Verdict.wrong("the report differs from the expected result")
    line = f"sent={len(wanted['sent'])} failed={len(wanted['failed'])} unknown={len(wanted['unknown'])}"
    if outcome.stdout.strip() != line:
        return Verdict.wrong("the summary line differs from the expected counts")
    return Verdict.ok(calls=len(outcome.calls))
