from argparse import Namespace
from pathlib import Path
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import acceptance


ROOT = Path(__file__).resolve().parents[2]


class SelectorTests(unittest.TestCase):
    def args(self, **changes):
        values = {
            "all": False,
            "command": None,
            "commands": None,
            "case": None,
            "require_positive_boundaries": False,
        }
        values.update(changes)
        return Namespace(**values)

    def test_unknown_command_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "outside the cohort"):
            acceptance._selected_commands(self.args(commands="lslocks,lsfd"), ("lslocks",))

    def test_empty_and_repeated_commands_are_rejected(self):
        for selector in ("", "lslocks,", ",lslocks"):
            with self.subTest(selector=selector), self.assertRaisesRegex(ValueError, "nonempty"):
                acceptance._selected_commands(self.args(commands=selector), ("lslocks",))
        with self.assertRaisesRegex(ValueError, "distinct"):
            acceptance._selected_commands(self.args(commands="lslocks,lslocks"), ("lslocks",))

    def test_case_cannot_select_multiple_commands(self):
        with self.assertRaisesRegex(ValueError, "requires one command"):
            acceptance._selected_commands(
                self.args(commands="lslocks,lsipc", case="test_lslocks_blocker_flock"),
                ("lslocks", "lsipc"),
            )

    def test_positive_boundaries_are_real_declared_selectors(self):
        for command, boundary in acceptance.POSITIVE_BOUNDARIES.items():
            with self.subTest(command=command):
                self.assertIn(boundary, acceptance._case_names(ROOT, command))

    def test_missing_required_positive_boundary_fails_before_dispatch(self):
        runtime = SimpleNamespace(
            ROOT=ROOT,
            COHORT=("lslocks",),
            container=lambda *_args, **_kwargs: self.fail("unexpected native dispatch"),
            protected_products=lambda: self.fail("unexpected product check"),
            native_case=lambda *_args, **_kwargs: self.fail("unexpected exact dispatch"),
        )
        with patch.dict(acceptance.POSITIVE_BOUNDARIES, {}, clear=True):
            with self.assertRaisesRegex(ValueError, "required positive boundary is absent"):
                acceptance.parity(
                    self.args(command="lslocks", require_positive_boundaries=True),
                    runtime,
                )

    def test_unknown_case_is_rejected(self):
        names = acceptance._case_names(ROOT, "lslocks")
        with self.assertRaisesRegex(ValueError, "matched no behavior"):
            acceptance._selected_case(self.args(case="test_not_a_real_case"), "lslocks", names)

    def test_empty_case_is_not_treated_as_a_group_run(self):
        names = acceptance._case_names(ROOT, "lslocks")
        with self.assertRaisesRegex(ValueError, "matched no behavior"):
            acceptance._selected_case(self.args(case=""), "lslocks", names)


