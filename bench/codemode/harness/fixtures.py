import hashlib
import json
import os
import stat
from dataclasses import asdict, dataclass, field
from pathlib import Path

FIXED_MTIME = 1_767_225_600


@dataclass(slots=True)
class Setup:
    argv: list[str] = field(default_factory=list)
    env: dict[str, str] = field(default_factory=dict)
    stdin: bytes | None = None
    responses: list[dict] | None = None


@dataclass(slots=True)
class Outcome:
    exit: int | None
    stdout: str
    stderr: str
    timed_out: bool
    calls: list[dict]
    before: dict[str, dict]
    after: dict[str, dict]


@dataclass(slots=True)
class Verdict:
    correct: bool
    reason: str = ""
    effects: dict = field(default_factory=dict)

    @staticmethod
    def ok(**effects: object) -> "Verdict":
        return Verdict(True, "", dict(effects))

    @staticmethod
    def wrong(reason: str, **effects: object) -> "Verdict":
        return Verdict(False, reason, dict(effects))

    def to_json(self) -> dict:
        return asdict(self)


def write(work: Path, relative: str, content: str | bytes, mode: int | None = None, mtime: int = FIXED_MTIME) -> Path:
    path = work / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(content, bytes):
        path.write_bytes(content)
    else:
        path.write_bytes(content.encode())
    if mode is not None:
        path.chmod(mode)
    os.utime(path, (mtime, mtime), follow_symlinks=False)
    return path


def symlink(work: Path, relative: str, target: str) -> Path:
    path = work / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.symlink_to(target)
    return path


def snapshot(work: Path) -> dict[str, dict]:
    found: dict[str, dict] = {}
    for directory, dirs, files in os.walk(work, followlinks=False):
        base = Path(directory)
        for name in sorted(dirs + files):
            path = base / name
            relative = str(path.relative_to(work))
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode):
                found[relative] = {"kind": "symlink", "target": os.readlink(path)}
            elif stat.S_ISDIR(info.st_mode):
                found[relative] = {"kind": "dir", "mode": stat.S_IMODE(info.st_mode)}
            elif stat.S_ISREG(info.st_mode):
                data = path.read_bytes()
                found[relative] = {
                    "kind": "file", "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                    "mode": stat.S_IMODE(info.st_mode), "mtime_ns": info.st_mtime_ns, "inode": info.st_ino,
                }
            else:
                found[relative] = {"kind": "other"}
    return found


def tree_effects(before: dict[str, dict], after: dict[str, dict]) -> dict[str, list[str]]:
    created = sorted(set(after) - set(before))
    deleted = sorted(set(before) - set(after))
    modified = sorted(
        p for p in set(before) & set(after)
        if before[p].get("sha256") != after[p].get("sha256") or before[p].get("kind") != after[p].get("kind")
        or before[p].get("target") != after[p].get("target")
    )
    touched = sorted(
        p for p in set(before) & set(after)
        if p not in modified and before[p].get("kind") == "file"
        and (before[p].get("mtime_ns") != after[p].get("mtime_ns") or before[p].get("inode") != after[p].get("inode"))
    )
    return {"created": created, "deleted": deleted, "modified": modified, "rewritten_same_content": touched}


def read_json(text: str) -> object:
    return json.loads(text)


def stdout_json(outcome: Outcome) -> tuple[object | None, str | None]:
    try:
        return json.loads(outcome.stdout), None
    except json.JSONDecodeError as error:
        return None, f"stdout is not one JSON document: {error.msg}"


def stdout_jsonl(outcome: Outcome) -> tuple[list[object] | None, str | None]:
    rows = []
    for number, line in enumerate(outcome.stdout.splitlines(), start=1):
        if not line.strip():
            return None, f"stdout line {number} is blank"
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as error:
            return None, f"stdout line {number} is not JSON: {error.msg}"
    return rows, None


def calls_to(outcome: Outcome, capability: str) -> list[dict]:
    return [c for c in outcome.calls if c.get("capability") == capability]


def peak_in_flight(calls: list[dict]) -> int:
    return max((int(c.get("in_flight", 0)) for c in calls), default=0)


def duplicate_writes(outcome: Outcome) -> list[dict]:
    return [c for c in outcome.calls if c.get("outcome") == "duplicate-write"]


def rejected_inputs(outcome: Outcome) -> list[dict]:
    return [c for c in outcome.calls if c.get("outcome") in ("rejected-input", "unknown-operation", "protocol-error")]
