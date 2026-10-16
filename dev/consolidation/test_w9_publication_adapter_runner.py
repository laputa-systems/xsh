import argparse
import io
import hashlib
import multiprocessing
import os
import pathlib
import re
import signal
import shutil
import sys
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch


sys.path.insert(0, str(pathlib.Path(__file__).parent))
import w9_publication_adapter_runner as adapter_runner
import test_w9_publication_sources as source_fixtures

FIXTURE_INITIALIZER_STATEMENT_START = 470
FIXTURE_INITIALIZER_EXPRESSION_START = 472
EMPTY_EXACT_OWNER_FACTS = {
    "exprs": {},
    "expr_count": 0,
    "record_field_count": 0,
    "records": {},
    "aliases": {},
    "tails": {},
    "structured_stages": {},
    "stage_arguments": {},
    "stage_callbacks": {},
}


class AdapterRawOutcomeTests(unittest.TestCase):
    def test_source_qualified_raw_output_rejects_worktree_path_before_creation(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        controller_root = runner.CONTROLLER_ROOT
        worktrees = controller_root / ".work/wt"
        worktrees.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="w9-raw-output-guard-", dir=worktrees) as attempt:
            raw_root = pathlib.Path(attempt) / "raw"
            source_record = {
                "schema": runner.SOURCE_COHORT_SCHEMA,
                "candidate_root": str(controller_root),
                "controller_root": str(controller_root),
                "controller_commit": runner.source_identity(controller_root)[0],
                "controller_tree": runner.source_identity(controller_root)[1],
                "builder_path": str(runner.SOURCE_BUILDER_PATH),
                "input_root": str(repository / ".work/tmp"),
                "program_root": str(runner.PROGRAM_SNAPSHOT_ROOT.resolve()),
            }

            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "overlaps a source-qualified protected root",
            ):
                adapter_runner.prepare_raw_output_directory(
                    raw_root, controller_root, runner, source_record
                )

            self.assertFalse(raw_root.exists())

    def test_scratch_overlay_identity_accepts_only_the_recorded_source_change(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_parent = repository / ".work/tmp"
        scratch_parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="w9-adapter-overlay-identity-", dir=scratch_parent) as scratch:
            source_root = pathlib.Path(scratch)
            tests_path = source_root / "tests/sema.rs"
            tests_path.parent.mkdir()
            tests_path.write_bytes(b"#[test]\nfn original_test() {}\n")
            extra_source = source_root / "src/lib.rs"
            extra_source.parent.mkdir()
            extra_source.write_bytes(b"pub fn original() {}\n")
            for command in (
                ["git", "init", "--quiet", str(source_root)],
                ["git", "-C", str(source_root), "config", "user.name", "Test"],
                ["git", "-C", str(source_root), "config", "user.email", "test@example.invalid"],
                ["git", "-C", str(source_root), "add", "tests/sema.rs", "src/lib.rs"],
                ["git", "-C", str(source_root), "commit", "--quiet", "-m", "test(fixtures): initialize source"],
            ):
                subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            runner = adapter_runner._load_publication_runner(
                repository / "dev/consolidation/w9-publication-run.py"
            )
            commit, tree = runner.source_identity(source_root)
            tests_path.write_bytes(adapter_runner._adapter_overlay(tests_path.read_bytes(), runner))
            adapter_runner._require_scratch_overlay_identity(
                source_root, commit, tree, "candidate", runner
            )

            extra_source.write_bytes(b"pub fn unexpected() {}\n")
            with self.assertRaisesRegex(
                (adapter_runner.PublicationError, runner.PublicationError),
                "outside its parity include",
            ):
                adapter_runner._require_scratch_overlay_identity(
                    source_root, commit, tree, "candidate", runner
                )
            extra_source.write_bytes(b"pub fn original() {}\n")
            subprocess.run(
                ["git", "-C", str(source_root), "commit", "--quiet", "--allow-empty", "-m", "test(fixtures): commit source drift"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            with self.assertRaisesRegex(adapter_runner.PublicationError, "source identity changed"):
                adapter_runner._require_scratch_overlay_identity(
                    source_root, commit, tree, "candidate", runner
                )

    def test_reference_source_accepts_the_main_checkout_as_candidate(self):
        candidate = pathlib.Path("/checkout")
        reference = candidate / ".work/wt/w9-adapter-ref-e240"
        runner = SimpleNamespace(
            REFERENCE_HEAD="e240efad273fe35d66d91f4037b6a8193527da2e",
            real_directory=lambda path, label: pathlib.Path(path),
            reference_source_root=lambda path: pathlib.Path(path) / ".work/wt/w9-adapter-ref-e240",
            source_identity=lambda path: (
                "e240efad273fe35d66d91f4037b6a8193527da2e",
                adapter_runner.REFERENCE_TREE,
            ),
        )
        self.assertEqual(
            adapter_runner._prepare_reference_source(candidate, reference, runner),
            reference,
        )

    def test_capture_plan_covers_all_1690_inputs_in_four_profiles(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        ranges, pending_ranges = adapter_runner._capture_plan(1690, runner)

        self.assertEqual(len(ranges), 68)
        self.assertEqual(pending_ranges, ranges)
        self.assertEqual(sum(end - start for start, end in ranges), 1690)
        self.assertEqual(len(adapter_runner.CAPTURE_SLOTS), 4)
        self.assertEqual(1690 * len(adapter_runner.PROFILES), 6760)

    def test_reused_publication_sizes_prioritize_large_ranges_with_canonical_ties(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        ranges = [(0, 1), (1, 2), (2, 3)]
        with tempfile.TemporaryDirectory(prefix="w9-capture-priority-", dir=scratch_root) as scratch:
            raw_root = pathlib.Path(scratch)
            for index, per_profile_size in enumerate((1, 5, 5)):
                for profile in adapter_runner.PROFILES:
                    record = raw_root / f"record-{index}-{profile}"
                    record.mkdir()
                    (record / "kind").write_text("checked", encoding="utf-8")
                    (record / "diagnostics").write_bytes(b"")
                    (record / "publication").write_bytes(b"p" * per_profile_size)
                    (record / "receipt-hashes").write_text("\n", encoding="utf-8")

            with patch.object(runner, "expected_ranges", return_value=ranges):
                publication_bytes = adapter_runner._publication_bytes_by_range(
                    raw_root, len(ranges), ranges, runner
                )
                planned_ranges, pending_ranges = adapter_runner._capture_plan(
                    len(ranges), runner, publication_bytes
                )

        self.assertEqual(planned_ranges, ranges)
        self.assertEqual(publication_bytes, {(0, 1): 4, (1, 2): 20, (2, 3): 20})
        self.assertEqual(pending_ranges, [(1, 2), (2, 3), (0, 1)])

    def test_capture_plan_rejects_incomplete_reference_size_hints(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )

        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "reference publication byte counts do not cover each capture range",
        ):
            adapter_runner._capture_plan(50, runner, {(0, 25): 1})

    def test_reference_size_hint_rejects_symlinked_publication(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        ranges = [(0, 1)]
        with tempfile.TemporaryDirectory(prefix="w9-capture-priority-link-", dir=scratch_root) as scratch:
            raw_root = pathlib.Path(scratch) / "raw"
            raw_root.mkdir()
            outside_publication = pathlib.Path(scratch) / "outside-publication"
            outside_publication.write_bytes(b"outside")
            for profile in adapter_runner.PROFILES:
                record = raw_root / f"record-0-{profile}"
                record.mkdir()
                (record / "kind").write_text("checked", encoding="utf-8")
                (record / "diagnostics").write_bytes(b"")
                (record / "receipt-hashes").write_text("\n", encoding="utf-8")
                publication = record / "publication"
                if profile == adapter_runner.PROFILES[0]:
                    publication.symlink_to(outside_publication)
                else:
                    publication.write_bytes(b"p")

            with patch.object(runner, "expected_ranges", return_value=ranges):
                with self.assertRaisesRegex(
                    adapter_runner.PublicationError,
                    "cannot open reference publication bytes for record-0-default",
                ):
                    adapter_runner._publication_bytes_by_range(raw_root, 1, ranges, runner)

    def test_record_rejects_a_symlinked_sidecar_member(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        case_root = pathlib.Path(tempfile.mkdtemp(prefix="w9-adapter-symlink-", dir=scratch_root))
        try:
            record = case_root / "record-0-default"
            record.mkdir()
            target = case_root / "outside"
            target.write_text("checked", encoding="utf-8")
            (record / "kind").symlink_to(target)
            (record / "diagnostics").write_text("[]", encoding="utf-8")
            (record / "publication").write_text("{}", encoding="utf-8")
            (record / "receipt-hashes").write_text("0" * 64 + "\t" + "0" * 64, encoding="utf-8")
            root = adapter_runner._load_publication_runner(
                repository / "dev/consolidation/w9-publication-run.py"
            ).open_absolute_nofollow(case_root, directory=True)
            try:
                with self.assertRaisesRegex(adapter_runner.PublicationError, "cannot read regular sidecar"):
                    adapter_runner._record(root, (0, "", "default"))
            finally:
                os.close(root)
        finally:
            shutil.rmtree(case_root)

    def test_native_raw_hashes_bind_file_bytes_to_the_receipt_hashes(self):
        diagnostics_hash = "1" * 64
        publication_hash = "2" * 64
        output = "".join(
            f"w9rawhash\t0\t{profile}\t{field}\t{digest}\n"
            for profile in adapter_runner.PROFILES
            for field, digest in (("diagnostics", diagnostics_hash), ("publication", publication_hash))
        )
        hashes = adapter_runner._parse_raw_hash_output(output, 1)

        self.assertTrue(adapter_runner._raw_hash_pair_matches(
            hashes, 0, "default", (diagnostics_hash, publication_hash)
        ))
        self.assertFalse(adapter_runner._raw_hash_pair_matches(
            hashes, 0, "default", (diagnostics_hash, "3" * 64)
        ))

    def test_native_raw_hash_output_rejects_incomplete_or_repeated_keys(self):
        with self.assertRaisesRegex(adapter_runner.PublicationError, "coverage differs"):
            adapter_runner._parse_raw_hash_output("", 1)
        duplicate = "w9rawhash\t0\tdefault\tdiagnostics\t" + "a" * 64 + "\n"
        with self.assertRaisesRegex(adapter_runner.PublicationError, "repeats"):
            adapter_runner._parse_raw_hash_output(duplicate + duplicate, 1)

    def test_native_raw_hash_output_accepts_only_the_libtest_progress_prefix(self):
        rows = [
            f"w9rawhash\t{index}\t{profile}\t{field}\t{'a' * 64}"
            for index in range(1690)
            for profile in adapter_runner.PROFILES
            for field in ("diagnostics", "publication")
        ]
        progress = "test sema::w9_repository_parity_receipt ... "
        hashes = adapter_runner._parse_raw_hash_output(
            progress + rows[0] + "\n" + "\n".join(rows[1:]), 1690
        )
        self.assertEqual(len(hashes), 13520)

        with self.assertRaisesRegex(adapter_runner.PublicationError, "unknown prefix"):
            adapter_runner._parse_raw_hash_output(
                "test sema::different_test ... " + rows[0] + "\n" + "\n".join(rows[1:]),
                1690,
            )


class SourceQualifiedAdapterReceiptTests(unittest.TestCase):
    def setUp(self):
        self.repository = pathlib.Path(__file__).resolve().parents[2]
        self.runner = adapter_runner._load_publication_runner(
            self.repository / "dev/consolidation/w9-publication-run.py"
        )
        self.original_source_identity = self.runner.source_identity
        self.source_identity_overrides = {}
        self.scratch = self.runner.CONTROLLER_ROOT / ".work/tmp"
        self.scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="w9-source-adapter-receipt-", dir=self.scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.source_root = self.repository
        self.source_commit, self.source_tree = "a" * 40, "b" * 40
        self.input_root = self.root / "inputs"
        self.input_root.mkdir()
        self.input_manifest = self.root / "input-files.tsv"
        self.input_manifest.write_text("0\tsource.xsh\t" + "a" * 64 + "\t444\n", encoding="utf-8")
        self.source_receipt = self.root / "source-cohort.receipt"
        self.source_receipt.write_text("source receipt fixture\n", encoding="utf-8")
        self.controller_root = self.root / "controller"
        self.controller_helper_source = (
            self.controller_root / "dev/consolidation/w9-publication-owner-helper.rs"
        )
        self.controller_helper_source.parent.mkdir(parents=True)
        shutil.copyfile(
            self.repository / "dev/consolidation/w9-publication-owner-helper.rs",
            self.controller_helper_source,
        )
        subprocess.run(
            ["git", "init", "--quiet", str(self.controller_root)],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        (self.controller_root / ".work").mkdir()
        (self.controller_root / ".git/info/exclude").write_text("/.work/\n", encoding="utf-8")
        subprocess.run(
            ["git", "-C", str(self.controller_root), "config", "user.name", "W9 test"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        subprocess.run(
            ["git", "-C", str(self.controller_root), "config", "user.email", "w9-test@example.invalid"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        subprocess.run(
            ["git", "-C", str(self.controller_root), "add", "dev/consolidation/w9-publication-owner-helper.rs"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        subprocess.run(
            ["git", "-C", str(self.controller_root), "commit", "--quiet", "-m", "test(fixtures): pin controller helper"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.controller_commit, self.controller_tree = self.original_source_identity(
            self.controller_root
        )
        self.source_identity_overrides[self.controller_root] = (
            self.controller_commit,
            self.controller_tree,
        )
        self.source_identity_overrides[self.source_root] = (
            self.source_commit,
            self.source_tree,
        )
        self.source_record = {
            "schema": self.runner.SOURCE_COHORT_SCHEMA,
            "role": "source-cohort",
            "candidate_root": str(self.source_root),
            "candidate_commit": self.source_commit,
            "candidate_tree": self.source_tree,
            "controller_root": str(self.controller_root),
            "controller_commit": self.controller_commit,
            "controller_tree": self.controller_tree,
            "input_root": str(self.input_root),
            "input_manifest": str(self.input_manifest),
            "input_manifest_sha256": self.runner.sha256_file(self.input_manifest),
            "corpus_blake3": self.runner.CORPUS_BLAKE3,
            "program_root": str(self.runner.PROGRAM_SNAPSHOT_ROOT.resolve()),
            "builder_path": str(self.runner.SOURCE_BUILDER_PATH),
            "builder_sha256": self.runner.sha256_file(self.runner.SOURCE_BUILDER_PATH),
            "builder_image": self.runner.SOURCE_BUILD_IMAGE,
            "build_target": self.runner.SOURCE_BUILD_TARGET,
            "build_profile": self.runner.SOURCE_BUILD_PROFILE,
            "rustflags": self.runner.SOURCE_RUSTFLAGS,
            "linker_path": str(self.runner.SOURCE_LINKER_PATH),
            "linker_sha256": self.runner.sha256_file(self.runner.SOURCE_LINKER_PATH),
            "runner": str(self.source_root / "dev/consolidation/w9-publication-run.py"),
            "runner_sha256": self.runner.sha256_file(
                self.source_root / "dev/consolidation/w9-publication-run.py"
            ),
        }
        self.binary = self.root / "adapter-test-binary"
        self.binary.write_bytes(b"adapter harness")
        self.binary.chmod(0o700)
        self.helper = self.root / "w9-publication-adapter.rs"
        self.helper.write_bytes(b"adapter helper")
        self.owner_helper = self.root / "w9-publication-owner-helper.rs"
        self.owner_helper.write_bytes(
            adapter_runner._derived_owner_helper_bytes(
                self.controller_helper_source.read_bytes()
            )
        )
        self.owner_helper_source = self.controller_helper_source
        tests = self.source_root / "tests/sema.rs"
        overlay = adapter_runner._adapter_overlay(self.runner.read_regular_bytes(tests), self.runner)
        self.build = {
            "schema": "2",
            "role": "candidate",
            "source_root": str(self.source_root),
            "source_commit": self.source_commit,
            "source_tree": self.source_tree,
            "binary": str(self.binary),
            "binary_sha256": self.runner.sha256_file(self.binary),
            "source_receipt": str(self.source_receipt),
            "source_receipt_sha256": self.runner.sha256_file(self.source_receipt),
            "input_root": str(self.input_root),
            "input_manifest": str(self.input_manifest),
            "input_manifest_sha256": self.source_record["input_manifest_sha256"],
            "corpus_blake3": self.runner.CORPUS_BLAKE3,
            "builder_path": self.source_record["builder_path"],
            "builder_sha256": self.source_record["builder_sha256"],
            "builder_image": self.runner.SOURCE_BUILD_IMAGE,
            "build_target": self.runner.SOURCE_BUILD_TARGET,
            "build_profile": self.runner.SOURCE_BUILD_PROFILE,
            "rustflags": self.runner.SOURCE_RUSTFLAGS,
            "linker_path": self.source_record["linker_path"],
            "linker_sha256": self.source_record["linker_sha256"],
            "runner_path": self.source_record["runner"],
            "runner_sha256": self.source_record["runner_sha256"],
            "approved_helper_path": str(adapter_runner.SOURCE_APPROVED_HELPER),
            "approved_helper_sha256": adapter_runner.APPROVED_HELPER_SHA256,
            "helper_path": str(self.helper),
            "helper_sha256": self.runner.sha256_file(self.helper),
            "owner_helper_path": str(self.owner_helper),
            "owner_helper_sha256": self.runner.sha256_file(self.owner_helper),
            "owner_helper_source_path": str(self.owner_helper_source),
            "owner_helper_source_sha256": self.runner.sha256_file(self.owner_helper_source),
            "overlay_sha256": hashlib.sha256(overlay).hexdigest(),
        }
        self.build_receipt = self.root / "adapter-build.receipt"
        adapter_runner._write_adapter_build_receipt(self.build_receipt, self.build, self.runner)

    def parse_build(self, record=None):
        if record is not None:
            fields = adapter_runner.SOURCE_ADAPTER_BUILD_FIELDS
            self.build_receipt.write_text(
                "".join(f"{field}\t{record[field]}\n" for field in fields),
                encoding="utf-8",
            )
        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=self.source_record),
            patch.object(
                self.runner,
                "source_identity",
                side_effect=self._fixture_source_identity,
            ),
            patch.object(self.runner, "ROOT", self.controller_root),
        ):
            return adapter_runner._parse_adapter_build_receipt(
                self.build_receipt, "candidate", self.runner, self.owner_helper_source
            )

    def _fixture_source_identity(self, root):
        root = pathlib.Path(root)
        if root in self.source_identity_overrides:
            return self.source_identity_overrides[root]
        return self.original_source_identity(root)

    def capture_fixture(self):
        capture_root = self.controller_root / ".work/captures/candidate-capture"
        capture_root.parent.mkdir(parents=True, exist_ok=True)
        raw_root = capture_root / "raw"
        receipt_directory = capture_root / "receipts"
        raw_root.mkdir(parents=True)
        receipt_directory.mkdir()
        range_receipt = self.runner.candidate_receipt_path(receipt_directory, 0, 1)
        range_receipt.write_text("range fixture\n", encoding="utf-8")
        capture_receipt = capture_root / "capture.receipt"
        adapter_runner._write_capture_receipt(
            capture_receipt,
            "candidate",
            self.build_receipt,
            self.build,
            capture_root,
            raw_root,
            receipt_directory,
            self.input_root,
            self.input_manifest,
            {(0, 1): range_receipt},
            self.runner,
        )
        return capture_receipt, capture_root, raw_root, receipt_directory

    def parse_capture(self, capture_receipt, capture_root, raw_root, receipt_directory):
        with patch.object(self.runner, "parse_candidate_receipt", return_value=self.source_record):
            return adapter_runner._parse_capture_receipt(
                capture_receipt,
                "candidate",
                receipt_directory,
                capture_root,
                raw_root,
                self.build,
                {0: self.runner.ManifestSource("source.xsh", "a" * 64, 0o444)},
                self.input_root,
                self.input_manifest,
                self.runner,
            )

    def _local_correction_sources(self, correction):
        original_root = self.root / ".work/wt/w9-adapter-ref-e240"
        derived_root = self.root / "derived-reference"
        patch_path = self.root / "derived-reference.patch"
        patch_bytes = b"fixture correction patch\n"
        patch_path.write_bytes(patch_bytes)
        for root, source in (
            (original_root, source_fixtures.OLD_SOURCE),
            (derived_root, source_fixtures.OLD_SOURCE),
        ):
            tui = root / "stdlib/tui.xsh"
            tui.parent.mkdir(parents=True)
            tui.write_bytes(source)
        for name, value in (
            ("CHECKOUT_ROOT", self.root),
            ("SOURCE_ROOT", derived_root),
            ("PATCH_PATH", patch_path),
            ("PATCH_SHA256", hashlib.sha256(patch_bytes).hexdigest()),
        ):
            patcher = patch.object(correction, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        source = {
            "base_commit": correction.BASE_COMMIT,
            "base_tree": correction.BASE_TREE,
            "patch_path": str(patch_path),
            "patch_sha256": hashlib.sha256(patch_bytes).hexdigest(),
            "source_root": str(derived_root),
            "source_commit": correction.SOURCE_COMMIT,
            "source_tree": correction.SOURCE_TREE,
            "source_parent": correction.SOURCE_PARENT,
            "source_state": "clean",
        }

        def validate_fixture_source(source_root, observed_patch_path, _runner):
            self.assertEqual(pathlib.Path(source_root), derived_root)
            self.assertEqual(pathlib.Path(observed_patch_path), patch_path)
            return source

        source_validator = patch.object(
            correction, "validate_source", side_effect=validate_fixture_source
        )
        source_validator.start()
        self.addCleanup(source_validator.stop)
        return original_root, derived_root, patch_path, source

    def test_schema_two_build_and_capture_receipts_bind_source_and_cohort(self):
        build = self.parse_build()
        capture_receipt, capture_root, raw_root, receipt_directory = self.capture_fixture()
        capture = self.parse_capture(capture_receipt, capture_root, raw_root, receipt_directory)

        self.assertEqual(build, self.build)
        self.assertEqual(capture["source_receipt"], str(self.source_receipt))
        self.assertEqual(capture["source_receipt_sha256"], self.runner.sha256_file(self.source_receipt))
        self.assertEqual(capture["range_count"], "1")
        self.assertEqual(capture["outcome_count"], str(len(adapter_runner.PROFILES)))
        self.assertNotIn("legacy_capture_receipt", capture)

    def test_schema_two_build_receipt_pins_approved_helper_to_candidate_cohort(self):
        candidate_root = self.root / "candidate-source"
        candidate_root.mkdir()
        source_paths = (
            pathlib.Path("tests/sema.rs"),
            pathlib.Path("dev/consolidation/w9-source-build.sh"),
            pathlib.Path("dev/consolidation/w9-publication-run.py"),
            pathlib.Path("dev/consolidation/w9-publication-owner-helper.rs"),
            pathlib.Path("dev/consolidation/winddown/gate-tools/w9-repository-parity.rs"),
        )
        for relative_path in source_paths:
            destination = candidate_root / relative_path
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(self.repository / relative_path, destination)
        for command in (
            ["git", "init", "--quiet", str(candidate_root)],
            ["git", "-C", str(candidate_root), "config", "user.name", "Test"],
            ["git", "-C", str(candidate_root), "config", "user.email", "test@example.invalid"],
            ["git", "-C", str(candidate_root), "add", *[str(path) for path in source_paths]],
            ["git", "-C", str(candidate_root), "commit", "--quiet", "-m", "test(fixtures): initialize candidate source"],
        ):
            subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        candidate_commit, candidate_tree = self.runner.source_identity(candidate_root)
        self.source_identity_overrides[candidate_root] = (candidate_commit, candidate_tree)
        candidate_approved_helper = (
            candidate_root / "dev/consolidation/winddown/gate-tools/w9-repository-parity.rs"
        )
        self.assertEqual(
            adapter_runner._candidate_approved_helper(candidate_root, self.runner),
            candidate_approved_helper,
        )
        controller_approved_helper = adapter_runner.SOURCE_APPROVED_HELPER
        derived_owner_bytes = adapter_runner._derived_owner_helper_bytes(
            self.controller_helper_source.read_bytes()
        )
        source_record = dict(
            self.source_record,
            candidate_root=str(candidate_root),
            candidate_commit=candidate_commit,
            candidate_tree=candidate_tree,
            builder_path=str(candidate_root / "dev/consolidation/w9-source-build.sh"),
            builder_sha256=self.runner.sha256_file(
                candidate_root / "dev/consolidation/w9-source-build.sh"
            ),
            runner=str(candidate_root / "dev/consolidation/w9-publication-run.py"),
            runner_sha256=self.runner.sha256_file(
                candidate_root / "dev/consolidation/w9-publication-run.py"
            ),
        )
        record = dict(
            self.build,
            source_root=str(candidate_root),
            source_commit=candidate_commit,
            source_tree=candidate_tree,
            builder_path=source_record["builder_path"],
            builder_sha256=source_record["builder_sha256"],
            runner_path=source_record["runner"],
            runner_sha256=source_record["runner_sha256"],
            approved_helper_path=str(candidate_approved_helper),
            owner_helper_source_path=str(self.controller_helper_source),
            owner_helper_source_sha256=self.runner.sha256_file(self.controller_helper_source),
            owner_helper_sha256=hashlib.sha256(derived_owner_bytes).hexdigest(),
            overlay_sha256=hashlib.sha256(
                adapter_runner._adapter_overlay(
                    (candidate_root / "tests/sema.rs").read_bytes(), self.runner
                )
            ).hexdigest(),
        )

        def parse_candidate_build(build_record):
            self.build_receipt.unlink(missing_ok=True)
            adapter_runner._write_adapter_build_receipt(
                self.build_receipt, build_record, self.runner
            )
            with patch.object(self.runner, "parse_candidate_receipt", return_value=source_record):
                with (
                    patch.object(
                        self.runner,
                        "source_identity",
                        side_effect=self._fixture_source_identity,
                    ),
                    patch.object(self.runner, "ROOT", self.controller_root),
                ):
                    return adapter_runner._parse_adapter_build_receipt(
                        self.build_receipt,
                        "candidate",
                        self.runner,
                        self.controller_helper_source,
                    )

        self.assertNotEqual(controller_approved_helper, candidate_approved_helper)
        self.assertEqual(
            self.runner.sha256_file(controller_approved_helper),
            self.runner.sha256_file(candidate_approved_helper),
        )
        parsed = parse_candidate_build(record)
        self.assertEqual(parsed["approved_helper_path"], str(candidate_approved_helper))

        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "source-qualified approved helper identity differs",
        ):
            parse_candidate_build(
                dict(record, approved_helper_path=str(controller_approved_helper))
            )

        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "owner helper source identity differs",
        ):
            parse_candidate_build(
                dict(
                    record,
                    owner_helper_source_path=str(
                        candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
                    ),
                )
            )

        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "owner helper source identity differs",
        ):
            parse_candidate_build(
                dict(record, owner_helper_source_sha256="f" * 64)
            )

        wrong_controller = dict(
            source_record,
            controller_root=str(candidate_root),
        )
        with (
            patch.object(
                self.runner,
                "source_identity",
                side_effect=self._fixture_source_identity,
            ),
            patch.object(self.runner, "ROOT", self.controller_root),
            patch.object(self.runner, "parse_candidate_receipt", return_value=wrong_controller),
        ):
            self.build_receipt.unlink(missing_ok=True)
            adapter_runner._write_adapter_build_receipt(
                self.build_receipt, record, self.runner
            )
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "source-qualified helper controller root differs",
            ):
                adapter_runner._parse_adapter_build_receipt(
                    self.build_receipt,
                    "candidate",
                    self.runner,
                    self.controller_helper_source,
                )

    def test_derived_owner_helper_passes_build_validation_and_rejects_drift(self):
        output_directory = adapter_runner._new_derived_helper_directory(
            self.source_root, self.runner
        )
        self.addCleanup(shutil.rmtree, output_directory.parent, ignore_errors=True)
        helper, helper_sha256, owner_sha256, owner_source_sha256 = (
            adapter_runner.derive_helper(
                approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
                owner_helper=self.controller_helper_source,
                output_directory=output_directory,
                output_root=self.source_root,
            )
        )
        owner_helper = output_directory / "w9-publication-owner-helper.rs"
        self.assertNotEqual(owner_sha256, owner_source_sha256)

        real_run = subprocess.run
        clone_calls = []

        def stop_before_compiler(command, *args, **kwargs):
            if command[:2] == ["git", "clone"]:
                clone_calls.append(command)
                return subprocess.CompletedProcess(command, 1, "", "stopped before compiler")
            return real_run(command, *args, **kwargs)

        def build_helper():
            with (
                patch.object(self.runner, "parse_candidate_receipt", return_value=self.source_record),
                patch.object(
                    self.runner,
                    "source_identity",
                    side_effect=self._fixture_source_identity,
                ),
                patch.object(self.runner, "ROOT", self.controller_root),
                patch.object(self.runner, "local_docker_environment", return_value={}),
                patch.object(adapter_runner.subprocess, "run", side_effect=stop_before_compiler),
            ):
                adapter_runner.build_adapter_harness(
                    self.source_root,
                    self.source_root,
                    self.source_receipt,
                    self.source_record,
                    "candidate",
                    helper,
                    helper_sha256,
                    owner_sha256,
                    owner_source_sha256,
                    self.runner,
                )

        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "could not clone candidate source for adapter build: stopped before compiler",
        ):
            build_helper()
        self.assertEqual(len(clone_calls), 1)

        owner_helper.write_bytes(owner_helper.read_bytes() + b"drift")
        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "owner helper changed before harness build",
        ):
            build_helper()
        self.assertEqual(len(clone_calls), 1)

    def test_build_receipt_authenticates_raw_and_derived_owner_helper_bytes(self):
        output_directory = adapter_runner._new_derived_helper_directory(
            self.source_root, self.runner
        )
        self.addCleanup(shutil.rmtree, output_directory.parent, ignore_errors=True)
        _helper, _helper_sha256, owner_sha256, owner_source_sha256 = (
            adapter_runner.derive_helper(
                approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
                owner_helper=self.controller_helper_source,
                output_directory=output_directory,
                output_root=self.source_root,
            )
        )
        owner_helper = output_directory / "w9-publication-owner-helper.rs"
        record = dict(
            self.build,
            owner_helper_path=str(owner_helper),
            owner_helper_sha256=owner_sha256,
            owner_helper_source_path=str(self.controller_helper_source),
            owner_helper_source_sha256=owner_source_sha256,
        )

        parsed = self.parse_build(record)
        self.assertEqual(parsed["owner_helper_source_sha256"], owner_source_sha256)
        self.assertEqual(parsed["owner_helper_sha256"], owner_sha256)
        self.assertNotEqual(owner_sha256, owner_source_sha256)

        tampered_helper = output_directory / "w9-publication-owner-helper-tampered.rs"
        tampered_helper.write_bytes(owner_helper.read_bytes() + b"drift")
        tampered_record = dict(
            record,
            owner_helper_path=str(tampered_helper),
            owner_helper_sha256=self.runner.sha256_file(tampered_helper),
        )
        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "derived owner helper bytes differ",
        ):
            self.parse_build(tampered_record)

        raw_owner_helper = self.controller_helper_source.read_bytes()
        self.controller_helper_source.write_bytes(raw_owner_helper + b"drift")
        try:
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "source-qualified controller owner helper SHA-256 changed",
            ):
                self.parse_build(record)
        finally:
            self.controller_helper_source.write_bytes(raw_owner_helper)

    def test_schema_three_build_keeps_candidate_owner_helper_authority(self):
        candidate_helper = self.source_root / "dev/consolidation/w9-publication-owner-helper.rs"
        self.owner_helper.write_bytes(candidate_helper.read_bytes())
        source_record = dict(self.source_record, schema="3")
        record = dict(
            self.build,
            owner_helper_source_path=str(candidate_helper),
            owner_helper_source_sha256=self.runner.sha256_file(candidate_helper),
            owner_helper_sha256=self.runner.sha256_file(candidate_helper),
        )
        self.build_receipt.unlink(missing_ok=True)
        adapter_runner._write_adapter_build_receipt(
            self.build_receipt, record, self.runner
        )
        with (
            patch.object(self.runner, "SOURCE_COHORT_SCHEMA", "3"),
            patch.object(self.runner, "parse_candidate_receipt", return_value=source_record),
            patch.object(
                self.runner,
                "source_identity",
                side_effect=self._fixture_source_identity,
            ),
            patch.object(self.runner, "ROOT", self.controller_root),
        ):
            parsed = adapter_runner._parse_adapter_build_receipt(
                self.build_receipt,
                "candidate",
                self.runner,
                candidate_helper,
            )
        self.assertEqual(parsed["owner_helper_source_path"], str(candidate_helper))

    def test_schema_two_build_receipt_rejects_stale_source_runner_or_mixed_schema(self):
        for field, value, message in (
            ("source_root", str(self.root / "stale-source"), "source root"),
            ("source_receipt_sha256", "f" * 64, "source receipt hash differs"),
            ("runner_path", str(self.root / "stale-runner"), "runner_path differs"),
            ("runner_sha256", "f" * 64, "runner_sha256 differs"),
            ("input_manifest", str(self.root / "stale-manifest"), "input cohort identity differs"),
        ):
            with self.subTest(field=field):
                record = dict(self.build, **{field: value})
                error_types = (adapter_runner.PublicationError, self.runner.PublicationError)
                with self.assertRaisesRegex(error_types, message):
                    self.parse_build(record)

        mixed = dict(self.build, schema="1")
        with self.assertRaisesRegex(adapter_runner.PublicationError, "malformed adapter build receipt row"):
            self.parse_build(mixed)

    def test_schema_two_capture_receipt_rejects_stale_identity_coverage_and_mixed_schema(self):
        capture_receipt, capture_root, raw_root, receipt_directory = self.capture_fixture()
        valid_lines = capture_receipt.read_text(encoding="utf-8").splitlines()
        field_indexes = {line.split("\t", 1)[0]: index for index, line in enumerate(valid_lines)}
        for field, value, message in (
            ("source_root", str(self.root / "stale-source"), "source_root differs"),
            ("source_receipt_sha256", "f" * 64, "source receipt hash differs"),
            ("runner_sha256", "f" * 64, "runner_sha256 differs"),
            ("input_manifest", str(self.root / "stale-manifest"), "source cohort identity differs"),
            ("capture_root", str(self.root / "stale-capture"), "capture output paths differ"),
            ("raw_root", str(self.root / "stale-raw"), "capture output paths differ"),
            ("outcome_count", "3", "required source/profile coverage differs"),
            ("range_count", "2", "required source/profile coverage differs"),
        ):
            with self.subTest(field=field):
                lines = list(valid_lines)
                lines[field_indexes[field]] = f"{field}\t{value}"
                capture_receipt.write_text("\n".join(lines) + "\n", encoding="utf-8")
                with self.assertRaisesRegex(adapter_runner.PublicationError, message):
                    self.parse_capture(capture_receipt, capture_root, raw_root, receipt_directory)

        lines = list(valid_lines)
        range_row = next(index for index, line in enumerate(lines) if line.startswith("range\t"))
        lines[range_row] = lines[range_row].replace("range\t0\t1\t", "range\t1\t2\t", 1)
        capture_receipt.write_text("\n".join(lines) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(adapter_runner.PublicationError, "range set differs"):
            self.parse_capture(capture_receipt, capture_root, raw_root, receipt_directory)

        lines = list(valid_lines)
        lines[0] = "schema\t1"
        capture_receipt.write_text("\n".join(lines) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(adapter_runner.PublicationError, "schemas differ"):
            self.parse_capture(capture_receipt, capture_root, raw_root, receipt_directory)

    def test_schema_four_comparison_rejects_a_mixed_capture_bundle(self):
        expected_inputs = {
            0: self.runner.ManifestSource("source.xsh", "a" * 64, 0o444)
        }
        reference_root = self.root / "reference-source"
        reference_root.mkdir()
        reference_capture = self.root / "reference/capture.receipt"
        candidate_capture = self.root / "candidate/capture.receipt"
        reference_capture.parent.mkdir()
        candidate_capture.parent.mkdir()
        reference_build_path = self.root / "reference-build.receipt"
        candidate_build_path = self.root / "candidate-build.receipt"
        reference_build_path.write_text("reference build\n", encoding="utf-8")
        candidate_build_path.write_text("candidate build\n", encoding="utf-8")
        candidate_source_record = {
            "schema": self.runner.SOURCE_COHORT_SCHEMA,
            "candidate_commit": self.source_commit,
            "candidate_tree": self.source_tree,
        }
        builds = {
            reference_build_path: {
                "schema": "2",
                "source_root": str(reference_root),
                "source_commit": "c" * 40,
                "source_tree": "d" * 40,
                "helper_sha256": "e" * 64,
                "owner_helper_sha256": "f" * 64,
            },
            candidate_build_path: {
                "schema": "2",
                "source_root": str(self.source_root),
                "source_commit": self.source_commit,
                "source_tree": self.source_tree,
                "helper_sha256": "e" * 64,
                "owner_helper_sha256": "f" * 64,
            },
        }
        capture_records = {
            reference_capture: {"schema": "2"},
            candidate_capture: {"schema": "1"},
        }

        with (
            patch.object(self.runner, "regular_file", side_effect=lambda path, *_args, **_kwargs: pathlib.Path(path)),
            patch.object(self.runner, "require_sha256", return_value="a" * 64),
            patch.object(
                self.runner,
                "validate_source_cohort_inputs",
                return_value=(expected_inputs, {}),
            ),
            patch.object(
                adapter_runner,
                "_capture_field",
                side_effect=lambda _path, field, _runner: (
                    str(reference_build_path)
                    if field == "build_receipt" and _path == reference_capture
                    else str(candidate_build_path)
                ),
            ),
            patch.object(
                adapter_runner,
                "_parse_adapter_build_receipt",
                side_effect=lambda path, *_args: builds[path],
            ),
            patch.object(
                adapter_runner,
                "_parse_capture_receipt",
                side_effect=lambda path, *_args: capture_records[path],
            ),
            patch.object(self.runner, "real_directory", side_effect=lambda path, *_args: pathlib.Path(path)),
        ):
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "source-qualified comparison requires source-qualified adapter captures",
            ):
                adapter_runner.compare_full_publications(
                    self.runner,
                    None,
                    self.root / "candidate-receipts",
                    self.root / "reference-raw",
                    self.root / "candidate-raw",
                    expected_inputs,
                    self.root / "mismatches.tsv",
                    reference_capture,
                    candidate_capture,
                    self.source_root,
                    reference_root,
                    candidate_source_record,
                    self.input_root,
                    self.input_manifest,
                    {"PATH": os.environ["PATH"]},
                    reference_exact_owners={0: (object(), {})},
                    candidate_exact_owners={0: (object(), {})},
                )

    def test_exact_owner_binding_dispatches_capture_schema_without_legacy_fallback(self):
        capture_receipt = self.root / "adapter-capture.receipt"
        capture_receipt.write_text("adapter capture fixture\n", encoding="utf-8")
        proof = {
            "capture": {
                "schema": "2",
                "adapter_capture_receipt": str(capture_receipt),
                "adapter_capture_receipt_sha256": self.runner.sha256_file(capture_receipt),
            }
        }
        adapter_runner._require_exact_capture_binding(
            proof, "reference", capture_receipt, self.runner
        )

        unknown = {
            "capture": {
                "schema": "3",
                "legacy_capture_receipt": str(capture_receipt),
                "legacy_capture_receipt_sha256": self.runner.sha256_file(capture_receipt),
            }
        }
        with self.assertRaisesRegex(adapter_runner.PublicationError, "unsupported capture schema"):
            adapter_runner._require_exact_capture_binding(
                unknown, "reference", capture_receipt, self.runner
            )

    def test_capture_field_rejects_unknown_schema(self):
        receipt = self.root / "unknown-capture.receipt"
        receipt.write_text("schema\t3\nlegacy_capture_receipt\tignored\n", encoding="utf-8")
        with self.assertRaisesRegex(adapter_runner.PublicationError, "unsupported adapter capture receipt schema"):
            adapter_runner._capture_field(receipt, "legacy_capture_receipt", self.runner)

    def test_capture_launch_uses_the_source_qualified_builder_and_identity_environment(self):
        source_record = {
            "schema": self.runner.SOURCE_COHORT_SCHEMA,
            "candidate_root": str(self.source_root),
            "candidate_commit": self.source_commit,
            "candidate_tree": self.source_tree,
            "builder_path": self.source_record["builder_path"],
            "input_root": str(self.input_root),
            "program_root": str(self.runner.PROGRAM_SNAPSHOT_ROOT.resolve()),
        }
        output = self.root / "capture-jobs"
        receipt_directory = output / "candidate/receipts"
        receipt_directory.mkdir(parents=True)
        raw_root = output / "candidate/raw"
        raw_root.mkdir()
        captured = {}

        def record_launch(job, active, command, cwd, environment):
            captured.update(job=job, active=active, command=command, cwd=cwd, environment=environment)
            active.append(job)

        with (
            patch.object(self.runner, "existing_container", return_value=False),
            patch.object(self.runner, "start_and_register_job", side_effect=record_launch),
        ):
            job = adapter_runner._launch_capture_job(
                self.runner,
                source_record,
                output,
                "candidate",
                self.build,
                self.binary,
                raw_root,
                receipt_directory,
                0,
                1,
                "A",
                [],
                {"PATH": "/usr/bin:/bin"},
            )
        job["output"].close()

        self.assertEqual(
            captured["command"][2:5],
            [source_record["builder_path"], str(self.source_root), "run"],
        )
        self.assertIn(f"W9_OUTPUT_ROOT={output.resolve()}", captured["command"][:captured["command"].index("env")])
        self.assertIn(f"W9_INPUT_ROOT={self.input_root}", captured["command"])
        self.assertIn(f"W9_OUTPUT_ROOT={output.resolve()}", captured["command"])
        self.assertIn("W9_CPU_SET=0", captured["command"])
        self.assertEqual(captured["environment"]["W9_SOURCE_COMMIT"], self.source_commit)
        self.assertEqual(captured["environment"]["W9_SOURCE_TREE"], self.source_tree)

    def test_integrated_main_context_worker_plan_uses_controller_output_with_sibling_roots(self):
        controller_root = self.runner.CONTROLLER_ROOT
        worktrees = controller_root / ".work/wt"
        worktrees.mkdir(parents=True, exist_ok=True)
        candidate_root = worktrees / f"w9-output-plan-{os.getpid()}"
        if candidate_root.exists():
            self.fail(f"test candidate worktree already exists: {candidate_root}")
        subprocess.run(
            ["git", "-C", str(controller_root), "worktree", "add", "--quiet", "--detach", str(candidate_root), "HEAD"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.addCleanup(
            lambda: subprocess.run(
                ["git", "-C", str(controller_root), "worktree", "remove", "--force", str(candidate_root)],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
        )
        program_root = self.runner.PROGRAM_SNAPSHOT_ROOT.resolve(strict=True)
        input_root = (controller_root / ".work/tmp/w9-corpus-recovery/input-corpus").resolve(strict=True)
        output_parent = controller_root / ".work/consolidation"
        output_parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="w9-output-plan-", dir=output_parent) as output_name:
            output = pathlib.Path(output_name)
            role_root = output / "candidate"
            receipt_directory = role_root / "receipts"
            raw_root = role_root / "raw"
            (role_root / "jobs/A").mkdir(parents=True)
            receipt_directory.mkdir()
            raw_root.mkdir()
            candidate_commit, candidate_tree = self.runner.source_identity(candidate_root)
            controller_commit, controller_tree = self.runner.source_identity(controller_root)
            source_record = {
                "schema": self.runner.SOURCE_COHORT_SCHEMA,
                "candidate_root": str(candidate_root),
                "candidate_commit": candidate_commit,
                "candidate_tree": candidate_tree,
                "controller_root": str(controller_root),
                "controller_commit": controller_commit,
                "controller_tree": controller_tree,
                "builder_path": str(controller_root / "dev/consolidation/w9-source-build.sh"),
                "input_root": str(input_root),
                "program_root": str(program_root),
            }
            self.runner._validate_publication_output_path(
                output,
                candidate_root,
                source_record,
                "source-qualified worker output",
            )
            captured = {}

            def record_launch(job, active, command, cwd, environment):
                captured.update(job=job, active=active, command=command, cwd=cwd, environment=environment)
                active.append(job)

            with (
                patch.object(self.runner, "existing_container", return_value=False),
                patch.object(self.runner, "start_and_register_job", side_effect=record_launch),
            ):
                job = adapter_runner._launch_capture_job(
                    self.runner,
                    source_record,
                    output,
                    "candidate",
                    {
                        "binary": str(candidate_root / ".work/test-binary"),
                        "binary_sha256": "a" * 64,
                        "helper_sha256": "b" * 64,
                        "source_commit": candidate_commit,
                        "source_tree": candidate_tree,
                    },
                    candidate_root / ".work/test-binary",
                    raw_root,
                    receipt_directory,
                    0,
                    1,
                    "A",
                    [],
                    {"PATH": "/usr/bin:/bin"},
                )
            job["output"].close()
            self.assertEqual(candidate_root.parent, program_root.parent)
            self.assertEqual(pathlib.Path(captured["environment"]["W9_PROGRAM_ROOT"]), program_root)
            command = list(captured["command"])
            command[command.index("run")] = "plan-run"
            result = subprocess.run(
                command,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env={**os.environ, **captured["environment"]},
            )
            plan_arguments = result.stdout.split("\0")
            self.assertIn(f"{output}:{output}:rw", plan_arguments)
            self.assertIn(f"{program_root}:{program_root}:ro", plan_arguments)
            self.assertIn(f"{input_root}:{input_root}:ro", plan_arguments)

    def test_source_qualified_correction_flow_uses_fresh_original_capture_and_proof(self):
        correction = adapter_runner.reference_correction
        original_source_root, derived_source_root, patch_path, correction_source = (
            self._local_correction_sources(correction)
        )
        candidate_root = self.root / "candidate-source"
        candidate_root.mkdir()
        subprocess.run(
            ["git", "init", "--quiet", str(candidate_root)],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        candidate_work = candidate_root / ".work"
        candidate_work.mkdir()
        expected_inputs = {
            index: self.runner.ManifestSource(
                f"source/{index:04}.xsh", f"{index + 1:064x}", 0o444
            )
            for index in range(1690)
        }
        source_record = dict(
            self.source_record,
            candidate_root=str(candidate_root),
            candidate_commit=self.source_commit,
            candidate_tree=self.source_tree,
        )

        def capture_fixture(name, role, source_root, commit, tree, build_receipt):
            capture_root = self.controller_root / ".work/captures" / name
            capture_root.parent.mkdir(parents=True, exist_ok=True)
            raw_root = capture_root / "raw"
            receipt_directory = capture_root / "receipts"
            raw_root.mkdir(parents=True)
            receipt_directory.mkdir()
            capture_receipt = capture_root / "capture.receipt"
            fields = {
                "schema": "2",
                "role": role,
                "source_root": str(source_root),
                "source_commit": commit,
                "source_tree": tree,
                "build_receipt": str(build_receipt),
                "capture_root": str(capture_root),
                "raw_root": str(raw_root),
                "receipt_directory": str(receipt_directory),
            }
            capture_receipt.write_text(
                "".join(f"{field}\t{value}\n" for field, value in fields.items()),
                encoding="utf-8",
            )
            return capture_receipt, fields

        original_build_receipt = self.root / "original-build.receipt"
        original_build_receipt.write_text("original build fixture\n", encoding="utf-8")
        derived_build_receipt = self.root / "derived-build.receipt"
        derived_build_receipt.write_text("derived build fixture\n", encoding="utf-8")
        candidate_build_receipt = self.root / "candidate-build.receipt"
        candidate_build_receipt.write_text("candidate build fixture\n", encoding="utf-8")
        original_capture_receipt, original_capture_fields = capture_fixture(
            "original-capture",
            "reference",
            original_source_root,
            correction.BASE_COMMIT,
            correction.BASE_TREE,
            original_build_receipt,
        )
        original_exact_capture_receipt = self.root / "original-exact-capture.receipt"
        original_exact_capture_receipt.write_text("fresh exact-owner capture\n", encoding="utf-8")
        derived_capture_receipt, derived_capture_fields = capture_fixture(
            "derived-capture",
            "derived-reference",
            derived_source_root,
            correction.SOURCE_COMMIT,
            correction.SOURCE_TREE,
            derived_build_receipt,
        )
        candidate_capture_receipt, candidate_capture_fields = capture_fixture(
            "candidate-capture",
            "candidate",
            candidate_root,
            self.source_commit,
            self.source_tree,
            candidate_build_receipt,
        )
        source = correction.validate_source(derived_source_root, patch_path, self.runner)
        helper_sha256 = "c" * 64
        owner_helper_sha256 = "d" * 64
        derived_build = {
            "schema": "2",
            "role": "derived-reference",
            "source_root": str(derived_source_root),
            "source_commit": correction.SOURCE_COMMIT,
            "source_tree": correction.SOURCE_TREE,
            "binary": str(self.binary),
            "binary_sha256": self.runner.sha256_file(self.binary),
            "helper_sha256": helper_sha256,
            "owner_helper_sha256": owner_helper_sha256,
        }
        derived_capture = {
            **derived_capture_fields,
            "binary_sha256": derived_build["binary_sha256"],
            "helper_sha256": helper_sha256,
            "owner_helper_sha256": owner_helper_sha256,
            "range_count": str(correction.EXPECTED_RANGE_COUNT),
            "outcome_count": str(correction.EXPECTED_OUTCOME_COUNT),
        }
        correction_record = correction.create_receipt_record(
            source,
            derived_build_receipt,
            derived_build,
            derived_capture_receipt,
            original_capture_receipt,
            original_exact_capture_receipt,
            self.runner,
        )
        correction_receipt = self.root / "derived-reference-correction.receipt"
        correction_receipt.write_bytes(correction.receipt_bytes(correction_record))
        source_receipt = self.source_receipt
        reference_exact_build_receipt = self.root / "original-exact-build.receipt"
        reference_exact_build_receipt.write_text("original exact build\n", encoding="utf-8")
        candidate_exact_build_receipt = self.root / "candidate-exact-build.receipt"
        candidate_exact_build_receipt.write_text("candidate exact build\n", encoding="utf-8")
        candidate_exact_capture_receipt = self.root / "candidate-exact-capture.receipt"
        candidate_exact_capture_receipt.write_text("candidate exact capture\n", encoding="utf-8")
        owners = {index: (({}, {}, {}, {}), {}) for index in range(1690)}
        assignment_capture_receipt = self.root / "assignment-facts-capture.receipt"
        assignment_capture_receipt.write_text("assignment proof fixture\n", encoding="utf-8")
        assignment_proofs = {index: {} for index in expected_inputs}
        assignment_module = SimpleNamespace()
        proof_calls = []

        def validate_proof(
            _candidate_root,
            _source_receipt,
            _reference_source_root,
            role,
            _build_receipt,
            _exact_capture_receipt,
            proof_output,
            _bootstrap_runner,
            dry_run=False,
        ):
            self.assertFalse(dry_run)
            proof_output.mkdir()
            adapter_capture = (
                original_capture_receipt if role == "reference" else candidate_capture_receipt
            )
            proof_calls.append((role, adapter_capture))
            return {
                "capture": {
                    "schema": "2",
                    "adapter_capture_receipt": str(adapter_capture),
                    "adapter_capture_receipt_sha256": self.runner.sha256_file(adapter_capture),
                },
                "owners": owners,
            }

        first_delta = ("0", expected_inputs[0].relative_path, "default", "diagnostics", "a" * 64, "b" * 64)

        def source_identity(path):
            if pathlib.Path(path) == self.controller_root:
                return self.controller_commit, self.controller_tree
            if pathlib.Path(path) == derived_source_root:
                return correction.SOURCE_COMMIT, correction.SOURCE_TREE
            return self.source_commit, self.source_tree

        def compare_full(*args, **kwargs):
            self.assertIsNone(args[1])
            self.assertTrue(kwargs["corrected_reference"])
            self.assertEqual(kwargs["original_capture_receipt"], original_capture_receipt)
            self.assertEqual(len(kwargs["reference_exact_owners"]), 1690)
            self.assertIs(kwargs["candidate_assignment_proofs"], assignment_proofs)
            self.assertIs(kwargs["assignment_facts_module"], assignment_module)
            adapter_runner._write_new_file(args[6], b"", self.runner)
            return correction.EXPECTED_OUTCOME_COUNT, 0, first_delta

        args = argparse.Namespace(
            candidate_root=candidate_root,
            candidate_source_receipt=source_receipt,
            reference_source_root=original_source_root,
            reference_capture_receipt=original_capture_receipt,
            candidate_capture_receipt=candidate_capture_receipt,
            reference_exact_build_receipt=reference_exact_build_receipt,
            candidate_exact_build_receipt=candidate_exact_build_receipt,
            reference_exact_capture_receipt=original_exact_capture_receipt,
            candidate_exact_capture_receipt=candidate_exact_capture_receipt,
            assignment_facts_capture_receipt=assignment_capture_receipt,
            original_capture_receipt=original_capture_receipt,
            derived_reference_source_root=derived_source_root,
            derived_reference_correction_receipt=correction_receipt,
            output=self.controller_root / ".work/consolidation/source-qualified-comparison",
        )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=source_record),
            patch.object(self.runner, "source_identity", side_effect=source_identity),
            patch.object(self.runner, "validate_source_cohort_inputs", return_value=(expected_inputs, {})),
            patch.object(adapter_runner, "_prepare_reference_source", return_value=original_source_root),
            patch.object(
                adapter_runner,
                "_parse_adapter_build_receipt",
                return_value=derived_build,
            ),
            patch.object(
                adapter_runner,
                "_parse_capture_receipt",
                return_value=derived_capture,
            ),
            patch.object(
                correction,
                "validate_receipt_associations",
                wraps=correction.validate_receipt_associations,
            ) as validate_associations,
            patch.object(self.runner, "local_docker_environment", return_value={"PATH": os.environ["PATH"]}),
            patch.object(
                adapter_runner,
                "_load_exact_owner_runner",
                return_value=SimpleNamespace(
                    validate_proof=validate_proof,
                    validate_assignment_facts_capture=lambda *_args: (
                        assignment_proofs,
                        assignment_module,
                        {},
                    ),
                    ExactOwnerError=ValueError,
                ),
            ),
            patch.object(adapter_runner, "compare_full_publications", side_effect=compare_full),
            patch("builtins.print"),
        ):
            result = adapter_runner.compare_captured_publications(args, self.runner)

        self.assertEqual(result, 0)
        validate_associations.assert_called_once()
        self.assertEqual(
            proof_calls,
            [("reference", original_capture_receipt), ("candidate", candidate_capture_receipt)],
        )
        comparison_receipt = args.output / "comparison/derived-reference-comparison.receipt"
        fields = dict(
            line.split("\t", 1)
            for line in comparison_receipt.read_text(encoding="utf-8").splitlines()
        )
        self.assertEqual(fields["original_capture_receipt"], str(original_capture_receipt))
        self.assertEqual(fields["original_exact_capture_receipt"], str(original_exact_capture_receipt))
        self.assertEqual(fields["outcome_count"], str(correction.EXPECTED_OUTCOME_COUNT))

    def test_source_qualified_corrected_compare_aggregates_all_three_fresh_captures(self):
        correction = adapter_runner.reference_correction
        original_source_root, derived_source_root, patch_path, _correction_source = (
            self._local_correction_sources(correction)
        )
        expected_inputs = {
            index: self.runner.ManifestSource(
                f"source/{index:04}.xsh", f"{index + 1:064x}", 0o444
            )
            for index in range(1690)
        }
        def owner_text(source):
            sizes = source_fixtures.source_sizes(source)
            sizes.update({source_id: sizes.get(source_id, 0) for source_id in range(max(sizes) + 1)})
            owners = source_fixtures.owner_index(source)
            lines = [
                f"source\tSourceId({source_id})\t{size}"
                for source_id, size in sorted(sizes.items())
            ]
            lines.extend(
                f"owner\t{span}\t{kind}"
                for span, kinds in sorted(owners.items())
                for kind in sorted(kinds)
            )
            return "\n".join(lines) + "\n"

        reference_owner_text = owner_text(source_fixtures.OLD_SOURCE)
        candidate_owner_text = owner_text(source_fixtures.CURRENT_SOURCE)
        reference_owner_projection = adapter_runner.adapter.parse_owner_index(reference_owner_text)
        candidate_owner_projection = adapter_runner.adapter.parse_owner_index(candidate_owner_text)
        empty_exact_owners = dict(EMPTY_EXACT_OWNER_FACTS)
        empty_exact_owners.update(
            runs={}, calls={}, field_accesses={}, binding_initializers={}
        )
        reference_exact_owners = {
            index: (reference_owner_projection, empty_exact_owners)
            for index in expected_inputs
        }
        candidate_exact_owners = {
            index: (candidate_owner_projection, empty_exact_owners)
            for index in expected_inputs
        }
        reference_publication = TuiPublicationRunnerAlignmentTests()._schema_publication(
            source_fixtures.publication(source_fixtures.OLD_SOURCE)[0], "legacy"
        )
        candidate_publication = TuiPublicationRunnerAlignmentTests()._schema_publication(
            source_fixtures.publication(
                source_fixtures.CURRENT_SOURCE, reverse_maps=True
            )[0],
            "current",
        )
        builder_source_root = self.root / "builder-source"
        builder_source_root.mkdir()
        for relative_path in (
            pathlib.Path("stdlib/tui.xsh"),
            pathlib.Path("dev/consolidation/w9-publication-owner-helper.rs"),
        ):
            destination = builder_source_root / relative_path
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(self.repository / relative_path, destination)
        for command in (
            ["git", "init", "--quiet", str(builder_source_root)],
            ["git", "-C", str(builder_source_root), "config", "user.name", "Test"],
            ["git", "-C", str(builder_source_root), "config", "user.email", "test@example.invalid"],
            [
                "git", "-C", str(builder_source_root), "add",
                "stdlib/tui.xsh", "dev/consolidation/w9-publication-owner-helper.rs",
            ],
            ["git", "-C", str(builder_source_root), "commit", "--quiet", "-m", "test(fixtures): initialize builder source"],
        ):
            subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        (builder_source_root / ".git/info/exclude").write_text("/.work/\n", encoding="utf-8")
        builder_source_root_commit, builder_source_root_tree = self.runner.source_identity(
            builder_source_root
        )
        source_record = {
            "schema": self.runner.SOURCE_COHORT_SCHEMA,
            "candidate_root": str(builder_source_root),
            "candidate_commit": builder_source_root_commit,
            "candidate_tree": builder_source_root_tree,
            "controller_root": str(self.controller_root),
            "controller_commit": self.controller_commit,
            "controller_tree": self.controller_tree,
            "input_root": str(self.input_root),
            "input_manifest": str(self.input_manifest),
            "input_manifest_sha256": "a" * 64,
            "corpus_blake3": self.runner.CORPUS_BLAKE3,
            "builder_path": str(self.runner.SOURCE_BUILDER_PATH),
            "program_root": str(self.runner.PROGRAM_SNAPSHOT_ROOT.resolve()),
        }
        capture_roots = {}
        builds = {}
        captures = {}
        build_paths = {}
        capture_paths = {}
        digest_pair = ("d" * 64, "e" * 64)
        test_output_root = self.controller_root / ".work/captures/aggregate-compare"
        test_output_root.mkdir(parents=True)

        def write_ranges(receipt_directory):
            for start, end in self.runner.expected_ranges(len(expected_inputs)):
                shard = start // 100
                lines = [
                    f"corpus={len(expected_inputs)}",
                    f"shard={shard}/{(len(expected_inputs) + 99) // 100}",
                    f"range={start}..{end}",
                    "shard_size=100",
                    f"input_digest={self.runner.CORPUS_BLAKE3}",
                    self.runner.OPTION_HEADER,
                ]
                for index in range(start, end):
                    source = expected_inputs[index]
                    input_digest = f"{index + 1:064x}"
                    lines.append(f"input\t{index}\t{source.relative_path}\t{input_digest}")
                    for profile in adapter_runner.PROFILES:
                        lines.append(
                            f"output\t{index}\t{source.relative_path}\t{profile}\t"
                            f"{digest_pair[0]}\t{digest_pair[1]}"
                        )
                lines.append(f"terminal\tshard={shard}\tcompleted={end - start}")
                receipt = self.runner.candidate_receipt_path(receipt_directory, start, end)
                receipt.write_text("\n".join(lines) + "\n", encoding="utf-8")

        for role, source_root, commit, tree in (
            ("derived-reference", derived_source_root, correction.SOURCE_COMMIT, correction.SOURCE_TREE),
            ("original-reference", original_source_root, correction.BASE_COMMIT, correction.BASE_TREE),
            ("candidate", builder_source_root, builder_source_root_commit, builder_source_root_tree),
        ):
            capture_root = test_output_root / f"aggregate-{role}"
            raw_root = capture_root / "raw"
            receipt_directory = capture_root / "receipts"
            capture_root.mkdir()
            raw_root.mkdir(parents=True)
            (raw_root / "owners-0.tsv").write_text(
                candidate_owner_text if role == "candidate" else reference_owner_text,
                encoding="utf-8",
            )
            receipt_directory.mkdir()
            write_ranges(receipt_directory)
            build_path = self.root / f"aggregate-{role}.build"
            build_path.write_text(f"build {role}\n", encoding="utf-8")
            build_paths[role] = build_path
            builds[build_path] = {
                "schema": "2",
                "role": "derived-reference" if role == "derived-reference" else role,
                "source_root": str(source_root),
                "source_commit": commit,
                "source_tree": tree,
                "binary": self.binary,
                "helper_sha256": "c" * 64,
                "owner_helper_sha256": "d" * 64,
            }
            capture_path = capture_root / "capture.receipt"
            capture_path.write_text(
                "schema\t2\n"
                f"build_receipt\t{build_path}\n"
                f"capture_root\t{capture_root}\n"
                f"raw_root\t{raw_root}\n"
                f"receipt_directory\t{receipt_directory}\n",
                encoding="utf-8",
            )
            capture_paths[role] = capture_path
            capture_roots[role] = (capture_root, raw_root, receipt_directory)
            captures[capture_path] = {
                "schema": "2",
                "role": "derived-reference" if role == "derived-reference" else role,
                "source_root": str(source_root),
                "source_commit": commit,
                "source_tree": tree,
                "raw_root": str(raw_root),
                "receipt_directory": str(receipt_directory),
                "capture_root": str(capture_root),
            }

        raw_hashes = {
            (index, profile, field): digest
            for index in expected_inputs
            for profile in adapter_runner.PROFILES
            for field, digest in zip(("diagnostics", "publication"), digest_pair)
        }
        planned_container_names = []

        def plan_hash_job(job, active, command, cwd, environment):
            self.assertEqual(command[4], "run")
            planned_container_names.append(
                next(argument.split("=", 1)[1] for argument in command if argument.startswith("W9_CONTAINER_NAME="))
            )
            plan_command = list(command)
            plan_command[4] = "plan-run"
            result = subprocess.run(
                plan_command,
                cwd=cwd,
                env=environment,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            job["output"].write(result.stderr if result.returncode else b"builder plan accepted\n")
            job["process"] = SimpleNamespace(
                pid=2**30,
                returncode=result.returncode,
                poll=lambda: result.returncode,
                wait=lambda timeout=None: None,
            )
            active.append(job)

        mismatch_root = test_output_root / "aggregate-comparison"
        mismatch_root.mkdir()
        mismatch_path = mismatch_root / "mismatches.tsv"
        per_input_results = []

        def compare_inputs(context, count, callback):
            result = adapter_runner._compare_publication_input(0, context)
            per_input_results.append((0, result))
            callback(0, result)
            for index in range(1, count):
                callback(index, ())

        candidate_raw_inode = capture_roots["candidate"][1].stat().st_ino

        def read_record(root_fd, _key):
            publication = (
                candidate_publication
                if os.fstat(root_fd).st_ino == candidate_raw_inode
                else reference_publication
            )
            return "checked", "[]", publication, digest_pair

        with (
            patch.object(self.runner, "regular_file", side_effect=lambda path, *_args, **_kwargs: pathlib.Path(path)),
            patch.object(self.runner, "require_sha256", return_value="a" * 64),
            patch.object(self.runner, "validate_source_cohort_inputs", return_value=(expected_inputs, {})),
            patch.object(self.runner, "validate_reference_receipt", side_effect=AssertionError("legacy baseline used")),
            patch.object(adapter_runner, "_parse_adapter_build_receipt", side_effect=lambda path, *_args: builds[path]),
            patch.object(adapter_runner, "_parse_capture_receipt", side_effect=lambda path, *_args: captures[path]),
            patch.object(self.runner, "existing_container", return_value=False),
            patch.object(self.runner, "inspect_owned_containers", return_value=()),
            patch.object(self.runner, "start_and_register_job", side_effect=plan_hash_job),
            patch.object(adapter_runner, "_parse_raw_hash_output", return_value=raw_hashes),
            patch.object(adapter_runner, "_record", side_effect=read_record),
            patch.object(
                adapter_runner,
                "_run_parallel_publication_comparisons",
                side_effect=compare_inputs,
            ),
            patch.object(adapter_runner, "_load_publication_runner", return_value=self.runner),
            patch("builtins.print"),
        ):
            result = adapter_runner.compare_full_publications(
                self.runner,
                None,
                capture_roots["candidate"][2],
                capture_roots["derived-reference"][1],
                capture_roots["candidate"][1],
                expected_inputs,
                mismatch_path,
                capture_paths["derived-reference"],
                capture_paths["candidate"],
                builder_source_root,
                derived_source_root,
                source_record,
                self.input_root,
                self.input_manifest,
                {"PATH": os.environ["PATH"]},
                reference_exact_owners=reference_exact_owners,
                candidate_exact_owners=candidate_exact_owners,
                corrected_reference=True,
                original_capture_receipt=capture_paths["original-reference"],
                original_reference_source_root=original_source_root,
            )

        self.assertEqual(result, (correction.EXPECTED_OUTCOME_COUNT, 0, None))
        self.assertEqual(mismatch_path.read_bytes(), b"")
        self.assertEqual(per_input_results, [(0, ())])
        self.assertEqual(
            [name.rsplit("-hash-", 1)[1] for name in planned_container_names],
            ["derived-reference", "candidate", "reference"],
        )


class AdapterRawOutcomeContinuationTests(unittest.TestCase):
    def test_build_and_capture_receipt_headers_keep_order_and_reject_missing_fields(self):
        for fields, label in (
            (adapter_runner.ADAPTER_BUILD_FIELDS, "adapter build"),
            (adapter_runner.CAPTURE_FIELDS, "adapter capture"),
        ):
            lines = [f"{field}\tvalue" for field in fields]
            parsed, remainder = adapter_runner._parse_receipt_header(
                pathlib.Path("receipt"), lines, fields, label
            )
            self.assertEqual(tuple(parsed), fields)
            self.assertEqual(remainder, [])
            with self.assertRaisesRegex(adapter_runner.PublicationError, "malformed"):
                adapter_runner._parse_receipt_header(
                    pathlib.Path("receipt"), lines[1:] + lines[:1], fields, label
                )
            with self.assertRaisesRegex(adapter_runner.PublicationError, "incomplete"):
                adapter_runner._parse_receipt_header(
                    pathlib.Path("receipt"), lines[:-1], fields, label
                )

    def test_new_adapter_output_files_do_not_follow_leaf_symlinks(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        case_root = pathlib.Path(tempfile.mkdtemp(prefix="w9-adapter-write-symlink-", dir=scratch_root))
        try:
            target = case_root / "outside"
            target.write_text("original", encoding="utf-8")
            link = case_root / "record"
            link.symlink_to(target)
            runner = adapter_runner._load_publication_runner(
                repository / "dev/consolidation/w9-publication-run.py"
            )
            with self.assertRaises(FileExistsError):
                adapter_runner._write_new_file(link, b"changed", runner)
            self.assertEqual(target.read_text(encoding="utf-8"), "original")
        finally:
            shutil.rmtree(case_root)

    def test_derived_helper_keeps_pins_and_uses_native_hash_mode(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="w9-adapter-derived-", dir=scratch_root) as scratch:
            output = pathlib.Path(scratch) / "derived"
            helper, helper_hash, owner_hash, owner_source_hash = adapter_runner.derive_helper(
                approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
                output_directory=output,
                output_root=repository,
            )
            source = helper.read_text(encoding="utf-8")
            owner = (output / "w9-publication-owner-helper.rs").read_text(encoding="utf-8")

            self.assertEqual(len(helper_hash), 64)
            self.assertEqual(len(owner_hash), 64)
            self.assertEqual(owner_source_hash, adapter_runner.OWNER_HELPER_SHA256)
            self.assertEqual(
                hashlib.sha256(adapter_runner.OWNER_HELPER.read_bytes()).hexdigest(),
                adapter_runner.OWNER_HELPER_SHA256,
            )
            self.assertEqual(
                owner.count("xsh::execution::value::Value::Record(fields)"),
                2,
            )
            self.assertNotIn("xsh::runtime::value::Value::Record(fields)", owner)
            self.assertIn("W9_ADAPTER_HASH_ROOT", source)
            self.assertIn("w9_hash_adapter_tree", source)
            self.assertEqual(
                source.count('let publication_facts = w9_canonical_debug(&format!("{checked:?}"));'),
                1,
            )
            self.assertNotIn(
                'let publication_facts = w9_canonical_debug(&format!("{checked:#?}"));',
                source,
            )
            self.assertNotIn("create_dir_all(outputdir)", owner)
            self.assertIn("w9_create_new_adapter_file", owner)
            self.assertIn("w9_resolve_project_module_config(&program_entry, &program_root)", source)
            self.assertIn("let roots = resolved_config.module_roots.clone();", source)
            self.assertIn("&program_root, &resolved_config", source)
            self.assertNotIn("project_module_roots(&program_entry)", source)

    def test_retry_uses_a_new_retained_derived_helper_directory(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        case_root = pathlib.Path(tempfile.mkdtemp(prefix="w9-adapter-retry-", dir=scratch_root))
        try:
            first_directory = adapter_runner._new_derived_helper_directory(case_root, runner)
            first_helper, first_hash, _, _ = adapter_runner.derive_helper(
                approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
                output_directory=first_directory,
                output_root=case_root,
            )
            first_contents = first_helper.read_bytes()
            first_binary, first_receipt = adapter_runner._adapter_build_output_paths(
                case_root,
                first_helper,
                "reference",
                runner.REFERENCE_HEAD,
                runner,
            )
            adapter_runner._write_new_file(first_binary, b"first binary", runner)
            adapter_runner._write_new_file(first_receipt, b"first receipt", runner)

            retry_directory = adapter_runner._new_derived_helper_directory(case_root, runner)
            retry_helper, retry_hash, _, _ = adapter_runner.derive_helper(
                approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
                output_directory=retry_directory,
                output_root=case_root,
            )
            retry_binary, retry_receipt = adapter_runner._adapter_build_output_paths(
                case_root,
                retry_helper,
                "reference",
                runner.REFERENCE_HEAD,
                runner,
            )

            self.assertNotEqual(first_directory, retry_directory)
            self.assertEqual(first_directory.parent.parent, case_root / ".work/consolidation")
            self.assertEqual(retry_directory.parent.parent, first_directory.parent.parent)
            self.assertTrue(first_directory.is_dir())
            self.assertTrue(retry_directory.is_dir())
            self.assertNotEqual(first_binary, retry_binary)
            self.assertNotEqual(first_receipt, retry_receipt)
            self.assertEqual(first_binary.parent, first_directory.parent)
            self.assertEqual(retry_binary.parent, retry_directory.parent)
            self.assertEqual(first_binary.read_bytes(), b"first binary")
            self.assertEqual(first_receipt.read_bytes(), b"first receipt")
            self.assertEqual(first_helper.read_bytes(), first_contents)
            self.assertEqual(len(first_hash), 64)
            self.assertEqual(len(retry_hash), 64)
        finally:
            shutil.rmtree(case_root)

    def test_adapter_overlay_is_derived_from_clean_semantic_tests(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        source = b"#[test]\nfn existing_test() {}\n"
        overlay = adapter_runner._adapter_overlay(source, runner)

        self.assertTrue(overlay.startswith(source))
        self.assertEqual(
            overlay.count(b'include!("../.work/consolidation/w9-publication-adapter.rs");'),
            1,
        )
        self.assertEqual(source.count(runner.PARITY_TEST_INCLUDE), 0)

    def test_reference_source_requires_a_clean_pinned_clone(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="w9-adapter-reference-source-", dir=scratch_root) as scratch:
            workspace = pathlib.Path(scratch)
            worktrees = workspace / ".work/wt"
            candidate_root = worktrees / "candidate"
            reference_root = worktrees / "w9-adapter-ref-e240"
            historical_root = workspace / "historical-reference"
            candidate_root.mkdir(parents=True)
            historical_root.mkdir()
            commands = (
                ["git", "init", "--quiet", str(historical_root)],
                ["git", "-C", str(historical_root), "config", "user.name", "Test"],
                ["git", "-C", str(historical_root), "config", "user.email", "test@example.invalid"],
            )
            for command in commands:
                subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            tests = historical_root / "tests/sema.rs"
            tests.parent.mkdir()
            committed = b"#[test]\nfn committed_test() {}\n"
            tests.write_bytes(committed)
            subprocess.run(
                ["git", "-C", str(historical_root), "add", "tests/sema.rs"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            subprocess.run(
                ["git", "-C", str(historical_root), "commit", "--quiet", "-m", "test(fixtures): initialize historical source"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            commit = subprocess.run(
                ["git", "-C", str(historical_root), "rev-parse", "HEAD"],
                check=True,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            ).stdout.strip()
            tree = subprocess.run(
                ["git", "-C", str(historical_root), "rev-parse", "HEAD^{tree}"],
                check=True,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            ).stdout.strip()
            historical_edit = b"#[test]\nfn historical_overlay() {}\n"
            tests.write_bytes(historical_edit)
            subprocess.run(
                [
                    "git", "clone", "--quiet", "--no-local", "--no-checkout", "--depth=1",
                    str(historical_root), str(reference_root),
                ],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            subprocess.run(
                ["git", "-C", str(reference_root), "checkout", "--quiet", "--detach", commit],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            runner = adapter_runner._load_publication_runner(
                repository / "dev/consolidation/w9-publication-run.py"
            )
            with (
                patch.object(runner, "REFERENCE_HEAD", commit),
                patch.object(adapter_runner, "REFERENCE_TREE", tree),
            ):
                clean_source = adapter_runner._prepare_reference_source(
                    candidate_root, reference_root, runner
                )
                self.assertEqual(runner.source_identity(clean_source), (commit, tree))
                self.assertEqual((clean_source / "tests/sema.rs").read_bytes(), committed)
                self.assertEqual(tests.read_bytes(), historical_edit)
                with self.assertRaisesRegex(adapter_runner.PublicationError, "approved shared worktree path"):
                    adapter_runner._prepare_reference_source(candidate_root, historical_root, runner)

    def test_hash_mode_registers_a_limited_job_and_cleans_failed_processes(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="w9-adapter-hash-job-", dir=scratch_root) as scratch:
            root = pathlib.Path(scratch)
            candidate_root = root / "candidate"
            (candidate_root / ".work/raw").mkdir(parents=True)
            capture_output = root / "capture"
            (capture_output / "hashjobs").mkdir(parents=True)
            binary = root / "harness"
            binary.write_bytes(b"harness")
            wrapper = root / "wrapper"
            wrapper.write_text("#!/bin/sh\n", encoding="utf-8")
            wrapper.chmod(0o755)
            source_record = {
                "wrapper": str(wrapper),
                "wrapper_root": str(candidate_root),
                "candidate_root": str(candidate_root),
                "input_root": str(candidate_root / "inputs"),
            }
            environment = {"PATH": "/usr/bin:/bin"}
            commands = []
            cleanup_calls = []
            launches = 0

            def start_job(job, active, command, _cwd, _environment):
                nonlocal launches
                commands.append(command)
                launches += 1
                if launches == 2:
                    job["process"] = SimpleNamespace(poll=lambda: 1, returncode=1)
                    active.append(job)
                    return
                for profile in adapter_runner.PROFILES:
                    job["output"].write(
                        ("w9rawhash\t0\t" + profile + "\tdiagnostics\t" + "1" * 64 + "\n").encode()
                    )
                    job["output"].write(
                        ("w9rawhash\t0\t" + profile + "\tpublication\t" + "2" * 64 + "\n").encode()
                    )
                job["process"] = SimpleNamespace(poll=lambda: 0, returncode=0)
                active.append(job)

            with (
                patch.object(runner, "regular_file", side_effect=lambda path, *_args, **_kwargs: pathlib.Path(path)),
                patch.object(runner, "real_directory", side_effect=lambda path, *_args, **_kwargs: pathlib.Path(path)),
                patch.object(runner, "validate_wrapper_root"),
                patch.object(runner, "require_sha256", return_value="a" * 64),
                patch.object(runner, "existing_container", return_value=False),
                patch.object(runner, "inspect_owned_containers", return_value=()),
                patch.object(runner, "start_and_register_job", side_effect=start_job),
                patch.object(runner, "stop_jobs", side_effect=lambda active, _env: (cleanup_calls.append(len(active)), active.clear())),
            ):
                hashes = adapter_runner._hash_raw_outcomes(
                    binary, candidate_root / ".work/raw", source_record, 1,
                    capture_output, "reference", 0, environment, runner,
                )
                self.assertTrue(adapter_runner._raw_hash_pair_matches(
                    hashes, 0, "default", ("1" * 64, "2" * 64)
                ))
                with self.assertRaisesRegex(adapter_runner.PublicationError, "failed with status 1"):
                    adapter_runner._hash_raw_outcomes(
                        binary, candidate_root / ".work/raw", source_record, 1,
                        capture_output, "candidate", 1, environment, runner,
                    )

            self.assertEqual(len(commands), 2)
            self.assertEqual(commands[0][0:2], ["timeout", "900s"])
            self.assertIn("--cpuset-cpus=0", (capture_output / "hashjobs/reference/docker-shim/docker").read_text())
            self.assertEqual(cleanup_calls, [1])


class DerivedCaptureClaimTests(unittest.TestCase):
    def _candidate_root(self, scratch_root):
        candidate_root = scratch_root / "candidate"
        candidate_root.mkdir()
        (candidate_root / ".work").mkdir()
        subprocess.run(["git", "init", "--quiet", str(candidate_root)], check=True)
        return candidate_root

    def test_derived_capture_claims_output_before_recording_failure_status(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        with tempfile.TemporaryDirectory(prefix="w9-derived-claim-", dir=scratch_root) as scratch:
            root = pathlib.Path(scratch)
            candidate_root = self._candidate_root(root)
            derived_root = root / "derived"
            derived_root.mkdir()
            output = candidate_root / ".work/capture"
            original_capture_path = root / "original-capture.receipt"
            original_build_path = root / "original-build.receipt"
            original_exact_build_path = root / "original-exact-build.receipt"
            original_exact_capture_path = root / "original-exact-capture.receipt"
            derived_build_path = root / "derived-build.receipt"
            candidate_commit, candidate_tree = "1" * 40, "2" * 40
            derived_commit, derived_tree = "3" * 40, "4" * 40
            source_record = {
                "candidate_root": str(candidate_root),
                "candidate_commit": candidate_commit,
                "candidate_tree": candidate_tree,
                "input_root": str(root / "inputs"),
                "input_manifest": str(root / "inputs.manifest"),
                "wrapper": str(root / "wrapper"),
                "wrapper_root": str(candidate_root),
            }
            build_records = {
                original_build_path: {
                    "source_root": str(root / "original"),
                    "source_commit": runner.REFERENCE_HEAD,
                    "source_tree": adapter_runner.REFERENCE_TREE,
                    "helper_sha256": "a" * 64,
                    "owner_helper_sha256": "b" * 64,
                    "binary": str(root / "original-binary"),
                },
                original_exact_build_path: {
                    "source_root": str(root / "original"),
                    "source_commit": runner.REFERENCE_HEAD,
                    "source_tree": adapter_runner.REFERENCE_TREE,
                    "helper_sha256": "a" * 64,
                    "owner_helper_sha256": "b" * 64,
                    "binary": str(root / "exact-binary"),
                },
                derived_build_path: {
                    "source_root": str(derived_root),
                    "source_commit": derived_commit,
                    "source_tree": derived_tree,
                    "helper_sha256": "a" * 64,
                    "owner_helper_sha256": "b" * 64,
                    "binary": str(root / "derived-binary"),
                },
            }
            exact_owner_runner = SimpleNamespace(
                _load_exact_helper=lambda _runner: (None, b"", "c" * 64),
                _parse_exact_capture_receipt=lambda *_args: (
                    {
                        "schema": "1",
                        "legacy_capture_receipt": str(original_capture_path),
                        "legacy_capture_receipt_sha256": "d" * 64,
                    },
                    (),
                    {},
                ),
            )
            args = argparse.Namespace(
                candidate_root=candidate_root,
                candidate_source_receipt=root / "candidate.receipt",
                derived_reference_source_root=derived_root,
                original_capture_receipt=original_capture_path,
                original_exact_build_receipt=original_exact_build_path,
                original_exact_capture_receipt=original_exact_capture_path,
                derived_reference_build_receipt=derived_build_path,
                output=output,
            )
            expected_inputs = dict.fromkeys(range(1690))
            source_root_patch = patch.object(
                adapter_runner.reference_correction, "SOURCE_ROOT", derived_root
            )
            source_root_patch.start()
            self.addCleanup(source_root_patch.stop)
            with (
                patch.object(runner, "regular_file", side_effect=lambda path, *_args, **_kwargs: pathlib.Path(path)),
                patch.object(runner, "parse_candidate_receipt", return_value=source_record),
                patch.object(runner, "validate_all_inputs", return_value=(expected_inputs, {}, object())),
                patch.object(
                    runner,
                    "source_identity",
                    return_value=(candidate_commit, candidate_tree),
                ),
                patch.object(adapter_runner.reference_correction, "validate_source", return_value={
                    "source_commit": derived_commit,
                    "source_tree": derived_tree,
                }),
                patch.object(adapter_runner, "_capture_field", return_value=original_build_path),
                patch.object(
                    adapter_runner,
                    "_parse_adapter_build_receipt",
                    side_effect=lambda path, *_args: build_records[path],
                ),
                patch.object(
                    adapter_runner,
                    "_reuse_reference_capture",
                    return_value=(root / "raw", root / "receipts", {}),
                ),
                patch.object(adapter_runner, "_load_exact_owner_runner", return_value=exact_owner_runner),
                patch.object(runner, "sha256_file", return_value="d" * 64),
                patch.object(runner, "require_sha256", return_value="e" * 64),
                patch.object(runner, "validate_wrapper_root"),
                patch.object(runner, "local_docker_environment", return_value={}),
                patch.object(runner, "expected_ranges", return_value=[(0, 1690)]),
                patch.object(adapter_runner, "_publication_bytes_by_range", return_value={(0, 1690): 0}),
                patch.object(
                    adapter_runner,
                    "_run_capture_role",
                    side_effect=RuntimeError("capture interrupted before Docker launch"),
                ),
            ):
                with self.assertRaisesRegex(RuntimeError, "capture interrupted before Docker launch"):
                    adapter_runner.capture_derived_reference(args, runner)

                self.assertTrue(output.is_dir())
                status = (output / "attempt-status.txt").read_text(encoding="utf-8")
                self.assertIn("status=incomplete\n", status)
                self.assertIn("detail=RuntimeError: capture interrupted before Docker launch\n", status)
                self.assertFalse((candidate_root / ".work/capture.lock").exists())
                self.assertFalse((candidate_root / ".work/.w9-publication-run.lock").exists())

                foreign_output = candidate_root / ".work/foreign"
                foreign_link = candidate_root / ".work/foreign-link"
                foreign_target = candidate_root / ".work/foreign-target"
                foreign_target.mkdir()
                status_path = foreign_target / "attempt-status.txt"
                status_path.write_text("status=foreign\n", encoding="utf-8")
                require_output_path = runner.require_output_path
                for path, create_output in (
                    (foreign_output, lambda value: value.mkdir()),
                    (foreign_link, lambda value: value.symlink_to(foreign_target)),
                ):
                    def create_after_preflight(value, candidate, source_record=None):
                        checked = require_output_path(value, candidate, source_record)
                        create_output(checked)
                        return checked

                    args.output = path
                    with self.subTest(output=path.name):
                        with patch.object(
                            runner,
                            "require_output_path",
                            side_effect=create_after_preflight,
                        ):
                            with self.assertRaisesRegex(
                                runner.PublicationError,
                                "refusing to reuse existing output path",
                            ):
                                adapter_runner.capture_derived_reference(args, runner)
                        self.assertEqual(
                            status_path.read_text(encoding="utf-8"), "status=foreign\n"
                        )
                        self.assertFalse(
                            (candidate_root / ".work/.w9-publication-run.lock").exists()
                        )


class AdapterCaptureSchedulingTests(unittest.TestCase):
    def test_freed_slots_take_global_pending_ranges_while_one_worker_lags(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        ranges = runner.expected_ranges(200)
        launched = []
        slot_peaks = []

        class Process:
            def __init__(self, polls):
                self.polls = polls

            def poll(self):
                self.polls -= 1
                return 0 if self.polls == 0 else None

        class Output:
            closed = False

            def close(self):
                self.closed = True

        with tempfile.TemporaryDirectory(prefix="w9-dynamic-capture-", dir=scratch_root) as scratch:
            root = pathlib.Path(scratch)
            candidate_root = root / "candidate"
            candidate_root.mkdir()
            output = root / "capture"
            output.mkdir()
            source_record = {
                "candidate_root": str(candidate_root),
                "input_root": str(root / "inputs"),
            }

            def prepare_raw(path, _candidate_root, _runner, _source_record=None):
                path = pathlib.Path(path)
                path.mkdir()
                return path

            def launch(
                _runner,
                _source_record,
                _output,
                _role,
                _build_record,
                _binary,
                _raw_root,
                receipt_directory,
                start,
                end,
                worker,
                active,
                _docker_environment,
            ):
                slot_peaks.append(len(active) + 1)
                job = {
                    "worker": worker,
                    "start": start,
                    "end": end,
                    "receipt": runner.candidate_receipt_path(receipt_directory, start, end),
                    "timing": root / f"{start}-{end}.timing",
                    "log": root / f"{start}-{end}.log",
                    "process": Process(3 if worker == "A" else 1),
                    "output": Output(),
                }
                active.append(job)
                launched.append((worker, (start, end)))
                return job

            with (
                patch.object(adapter_runner, "prepare_raw_output_directory", side_effect=prepare_raw),
                patch.object(adapter_runner, "_launch_capture_job", side_effect=launch),
                patch.object(runner, "regular_file", side_effect=lambda path, *_args, **_kwargs: pathlib.Path(path)),
                patch.object(runner, "write_status"),
                patch.object(
                    runner,
                    "discover_candidate_receipts",
                    return_value={
                        interval: runner.candidate_receipt_path(
                            output / "candidate/receipts", *interval
                        )
                        for interval in ranges
                    },
                ),
                patch.object(runner, "inspect_owned_containers", return_value=()),
                patch.object(adapter_runner.time, "sleep"),
            ):
                adapter_runner._run_capture_role(
                    runner,
                    source_record,
                    output,
                    "candidate",
                    {},
                    root / "harness",
                    200,
                    {"PATH": os.environ["PATH"]},
                )

        self.assertEqual(
            launched,
            [
                ("A", (0, 25)),
                ("B", (25, 50)),
                ("C", (50, 75)),
                ("D", (75, 100)),
                ("B", (100, 125)),
                ("C", (125, 150)),
                ("D", (150, 175)),
                ("B", (175, 200)),
            ],
        )
        self.assertEqual(sorted(interval for _worker, interval in launched), ranges)
        self.assertEqual(len(set(interval for _worker, interval in launched)), len(ranges))
        self.assertLessEqual(max(slot_peaks), len(adapter_runner.CAPTURE_SLOTS))


class AdapterReferenceReuseTests(unittest.TestCase):
    def _parallel_test_context(self):
        return adapter_runner._PublicationComparisonContext(
            -1,
            -1,
            {},
            None,
            {},
            {},
            {},
            {},
            {},
            b"",
            b"",
            None,
            None,
            False,
        )

    def setUp(self):
        self.repository = pathlib.Path(__file__).resolve().parents[2]
        self.runner = adapter_runner._load_publication_runner(
            self.repository / "dev/consolidation/w9-publication-run.py"
        )
        self.scratch_root = self.repository / ".work/tmp"
        self.scratch_root.mkdir(parents=True, exist_ok=True)
        self.case_root = pathlib.Path(
            tempfile.mkdtemp(prefix="w9-adapter-reference-reuse-", dir=self.scratch_root)
        )
        self.worktree_root = self.case_root / ".work/wt"
        self.helper_attempts = []
        allocate_helper = adapter_runner._new_derived_helper_directory

        def allocate(root, runner):
            directory = allocate_helper(root, runner)
            self.helper_attempts.append(directory.parent)
            return directory

        self.allocate_helper_patch = patch.object(
            adapter_runner, "_new_derived_helper_directory", side_effect=allocate
        )
        self.allocate_helper_patch.start()
        self.addCleanup(self._cleanup)
        self.addCleanup(self.allocate_helper_patch.stop)
        self.candidate_root = self.worktree_root / "candidate"
        self.owner_helper_source = (
            self.candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
        )
        self.candidate_commit, self.candidate_tree = self._initialize_source(
            self.candidate_root,
            b"#[test]\nfn candidate_source() {}\n",
            adapter_runner.OWNER_HELPER.read_bytes(),
            b"candidate publication runner",
        )
        self.runner.exclude_work_from_git(self.candidate_root)
        self.artifact_root = self.candidate_root / ".work/test"
        self.artifact_root.mkdir(parents=True)
        self.test_builder = self.artifact_root / "native-build-runner"
        self.test_builder.write_text(
            f"#!/bin/sh\n{self.runner.PINNED_IMAGE}\n", encoding="utf-8"
        )
        self.test_builder.chmod(0o700)
        self.test_wrapper = self.artifact_root / "native-test-wrapper"
        self.test_wrapper.write_text(
            f"#!/bin/sh\n{self.runner.PINNED_IMAGE}\n", encoding="utf-8"
        )
        self.test_wrapper.chmod(0o700)
        self.test_builder_sha256 = self.runner.sha256_file(self.test_builder)
        self.test_wrapper_sha256 = self.runner.sha256_file(self.test_wrapper)
        build_contract = self.runner.w9_build_contract()
        build_contract["W9_CACHE_BUILD_RUNNER_SHA256"] = self.test_builder_sha256
        self.test_runner_patches = (
            patch.object(self.runner, "APPROVED_BUILD_RUNNER", self.test_builder),
            patch.object(
                self.runner, "PINNED_BUILD_RUNNER_SHA256", self.test_builder_sha256
            ),
            patch.object(self.runner, "w9_build_contract", return_value=build_contract),
            patch.object(
                self.runner,
                "APPROVED_HELPER_SOURCE",
                adapter_runner.SOURCE_APPROVED_HELPER,
            ),
            patch.object(self.runner, "WRAPPER_SHA256", self.test_wrapper_sha256),
        )
        for runner_patch in self.test_runner_patches:
            runner_patch.start()
            self.addCleanup(runner_patch.stop)
        self.reference_root = self.worktree_root / "w9-adapter-ref-e240"
        self.reference_commit, self.reference_tree = self._initialize_source(
            self.reference_root,
            b"#[test]\nfn reference_source() {}\n",
            adapter_runner.OWNER_HELPER.read_bytes(),
            b"reference publication runner",
            {"stdlib/tui.xsh": source_fixtures.OLD_SOURCE},
        )
        self.reference_head_patch = patch.object(
            self.runner, "REFERENCE_HEAD", self.reference_commit
        )
        self.reference_tree_patch = patch.object(
            adapter_runner, "REFERENCE_TREE", self.reference_tree
        )
        self.reference_head_patch.start()
        self.reference_tree_patch.start()
        self.addCleanup(self.reference_tree_patch.stop)
        self.addCleanup(self.reference_head_patch.stop)
        self.expected_inputs = {
            index: self.runner.ManifestSource(
                f"source/{index:04}.xsh", f"{index + 1:064x}", 0o444
            )
            for index in range(1690)
        }
        self.reference = self._reference_receipt()
        self.helper = self._derive_helper()

    def _cleanup(self):
        for attempt in self.helper_attempts:
            shutil.rmtree(attempt, ignore_errors=True)
        shutil.rmtree(self.case_root, ignore_errors=True)

    def _initialize_source(
        self, root, source, owner_helper=None, publication_runner=None, extra_files=None
    ):
        root.mkdir(parents=True, exist_ok=True)
        tests = root / "tests/sema.rs"
        tests.parent.mkdir(parents=True)
        tests.write_bytes(source)
        add_paths = ["tests/sema.rs"]
        if owner_helper is not None:
            helper_source = root / "dev/consolidation/w9-publication-owner-helper.rs"
            helper_source.parent.mkdir(parents=True)
            helper_source.write_bytes(owner_helper)
            add_paths.append("dev/consolidation/w9-publication-owner-helper.rs")
        if publication_runner is not None:
            runner_source = root / "dev/consolidation/w9-publication-run.py"
            runner_source.parent.mkdir(parents=True, exist_ok=True)
            runner_source.write_bytes(publication_runner)
            add_paths.append("dev/consolidation/w9-publication-run.py")
        for relative_path, contents in (extra_files or {}).items():
            path = root / relative_path
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)
            add_paths.append(relative_path)
        for command in (
            ["git", "init", "--quiet", str(root)],
            ["git", "-C", str(root), "config", "user.name", "Test"],
            ["git", "-C", str(root), "config", "user.email", "test@example.invalid"],
            ["git", "-C", str(root), "add", *add_paths],
            [
                "git",
                "-C",
                str(root),
                "commit",
                "--quiet",
                "-m",
                "test(fixtures): initialize source",
            ],
        ):
            subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        commit = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout.strip()
        tree = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD^{tree}"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout.strip()
        return commit, tree

    def _derive_helper(self):
        directory = adapter_runner._new_derived_helper_directory(
            self.candidate_root, self.runner
        )
        return adapter_runner.derive_helper(
            approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
            output_directory=directory,
            output_root=self.candidate_root,
        )

    def _reference_receipt(self):
        inputs = {
            index: self.runner.ReceiptSource(source.relative_path, "b" * 64)
            for index, source in self.expected_inputs.items()
        }
        outcomes = {
            (index, source.relative_path, profile): ("1" * 64, "2" * 64)
            for index, source in self.expected_inputs.items()
            for profile in adapter_runner.PROFILES
        }
        return self.runner.ParsedReceipt((), inputs, outcomes, ())

    def _write_build_receipt(
        self,
        role,
        source_root,
        commit,
        tree,
        helper=None,
        helper_sha256=None,
        owner_source_path=None,
    ):
        helper, helper_sha, owner_sha, owner_source_sha = (
            self.helper if helper is None else helper
        )
        if helper_sha256 is not None:
            helper_sha = helper_sha256
        root_hash = hashlib.sha256(str(source_root).encode()).hexdigest()[:8]
        binary = self.artifact_root / "binaries" / f"{role}-{commit[:12]}-{root_hash}"
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_bytes(f"{role} harness".encode())
        binary.chmod(0o700)
        builder = self.runner.regular_file(
            self.runner.APPROVED_BUILD_RUNNER, "pinned adapter build runner", executable=True
        )
        contract = self.runner.w9_build_contract()
        source_path = pathlib.Path(owner_source_path or self.owner_helper_source)
        source_tests = self.runner.read_regular_bytes(source_root / "tests/sema.rs")
        record = {
            "schema": "1",
            "role": role,
            "source_root": str(source_root),
            "source_commit": commit,
            "source_tree": tree,
            "binary": str(binary),
            "binary_sha256": self.runner.sha256_file(binary),
            "builder_path": str(builder),
            "builder_sha256": self.runner.sha256_file(builder),
            "builder_image": contract["W9_CACHE_BUILD_IMAGE"],
            "build_target": contract["W9_CACHE_BUILD_TARGET"],
            "build_profile": contract["W9_CACHE_BUILD_PROFILE"],
            "approved_helper_path": str(self.runner.APPROVED_HELPER_SOURCE),
            "approved_helper_sha256": adapter_runner.APPROVED_HELPER_SHA256,
            "helper_path": str(helper),
            "helper_sha256": helper_sha,
            "owner_helper_path": str(helper.parent / "w9-publication-owner-helper.rs"),
            "owner_helper_sha256": owner_sha,
            "owner_helper_source_path": str(source_path),
            "owner_helper_source_sha256": self.runner.sha256_file(source_path),
            "overlay_sha256": hashlib.sha256(
                adapter_runner._adapter_overlay(source_tests, self.runner)
            ).hexdigest(),
        }
        receipt = self.artifact_root / "build-receipts" / f"{role}-{commit[:12]}-{root_hash}-{helper_sha[:8]}.receipt"
        receipt.parent.mkdir(parents=True, exist_ok=True)
        adapter_runner._write_adapter_build_receipt(receipt, record, self.runner)
        return receipt, record

    def _candidate_source_record(self):
        return {
            "candidate_root": str(self.candidate_root),
            "candidate_commit": self.candidate_commit,
            "candidate_tree": self.candidate_tree,
            "reference_receipt": str(self.artifact_root / "pinned-reference.receipt"),
            "input_root": str(self.runner.REFERENCE_INPUT_ROOT),
            "input_manifest": str(self.artifact_root / "input-files.sha256.tsv"),
            "wrapper": str(self.test_wrapper),
            "wrapper_sha256": self.test_wrapper_sha256,
            "wrapper_root": str(self.runner.CONTROLLER_ROOT),
        }

    def _capture_receipt(
        self, build_path, build_record, capture_root, role="reference", outcome_count=6760
    ):
        raw_root = capture_root / "raw"
        receipt_directory = capture_root / "receipts"
        raw_root.mkdir(parents=True)
        receipt_directory.mkdir()
        ranges = self.runner.expected_ranges(len(self.expected_inputs))
        range_receipts = {}
        for start, end in ranges:
            path = self.runner.candidate_receipt_path(receipt_directory, start, end)
            path.write_bytes(f"range {start}..{end}".encode())
            range_receipts[(start, end)] = path
        receipt = capture_root / "capture.receipt"
        adapter_runner._write_capture_receipt(
            receipt,
            role,
            build_path,
            build_record,
            capture_root,
            raw_root,
            receipt_directory,
            self.runner.REFERENCE_INPUT_ROOT,
            pathlib.Path(self._candidate_source_record()["input_manifest"]),
            range_receipts,
            self.runner,
        )
        if outcome_count != 6760:
            lines = self.runner.read_regular_text(receipt).splitlines()
            lines[adapter_runner.CAPTURE_FIELDS.index("outcome_count")] = f"outcome_count\t{outcome_count}"
            receipt.write_text("\n".join(lines) + "\n", encoding="utf-8")
        return receipt, range_receipts

    def _compare_case(self):
        reference_build_path, reference_build = self._write_build_receipt(
            "reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
        )
        candidate_build_path, _candidate_build = self._write_build_receipt(
            "candidate",
            self.candidate_root,
            self.candidate_commit,
            self.candidate_tree,
        )
        reference_capture, reference_ranges = self._capture_receipt(
            reference_build_path,
            reference_build,
            self.artifact_root / "reference-capture",
        )
        candidate_capture, candidate_ranges = self._capture_receipt(
            candidate_build_path,
            _candidate_build,
            self.artifact_root / "candidate-capture",
            role="candidate",
        )
        reference_exact_capture = self.artifact_root / "reference-exact-capture.receipt"
        candidate_exact_capture = self.artifact_root / "candidate-exact-capture.receipt"
        reference_exact_capture.write_text("reference exact capture fixture\n", encoding="utf-8")
        candidate_exact_capture.write_text("candidate exact capture fixture\n", encoding="utf-8")
        source_receipt = self.artifact_root / "candidate-source.receipt"
        source_receipt.write_text("fixture\n", encoding="utf-8")
        return {
            "candidate_source_receipt": source_receipt,
            "reference_build_receipt": reference_build_path,
            "candidate_build_receipt": candidate_build_path,
            "reference_exact_build_receipt": reference_build_path,
            "candidate_exact_build_receipt": candidate_build_path,
            "reference_capture_receipt": reference_capture,
            "candidate_capture_receipt": candidate_capture,
            "reference_exact_capture_receipt": reference_exact_capture,
            "candidate_exact_capture_receipt": candidate_exact_capture,
            "reference_ranges": reference_ranges,
            "candidate_ranges": candidate_ranges,
            "source_record": self._candidate_source_record(),
        }

    def _compare_args(self, case, suffix="compare"):
        return argparse.Namespace(
            candidate_root=self.candidate_root,
            candidate_source_receipt=case["candidate_source_receipt"],
            reference_source_root=self.reference_root,
            reference_exact_build_receipt=case["reference_exact_build_receipt"],
            candidate_exact_build_receipt=case["candidate_exact_build_receipt"],
            reference_capture_receipt=case["reference_capture_receipt"],
            candidate_capture_receipt=case["candidate_capture_receipt"],
            reference_exact_capture_receipt=case["reference_exact_capture_receipt"],
            candidate_exact_capture_receipt=case["candidate_exact_capture_receipt"],
            output=str(self.artifact_root / suffix),
        )

    def _assignment_facts_fixture(self, args):
        receipt = self.artifact_root / "assignment-facts-capture.receipt"
        receipt.write_text("assignment proof fixture\n", encoding="utf-8")
        args.assignment_facts_capture_receipt = receipt
        proofs = {index: {} for index in self.expected_inputs}
        module = SimpleNamespace()
        return proofs, module

    def _run_compare(self, case):
        return self._run_compare_to(case, "compare")

    def _run_compare_to(self, case, suffix):
        args = self._compare_args(case, suffix)
        return adapter_runner.compare_captured_publications(
            args,
            self.runner,
        )

    def _run_full_compare(self, case):
        original_regular_file = self.runner.regular_file
        original_require_sha256 = self.runner.require_sha256
        input_manifest = pathlib.Path(case["source_record"]["input_manifest"])

        def regular_file(path, kind, executable=False):
            if pathlib.Path(path) == input_manifest:
                return input_manifest
            return original_regular_file(path, kind, executable)

        def require_sha256(path, expected, _kind):
            if pathlib.Path(path) == input_manifest:
                self.assertEqual(expected, self.runner.INPUT_MANIFEST_SHA256)
                return expected
            return original_require_sha256(path, expected, _kind)

        with (
            patch.object(self.runner, "regular_file", side_effect=regular_file),
            patch.object(self.runner, "require_sha256", side_effect=require_sha256),
            patch.object(self.runner, "validate_reference_receipt", return_value=self.reference),
            patch.object(adapter_runner, "_hash_raw_outcomes") as hash_raw,
        ):
            result = adapter_runner.compare_full_publications(
                self.runner,
                pathlib.Path(case["source_record"]["reference_receipt"]),
                case["candidate_capture_receipt"].parent / "receipts",
                case["reference_capture_receipt"].parent / "raw",
                case["candidate_capture_receipt"].parent / "raw",
                self.expected_inputs,
                self.artifact_root / "mismatches.tsv",
                case["reference_capture_receipt"],
                case["candidate_capture_receipt"],
                self.candidate_root,
                self.reference_root,
                case["source_record"],
                pathlib.Path(case["source_record"]["input_root"]),
                input_manifest,
                {"PATH": os.environ["PATH"]},
                reference_exact_owners={
                    index: (({}, {}, {}, {}), {}) for index in self.expected_inputs
                },
                candidate_exact_owners={
                    index: (({}, {}, {}, {}), {}) for index in self.expected_inputs
                },
            )
        hash_raw.assert_not_called()
        return result

    def _parse_shard(self, path, _expected_inputs):
        match = re.fullmatch(r"parallel-(\d+)-(\d+)-all\.receipt", path.name)
        if match is None:
            raise AssertionError(path)
        start, end = int(match.group(1)), int(match.group(2)) + 1
        shard = start // 100
        count = len(self.expected_inputs)
        inputs = {
            index: self.reference.inputs[index]
            for index in range(start, end)
        }
        outcomes = {
            key: self.reference.outcomes[key]
            for index in range(start, end)
            for key in (
                (index, self.expected_inputs[index].relative_path, profile)
                for profile in adapter_runner.PROFILES
            )
        }
        header = (
            f"corpus={count}",
            f"shard={shard}/{(count + 99) // 100}",
            f"range={start}..{end}",
            "shard_size=100",
            f"input_digest={self.runner.CORPUS_BLAKE3}",
            self.runner.OPTION_HEADER,
        )
        terminals = (f"terminal\tshard={shard}\tcompleted={end - start}",)
        return self.runner.ParsedReceipt(header, inputs, outcomes, terminals)

    def test_build_reuse_rejects_historical_reference_build_receipt(self):
        owner_source = self.owner_helper_source
        reference_receipt, _record = self._write_build_receipt(
            "reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
            owner_source_path=owner_source,
        )
        source_receipt = self.artifact_root / "candidate-source.receipt"
        source_receipt.write_text("fixture\n", encoding="utf-8")
        source_record = self._candidate_source_record()
        source_record["schema"] = self.runner.SOURCE_COHORT_SCHEMA
        build_roles = []

        def build(_source, _candidate, role, *_args, **_kwargs):
            build_roles.append(role)
            return self.case_root / "candidate-harness", self.case_root / "candidate-build.receipt"

        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=source_record),
            patch.object(
                self.runner,
                "validate_source_cohort_inputs",
                return_value=(self.expected_inputs, {}),
            ),
            patch.object(adapter_runner, "build_adapter_harness", side_effect=build),
        ):
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "reused reference adapter build is not source-qualified",
            ):
                adapter_runner.build_adapter_harnesses(
                    argparse.Namespace(
                        candidate_root=self.candidate_root,
                        candidate_source_receipt=source_receipt,
                        reference_source_root=self.reference_root,
                        reference_build_receipt=reference_receipt,
                    ),
                    self.runner,
                )

        self.assertEqual(build_roles, [])

    def test_reference_source_reuse_requires_approved_sibling_path(self):
        other_root = self.worktree_root / "other/w9-adapter-ref-e240"
        other_root.parent.mkdir()
        subprocess.run(
            ["git", "clone", "--quiet", "--no-local", str(self.reference_root), str(other_root)],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "reference source clone must use the approved shared worktree path",
        ):
            adapter_runner._prepare_reference_source(
                self.candidate_root, other_root, self.runner
            )

    def test_reference_capture_reuse_rejects_partial_coverage(self):
        owner_source = self.owner_helper_source
        build_path, build = self._write_build_receipt(
            "reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
            owner_source_path=owner_source,
        )
        capture_root = self.artifact_root / "partial-reference-capture"
        raw_root = capture_root / "raw"
        receipt_directory = capture_root / "receipts"
        raw_root.mkdir(parents=True)
        receipt_directory.mkdir()
        record = {
            "schema": "1",
            "role": "reference",
            "source_root": build["source_root"],
            "source_commit": build["source_commit"],
            "source_tree": build["source_tree"],
            "binary": build["binary"],
            "binary_sha256": build["binary_sha256"],
            "build_receipt": str(build_path),
            "build_receipt_sha256": self.runner.sha256_file(build_path),
            "helper_sha256": build["helper_sha256"],
            "owner_helper_sha256": build["owner_helper_sha256"],
            "input_root": str(self.runner.REFERENCE_INPUT_ROOT),
            "input_manifest": self._candidate_source_record()["input_manifest"],
            "input_manifest_sha256": self.runner.INPUT_MANIFEST_SHA256,
            "corpus_blake3": self.runner.CORPUS_BLAKE3,
            "capture_root": str(capture_root),
            "raw_root": str(raw_root),
            "receipt_directory": str(receipt_directory),
            "range_count": "67",
            "outcome_count": "6700",
        }
        capture_receipt = capture_root / "capture.receipt"
        adapter_runner._write_new_file(
            capture_receipt,
            "".join(f"{field}\t{record[field]}\n" for field in adapter_runner.CAPTURE_FIELDS).encode(),
            self.runner,
        )

        with self.assertRaisesRegex(adapter_runner.PublicationError, "coverage differs"):
            adapter_runner._reuse_reference_capture(
                capture_receipt,
                build_path,
                build,
                self.expected_inputs,
                self.reference,
                self.runner.REFERENCE_INPUT_ROOT,
                pathlib.Path(record["input_manifest"]),
                self.runner,
            )

    def test_reference_capture_reuse_rejects_outcome_drift(self):
        build_path, build = self._write_build_receipt(
            "reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
        )
        capture_receipt, _ranges = self._capture_receipt(
            build_path, build, self.artifact_root / "outcome-drift-reference-capture"
        )

        def parse_with_changed_outcome(path, expected_inputs):
            parsed = self._parse_shard(path, expected_inputs)
            outcomes = dict(parsed.outcomes)
            key = min(outcomes)
            outcomes[key] = ("c" * 64, "d" * 64)
            return self.runner.ParsedReceipt(
                parsed.header, parsed.inputs, outcomes, parsed.terminals
            )

        with patch.object(self.runner, "parse_receipt", side_effect=parse_with_changed_outcome):
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "reused reference harness outcomes differ",
            ):
                adapter_runner._reuse_reference_capture(
                    capture_receipt,
                    build_path,
                    build,
                    self.expected_inputs,
                    self.reference,
                    self.runner.REFERENCE_INPUT_ROOT,
                    pathlib.Path(self._candidate_source_record()["input_manifest"]),
                    self.runner,
                )

    def test_run_rejects_inline_comparison_before_capturing(self):
        with patch.object(adapter_runner, "_run_capture_role") as capture:
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "run requires --capture-only; then capture exact owners and use compare-captures",
            ):
                adapter_runner.execute_adapter_publication(argparse.Namespace(), self.runner)

        capture.assert_not_called()

    def test_run_capture_only_reuses_reference_capture_and_writes_candidate_receipt(self):
        owner_source = self.owner_helper_source
        reference_build_path, reference_build = self._write_build_receipt(
            "reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
            owner_source_path=owner_source,
        )
        candidate_commit, candidate_tree = self.runner.source_identity(self.candidate_root)
        candidate_build_path, _candidate_build = self._write_build_receipt(
            "candidate",
            self.candidate_root,
            candidate_commit,
            candidate_tree,
        )
        reference_capture, reference_ranges = self._capture_receipt(
            reference_build_path, reference_build, self.artifact_root / "prior-reference-capture"
        )
        source_receipt = self.artifact_root / "candidate-source.receipt"
        source_receipt.write_text("fixture\n", encoding="utf-8")
        source_record = self._candidate_source_record()
        capture_roles = []
        capture_priorities = []
        reference_range_priority = {
            interval: index for index, interval in enumerate(self.runner.expected_ranges(1690))
        }

        def capture(
            runner,
            record,
            output,
            role,
            _build,
            _binary,
            _count,
            _environment,
            *,
            publication_bytes_by_range,
        ):
            capture_roles.append(role)
            capture_priorities.append(publication_bytes_by_range)
            capture_root = output / role
            raw_root = capture_root / "raw"
            receipt_directory = capture_root / "receipts"
            raw_root.mkdir(parents=True)
            receipt_directory.mkdir()
            range_receipts = {}
            for (start, end), source in reference_ranges.items():
                destination = runner.candidate_receipt_path(receipt_directory, start, end)
                destination.write_bytes(source.read_bytes())
                range_receipts[(start, end)] = destination
            return raw_root, receipt_directory, range_receipts

        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=source_record),
            patch.object(
                self.runner,
                "validate_all_inputs",
                return_value=(self.expected_inputs, {}, self.reference),
            ),
            patch.object(self.runner, "parse_receipt", side_effect=self._parse_shard),
            patch.object(self.runner, "local_docker_environment", return_value={"PATH": os.environ["PATH"]}),
            patch.object(adapter_runner, "_run_capture_role", side_effect=capture),
            patch.object(
                adapter_runner,
                "_publication_bytes_by_range",
                return_value=reference_range_priority,
            ) as publication_sizes,
            patch.object(adapter_runner, "compare_full_publications") as compare,
            patch("builtins.print") as printed,
        ):
            result = adapter_runner.execute_adapter_publication(
                argparse.Namespace(
                    candidate_root=self.candidate_root,
                    candidate_source_receipt=source_receipt,
                    reference_source_root=self.reference_root,
                    reference_build_receipt=reference_build_path,
                    candidate_build_receipt=candidate_build_path,
                    reference_capture_receipt=reference_capture,
                    output=str(self.candidate_root / ".work/consolidation/new-publication"),
                    capture_only=True,
                    dry_run=False,
                ),
                self.runner,
            )

        self.assertEqual(result, 0)
        self.assertEqual(capture_roles, ["candidate"])
        self.assertEqual(capture_priorities, [reference_range_priority])
        publication_sizes.assert_called_once()
        self.assertEqual(publication_sizes.call_args.args[1], len(self.expected_inputs))
        compare.assert_not_called()
        output = self.candidate_root / ".work/consolidation/new-publication"
        self.assertTrue((output / "candidate/capture.receipt").is_file())
        self.assertFalse((output / "reference/capture.receipt").exists())
        self.assertEqual(
            (output / "attempt-status.txt").read_text(encoding="utf-8").splitlines()[0],
            "status=capture-complete",
        )
        self.assertIn(f"reference_capture={reference_capture}", printed.call_args.args[0])
        self.assertIn(
            f"candidate_capture={output / 'candidate/capture.receipt'}",
            printed.call_args.args[0],
        )
        self.assertIn("comparison=not_run", printed.call_args.args[0])

    def test_compare_captures_rechecks_source_and_routes_both_roles_to_comparator(self):
        case = self._compare_case()
        args = self._compare_args(case)
        assignment_proofs, assignment_module = self._assignment_facts_fixture(args)
        comparisons = []
        proof_calls = []
        reference_exact_owners = {
            index: (({}, {}, {}, {}), {"exprs": {index: ("reference", "Call", "")}})
            for index in self.expected_inputs
        }
        candidate_exact_owners = {
            index: (({}, {}, {}, {}), {"exprs": {index: ("candidate", "Call", "")}})
            for index in self.expected_inputs
        }

        def validate_proof(
            candidate_root,
            source_receipt,
            reference_source_root,
            role,
            build_receipt,
            exact_capture_receipt,
            output,
            _bootstrap_runner,
            dry_run=False,
        ):
            self.assertFalse(dry_run)
            output.mkdir()
            legacy_capture = (
                case["reference_capture_receipt"]
                if role == "reference"
                else case["candidate_capture_receipt"]
            )
            proof_calls.append((role, build_receipt, exact_capture_receipt, output))
            return {
                "capture": {
                    "schema": "1",
                    "legacy_capture_receipt": str(legacy_capture),
                    "legacy_capture_receipt_sha256": self.runner.sha256_file(legacy_capture),
                },
                "owners": reference_exact_owners if role == "reference" else candidate_exact_owners,
            }

        def compare(*args, **kwargs):
            comparisons.append((args, kwargs))
            self.assertIs(kwargs["candidate_assignment_proofs"], assignment_proofs)
            self.assertIs(kwargs["assignment_facts_module"], assignment_module)
            args[6].write_text("", encoding="utf-8")
            return len(self.expected_inputs) * len(adapter_runner.PROFILES), 0

        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=case["source_record"]),
            patch.object(
                self.runner,
                "validate_all_inputs",
                return_value=(self.expected_inputs, {}, self.reference),
            ),
            patch.object(self.runner, "parse_receipt", side_effect=self._parse_shard),
            patch.object(self.runner, "local_docker_environment", return_value={"PATH": os.environ["PATH"]}),
            patch.object(
                adapter_runner,
                "_load_exact_owner_runner",
                return_value=SimpleNamespace(
                    validate_proof=validate_proof,
                    validate_assignment_facts_capture=lambda *_args: (
                        assignment_proofs,
                        assignment_module,
                        {},
                    ),
                    ExactOwnerError=ValueError,
                ),
            ),
            patch.object(adapter_runner, "compare_full_publications", side_effect=compare),
        ):
            result = adapter_runner.compare_captured_publications(args, self.runner)

        self.assertEqual(result, 0)
        self.assertEqual(len(comparisons), 1)
        self.assertEqual(comparisons[0][0][2], case["candidate_capture_receipt"].parent / "receipts")
        self.assertEqual(comparisons[0][0][3], case["reference_capture_receipt"].parent / "raw")
        self.assertEqual(comparisons[0][0][4], case["candidate_capture_receipt"].parent / "raw")
        self.assertEqual(
            comparisons[0][1]["reference_exact_owners"], reference_exact_owners
        )
        self.assertEqual(
            comparisons[0][1]["candidate_exact_owners"], candidate_exact_owners
        )
        self.assertEqual(
            proof_calls,
            [
                (
                    "reference",
                    case["reference_exact_build_receipt"],
                    case["reference_exact_capture_receipt"],
                    self.artifact_root / "compare/exact-reference",
                ),
                (
                    "candidate",
                    case["candidate_exact_build_receipt"],
                    case["candidate_exact_capture_receipt"],
                    self.artifact_root / "compare/exact-candidate",
                ),
            ],
        )
        output = self.artifact_root / "compare"
        self.assertEqual(
            (output / "attempt-status.txt").read_text(encoding="utf-8").splitlines()[0],
            "status=complete",
        )
        self.assertTrue((output / "comparison/attempt-status.txt").is_file())

    def test_compare_without_assignment_facts_proof_is_diagnostic_only(self):
        case = self._compare_case()
        args = self._compare_args(case)
        exact_owners = {
            index: (({}, {}, {}, {}), {}) for index in self.expected_inputs
        }

        def validate_proof(
            _candidate_root,
            _source_receipt,
            _reference_source_root,
            role,
            _build_receipt,
            _exact_capture_receipt,
            output,
            _bootstrap_runner,
            dry_run=False,
        ):
            self.assertFalse(dry_run)
            output.mkdir()
            legacy_capture = (
                case["reference_capture_receipt"]
                if role == "reference"
                else case["candidate_capture_receipt"]
            )
            return {
                "capture": {
                    "schema": "1",
                    "legacy_capture_receipt": str(legacy_capture),
                    "legacy_capture_receipt_sha256": self.runner.sha256_file(legacy_capture),
                },
                "owners": exact_owners,
            }

        def compare(*args, **_kwargs):
            args[6].write_text("", encoding="utf-8")
            return len(self.expected_inputs) * len(adapter_runner.PROFILES), 0

        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=case["source_record"]),
            patch.object(
                self.runner,
                "validate_all_inputs",
                return_value=(self.expected_inputs, {}, self.reference),
            ),
            patch.object(self.runner, "parse_receipt", side_effect=self._parse_shard),
            patch.object(self.runner, "local_docker_environment", return_value={"PATH": os.environ["PATH"]}),
            patch.object(
                adapter_runner,
                "_load_exact_owner_runner",
                return_value=SimpleNamespace(
                    validate_proof=validate_proof,
                    ExactOwnerError=ValueError,
                ),
            ),
            patch.object(adapter_runner, "compare_full_publications", side_effect=compare),
            patch("builtins.print"),
        ):
            result = adapter_runner.compare_captured_publications(args, self.runner)

        self.assertEqual(result, 1)
        self.assertEqual(
            (self.artifact_root / "compare/comparison/attempt-status.txt")
            .read_text(encoding="utf-8")
            .splitlines()[0],
            "status=diagnostic-only",
        )

    def test_compare_derived_captures_writes_delta_before_hashing_receipt(self):
        case = self._compare_case()
        derived_build_path, derived_build = self._write_build_receipt(
            "derived-reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
        )
        derived_capture_path, _derived_ranges = self._capture_receipt(
            derived_build_path,
            derived_build,
            self.artifact_root / "derived-reference-capture",
            role="derived-reference",
        )
        correction_receipt = self.artifact_root / "correction.receipt"
        correction_receipt.write_text("correction fixture\n", encoding="utf-8")
        correction = adapter_runner.reference_correction
        correction_source = {
            "base_commit": correction.BASE_COMMIT,
            "base_tree": correction.BASE_TREE,
            "patch_path": str(correction.PATCH_PATH),
            "patch_sha256": correction.PATCH_SHA256,
            "source_root": str(self.reference_root),
            "source_commit": self.reference_commit,
            "source_tree": self.reference_tree,
            "source_parent": correction.SOURCE_PARENT,
            "source_state": "clean",
        }
        correction_record = {
            **correction_source,
            "original_capture_receipt": str(case["reference_capture_receipt"]),
            "original_exact_capture_receipt": str(case["reference_exact_capture_receipt"]),
            "build_receipt": str(derived_build_path),
            "capture_receipt": str(derived_capture_path),
        }
        proof_calls = []
        reference_exact_owners = {
            index: (({}, {}, {}, {}), {"exprs": {index: ("reference", "Call", "")}})
            for index in self.expected_inputs
        }
        candidate_exact_owners = {
            index: (({}, {}, {}, {}), {"exprs": {index: ("candidate", "Call", "")}})
            for index in self.expected_inputs
        }
        first_delta = (
            "0",
            self.expected_inputs[0].relative_path,
            "default",
            "diagnostics",
            "a" * 64,
            "b" * 64,
        )

        def validate_proof(
            _candidate_root,
            _source_receipt,
            _reference_source_root,
            role,
            build_receipt,
            exact_capture_receipt,
            output,
            _bootstrap_runner,
            dry_run=False,
        ):
            self.assertFalse(dry_run)
            output.mkdir()
            legacy_capture = (
                case["reference_capture_receipt"]
                if role == "reference"
                else case["candidate_capture_receipt"]
            )
            proof_calls.append((role, build_receipt, exact_capture_receipt))
            return {
                "capture": {
                    "schema": "1",
                    "legacy_capture_receipt": str(legacy_capture),
                    "legacy_capture_receipt_sha256": self.runner.sha256_file(legacy_capture),
                },
                "owners": reference_exact_owners if role == "reference" else candidate_exact_owners,
            }

        def compare(*args, **_kwargs):
            self.assertEqual(args[6], self.artifact_root / "derived-compare/comparison/mismatches.tsv")
            adapter_runner._write_new_file(args[6], b"", self.runner)
            return 6760, 0, first_delta

        args = self._compare_args(case, "derived-compare")
        assignment_proofs, assignment_module = self._assignment_facts_fixture(args)
        args.derived_reference_source_root = self.reference_root
        args.derived_reference_correction_receipt = correction_receipt
        args.original_capture_receipt = case["reference_capture_receipt"]

        source_root_patch = patch.object(
            adapter_runner.reference_correction, "SOURCE_ROOT", self.reference_root
        )
        source_root_patch.start()
        self.addCleanup(source_root_patch.stop)
        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=case["source_record"]),
            patch.object(
                self.runner,
                "validate_all_inputs",
                return_value=(self.expected_inputs, {}, self.reference),
            ),
            patch.object(
                adapter_runner.reference_correction,
                "validate_source",
                return_value=correction_source,
            ),
            patch.object(
                adapter_runner.reference_correction,
                "parse_receipt",
                return_value=correction_record,
            ),
            patch.object(adapter_runner.reference_correction, "validate_receipt_associations"),
            patch.object(self.runner, "local_docker_environment", return_value={"PATH": os.environ["PATH"]}),
            patch.object(
                adapter_runner,
                "_load_exact_owner_runner",
                return_value=SimpleNamespace(
                    validate_proof=validate_proof,
                    validate_assignment_facts_capture=lambda *_args: (
                        assignment_proofs,
                        assignment_module,
                        {},
                    ),
                    ExactOwnerError=ValueError,
                ),
            ),
            patch.object(adapter_runner, "compare_full_publications", side_effect=compare),
            patch("builtins.print"),
        ):
            result = adapter_runner.compare_captured_publications(args, self.runner)

        self.assertEqual(result, 0)
        self.assertEqual(
            proof_calls,
            [
                ("reference", case["reference_exact_build_receipt"], case["reference_exact_capture_receipt"]),
                ("candidate", case["candidate_exact_build_receipt"], case["candidate_exact_capture_receipt"]),
            ],
        )
        output = self.artifact_root / "derived-compare/comparison"
        delta_path = output / "original-to-derived-reference.tsv"
        comparison_receipt = output / "derived-reference-comparison.receipt"
        self.assertEqual(
            delta_path.read_bytes(), correction.first_difference_bytes(first_delta)
        )
        receipt = dict(
            line.split("\t", 1) for line in comparison_receipt.read_text(encoding="utf-8").splitlines()
        )
        self.assertEqual(receipt["delta_file"], str(delta_path))
        self.assertEqual(receipt["delta_file_sha256"], self.runner.sha256_file(delta_path))
        self.assertEqual(receipt["outcome_count"], "6760")

    def test_compare_captures_rejects_exact_proof_for_a_different_legacy_capture(self):
        case = self._compare_case()
        wrong_capture = self.artifact_root / "unrelated-capture.receipt"
        wrong_capture.write_text("unrelated\n", encoding="utf-8")
        proof_calls = []

        def validate_proof(*args, **kwargs):
            role = args[3]
            proof_calls.append(role)
            args[6].mkdir()
            return {
                "capture": {
                    "schema": "1",
                    "legacy_capture_receipt": str(wrong_capture),
                    "legacy_capture_receipt_sha256": self.runner.sha256_file(wrong_capture),
                },
                "owners": {index: (({}, {}, {}, {}), {}) for index in self.expected_inputs},
            }

        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=case["source_record"]),
            patch.object(
                self.runner,
                "validate_all_inputs",
                return_value=(self.expected_inputs, {}, self.reference),
            ),
            patch.object(
                adapter_runner,
                "_load_exact_owner_runner",
                return_value=SimpleNamespace(
                    validate_proof=validate_proof,
                    ExactOwnerError=ValueError,
                ),
            ),
            patch.object(adapter_runner, "compare_full_publications") as compare,
        ):
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "exact-owner reference proof names a different adapter capture receipt",
            ):
                self._run_compare_to(case, "wrong-capture")

        self.assertEqual(proof_calls, ["reference"])
        compare.assert_not_called()
        self.assertEqual(
            (self.artifact_root / "wrong-capture/attempt-status.txt")
            .read_text(encoding="utf-8")
            .splitlines()[0],
            "status=incomplete",
        )

    def test_compare_captures_rejects_stale_exact_proof_capture_hash(self):
        case = self._compare_case()
        proof_calls = []

        def validate_proof(*args, **kwargs):
            role = args[3]
            proof_calls.append(role)
            args[6].mkdir()
            legacy_capture = case["reference_capture_receipt"]
            return {
                "capture": {
                    "schema": "1",
                    "legacy_capture_receipt": str(legacy_capture),
                    "legacy_capture_receipt_sha256": "f" * 64,
                },
                "owners": {index: (({}, {}, {}, {}), {}) for index in self.expected_inputs},
            }

        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=case["source_record"]),
            patch.object(
                self.runner,
                "validate_all_inputs",
                return_value=(self.expected_inputs, {}, self.reference),
            ),
            patch.object(
                adapter_runner,
                "_load_exact_owner_runner",
                return_value=SimpleNamespace(
                    validate_proof=validate_proof,
                    ExactOwnerError=ValueError,
                ),
            ),
            patch.object(adapter_runner, "compare_full_publications") as compare,
        ):
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "exact-owner reference proof adapter capture hash differs",
            ):
                self._run_compare_to(case, "stale-capture-hash")

        self.assertEqual(proof_calls, ["reference"])
        compare.assert_not_called()

    def _run_full_compare_progress_case(
        self,
        scenario,
        input_count=1,
        *,
        verify_serial_parity=False,
        mismatch_name="full-mismatches.tsv",
        assignment_proof=False,
    ):
        expected_inputs = self.expected_inputs
        reference_receipt = self.reference
        original_profiles = adapter_runner.PROFILES
        profiles = (
            original_profiles
            if scenario in {
                "cache-identical", "cache-variants", "cache-boundary", "cache-failure",
                "statement-flow", "parse-error", "empty-checked",
                "empty-checked-diagnostics", "assignment-map", "assignment-no-field",
            }
            else ("default",)
        )
        candidate_tui_source = (
            source_fixtures.OLD_SOURCE
            if scenario in {"empty-checked", "empty-checked-diagnostics"}
            else source_fixtures.CURRENT_SOURCE
        )
        adapter_runner.PROFILES = profiles
        self.addCleanup(setattr, adapter_runner, "PROFILES", original_profiles)
        self.expected_inputs = {index: expected_inputs[index] for index in range(input_count)}
        self.runner.exclude_work_from_git(self.candidate_root)
        input_manifest = pathlib.Path(self._candidate_source_record()["input_manifest"])
        input_manifest.write_text("fixture manifest\n", encoding="utf-8")
        reference_tui_path = self.reference_root / "stdlib/tui.xsh"
        candidate_tui_path = self.candidate_root / "stdlib/tui.xsh"
        candidate_tui_path.parent.mkdir(parents=True, exist_ok=True)
        candidate_tui_path.write_bytes(candidate_tui_source)
        subprocess.run(
            ["git", "-C", str(self.candidate_root), "add", "stdlib/tui.xsh"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(self.candidate_root),
                "commit",
                "--quiet",
                "-m",
                "test(fixtures): add tui source",
            ],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.candidate_commit, self.candidate_tree = self.runner.source_identity(
            self.candidate_root
        )
        self.assertEqual(reference_tui_path.read_bytes(), source_fixtures.OLD_SOURCE)

        reference_build_path, reference_build = self._write_build_receipt(
            "reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
        )
        candidate_build_path, candidate_build = self._write_build_receipt(
            "candidate",
            self.candidate_root,
            self.candidate_commit,
            self.candidate_tree,
        )
        reference_capture, _reference_ranges = self._capture_receipt(
            reference_build_path,
            reference_build,
            self.artifact_root / "full-reference-capture",
            outcome_count=input_count * len(profiles),
        )
        candidate_capture, _candidate_ranges = self._capture_receipt(
            candidate_build_path,
            candidate_build,
            self.artifact_root / "full-candidate-capture",
            role="candidate",
            outcome_count=input_count * len(profiles),
        )
        base_reference_publication = TuiPublicationRunnerAlignmentTests()._schema_publication(
            source_fixtures.publication(source_fixtures.OLD_SOURCE)[0], "legacy"
        )
        base_candidate_publication = TuiPublicationRunnerAlignmentTests()._schema_publication(
            source_fixtures.publication(source_fixtures.CURRENT_SOURCE)[0], "current"
        )
        current_statement_start = source_fixtures.CURRENT_SOURCE.index(b"export pure reset") + 1
        initializer_statement_span = source_fixtures.span(
            5, FIXTURE_INITIALIZER_STATEMENT_START, 4
        )
        initializer_expression_span = source_fixtures.span(
            5, FIXTURE_INITIALIZER_EXPRESSION_START, 1
        )
        current_statement_rows = {
            "StmtId(20)": (source_fixtures.span(5, current_statement_start, 1), "stmt:Assert"),
            "StmtId(21)": (
                source_fixtures.span(5, current_statement_start + 1, 1),
                "stmt:AssertBare",
            ),
            "StmtId(22)": (
                source_fixtures.span(5, current_statement_start + 2, 1),
                "stmt:DeferExpr",
            ),
        }
        reference_statement_rows = {
            statement_id: (
                source_fixtures.span(
                    5,
                    current_statement_start + offset - source_fixtures.DELTA,
                    1,
                ),
                kind,
            )
            for offset, (statement_id, (_span, kind)) in enumerate(current_statement_rows.items())
        }

        def replace_fields(rendered, struct_name, replacements):
            prefix, entries = adapter_runner.adapter._struct_parts(rendered, struct_name)
            changed = []
            replaced = set()
            for entry in entries:
                name, value = adapter_runner.adapter._field(entry)
                if name in replacements:
                    value = replacements[name]
                    replaced.add(name)
                changed.append(f"{name}:{value}")
            if replaced != set(replacements):
                raise AssertionError((struct_name, set(replacements) - replaced))
            return f"{prefix}{{{','.join(changed)}}}"

        def add_statement_flow(rendered, current_schema):
            fields = adapter_runner.adapter._fields(rendered, "CheckOutput")
            statement_rows = current_statement_rows if current_schema else reference_statement_rows
            propagated = set(
                adapter_runner.adapter._set_items_ordered(fields["propagating_statements"])
            )
            propagated.update(span for span, _kind in statement_rows.values())
            replacements = {
                "propagating_statements": "{" + ",".join(sorted(propagated)) + "}"
            }
            statement_positions = adapter_runner.adapter._map_entries(
                fields["statement_positions"]
            )
            statement_positions.update(
                {span: "Value" for span, _kind in statement_rows.values()}
            )
            replacements["statement_positions"] = "{" + ",".join(
                f"{span}:{value}" for span, value in sorted(statement_positions.items())
            ) + "}"
            if current_schema:
                control_flow = adapter_runner.adapter._fields(
                    fields["checked_control_flow"], "CheckedControlFlowFacts"
                )
                propagating_errors = set(
                    adapter_runner.adapter._set_items_ordered(
                        control_flow["propagating_error_spans"]
                    )
                )
                propagating_errors.update(span for span, _kind in statement_rows.values())
                replacements["checked_control_flow"] = replace_fields(
                    fields["checked_control_flow"],
                    "CheckedControlFlowFacts",
                    {
                        "propagating_error_spans": "{"
                        + ",".join(sorted(propagating_errors))
                        + "}"
                    },
                )
            return replace_fields(rendered, "CheckOutput", replacements)

        reference_payloads = {}
        candidate_payloads = {}
        for profile in profiles:
            reference_publication = base_reference_publication
            candidate_publication = base_candidate_publication
            if scenario in {"empty-checked", "empty-checked-diagnostics"}:
                reference_publication = ""
                candidate_publication = ""
            if scenario == "statement-flow":
                reference_publication = add_statement_flow(reference_publication, False)
                candidate_publication = add_statement_flow(candidate_publication, True)
            if scenario == "assignment-map":
                candidate_publication = candidate_publication.replace(
                    "CheckOutput{",
                    "CheckOutput{assignment_facts:AssignmentFacts{owners:{}},",
                    1,
                )
            diagnostics = "[]"
            if scenario == "load-error":
                reference_publication = ""
                candidate_publication = ""
            if scenario == "cache-variants" and profile == "execution":
                reference_publication = reference_publication.replace(
                    'Other{literal:"', 'Other{literal:"profile-variant-'
                )
                candidate_publication = candidate_publication.replace(
                    'Other{literal:"', 'Other{literal:"profile-variant-'
                )
            if scenario == "cache-variants" and profile == "check-cli":
                diagnostics = '[{"message":"profile variant"}]'
            candidate_diagnostics = diagnostics
            if scenario == "empty-checked-diagnostics":
                candidate_diagnostics = '[{"message":"profile difference"}]'
            if scenario == "parse-error" and profile == "reveal":
                reference_publication = "not a CheckOutput"
            reference_payloads[profile] = (
                diagnostics,
                reference_publication,
                (
                    hashlib.sha256(diagnostics.encode()).hexdigest(),
                    hashlib.sha256(reference_publication.encode()).hexdigest(),
                ),
            )
            candidate_payloads[profile] = (
                candidate_diagnostics,
                candidate_publication,
                (
                    hashlib.sha256(candidate_diagnostics.encode()).hexdigest(),
                    hashlib.sha256(candidate_publication.encode()).hexdigest(),
                ),
            )

        def owner_text(source):
            sizes = source_fixtures.source_sizes(source)
            sizes.update(
                {source_id: sizes.get(source_id, 0) for source_id in range(max(sizes) + 1)}
            )
            owners = source_fixtures.owner_index(source)
            statement_rows = {}
            if scenario == "statement-flow":
                statement_rows = dict(
                    reference_statement_rows
                    if source == source_fixtures.OLD_SOURCE
                    else current_statement_rows
                )
                for span, kind in statement_rows.values():
                    owners.setdefault(span, set()).add(kind)
                initializer_owner_spans = (
                    initializer_statement_span,
                    initializer_expression_span,
                )
                if source == source_fixtures.OLD_SOURCE:
                    initializer_owner_spans = (
                        source_fixtures.span(
                            5,
                            FIXTURE_INITIALIZER_STATEMENT_START - source_fixtures.DELTA,
                            4,
                        ),
                        source_fixtures.span(
                            5,
                            FIXTURE_INITIALIZER_EXPRESSION_START - source_fixtures.DELTA,
                            1,
                        ),
                    )
                owners.setdefault(initializer_owner_spans[0], set()).add("stmt:Let")
                owners.setdefault(initializer_owner_spans[1], set()).add("expr:Ident")
            lines = [
                f"source\tSourceId({source_id})\t{size}"
                for source_id, size in sorted(sizes.items())
            ]
            lines.extend(
                f"owner\t{span}\t{kind}"
                for span, kinds in sorted(owners.items())
                for kind in sorted(kinds)
            )
            lines.extend(
                f"stmt\t{statement_id}\t{span}\t{kind}"
                for statement_id, (span, kind) in statement_rows.items()
            )
            return "\n".join(lines) + "\n"

        reference_owner_text = owner_text(source_fixtures.OLD_SOURCE)
        candidate_owner_text = owner_text(candidate_tui_source)
        reference_projection = adapter_runner.adapter.parse_owner_index(reference_owner_text)
        candidate_projection = adapter_runner.adapter.parse_owner_index(candidate_owner_text)

        def write_raw(capture, payloads, owners, kind="checked"):
            raw_root = capture.parent / "raw"
            (raw_root / "schema").write_text(adapter_runner.RAW_SCHEMA, encoding="utf-8")
            for index in self.expected_inputs:
                source_owners = owners(index) if callable(owners) else owners
                (raw_root / f"owners-{index}.tsv").write_text(source_owners, encoding="utf-8")
                for profile, (diagnostics, publication, hashes) in payloads.items():
                    record = raw_root / f"record-{index}-{profile}"
                    record.mkdir()
                    (record / "kind").write_text(kind, encoding="utf-8")
                    (record / "diagnostics").write_text(diagnostics, encoding="utf-8")
                    (record / "publication").write_text(publication, encoding="utf-8")
                    (record / "receipt-hashes").write_text("\t".join(hashes), encoding="utf-8")
            return raw_root

        reference_raw = write_raw(
            reference_capture,
            reference_payloads,
            lambda index: "invalid-owner-index\n"
            if scenario == "owner-error" and index == 0
            else reference_owner_text,
            "load-error" if scenario == "load-error" else "checked",
        )
        candidate_raw = write_raw(
            candidate_capture,
            candidate_payloads,
            candidate_owner_text,
            "load-error" if scenario == "load-error" else "checked",
        )
        reference_outcomes = {
            (index, self.expected_inputs[index].relative_path, profile): payload[2]
            for index in self.expected_inputs
            for profile, payload in reference_payloads.items()
        }
        candidate_outcomes = {
            (index, self.expected_inputs[index].relative_path, profile): payload[2]
            for index in self.expected_inputs
            for profile, payload in candidate_payloads.items()
        }
        self.reference = self.runner.ParsedReceipt(
            (),
            {
                index: self.runner.ReceiptSource(source.relative_path, "b" * 64)
                for index, source in self.expected_inputs.items()
            },
            reference_outcomes,
            (),
        )

        def parse_shard(path, _expected_inputs):
            match = re.fullmatch(r"parallel-(\d+)-(\d+)-all\.receipt", path.name)
            if match is None:
                raise AssertionError(path)
            start, end = int(match.group(1)), int(match.group(2)) + 1
            recorded = reference_outcomes if "full-reference-capture" in str(path) else candidate_outcomes
            outcomes = {
                key: value for key, value in recorded.items() if start <= key[0] < end
            }
            return self.runner.ParsedReceipt(
                (
                    f"corpus={len(self.expected_inputs)}",
                    f"shard={start // 100}/{(len(self.expected_inputs) + 99) // 100}",
                    f"range={start}..{end}",
                    "shard_size=100",
                    f"input_digest={self.runner.CORPUS_BLAKE3}",
                    self.runner.OPTION_HEADER,
                ),
                {index: self.reference.inputs[index] for index in range(start, end)},
                outcomes,
                (f"terminal\tshard={start // 100}\tcompleted={end - start}",),
            )

        candidate_start = candidate_tui_source.index(b"export pure reset")
        initializer_expression = (
            (initializer_expression_span, "Ident", "initializer")
            if scenario == "statement-flow"
            else (source_fixtures.span(5, candidate_start, 1), "Ident", "reset")
        )
        binding_initializers = (
            {(33, "%dest"): (initializer_statement_span, 0)}
            if scenario == "statement-flow"
            else {}
        )
        candidate_exact_owners = {
            "exprs": {
                0: initializer_expression
            },
            "expr_count": 1,
            "record_field_count": 0,
            "runs": {},
            "records": {},
            "aliases": {},
            "tails": {},
            "calls": {},
            "field_accesses": {},
            "binding_initializers": binding_initializers,
            "structured_stages": {},
            "stage_arguments": {},
            "stage_callbacks": {},
        }
        assignment_validation_calls = multiprocessing.get_context("fork").Value("i", 0)

        def validate_assignment(_rendered, _proof):
            with assignment_validation_calls.get_lock():
                assignment_validation_calls.value += 1

        reference_exact_owners = dict(candidate_exact_owners)
        if scenario == "statement-flow":
            reference_exact_owners["exprs"] = {
                0: (
                    source_fixtures.span(
                        5,
                        FIXTURE_INITIALIZER_EXPRESSION_START - source_fixtures.DELTA,
                        1,
                    ),
                    "Ident",
                    "initializer",
                )
            }
            reference_exact_owners["binding_initializers"] = {
                (33, "%dest"): (
                    source_fixtures.span(
                        5,
                        FIXTURE_INITIALIZER_STATEMENT_START - source_fixtures.DELTA,
                        4,
                    ),
                    0,
                )
            }
        else:
            reference_exact_owners["exprs"] = {
                0: (
                    source_fixtures.span(
                        5,
                        candidate_start - source_fixtures.DELTA,
                        1,
                    ),
                    "Ident",
                    "reset",
                )
            }
        exact_owner_projection = {
            "reference": {
                index: (reference_projection, reference_exact_owners)
                for index in self.expected_inputs
            },
            "candidate": {
                index: (candidate_projection, candidate_exact_owners)
                for index in self.expected_inputs
            },
        }
        observed = []
        observed_count = multiprocessing.get_context("fork").Value("i", 0)
        original_regular_file = self.runner.regular_file
        original_require_sha256 = self.runner.require_sha256

        def regular_file(path, kind, executable=False):
            if pathlib.Path(path) == input_manifest:
                return input_manifest
            return original_regular_file(path, kind, executable)

        def require_sha256(path, expected, kind):
            if pathlib.Path(path) == input_manifest:
                self.assertEqual(expected, self.runner.INPUT_MANIFEST_SHA256)
                return expected
            return original_require_sha256(path, expected, kind)

        def compare_adapter(*args, **kwargs):
            observed.append((args, kwargs))
            with observed_count.get_lock():
                observed_count.value += 1
            if scenario == "cache-failure":
                raise adapter_runner.adapter.AdapterError("fixture comparison failure")
            if scenario == "statement-flow":
                expected_initializer = (
                    source_fixtures.span(
                        5,
                        FIXTURE_INITIALIZER_EXPRESSION_START - source_fixtures.DELTA,
                        1,
                    ),
                    "Ident",
                    "initializer",
                )
                expected_binding_initializers = {
                    (33, "%dest"): (
                        source_fixtures.span(
                            5,
                            FIXTURE_INITIALIZER_STATEMENT_START - source_fixtures.DELTA,
                            4,
                        ),
                        0,
                    )
                }
                expected_statement_start = candidate_start + 1 - source_fixtures.DELTA
                expected_statement_owners = {
                    "StmtId(20)": (
                        source_fixtures.span(5, expected_statement_start, 1),
                        "stmt:Assert",
                    ),
                    "StmtId(21)": (
                        source_fixtures.span(5, expected_statement_start + 1, 1),
                        "stmt:AssertBare",
                    ),
                    "StmtId(22)": (
                        source_fixtures.span(5, expected_statement_start + 2, 1),
                        "stmt:DeferExpr",
                    ),
                }
                if (
                    kwargs.get("reference_exact_owners") != reference_exact_owners
                    or kwargs.get("reference_exact_owners", {}).get("exprs", {}).get(0)
                    != expected_initializer
                    or kwargs.get("candidate_exact_owners", {}).get("exprs", {}).get(0)
                    != expected_initializer
                    or kwargs.get("candidate_exact_owners", {}).get("binding_initializers")
                    != expected_binding_initializers
                    or kwargs.get("candidate_statement_owners") != expected_statement_owners
                ):
                    raise adapter_runner.adapter.AdapterError(
                        "parallel exact-owner forwarding differs from fixture facts"
                    )
                return actual_compare(*args, **kwargs)

        stderr = io.StringIO()
        actual_compare = adapter_runner.adapter._compare_parsed_publications
        actual_parallel_runner = adapter_runner._run_parallel_publication_comparisons

        self.comparison_parity = None

        def run_parallel_and_check_serial(context, count, on_result):
            parallel_results = []

            def accept_result(index, result):
                parallel_results.append((index, result))
                on_result(index, result)

            serial_context = context._replace(
                reference_root=os.dup(context.reference_root),
                candidate_root=os.dup(context.candidate_root),
            )
            try:
                actual_parallel_runner(
                    context,
                    count,
                    accept_result,
                )
                serial_results = tuple(
                    (index, adapter_runner._compare_publication_input(index, serial_context))
                    for index in range(count)
                )
                self.comparison_parity = (tuple(parallel_results), serial_results)
            finally:
                os.close(serial_context.reference_root)
                os.close(serial_context.candidate_root)

        with (
            patch.object(adapter_runner.sys, "stderr", stderr),
            patch.object(self.runner, "regular_file", side_effect=regular_file),
            patch.object(self.runner, "require_sha256", side_effect=require_sha256),
            patch.object(self.runner, "validate_reference_receipt", return_value=self.reference),
            patch.object(self.runner, "parse_receipt", side_effect=parse_shard),
            patch.object(self.runner, "PROFILES", profiles),
            patch.object(adapter_runner, "_load_publication_runner", return_value=self.runner),
            patch.object(
                adapter_runner,
                "_run_parallel_publication_comparisons",
                side_effect=(
                    run_parallel_and_check_serial
                    if verify_serial_parity
                    else actual_parallel_runner
                ),
            ),
            patch.object(adapter_runner, "_hash_raw_outcomes", side_effect=lambda binary, *_args, **_kwargs: (
                {
                    (index, profile, field): digest
                    for outcomes in (
                        reference_outcomes if "reference" in binary.name else candidate_outcomes,
                    )
                    for (index, relative_path, profile), pair in outcomes.items()
                    for field, digest in zip(("diagnostics", "publication"), pair)
                }
            )),
            patch.object(
                adapter_runner.adapter,
                "_compare_parsed_publications",
                side_effect=compare_adapter,
            ),
        ):
            result = adapter_runner.compare_full_publications(
                self.runner,
                pathlib.Path(self._candidate_source_record()["reference_receipt"]),
                candidate_capture.parent / "receipts",
                reference_raw,
                candidate_raw,
                self.expected_inputs,
                self.artifact_root / mismatch_name,
                reference_capture,
                candidate_capture,
                self.candidate_root,
                self.reference_root,
                self._candidate_source_record(),
                pathlib.Path(self._candidate_source_record()["input_root"]),
                input_manifest,
                {"PATH": os.environ["PATH"]},
                reference_exact_owners=exact_owner_projection["reference"],
                candidate_exact_owners=exact_owner_projection["candidate"],
                candidate_assignment_proofs=(
                    {index: object() for index in self.expected_inputs}
                    if assignment_proof
                    else None
                ),
                assignment_facts_module=(
                    SimpleNamespace(
                        AssignmentFactsError=ValueError,
                        validate_assignment_facts=validate_assignment,
                    )
                    if assignment_proof
                    else None
                ),
            )

        self.expected_inputs = expected_inputs
        self.reference = reference_receipt
        self.assignment_validation_calls = assignment_validation_calls.value
        if input_count > 1 and observed_count.value:
            observed = [None] * observed_count.value
        return result, observed, stderr.getvalue(), candidate_start

    def test_full_compare_aligns_candidate_exact_owner_spans_before_comparison(self):
        result, observed, progress, candidate_start = self._run_full_compare_progress_case("checked")

        self.assertEqual(result, (1, 0))
        self.assertEqual(len(observed), 1)
        self.assertEqual(
            observed[0][1]["candidate_exact_owners"]["exprs"][0][0],
            source_fixtures.span(5, candidate_start - source_fixtures.DELTA, 1),
        )
        self.assertEqual(
            observed[0][1]["reference_exact_owners"]["exprs"][0],
            (
                source_fixtures.span(5, candidate_start - source_fixtures.DELTA, 1),
                "Ident",
                "reset",
            ),
        )
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=1/1 mismatches=0\n",
        )

    def test_full_compare_forwards_and_aligns_exact_assert_bare_defer_owners(self):
        result, observed, progress, current_start = self._run_full_compare_progress_case(
            "statement-flow"
        )

        self.assertEqual(result, (4, 0))
        self.assertEqual(len(observed), 1)
        candidate_statement_owners = observed[0][1]["candidate_statement_owners"]
        reference_start = current_start + 1 - source_fixtures.DELTA
        self.assertEqual(
            candidate_statement_owners,
            {
                "StmtId(20)": (
                    source_fixtures.span(5, reference_start, 1),
                    "stmt:Assert",
                ),
                "StmtId(21)": (
                    source_fixtures.span(5, reference_start + 1, 1),
                    "stmt:AssertBare",
                ),
                "StmtId(22)": (
                    source_fixtures.span(5, reference_start + 2, 1),
                    "stmt:DeferExpr",
                ),
            },
        )
        self.assertEqual(
            observed[0][1]["candidate_exact_owners"]["binding_initializers"],
            {
                (33, "%dest"): (
                    source_fixtures.span(
                        5,
                        FIXTURE_INITIALIZER_STATEMENT_START - source_fixtures.DELTA,
                        4,
                    ),
                    0,
                )
            },
        )
        self.assertEqual(
            observed[0][1]["candidate_exact_owners"]["exprs"][0][0],
            source_fixtures.span(
                5,
                FIXTURE_INITIALIZER_EXPRESSION_START - source_fixtures.DELTA,
                1,
            ),
        )
        self.assertEqual(
            observed[0][1]["reference_exact_owners"]["exprs"][0],
            (
                source_fixtures.span(
                    5,
                    FIXTURE_INITIALIZER_EXPRESSION_START - source_fixtures.DELTA,
                    1,
                ),
                "Ident",
                "initializer",
            ),
        )
        self.assertEqual(
            observed[0][1]["reference_exact_owners"]["binding_initializers"],
            {
                (33, "%dest"): (
                    source_fixtures.span(
                        5,
                        FIXTURE_INITIALIZER_STATEMENT_START - source_fixtures.DELTA,
                        4,
                    ),
                    0,
                )
            },
        )
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=4/4 mismatches=0\n",
        )

    def test_full_compare_reports_load_error_sources_as_processed_outcomes(self):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "load-error", assignment_proof=True
        )

        self.assertEqual(result, (1, 0))
        self.assertEqual(observed, [])
        self.assertEqual(self.assignment_validation_calls, 0)
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=1/1 mismatches=0\n",
        )

    def test_assignment_projection_validates_all_checked_profile_publications(self):
        result, _observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "assignment-map", assignment_proof=True
        )

        self.assertEqual(result, (4, 0))
        self.assertEqual(self.assignment_validation_calls, 4)
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=4/4 mismatches=0\n",
        )

    def test_assignment_projection_rejects_checked_publication_without_facts(self):
        result, _observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "assignment-no-field", assignment_proof=True
        )

        self.assertEqual(result, (4, 1))
        self.assertEqual(self.assignment_validation_calls, 0)
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=4/4 mismatches=1\n",
        )

    def test_full_compare_reports_owner_errors_with_recorded_mismatch_count(self):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case("owner-error")

        self.assertEqual(result, (1, 1))
        self.assertEqual(observed, [])
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=1/1 mismatches=1\n",
        )

    def test_progress_reports_each_twenty_five_inputs_and_final_remainder(self):
        class ProgressStream:
            def __init__(self):
                self.parts = []
                self.flush_count = 0

            def write(self, value):
                self.parts.append(value)

            def flush(self):
                self.flush_count += 1

        stream = ProgressStream()
        with (
            patch.object(adapter_runner, "PROFILES", ("default", "reveal", "execution", "check-cli")),
            patch.object(adapter_runner.sys, "stderr", stream),
        ):
            for processed_inputs in range(1, 27):
                adapter_runner._report_comparison_progress(
                    processed_inputs, 26, 104, 7
                )

        self.assertEqual(
            "".join(stream.parts),
            "comparison_progress inputs=25/26 outcomes=100/104 mismatches=7\n"
            "comparison_progress inputs=26/26 outcomes=104/104 mismatches=7\n",
        )
        self.assertEqual(stream.flush_count, 2)

    def test_full_compare_reuses_identical_profile_pairs_within_one_input(self):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "cache-identical"
        )

        self.assertEqual(result, (4, 0))
        self.assertEqual(len(observed), 1)
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=4/4 mismatches=0\n",
        )

    def test_full_compare_rechecks_changed_publication_or_diagnostics(self):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "cache-variants"
        )

        self.assertEqual(result, (4, 0))
        self.assertEqual(len(observed), 3)
        self.assertEqual(
            len({
                (args[0], args[1], tuple(args[2]), args[7], args[8], tuple(args[9]))
                for args, _kwargs in observed
            }),
            3,
        )
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=4/4 mismatches=0\n",
        )

    def test_full_compare_does_not_reuse_results_across_input_indexes(self):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "cache-boundary",
            input_count=2,
        )

        self.assertEqual(result, (8, 0))
        self.assertEqual(len(observed), 2)
        self.assertEqual(
            progress,
            "comparison_progress inputs=2/2 outcomes=8/8 mismatches=0\n",
        )

    def test_parallel_full_compare_matches_serial_and_preserves_mismatch_order(self):
        result, _observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "owner-error",
            input_count=2,
            verify_serial_parity=True,
            mismatch_name="parallel-owner-error.tsv",
        )

        self.assertEqual(result, (2, 1))
        self.assertEqual(
            self.comparison_parity[0],
            self.comparison_parity[1],
        )
        self.assertEqual(
            (self.artifact_root / "parallel-owner-error.tsv").read_bytes(),
            b"0\towner-index\tAdapterError\towner index line 1: unrecognized row\n",
        )
        self.assertEqual(
            progress,
            "comparison_progress inputs=2/2 outcomes=2/2 mismatches=1\n",
        )

    def test_parallel_full_compare_detects_perturbed_profiles_like_serial(self):
        result, _observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "cache-failure",
            input_count=2,
            verify_serial_parity=True,
            mismatch_name="parallel-cache-failure.tsv",
        )

        self.assertEqual(result, (8, 8))
        self.assertEqual(self.comparison_parity[0], self.comparison_parity[1])
        report = (self.artifact_root / "parallel-cache-failure.tsv").read_bytes()
        self.assertEqual(report.count(b"\tadapter\tAdapterError\tfixture comparison failure\n"), 8)
        self.assertLess(report.index(b"(0, 'source/0000.xsh'"), report.index(b"(1, 'source/0001.xsh'"))
        self.assertEqual(
            progress,
            "comparison_progress inputs=2/2 outcomes=8/8 mismatches=8\n",
        )

    def test_parallel_full_compare_forwards_both_exact_owner_roles(self):
        result, _observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "statement-flow",
            input_count=2,
        )

        self.assertEqual(result, (8, 0))
        self.assertEqual(
            (self.artifact_root / "full-mismatches.tsv").read_bytes(),
            b"",
        )
        self.assertEqual(
            progress,
            "comparison_progress inputs=2/2 outcomes=8/8 mismatches=0\n",
        )

    def test_parallel_comparison_preserves_lowest_index_worker_exception(self):
        context = multiprocessing.get_context("fork")
        earlier_failure_ready = context.Event()
        lower_index_may_fail = context.Event()
        parent_observed_worker_exit = [False]
        actual_wait = adapter_runner.multiprocessing.connection.wait
        existing_children = {process.pid for process in multiprocessing.active_children()}

        def fail_by_index(index, _comparison_context):
            if index == 1:
                earlier_failure_ready.set()
                raise RuntimeError("input one failed first")
            self.assertTrue(earlier_failure_ready.wait(timeout=3))
            self.assertTrue(lower_index_may_fail.wait(timeout=3))
            raise RuntimeError("input zero failed second")

        def wait_for_worker_exit(waitables, timeout=None):
            if parent_observed_worker_exit[0]:
                lower_index_may_fail.set()
            ready = actual_wait(waitables, timeout)
            if any(
                not isinstance(item, adapter_runner.multiprocessing.connection.Connection)
                for item in ready
            ):
                parent_observed_worker_exit[0] = True
            return ready

        with (
            patch.object(adapter_runner, "_compare_publication_input", side_effect=fail_by_index),
            patch.object(
                adapter_runner.multiprocessing.connection,
                "wait",
                side_effect=wait_for_worker_exit,
            ),
        ):
            with self.assertRaisesRegex(RuntimeError, "input zero failed second"):
                adapter_runner._run_parallel_publication_comparisons(
                    self._parallel_test_context(), 2, lambda _index, _result: None
                )

        self.assertEqual(
            {process.pid for process in multiprocessing.active_children()},
            existing_children,
        )

    def test_parallel_comparison_reaps_workers_after_abrupt_exit(self):
        existing_children = {process.pid for process in multiprocessing.active_children()}

        def exit_abruptly(index, _comparison_context):
            if index == 0:
                os._exit(19)
            return ()

        with patch.object(adapter_runner, "_compare_publication_input", side_effect=exit_abruptly):
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "comparison worker exited (without an outcome for input 0|unexpectedly with status 19)",
            ):
                adapter_runner._run_parallel_publication_comparisons(
                    self._parallel_test_context(), 2, lambda _index, _result: None
                )

        self.assertEqual(
            {process.pid for process in multiprocessing.active_children()},
            existing_children,
        )

    def test_parallel_comparison_reaps_workers_after_sigint(self):
        signum = signal.SIGINT
        existing_children = {process.pid for process in multiprocessing.active_children()}
        previous_handler = signal.getsignal(signum)
        observed_signals = []

        def previous_interrupt_handler(received_signal, _frame):
            observed_signals.append(received_signal)
            raise KeyboardInterrupt(f"received signal {received_signal}")

        signal.signal(signum, previous_interrupt_handler)

        def interrupt_parent(index, _comparison_context):
            if index == 0:
                os.kill(os.getppid(), signum)
            signal.pause()

        try:
            with patch.object(adapter_runner, "_compare_publication_input", side_effect=interrupt_parent):
                with self.assertRaisesRegex(KeyboardInterrupt, "received signal 2"):
                    adapter_runner._run_parallel_publication_comparisons(
                        self._parallel_test_context(), 2, lambda _index, _result: None
                    )

            self.assertIs(signal.getsignal(signum), previous_interrupt_handler)
            self.assertEqual(observed_signals, [signum])
        finally:
            signal.signal(signum, previous_handler)
        self.assertEqual(
            {process.pid for process in multiprocessing.active_children()},
            existing_children,
        )

    def test_parallel_comparison_preserves_ignored_sigterm(self):
        previous_handler = signal.getsignal(signal.SIGTERM)

        def interrupt_parent(index, _comparison_context):
            if index == 0:
                os.kill(os.getppid(), signal.SIGTERM)
            return ()

        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        try:
            with patch.object(adapter_runner, "_compare_publication_input", side_effect=interrupt_parent):
                results = []
                adapter_runner._run_parallel_publication_comparisons(
                    self._parallel_test_context(),
                    2,
                    lambda index, result: results.append((index, result)),
                )
            self.assertEqual(results, [(0, ()), (1, ())])
            self.assertIs(signal.getsignal(signal.SIGTERM), signal.SIG_IGN)
        finally:
            signal.signal(signal.SIGTERM, previous_handler)

    def test_parallel_comparison_reaps_workers_after_sigterm_and_preserves_exit(self):
        script = """
import multiprocessing
import os
import pathlib
import signal
import sys
import subprocess
from unittest.mock import patch
import w9_publication_adapter_runner as runner

publication_runner = runner._load_publication_runner(
    runner.ROOT / "dev/consolidation/w9-publication-run.py"
)
candidate_root = pathlib.Path(sys.argv[1]) / "candidate"
mode = sys.argv[2]
candidate_root.mkdir()
(candidate_root / ".work").mkdir()
subprocess.run(["git", "init", "--quiet", str(candidate_root)], check=True)
output = candidate_root / ".work" / "comparison"
context = runner._PublicationComparisonContext(-1, -1, {}, None, {}, {}, {}, {}, {}, b"", b"", None, None, False)
original_kill = os.kill
original_excepthook = sys.excepthook

def observe_termination(pid, signum):
    if pid == os.getpid() and signum == signal.SIGTERM:
        reaped = len(multiprocessing.active_children()) == 0
        restored = signal.getsignal(signal.SIGTERM) == signal.SIG_DFL
        print(f"default=reaped={reaped} restored={restored}", flush=True)
    original_kill(pid, signum)

def observe_exception(error_type, error, traceback):
    if issubclass(error_type, KeyboardInterrupt):
        reaped = len(multiprocessing.active_children()) == 0
        restored = signal.getsignal(signal.SIGTERM) == signal.SIG_DFL
        locks_released = not publication_runner.global_runner_lock_path(candidate_root).exists()
        locks_released = locks_released and not publication_runner.output_lock_path(output).exists()
        status = (output / "attempt-status.txt").read_text()
        incomplete = "status=incomplete\\n" in status
        print(
            f"claimed={error_type.__name__}: {error} reaped={reaped} restored={restored} "
            f"locks_released={locks_released} incomplete={incomplete}",
            flush=True,
        )
    original_excepthook(error_type, error, traceback)

def interrupt_parent(index, _context):
    if index == 0:
        os.kill(os.getppid(), signal.SIGTERM)
    signal.pause()

os.kill = observe_termination
sys.excepthook = observe_exception
def compare(args, active_runner):
    if mode == "claimed":
        with active_runner.claim_run_resources(args.candidate_root, output, []):
            active_runner.write_status(output, "incomplete", [])
            with patch.object(runner, "_compare_publication_input", side_effect=interrupt_parent):
                runner._run_parallel_publication_comparisons(context, 2, lambda _index, _result: None)
    else:
        with patch.object(runner, "_compare_publication_input", side_effect=interrupt_parent):
            runner._run_parallel_publication_comparisons(context, 2, lambda _index, _result: None)

runner._load_candidate_publication_runner = lambda *_args: publication_runner
runner.compare_captured_publications = compare
arguments = [
    "compare-captures",
    "--candidate-root", str(candidate_root),
    "--candidate-source-receipt", str(candidate_root / "source.receipt"),
    "--reference-source-root", str(candidate_root),
    "--reference-exact-build-receipt", str(candidate_root / "reference-build.receipt"),
    "--candidate-exact-build-receipt", str(candidate_root / "candidate-build.receipt"),
    "--reference-capture-receipt", str(candidate_root / "reference-capture.receipt"),
    "--candidate-capture-receipt", str(candidate_root / "candidate-capture.receipt"),
    "--reference-exact-capture-receipt", str(candidate_root / "reference-exact.receipt"),
    "--candidate-exact-capture-receipt", str(candidate_root / "candidate-exact.receipt"),
    "--output", str(output),
]
raise SystemExit(runner.main(arguments))
"""
        scratch_parent = adapter_runner.ROOT / ".work/tmp"
        scratch_parent.mkdir(parents=True, exist_ok=True)
        def run(mode):
            with tempfile.TemporaryDirectory(
                prefix=f"w9-parallel-sigterm-{mode}-", dir=scratch_parent
            ) as scratch:
                return subprocess.run(
                    [sys.executable, "-c", script, scratch, mode],
                    cwd=pathlib.Path(__file__).parent,
                    capture_output=True,
                    text=True,
                    timeout=12,
                )

        claimed = run("claimed")
        self.assertEqual(claimed.returncode, -signal.SIGINT)
        self.assertEqual(
            claimed.stdout,
            "claimed=KeyboardInterrupt: received signal 15 reaped=True restored=True "
            "locks_released=True incomplete=True\n",
        )
        self.assertIn(
            "KeyboardInterrupt: received signal 15",
            claimed.stderr,
        )

        default = run("default")
        self.assertEqual(default.returncode, -signal.SIGTERM)
        self.assertEqual(default.stdout, "default=reaped=True restored=True\n")
        self.assertEqual(default.stderr, "")

    def test_full_compare_reuses_failed_profile_pairs_and_reports_each_mismatch(self):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "cache-failure"
        )

        mismatch_report = (self.artifact_root / "full-mismatches.tsv").read_bytes()
        expected_mismatches = "".join(
            f"{(0, self.expected_inputs[0].relative_path, profile)}"
            "\tadapter\tAdapterError\tfixture comparison failure\n"
            for profile in adapter_runner.PROFILES
        ).encode()
        self.assertEqual(result, (4, 4))
        self.assertEqual(len(observed), 1)
        self.assertEqual(mismatch_report, expected_mismatches)
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=4/4 mismatches=4\n",
        )

    def test_full_compare_rejects_parse_failure_before_parsed_comparison(self):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case(
            "parse-error"
        )

        mismatch_report = (self.artifact_root / "full-mismatches.tsv").read_bytes()
        expected_mismatch = (
            "0"
            "\ttui-alignment\tAdapterError\texpected named CheckOutput Debug struct\n"
        ).encode()
        self.assertEqual(result, (4, 1))
        self.assertEqual(observed, [])
        self.assertEqual(mismatch_report, expected_mismatch)
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=4/4 mismatches=1\n",
        )

    def _assert_empty_checked_publication_failure(self, scenario, expected_error):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case(
            scenario
        )
        mismatch_report = (self.artifact_root / "full-mismatches.tsv").read_bytes()
        expected_diagnostics = ""
        if scenario == "empty-checked-diagnostics":
            reference_hash = hashlib.sha256(b"[]").hexdigest()
            candidate_hash = hashlib.sha256(
                b'[{"message":"profile difference"}]'
            ).hexdigest()
            expected_diagnostics = "".join(
                f"{(0, self.expected_inputs[0].relative_path, profile)}"
                f"\tdiagnostics\t{reference_hash}\t{candidate_hash}\n"
                for profile in adapter_runner.PROFILES
            )
        expected_adapter_errors = "".join(
            f"{(0, self.expected_inputs[0].relative_path, profile)}"
            f"\tadapter\tAdapterError\t{expected_error}\n"
            for profile in adapter_runner.PROFILES
        )
        expected_mismatches = (expected_diagnostics + expected_adapter_errors).encode()
        mismatch_count = len(adapter_runner.PROFILES) + (
            len(adapter_runner.PROFILES) if expected_diagnostics else 0
        )
        self.assertEqual(result, (4, mismatch_count))
        self.assertEqual(observed, [])
        self.assertEqual(mismatch_report, expected_mismatches)
        self.assertEqual(
            progress,
            f"comparison_progress inputs=1/1 outcomes=4/4 mismatches={mismatch_count}\n",
        )

    def test_full_compare_preserves_raw_error_for_empty_checked_publications(self):
        self._assert_empty_checked_publication_failure(
            "empty-checked",
            "expected named CheckOutput Debug struct",
        )

    def test_full_compare_preserves_diagnostic_error_for_empty_checked_publications(self):
        self._assert_empty_checked_publication_failure(
            "empty-checked-diagnostics",
            "diagnostic mismatch",
        )

    def test_compare_captures_rejects_candidate_source_drift(self):
        case = self._compare_case()
        stale_record = dict(case["source_record"])
        stale_record["candidate_tree"] = "f" * 40

        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=stale_record),
            patch.object(
                self.runner,
                "validate_all_inputs",
                return_value=(self.expected_inputs, {}, self.reference),
            ),
            patch.object(adapter_runner, "compare_full_publications") as compare,
        ):
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "candidate source identity differs from its source receipt",
            ):
                self._run_compare(case)

        compare.assert_not_called()

    def test_compare_captures_rejects_incomplete_range_receipt_before_hashing(self):
        case = self._compare_case()
        receipt = case["reference_capture_receipt"]
        lines = receipt.read_text(encoding="utf-8").splitlines()
        lines[adapter_runner.CAPTURE_FIELDS.index("range_count")] = "range_count\t67"
        lines[adapter_runner.CAPTURE_FIELDS.index("outcome_count")] = "outcome_count\t6700"
        receipt.write_text("\n".join(lines[:-1]) + "\n", encoding="utf-8")

        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "required source/profile coverage differs",
        ):
            self._run_full_compare(case)

    def test_compare_captures_rejects_range_hash_drift_before_hashing(self):
        case = self._compare_case()
        changed_range = next(iter(case["candidate_ranges"].values()))
        changed_range.write_bytes(b"range receipt changed after capture")

        with self.assertRaisesRegex(
            adapter_runner.PublicationError,
            "range receipt hash differs",
        ):
            self._run_full_compare(case)

    def test_compare_cli_loads_publication_runner_from_candidate_source(self):
        case = self._compare_case()
        candidate_runner = SimpleNamespace(CONTROLLER_ROOT=self.runner.CONTROLLER_ROOT)
        loaded = []
        source_receipt = case["candidate_source_receipt"]
        candidate_runner_path = (
            self.candidate_root / "dev/consolidation/w9-publication-run.py"
        )
        source_record = self._candidate_source_record()
        source_record["runner"] = str(candidate_runner_path)
        source_record["runner_sha256"] = self.runner.sha256_file(candidate_runner_path)

        def load(path):
            loaded.append(pathlib.Path(path))
            return self.runner if len(loaded) == 1 else candidate_runner

        def compare(_args, runner):
            self.assertIs(runner, candidate_runner)
            return 0

        with (
            patch.object(adapter_runner, "_load_publication_runner", side_effect=load),
            patch.object(self.runner, "parse_candidate_receipt", return_value=source_record),
            patch.object(adapter_runner, "compare_captured_publications", side_effect=compare),
        ):
            result = adapter_runner.main(
                [
                    "compare-captures",
                    "--candidate-root",
                    str(self.candidate_root),
                    "--candidate-source-receipt",
                    str(source_receipt),
                    "--reference-source-root",
                    str(self.reference_root),
                    "--reference-exact-build-receipt",
                    str(case["reference_exact_build_receipt"]),
                    "--candidate-exact-build-receipt",
                    str(case["candidate_exact_build_receipt"]),
                    "--reference-capture-receipt",
                    str(self.artifact_root / "reference/capture.receipt"),
                    "--candidate-capture-receipt",
                    str(self.artifact_root / "candidate/capture.receipt"),
                    "--reference-exact-capture-receipt",
                    str(case["reference_exact_capture_receipt"]),
                    "--candidate-exact-capture-receipt",
                    str(case["candidate_exact_capture_receipt"]),
                    "--output",
                    str(self.artifact_root / "compare-cli"),
                ]
            )

        self.assertEqual(result, 0)
        self.assertEqual(loaded[1], self.candidate_root / "dev/consolidation/w9-publication-run.py")

    def test_exact_owner_validator_loads_from_controller_not_candidate_source(self):
        candidate_validator = (
            self.candidate_root / "dev/consolidation/w9_publication_exact_owner_runner.py"
        )
        candidate_validator.write_text("raise RuntimeError('candidate validator loaded')\n")

        loaded = adapter_runner._load_exact_owner_runner(self.runner)

        controller_validator = pathlib.Path(adapter_runner.__file__).with_name(
            "w9_publication_exact_owner_runner.py"
        )
        self.assertEqual(pathlib.Path(loaded.__file__).resolve(), controller_validator.resolve())
        self.assertNotEqual(pathlib.Path(loaded.__file__).resolve(), candidate_validator.resolve())