class NativeResultTests(unittest.TestCase):
    def test_matching_count_and_actual_selector_are_required(self):
        output = (
            b"running 1 tests\n"
            b"core/tests/test-lsmem.xsh::test_lsmem_rooted_sparse_memory_attributes_and_output_modes ... ok 1ms\n"
            b"test result: ok. 1 passed; 0 failed; 0 skipped\n"
        )
        result = acceptance._parse_native_group(
            "lsmem",
            ("test_lsmem_rooted_sparse_memory_attributes_and_output_modes",),
            output,
        )
        self.assertEqual(result.passed, 1)

    def test_summary_count_is_anchored_not_a_substring(self):
        output = (
            b"running 1 tests\n"
            b"core/tests/test-lsmem.xsh::test_lsmem_rooted_sparse_memory_attributes_and_output_modes ... ok 1ms\n"
            b"test result: ok. 11 passed; 0 failed; 0 skipped\n"
        )
        with self.assertRaisesRegex(ValueError, "did not pass"):
            acceptance._parse_native_group(
                "lsmem",
                ("test_lsmem_rooted_sparse_memory_attributes_and_output_modes",),
                output,
            )

    def test_zero_selected_tests_cannot_pass(self):
        output = b"running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 skipped\n"
        with self.assertRaisesRegex(ValueError, "no behavior"):
            acceptance._parse_native_group("lsmem", (), output)

    def test_overselected_native_count_cannot_pass(self):
        selector = "test_lsmem_rooted_sparse_memory_attributes_and_output_modes"
        output = (
            f"running 2 tests\ncore/tests/test-lsmem.xsh::{selector} ... ok 1ms\n"
            "test result: ok. 1 passed; 0 failed; 0 skipped\n"
        ).encode()
        with self.assertRaisesRegex(ValueError, "unexpected test count"):
            acceptance._parse_native_group("lsmem", (selector,), output)

    def test_unaccounted_skip_cannot_pass(self):
        output = (
            b"running 1 tests\n"
            b"core/tests/test-lsmem.xsh::test_lsmem_rooted_sparse_memory_attributes_and_output_modes ... skipped: unavailable 0ms\n"
            b"test result: ok. 0 passed; 0 failed; 1 skipped\n"
        )
        with self.assertRaisesRegex(ValueError, "did not pass"):
            acceptance._parse_native_group(
                "lsmem",
                ("test_lsmem_rooted_sparse_memory_attributes_and_output_modes",),
                output,
            )

    def test_missing_or_duplicated_selector_output_cannot_pass(self):
        selector = "test_lsmem_rooted_sparse_memory_attributes_and_output_modes"
        one = f"core/tests/test-lsmem.xsh::{selector} ... ok 1ms\n".encode()
        for result_lines in (b"", one + one):
            with self.subTest(result_lines=result_lines), self.assertRaisesRegex(
                ValueError, "exactly once"
            ):
                acceptance._parse_native_group(
                    "lsmem",
                    (selector,),
                    b"running 1 tests\n"
                    + result_lines
                    + b"test result: ok. 1 passed; 0 failed; 0 skipped\n",
                )

    def test_unselected_result_line_cannot_be_ignored(self):
        selector = "test_lsmem_rooted_sparse_memory_attributes_and_output_modes"
        output = (
            f"running 1 tests\ncore/tests/test-lsmem.xsh::{selector} ... ok 1ms\n"
            "core/tests/test-lsmem.xsh::test_unselected_behavior ... ok 1ms\n"
            "test result: ok. 1 passed; 0 failed; 0 skipped\n"
        ).encode()
        with self.assertRaisesRegex(ValueError, "unselected behavior"):
            acceptance._parse_native_group("lsmem", (selector,), output)

    def test_trailing_output_invalidates_final_summary(self):
        selector = "test_lsmem_rooted_sparse_memory_attributes_and_output_modes"
        output = (
            f"running 1 tests\ncore/tests/test-lsmem.xsh::{selector} ... ok 1ms\n"
            "test result: ok. 1 passed; 0 failed; 0 skipped\nextra\n"
        ).encode()
        with self.assertRaisesRegex(ValueError, "final result summary"):
            acceptance._parse_native_group("lsmem", (selector,), output)

    def test_mutated_behavior_output_cannot_report_success(self):
        selector = "test_lsmem_rooted_sparse_memory_attributes_and_output_modes"
        output = (
            f"running 1 tests\ncore/tests/test-lsmem.xsh::{selector} ... FAILED 1ms\n"
            "test result: FAILED. 0 passed; 1 failed; 0 skipped\n"
        ).encode()
        with self.assertRaisesRegex(ValueError, "did not pass"):
            acceptance._parse_native_group("lsmem", (selector,), output)


class RegressionDeclarationTests(unittest.TestCase):
    def test_frozen_selector_ids_come_from_both_actual_native_files(self):
        selectors = acceptance.declared_regression_selectors(ROOT)
        self.assertEqual(selectors, tuple(sorted(selectors)))
        self.assertEqual(
            {selector.rsplit("::", 1)[0] for selector in selectors},
            set(acceptance.REGRESSION_FILES),
        )

    def test_missing_or_empty_regression_sources_fail(self):
        with patch.object(acceptance, "REGRESSION_FILES", ("missing.xsh",)):
            with self.assertRaisesRegex(ValueError, "missing required regression source"):
                acceptance.declared_regression_selectors(ROOT)
        with patch.object(acceptance, "REGRESSION_FILES", ("dev/util-linux/README.md",)):
            with self.assertRaisesRegex(ValueError, "declares no behaviors"):
                acceptance.declared_regression_selectors(ROOT)


