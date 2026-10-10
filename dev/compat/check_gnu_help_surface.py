#!/usr/bin/env python3
"""Option-surface check against GNU coreutils' own `--help`.

    check_gnu_help_surface.py UTIL... [--stage DIR] [--json]

For every UTIL the option spellings GNU coreutils prints in its `--help` (run in
the throwaway `xsh-oracle` container) are compared with the spellings the applet's
`cli.applet` schema declares (options declared `unsupported` count as handled). GNU-only spellings are
gaps; XSH-only spellings are extensions or help-text differences and are reported separately. Help text is one input, not
proof of semantics: this complements check_option_surface.py, which compares
uutils' clap declarations, and covers the utilities that checker cannot parse.

Exit status is 1 when any UTIL has a GNU-only spelling.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
OPTION = re.compile(r"(?<![\w-])(--[A-Za-z0-9][A-Za-z0-9-]*|-[A-Za-z0-9])(?![\w-])")
SECTION = re.compile(r"^=== (\S+)$")


def spellings(help_text: str) -> set[str]:
    """Option spellings that start an option line of a help text."""
    found: set[str] = set()
    for line in help_text.splitlines():
        stripped = line.strip()
        if not stripped.startswith("-"):
            continue
        head = re.split(r"\s{2,}", stripped, maxsplit=1)[0]
        for token in OPTION.findall(head):
            found.add(token)
    return found


ARG_FORM = re.compile(r"(--[A-Za-z0-9][A-Za-z0-9-]*)(\[?=)")
SHORT_ARG = re.compile(r"(?<![\w-])(-[A-Za-z0-9])\s+[A-Z_]{2,}")


def takes_value(help_text: str) -> set[str]:
    """Spellings whose help line shows a value (`--opt=ARG`, `-x ARG`)."""
    values: set[str] = set()
    for line in help_text.splitlines():
        stripped = line.strip()
        if not stripped.startswith("-"):
            continue
        head = re.split(r"\s{2,}", stripped, maxsplit=1)[0]
        values.update(match.group(1) for match in ARG_FORM.finditer(head))
        values.update(match.group(1) for match in SHORT_ARG.finditer(head))
    return values


REJECTION = re.compile(r"(unrecognized option|invalid option|unknown option|unknown argument|unexpected argument)", re.I)


def probe(util: str, spelling: str, with_value: bool, stage: Path) -> str:
    """How the applet answers one spelling: 'accepted', 'rejected' or 'timeout'.

    The applet runs with no operands in an empty scratch directory and a closed
    stdin, so a recognized option ends in a usage or operand diagnostic. An option
    declared unsupported answers with its own explicit diagnostic and counts as
    accepted; only the parser's unknown-option wording is a rejection.
    """
    binary = stage / "bin" / util
    if not binary.exists():
        return "missing"
    arg = spelling
    if with_value:
        arg = f"{spelling}=1" if spelling.startswith("--") else spelling + "1"
    with tempfile.TemporaryDirectory() as scratch:
        try:
            run = subprocess.run([str(binary), arg], cwd=scratch, capture_output=True,
                                 timeout=10, stdin=subprocess.DEVNULL,
                                 env={"PATH": "/usr/bin:/bin", "LC_ALL": "C", "HOME": scratch,
                                      "TMPDIR": scratch})
        except subprocess.TimeoutExpired:
            return "timeout"
    return "rejected" if REJECTION.search(run.stderr.decode("utf-8", "replace")) else "accepted"


def gnu_help(utils: list[str]) -> dict[str, str]:
    # `env` bypasses the shell builtins (printf, test) so GNU's own binary answers.
    script = "for u in " + " ".join(utils) + '; do echo "=== $u"; env "$u" --help 2>&1; done'
    run = subprocess.run(
        [str(REPO / "dev/compat/oracle.sh"), "--", "sh", "-c", script],
        capture_output=True, text=True, timeout=300,
    )
    sections: dict[str, list[str]] = {}
    current = None
    for line in run.stdout.splitlines():
        match = SECTION.match(line)
        if match:
            current = match.group(1)
            sections[current] = []
        elif current:
            sections[current].append(line)
    return {name: "\n".join(lines) for name, lines in sections.items()}


FORM = re.compile(r'form:\s*"([^"]+)"')
UNSUPPORTED = re.compile(r"unsupported:\s*\{([^}]*)\}")
KEY = re.compile(r'"(-{1,2}[A-Za-z0-9][\w-]*)"\s*:')


def xsh_spellings(util: str, stage: Path) -> tuple[set[str], str]:
    """Option spellings the applet's `cli.applet` schema declares.

    The applets' own `--help` text is a usage line; the schema is the contract,
    including options declared `unsupported`, which fail with an explicit
    diagnostic and so count as handled.
    """
    source = REPO / "core" / f"{util}.xsh"
    if not source.exists():
        return set(), f"core/{util}.xsh not found"
    text = source.read_text(errors="replace")
    found: set[str] = set()
    for form in FORM.findall(text):
        found.update(token for token in OPTION.findall(form))
    for block in UNSUPPORTED.findall(text):
        found.update(KEY.findall(block))
    note = "" if found else "no cli.applet forms found"
    return found, note


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("utils", nargs="+")
    parser.add_argument("--stage", type=Path, default=REPO / ".work/claude-campaign/stage-master")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    gnu = gnu_help(args.utils)
    report = {}
    failed = False
    for util in args.utils:
        theirs = spellings(gnu.get(util, ""))
        ours, ours_note = xsh_spellings(util, args.stage)
        values = takes_value(gnu.get(util, ""))
        # A declarative schema misses hand-scanned options: ask the applet about
        # every spelling the schema did not declare.
        probed = {name for name in theirs - ours
                  if probe(util, name, name in values, args.stage) in ("accepted", "timeout")}
        if probed:
            ours = ours | probed
            ours_note = ours_note or f"{len(probed)} spellings confirmed by probe"
        entry = {
            "gnu": len(theirs),
            "xsh": len(ours),
            "gnu_only": sorted(theirs - ours),
            "xsh_only": sorted(ours - theirs),
            "note": ours_note or ("" if theirs else "no help text from GNU"),
        }
        report[util] = entry
        failed = failed or bool(entry["gnu_only"])

    if args.json:
        json.dump(report, sys.stdout, indent=2)
        print()
    else:
        for util, entry in report.items():
            print(f"{util}: GNU {entry['gnu']} spellings, XSH {entry['xsh']}"
                  + (f" ({entry['note']})" if entry["note"] else ""))
            if entry["gnu_only"]:
                print("  GNU only:", " ".join(entry["gnu_only"]))
            if entry["xsh_only"]:
                print("  XSH only:", " ".join(entry["xsh_only"]))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
