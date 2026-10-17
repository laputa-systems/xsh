from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import shlex
from typing import Callable, Sequence


_EXACT_SELECTOR = re.compile(
    r"core/tests/test-[a-z0-9-]+\.xsh::test_[A-Za-z0-9_]+"
)
_CASE_RESULT = re.compile(r"(?P<selector>\S+) \.\.\. ok (?P<duration>\S+)")
_SUMMARY = re.compile(
    r"test result: ok\. (?P<passed>\d+) passed; "
    r"(?P<failed>\d+) failed; (?P<skipped>\d+) skipped"
)


@dataclass(frozen=True)
class CaseSummary:
    selector: str
    passed: int
    failed: int
    skipped: int


Container = Callable[..., bytes]


def _validate_selectors(
    selectors: Sequence[str], known_selectors: Sequence[str]
) -> tuple[str, ...]:
    selected = tuple(selectors)
    if not 1 <= len(selected) <= 4:
        raise ValueError("an exact native batch must contain one to four selectors")
    if any(not isinstance(selector, str) or not _EXACT_SELECTOR.fullmatch(selector) for selector in selected):
        raise ValueError("invalid exact native selector")
    if len(set(selected)) != len(selected):
        raise ValueError("exact native selectors must be distinct")
    known = frozenset(known_selectors)
    unknown = tuple(selector for selector in selected if selector not in known)
    if unknown:
        raise ValueError(f"unrecognized exact native selector: {unknown[0]}")
    return selected


def _parse_case(selector: str, lines: Sequence[str]) -> CaseSummary:
    if len(lines) != 3 or lines[0] != "running 1 tests":
        raise ValueError(f"{selector}: native runner did not select exactly one test")
    case = _CASE_RESULT.fullmatch(lines[1])
    if case is None or case.group("selector") != selector:
        raise ValueError(f"{selector}: native runner did not report the exact selected case")
    summary = _SUMMARY.fullmatch(lines[2])
    if summary is None:
        raise ValueError(f"{selector}: native runner omitted one exact final summary")
    counts = tuple(int(summary.group(name)) for name in ("passed", "failed", "skipped"))
    if counts != (1, 0, 0):
        raise ValueError(f"{selector}: native case did not pass without skips: {lines[2]}")
    return CaseSummary(selector=selector, passed=counts[0], failed=counts[1], skipped=counts[2])


def _parse_batch(selectors: Sequence[str], output: bytes) -> tuple[CaseSummary, ...]:
    try:
        lines = output.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError("native batch output was not UTF-8") from error

    summaries = []
    cursor = 0
    for selector in selectors:
        begin = f"XSH-EXACT-BEGIN:{selector}"
        end = f"XSH-EXACT-END:{selector}"
        if cursor >= len(lines) or lines[cursor] != begin:
            raise ValueError(f"{selector}: native batch omitted or reordered its begin boundary")
        case_lines = lines[cursor + 1 : cursor + 4]
        summaries.append(_parse_case(selector, case_lines))
        end_index = cursor + 4
        if end_index >= len(lines) or lines[end_index] != end:
            raise ValueError(f"{selector}: native batch omitted or duplicated its end boundary")
        cursor = end_index + 1
    if cursor != len(lines):
        raise ValueError("native batch returned output outside its exact selector boundaries")
    return tuple(summaries)


def _script(selectors: Sequence[str]) -> str:
    commands = ["set -e", "cd /work"]
    for selector in selectors:
        commands.append("printf '%s\\n' " + shlex.quote(f"XSH-EXACT-BEGIN:{selector}"))
        commands.append(
            shlex.join(("/products/xsht", "test", "--exact", selector)) + " 2>&1"
        )
        commands.append("printf '%s\\n' " + shlex.quote(f"XSH-EXACT-END:{selector}"))
    return "\n".join(commands)


def run_exact_batch(
    selectors: Sequence[str],
    *,
    known_selectors: Sequence[str],
    root: Path,
    products: Path,
    container: Container,
) -> tuple[CaseSummary, ...]:
    selected = _validate_selectors(selectors, known_selectors)
    output = container(
        _script(selected),
        mounts=[
            f"type=bind,source={root},target=/work,readonly",
            f"type=bind,source={products},target=/products,readonly",
        ],
        capabilities=[],
        timeout=30,
    )
    return _parse_batch(selected, output)