class ResourceControlTests(unittest.TestCase):
    def test_changed_products_between_dispatch_checks_are_rejected(self):
        paths = iter((Path("products-before"), Path("products-after")))
        runtime = SimpleNamespace(
            ROOT=ROOT,
            COHORT=("lsmem",),
            PRIVATE_NAMESPACE="private",
            protected_products=lambda: next(paths),
            container=lambda *_args, **_kwargs: b"",
        )

        with self.assertRaisesRegex(ValueError, "products changed"):
            acceptance._run_group(runtime, "lsmem", acceptance._case_names(ROOT, "lsmem"))

    def test_stale_resource_source_is_rejected(self):
        current = acceptance.SourceIdentity("commit", "tree", True)
        stale = acceptance.SourceIdentity("old-commit", "old-tree", True)
        runtime = SimpleNamespace(PACKAGE_COMMIT="package", COHORT=("lsmem",))
        report = acceptance.ResourceReport(stale, (), (), (), acceptance.DeferredTiming("host busy"))
        with self.assertRaisesRegex(ValueError, "stale source"):
            acceptance._validate_resource_report(report, runtime, current)

    def test_binary_growth_above_one_percent_is_rejected(self):
        source = acceptance.SourceIdentity("commit", "tree", True)
        baseline = acceptance.BuildContext("product", "release", "target", "image", "musl", "one-cpu")
        candidate = acceptance.BuildContext(source.key, "release", "target", "image", "musl", "one-cpu")
        sizes = tuple(
            acceptance.BinarySize(
                name,
                100,
                102 if name == "xsh" else 100,
                baseline,
                candidate,
            )
            for name in ("xsh", "xsht", "xsh-test-helper")
        )
        report = acceptance.ResourceReport(source, sizes, (), (), acceptance.DeferredTiming("host busy"))
        runtime = SimpleNamespace(
            PACKAGE_COMMIT="package", PRODUCT_SOURCE="product", COHORT=("lsmem",)
        )
        with self.assertRaisesRegex(ValueError, "more than 1%"):
            acceptance._validate_resource_report(report, runtime, source)

    def test_candidate_context_must_match_frozen_source(self):
        source = acceptance.SourceIdentity("commit", "tree", True)
        baseline = acceptance.BuildContext("base", "release", "target", "image", "musl", "one-cpu")
        stale_candidate = acceptance.BuildContext("commit:old-tree", "release", "target", "image", "musl", "one-cpu")
        with self.assertRaisesRegex(ValueError, "stale source"):
            acceptance._validate_contexts(
                acceptance.MeasurementKind.EXISTING_BASELINE,
                baseline,
                stale_candidate,
                source,
                "package",
                "product",
            )

    def test_four_timing_pairs_cannot_qualify(self):
        ready = acceptance.Readiness(0, b"", b"", True)
        pair = acceptance.TimedPair(
            1,
            1,
            ready,
            ready,
            acceptance.PairOrder.BASELINE_FIRST,
            0,
            0,
            True,
            True,
        )
        with self.assertRaisesRegex(ValueError, "exactly 5"):
            acceptance._validate_timed_pairs((pair, pair, pair, pair))

    def test_failed_or_unexecuted_close_gate_cannot_pass(self):
        source = acceptance.SourceIdentity("commit", "tree", True)
        unexecuted = acceptance.GateResult(
            "focused-regressions", source, 0, 0, 0, 0, (), (), (), ()
        )
        with self.assertRaisesRegex(ValueError, "did not pass"):
            acceptance._validate_gate_result(unexecuted, source)
        failed = acceptance.GateResult(
            "focused-regressions", source, 1, 1, 0, 1, (), (), ("test::case",), ()
        )
        with self.assertRaisesRegex(ValueError, "did not pass"):
            acceptance._validate_gate_result(failed, source)

    def test_unaccounted_close_skip_cannot_pass(self):
        source = acceptance.SourceIdentity("commit", "tree", True)
        gate = acceptance.GateResult(
            "full-native", source, 0, 1, 0, 0, ("test::unavailable",), (), (), ()
        )
        with self.assertRaisesRegex(ValueError, "unaccounted skips"):
            acceptance._validate_gate_result(gate, source)

    def test_close_rejects_owned_process_leak(self):
        source = acceptance.SourceIdentity("commit", "tree", True)
        report = acceptance.CloseReport(
            source,
            (),
            acceptance.ResourceReport(source, (), (), (), acceptance.DeferredTiming("host busy")),
            remaining_owned_processes=1,
            lost_work=False,
        )
        runtime = SimpleNamespace(
            ROOT=ROOT,
            source_identity=lambda: source,
            close_gates=lambda: report,
        )
        with self.assertRaisesRegex(ValueError, "processes remain"):
            acceptance.close(runtime)


if __name__ == "__main__":
    unittest.main()
