import base64
import hashlib
from pathlib import Path

from fixtures import (
    Outcome, Setup, Verdict, calls_to, duplicate_writes, peak_in_flight, rejected_inputs, stdout_json, symlink,
    tree_effects, write,
)

PNG = bytes(range(256)) + b"\x89PNG\r\n\x1a\n\xff\xfe"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


VARIANTS = {
    "public": {
        "argv": ["site-prod"], "bucket": "site-prod", "delete": False,
        "files": {
            "index.html": b"<h1>Welcome</h1>\n",
            "css/site.css": b"body { margin: 0 }\n",
            "img/logo.png": PNG,
            "docs/getting started.html": b"<p>Start here</p>\n",
            "docs/faq.html": b"<p>FAQ</p>\n",
            "robots.txt": b"User-agent: *\n",
            ".env": b"SECRET=1\n",
            ".git/config": b"[core]\n",
            "docs/.draft.html": b"<p>draft</p>\n",
        },
        "links": {"latest": "docs", "home.html": "index.html"},
        "stored": {"index.html": b"<h1>Welcome</h1>\n", "css/site.css": b"body { margin: 1px }\n",
                   "old/page.html": b"<p>old</p>\n", "robots.txt": b"User-agent: *\n"},
        "failures": {},
    },
    "delete": {
        "argv": ["--delete", "site-stage"], "bucket": "site-stage", "delete": True,
        "files": {
            "a.txt": b"alpha\n",
            "b/c.txt": b"charlie\n",
            "b/d e.txt": b"delta echo\n",
            "z.bin": b"\x00\x01\x02",
        },
        "links": {},
        "stored": {"a.txt": b"alpha\n", "b/c.txt": b"charlie (old)\n", "gone.txt": b"x\n", "b/old.txt": b"y\n",
                   "vanished.txt": b"z\n", ".hidden-remote": b"h\n"},
        "failures": {"z.bin": ("PermissionDenied", False), "vanished.txt": ("NotFound", False)},
    },
    "conflict": {
        "argv": ["site-prod"], "bucket": "site-prod", "delete": False,
        "files": {"one.html": b"1\n", "two.html": b"2\n", "three.html": b"3\n", "four.html": b"4\n",
                  "five.html": b"5\n"},
        "links": {},
        "stored": {"gone.html": b"g\n"},
        "failures": {"two.html": ("Conflict", True), "four.html": ("Failed", False)},
    },
    "only-hidden": {
        "argv": ["--delete", "bucket-x"], "bucket": "bucket-x", "delete": True,
        "files": {".htaccess": b"deny\n", ".well-known/security.txt": b"contact\n"},
        "links": {"index.html": "/etc/hostname"},
        "stored": {"index.html": b"stale\n", "img/x.png": b"x"},
        "failures": {},
    },
    "usage": {
        "argv": ["--force", "site-prod"], "bucket": "site-prod", "delete": False,
        "files": {"index.html": b"hi\n"}, "links": {}, "stored": {}, "failures": {}, "status": 2,
    },
}


def local(variant: str) -> dict[str, bytes]:
    files = VARIANTS[variant]["files"]
    return {k: v for k, v in files.items() if not any(part.startswith(".") for part in k.split("/"))}


def plan(variant: str) -> tuple[list[str], list[str], int]:
    spec = VARIANTS[variant]
    mine = local(variant)
    uploads = [k for k, v in mine.items() if k not in spec["stored"] or sha(spec["stored"][k]) != sha(v)]
    deletes = [k for k in spec["stored"] if k not in mine] if spec["delete"] else []
    return uploads, deletes, len(mine) - len(uploads)


def failed_keys(variant: str) -> list[str]:
    spec = VARIANTS[variant]
    uploads, deletes, _ = plan(variant)
    kinds = {k: f[0] for k, f in spec["failures"].items()}
    failed = [k for k in uploads if kinds.get(k, "Conflict") != "Conflict"]
    failed += [k for k in deletes if kinds.get(k, "Conflict") not in ("Conflict", "NotFound")]
    return sorted(failed, key=str.encode)


def result(spec: dict, key: str) -> dict:
    failure = spec["failures"].get(key)
    if failure:
        return {"error": {"kind": failure[0], "message": f"{key}: {failure[0]}", "performed": failure[1]},
                "delay_ms": 150}
    return {"output": {"key": key}, "delay_ms": 200}


def setup(variant: str, work: Path) -> Setup:
    spec = VARIANTS[variant]
    for rel, content in spec["files"].items():
        write(work, f"site/{rel}", content)
    for rel, target in spec["links"].items():
        symlink(work, f"site/{rel}", target)
    bucket = spec["bucket"]
    listing = [{"key": k, "sha256": sha(v), "size": len(v)} for k, v in spec["stored"].items()]
    responses = [{"capability": "store.list", "input": {"bucket": bucket}, "results": [{"output": listing}]}]
    for key, content in spec["files"].items():
        responses.append({"capability": "store.put",
                          "input": {"bucket": bucket, "key": key, "sha256": sha(content),
                                    "content_base64": base64.b64encode(content).decode()},
                          "results": [result(spec, key)]})
    for key in spec["stored"]:
        responses.append({"capability": "store.delete", "input": {"bucket": bucket, "key": key},
                          "results": [result(spec, key)]})
    return Setup(argv=list(spec["argv"]), responses=responses)


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    spec = VARIANTS[variant]
    effects = tree_effects(outcome.before, outcome.after)
    if any(effects.values()):
        return Verdict.wrong(f"the program changed /work: {effects}")
    if "status" in spec:
        if outcome.exit != spec["status"]:
            return Verdict.wrong(f"exit status {outcome.exit}, expected {spec['status']}")
        if outcome.calls:
            return Verdict.wrong("the service was called despite a usage error")
        return Verdict.ok()
    uploads, deletes, unchanged = plan(variant)
    failed = failed_keys(variant)
    status = 4 if failed else 0
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls (wrong key, digest or content?)")
    if duplicate_writes(outcome):
        return Verdict.wrong("an object was written twice")
    if not calls_to(outcome, "store.list"):
        return Verdict.wrong("the bucket was never listed")
    puts = sorted(c["input"]["key"] for c in calls_to(outcome, "store.put"))
    dels = sorted(c["input"]["key"] for c in calls_to(outcome, "store.delete"))
    if puts != sorted(uploads):
        return Verdict.wrong(f"uploaded {puts}, expected {sorted(uploads)}")
    if dels != sorted(deletes):
        return Verdict.wrong(f"deleted {dels}, expected {sorted(deletes)}")
    writes = calls_to(outcome, "store.put") + calls_to(outcome, "store.delete")
    peak = peak_in_flight(writes)
    if peak > 3:
        return Verdict.wrong(f"{peak} writes were in flight at once")
    if len(writes) >= 2 and peak < min(3, len(writes)):
        return Verdict.wrong(f"only {peak} writes were in flight at once")
    report, problem = stdout_json(outcome)
    if problem:
        return Verdict.wrong(problem)
    wanted = {
        "uploaded": sorted((k for k in uploads if k not in failed), key=str.encode),
        "deleted": sorted((k for k in deletes if k not in failed), key=str.encode),
        "failed": failed,
        "unchanged": unchanged,
    }
    if report != wanted:
        return Verdict.wrong("the printed report differs from the expected result")
    return Verdict.ok(writes=len(writes))
