from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, rejected_inputs, stdout_json, tree_effects, write

TRANSIENT = ("Timeout", "Unavailable", "RateLimited")
ATTEMPTS = 4
WAIT_MS = 200


def shift(person: str, start: int, end: int) -> dict:
    return {"person": person, "start": start, "end": end}


def page(ident: str, at: int, severity: str = "warning") -> dict:
    return {"id": ident, "at": at, "severity": severity}


# Each team: shifts, pages, and failures per read ("shifts"/"pages" -> list of kinds before success;
# a list with no success ends in the last kind repeating).
VARIANTS = {
    "public": {
        "teams_txt": "# on-call teams\npayments\n\nplatform\n",
        "teams": {
            "payments": {
                "shifts": [shift("alice", 0, 480), shift("bob", 480, 960), shift("alice", 960, 1440)],
                "pages": [page("p-1", 10, "critical"), page("p-2", 479), page("p-3", 480, "critical"),
                          page("p-4", 1000), page("p-2", 479), page("p-5", 1440, "critical"), page("p-6", 1439)],
            },
            "platform": {
                "shifts": [shift("carol", 0, 720), shift("dan", 720, 1440)],
                "pages": [page("q-1", 700, "critical"), page("q-2", 800, "critical"), page("q-3", 900)],
                "fail": {"pages": ["RateLimited"]},
            },
        },
    },
    "flaky": {
        "teams_txt": "  search \t\napi\n# retired: legacy\nghost\nsearch\n\t\nbatch\n",
        "teams": {
            "search": {
                "shifts": [shift("erin", 100, 200), shift("frank", 200, 300)],
                "pages": [page("s-1", 150, "critical"), page("s-2", 250), page("s-3", 99)],
                "fail": {"shifts": ["Unavailable", "Timeout"]},
            },
            "api": {
                "shifts": [shift("gus", 0, 10080)],
                "pages": [page("a-1", 5)],
                "fail": {"pages": ["RateLimited", "RateLimited", "RateLimited", "RateLimited"]},
            },
            "ghost": {"shifts": [], "pages": [], "fail": {"shifts": ["NotFound"]}},
            "batch": {
                "shifts": [shift("hana", 0, 60)],
                "pages": [page("b-1", 30), page("b-1", 30), page("b-2", 59, "critical"), page("b-3", 60)],
                "fail": {"pages": ["RateLimited", "Unavailable", "RateLimited"]},
            },
        },
    },
    "quiet": {
        "teams_txt": "db\nnet\n",
        "teams": {
            "db": {
                "shifts": [shift("Zoe", 0, 100), shift("adam", 100, 200), shift("mia", 200, 300),
                           shift("Zoe", 300, 400)],
                "pages": [page("d-1", 50), page("d-2", 150), page("d-3", 250, "critical"), page("d-4", 350, "info")],
            },
            "net": {"shifts": [], "pages": [page("n-1", 1), page("n-2", 2, "critical")]},
        },
    },
    "denied": {
        "teams_txt": "web\nsecrets\nmobile\n",
        "teams": {
            "web": {"shifts": [shift("ivy", 0, 100)], "pages": [page("w-1", 1)]},
            "secrets": {"shifts": [], "pages": [], "fail": {"shifts": ["PermissionDenied"]}},
            "mobile": {"shifts": [shift("jo", 0, 100)], "pages": [], "fail": {"pages": ["NotFound"]}},
        },
    },
}


def team_order(variant: str) -> list[str]:
    seen = []
    for line in VARIANTS[variant]["teams_txt"].split("\n"):
        name = line.strip(" \t")
        if name and not name.startswith("#") and name not in seen:
            seen.append(name)
    return seen


def read_outcome(failures: list[str]) -> tuple[str, int]:
    """('ok' | 'unknown' | 'failed', attempts) for a read with these failures before success."""
    for attempt, kind in enumerate(failures[:ATTEMPTS], start=1):
        if kind == "NotFound":
            return "unknown", attempt
        if kind not in TRANSIENT:
            return "failed", attempt
    if len(failures) >= ATTEMPTS:
        return "failed", ATTEMPTS
    return "ok", len(failures) + 1


