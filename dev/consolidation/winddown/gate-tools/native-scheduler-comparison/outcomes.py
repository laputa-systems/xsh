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
FAILURE_DETAILS_HEADER = "failures:"
FAILURE_DETAIL_RE = re.compile(r"^---- (\S+) ----$")
RUNNER_ERROR_PREFIX = "xsht: "
TEST_FAILURE_STATUSES = frozenset({"failed", "timeout"})
RUN_SCRATCH_RE = re.compile(r"(xsh-test-\d+)-\d+-")
DURATION_RE = re.compile(r"^(?:\d+(?:\.\d+)?(?:ns|us|µs|ms|s|m|h))+$")


@dataclass(frozen=True)
class Outcome:
    status: str
    skip_reason: str | None = None

    def comparable(self) -> "Outcome":
        if self.skip_reason is None:
            return self
        return Outcome(self.status, RUN_SCRATCH_RE.sub(r"\1-RUN-", self.skip_reason))


@dataclass(frozen=True)
class RunValidation:
    log_path: str
    expected_count: int
    outcomes: dict[str, Outcome]
    missing_ids: tuple[str, ...]
    issues: tuple[str, ...]
    summary: tuple[int, int, int] | None
    counts: tuple[int, int, int]
    stray_line_count: int = 0
    prefixed_row_count: int = 0

    @property
    def complete(self) -> bool:
        return (
            not self.missing_ids
            and not self.issues
            and len(self.outcomes) == self.expected_count
            and sum(self.counts) == self.expected_count
        )

    @property
    def test_failures(self) -> tuple[str, ...]:
        return tuple(
            test_id
            for test_id, outcome in sorted(self.outcomes.items())
            if outcome.status in TEST_FAILURE_STATUSES
        )

    @property
    def accepted(self) -> bool:
        return self.complete and self.counts[1] == 0

    @property
    def expected_exit_code(self) -> int:
        return 1 if self.counts[1] else 0

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
            "stray_line_count": self.stray_line_count,
            "prefixed_row_count": self.prefixed_row_count,
            "complete": self.complete,
            "test_failures": [
                {"id": test_id, "status": self.outcomes[test_id].status}
                for test_id in self.test_failures
            ],
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
    return _validate_lines(path, _read_log(path), frozenset(expected_ids))


def validate_counted_log(path: Path) -> RunValidation:
    """Require one well-formed terminal row per case the run's own header counts."""
    return _validate_lines(path, _read_log(path), None)


