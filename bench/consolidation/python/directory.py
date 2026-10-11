import os
from pathlib import Path
import stat
import sys

root = Path(sys.argv[1]) / "tree"
entries = []


def visit(directory):
    with os.scandir(directory) as children:
        for entry in children:
            mode = entry.stat(follow_symlinks=False).st_mode
            kind = "link" if stat.S_ISLNK(mode) else "dir" if stat.S_ISDIR(mode) else "file"
            entries.append((Path(entry.path).relative_to(root).as_posix(), kind))
            if kind == "dir":
                visit(entry.path)


visit(root)
for path, kind in sorted(entries):
    print(f"{path}\t{kind}")
