#!/usr/bin/env python3
"""Pack the clean smoke image's root filesystem as a tar for `ADD`.

Inputs are the tree `dev/compat/stage.py` wrote (STAGE/bin, STAGE/applets.json)
and the interpreter. The staged applets carry the shebang of whatever
interpreter path stage.py was given; the image installs that interpreter at
/bin/xsh, so each applet's first line is rewritten here and nothing else is
touched. Writing the tar directly (instead of COPY from a directory) fixes
ownership to root and keeps the sticky bit on /tmp, which COPY does not
promise.

Layout produced:

    /bin/xsh                 the static interpreter, the only file in /bin
    /usr/bin/<applet>        staged applets and alias symlinks
    /usr/bin/lib/*.xsh       library modules, adjacent so `use lib.x` resolves
    /etc/passwd, /etc/group  accounts so id, whoami and ls -l resolve names
    /tmp                     sticky and world-writable
    /smoke/smoke.xsh         the workflows
    /smoke/applets.json      the manifest the clean-image check compares against
"""

from __future__ import annotations

import argparse
import io
import sys
import tarfile
from pathlib import Path

PASSWD = (
    "root:x:0:0:root:/root:/bin/xsh\n"
    "smoke:x:1000:1000:smoke:/tmp:/bin/xsh\n"
)
GROUP = "root:x:0:\nsmoke:x:1000:\n"


def add_dir(tar: tarfile.TarFile, name: str, mode: int) -> None:
    info = tarfile.TarInfo(name)
    info.type = tarfile.DIRTYPE
    info.mode = mode
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    tar.addfile(info)


def add_file(tar: tarfile.TarFile, name: str, data: bytes, mode: int) -> None:
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = mode
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    tar.addfile(info, io.BytesIO(data))


def add_link(tar: tarfile.TarFile, name: str, target: str) -> None:
    info = tarfile.TarInfo(name)
    info.type = tarfile.SYMTYPE
    info.linkname = target
    info.mode = 0o777
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    tar.addfile(info)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stage", required=True, type=Path)
    parser.add_argument("--xsh", required=True, type=Path)
    parser.add_argument("--staged-shebang", required=True, help="interpreter path stage.py baked into the applets")
    parser.add_argument("--smoke", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    old = f"#!{args.staged_shebang} --\n".encode()
    new = b"#!/bin/xsh --\n"
    bin_dir = args.stage / "bin"

    with tarfile.open(args.out, "w", format=tarfile.GNU_FORMAT) as tar:
        for d in ("bin", "usr", "usr/bin", "usr/bin/lib", "etc", "smoke"):
            add_dir(tar, d, 0o755)
        add_dir(tar, "tmp", 0o1777)
        add_file(tar, "bin/xsh", args.xsh.read_bytes(), 0o755)

        for entry in sorted(bin_dir.iterdir()):
            if entry.is_symlink():
                add_link(tar, f"usr/bin/{entry.name}", str(entry.readlink()))
            elif entry.is_file():
                data = entry.read_bytes()
                if not data.startswith(old):
                    print(f"{entry}: first line is not {old!r}", file=sys.stderr)
                    return 1
                add_file(tar, f"usr/bin/{entry.name}", new + data[len(old):], 0o755)
        for lib in sorted((bin_dir / "lib").iterdir()):
            # Modules are imported, never executed, so most carry no shebang.
            data = lib.read_bytes()
            if data.startswith(old):
                data = new + data[len(old):]
            elif data.startswith(b"#!"):
                print(f"{lib}: unexpected interpreter line", file=sys.stderr)
                return 1
            add_file(tar, f"usr/bin/lib/{lib.name}", data, 0o644)

        add_file(tar, "etc/passwd", PASSWD.encode(), 0o644)
        add_file(tar, "etc/group", GROUP.encode(), 0o644)
        add_file(tar, "smoke/smoke.xsh", args.smoke.read_bytes(), 0o644)
        add_file(tar, "smoke/applets.json", (args.stage / "applets.json").read_bytes(), 0o644)
    return 0


if __name__ == "__main__":
    sys.exit(main())
