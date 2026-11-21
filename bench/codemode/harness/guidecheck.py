import json
import re
import shutil
import tempfile
from dataclasses import dataclass
from pathlib import Path

import boundary
from config import CONTEXTS, RUNS, XSH_IMAGE

EXAMPLE = re.compile(r"```xsh\n(.*?)```\n\n```text\n(.*?)```", re.DOTALL)


@dataclass(frozen=True, slots=True)
class Result:
    name: str
    ok: bool
    detail: str


def run_in_boundary(directory: Path, argv: list[str]) -> boundary.Execution:
    name = boundary.container_name("guide")
    command = [
        *boundary.base_flags(name, "none"), *boundary.xsh_mounts(), "-v", f"{directory}:/task:ro", "-w", "/tmp",
        XSH_IMAGE, *argv,
    ]
    return boundary.execute(command, name, 120)


def check_one(directory: Path, spec: dict) -> Result:
    file = spec["file"]
    label = f"{file} {' '.join(spec.get('args', []))}".strip()
    checked = run_in_boundary(directory, ["xsht", "check", f"/task/{file}"])
    output = checked.stdout + checked.stderr
    if spec["expect"] == "reject":
        if checked.exit != 2:
            return Result(label, False, f"xsht check exited {checked.exit}, expected 2")
        code = spec.get("code")
        if code and f"err[{code}]" not in output:
            return Result(label, False, f"the rejection lacks {code}: {output.strip()[:300]}")
        ran = run_in_boundary(directory, ["xsh", f"/task/{file}"])
        if ran.exit != 2 or ran.stdout:
            return Result(label, False, f"xsh exited {ran.exit} with output {ran.stdout[:100]!r}, expected 2 and none")
        return Result(label, True, f"rejected with {code or 'a check error'}; xsh exits 2 and runs nothing")
    if checked.exit != 0:
        return Result(label, False, f"xsht check exited {checked.exit}: {output.strip()[:500]}")
    args = spec.get("args", [])
    ran = run_in_boundary(directory, ["xsh", f"/task/{file}", *(["--", *args] if args else [])])
    status = spec.get("status", 0)
    if ran.exit != status:
        return Result(label, False, f"xsh exited {ran.exit}, expected {status}: {ran.stderr.strip()[:500]}")
    expected = directory / (Path(file).stem + ".out")
    if spec.get("compare", True) and expected.exists() and ran.stdout != expected.read_text():
        return Result(label, False, f"stdout differs:\n{ran.stdout}")
    return Result(label, True, "ran as stated")


def check_examples(guide: Path, scratch: Path) -> list[Result]:
    results = []
    for index, match in enumerate(EXAMPLE.finditer(guide.read_text())):
        directory = scratch / f"example-{index}"
        directory.mkdir()
        (directory / "example.xsh").write_text(match.group(1))
        (directory / "example.out").write_text(match.group(2))
        results.append(check_one(directory, {"file": "example.xsh", "expect": "run"}))
    return results


def verify(context: str) -> list[Result]:
    guide = CONTEXTS / context
    checks = CONTEXTS / "checks" / Path(context).stem
    spec = json.loads((checks / "checks.json").read_text())
    if spec["guide"] != context:
        raise ValueError(f"{checks / 'checks.json'} checks {spec['guide']}, not {context}")
    scratch_root = RUNS / "guide-checks"
    scratch_root.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix=Path(context).stem + "-", dir=scratch_root))
    copy = scratch / "checks"
    shutil.copytree(checks, copy)
    results = [check_one(copy, item) for item in spec["checks"]]
    for selector in spec.get("api", []):
        found = run_in_boundary(copy, ["xsht", "api", "--strict", selector])
        results.append(Result(f"xsht api {selector}", found.exit == 0, f"exit {found.exit}"))
    for selector in spec.get("plain", []):
        found = run_in_boundary(copy, ["xsht", "api", selector])
        results.append(Result(f"xsht api {selector}", found.exit == 0 and "api: " not in found.stderr, f"exit {found.exit}"))
    for terms in spec.get("search", []):
        found = run_in_boundary(copy, ["xsht", "api", f"search:{terms}"])
        results.append(Result(f"xsht api search:{terms}", found.exit == 0 and "api: " in found.stdout, f"exit {found.exit}"))
    results += check_examples(guide, scratch)
    return results
