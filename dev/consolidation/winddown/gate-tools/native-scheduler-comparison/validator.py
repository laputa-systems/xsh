#!/usr/bin/env python3
"""CLI wrapper and self-checks for strict native-suite result validation."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile

sys.dont_write_bytecode = True

from outcomes import CASE_ID_RE, FILE_ID_RE, compare_pair, read_manifest, validate_log


def _write(path: Path, value: str) -> Path:
    path.write_text(value, encoding="utf-8")
    return path


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def _case_ids_from_log(path: Path) -> list[str]:
    ids = set()
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        test_id, separator, _result = line.partition(" ... ")
        if separator and CASE_ID_RE.fullmatch(test_id):
            ids.add(test_id)
    return sorted(ids)


def self_check(history_pairs: list[tuple[Path, Path]], complete_log: Path | None) -> None:
    passed_checks: list[str] = []
    with tempfile.TemporaryDirectory(prefix="xsh-scheduler-validator-") as temp_name:
        temp = Path(temp_name)
        one = "core/tests/a.xsh::test_ok"
        two = "core/tests/b.xsh::test_skip"
        ids = sorted([one, two])

        complete = (
            "running 2 tests\n"
            f"{one} ... ok 1ms\n"
            f"{two} ... skipped: unavailable privilege 2.5ms\n"
            "test result: ok. 1 passed; 0 failed; 1 skipped\n"
        )
        ref_log = _write(temp / "reference-good.log", complete)
        cand_log = _write(temp / "candidate-good.log", complete)
        ref = validate_log(ref_log, ids)
        cand = validate_log(cand_log, ids)
        pair = compare_pair(ref, cand, 0, 0)
        _require(pair["accepted"], "identical complete results should pass")
        passed_checks.append("complete pair with a matching skip reason accepted")
        nonzero = compare_pair(ref, cand, 1, 0)
        _require(not nonzero["accepted"], "nonzero process exit must reject the pair")
        passed_checks.append("nonzero process exit rejected")

        different_skip = _write(
            temp / "candidate-different-skip.log",
            complete.replace("unavailable privilege", "different reason"),
        )
        mismatch = compare_pair(ref, validate_log(different_skip, ids), 0, 0)
        _require(not mismatch["accepted"], "different skip reasons must reject the pair")
        passed_checks.append("skip-reason mismatch rejected")

        duplicate = _write(
            temp / "duplicate.log",
            f"{one} ... ok 1ms\n{one} ... ok 2ms\n"
            "test result: ok. 2 passed; 0 failed; 0 skipped\n",
        )
        _require(not validate_log(duplicate, [one]).accepted, "duplicate ID must reject")
        passed_checks.append("duplicate terminal row rejected")

        missing = _write(
            temp / "missing.log",
            f"{one} ... ok 1ms\n"
            "test result: ok. 1 passed; 0 failed; 0 skipped\n",
        )
        missing_result = validate_log(missing, ids)
        _require(two in missing_result.missing_ids, "missing ID must be reported")
        _require(not missing_result.accepted, "missing ID must reject")
        passed_checks.append("missing terminal row rejected")

        file_only = _write(
            temp / "file-only.log",
            "core/tests/b.xsh ... FAILED 1ms\n"
            "test result: FAILED. 0 passed; 1 failed; 0 skipped\n",
        )
        file_only_result = validate_log(file_only, [one])
        _require(
            any("file-only terminal row" in issue for issue in file_only_result.issues),
            "file-only preparation/error ID must be identified",
        )
        _require(not file_only_result.accepted, "file-only row must reject")
        passed_checks.append("file-only preparation/error row rejected")

        extra = _write(
            temp / "extra.log",
            f"{one} ... ok 1ms\n"
            "core/tests/extra.xsh::test_extra ... ok 1ms\n"
            "test result: ok. 2 passed; 0 failed; 0 skipped\n",
        )
        extra_result = validate_log(extra, [one])
        _require(
            any("out-of-manifest" in issue for issue in extra_result.issues),
            "extra test ID must be identified",
        )
        _require(not extra_result.accepted, "out-of-manifest terminal row must reject")
        passed_checks.append("out-of-manifest terminal row rejected")

        wrong_summary = _write(
            temp / "wrong-summary.log",
            f"{one} ... ok 1ms\n"
            "test result: ok. 0 passed; 0 failed; 0 skipped\n",
        )
        summary_result = validate_log(wrong_summary, [one])
        _require(
            any("do not match summary" in issue for issue in summary_result.issues),
            "summary count mismatch must be identified",
        )
        _require(not summary_result.accepted, "summary count mismatch must reject")
        passed_checks.append("terminal summary count mismatch rejected")

        failed = _write(
            temp / "failed.log",
            f"{one} ... FAILED 1ms\n{two} ... TIMEOUT 2m\n"
            "test result: FAILED. 0 passed; 2 failed; 0 skipped\n",
        )
        failed_result = validate_log(failed, ids)
        _require(failed_result.counts == (0, 2, 0), "failure/timeout counts must normalize")
        _require(not failed_result.accepted, "failure or timeout must reject")
        passed_checks.append("failure and timeout outcomes rejected")

        malformed = _write(
            temp / "malformed.log",
            f"{one} ... ok maybe\n"
            "test result: ok. 1 passed; 0 failed; 0 skipped\n",
        )
        malformed_result = validate_log(malformed, [one])
        _require(
            any("malformed passing status" in issue for issue in malformed_result.issues),
            "malformed terminal status must be identified",
        )
        passed_checks.append("malformed terminal status rejected")

        duplicate_manifest = _write(temp / "duplicate.ids", f"{one}\n{one}\n")
        try:
            read_manifest(duplicate_manifest)
        except ValueError as exc:
            _require("duplicate test ID" in str(exc), "duplicate manifest ID needs a specific error")
        else:
            raise AssertionError("duplicate manifest IDs were accepted")
        passed_checks.append("duplicate manifest ID rejected")

        file_only_manifest = _write(temp / "file-only.ids", "core/tests/b.xsh\n")
        try:
            read_manifest(file_only_manifest)
        except ValueError as exc:
            _require("file-only preparation/error row" in str(exc), "file-only manifest needs a specific error")
        else:
            raise AssertionError("file-only manifest row was accepted")
        passed_checks.append("file-only manifest row rejected")

        unsorted_manifest = _write(temp / "unsorted.ids", f"{two}\n{one}\n")
        try:
            read_manifest(unsorted_manifest)
        except ValueError as exc:
            _require("not sorted" in str(exc), "unsorted manifest needs a specific error")
        else:
            raise AssertionError("unsorted manifest was accepted")
        passed_checks.append("unsorted manifest rejected")

        if complete_log is not None:
            complete_ids = _case_ids_from_log(complete_log)
            _require(bool(complete_ids), f"complete raw log has no terminal rows: {complete_log}")
            complete_result = validate_log(complete_log, complete_ids)
            _require(complete_result.accepted, f"complete raw log rejected: {complete_log}")
            passed_checks.append(
                f"historical complete raw log accepted ({complete_log.name}: "
                f"{len(complete_ids)} complete cases)"
            )

        for history_manifest, history_log in history_pairs:
            manifest_rows = history_manifest.read_text(encoding="utf-8", errors="replace").splitlines()
            try:
                read_manifest(history_manifest)
            except ValueError as exc:
                _require("file-only preparation/error row" in str(exc),
                         f"unexpected historical manifest rejection: {history_manifest}: {exc}")
            else:
                raise AssertionError(f"historical invalid manifest unexpectedly passed: {history_manifest}")
            expected = sorted(row for row in manifest_rows if CASE_ID_RE.fullmatch(row))
            file_only = [row for row in manifest_rows if FILE_ID_RE.fullmatch(row)]
            _require(bool(expected), f"historical manifest has no valid case IDs: {history_manifest}")
            _require(len(expected) == len(set(expected)), f"historical manifest duplicates IDs: {history_manifest}")
            _require(expected == sorted(expected), f"historical manifest is unsorted: {history_manifest}")
            _require(bool(file_only), f"historical manifest no longer contains file-only rows: {history_manifest}")
            result = validate_log(history_log, expected)
            _require(not result.accepted, f"historical partial log accepted: {history_log}")
            _require(result.summary is None, f"historical partial unexpectedly has a summary: {history_log}")
            _require(len(result.missing_ids) > 1, f"historical partial lacks the expected missing rows: {history_log}")
            passed_checks.append(
                f"historical interrupted log rejected ({history_log.name}: {len(file_only)} file-only IDs "
                f"in old list, {len(result.outcomes)}/{len(expected)} case IDs terminal, "
                f"{len(result.missing_ids)} case IDs missing)"
            )

    for check in passed_checks:
        print(f"PASS {check}")
    print(f"validator self-check passed: {len(passed_checks)} checks")


def validate_pair_cli(args: argparse.Namespace) -> int:
    ids = read_manifest(args.manifest)
    reference = validate_log(args.reference_log, ids)
    candidate = validate_log(args.candidate_log, ids)
    report = compare_pair(
        reference,
        candidate,
        args.reference_exit_code,
        args.candidate_exit_code,
    )
    output = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(output, encoding="utf-8")
    else:
        sys.stdout.write(output)
    return 0 if report["accepted"] else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate = subparsers.add_parser("pair", help="validate two complete suite logs")
    validate.add_argument("--manifest", type=Path, required=True)
    validate.add_argument("--reference-log", type=Path, required=True)
    validate.add_argument("--candidate-log", type=Path, required=True)
    validate.add_argument("--reference-exit-code", type=int, required=True)
    validate.add_argument("--candidate-exit-code", type=int, required=True)
    validate.add_argument("--output", type=Path)

    check = subparsers.add_parser("self-check", help="check parser behavior without running tests")
    check.add_argument("--history-pair", type=Path, action="append", nargs=2,
                       metavar=("MANIFEST", "LOG"), required=True)
    check.add_argument("--complete-log", type=Path)

    args = parser.parse_args()
    try:
        if args.command == "pair":
            return validate_pair_cli(args)
        self_check([(manifest, log) for manifest, log in args.history_pair], args.complete_log)
        return 0
    except (OSError, ValueError) as exc:
        print(f"validator error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
