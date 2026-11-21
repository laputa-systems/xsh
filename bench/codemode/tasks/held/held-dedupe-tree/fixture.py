import hashlib
import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, stdout_json, symlink, write

ROOT = "photos"
DEST = "duplicates"

# Each tree: files as (relative path, content, mode), symlinks as (relative path, target),
# and files already under /work/duplicates.
TREES = {
    "public": {
        "files": [
            ("IMG 0001.jpg", b"\xff\xd8 sunrise over the bay", 0o644),
            ("2024/summer/IMG 0001.jpg", b"\xff\xd8 sunrise over the bay", 0o644),
            ("2024/summer/IMG 0002.jpg", b"\xff\xd8 the harbour at noon", 0o644),
            ("2024/backup/IMG 0002 (1).jpg", b"\xff\xd8 the harbour at noon", 0o600),
            ("2024/backup/notes.txt", b"camera settings\n", 0o644),
            (".thumbs/IMG 0001.jpg", b"thumb:sunrise", 0o644),
            ("raw/a.cr2", b"RAW-A-0123456789", 0o644),
            ("raw/b.cr2", b"RAW-B-0123456789", 0o644),
            ("raw/empty.cr2", b"", 0o644),
            ("raw/also-empty.cr2", b"", 0o644),
            ("Edits/IMG 0001 final.jpg", b"\xff\xd8 sunrise over the bay", 0o640),
        ],
        "links": [("favourite.jpg", "IMG 0001.jpg"), ("latest", "2024/summer")],
        "existing": [],
    },
    "tricky": {
        "files": [
            ("b/report.pdf", b"%PDF-1.7 quarterly", 0o644),
            ("a/report.pdf", b"%PDF-1.7 quarterly", 0o644),
            ("A/report.pdf", b"%PDF-1.7 quarterly", 0o644),
            (".cache/report.pdf", b"%PDF-1.7 quarterly", 0o644),
            ("odd $name*.png", b"PNG glob-ish", 0o644),
            ("odd [1].png", b"PNG glob-ish", 0o644),
            ("line\nbreak.png", b"PNG glob-ish", 0o644),
            ("café/menu.txt", b"soup of the day\n", 0o644),
            ("cafe/menu.txt", b"soup of the day\n", 0o644),
            ("same-size-1.bin", b"AAAA", 0o644),
            ("same-size-2.bin", b"AAAB", 0o644),
            ("outside/keep.txt", b"linked tree\n", 0o644),
        ],
        "links": [("linked-dir", "outside"), ("linked-file.pdf", "b/report.pdf"), ("dangling.png", "missing.png")],
        "existing": [("README", b"previous run\n", 0o644)],
    },
    "unique": {
        "files": [
            ("one.jpg", b"one", 0o644),
            ("two.jpg", b"two", 0o644),
            ("sub/three.jpg", b"three", 0o644),
            ("sub/.hidden.jpg", b"four", 0o644),
            ("blank-a", b"", 0o644),
            ("sub/blank-b", b"", 0o644),
        ],
        "links": [("one-again.jpg", "one.jpg")],
        "existing": [],
    },
    "nested": {
        "files": [
            ("z/y/x/w/deep.dat", b"deep payload " * 40, 0o644),
            ("0-first/deep.dat", b"deep payload " * 40, 0o755),
            ("m/n/deep copy.dat", b"deep payload " * 40, 0o644),
            ("m/n/other.dat", b"other payload", 0o644),
            ("m/.n/other.dat", b"other payload", 0o644),
            ("m/n-1/other.dat", b"other payload", 0o644),
            ("solo/x.dat", b"alone", 0o644),
        ],
        "links": [("m/loop", "..")],
        "existing": [("m/n/unrelated.dat", b"kept from before", 0o644)],
    },
}


def groups(variant: str) -> list[dict]:
    by_hash: dict[str, list[str]] = {}
    for rel, content, _ in TREES[variant]["files"]:
        if content:
            by_hash.setdefault(hashlib.sha256(content).hexdigest(), []).append(rel)
    found = []
    for paths in by_hash.values():
        if len(paths) > 1:
            ordered = sorted(paths, key=lambda p: p.encode())
            found.append({"kept": ordered[0], "moved": ordered[1:]})
    return sorted(found, key=lambda g: g["kept"].encode())


def setup(variant: str, work: Path) -> Setup:
    tree = TREES[variant]
    for rel, content, mode in tree["files"]:
        write(work, f"{ROOT}/{rel}", content, mode=mode)
    for rel, target in tree["links"]:
        symlink(work, f"{ROOT}/{rel}", target)
    for rel, content, mode in tree["existing"]:
        write(work, f"{DEST}/{rel}", content, mode=mode)
    return Setup()


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    wanted = groups(variant)
    sizes = {rel: len(content) for rel, content, _ in TREES[variant]["files"]}
    moved = {rel for g in wanted for rel in g["moved"]}
    before, after = outcome.before, outcome.after
    for rel in moved:
        source, dest = f"{ROOT}/{rel}", f"{DEST}/{rel}"
        if source in after:
            return Verdict.wrong(f"{rel!r} was not moved away")
        if dest not in after:
            return Verdict.wrong(f"{rel!r} is missing under /work/duplicates")
        old, new = before[source], after[dest]
        for key in ("sha256", "mode", "mtime_ns", "inode"):
            if old.get(key) != new.get(key):
                return Verdict.wrong(f"{rel!r} was not moved by renaming ({key} changed)")
    for rel, info in before.items():
        if rel.startswith(f"{ROOT}/") and rel[len(ROOT) + 1:] in moved:
            continue
        if after.get(rel) != info and not (info["kind"] == "dir" and after.get(rel, {}).get("kind") == "dir"):
            return Verdict.wrong(f"{rel!r} was changed or removed")
    allowed_new = {f"{DEST}/{rel}" for rel in moved}
    for rel in moved:
        parts = f"{DEST}/{rel}".split("/")
        allowed_new |= {"/".join(parts[:i]) for i in range(1, len(parts))}
    created = sorted(set(after) - set(before))
    unexpected = [c for c in created if c not in allowed_new]
    if unexpected:
        return Verdict.wrong(f"unexpected new entries: {unexpected}")
    if not moved and DEST in after and DEST not in before:
        return Verdict.wrong("/work/duplicates was created although nothing was moved")
    result, problem = stdout_json(outcome)
    if problem:
        return Verdict.wrong(problem)
    expected = {"groups": wanted, "moved_bytes": sum(sizes[r] for r in moved)}
    if result != expected:
        return Verdict.wrong("the printed report differs from the expected groups")
    return Verdict.ok(moved=len(moved))
