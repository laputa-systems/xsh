import io
import tarfile
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, write

LOGS = {
    "app.log": "2026-11-01 app started\n2026-11-01 app ready\n",
    "web access.log": "GET / 200\nGET /missing 404\n",
    "2026/11/db.log": "checkpoint complete\n",
    "unlisted.log": "should not be bundled\n",
    "2026/11/unlisted.log": "nor this\n",
    "debug.txt": "not a log\n",
}
OLD = b"old archive bytes"

PLANS = {
    "public": {"list": "app.log\n\n  web access.log  \n2026/11/db.log\n", "old": False, "missing": None},
    "missing": {"list": "app.log\n2026/11/gone.log\nweb access.log\nalso-gone.log\n", "old": True, "missing": "2026/11/gone.log"},
    "replace": {"list": "2026/11/db.log\ndebug.txt\n", "old": True, "missing": None},
}


def listed(variant: str) -> list[str]:
    return [line.strip() for line in PLANS[variant]["list"].splitlines() if line.strip()]


def setup(variant: str, work: Path) -> Setup:
    for name, content in LOGS.items():
        write(work, f"logs/{name}", content)
    write(work, "bundle.txt", PLANS[variant]["list"])
    (work / "out").mkdir()
    write(work, "out/README", "downstream reads logs.tar.gz from here\n")
    if PLANS[variant]["old"]:
        write(work, "out/logs.tar.gz", OLD)
    return Setup()


def archive_files(path: Path) -> tuple[dict[str, bytes] | None, str | None]:
    try:
        with tarfile.open(fileobj=io.BytesIO(path.read_bytes()), mode="r:gz") as archive:
            files: dict[str, bytes] = {}
            for member in archive.getmembers():
                name = member.name.removeprefix("./").rstrip("/")
                if member.isdir():
                    continue
                if not member.isfile():
                    return None, f"the archive holds a member that is not a file: {name}"
                extracted = archive.extractfile(member)
                files[name] = extracted.read() if extracted else b""
            return files, None
    except (tarfile.TarError, OSError, EOFError) as error:
        return None, f"logs.tar.gz is not a gzip-compressed tar archive: {type(error).__name__}"


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    plan = PLANS[variant]
    created = sorted(set(outcome.after) - set(outcome.before))
    out_before = {p: v for p, v in outcome.before.items() if p.startswith("out")}
    if plan["missing"] is not None:
        out_after = {p: v for p, v in outcome.after.items() if p.startswith("out")}
        if outcome.exit != 4:
            return Verdict.wrong(f"exit status {outcome.exit}, expected 4")
        if created:
            return Verdict.wrong(f"files were left behind: {created}")
        if out_after != out_before:
            return Verdict.wrong("/work/out changed although a listed file is missing")
        if plan["missing"] not in outcome.stderr:
            return Verdict.wrong("standard error does not name the first missing file")
        return Verdict.ok()
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    if created not in ([], ["out/logs.tar.gz"]):
        return Verdict.wrong(f"files were left behind: {[c for c in created if c != 'out/logs.tar.gz']}")
    if outcome.after.get("out/README") != outcome.before.get("out/README"):
        return Verdict.wrong("an unrelated file in /work/out changed")
    path = work / "out" / "logs.tar.gz"
    if not path.is_file():
        return Verdict.wrong("/work/out/logs.tar.gz was not written")
    files, problem = archive_files(path)
    if problem:
        return Verdict.wrong(problem)
    want = {f"logs/{name}": LOGS[name].encode() for name in listed(variant)}
    if set(files) != set(want):
        return Verdict.wrong(f"the archive's files are {sorted(files)}, expected {sorted(want)}")
    if files != want:
        return Verdict.wrong("a bundled file's content differs from the original")
    if outcome.stdout.strip() != str(len(want)):
        return Verdict.wrong("the printed count differs from the number of files bundled")
    return Verdict.ok()
