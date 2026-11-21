import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, tree_effects, write

BEGIN = "# BEGIN fleet"
END = "# END fleet"
HOSTS = "etc/hosts"

PUBLIC_FLEET = [
    {"ip": "10.0.0.11", "names": ["web-1", "web-1.fleet"]},
    {"ip": "10.0.0.12", "names": ["web-2"]},
    {"ip": "10.0.0.99", "names": []},
    {"ip": "fd00::5", "names": ["db-1", "db-1.fleet", "db"]},
]

VARIANTS = {
    "public": {
        "mode": 0o644,
        "hosts": (
            "# static entries\n"
            "127.0.0.1\tlocalhost\n"
            "::1\tlocalhost ip6-localhost\n"
            "\n"
            f"{BEGIN}\n"
            "10.0.0.1\told-web\n"
            "10.0.0.2\told-db\n"
            f"{END}\n"
            "192.168.1.50\tprinter.lan   # office printer\n"
        ),
        "fleet": PUBLIC_FLEET,
    },
    "append": {
        "mode": 0o640,
        "hosts": (
            "127.0.0.1 localhost\r\n"
            "10.1.1.1\tlegacy   \r\n"
            "#BEGIN fleet\n"
            "192.168.0.1 router"
        ),
        "fleet": [{"ip": "172.16.0.4", "names": ["cache"]}, {"ip": "172.16.0.5", "names": ["queue", "mq"]}],
    },
    "same": {
        "mode": 0o600,
        "hosts": (
            "127.0.0.1\tlocalhost\r\n"
            f"{BEGIN}\n"
            "10.2.0.1\tapi api.internal\n"
            "10.2.0.2\tworker\n"
            f"{END}\n"
            "# trailing comment without newline"
        ),
        "fleet": [{"ip": "10.2.0.1", "names": ["api", "api.internal"]}, {"ip": "10.2.0.9", "names": []},
                  {"ip": "10.2.0.2", "names": ["worker"]}],
    },
    "broken": {
        "mode": 0o644,
        "hosts": f"127.0.0.1\tlocalhost\n{BEGIN}\n10.0.0.1\told\n{END}  \n",
        "fleet": PUBLIC_FLEET,
        "status": 4,
    },
    "reversed": {
        "mode": 0o644,
        "hosts": f"127.0.0.1\tlocalhost\n{END}\n10.0.0.1\told\n{BEGIN}\n",
        "fleet": PUBLIC_FLEET,
        "status": 4,
    },
    "bad-fleet": {
        "mode": 0o644,
        "hosts": f"127.0.0.1\tlocalhost\n{BEGIN}\n{END}\n",
        "fleet": [{"ip": "10.0.0.1", "names": "web-1"}],
        "status": 4,
    },
}


def block(fleet: list[dict]) -> str:
    return "".join(f"{e['ip']}\t{' '.join(e['names'])}\n" for e in fleet if e["names"])


def expected(variant: str) -> str:
    spec = VARIANTS[variant]
    text = spec["hosts"]
    parts = text.split("\n")
    begins = [i for i, line in enumerate(parts) if line == BEGIN]
    ends = [i for i, line in enumerate(parts) if line == END]
    content = block(spec["fleet"])
    if not begins and not ends:
        if text and not text.endswith("\n"):
            text += "\n"
        return text + f"{BEGIN}\n{content}{END}\n"
    assert len(begins) == 1 and len(ends) == 1 and begins[0] < ends[0]
    head = "\n".join(parts[: begins[0] + 1]) + "\n"
    tail = "\n".join(parts[ends[0]:])
    return head + content + tail


def setup(variant: str, work: Path) -> Setup:
    spec = VARIANTS[variant]
    write(work, HOSTS, spec["hosts"], mode=spec["mode"])
    write(work, "fleet.json", json.dumps(spec["fleet"], indent=2) + "\n")
    return Setup()


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    spec = VARIANTS[variant]
    effects = tree_effects(outcome.before, outcome.after)
    if "status" in spec:
        if outcome.exit != spec["status"]:
            return Verdict.wrong(f"exit status {outcome.exit}, expected {spec['status']}")
        if any(effects.values()):
            return Verdict.wrong(f"a refused update changed files: {effects}")
        return Verdict.ok()
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    wanted = expected(variant)
    if wanted == spec["hosts"]:
        if any(effects.values()):
            return Verdict.wrong(f"an unchanged hosts file was written: {effects}")
        if outcome.stdout.strip() != "unchanged":
            return Verdict.wrong("the command did not print unchanged")
        return Verdict.ok()
    if effects["created"] or effects["deleted"] or effects["modified"] != [HOSTS]:
        return Verdict.wrong(f"unexpected file changes: {effects}")
    if (work / HOSTS).read_bytes() != wanted.encode():
        return Verdict.wrong("the hosts file does not hold the expected content")
    before, after = outcome.before[HOSTS], outcome.after[HOSTS]
    if before["mode"] != after["mode"]:
        return Verdict.wrong("the hosts file lost its permission bits")
    if before["inode"] == after["inode"]:
        return Verdict.wrong("the hosts file was rewritten in place instead of replaced in one step")
    if outcome.stdout.strip() != "updated":
        return Verdict.wrong("the command did not print updated")
    return Verdict.ok()
