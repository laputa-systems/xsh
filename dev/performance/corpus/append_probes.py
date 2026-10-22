import argparse
import csv
from pathlib import Path
import re
import shutil


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


def _append_probes(root, project):
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


def prepare_signature_views(inventory, xsh_revision, laputa_revision, views, output):
    inventory = inventory.resolve()
    views = views.resolve()
    output = output.resolve()
    if not all(under_work_tmp(path) for path in (inventory, views, output)):
        raise SystemExit("inventory and signature views must stay under this checkout's .work/tmp")
    if output.exists() or output.is_symlink():
        raise SystemExit(f"signature view output already exists: {output}")
    verify_pins(inventory, {"xsh": xsh_revision, "laputa": laputa_revision})
    for view in ("original", "reduced"):
        for project in ("xsh", "laputa"):
            source = views / view / project
            if not source.is_dir():
                raise SystemExit(f"signature source view is missing: {source}")
            destination = output / view / project
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(source, destination, symlinks=True)
            _append_probes(destination, project)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("inventory", type=Path)
    parser.add_argument("xsh_revision")
    parser.add_argument("laputa_revision")
    parser.add_argument("views", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    prepare_signature_views(
        args.inventory,
        args.xsh_revision,
        args.laputa_revision,
        args.views,
        args.output,
    )


if __name__ == "__main__":
    main()
