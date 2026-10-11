#!/usr/bin/env python3
"""Project installed GNU libc locale metadata into byte-preserving runtime data."""

import argparse
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "src/modules/locale/data.json"
FIELDS = (b"decimal_point", b"thousands_sep", b"abmon")


def oracle(image, *command, locale=None):
    argv = ["docker", "run", "--rm"]
    if locale is not None:
        argv += ["-e", "LC_ALL=" + locale]
    result = subprocess.run(argv + [image, *command], capture_output=True, timeout=30, check=True)
    if result.stderr:
        raise RuntimeError(f"oracle diagnostic for {command!r}: {result.stderr!r}")
    return result.stdout


def query(name, keyed, raw):
    values = []
    for field, line in zip(FIELDS, keyed.splitlines(), strict=True):
        prefix = field + b'="'
        if not line.startswith(prefix) or not line.endswith(b'"'):
            raise RuntimeError(f"unexpected quoted locale field: {line!r}")
        value = line[len(prefix):-1]
        if any(byte in value for byte in (b'"', b'\\', b'\x00', b'\n', b'\r')):
            raise RuntimeError(f"unsupported locale quoting: {line!r}")
        values.append(value)
    if raw != b"\n".join(values) + b"\n":
        raise RuntimeError(f"keyed/unkeyed metadata disagree for {name}")
    months = values[2].split(b";")
    if len(months) != 12 or any(not month for month in months) or not values[0]:
        raise RuntimeError(f"incomplete locale metadata for {name}")
    return {
        "decimal_point": list(values[0]),
        "thousands_separator": list(values[1]),
        "abbreviated_months": [list(month) for month in months],
    }


def aliases_for(name):
    if name.endswith(".utf8"):
        stem = name[:-5]
        return [stem + suffix for suffix in (".UTF-8", ".utf-8", ".UTF8")]
    if name.endswith(".iso88591"):
        stem = name[:-9]
        return [stem + suffix for suffix in (".ISO-8859-1", ".iso-8859-1", ".ISO8859-1", ".ISO88591")]
    if name.endswith(".gb18030"):
        return [name[:-8] + ".GB18030"]
    return []


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="xsh-oracle-gnu-9.12")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    names = oracle(args.image, "locale", "-a").decode("ascii").splitlines()
    if names != sorted(set(names)):
        raise RuntimeError("locale inventory must be unique and sorted")
    aliases = {alias: name for name in names for alias in aliases_for(name) if alias not in names}
    all_names = names + list(aliases)
    script = 'for name do LC_ALL="$name" locale -k decimal_point thousands_sep abmon; printf "\\0"; LC_ALL="$name" locale decimal_point thousands_sep abmon; printf "\\0"; done'
    fields = oracle(args.image, "sh", "-c", script, "sh", *all_names).split(b"\0")
    if fields[-1] or len(fields) != 2 * len(all_names) + 1:
        raise RuntimeError("invalid framed locale projection")
    queried = {name: query(name, fields[2 * i], fields[2 * i + 1]) for i, name in enumerate(all_names)}
    locales = {name: queried[name] for name in names}
    for alias, name in aliases.items():
        if queried[alias] != locales[name]:
            raise RuntimeError(f"alias {alias} differs from {name}")
    notes = oracle(args.image, "readelf", "-n", "/lib/x86_64-linux-gnu/libc.so.6").decode("ascii")
    build_id = re.search(r"Build ID: ([0-9a-f]+)", notes)
    if build_id is None:
        raise RuntimeError("libc build ID missing")
    coreutils = oracle(args.image, "sort", "--version").decode().splitlines()[0]
    if coreutils != "sort (GNU coreutils) 9.12":
        raise RuntimeError(f"unexpected GNU oracle: {coreutils}")
    projection = {
        "provenance": {
            "image": args.image,
            "glibc": oracle(args.image, "locale", "--version").decode().splitlines()[0],
            "libc_build_id": build_id.group(1),
            "coreutils": coreutils,
            "query": "locale -k decimal_point thousands_sep abmon; independently checked against unkeyed output",
        },
        "locales": locales,
        "aliases": dict(sorted(aliases.items())),
    }
    rendered = json.dumps(projection, ensure_ascii=True, indent=2) + "\n"
    if args.check:
        if OUTPUT.read_text() != rendered:
            raise RuntimeError("committed locale metadata differs from oracle projection")
    else:
        OUTPUT.write_text(rendered)
    print(f"Verified {len(locales)} locale records and {len(aliases)} exact aliases")


if __name__ == "__main__":
    main()
