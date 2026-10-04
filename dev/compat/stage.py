#!/usr/bin/env python3
"""Stage XSH core applets as real executables for the compatibility suites.

Layout written to STAGE (default: target/compat-stage):

    STAGE/bin/<applet>      core/<applet>.xsh with its shebang pointing at XSH_BIN
    STAGE/bin/lib/*.xsh     core/lib modules, adjacent so `use lib.x` resolves
    STAGE/bin/<alias>       symlink to its target applet (aliases.json)
    STAGE/mem-limit-kb      address-space cap (KiB) the adapter applies to each applet
    STAGE/applets.json      the deterministic applet manifest for this stage
    STAGE/xsh-uutests       the uutils multicall adapter; it finds the stage from its own
                            path because the uutils framework clears the environment
    STAGE/gnu-bin/          (with --gnu-programs) every GNU program name: a
                            symlink into bin/ when XSH provides it, otherwise a
                            copy of `false`, so a missing command fails instead
                            of silently falling through to the host's GNU copy

This mirrors `dev/release.xsh::package_core` (suffix dropped for applets, kept
for lib modules) so the suites exercise the installed shape, not core/ in the
source tree. Aliases are symlinks so the kernel passes the alias path to the
interpreter and the applet can select command-specific defaults from it.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import stat
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CORE = REPO / "core"
ALIASES = REPO / "dev" / "compat" / "aliases.json"


def applets() -> list[str]:
    return sorted(p.stem for p in CORE.glob("*.xsh"))


def install_script(src: Path, dst: Path, interpreter: str, mode: int) -> None:
    lines = src.read_bytes().split(b"\n", 1)
    body = lines[1] if len(lines) > 1 else b""
    if lines[0].startswith(b"#!"):
        data = b"#!" + interpreter.encode() + b"\n" + body
    else:
        data = src.read_bytes()
    dst.write_bytes(data)
    dst.chmod(mode)


def manifest(names: list[str], aliases: dict[str, str]) -> dict:
    return {
        "generated_by": "dev/compat/stage.py",
        "applets": [{"name": n, "source": f"core/{n}.xsh"} for n in names],
        "aliases": [{"name": a, "target": t} for a, t in sorted(aliases.items())],
        "libraries": sorted(f"lib/{p.name}" for p in (CORE / "lib").glob("*.xsh")),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stage", default=str(REPO / "target" / "compat-stage"))
    parser.add_argument("--xsh", default=os.environ.get("XSH_BIN", str(REPO / "target" / "release" / "xsh")))
    parser.add_argument("--gnu-programs", help="file listing GNU program names, one per line")
    args = parser.parse_args()

    xsh = Path(args.xsh).resolve()
    if not os.access(xsh, os.X_OK):
        print(f"XSH interpreter {xsh} is not executable; build it or set XSH_BIN", file=sys.stderr)
        return 2
    stage = Path(args.stage).resolve()
    if stage.exists():
        shutil.rmtree(stage)
    bin_dir = stage / "bin"
    (bin_dir / "lib").mkdir(parents=True)

    names = applets()
    aliases = {e["name"]: e["target"] for e in json.loads(ALIASES.read_text()).get("aliases", [])} if ALIASES.exists() else {}
    for name in names:
        install_script(CORE / f"{name}.xsh", bin_dir / name, str(xsh), 0o755)
    for lib in sorted((CORE / "lib").glob("*.xsh")):
        install_script(lib, bin_dir / "lib" / lib.name, str(xsh), 0o644)
    for alias, target in sorted(aliases.items()):
        if target not in names:
            print(f"alias {alias} targets missing applet {target}", file=sys.stderr)
            return 1
        if alias in names:
            print(f"alias {alias} collides with applet core/{alias}.xsh", file=sys.stderr)
            return 1
        (bin_dir / alias).symlink_to(target)

    adapter = stage / "xsh-uutests"
    shutil.copy(REPO / "dev" / "compat" / "xsh-uutests", adapter)
    adapter.chmod(0o755)
    (stage / "mem-limit-kb").write_text(os.environ.get("XSH_COMPAT_MEM_KB", "3145728") + "\n")
    (stage / "applets.json").write_text(json.dumps(manifest(names, aliases), indent=2) + "\n")

    if args.gnu_programs:
        gnu_dir = stage / "gnu-bin"
        gnu_dir.mkdir()
        false = shutil.which("false") or "/bin/false"
        provided = set(names) | set(aliases)
        missing = []
        for prog in Path(args.gnu_programs).read_text().split():
            dst = gnu_dir / prog
            if prog in provided:
                dst.symlink_to(bin_dir / prog)
            elif prog == "ginstall" and "install" in provided:
                dst.symlink_to(bin_dir / "install")
            else:
                shutil.copy(false, dst)
                dst.chmod(dst.stat().st_mode | stat.S_IXUSR)
                missing.append(prog)
        (stage / "gnu-missing.json").write_text(json.dumps(sorted(missing), indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
