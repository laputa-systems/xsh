from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import acceptance
import resources
from acceptance import GateResult, SourceIdentity


ROOT = Path(__file__).resolve().parents[2]
SCRATCH = ROOT / ".work" / "tmp"
COMMIT = "a" * 40
TREE = "b" * 40
SOURCE = SourceIdentity(COMMIT, TREE, True)
COHORT = ("lslocks", "lsipc", "lsmem", "mountpoint", "namei")
REGRESSIONS = (
    "core/tests/test-util-linux-process-regressions.xsh::test_process_case",
    "core/tests/test-util-linux-storage-regressions.xsh::test_storage_case",
)


class SourceIdentityTests(unittest.TestCase):
    def test_clean_identity_binds_commit_tree_and_worktree_status(self):
        outputs = iter((COMMIT + "\n", TREE + "\n", ""))
        calls = []

        def run(argv, **kwargs):
            calls.append((argv, kwargs))
            return subprocess.CompletedProcess(argv, 0, next(outputs), "")

        identity = resources.source_identity(ROOT, run=run)

        self.assertEqual(identity, SOURCE)
        self.assertEqual(len(calls), 3)
        self.assertTrue(all(call[1]["cwd"] == ROOT for call in calls))
        self.assertEqual(calls[-1][0], ["git", "status", "--porcelain=v1", "--untracked-files=all"])

    def test_dirty_worktree_is_reported_as_dirty(self):
        outputs = iter((COMMIT, TREE, " M core/lslocks.xsh\n"))

        def run(argv, **_kwargs):
            return subprocess.CompletedProcess(argv, 0, next(outputs), "")

        self.assertFalse(resources.source_identity(ROOT, run=run).clean)

    def test_git_failure_is_not_replaced_with_a_source_default(self):
        def run(argv, **_kwargs):
            return subprocess.CompletedProcess(argv, 128, "", "fatal: no HEAD")

        with self.assertRaisesRegex(RuntimeError, "rev-parse.*failed"):
            resources.source_identity(ROOT, run=run)


class RegressionSelectorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-selectors-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.files = (
            "core/tests/test-util-linux-process-regressions.xsh",
            "core/tests/test-util-linux-storage-regressions.xsh",
        )
        for relative, name in zip(self.files, ("test_process_case", "test_storage_case"), strict=True):
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"test {name} {{\n}}\n", encoding="utf-8")

    def test_clean_source_returns_actual_declared_selectors_in_stable_order(self):
        identities = iter((SOURCE, SOURCE))
        with patch.object(acceptance, "REGRESSION_FILES", self.files):
            selectors = resources.regression_selectors(
                self.root,
                SOURCE,
                identify=lambda _root: next(identities),
            )

        self.assertEqual(selectors, tuple(sorted(REGRESSIONS)))

    def test_dirty_or_stale_source_fails_before_reading_files(self):
        dirty = SourceIdentity(COMMIT, TREE, False)
        stale = SourceIdentity("c" * 40, TREE, True)
        with patch.object(acceptance, "REGRESSION_FILES", ("missing.xsh",)):
            for source, observed in ((SOURCE, dirty), (stale, SOURCE)):
                with self.subTest(source=source), self.assertRaisesRegex(ValueError, "current clean source"):
                    resources.regression_selectors(
                        self.root,
                        source,
                        identify=lambda _root, value=observed: value,
                    )

    def test_missing_empty_changed_and_duplicate_declarations_fail(self):
        with patch.object(acceptance, "REGRESSION_FILES", ("missing.xsh",)):
            with self.assertRaisesRegex(ValueError, "missing required regression source"):
                acceptance.declared_regression_selectors(self.root)
        empty = self.root / self.files[0]
        empty.write_text("let no_test = 1\n", encoding="utf-8")
        with patch.object(acceptance, "REGRESSION_FILES", (self.files[0],)):
            with self.assertRaisesRegex(ValueError, "declares no behaviors"):
                acceptance.declared_regression_selectors(self.root)
        empty.write_text("test test_duplicate {\n}\ntest test_duplicate {\n}\n", encoding="utf-8")
        with patch.object(acceptance, "REGRESSION_FILES", (self.files[0],)):
            with self.assertRaisesRegex(ValueError, "duplicates behavior selectors"):
                acceptance.declared_regression_selectors(self.root)

    def test_source_change_while_selectors_are_read_fails(self):
        changed = SourceIdentity(COMMIT, "c" * 40, True)
        identities = iter((SOURCE, changed))
        with patch.object(acceptance, "REGRESSION_FILES", self.files):
            with self.assertRaisesRegex(ValueError, "source changed while reading"):
                resources.regression_selectors(
                    self.root,
                    SOURCE,
                    identify=lambda _root: next(identities),
                )


class ProductSignatureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-products-")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        for name in resources.PRODUCTS:
            product = self.directory / name
            product.write_bytes((name + "\n").encode())
            product.chmod(0o755)

    def test_signature_tracks_only_regular_executable_product_contents(self):
        before = resources._product_signature(self.directory)
        self.assertEqual(tuple(row[0] for row in before), resources.PRODUCTS)
        (self.directory / "xsh").write_bytes(b"different\n")
        after = resources._product_signature(self.directory)
        self.assertNotEqual(before, after)

    def test_symlink_nonexecutable_and_non_directory_products_fail(self):
        product = self.directory / "xsht"
        product.unlink()
        product.symlink_to(self.directory / "xsh")
        with self.assertRaises((OSError, ValueError)):
            resources._product_signature(self.directory)
        product.unlink()
        product.write_bytes(b"not executable")
        product.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "regular executable"):
            resources._product_signature(self.directory)
        with self.assertRaises((NotADirectoryError, OSError)):
            resources._product_signature(self.directory / "xsh")


