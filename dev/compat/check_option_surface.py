#!/usr/bin/env python3
"""Compare pinned uutils option declarations with XSH applet declarations."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
LOCK = REPO / "dev" / "compat" / "upstream.lock.json"
SUPPORTED_UTILITIES = (
    "arch",
    "basename",
    "cat",
    "chroot",
    "comm",
    "dirname",
    "echo",
    "expand",
    "factor",
    "false",
    "fold",
    "fmt",
    "groups",
    "hostid",
    "hostname",
    "id",
    "join",
    "kill",
    "link",
    "logname",
    "mkdir",
    "more",
    "nice",
    "nl",
    "nohup",
    "nproc",
    "paste",
    "pathchk",
    "pinky",
    "printenv",
    "ptx",
    "pwd",
    "readlink",
    "realpath",
    "rmdir",
    "seq",
    "shuf",
    "sleep",
    "stat",
    "sum",
    "sync",
    "tac",
    "timeout",
    "true",
    "truncate",
    "tsort",
    "tty",
    "uname",
    "unexpand",
    "unlink",
    "uptime",
    "users",
    "wc",
    "whoami",
    "yes",
)
SOURCE_PATHS = {
    "arch": ("arch/src/arch.rs", "arch.xsh"),
    "basename": ("basename/src/basename.rs", "basename.xsh"),
    "cat": ("cat/src/cat.rs", "cat.xsh"),
    "chroot": ("chroot/src/chroot.rs", "chroot.xsh"),
    "comm": ("comm/src/comm.rs", "comm.xsh"),
    "dirname": ("dirname/src/dirname.rs", "dirname.xsh"),
    "echo": ("echo/src/echo.rs", "echo.xsh"),
    "expand": ("expand/src/expand.rs", "expand.xsh"),
    "factor": ("factor/src/factor.rs", "factor.xsh"),
    "false": ("false/src/false.rs", "false.xsh"),
    "fold": ("fold/src/fold.rs", "fold.xsh"),
    "fmt": ("fmt/src/fmt.rs", "fmt.xsh"),
    "groups": ("groups/src/groups.rs", "groups.xsh"),
    "hostid": ("hostid/src/hostid.rs", "hostid.xsh"),
    "hostname": ("hostname/src/hostname.rs", "hostname.xsh"),
    "id": ("id/src/id.rs", "id.xsh"),
    "join": ("join/src/join.rs", "join.xsh"),
    "kill": ("kill/src/kill.rs", "kill.xsh"),
    "link": ("link/src/link.rs", "link.xsh"),
    "logname": ("logname/src/logname.rs", "logname.xsh"),
    "mkdir": ("mkdir/src/mkdir.rs", "mkdir.xsh"),
    "more": ("more/src/more.rs", "more.xsh"),
    "nice": ("nice/src/nice.rs", "nice.xsh"),
    "nl": ("nl/src/nl.rs", "nl.xsh"),
    "nohup": ("nohup/src/nohup.rs", "nohup.xsh"),
    "nproc": ("nproc/src/nproc.rs", "nproc.xsh"),
    "paste": ("paste/src/paste.rs", "paste.xsh"),
    "pathchk": ("pathchk/src/pathchk.rs", "pathchk.xsh"),
    "pinky": ("pinky/src/pinky.rs", "pinky.xsh"),
    "printenv": ("printenv/src/printenv.rs", "printenv.xsh"),
    "ptx": ("ptx/src/ptx.rs", "ptx.xsh"),
    "pwd": ("pwd/src/pwd.rs", "pwd.xsh"),
    "readlink": ("readlink/src/readlink.rs", "readlink.xsh"),
    "realpath": ("realpath/src/realpath.rs", "realpath.xsh"),
    "rmdir": ("rmdir/src/rmdir.rs", "rmdir.xsh"),
    "seq": ("seq/src/seq.rs", "seq.xsh"),
    "shuf": ("shuf/src/shuf.rs", "shuf.xsh"),
    "sleep": ("sleep/src/sleep.rs", "sleep.xsh"),
    "stat": ("stat/src/stat.rs", "stat.xsh"),
    "sum": ("sum/src/sum.rs", "sum.xsh"),
    "sync": ("sync/src/sync.rs", "sync.xsh"),
    "tac": ("tac/src/tac.rs", "tac.xsh"),
    "timeout": ("timeout/src/timeout.rs", "timeout.xsh"),
    "true": ("true/src/true.rs", "true.xsh"),
    "truncate": ("truncate/src/truncate.rs", "truncate.xsh"),
    "tsort": ("tsort/src/tsort.rs", "tsort.xsh"),
    "tty": ("tty/src/tty.rs", "tty.xsh"),
    "uname": ("uname/src/uname.rs", "uname.xsh"),
    "unexpand": ("unexpand/src/unexpand.rs", "unexpand.xsh"),
    "unlink": ("unlink/src/unlink.rs", "unlink.xsh"),
    "uptime": ("uptime/src/uptime.rs", "uptime.xsh"),
    "users": ("users/src/users.rs", "users.xsh"),
    "wc": ("wc/src/wc.rs", "wc.xsh"),
    "whoami": ("whoami/src/whoami.rs", "whoami.xsh"),
    "yes": ("yes/src/yes.rs", "yes.xsh"),
}
# `sum` reads these algorithm selectors from raw byte argv instead of parser
# fields, so their source branches establish their disposition.
MANUAL_RAW_OPTION_USES = {
    "sum": {
        "-s": "if flag == 115",
        "--sysv": 'if arg == b"--sysv"',
    },
}
# Echo declares Clap metadata for help text but scans raw option bytes itself.
# These source branches distinguish implemented flags from parsed-only fields.
MANUAL_UUTILS_OPTION_USES = {
    "echo": {
        "options::NO_NEWLINE": "b'n' => options_.trailing_newline = false",
        "options::ENABLE_BACKSLASH_ESCAPE": "b'e' => options_.escape = true",
        "options::DISABLE_BACKSLASH_ESCAPE": "b'E' => options_.escape = false",
    },
}


class SurfaceParseError(ValueError):
    pass


def _matching_delimiter(source: str, start: int, opening: str, closing: str, language: str) -> int:
    depth = 0
    index = start
    line_comment = False
    block_comment = 0
    quoted = False
    escaped = False

    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""

        if line_comment:
            if char == "\n":
                line_comment = False
            index += 1
            continue

        if block_comment:
            if char == "/" and following == "*":
                block_comment += 1
                index += 2
            elif char == "*" and following == "/":
                block_comment -= 1
                index += 2
            else:
                index += 1
            continue

        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
            index += 1
            continue

        if language == "rust":
            if char == "/" and following == "/":
                line_comment = True
                index += 2
                continue
            if char == "/" and following == "*":
                block_comment = 1
                index += 2
                continue
            if char == "'":
                literal_end = index + 1
                if literal_end < len(source) and source[literal_end] == "\\":
                    literal_end += 2
                else:
                    literal_end += 1
                if literal_end < len(source) and source[literal_end] == "'":
                    index = literal_end + 1
                    continue
        elif char == "#":
            line_comment = True
            index += 1
            continue

        if char == '"':
            quoted = True
        elif char == opening:
            depth += 1
        elif char == closing:
            depth -= 1
            if depth == 0:
                return index
        index += 1

    raise SurfaceParseError(f"unclosed {opening!r} block")


def _option_entry(surface: dict[str, int], spelling: str, arity: int) -> None:
    previous = surface.get(spelling)
    if previous is not None and previous != arity:
        raise SurfaceParseError(f"{spelling} has conflicting argument counts")
    surface[spelling] = arity


def _rust_string_constants(source: str) -> dict[str, str]:
    return {
        name: value
        for name, value in re.findall(
            r"\b(?:pub\s+)?(?:static|const)\s+(\w+)\s*:\s*&str\s*=\s*\"([^\"]+)\"",
            source,
        )
    }


def _function_body(source: str, signature: str, language: str) -> str:
    match = re.search(signature, source)
    if not match:
        raise SurfaceParseError(f"source is missing {signature!r}")
    opening = source.find("{", match.end())
    if opening < 0:
        raise SurfaceParseError("function body is missing")
    closing = _matching_delimiter(source, opening, "{", "}", language)
    return source[opening + 1 : closing]


def _rust_argument_blocks(body: str) -> list[str]:
    blocks = []
    for match in re.finditer(r"\.arg\s*\(", body):
        opening = body.find("(", match.start())
        closing = _matching_delimiter(body, opening, "(", ")", "rust")
        blocks.append(body[opening + 1 : closing])
    return blocks


def _rust_char_argument(body: str, method: str) -> str | None:
    match = re.search(rf"\.{method}\s*\(\s*'([^'\\])'\s*\)", body)
    return match.group(1) if match else None


def parse_uutils_declarations(
    source: str, utility: str | None = None
) -> dict[str, dict[str, int | str]]:
    """Read literal Clap declarations, including generated help flags."""
    constants = _rust_string_constants(source)
    signature = r"\bpub\s+fn\s+uu_app\s*\(\)"
    function = re.search(signature, source)
    if not function:
        raise SurfaceParseError(f"source is missing {signature!r}")
    body = _function_body(source, signature, "rust")
    function_open = source.find("{", function.end())
    function_close = _matching_delimiter(source, function_open, "{", "}", "rust")
    behavior = source[:function_open] + source[function_close + 1 :]
    declarations: dict[str, dict[str, int | str]] = {}

    aliases = r"\.(?:aliases|short_aliases|visible_aliases|visible_short_alias|visible_short_aliases)\s*\("
    if re.search(aliases, body):
        raise SurfaceParseError(
            "this Clap alias form needs an explicit parser before the utility can be checked"
        )

    for argument in _rust_argument_blocks(body):
        if ".short(" not in argument and ".long(" not in argument:
            continue

        action = re.search(r"\.action\s*\(\s*ArgAction::(\w+)\s*\)", argument)
        action_name = action.group(1) if action else "Set"
        num_args = re.search(r"\.num_args\s*\(\s*(\d+)\s*\)", argument)
        if ".num_args(" in argument and not num_args:
            raise SurfaceParseError("variable or optional argument counts need an explicit parser")
        if action_name in ("SetTrue", "SetFalse", "Count", "Help", "Version"):
            arity = 0
            if num_args and num_args.group(1) != "0":
                raise SurfaceParseError("flag action declares value arguments")
        elif action_name in ("Set", "Append"):
            arity = int(num_args.group(1)) if num_args else 1
        else:
            raise SurfaceParseError(f"option argument has unsupported ArgAction::{action_name}")

        arg_id = re.search(r"Arg::new\s*\(\s*([^)]+)\s*\)", argument)
        if not arg_id:
            raise SurfaceParseError("option argument has no readable Arg ID")
        field = arg_id.group(1).removeprefix("options::")
        manual_use = MANUAL_UUTILS_OPTION_USES.get(utility, {}).get(arg_id.group(1))
        disposition = (
            "implemented"
            if action_name in ("Help", "Version")
            or re.search(rf"\b{re.escape(arg_id.group(1))}\b", behavior)
            or (manual_use is not None and manual_use in source)
            else "parsed-but-unused"
        )

        short = _rust_char_argument(argument, "short")
        if ".short(" in argument and short is None:
            raise SurfaceParseError("short option is not a literal character")
        short_aliases = re.findall(r"\.short_alias\s*\(\s*'([^'\\])'\s*\)", argument)
        if ".short_alias(" in argument and not short_aliases:
            raise SurfaceParseError("short alias is not a literal character")
        long = re.search(r"\.long\s*\(\s*([^\s)]+)\s*\)", argument)
        if short:
            declarations[f"-{short}"] = {"arity": arity, "field": field, "disposition": disposition}
        if long:
            value = long.group(1)
            if value.startswith("options::"):
                name = value.removeprefix("options::")
                if name not in constants:
                    raise SurfaceParseError(f"unknown uutils option constant {name}")
                value = constants[name]
            elif value in constants:
                value = constants[value]
            elif value.startswith('"') and value.endswith('"'):
                value = value[1:-1]
            else:
                raise SurfaceParseError(f"unsupported Clap long option expression {value!r}")
            declarations[f"--{value}"] = {"arity": arity, "field": field, "disposition": disposition}
        alias_calls = re.findall(r"\.(?:alias|visible_alias)\s*\(", argument)
        aliases = re.findall(r"\.(?:alias|visible_alias)\s*\(\s*\"([^\"]+)\"\s*\)", argument)
        if len(alias_calls) != len(aliases):
            raise SurfaceParseError("Clap alias is not a literal string")
        for alias in aliases:
            declarations[f"--{alias}"] = {"arity": arity, "field": field, "disposition": disposition}
        for alias in short_aliases:
            declarations[f"-{alias}"] = {"arity": arity, "field": field, "disposition": disposition}

    if ".disable_help_flag(true)" not in body:
        help_short = _rust_char_argument(body, "help_short")
        if ".help_short(" in body and help_short is None:
            raise SurfaceParseError("custom help short option is not a literal character")
        help_short = help_short or "h"
        generated = {"arity": 0, "field": "clap-generated", "disposition": "generated"}
        declarations[f"-{help_short}"] = generated.copy()
        declarations["--help"] = generated.copy()
    if re.search(r"\.version\s*\(", body) and ".disable_version_flag(true)" not in body:
        version_short = _rust_char_argument(body, "version_short")
        if ".version_short(" in body and version_short is None:
            raise SurfaceParseError("custom version short option is not a literal character")
        version_short = version_short or "V"
        generated = {"arity": 0, "field": "clap-generated", "disposition": "generated"}
        declarations[f"-{version_short}"] = generated.copy()
        declarations["--version"] = generated.copy()

    return declarations


def parse_uutils_source(source: str) -> dict[str, int]:
    return {
        spelling: int(declaration["arity"])
        for spelling, declaration in parse_uutils_declarations(source).items()
    }


def _form_entries(form: str, field: str) -> list[tuple[str, int]]:
    if "[" in form or "]" in form:
        raise SurfaceParseError(f"{field} uses an optional form that needs explicit arity handling")
    tokens = [token.rstrip(",") for token in form.split()]
    entries = []
    index = 0
    while index < len(tokens):
        if not tokens[index].startswith("-") or tokens[index] == "--":
            index += 1
            continue

        aliases: list[tuple[str, int | None]] = []
        while index < len(tokens) and tokens[index].startswith("-") and tokens[index] != "--":
            spelling, separator, _value = tokens[index].partition("=")
            aliases.append((spelling, int(bool(separator)) if separator else None))
            index += 1

        explicit_arities = {arity for _spelling, arity in aliases if arity is not None}
        if len(explicit_arities) > 1:
            raise SurfaceParseError(f"{field} declares aliases with different argument counts")
        if explicit_arities:
            arity = explicit_arities.pop()
        else:
            arity = 0
            while (
                index < len(tokens)
                and not tokens[index].startswith("-")
                and not tokens[index].startswith("...")
            ):
                arity += 1
                index += 1
        for spelling, _explicit_arity in aliases:
            entries.append((spelling, arity))
    return entries


def _manual_xsh_declarations(source: str, utility: str) -> dict[str, dict[str, int | str]]:
    if utility not in ("true", "false", "echo"):
        raise SurfaceParseError("XSH applet has no cli.applet schema")
    if not re.search(r"argv\.len\(\)\s*(?:==|!=)\s*1", source):
        raise SurfaceParseError(f"manual {utility} help/version guard is not recognized")
    spellings = set(re.findall(r'argv\s*\[\s*0\s*\]\s*==\s*"(--[A-Za-z0-9-]+)"', source))
    if spellings != {"--help", "--version"}:
        raise SurfaceParseError(f"manual {utility} option branches are not recognized")
    declarations = {
        spelling: {"arity": 0, "field": "manual", "disposition": "implemented"}
        for spelling in sorted(spellings)
    }

    if utility == "echo":
        markers = (
            'rx"^-[neE]+$"',
            'flag == "e"',
            'flag == "E"',
            "newline = false",
        )
        if not all(marker in source for marker in markers):
            raise SurfaceParseError("manual echo option scan is not recognized")
        declarations.update(
            {
                spelling: {"arity": 0, "field": "manual", "disposition": "implemented"}
                for spelling in ("-n", "-e", "-E")
            }
        )

    return declarations


def parse_xsh_declarations(
    source: str, utility: str | None = None
) -> dict[str, dict[str, int | str]]:
    """Read option forms from cli.applet or a narrowly supported manual parser."""
    applet = re.search(r"\bcli\.applet\s*\(", source)
    if not applet:
        if utility is None:
            raise SurfaceParseError("XSH applet has no cli.applet call")
        return _manual_xsh_declarations(source, utility)
    opening = source.find("{", applet.end())
    if opening < 0:
        raise SurfaceParseError("cli.applet schema is missing")
    closing = _matching_delimiter(source, opening, "{", "}", "xsh")
    schema = source[opening + 1 : closing]
    declarations: dict[str, dict[str, int | str]] = {}

    for field in re.finditer(r"\b([A-Za-z_]\w*)\s*:\s*\{", schema):
        field_opening = schema.find("{", field.start())
        field_closing = _matching_delimiter(schema, field_opening, "{", "}", "xsh")
        declaration = schema[field_opening + 1 : field_closing]
        form = re.search(r'\bform\s*:\s*"([^\"]*)"', declaration)
        if form:
            field_name = field.group(1)
            field_use = rf"\bopts\.{re.escape(field_name)}\b"
            manual_uses = MANUAL_RAW_OPTION_USES.get(utility, {})
            for spelling, arity in _form_entries(form.group(1), field_name):
                previous = declarations.get(spelling)
                if previous is not None and previous["arity"] != arity:
                    raise SurfaceParseError(f"{spelling} has conflicting argument counts")
                manual_use = manual_uses.get(spelling)
                if manual_use is not None and manual_use not in source:
                    raise SurfaceParseError(f"manual implementation of {spelling} is not recognized")
                disposition = (
                    "implemented"
                    if re.search(field_use, source) or manual_use is not None
                    else "parsed-but-unused"
                )
                declarations[spelling] = {
                    "arity": arity,
                    "field": field_name,
                    "disposition": disposition,
                }

    return declarations


def parse_xsh_source(source: str, utility: str | None = None) -> dict[str, int]:
    return {
        spelling: int(declaration["arity"])
        for spelling, declaration in parse_xsh_declarations(source, utility).items()
    }


def compare(reference: dict[str, int], xsh: dict[str, int]) -> dict[str, dict]:
    shared = reference.keys() & xsh.keys()
    return {
        "uutils_only": {key: reference[key] for key in sorted(reference.keys() - xsh.keys())},
        "xsh_only": {key: xsh[key] for key in sorted(xsh.keys() - reference.keys())},
        "arity_mismatches": {
            key: (reference[key], xsh[key])
            for key in sorted(shared)
            if reference[key] != xsh[key]
        },
    }


def _uutils_root() -> tuple[Path, str]:
    lock = json.loads(LOCK.read_text())
    wanted = lock["uutils"]["commit"]
    root = Path(os.environ.get("UUTILS_ROOT", REPO.parent / "ref" / "uutils-coreutils"))
    try:
        actual = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "HEAD"], text=True, stderr=subprocess.PIPE
        ).strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise SurfaceParseError(f"cannot read uutils checkout at {root}: {error}") from error
    if actual != wanted:
        raise SurfaceParseError(
            f"uutils checkout at {actual}, but upstream.lock.json pins {wanted}"
        )
    return root, wanted


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--util", choices=SUPPORTED_UTILITIES, required=True)
    args = parser.parse_args()

    try:
        root, revision = _uutils_root()
        uutils_source, xsh_source = SOURCE_PATHS[args.util]
        uutils_declarations = parse_uutils_declarations(
            (root / "src/uu" / uutils_source).read_text(), args.util
        )
        xsh_declarations = parse_xsh_declarations(
            (REPO / "core" / xsh_source).read_text(), args.util
        )
    except (OSError, SurfaceParseError) as error:
        print(f"option surface check: {error}", file=sys.stderr)
        return 2

    reference = {
        spelling: int(declaration["arity"])
        for spelling, declaration in uutils_declarations.items()
    }
    xsh = {
        spelling: int(declaration["arity"])
        for spelling, declaration in xsh_declarations.items()
    }
    result = compare(reference, xsh)
    print(
        f"{args.util}: pinned uutils {revision[:12]} has {len(reference)} option spellings; "
        f"XSH has {len(xsh)}"
    )
    for name, declarations in (("uutils", uutils_declarations), ("XSH", xsh_declarations)):
        unused = {
            spelling: declaration["field"]
            for spelling, declaration in declarations.items()
            if declaration["disposition"] == "parsed-but-unused"
        }
        if unused:
            print(f"{name} parsed-but-unused spellings:")
            for spelling, field in sorted(unused.items()):
                print(f"  {spelling}: field {field}")
    for key, title in (
        ("uutils_only", "uutils-only spellings"),
        ("xsh_only", "XSH-only spellings"),
        ("arity_mismatches", "argument-count mismatches"),
    ):
        values = result[key]
        if values:
            print(f"{title}:")
            for spelling, detail in values.items():
                print(f"  {spelling}: {detail}")
    if any(result.values()):
        return 1
    print("option surface matches")
    return 0


if __name__ == "__main__":
    sys.exit(main())
