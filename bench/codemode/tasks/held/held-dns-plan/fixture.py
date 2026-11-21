import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, duplicate_writes, rejected_inputs, stdout_jsonl, tree_effects, write

OWNER = "dnsplan"


def rec(name: str, kind: str, value: str, ttl: int, owner: str = OWNER) -> dict:
    return {"name": name, "kind": kind, "value": value, "ttl": ttl, "owner": owner}


def want(name: str, kind: str, value: str, ttl: int) -> dict:
    return {"name": name, "kind": kind, "value": value, "ttl": ttl}


PUBLIC_CURRENT = [
    rec("www", "A", "192.0.2.10", 300),
    rec("www", "AAAA", "2001:db8::10", 300),
    rec("api", "A", "192.0.2.20", 300),
    rec("@", "MX", "10 mail.example.test.", 3600),
    rec("old", "CNAME", "www.example.test.", 300),
    rec("_dmarc", "TXT", "v=DMARC1; p=none", 3600),
    rec("@", "NS", "ns1.provider.test.", 86400, owner="provider"),
    rec("legacy", "A", "198.51.100.7", 600, owner="ops-manual"),
    rec("status", "CNAME", "status.vendor.test.", 300),
]
PUBLIC_DESIRED = [
    want("www", "A", "192.0.2.10", 300),
    want("www", "AAAA", "2001:db8::10", 300),
    want("api", "A", "192.0.2.21", 300),
    want("@", "MX", "10 mail.example.test.", 1800),
    want("_dmarc", "TXT", "v=DMARC1; p=quarantine", 3600),
    want("cdn", "CNAME", "edge.cdn.test.", 300),
    want("Api", "TXT", "case matters", 60),
    want("status", "CNAME", "status.vendor.test.", 300),
]

CONFLICT_CURRENT = [
    rec("app", "A", "203.0.113.5", 120),
    rec("app", "TXT", "owner=team-a", 120, owner="team-a"),
    rec("mail", "A", "203.0.113.25", 300),
    rec("mail", "MX", "5 mail.corp.test.", 300),
    rec("gone", "A", "203.0.113.99", 300),
    rec("gone", "AAAA", "2001:db8::99", 300, owner="someone"),
]
CONFLICT_DESIRED = [
    want("mail", "MX", "5 mail.corp.test.", 300),
    want("app", "TXT", "owner=platform", 120),
    want("app", "A", "203.0.113.6", 120),
    want("new", "A", "203.0.113.50", 300),
]

FAILURE_CURRENT = [
    rec("a", "A", "10.0.0.1", 60),
    rec("b", "A", "10.0.0.2", 60),
    rec("c", "A", "10.0.0.3", 60),
    rec("d", "A", "10.0.0.4", 60),
    rec("e", "A", "10.0.0.5", 60),
]
FAILURE_DESIRED = [
    want("a", "A", "10.0.1.1", 60),
    want("b", "A", "10.0.0.2", 120),
    want("f", "A", "10.0.0.6", 60),
    want("g", "A", "10.0.0.7", 60),
]

VARIANTS = {
    "public": {"argv": ["--apply", "example.test", "/work/zones/example.test.json"], "zone": "example.test",
               "file": "zones/example.test.json", "current": PUBLIC_CURRENT, "desired": PUBLIC_DESIRED,
               "apply": True, "fail": {}},
    "dry-run": {"argv": ["corp.test", "/work/desired zone.json"], "zone": "corp.test", "file": "desired zone.json",
                "current": CONFLICT_CURRENT, "desired": CONFLICT_DESIRED, "apply": False, "fail": {}},
    "failures": {"argv": ["--apply", "lab.test", "lab.json"], "zone": "lab.test", "file": "lab.json",
                 "current": FAILURE_CURRENT, "desired": FAILURE_DESIRED, "apply": True,
                 "fail": {("b", "A"): "Failed", ("d", "A"): "NotFound", ("g", "A"): "PermissionDenied"}},
    "usage": {"argv": ["--force", "example.test", "/work/zones/example.test.json"], "zone": "example.test",
              "file": "zones/example.test.json", "current": PUBLIC_CURRENT, "desired": PUBLIC_DESIRED,
              "apply": True, "fail": {}, "status": 2},
    "duplicate": {"argv": ["--apply", "example.test", "/work/zones/example.test.json"], "zone": "example.test",
                  "file": "zones/example.test.json", "current": PUBLIC_CURRENT,
                  "desired": PUBLIC_DESIRED + [want("www", "A", "192.0.2.11", 300)], "apply": True, "fail": {},
                  "status": 2},
}


