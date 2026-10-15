#!/usr/bin/env python3
"""Strict parser and paired-result gate for raw `xsht test` output."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import re
from pathlib import Path


CASE_ID_RE = re.compile(r"^[^\s]+\.xsh::[^\s]+$")
FILE_ID_RE = re.compile(r"^[^\s]+\.xsh$")
SUMMARY_RE = re.compile(
    r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) skipped$"
)
RUNNING_RE = re.compile(r"^running (\d+) tests$")
DURATION_RE = re.compile(r"^(?:\d+(?:\.\d+)?(?:ns|us|µs|ms|s|m|h))+$")


@dataclass(frozen=True)
class Outcome:
    status: str
    skip_reason: str | None = None


@dataclass(frozen=True)
class RunValidation:
    log_path: str
    expected_count: int
    outcomes: dict[str, Outcome]
    missing_ids: tuple[str, ...]
    issues: tuple[str, ...]
    summary: tuple[int, int, int] | None
    counts: tuple[int, int, int]

    @property
    def accepted(self) -> bool:
        passed, failed, _skipped = self.counts
        return (
            not self.missing_ids
            and not self.issues
            and len(self.outcomes) == self.expected_count
            and failed == 0
            and passed + self.counts[2] == self.expected_count
        )

    def to_dict(self) -> dict:
        return {
            "log_path": self.log_path,
            "expected_count": self.expected_count,
            "terminal_count": len(self.outcomes),
            "counts": {
                "passed": self.counts[0],
                "failed_or_timed_out": self.counts[1],
                "skipped": self.counts[2],
            },
            "summary": self.summary,
            "missing_ids": list(self.missing_ids),
            "issues": list(self.issues),
            "accepted": self.accepted,
            "outcomes": {
                test_id: {
                    "status": outcome.status,
                    "skip_reason": outcome.skip_reason,
                }
                for test_id, outcome in sorted(self.outcomes.items())
            },
        }


def read_manifest(path: Path) -> list[str]:
    """Read the raw newline-delimited IDs emitted by `xsht test --list`."""
    try:
        text = path.read_bytes().decode("utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise ValueError(f"cannot read UTF-8 manifest {path}: {exc}") from exc
    ids = text.splitlines()
    if not ids:
        raise ValueError(f"empty test manifest: {path}")
    for row_number, test_id in enumerate(ids, 1):
        if FILE_ID_RE.fullmatch(test_id):
            raise ValueError(
                f"file-only preparation/error row at {path}:{row_number}: {test_id}"
            )
        if not CASE_ID_RE.fullmatch(test_id):
            raise ValueError(f"malformed test ID at {path}:{row_number}: {test_id!r}")
    if ids != sorted(ids):
        raise ValueError(f"test IDs are not sorted: {path}")
    if len(ids) != len(set(ids)):
        raise ValueError(f"duplicate test ID in manifest: {path}")
    return ids


def manifest_sha256(ids: list[str]) -> str:
    return hashlib.sha256(("\n".join(ids) + "\n").encode("utf-8")).hexdigest()


def _durationless_tail(text: str) -> str | None:
    head, separator, tail = text.rpartition(" ")
    if not separator or not DURATION_RE.fullmatch(tail):
        return None
    return head


def _classify_terminal(result: str) -> tuple[Outcome | None, str | None]:
    if result.startswith("ok "):
        if DURATION_RE.fullmatch(result[3:].strip()):
            return Outcome("ok"), None
        return None, f"malformed passing status: {result!r}"

    if result.startswith("FAILED "):
        if DURATION_RE.fullmatch(result[7:].strip()):
            return Outcome("failed"), None
        return None, f"malformed failure status: {result!r}"

    if result.startswith("TIMEOUT "):
        if DURATION_RE.fullmatch(result[8:].strip()):
            return Outcome("timeout"), None
        return None, f"malformed timeout status: {result!r}"

    if result.startswith("skipped:"):
        detail = result[len("skipped:") :].strip()
    elif result.startswith("skipped "):
        detail = result[len("skipped ") :].strip()
    else:
        return None, f"unrecognized terminal status: {result!r}"
    reason = _durationless_tail(detail)
    if reason is None or not reason.strip():
        return None, f"malformed skip status (reason and duration required): {result!r}"
    return Outcome("skipped", reason.strip()), None


def validate_log(path: Path, expected_ids: list[str]) -> RunValidation:
    """Require one well-formed, in-manifest terminal row per expected case."""
    if expected_ids != sorted(expected_ids) or len(expected_ids) != len(set(expected_ids)):
        raise ValueError("validator expected IDs must be unique and sorted")
    if not expected_ids or any(not CASE_ID_RE.fullmatch(item) for item in expected_ids):
        raise ValueError("validator expected IDs must be nonempty case IDs")

    try:
        lines = path.read_bytes().decode("utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as exc:
        raise ValueError(f"cannot read UTF-8 run log {path}: {exc}") from exc

    expected = set(expected_ids)
    outcomes: dict[str, Outcome] = {}
    issues: list[str] = []
    summaries: list[tuple[int, int, int]] = []
    running_counts: list[int] = []

    for line_number, line in enumerate(lines, 1):
        if not line:
            continue
        summary = SUMMARY_RE.fullmatch(line)
        if summary:
            summaries.append(tuple(int(value) for value in summary.groups()[1:]))
            continue
        running = RUNNING_RE.fullmatch(line)
        if running:
            running_counts.append(int(running.group(1)))
            continue

        test_id, separator, result = line.partition(" ... ")
        if not separator:
            issues.append(f"unexpected non-result log line {line_number}: {line!r}")
            continue
        if FILE_ID_RE.fullmatch(test_id):
            issues.append(f"file-only terminal row at line {line_number}: {test_id}")
            continue
        if not CASE_ID_RE.fullmatch(test_id):
            issues.append(f"malformed terminal ID at line {line_number}: {test_id!r}")
            continue
        if test_id not in expected:
            issues.append(f"out-of-manifest terminal ID at line {line_number}: {test_id}")
            continue
        if test_id in outcomes:
            issues.append(f"duplicate terminal result at line {line_number}: {test_id}")
            continue
        outcome, error = _classify_terminal(result)
        if error:
            issues.append(f"{test_id}: {error}")
            continue
        assert outcome is not None
        outcomes[test_id] = outcome

    if len(summaries) != 1:
        issues.append(f"expected one terminal summary, found {len(summaries)}")
    if len(running_counts) > 1:
        issues.append(f"expected at most one running-count header, found {len(running_counts)}")
    elif running_counts and running_counts[0] != len(expected_ids):
        issues.append(
            f"running-count header says {running_counts[0]}, manifest has {len(expected_ids)}"
        )

    missing = tuple(sorted(expected - outcomes.keys()))
    passed = sum(outcome.status == "ok" for outcome in outcomes.values())
    failed = sum(outcome.status in {"failed", "timeout"} for outcome in outcomes.values())
    skipped = sum(outcome.status == "skipped" for outcome in outcomes.values())
    counts = (passed, failed, skipped)
    summary_value = summaries[0] if len(summaries) == 1 else None
    if summary_value is not None and summary_value != counts:
        issues.append(f"terminal counts {counts} do not match summary {summary_value}")

    return RunValidation(
        log_path=str(path),
        expected_count=len(expected_ids),
        outcomes=outcomes,
        missing_ids=missing,
        issues=tuple(issues),
        summary=summary_value,
        counts=counts,
    )


def compare_pair(
    reference: RunValidation,
    candidate: RunValidation,
    reference_exit_code: int,
    candidate_exit_code: int,
) -> dict:
    """Reject failed/incomplete runs and differing statuses or skip reasons."""
    issues: list[str] = []
    if reference_exit_code != 0:
        issues.append(f"reference process exit code is {reference_exit_code}")
    if candidate_exit_code != 0:
        issues.append(f"candidate process exit code is {candidate_exit_code}")
    if not reference.accepted:
        issues.append("reference terminal log is incomplete or has failures/timeouts")
    if not candidate.accepted:
        issues.append("candidate terminal log is incomplete or has failures/timeouts")

    reference_map = reference.outcomes
    candidate_map = candidate.outcomes
    if set(reference_map) != set(candidate_map):
        issues.append("reference and candidate terminal ID sets differ")
    status_differences = []
    for test_id in sorted(set(reference_map) & set(candidate_map)):
        if reference_map[test_id] != candidate_map[test_id]:
            status_differences.append(
                {
                    "id": test_id,
                    "reference": {
                        "status": reference_map[test_id].status,
                        "skip_reason": reference_map[test_id].skip_reason,
                    },
                    "candidate": {
                        "status": candidate_map[test_id].status,
                        "skip_reason": candidate_map[test_id].skip_reason,
                    },
                }
            )
    if status_differences:
        issues.append(f"{len(status_differences)} status/skip-reason differences")

    return {
        "accepted": not issues,
        "issues": issues,
        "reference_exit_code": reference_exit_code,
        "candidate_exit_code": candidate_exit_code,
        "reference": reference.to_dict(),
        "candidate": candidate.to_dict(),
        "status_differences": status_differences,
    }
