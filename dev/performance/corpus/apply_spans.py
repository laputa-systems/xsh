import argparse
import csv
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
ALLOWED_PATHS = {
    "xsh": {"core/wc.xsh", "core/sort.xsh", "core/awk.xsh"},
    "laputa": {"pm/generation.xsh"},
}


def under_work_tmp(path):
    try:
        path.resolve().relative_to((ROOT / ".work/tmp").resolve())
    except ValueError:
        return False
    return True


def unescape(value):
    output = []
    index = 0
    escapes = {"\\": "\\", "t": "\t", "r": "\r", "n": "\n"}
    while index < len(value):
        if value[index] != "\\":
            output.append(value[index])
            index += 1
            continue
        index += 1
        if index == len(value) or value[index] not in escapes:
            raise ValueError("inventory contains an invalid escaped field")
        output.append(escapes[value[index]])
        index += 1
    return "".join(output)


def read_sites(inventory):
    with inventory.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    required = {
        "site",
        "project",
        "source_commit",
        "path",
        "kind",
        "declaration_kind",
        "visibility",
        "initializer",
        "line",
        "owner",
        "edit_start",
        "edit_end",
        "edit",
    }
    if not rows or not required.issubset(rows[0]):
        raise ValueError("inventory has no rows or is missing required columns")
    sites = {}
    for row in rows:
        site = int(row["site"])
        if site in sites:
            raise ValueError(f"duplicate inventory site {site}")
        sites[site] = row
    return sites


def select_sites(value, sites):
    selected = []
    for selector in value.split(","):
        if selector.isdecimal():
            selected.append(int(selector))
            continue
        fields = selector.split("|", 4)
        if len(fields) != 5:
            raise SystemExit(
                "site selectors must be ids or project|path|kind|line|owner"
            )
        project, path, kind, line, owner = fields
        matches = [
            int(site)
            for site, row in sites.items()
            if row["project"] == project
            and row["path"] == path
            and row["kind"] == kind
            and row["line"] == line
            and unescape(row["owner"]) == owner
        ]
        if len(matches) != 1:
            raise SystemExit(
                f"site selector matched {len(matches)} rows: {selector}"
            )
        selected.extend(matches)
    if not selected or len(selected) != len(set(selected)):
        raise SystemExit("select at least one unique inventory site")
    return selected


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("inventory", type=Path)
    parser.add_argument(
        "site_ids",
        help="comma-separated inventory ids or exact project|path|kind|line|owner selectors",
    )
    parser.add_argument("xsh_revision")
    parser.add_argument("laputa_revision")
    parser.add_argument("xsh_root", type=Path)
    parser.add_argument("laputa_root", type=Path)
    args = parser.parse_args()

    inventory = args.inventory.resolve()
    xsh_root = args.xsh_root.resolve()
    laputa_root = args.laputa_root.resolve()
    if not under_work_tmp(inventory) or not under_work_tmp(xsh_root) or not under_work_tmp(laputa_root):
        raise SystemExit("inventory and reduced roots must stay under this checkout's .work/tmp")
    if xsh_root == laputa_root or xsh_root in laputa_root.parents or laputa_root in xsh_root.parents:
        raise SystemExit("XSH and Laputa reduced roots must be separate directories")

    sites = read_sites(inventory)
    selected = select_sites(args.site_ids, sites)
    missing = sorted(set(selected) - set(sites))
    if missing:
        raise SystemExit(f"inventory site ids do not exist: {missing}")

    roots = {"xsh": xsh_root, "laputa": laputa_root}
    revisions = {"xsh": args.xsh_revision, "laputa": args.laputa_revision}
    by_file = {}
    for site in selected:
        row = sites[site]
        project = row["project"]
        if project not in roots or row["path"] not in ALLOWED_PATHS[project]:
            raise SystemExit(f"site {site} is outside the four-file practical corpus")
        if row["source_commit"] != revisions[project]:
            raise SystemExit(f"site {site} has a different pinned source revision")
        if row["visibility"] != "private" or row["kind"] not in {"local", "parameter", "return"}:
            raise SystemExit(f"site {site} is not a private local or function type annotation")
        if row["declaration_kind"] not in {"local", "pure"}:
            raise SystemExit(f"site {site} is not a local or pure function annotation")
        if row["kind"] == "local" and row["initializer"] in {"map-typed", "require-target"}:
            raise SystemExit(f"site {site} carries a retained Map or .require() contract")
        by_file.setdefault((project, row["path"]), []).append(row)

    for (project, relative), rows in by_file.items():
        path = roots[project] / relative
        if not path.resolve().is_relative_to(roots[project]):
            raise SystemExit(f"source path escapes reduced root: {relative}")
        content = path.read_bytes()
        edits = []
        for row in rows:
            start = int(row["edit_start"])
            end = int(row["edit_end"])
            if start < 0 or end < start or end > len(content):
                raise SystemExit(f"site {row['site']} has an invalid byte span")
            expected = unescape(row["edit"]).encode("utf-8")
            if content[start:end] != expected:
                raise SystemExit(f"site {row['site']} no longer matches its source bytes")
            edits.append((start, end, row["site"]))
        edits.sort()
        if any(previous[1] > current[0] for previous, current in zip(edits, edits[1:])):
            raise SystemExit(f"selected annotation spans overlap in {relative}")
        for start, end, _ in reversed(edits):
            content = content[:start] + content[end:]
        path.write_bytes(content)


if __name__ == "__main__":
    main()
