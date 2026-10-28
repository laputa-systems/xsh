#!/usr/bin/env python3
"""Measure how many inventory annotation sites a compiler lets a source tree drop.

Each site is erased alone, then every site that passed alone is erased together.
Nothing is recorded: the summary goes to standard output and the scratch trees,
built from hard links, are removed on exit.
"""
import argparse
import collections
import csv
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor

DETAILED = ("check.local-inference", "compact.indexed-build")
NAMED_METHOD = ("check.ambiguous-overload",)
ERROR_BLOCK = re.compile(r"^err\[([a-z.-]+)\]: (.*)(?:\n {2}[^\n]*\n( {2}[^\n]*)\n( {2}[^\n]*))?$", re.MULTILINE)
CALLED_METHOD = re.compile(r"\.(\w+)\(")


def read_sites(inventory):
    with open(inventory, newline="", encoding="utf-8") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


def link_tree(source, destination):
    subprocess.run(["cp", "-al", str(source), str(destination)], check=True)


def replace_file(path, data):
    os.unlink(path)
    path.write_bytes(data)


def check(xsht, root, path, timeout):
    try:
        result = subprocess.run([xsht, "check", path], cwd=root, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return "timeout", []
    errors = [describe(*block) for block in ERROR_BLOCK.findall(result.stderr.decode(errors="replace"))]
    return ("pass" if result.returncode == 0 else "fail"), errors


def describe(code, message, source_line, caret_line):
    if code in DETAILED:
        return f"{code}: {message[:90]}"
    if code in NAMED_METHOD:
        start = caret_line.find("^")
        called = CALLED_METHOD.search(source_line[start:start + caret_line.count("^")]) if start >= 0 else None
        return f"{code}: {called.group(1)}" if called else code
    return code


def erase(data, edits):
    for start, end in sorted(edits, reverse=True):
        data = data[:start] + data[end:]
    return data


def measure(sites, roots, xsht, scratch, jobs, timeout):
    originals = {}
    for site in sites:
        key = (site["project"], site["path"])
        originals.setdefault(key, (roots[site["project"]] / site["path"]).read_bytes())
    workers = []
    for index in range(jobs):
        worker = {}
        for project, root in roots.items():
            copy = scratch / f"worker-{index}" / project
            copy.parent.mkdir(parents=True, exist_ok=True)
            link_tree(root, copy)
            worker[project] = copy
        workers.append(worker)
    free = list(range(jobs))
    lock = threading.Lock()

    def single(site):
        with lock:
            index = free.pop()
        try:
            root = workers[index][site["project"]]
            path = root / site["path"]
            data = originals[(site["project"], site["path"])]
            edit = (int(site["edit_start"]), int(site["edit_end"]))
            replace_file(path, erase(data, [edit]))
            try:
                status, codes = check(xsht, root, site["path"], timeout)
            finally:
                replace_file(path, data)
            return site, status, codes
        finally:
            with lock:
                free.append(index)

    with ThreadPoolExecutor(jobs) as pool:
        singles = list(pool.map(single, sites))
    passing = [site for site, status, _ in singles if status == "pass"]
    joint = {}
    root_paths = sorted({(site["project"], site["path"]) for site in passing})
    for project, relative in root_paths:
        root = workers[0][project]
        path = root / relative
        data = originals[(project, relative)]
        edits = [(int(site["edit_start"]), int(site["edit_end"])) for site in passing
                 if (site["project"], site["path"]) == (project, relative)]
        replace_file(path, erase(data, edits))
        try:
            joint[(project, relative)] = (len(edits), *check(xsht, root, relative, timeout))
        finally:
            replace_file(path, data)
    return singles, joint


def summarize(singles, joint, out):
    totals = collections.Counter()
    passed = collections.Counter()
    causes = collections.Counter()
    for site, status, codes in singles:
        key = (site["kind"], site["declaration_kind"], site["visibility"])
        totals[key] += 1
        if status == "pass":
            passed[key] += 1
        else:
            causes[(site["kind"], codes[0] if codes else status)] += 1
    print(f"single-site erasure: {sum(passed.values())}/{sum(totals.values())} pass", file=out)
    for key in sorted(totals):
        print(f"  {passed[key]}/{totals[key]} {' '.join(key)}", file=out)
    print("failing causes:", file=out)
    for (kind, code), count in sorted(causes.items(), key=lambda item: (-item[1], item[0])):
        print(f"  {count} {kind} {code}", file=out)
    print("joint erasure of every site that passed alone:", file=out)
    for (project, path), (count, status, codes) in sorted(joint.items()):
        detail = dict(collections.Counter(codes)) if codes else ""
        print(f"  {project}/{path}: {count} sites, {status} {detail}".rstrip(), file=out)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inventory", type=Path)
    parser.add_argument("--root", action="append", required=True, metavar="PROJECT=DIR")
    parser.add_argument("--xsht", required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--kind", action="append", help="measure only these site kinds")
    args = parser.parse_args(argv)
    roots = {}
    for entry in args.root:
        project, _, directory = entry.partition("=")
        roots[project] = Path(directory).resolve()
    sites = read_sites(args.inventory)
    if args.kind:
        sites = [site for site in sites if site["kind"] in args.kind]
    args.scratch.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="erase-sites-", dir=args.scratch))
    try:
        singles, joint = measure(sites, roots, args.xsht, scratch, args.jobs, args.timeout)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    summarize(singles, joint, sys.stdout)


if __name__ == "__main__":
    main()