def byte_key(name: str, kind: str) -> tuple[bytes, bytes]:
    return name.encode(), kind.encode()


def plan(variant: str) -> list[dict]:
    spec = VARIANTS[variant]
    current = {(r["name"], r["kind"]): r for r in spec["current"]}
    desired = {(r["name"], r["kind"]): r for r in spec["desired"]}
    actions = []
    for key, wanted in desired.items():
        present = current.get(key)
        if present is None:
            actions.append(("create", key))
        elif (present["value"], present["ttl"]) != (wanted["value"], wanted["ttl"]):
            actions.append(("update" if present["owner"] == OWNER else "conflict", key))
    for key, present in current.items():
        if present["owner"] == OWNER and key not in desired:
            actions.append(("delete", key))
    lines = []
    for action, key in sorted(actions, key=lambda a: byte_key(*a[1])):
        if action == "conflict":
            result = "skipped"
        elif not spec["apply"]:
            result = "planned"
        else:
            result = "failed" if key in spec["fail"] else "applied"
        lines.append({"action": action, "name": key[0], "kind": key[1], "result": result})
    return lines


def setup(variant: str, work: Path) -> Setup:
    spec = VARIANTS[variant]
    write(work, spec["file"], json.dumps(spec["desired"], indent=2) + "\n")
    zone = spec["zone"]
    responses = [{"capability": "dns.records", "input": {"zone": zone}, "results": [{"output": spec["current"]}]}]
    desired = {(r["name"], r["kind"]): r for r in spec["desired"]}
    current = {(r["name"], r["kind"]): r for r in spec["current"]}
    for key, wanted in desired.items():
        failure = spec["fail"].get(key)
        result = ({"error": {"kind": failure, "message": f"{key[0]} {key[1]}: refused", "performed": False}}
                  if failure else {"output": {"name": key[0], "kind": key[1]}})
        responses.append({"capability": "dns.put", "input": {"zone": zone, **wanted}, "results": [result]})
    for key in current:
        failure = spec["fail"].get(key)
        result = ({"error": {"kind": failure, "message": f"{key[0]} {key[1]}: refused", "performed": False}}
                  if failure else {"output": {"name": key[0], "kind": key[1]}})
        responses.append({"capability": "dns.delete", "input": {"zone": zone, "name": key[0], "kind": key[1]},
                          "results": [result]})
    return Setup(argv=list(spec["argv"]), responses=responses)


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    spec = VARIANTS[variant]
    effects = tree_effects(outcome.before, outcome.after)
    if effects["created"] or effects["deleted"] or effects["modified"]:
        return Verdict.wrong(f"the command changed files under /work: {effects}")
    if "status" in spec:
        if outcome.exit != spec["status"]:
            return Verdict.wrong(f"exit status {outcome.exit}, expected {spec['status']}")
        if outcome.calls:
            return Verdict.wrong("the service was called despite a usage or input error")
        if outcome.stdout.strip():
            return Verdict.wrong("an action was printed despite a usage or input error")
        return Verdict.ok()
    lines = plan(variant)
    status = 0 if all(line["result"] in ("planned", "applied") for line in lines) else 4
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls")
    if duplicate_writes(outcome):
        return Verdict.wrong("a record was written twice")
    if not calls_to(outcome, "dns.records"):
        return Verdict.wrong("the zone was never read")
    wanted_puts = sorted((l["name"], l["kind"]) for l in lines if spec["apply"] and l["action"] in ("create", "update"))
    wanted_deletes = sorted((l["name"], l["kind"]) for l in lines if spec["apply"] and l["action"] == "delete")
    puts = sorted((c["input"]["name"], c["input"]["kind"]) for c in calls_to(outcome, "dns.put"))
    deletes = sorted((c["input"]["name"], c["input"]["kind"]) for c in calls_to(outcome, "dns.delete"))
    if puts != wanted_puts:
        return Verdict.wrong(f"dns.put was called for {puts}, expected {wanted_puts}")
    if deletes != wanted_deletes:
        return Verdict.wrong(f"dns.delete was called for {deletes}, expected {wanted_deletes}")
    rows, problem = stdout_jsonl(outcome)
    if problem:
        return Verdict.wrong(problem)
    if rows != lines:
        return Verdict.wrong("the printed actions differ from the expected plan")
    return Verdict.ok(actions=len(lines))