class MeasurementProtocolTests(unittest.TestCase):
    def rows(self, regressions=REGRESSIONS):
        rows = [
            "\t".join(("positive", command, resources.POSITIVE_SELECTORS[command], "1024", "2048", "0", "0", "1", "1"))
            for command in COHORT
        ]
        rows.extend("\t".join(("regression", selector, "3072", "4096", "0", "0", "1", "1")) for selector in regressions)
        return rows

    def parse(self, rows, regressions=REGRESSIONS, cohort=COHORT):
        return resources._parse_measurements("\n".join(rows).encode(), cohort, regressions)

    def test_exact_positive_and_regression_denominators_are_required(self):
        parsed = self.parse(self.rows())
        self.assertEqual(len(parsed), len(COHORT) + len(REGRESSIONS))
        with self.assertRaisesRegex(ValueError, "positive boundary exactly once"):
            self.parse(self.rows()[:-1])
        with self.assertRaisesRegex(ValueError, "frozen regression selectors exactly once"):
            self.parse(self.rows(), regressions=REGRESSIONS + ("unexpected::test_extra",))

    def test_zero_rows_duplicate_selectors_and_unknown_rows_fail(self):
        with self.assertRaisesRegex(ValueError, "unrecognized measurement"):
            self.parse(())
        rows = self.rows()
        rows.append(rows[0])
        with self.assertRaisesRegex(ValueError, "repeated positive fixture"):
            self.parse(rows)
        rows = self.rows()
        rows[0] = rows[0].replace("positive", "unknown", 1)
        with self.assertRaisesRegex(ValueError, "unrecognized measurement"):
            self.parse(rows)

    def test_failed_skipped_zero_rss_mutated_output_and_state_fail(self):
        mutations = (
            ("1024\t2048\t0\t0\t1\t1", "0\t2048\t0\t0\t1\t1", "positive resource sample did not complete"),
            ("1024\t2048\t0\t0\t1\t1", "1024\t2048\t2\t0\t1\t1", "positive resource sample did not complete"),
            ("1024\t2048\t0\t0\t1\t1", "1024\t2048\t0\t0\t0\t1", "changed output or fixture state"),
            ("1024\t2048\t0\t0\t1\t1", "1024\t2048\t0\t0\t1\t0", "changed output or fixture state"),
        )
        for original, mutated, error in mutations:
            with self.subTest(mutated=mutated):
                rows = self.rows()
                rows[0] = rows[0].replace(original, mutated)
                with self.assertRaisesRegex(ValueError, error):
                    self.parse(rows)

    def test_malformed_numeric_observation_fails(self):
        rows = self.rows()
        rows[0] = rows[0].replace("1024", "1KiB", 1)
        with self.assertRaisesRegex(ValueError, "nonnumeric measurement"):
            self.parse(rows)


class EffectBoundaryTests(unittest.TestCase):
    def test_readiness_uses_the_named_program_and_preserves_quiet_result(self):
        calls = []

        def run(argv, **kwargs):
            calls.append((argv, kwargs))
            return subprocess.CompletedProcess(argv, 0, b"", b"")

        ready = resources._readiness(Path("/tools/perf-gate-ready"), ("-1",), run=run)

        self.assertTrue(ready.host_quiet)
        self.assertEqual(calls, [(["/tools/perf-gate-ready", "-1"], {"capture_output": True, "check": False, "timeout": 30})])

    def test_dirty_source_stops_resource_and_close_effects(self):
        class Runtime:
            COHORT = COHORT

            def source_identity(self):
                return SourceIdentity(COMMIT, TREE, False)

            def __getattr__(self, name):
                raise AssertionError(f"effect ran before source validation: {name}")

        runtime = Runtime()
        with self.assertRaisesRegex(ValueError, "clean final source"):
            resources.measure(runtime)
        with self.assertRaisesRegex(ValueError, "clean final source"):
            resources.close(runtime)

    def test_close_does_not_infer_missing_gates(self):
        class Runtime:
            COHORT = COHORT

            def source_identity(self):
                return SOURCE

            def regression_selectors(self, source):
                raise AssertionError("not reached")

        with self.assertRaises(AttributeError):
            resources.close(Runtime())

    def test_returned_close_gate_must_be_current_and_passed(self):
        stale = SourceIdentity("old", "old-tree", True)
        gate = GateResult("docs", stale, 0, 1, 1, 0, (), (), (), ())
        with self.assertRaisesRegex(ValueError, "mismatched gate evidence"):
            resources._validate_returned_gate(gate, "docs", SOURCE)
        failed = GateResult("docs", SOURCE, 1, 1, 0, 1, (), (), (), ())
        with self.assertRaisesRegex(ValueError, "did not pass"):
            resources._validate_returned_gate(failed, "docs", SOURCE)


if __name__ == "__main__":
    unittest.main()
