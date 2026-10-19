import filecmp
import os
from pathlib import Path
import stat
import sys


def entries(root):
    root = root.resolve()
    found = {}
    for directory, directory_names, file_names in os.walk(root, followlinks=False):
        parent = Path(directory)
        for name in (*directory_names, *file_names):
            path = parent / name
            mode = path.lstat().st_mode
            if stat.S_ISLNK(mode):
                entry = ("symlink", os.readlink(path))
            elif stat.S_ISDIR(mode):
                entry = ("directory", stat.S_IMODE(mode))
            elif stat.S_ISREG(mode):
                entry = ("file", stat.S_IMODE(mode), path.stat().st_size)
            else:
                entry = ("other", stat.S_IFMT(mode), stat.S_IMODE(mode))
            found[path.relative_to(root)] = entry
    return found


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: compare_tree.py LEFT RIGHT")
    left = Path(sys.argv[1]).resolve()
    right = Path(sys.argv[2]).resolve()
    left_entries = entries(left)
    right_entries = entries(right)
    different = False
    for relative in sorted(left_entries.keys() | right_entries.keys()):
        left_entry = left_entries.get(relative)
        right_entry = right_entries.get(relative)
        if left_entry is None:
            print(f"extra path in right tree: {relative}", file=sys.stderr)
            different = True
        elif right_entry is None:
            print(f"missing path in right tree: {relative}", file=sys.stderr)
            different = True
        elif left_entry != right_entry:
            print(f"path metadata differs: {relative}", file=sys.stderr)
            different = True
        elif left_entry[0] == "file" and not filecmp.cmp(
            left / relative, right / relative, shallow=False
        ):
            print(f"file contents differ: {relative}", file=sys.stderr)
            different = True
    return int(different)


if __name__ == "__main__":
    raise SystemExit(main())
