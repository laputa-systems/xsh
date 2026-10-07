#!/usr/bin/env python3
"""Compare a pilot uutils option declaration with its XSH cli.applet schema."""

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
SUPPORTED_UTILITIES = ("cat",)


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


def _rust_options_module(source: str) -> dict[str, str]:
    module = re.search(r"\bmod\s+options\s*\{", source)
    if not module:
        raise SurfaceParseError("uutils options module is missing")
    opening = source.find("{", module.start())
    closing = _matching_delimiter(source, opening, "{", "}", "rust")
    body = source[opening + 1 : closing]
    return {
        name: value
        for name, value in re.findall(
            r"\b(?:pub\s+)?(?:static|const)\s+(\w+)\s*:\s*&str\s*=\s*\"([^\"]+)\"",
            body,
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


def parse_uutils_declarations(source: str) -> dict[str, dict[str, int | str]]:
    """Read Cat's literal Clap declarations, including generated help flags."""
    constants = _rust_options_module(source)
    signature = r"\bpub\s+fn\s+uu_app\s*\(\)"
    function = re.search(signature, source)
    if not function:
        raise SurfaceParseError(f"source is missing {signature!r}")
    body = _function_body(source, signature, "rust")
    function_open = source.find("{", function.end())
    function_close = _matching_delimiter(source, function_open, "{", "}", "rust")
    behavior = source[:function_open] + source[function_close + 1 :]
    declarations: dict[str, dict[str, int | str]] = {}

    aliases = r"\.(?:alias|aliases|short_alias|short_aliases|visible_alias|visible_aliases)\s*\("
    if re.search(aliases, body):
        raise SurfaceParseError(
            "Clap aliases need an explicit parser before this utility can be checked"
        )

    for argument in _rust_argument_blocks(body):
        if ".short(" not in argument and ".long(" not in argument:
            continue

        action = re.search(r"\.action\s*\(\s*ArgAction::(\w+)\s*\)", argument)
        if not action or action.group(1) not in ("SetTrue", "SetFalse", "Count"):
            raise SurfaceParseError("option argument has an unsupported or missing ArgAction")

        arg_id = re.search(r"Arg::new\s*\(\s*([^)]+)\s*\)", argument)
        if not arg_id:
            raise SurfaceParseError("option argument has no readable Arg ID")
        field = arg_id.group(1).removeprefix("options::")
        disposition = (
            "implemented"
            if arg_id.group(1).startswith("options::")
            and re.search(rf"\boptions::{re.escape(field)}\b", behavior)
            else "parsed-but-unused"
        )

        short = _rust_char_argument(argument, "short")
        if ".short(" in argument and short is None:
            raise SurfaceParseError("short option is not a literal character")
        long = re.search(r"\.long\s*\(\s*([^\s)]+)\s*\)", argument)
        if short:
            declarations[f"-{short}"] = {"arity": 0, "field": field, "disposition": disposition}
        if long:
            value = long.group(1)
            if value.startswith("options::"):
                name = value.removeprefix("options::")
                if name not in constants:
                    raise SurfaceParseError(f"unknown uutils option constant {name}")
                value = constants[name]
            elif value.startswith('"') and value.endswith('"'):
                value = value[1:-1]
            else:
                raise SurfaceParseError(f"unsupported Clap long option expression {value!r}")
            declarations[f"--{value}"] = {"arity": 0, "field": field, "disposition": disposition}

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
            if (
                index < len(tokens)
                and not tokens[index].startswith("-")
                and not tokens[index].startswith("...")
            ):
                arity = 1
                index += 1
        for spelling, _explicit_arity in aliases:
            entries.append((spelling, arity))
    return entries


def parse_xsh_declarations(source: str) -> dict[str, dict[str, int | str]]:
    """Read spellings, operand counts, and source-field use from cli.applet."""
    applet = re.search(r"\bcli\.applet\s*\(", source)
    if not applet:
        raise SurfaceParseError("XSH applet has no cli.applet call")
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
            disposition = "implemented" if re.search(field_use, source) else "parsed-but-unused"
            for spelling, arity in _form_entries(form.group(1), field_name):
                previous = declarations.get(spelling)
                if previous is not None and previous["arity"] != arity:
                    raise SurfaceParseError(f"{spelling} has conflicting argument counts")
                declarations[spelling] = {
                    "arity": arity,
                    "field": field_name,
                    "disposition": disposition,
                }

    return declarations


def parse_xsh_source(source: str) -> dict[str, int]:
    return {
        spelling: int(declaration["arity"])
        for spelling, declaration in parse_xsh_declarations(source).items()
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
        uutils_declarations = parse_uutils_declarations(
            (root / "src/uu/cat/src/cat.rs").read_text()
        )
        xsh_declarations = parse_xsh_declarations((REPO / "core/cat.xsh").read_text())
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
