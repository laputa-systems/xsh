from pathlib import Path

from fixtures import Outcome, Setup, Verdict, symlink, tree_effects, write

TREES = {
    "standard": {
        "files": {
            "README": 120,
            "src/main.c": 4000,
            "src/util/strings.c": 2500,
            "src/util/.cache/obj.o": 9000,
            "docs/guide v2.md": 1500,
            "docs/images/logo.png": 30000,
            ".git/objects/ab/cdef": 7000,
            ".git/HEAD": 23,
            "empty-dir/.keep": 0,
            "build/out/app": 4000,
            "build/out/app.map": 2500,
            "build/log.txt": 0,
        },
        "dirs": ["tmp", "tmp/a", "src/util/empty"],
        "links": {"latest": "build", "big.iso": "/dev/zero", "docs/link-to-src": "../src"},
    },
}

VARIANTS = {
    "public": {"argv": ["/work/data"], "depth": 1, "min": 0, "tree": "standard"},
    "deep": {"argv": ["--depth", "3", "--min-bytes", "2500", "/work/data"], "depth": 3, "min": 2500,
             "tree": "standard"},
    "flat": {"argv": ["--depth", "0", "data"], "depth": 0, "min": 0, "tree": "standard"},
    "usage": {"argv": ["--depth", "-1", "/work/data"], "tree": "standard", "status": 2},
    "missing": {"argv": ["/work/data/README"], "tree": "standard", "status": 2},
}


def directories(tree: dict) -> set[str]:
    found = {"."}
    for rel in list(tree["files"]) + [d + "/x" for d in tree["dirs"]]:
        parts = rel.split("/")[:-1]
        for i in range(1, len(parts) + 1):
            found.add("/".join(parts[:i]))
    return found


def expected(variant: str) -> str:
    spec = VARIANTS[variant]
    tree = TREES[spec["tree"]]
    rows = []
    for d in directories(tree):
        depth = 0 if d == "." else d.count("/") + 1
        if depth > spec["depth"]:
            continue
        total = sum(size for rel, size in tree["files"].items() if d == "." or rel.startswith(d + "/"))
        if total >= spec["min"]:
            rows.append((total, d))
    rows.sort(key=lambda r: (-r[0], r[1].encode()))
    return "".join(f"{total}\t{d}\n" for total, d in rows)


def setup(variant: str, work: Path) -> Setup:
    spec = VARIANTS[variant]
    tree = TREES[spec["tree"]]
    for rel, size in tree["files"].items():
        write(work, f"data/{rel}", bytes((i * 7 + 3) % 256 for i in range(size)))
    for rel in tree["dirs"]:
        (work / "data" / rel).mkdir(parents=True, exist_ok=True)
    for rel, target in tree["links"].items():
        symlink(work, f"data/{rel}", target)
    return Setup(argv=list(spec["argv"]))


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    spec = VARIANTS[variant]
    effects = tree_effects(outcome.before, outcome.after)
    if any(effects.values()):
        return Verdict.wrong(f"the command changed /work: {effects}")
    if "status" in spec:
        if outcome.exit != spec["status"]:
            return Verdict.wrong(f"exit status {outcome.exit}, expected {spec['status']}")
        if outcome.stdout.strip():
            return Verdict.wrong("a usage or input error still printed a summary")
        return Verdict.ok()
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    if outcome.stdout != expected(variant):
        return Verdict.wrong("the summary differs from the expected lines")
    return Verdict.ok(lines=outcome.stdout.count("\n"))
