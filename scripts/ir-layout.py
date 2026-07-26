#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import time
from pathlib import Path


IR_TYPES = (
    "source::SourceMap",
    "syntax::token::TokenTable",
    "syntax::cst::LazyCst",
    "syntax::cst::SyntaxTree",
    "syntax::arena::ArenaProgramBuilder<'_>",
    "syntax::arena::ArenaProgram",
    "syntax::arena::AstArena",
    "syntax::arena::ArenaStmtTag",
    "syntax::arena::ArenaStmtData",
    "syntax::arena::ArenaExprTag",
    "syntax::arena::ArenaExprData",
    "syntax::arena::ArenaTypeExprTag",
    "syntax::arena::ArenaTypeExprData",
    "sema::check::CheckOutput",
    "sema::types::Type",
    "runtime::eval::LoweredValue",
    "runtime::value::RecordMap",
    "runtime::value::RecordShapeData",
    "runtime::value::Value",
    "runtime::eval::Evaluator",
    "runtime::eval::lower::CompactLowerConstructProbe<'_, '_>",
    "runtime::eval::CompactLowerConstructProbeOutput",
    "runtime::eval::FrontendLoweredStats",
    "runtime::eval::BuildExprId",
    "runtime::eval::BuildStmtId",
    "runtime::eval::BuildPatternId",
    "runtime::eval::BuildIntId",
    "runtime::eval::BuildBoolId",
    "runtime::eval::BuildTopStmtId",
    "runtime::eval::IndexedBuildScratch",
    "runtime::eval::IndexedFunctionBuild",
    "runtime::eval::IndexedProgramBuild",
    "runtime::eval::indexed::IrData",
    "runtime::eval::indexed::IrRange",
    "runtime::eval::indexed::IrLocation",
    "runtime::eval::indexed::TypeId",
    "runtime::eval::indexed::SignatureId",
    "runtime::eval::indexed::ShapeId",
    "runtime::eval::indexed::semantic::TypeTag",
    "runtime::eval::indexed::semantic::SemanticPools",
    "runtime::eval::indexed::full::FullTag",
    "runtime::eval::indexed::full::FullPatternTag",
    "runtime::eval::indexed::full::FullStageTag",
    "runtime::eval::indexed::full::FullValueTag",
    "runtime::eval::indexed::full::FullDriverTag",
    "runtime::eval::indexed::full::FullDriverStep",
    "runtime::eval::indexed::full::FullDriverSlot",
    "runtime::eval::indexed::full::FullDriverSync",
    "runtime::eval::indexed::full::FullDriverRegion",
    "runtime::eval::indexed::full::FullDriverProgram",
    "runtime::eval::indexed::full::FullBlock",
    "runtime::eval::indexed::full::FullFunction",
    "runtime::eval::indexed::full::FullParam",
    "runtime::eval::indexed::full::FullParamCold",
    "runtime::eval::indexed::full::FullCapture",
    "runtime::eval::indexed::full::FullFunctionMetadata",
    "runtime::eval::indexed::full::FullValidation",
    "runtime::eval::indexed::full::FullStore",
    "runtime::eval::indexed::full::FullProgram",
)
HEADER_RE = re.compile(
    r"^print-type-size type: `([^`]+)`: ([0-9]+) bytes, alignment: ([0-9]+) bytes$"
)


def type_blocks(output: str) -> dict[str, list[str]]:
    blocks: dict[str, list[str]] = {}
    current: list[str] | None = None
    for line in output.splitlines():
        match = HEADER_RE.match(line)
        if match:
            current = [line]
            blocks[match.group(1)] = current
        elif current is not None and line.startswith("print-type-size "):
            current.append(line)
        else:
            current = None
    return blocks


def resolve_type(name: str) -> str:
    matches = []
    for ty in IR_TYPES:
        short = ty.rsplit("::", 1)[-1]
        if ty == name or short == name or short.split("<", 1)[0] == name:
            matches.append(ty)
    if len(matches) == 1:
        return matches[0]
    choices = ", ".join(ty.rsplit("::", 1)[-1] for ty in IR_TYPES)
    raise ValueError(f"unknown or ambiguous type {name!r}; choose one of: {choices}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Report compiler layouts for the hot frontend, lowered-IR, "
            "and evaluator types"
        )
    )
    parser.add_argument(
        "--details",
        "--only",
        action="append",
        default=None,
        metavar="TYPE",
        help=(
            "limit variant and field layouts to TYPE; may be repeated "
            "(the default reports every tracked type)"
        ),
    )
    args = parser.parse_args()

    try:
        details = (
            set(IR_TYPES)
            if args.details is None
            else {resolve_type(name) for name in args.details}
        )
    except ValueError as error:
        parser.error(str(error))

    root = Path(__file__).resolve().parents[1]
    metadata = f"xsh-ir-layout-{os.getpid()}-{time.time_ns()}"
    completed = subprocess.run(
        [
            "cargo",
            "rustc",
            "--lib",
            "--release",
            "--",
            "-Zprint-type-sizes",
            f"-Cmetadata={metadata}",
        ],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if completed.returncode:
        print(completed.stdout, end="", file=sys.stderr)
        return completed.returncode

    blocks = type_blocks(completed.stdout)
    missing = [ty for ty in IR_TYPES if ty not in blocks]
    if missing:
        print(
            "rustc did not report expected types: " + ", ".join(missing),
            file=sys.stderr,
        )
        return 1

    print("type\tbytes\talignment")
    for ty in IR_TYPES:
        match = HEADER_RE.match(blocks[ty][0])
        assert match is not None
        print(f"{ty}\t{match.group(2)}\t{match.group(3)}")

    for ty in IR_TYPES:
        if ty in details:
            print()
            print("\n".join(blocks[ty]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
