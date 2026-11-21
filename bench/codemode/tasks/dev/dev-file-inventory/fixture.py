import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, stdout_jsonl, symlink, write

TREES = {
    "public": {
        "files": {
            "report final.txt": "quarterly numbers\n",
            "notes/$HOME.md": "not a variable\n",
            "notes/star*.log": "literal star\n",
            ".env": "TOKEN=fixture\n",
            ".cache/index.bin": "\x00\x01\x02\x03",
            "src/main.c": "int main(void) { return 0; }\n",
            "src/lib/util.h": "#pragma once\n",
            "-rf": "leading dash\n",
            "Makefile": "all:\n",
        },
        "links": {"latest": "src/main.c", "src/up": "..", "dangling": "nowhere/file.txt"},
        "dirs": ["empty", "notes/archive"],
    },
    "tricky": {
        "files": {
            ".gitignore": "*.log\nbuild/\n",
            "app.log": "ignored by git, not by you\n",
            "build/out.o": "objects",
            "quote'd \"name\".txt": "quotes\n",
            "café.txt": "accent\n",
            "line\nbreak.txt": "newline in the name\n",
            "Zeta/alpha.txt": "uppercase sorts first\n",
            "zeta/alpha.txt": "lowercase\n",
            "a b/c d/e f.txt": "spaces everywhere\n",
            ".hidden/.deeper/.x": "x",
        },
        "links": {"loop": ".", "a b/link to dir": "../Zeta", "abs": "/etc/hostname"},
        "dirs": [".hidden/empty"],
    },
    "flat": {
        "files": {"one.txt": "1", "two.txt": "22", "three.txt": "333"},
        "links": {},
        "dirs": [],
    },
}


def setup(variant: str, work: Path) -> Setup:
    tree = TREES[variant]
    (work / "data").mkdir()
    for name, content in tree["files"].items():
        write(work, f"data/{name}", content)
    for name in tree["dirs"]:
        (work / "data" / name).mkdir(parents=True, exist_ok=True)
    for name, target in tree["links"].items():
        symlink(work, f"data/{name}", target)
    return Setup()


def expected(variant: str) -> tuple[list[dict], dict]:
    tree = TREES[variant]
    rows = [{"path": n, "kind": "file", "size": len(c.encode())} for n, c in tree["files"].items()]
    rows += [{"path": n, "kind": "symlink", "target": t} for n, t in tree["links"].items()]
    rows.sort(key=lambda r: r["path"].encode())
    summary = {
        "files": len(tree["files"]), "symlinks": len(tree["links"]),
        "bytes": sum(len(c.encode()) for c in tree["files"].values()),
    }
    return rows, summary


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    before = {p: v for p, v in outcome.before.items() if p.startswith("data")}
    after = {p: v for p, v in outcome.after.items() if p.startswith("data")}
    if before != after:
        return Verdict.wrong("the program changed /work/data")
    rows, problem = stdout_jsonl(outcome)
    if problem:
        return Verdict.wrong(problem)
    want_rows, want_summary = expected(variant)
    if rows != want_rows:
        got = {r.get("path") for r in rows if isinstance(r, dict)}
        missing = len({r["path"] for r in want_rows} - got)
        return Verdict.wrong(f"the listing differs from the expected listing ({missing} expected paths missing)")
    path = work / "summary.json"
    if not path.is_file():
        return Verdict.wrong("summary.json was not written")
    try:
        summary = json.loads(path.read_text())
    except json.JSONDecodeError:
        return Verdict.wrong("summary.json is not JSON")
    if summary != want_summary:
        return Verdict.wrong("summary.json differs from the expected summary")
    return Verdict.ok(entries=len(rows))