class SourceQualifiedBuildArtifactTests(unittest.TestCase):
    def test_adapter_build_retains_cargo_output_before_success_and_failure_validation(self):
        repository = pathlib.Path(adapter_runner.__file__).resolve().parents[2]
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)

        with tempfile.TemporaryDirectory(prefix="w9-cargo-output-", dir=scratch_root) as scratch:
            attempt = pathlib.Path(scratch)
            success_binary = attempt / "candidate-success"
            success_stdout = b'{"reason":"build-finished"}\n'
            success_stderr = b"warning: retained warning\n"
            success_command = [
                sys.executable,
                "-c",
                "import sys; sys.stdout.buffer.write(bytes.fromhex(sys.argv[1])); "
                "sys.stderr.buffer.write(bytes.fromhex(sys.argv[2]))",
                success_stdout.hex(),
                success_stderr.hex(),
            ]

            output = adapter_runner._run_adapter_build(
                success_command, {}, "candidate", success_binary, runner
            )

            self.assertEqual(output.encode("utf-8"), success_stdout)
            self.assertEqual(
                success_binary.with_name(success_binary.name + ".cargo.jsonl").read_bytes(),
                success_stdout,
            )
            self.assertEqual(
                success_binary.with_name(success_binary.name + ".cargo.stderr").read_bytes(),
                success_stderr,
            )

            failed_binary = attempt / "candidate-failed"
            failure_stdout = b"cargo build output\n"
            failure_stderr = b"compiler stopped\n"
            failure_command = [
                sys.executable,
                "-c",
                "import sys; sys.stdout.buffer.write(bytes.fromhex(sys.argv[1])); "
                "sys.stderr.buffer.write(bytes.fromhex(sys.argv[2])); sys.exit(7)",
                failure_stdout.hex(),
                failure_stderr.hex(),
            ]

            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "pinned candidate adapter harness build failed: compiler stopped",
            ):
                adapter_runner._run_adapter_build(
                    failure_command, {}, "candidate", failed_binary, runner
                )

            self.assertEqual(
                failed_binary.with_name(failed_binary.name + ".cargo.jsonl").read_bytes(),
                failure_stdout,
            )
            self.assertEqual(
                failed_binary.with_name(failed_binary.name + ".cargo.stderr").read_bytes(),
                failure_stderr,
            )

    def test_build_command_matches_the_frozen_cargo_argv(self):
        source_record = {
            "build_profile": "verification",
            "build_target": "x86_64-unknown-linux-musl",
            "program_root": "/repository/.work/wt/w9-program-e240",
        }

        command = adapter_runner._source_adapter_build_command(
            pathlib.Path("/repository/dev/consolidation/w9-source-build.sh"),
            pathlib.Path("/repository/.work/tmp/adapter-source"),
            source_record,
        )

        self.assertEqual(
            command,
            [
                "/repository/dev/consolidation/w9-source-build.sh",
                "/repository/.work/tmp/adapter-source",
                "build",
                "cargo",
                "test",
                "--locked",
                "--profile",
                "verification",
                "--target",
                "x86_64-unknown-linux-musl",
                "-p",
                "xsh",
                "--test",
                "integration",
                "--no-run",
                "--message-format=json-render-diagnostics",
            ],
        )
        self.assertNotIn("W9_PROGRAM_ROOT=/repository/.work/wt/w9-program-e240", command)

    def test_build_command_passes_the_pinned_builder_plan(self):
        repository = pathlib.Path(adapter_runner.__file__).resolve().parents[2]
        runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        scratch_parent = runner.CONTROLLER_ROOT / ".work/wt"
        scratch_parent.mkdir(parents=True, exist_ok=True)

        with tempfile.TemporaryDirectory(prefix="w9-build-plan-", dir=scratch_parent) as scratch:
            source_root = pathlib.Path(scratch)
            tests_path = source_root / "tests/sema.rs"
            tests_path.parent.mkdir()
            tests_path.write_bytes(b"#[test]\nfn original_test() {}\n")
            for command in (
                ["git", "init", "--quiet", str(source_root)],
                ["git", "-C", str(source_root), "config", "user.name", "Test"],
                ["git", "-C", str(source_root), "config", "user.email", "test@example.invalid"],
                ["git", "-C", str(source_root), "add", "tests/sema.rs"],
                ["git", "-C", str(source_root), "commit", "--quiet", "-m", "test(fixtures): initialize source"],
            ):
                subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            commit, tree = runner.source_identity(source_root)
            runner.exclude_work_from_git(source_root)
            helper_output = adapter_runner._new_derived_helper_directory(source_root, runner)
            helper, helper_sha256, owner_sha256, _owner_source_sha256 = (
                adapter_runner.derive_helper(
                    approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
                    owner_helper=adapter_runner.OWNER_HELPER,
                    output_directory=helper_output,
                    output_root=source_root,
                )
            )
            owner_helper = helper_output / "w9-publication-owner-helper.rs"
            consolidation = source_root / ".work/consolidation"
            adapter_include = consolidation / "w9-publication-adapter.rs"
            owner_include = consolidation / "w9-publication-owner-helper.rs"
            adapter_runner._write_new_file(
                adapter_include, runner.read_regular_bytes(helper), runner
            )
            adapter_runner._write_new_file(
                owner_include, runner.read_regular_bytes(owner_helper), runner
            )
            self.assertEqual(hashlib.sha256(adapter_include.read_bytes()).hexdigest(), helper_sha256)
            self.assertEqual(hashlib.sha256(owner_include.read_bytes()).hexdigest(), owner_sha256)
            overlay = adapter_runner._adapter_overlay(tests_path.read_bytes(), runner)
            tests_path.write_bytes(overlay)
            overlay_sha256 = hashlib.sha256(overlay).hexdigest()
            command = adapter_runner._source_adapter_build_command(
                runner.SOURCE_BUILDER_PATH,
                source_root,
                {
                    "build_profile": "verification",
                    "build_target": "x86_64-unknown-linux-musl",
                    "program_root": str(runner.PROGRAM_SNAPSHOT_ROOT),
                },
            )
            environment = os.environ.copy()
            environment.pop("LD_PRELOAD", None)
            environment.update(
                {
                    "W9_SOURCE_COMMIT": commit,
                    "W9_SOURCE_TREE": tree,
                    "W9_ROLE": "candidate",
                    "W9_OVERLAY_SHA256": overlay_sha256,
                    "W9_LINKER_PATH": str(runner.SOURCE_LINKER_PATH),
                    "W9_LINKER_SHA256": runner.SOURCE_LINKER_SHA256,
                }
            )

            artifact_root = adapter_runner._planned_adapter_artifact_root(
                command, environment, runner
            )
            helper_bytes = adapter_include.read_bytes()
            adapter_include.write_bytes(helper_bytes + b"\n")
            changed_artifact_root = adapter_runner._planned_adapter_artifact_root(
                command, environment, runner
            )
            adapter_include.write_bytes(helper_bytes)

        target_prefix = f"w9-target-{commit}-{tree}-"
        self.assertTrue(artifact_root.name.startswith(target_prefix))
        self.assertRegex(artifact_root.name[len(target_prefix):], r"^[0-9a-f]{64}-candidate$")
        self.assertNotEqual(changed_artifact_root, artifact_root)

    def test_artifact_root_comes_from_same_builder_request_and_environment(self):
        artifact_root = pathlib.Path("/repository/.work/tmp/w9-target-source-role")
        plan_arguments = b"/usr/bin/docker\0-e\0CARGO_TARGET_DIR=/repository/.work/tmp/w9-target-source-role\0"
        parsed = SimpleNamespace(cargo_target_directory_from_plan=unittest.mock.Mock(return_value=artifact_root))
        command = [
            "/repository/dev/consolidation/w9-source-build.sh",
            "/repository/.work/wt/reference",
            "build",
            "cargo",
            "test",
            "--locked",
            "--profile",
            "verification",
        ]
        environment = {
            "W9_SOURCE_COMMIT": "a" * 40,
            "W9_SOURCE_TREE": "b" * 40,
            "W9_ROLE": "reference",
            "W9_OVERLAY_SHA256": "c" * 64,
            "W9_LINKER_PATH": "/repository/dev/musl-linker.sh",
            "W9_LINKER_SHA256": "d" * 64,
        }
        result = SimpleNamespace(returncode=0, stdout=plan_arguments, stderr=b"")

        with patch.object(adapter_runner.subprocess, "run", return_value=result) as launch:
            selected = adapter_runner._planned_adapter_artifact_root(command, environment, parsed)

        self.assertEqual(selected, artifact_root)
        self.assertEqual(
            launch.call_args.args[0],
            [*command[:2], "plan-build", *command[3:]],
        )
        self.assertEqual(launch.call_args.kwargs["env"], environment)
        self.assertEqual(launch.call_args.kwargs["stdout"], adapter_runner.subprocess.PIPE)
        self.assertEqual(launch.call_args.kwargs["stderr"], adapter_runner.subprocess.PIPE)
        parsed.cargo_target_directory_from_plan.assert_called_once_with(plan_arguments)

    def test_artifact_plan_failure_stops_before_cargo_output_selection(self):
        parsed = SimpleNamespace(cargo_target_directory_from_plan=unittest.mock.Mock())
        command = ["builder", "source", "build", "cargo", "test"]
        result = SimpleNamespace(returncode=2, stdout=b"", stderr=b"source identity differs")

        with patch.object(adapter_runner.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(
                adapter_runner.PublicationError,
                "source-qualified adapter build planning failed with status 2: source identity differs",
            ):
                adapter_runner._planned_adapter_artifact_root(command, {}, parsed)

        parsed.cargo_target_directory_from_plan.assert_not_called()


class TuiPublicationRunnerAlignmentTests(unittest.TestCase):
    def _schema_publication(self, rendered, schema):
        adapter = adapter_runner.adapter
        prefix, entries = adapter._struct_parts(rendered, "CheckOutput")
        if schema == "legacy":
            entries = [
                entry
                for entry in entries
                if adapter._field(entry)[0] not in {"selected_function_calls", "checked_control_flow"}
            ]
        entries.extend(
            (
                "static_callable_aliases:{}",
                "resolved_bindings:ResolvedBindings{declarations:{},value_sites:{},tails:{}}",
                "prepared_constants:PreparedConstants{binding_values:{},values:{},"
                + ("global_bindings:{},tail_bindings:{}," if schema == "legacy" else "")
                + "cli_plans:Mutex{..,data:{},poisoned:false}}",
            )
        )
        return f"{prefix}{{{','.join(entries)}}}"

    def test_four_profiles_align_real_tui_source_and_compare_with_unrelated_owner_differences(self):
        reference_publication = self._schema_publication(
            source_fixtures.publication(source_fixtures.OLD_SOURCE)[0], "legacy"
        )
        candidate_publication = self._schema_publication(
            source_fixtures.publication(
                source_fixtures.CURRENT_SOURCE,
                reverse_maps=True,
            )[0],
            "current",
        )
        reference_publications = [reference_publication] * len(adapter_runner.PROFILES)
        candidate_publications = [candidate_publication] * len(adapter_runner.PROFILES)
        reference_owners = source_fixtures.owner_index(source_fixtures.OLD_SOURCE)
        candidate_owners = source_fixtures.owner_index(source_fixtures.CURRENT_SOURCE)
        unrelated_span = source_fixtures.span(0, 3, 2)
        reference_owners[unrelated_span] = {"expr:Name"}
        candidate_owners[unrelated_span] = {"stmt:Return"}
        reference_sizes = source_fixtures.source_sizes(source_fixtures.OLD_SOURCE)
        candidate_sizes = source_fixtures.source_sizes(source_fixtures.CURRENT_SOURCE)

        aligned = adapter_runner._align_profile_publications(
            reference_publications,
            candidate_publications,
            (reference_sizes, reference_owners, {}, {}),
            (candidate_sizes, candidate_owners, {}, {}),
            source_fixtures.OLD_SOURCE,
            source_fixtures.CURRENT_SOURCE,
        )

        self.assertEqual(aligned.tui_source_id, 5)
        self.assertEqual(aligned.candidate_source_sizes, reference_sizes)
        self.assertEqual(aligned.candidate_source_owners[unrelated_span], {"stmt:Return"})
        for index in range(len(adapter_runner.PROFILES)):
            with self.subTest(profile=adapter_runner.PROFILES[index]):
                adapter_runner.adapter.compare_publications(
                    "",
                    aligned.reference_publications[index],
                    reference_sizes,
                    reference_owners,
                    {},
                    {},
                    "",
                    aligned.candidate_publications[index],
                    aligned.candidate_source_sizes,
                    aligned.candidate_source_owners,
                )

    def test_parsed_alignment_comparison_preserves_profile_reports_without_rescanning(self):
        source = source_fixtures.OLD_SOURCE
        reference_base = self._schema_publication(
            source_fixtures.publication(source, include_tui=False)[0], "legacy"
        )
        candidate_base = self._schema_publication(
            source_fixtures.publication(source, include_tui=False)[0], "current"
        )

        def labelled(publication, label):
            return publication.replace(
                'literal:"Span{source_id:SourceId(5),start:999,len:2}"',
                f'literal:"{label}"',
                1,
            )

        reference_publications = [
            labelled(reference_base, "alpha"),
            labelled(reference_base, "alpha"),
            labelled(reference_base, "beta"),
            labelled(reference_base, "beta"),
        ]
        candidate_publications = [
            labelled(candidate_base, "alpha"),
            labelled(candidate_base, "alpha"),
            labelled(candidate_base, "gamma"),
            labelled(candidate_base, "beta"),
        ]
        reference_diagnostics = ["", "", "", ""]
        candidate_diagnostics = ["", "", "", "[profile difference]"]
        source_sizes = source_fixtures.source_sizes(source)
        source_owners = source_fixtures.owner_index(source)
        adapter = adapter_runner.adapter
        original_struct_parts = adapter._struct_parts

        def compare_profiles(use_parsed):
            with patch.object(adapter, "_struct_parts", wraps=original_struct_parts) as parse_struct:
                publication_alignment = adapter_runner._align_profile_publications_with_parsed(
                    reference_publications,
                    candidate_publications,
                    (source_sizes, source_owners, {}, {}),
                    (source_sizes, source_owners, {}, {}),
                    source,
                    source,
                )
                aligned = publication_alignment.aligned
                outcomes = {}
                report = []
                for index, profile in enumerate(adapter_runner.PROFILES):
                    pair = (
                        reference_diagnostics[index],
                        aligned.reference_publications[index],
                        candidate_diagnostics[index],
                        aligned.candidate_publications[index],
                    )
                    if pair in outcomes:
                        error = outcomes[pair]
                        if error is not None:
                            report.append(
                                f"{profile}\tadapter\t{error[0].__name__}\t{error[1]}\n"
                            )
                        continue
                    try:
                        if use_parsed:
                            reference = publication_alignment.reference[index]
                            candidate = publication_alignment.candidate[index]
                            if reference is None or candidate is None:
                                raise adapter_runner.PublicationError(
                                    "aligned publication is missing parsed fields"
                                )
                            adapter._compare_parsed_publications(
                                reference_diagnostics[index],
                                reference.prefix,
                                tuple(field.rendered for field in reference.fields),
                                source_sizes,
                                source_owners,
                                {},
                                {},
                                candidate_diagnostics[index],
                                candidate.prefix,
                                tuple(field.rendered for field in candidate.fields),
                                aligned.candidate_source_sizes,
                                aligned.candidate_source_owners,
                            )
                        else:
                            adapter.compare_publications(
                                reference_diagnostics[index],
                                aligned.reference_publications[index],
                                source_sizes,
                                source_owners,
                                {},
                                {},
                                candidate_diagnostics[index],
                                aligned.candidate_publications[index],
                                aligned.candidate_source_sizes,
                                aligned.candidate_source_owners,
                            )
                    except adapter.AdapterError as error:
                        outcomes[pair] = (type(error), str(error))
                        report.append(
                            f"{profile}\tadapter\t{type(error).__name__}\t{error}\n"
                        )
                    else:
                        outcomes[pair] = None
                checkoutput_parses = sum(
                    call.args[1] == "CheckOutput" for call in parse_struct.call_args_list
                )
                return aligned, "".join(report).encode(), checkoutput_parses

        raw_aligned, raw_report, raw_parse_count = compare_profiles(use_parsed=False)
        parsed_aligned, parsed_report, parsed_parse_count = compare_profiles(use_parsed=True)
        self.assertEqual(raw_aligned.reference_publications, parsed_aligned.reference_publications)
        self.assertEqual(raw_aligned.candidate_publications, parsed_aligned.candidate_publications)
        self.assertEqual(raw_report, parsed_report)
        self.assertEqual(raw_parse_count, 9)
        self.assertEqual(parsed_parse_count, 5)

    def test_alignment_rejects_a_single_profile_with_different_tui_declarations(self):
        reference_publication = source_fixtures.publication(source_fixtures.OLD_SOURCE)[0]
        candidate_publications = [
            source_fixtures.publication(
                source_fixtures.CURRENT_SOURCE,
                name_change="different" if index == 3 else None,
            )[0]
            for index in range(len(adapter_runner.PROFILES))
        ]

        with self.assertRaisesRegex(
            source_fixtures.SourceAlignmentError,
            "declarations disagree across candidate profiles",
        ):
            adapter_runner._align_profile_publications(
                [reference_publication] * len(adapter_runner.PROFILES),
                candidate_publications,
                (
                    source_fixtures.source_sizes(source_fixtures.OLD_SOURCE),
                    source_fixtures.owner_index(source_fixtures.OLD_SOURCE),
                    {},
                    {},
                ),
                (
                    source_fixtures.source_sizes(source_fixtures.CURRENT_SOURCE),
                    source_fixtures.owner_index(source_fixtures.CURRENT_SOURCE),
                    {},
                    {},
                ),
                source_fixtures.OLD_SOURCE,
                source_fixtures.CURRENT_SOURCE,
            )


if __name__ == "__main__":
    unittest.main()
