"""Generate byte-stable inputs and independently computed observable outputs."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat


def digest(path):
    with open(path, "rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def snapshot(root):
    result = {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs + files:
            path = Path(directory) / name
            info = path.lstat()
            relative = path.relative_to(root).as_posix()
            mode = stat.S_IMODE(info.st_mode)
            if stat.S_ISLNK(info.st_mode):
                result[relative] = ["link", mode, os.readlink(path)]
            elif stat.S_ISDIR(info.st_mode):
                result[relative] = ["dir", mode]
            elif stat.S_ISREG(info.st_mode):
                result[relative] = ["file", mode, digest(path)]
            else:
                raise ValueError(f"unsupported fixture kind: {path}")
    return dict(sorted(result.items()))


def generate(root):
    root.mkdir(parents=True, exist_ok=False)
    header = (
        b"#define CAP_CHOWN 0\n#define\tCAP_NET_ADMIN\t0007 \t\n"
        b"#define CAP_LAST_CAP (CAP_CHOWN)\n#define cap_bad 9\n"
        b" #define CAP_INDENTED 10\n#define CAP_TRAILING 11 // ignored\n"
        + (b"/* " + b"x" * 120 + b" */\n") * 8
    ) * 8192
    (root / "capability.h").write_bytes(header)
    tree = root / "tree"
    tree.mkdir()
    for directory in range(32):
        folder = tree / f"{'hidden' if directory % 2 else 'group'}-{directory:03}"
        if directory % 2:
            folder = tree / ("." + folder.name)
        folder.mkdir()
        for file in range(32):
            (folder / f"file {file:03} [x]*.txt").write_text("fixture\n", encoding="ascii")
        if directory % 4 == 0:
            (folder / ".nested").mkdir()
            (folder / ".nested" / "leaf").write_text("leaf\n", encoding="ascii")
    (tree / ".gitignore").write_text("*.txt\n", encoding="ascii")
    (tree / "link-directory").symlink_to("group-000", target_is_directory=True)
    (tree / "link-file").symlink_to("group-000/file 000 [x]*.txt")
    (tree / "link-dangling").symlink_to("missing")
    (tree / "link-parent").symlink_to(".", target_is_directory=True)
    rows = [{"name": f"pkg-{index:05}", "version": index} for index in range(2048)]
    rows.reverse()
    rows.insert(19, {"name": "pkg-00500", "version": -1})
    replacement = {"name": "pkg-00500", "version": 9000}
    addition = {"name": "pkg-added", "version": 1}
    for filename, value in (("index.json", rows), ("replacement.json", replacement), ("addition.json", addition)):
        (root / filename).write_text(json.dumps(value, separators=(",", ":")) + "\n", encoding="ascii")
    expected = root / "expected"
    expected.mkdir()
    (expected / "startup").write_bytes(b"ready\n")
    (expected / "spawn").write_bytes(b"".join(f"probe {index}\n".encode() for index in range(128)))
    pattern = re.compile(rb"^#define[ \t]+(CAP_[A-Z0-9_]+)[ \t]+([0-9]+)[ \t]*$")
    matches = [pattern.fullmatch(line) for line in header.split(b"\n")]
    (expected / "pipeline").write_bytes(b"".join(b'{"' + match[1].lower() + b'",' + match[2] + b'},\n' for match in matches if match))
    entries = snapshot(tree)
    (expected / "directory").write_text("".join(f"{path}\t{entry[0]}\n" for path, entry in entries.items()), encoding="ascii")
    updated = [replacement if row["name"] == replacement["name"] else row for row in rows] + [addition]
    updated.sort(key=lambda row: row["name"])
    (expected / "json").write_text(json.dumps(updated, separators=(",", ":")) + "\n", encoding="ascii")
    for directory, dirs, files in os.walk(root, followlinks=False):
        Path(directory).chmod(0o755)
        for name in files:
            path = Path(directory) / name
            if not path.is_symlink():
                path.chmod(0o644)
    manifest = snapshot(root)
    (root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n", encoding="ascii")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    generate(parser.parse_args().destination)
