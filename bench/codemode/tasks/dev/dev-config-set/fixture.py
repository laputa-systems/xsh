from pathlib import Path

from fixtures import Outcome, Setup, Verdict, write

BASE = """# app.conf: managed by hand
log_level = info

[server]
host = 0.0.0.0
port=8080
# port = 9090
workers = 4

[database]
dsn = postgres://app@db/app
pool = 5
; pool = 50

[ui]
title = Fixture App
"""

TRICKY = """name = top-level
[server]
port = 1

; a comment between settings
port = 2
mode = a=b=c

[server-extra]
port = 3
[cache]
# only comments here

[empty]
"""

PLANS = {
    "public": (BASE, ["server.port=9000", "database.timeout=30s", "metrics.enabled=true", "ui.title=Fixture App v2"]),
    "idempotent": (BASE, ["server.host=0.0.0.0", "ui.title=Fixture App", "database.pool=5"]),
    "usage": (BASE, ["server.port=9000", "no_dot_here=1", "database.pool=7"]),
    "tricky": (TRICKY, [
        "server.port=10", "server.mode=x=y", "cache.size=64 MB", "empty.flag=", "server-extra.port=4",
        "name.value=keep", "server.port=11",
    ]),
}


def parse(argument: str) -> tuple[str, str, str] | None:
    if "=" not in argument:
        return None
    left, value = argument.split("=", 1)
    if "." not in left:
        return None
    section, key = left.split(".", 1)
    if not section or not key:
        return None
    return section, key, value


def header(line: str) -> str | None:
    text = line.strip()
    return text[1:-1] if len(text) >= 2 and text.startswith("[") and text.endswith("]") else None


def setting_key(line: str) -> str | None:
    text = line.strip()
    if header(line) is not None or text.startswith(("#", ";")) or "=" not in line:
        return None
    return line.split("=", 1)[0].strip()


def apply(lines: list[str], section: str, key: str, value: str) -> list[str]:
    wanted = f"{key} = {value}"
    current, header_at, last = None, None, None
    for index, line in enumerate(lines):
        name = header(line)
        if name is not None:
            current = name
            if name == section and header_at is None:
                header_at = index
            continue
        if current == section and header_at is not None:
            found = setting_key(line)
            if found is not None:
                if found == key:
                    return lines[:index] + [wanted] + lines[index + 1:]
                last = index
    if header_at is None:
        return lines + [f"[{section}]", wanted]
    at = (last if last is not None else header_at) + 1
    return lines[:at] + [wanted] + lines[at:]


def expected(variant: str) -> tuple[int, str, str]:
    text, arguments = PLANS[variant]
    parsed = [parse(a) for a in arguments]
    if any(p is None for p in parsed):
        return 2, text, ""
    lines = text.split("\n")[:-1]
    for section, key, value in parsed:
        lines = apply(lines, section, key, value)
    new = "\n".join(lines) + "\n"
    return 0, new, "unchanged" if new == text else "changed"


def witness(work: Path) -> Path:
    return work.parent / "app.conf.witness"


def setup(variant: str, work: Path) -> Setup:
    text, arguments = PLANS[variant]
    write(work, "app.conf", text)
    witness(work).hardlink_to(work / "app.conf")
    return Setup(argv=list(arguments))


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    status, text, line = expected(variant)
    if set(outcome.after) != set(outcome.before):
        extra = sorted(set(outcome.after) - set(outcome.before))
        return Verdict.wrong(f"files appeared or vanished in /work: {extra}")
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    current = (work / "app.conf").read_text()
    if current != text:
        return Verdict.wrong("app.conf differs from the expected text")
    before, after = outcome.before["app.conf"], outcome.after["app.conf"]
    if status == 2:
        if not outcome.stderr.strip():
            return Verdict.wrong("a usage error printed nothing to standard error")
        if (before["inode"], before["mtime_ns"]) != (after["inode"], after["mtime_ns"]):
            return Verdict.wrong("app.conf was rewritten after a usage error")
        return Verdict.ok()
    if outcome.stdout.strip() != line:
        return Verdict.wrong(f"printed {outcome.stdout.strip()[:40]!r}, expected {line!r}")
    if line == "unchanged" and (before["inode"], before["mtime_ns"]) != (after["inode"], after["mtime_ns"]):
        return Verdict.wrong("app.conf was rewritten although nothing changed")
    if line == "changed" and witness(work).read_text() != PLANS[variant][0]:
        return Verdict.wrong("app.conf was rewritten in place instead of renamed over")
    return Verdict.ok()
