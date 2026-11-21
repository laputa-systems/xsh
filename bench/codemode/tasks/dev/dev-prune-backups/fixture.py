import json
import re
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, stdout_json, symlink, write

TREES = {
    "standard": {
        "api": {
            "files": ["api-2026-01-03.tar.gz", "api-2025-12-30.tar.gz", "api-2026-01-01.tar.gz", "api-2026-01-02.tar.gz",
                      "api-2025-11-15.tar.gz", "notes.txt", "api-2026-1-04.tar.gz", "web-2026-01-05.tar.gz"],
            "links": {"latest.tar.gz": "api-2026-01-03.tar.gz", "api-2026-09-09.tar.gz": "api-2026-01-02.tar.gz"},
            "dirs": ["api-2020-01-01.tar.gz"],
        },
        "db": {
            "files": ["db-2026-03-01.tar.gz", "db-2026-02-01.tar.gz", "db-2026-01-01.tar.gz", "db-2025-12-01.tar.gz", ".lock"],
            "links": {}, "dirs": [],
        },
        "web": {"files": ["web-2026-05-05.tar.gz", "web-2026-05-06.tar.gz"], "links": {}, "dirs": []},
        "empty": {"files": [], "links": {}, "dirs": []},
    },
}
DATED = re.compile(r"^(?P<service>.+)-(\d{4})-(\d{2})-(\d{2})\.tar\.gz$")

PLANS = {
    "public": (["--keep", "2", "/work/backups"], 2, False, True),
    "defaults": (["/work/backups"], 3, False, True),
    "dry-run": (["--dry-run", "--keep", "1", "/work/backups"], 1, True, True),
    "bad-keep": (["--keep", "0", "/work/backups"], None, False, True),
    "no-dir": (["--keep", "2", "/work/nothing-here"], None, False, True),
}


def setup(variant: str, work: Path) -> Setup:
    for service, spec in TREES["standard"].items():
        (work / "backups" / service).mkdir(parents=True)
        for name in spec["files"]:
            write(work, f"backups/{service}/{name}", f"{service}/{name}\n")
        for name, target in spec["links"].items():
            symlink(work, f"backups/{service}/{name}", target)
        for name in spec["dirs"]:
            (work / "backups" / service / name).mkdir()
    write(work, "backups/README.md", "one directory per service\n")
    return Setup(argv=list(PLANS[variant][0]))


def doomed(keep: int) -> dict[str, list[str]]:
    result = {}
    for service, spec in TREES["standard"].items():
        if ".lock" in spec["files"]:
            result[service] = []
            continue
        dated = sorted(
            (n for n in spec["files"] if (m := DATED.match(n)) and m.group("service") == service),
            key=lambda n: DATED.match(n).group(2, 3, 4), reverse=True,
        )
        result[service] = sorted(dated[keep:])
    return result


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    _, keep, dry, _ = PLANS[variant]
    deleted = sorted(set(outcome.before) - set(outcome.after))
    created = sorted(set(outcome.after) - set(outcome.before))
    if created:
        return Verdict.wrong(f"the tool created {created}")
    if keep is None:
        if outcome.exit != 2:
            return Verdict.wrong(f"exit status {outcome.exit}, expected 2 for a usage error")
        if deleted:
            return Verdict.wrong(f"a usage error deleted {len(deleted)} entries")
        if not outcome.stderr.strip():
            return Verdict.wrong("a usage error printed nothing to standard error")
        return Verdict.ok()
    want = doomed(keep)
    want_deleted = [] if dry else sorted(f"backups/{s}/{n}" for s, names in want.items() for n in names)
    if deleted != want_deleted:
        extra = sorted(set(deleted) - set(want_deleted))
        kept = sorted(set(want_deleted) - set(deleted))
        return Verdict.wrong(f"deleted {len(extra)} entries it should have kept and kept {len(kept)} it should have deleted")
    modified = [p for p in outcome.after if outcome.after[p] != outcome.before.get(p) and outcome.after[p].get("kind") == "file"]
    if modified:
        return Verdict.wrong(f"the tool modified {modified}")
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    result, problem = stdout_json(outcome)
    if problem:
        return Verdict.wrong(problem)
    if result != want:
        return Verdict.wrong("the printed report differs from the expected report")
    return Verdict.ok(deleted=len(deleted))
