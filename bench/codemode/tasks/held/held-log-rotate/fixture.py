import gzip
import hashlib
import json
import re
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, stdout_jsonl, symlink, tree_effects, write


def text(seed: str, size: int) -> bytes:
    line = f"{seed} request served in 12ms\n".encode()
    return (line * (size // len(line) + 1))[:size]


def gz(content: bytes) -> bytes:
    return gzip.compress(content, mtime=0)


VARIANTS = {
    "public": {
        "dir": "logs", "argv": ["/work/logs"], "keep": 3, "min": 1024,
        "files": {
            "app.log": text("app", 3000),
            "app.log.1.gz": gz(b"generation one\n"),
            "app.log.2.gz": gz(b"generation two\n"),
            "app.log.3.gz": gz(b"generation three\n"),
            "worker.log": text("worker", 2048),
            "small.log": text("small", 100),
            "small.log.1.gz": gz(b"small old\n"),
            "README": b"logs for the demo service\n",
            "archive/old.log": text("old", 5000),
        },
        "links": {"current.log": "app.log"},
    },
    "gaps": {
        "dir": "var log", "argv": ["--keep", "2", "--min-bytes", "10", "/work/var log"], "keep": 2, "min": 10,
        "files": {
            "a.log": text("a", 400),
            "a.log.1.gz": gz(b"a one\n"),
            "a.log.3.gz": gz(b"a three\n"),
            "a.log.7.gz": gz(b"a seven\n"),
            "a.log.01.gz": gz(b"not a generation\n"),
            "a.log.x.gz": gz(b"not a generation either\n"),
            ".hidden.log": text("hidden", 50),
            "b.log": text("b", 10),
            "b.log.2.gz": gz(b"b two\n"),
            "c.log": text("c", 9),
            "d.log.1.gz": gz(b"orphan\n"),
            "e.log/inner.txt": b"a directory named like a log\n",
            "notes.log.txt": text("notes", 900),
        },
        "links": {"link.log": "a.log", "dangling.log": "missing.log"},
        "modes": {"a.log": 0o640, "b.log": 0o600},
    },
    "zero": {
        "dir": "logs", "argv": ["--min-bytes", "0", "/work/logs"], "keep": 3, "min": 0,
        "files": {
            "empty.log": b"",
            "empty.log.1.gz": gz(b"was not empty\n"),
            "x.log": b"tiny\n",
            "x.log.1.gz": gz(b"x1\n"),
            "x.log.2.gz": gz(b"x2\n"),
            "x.log.3.gz": gz(b"x3\n"),
            "x.log.4.gz": gz(b"x4 from an older keep\n"),
        },
        "links": {},
    },
    "usage": {
        "dir": "logs", "argv": ["--keep", "0", "/work/logs"], "status": 2,
        "files": {"app.log": text("app", 3000), "app.log.1.gz": gz(b"one\n")},
        "links": {},
    },
}

GENERATION = re.compile(r"^(?P<base>.+)\.(?P<k>[1-9][0-9]*)\.gz$")


def plan(variant: str) -> tuple[list[dict], dict[str, bytes | None]]:
    """Expected report lines and the expected content (None: absent) of every top-level file touched."""
    spec = VARIANTS[variant]
    files = spec["files"]
    top = {name: content for name, content in files.items() if "/" not in name}
    logs = sorted((n for n in top if n.endswith(".log")), key=lambda n: n.encode())
    lines, final = [], {}
    for name in logs:
        content = top[name]
        if len(content) < max(spec["min"], 1):
            continue
        generations = []
        for other in top:
            match = GENERATION.match(other)
            if match and match.group("base") == name:
                generations.append(int(match.group("k")))
        deleted = []
        for k in sorted(generations, reverse=True):
            source = f"{name}.{k}.gz"
            if k + 1 <= spec["keep"]:
                final[f"{name}.{k + 1}.gz"] = top[source]
            else:
                deleted.append(k)
            final.setdefault(source, None)
        final[f"{name}.1.gz"] = ("gzip", content)
        final[name] = b""
        lines.append({"log": name, "bytes": len(content),
                      "deleted": [f"{name}.{k}.gz" for k in sorted(deleted)]})
    return lines, final


def setup(variant: str, work: Path) -> Setup:
    spec = VARIANTS[variant]
    modes = spec.get("modes", {})
    for name, content in spec["files"].items():
        write(work, f"{spec['dir']}/{name}", content, mode=modes.get(name, 0o644))
    for name, target in spec["links"].items():
        symlink(work, f"{spec['dir']}/{name}", target)
    return Setup(argv=list(spec["argv"]))


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    spec = VARIANTS[variant]
    effects = tree_effects(outcome.before, outcome.after)
    if "status" in spec:
        if outcome.exit != spec["status"]:
            return Verdict.wrong(f"exit status {outcome.exit}, expected {spec['status']}")
        if any(effects[k] for k in ("created", "deleted", "modified")):
            return Verdict.wrong(f"a usage error changed files: {effects}")
        return Verdict.ok()
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    lines, final = plan(variant)
    prefix = f"{spec['dir']}/"
    for name, wanted in final.items():
        rel = prefix + name
        got = outcome.after.get(rel)
        if wanted is None:
            if got is not None:
                return Verdict.wrong(f"{name} should be gone")
            continue
        if got is None or got["kind"] != "file":
            return Verdict.wrong(f"{name} is missing")
        data = (work / rel).read_bytes()
        if isinstance(wanted, tuple):
            try:
                unpacked = gzip.decompress(data)
            except (OSError, EOFError) as error:
                return Verdict.wrong(f"{name} is not gzip data: {error}")
            if unpacked != wanted[1]:
                return Verdict.wrong(f"{name} does not hold the log's previous content")
        elif data != wanted:
            return Verdict.wrong(f"{name} does not hold the expected content")
    for line in lines:
        rel = prefix + line["log"]
        if outcome.before[rel]["inode"] != outcome.after[rel]["inode"]:
            return Verdict.wrong(f"{line['log']} was replaced instead of truncated in place")
        if outcome.before[rel]["mode"] != outcome.after[rel]["mode"]:
            return Verdict.wrong(f"{line['log']} changed mode")
    touched = {prefix + name for name in final}
    for rel, info in outcome.before.items():
        if rel not in touched and outcome.after.get(rel) != info:
            return Verdict.wrong(f"{rel} was changed although it is not part of a rotation")
    extra = sorted(set(outcome.after) - set(outcome.before) - touched)
    if extra:
        return Verdict.wrong(f"unexpected new files: {extra}")
    rows, problem = stdout_jsonl(outcome)
    if problem:
        return Verdict.wrong(problem)
    if rows != lines:
        return Verdict.wrong("the printed report differs from the expected rotations")
    return Verdict.ok(rotated=len(lines))