def expected(variant: str) -> dict:
    report = {"teams": [], "unknown": [], "failed": []}
    for name in team_order(variant):
        team = VARIANTS[variant]["teams"][name]
        fails = team.get("fail", {})
        states = [read_outcome(fails.get(kind, []))[0] for kind in ("shifts", "pages")]
        if "unknown" in states:
            report["unknown"].append(name)
            continue
        if "failed" in states:
            report["failed"].append(name)
            continue
        seen, pages = set(), []
        for p in team["pages"]:
            if p["id"] not in seen:
                seen.add(p["id"])
                pages.append(p)
        people = {}
        for s in team["shifts"]:
            people.setdefault(s["person"], {"person": s["person"], "pages": 0, "critical": 0})
        unassigned = 0
        for p in pages:
            owner = next((s["person"] for s in team["shifts"] if s["start"] <= p["at"] < s["end"]), None)
            if owner is None:
                unassigned += 1
                continue
            people[owner]["pages"] += 1
            if p["severity"] == "critical":
                people[owner]["critical"] += 1
        ordered = sorted(people.values(), key=lambda r: (-r["pages"], r["person"].encode()))
        report["teams"].append({"team": name, "people": ordered, "unassigned": unassigned})
    return report


def results(kinds: list[str], output: list[dict]) -> list[dict]:
    found = [{"error": {"kind": k, "message": f"{k} while reading", "performed": False}, "delay_ms": 10} for k in kinds]
    if len(kinds) < ATTEMPTS and all(k in TRANSIENT for k in kinds):
        found.append({"output": output, "delay_ms": 20})
    return found


def setup(variant: str, work: Path) -> Setup:
    spec = VARIANTS[variant]
    write(work, "teams.txt", spec["teams_txt"])
    responses = []
    for name, team in spec["teams"].items():
        fails = team.get("fail", {})
        responses.append({"capability": "oncall.shifts", "input": {"team": name},
                          "results": results(fails.get("shifts", []), team["shifts"])})
        responses.append({"capability": "oncall.pages", "input": {"team": name},
                          "results": results(fails.get("pages", []), team["pages"])})
    return Setup(responses=responses)


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    wanted = expected(variant)
    status = 4 if wanted["failed"] else 0
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls (wrong team names?)")
    effects = tree_effects(outcome.before, outcome.after)
    if any(effects.values()):
        return Verdict.wrong(f"the program changed /work: {effects}")
    for name in team_order(variant):
        fails = VARIANTS[variant]["teams"][name].get("fail", {})
        for kind, capability in (("shifts", "oncall.shifts"), ("pages", "oncall.pages")):
            calls = sorted((c for c in calls_to(outcome, capability) if c["input"]["team"] == name),
                           key=lambda c: c["seq"])
            state, attempts = read_outcome(fails.get(kind, []))
            # A program may skip the second read once the first decided the team, but never over-retry.
            if len(calls) > attempts:
                return Verdict.wrong(f"{capability} for {name} was called {len(calls)} times, at most {attempts}")
            if state == "ok" and len(calls) not in (0, attempts):
                return Verdict.wrong(f"{capability} for {name} was called {len(calls)} times, expected {attempts}")
            if state != "ok" and len(calls) not in (0, attempts):
                return Verdict.wrong(f"{capability} for {name} gave up after {len(calls)} attempts")
            for previous, again in zip(calls, calls[1:]):
                if again["start_ms"] - previous["end_ms"] < WAIT_MS - 10:
                    return Verdict.wrong(f"{capability} for {name} was retried without waiting 200 ms")
    report, problem = stdout_json(outcome)
    if problem:
        return Verdict.wrong(problem)
    if report != wanted:
        return Verdict.wrong("the printed report differs from the expected result")
    return Verdict.ok(calls=len(outcome.calls))
