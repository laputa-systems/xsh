import argparse
import csv
from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[3]
PROBES = {
    "xsh": {
        "core/wc.xsh": ["byte_count"],
        "core/sort.xsh": ["padded_decimal"],
        "core/awk.xsh": ["directive_operand", "newlines"],
    },
    "laputa": {
        "pm/generation.xsh": ["generation_sorted_unique", "generation_plan_dto"],
    },
}


def under_work_tmp(path):
    try:
        path.resolve().relative_to((ROOT / ".work/tmp").resolve())
    except ValueError:
        return False
    return True


def verify_pins(inventory, revisions):
    with inventory.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    if not rows:
        raise SystemExit("inventory has no source rows")
    for project, paths in PROBES.items():
        for path in paths:
            matches = [
                row
                for row in rows
                if row["project"] == project and row["path"] == path
            ]
            if not matches or any(row["source_commit"] != revisions[project] for row in matches):
                raise SystemExit(f"inventory source pin does not match {project}:{path}")


def append_probes(root, project):
    for relative, names in PROBES[project].items():
        path = root / relative
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root):
            raise SystemExit(f"probe source is missing or escapes its view: {relative}")
        source = path.read_bytes()
        text = source.decode("utf-8")
        for name in names:
            if not re.search(rf"^pure\s+{re.escape(name)}\s*\(", text, re.MULTILINE):
                raise SystemExit(f"probe declaration is missing: {relative}:{name}")
        probes = "\n".join(f"reveal_type({name})" for name in names).encode("utf-8")
        if any(f"reveal_type({name})".encode("utf-8") in source for name in names):
            raise SystemExit(f"probe already exists in {relative}")
        path.write_bytes(source + (b"" if source.endswith(b"\n") else b"\n") + probes + b"\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("inventory", type=Path)
    parser.add_argument("xsh_revision")
    parser.add_argument("laputa_revision")
    parser.add_argument("xsh_root", type=Path)
    parser.add_argument("laputa_root", type=Path)
    args = parser.parse_args()

    inventory = args.inventory.resolve()
    xsh_root = args.xsh_root.resolve()
    laputa_root = args.laputa_root.resolve()
    if not all(under_work_tmp(path) for path in (inventory, xsh_root, laputa_root)):
        raise SystemExit("inventory and probe roots must stay under this checkout's .work/tmp")
    if xsh_root == laputa_root or xsh_root in laputa_root.parents or laputa_root in xsh_root.parents:
        raise SystemExit("XSH and Laputa probe roots must be separate directories")
    verify_pins(inventory, {"xsh": args.xsh_revision, "laputa": args.laputa_revision})
    append_probes(xsh_root, "xsh")
    append_probes(laputa_root, "laputa")


if __name__ == "__main__":
    main()