def _read_log(path: Path) -> list[str]:
    try:
        return path.read_bytes().decode("utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as exc:
        raise ValueError(f"cannot read UTF-8 run log {path}: {exc}") from exc


def _validate_lines(path: Path, lines: list[str], expected: frozenset[str] | None) -> RunValidation:
    outcomes: dict[str, Outcome] = {}
    issues: list[str] = []
    summaries: list[tuple[int, int, int]] = []
    running_counts: list[int] = []
    failure_detail_ids: set[str] = set()
    in_failure_details = False
    detail_summary: tuple[int, int, int] | None = None
    detail_runner_errors: list[str] = []
    stray_lines = 0
    prefixed_rows = 0

    for line_number, line in enumerate(lines, 1):
        if not line:
            continue
        summary = SUMMARY_RE.fullmatch(line)
        if in_failure_details:
            if summary:
                detail_summary = tuple(int(value) for value in summary.groups()[1:])
                detail_runner_errors = []
            elif line.startswith(RUNNER_ERROR_PREFIX):
                detail_runner_errors.append(f"runner error at line {line_number}: {line!r}")
            elif detail := FAILURE_DETAIL_RE.fullmatch(line):
                failure_detail_ids.add(detail.group(1))
            continue
        if summary:
            summaries.append(tuple(int(value) for value in summary.groups()[1:]))
            continue
        if line.startswith(RUNNER_ERROR_PREFIX):
            issues.append(f"runner error at line {line_number}: {line!r}")
            continue
        if line == FAILURE_DETAILS_HEADER and not summaries:
            in_failure_details = True
            continue
        running = RUNNING_RE.fullmatch(line)
        if running:
            running_counts.append(int(running.group(1)))
            continue

        row, separator, result = line.partition(" ... ")
        stray_prefix, _space, test_id = row.rpartition(" ")
        if not separator or not (CASE_ID_RE.fullmatch(test_id) or FILE_ID_RE.fullmatch(test_id)):
            stray_lines += 1
            continue
        if FILE_ID_RE.fullmatch(test_id):
            issues.append(f"file-only terminal row at line {line_number}: {test_id}")
            continue
        if expected is not None and test_id not in expected:
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
        if stray_prefix:
            prefixed_rows += 1
        outcomes[test_id] = outcome

    if detail_summary is not None:
        summaries.append(detail_summary)
    issues.extend(detail_runner_errors)
    if len(summaries) != 1:
        issues.append(f"expected one terminal summary, found {len(summaries)}")
    if len(running_counts) > 1:
        issues.append(f"expected at most one running-count header, found {len(running_counts)}")
    elif expected is None and not running_counts:
        issues.append("a counted run requires its running-count header")
    elif running_counts and expected is not None and running_counts[0] != len(expected):
        issues.append(
            f"running-count header says {running_counts[0]}, manifest has {len(expected)}"
        )

    if expected is None:
        expected_count = running_counts[0] if len(running_counts) == 1 else 0
        missing = ()
        if expected_count == 0:
            issues.append("a counted run must select at least one case")
        if len(outcomes) != expected_count:
            issues.append(f"{len(outcomes)} terminal rows for {expected_count} counted cases")
    else:
        expected_count = len(expected)
        missing = tuple(sorted(expected - outcomes.keys()))
    passed = sum(outcome.status == "ok" for outcome in outcomes.values())
    failed = sum(outcome.status in TEST_FAILURE_STATUSES for outcome in outcomes.values())
    skipped = sum(outcome.status == "skipped" for outcome in outcomes.values())
    counts = (passed, failed, skipped)
    summary_value = summaries[0] if len(summaries) == 1 else None
    if summary_value is not None and summary_value != counts:
        issues.append(f"terminal counts {counts} do not match summary {summary_value}")
    undetailed = sorted(
        test_id
        for test_id, outcome in outcomes.items()
        if outcome.status in TEST_FAILURE_STATUSES and test_id not in failure_detail_ids
    )
    if undetailed:
        issues.append(f"failed rows without a failure-detail entry: {', '.join(undetailed)}")

    return RunValidation(
        log_path=str(path),
        expected_count=expected_count,
        outcomes=outcomes,
        missing_ids=missing,
        issues=tuple(issues),
        summary=summary_value,
        counts=counts,
        stray_line_count=stray_lines,
        prefixed_row_count=prefixed_rows,
    )


def compare_pair(
    reference: RunValidation,
    candidate: RunValidation,
    reference_exit_code: int | None,
    candidate_exit_code: int | None,
) -> dict:
    """Accept two completely accounted runs whose exit codes match their outcomes.

    A failed or timed-out test does not reject the pair; it is listed in
    `test_failures` for the correctness owner. Incomplete or malformed runs, an
    exit code other than the one the outcomes imply, differing ID sets, and
    differing pass/skip outcomes reject the pair.
    """
    issues: list[str] = []
    for arm, run, exit_code in (
        ("reference", reference, reference_exit_code),
        ("candidate", candidate, candidate_exit_code),
    ):
        if not run.complete:
            issues.append(f"{arm} terminal log is incomplete or malformed")
        if exit_code != run.expected_exit_code:
            issues.append(
                f"{arm} process exit code is {exit_code}; its outcomes require {run.expected_exit_code}"
            )

    reference_map = reference.outcomes
    candidate_map = candidate.outcomes
    if set(reference_map) != set(candidate_map):
        issues.append("reference and candidate terminal ID sets differ")
    status_differences = []
    for test_id in sorted(set(reference_map) & set(candidate_map)):
        reference_outcome = reference_map[test_id]
        candidate_outcome = candidate_map[test_id]
        if reference_outcome.comparable() == candidate_outcome.comparable():
            continue
        if {reference_outcome.status, candidate_outcome.status} & TEST_FAILURE_STATUSES:
            continue
        status_differences.append(
            {
                "id": test_id,
                "reference": {
                    "status": reference_outcome.status,
                    "skip_reason": reference_outcome.skip_reason,
                },
                "candidate": {
                    "status": candidate_outcome.status,
                    "skip_reason": candidate_outcome.skip_reason,
                },
            }
        )
    if status_differences:
        issues.append(f"{len(status_differences)} status/skip-reason differences")

    test_failures = [
        {"arm": arm, "id": test_id, "status": run.outcomes[test_id].status}
        for arm, run in (("reference", reference), ("candidate", candidate))
        for test_id in run.test_failures
    ]
    return {
        "accepted": not issues,
        "issues": issues,
        "test_failures": test_failures,
        "reference_exit_code": reference_exit_code,
        "candidate_exit_code": candidate_exit_code,
        "reference": reference.to_dict(),
        "candidate": candidate.to_dict(),
        "status_differences": status_differences,
    }
