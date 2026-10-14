import argparse
import io
import hashlib
import os
import pathlib
import re
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


class AdapterRawOutcomeTests(unittest.TestCase):
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
                ["git", "-C", str(source_root), "commit", "--quiet", "-m", "source"],
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
                ["git", "-C", str(source_root), "commit", "--quiet", "--allow-empty", "-m", "source drift"],
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
                output_directory=output,
                output_root=repository,
            )
            source = helper.read_text(encoding="utf-8")
            owner = (output / "w9-publication-owner-helper.rs").read_text(encoding="utf-8")

            self.assertEqual(len(helper_hash), 64)
            self.assertEqual(len(owner_hash), 64)
            self.assertEqual(owner_source_hash, adapter_runner.OWNER_HELPER_SHA256)
            self.assertIn("W9_ADAPTER_HASH_ROOT", source)
            self.assertIn("w9_hash_adapter_tree", source)
            self.assertNotIn("create_dir_all(outputdir)", owner)
            self.assertIn("w9_create_new_adapter_file", owner)

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
                ["git", "-C", str(historical_root), "commit", "--quiet", "-m", "initial"],
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
                    def create_after_preflight(value, candidate):
                        checked = require_output_path(value, candidate)
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

            def prepare_raw(path, _candidate_root, _runner):
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
    def setUp(self):
        self.repository = pathlib.Path(__file__).resolve().parents[2]
        self.runner = adapter_runner._load_publication_runner(
            self.repository / "dev/consolidation/w9-publication-run.py"
        )
        self.scratch_root = self.repository.parents[2] / ".work/wt"
        self.scratch_root.mkdir(parents=True, exist_ok=True)
        self.case_root = pathlib.Path(
            tempfile.mkdtemp(prefix="w9-adapter-reference-reuse-", dir=self.scratch_root)
        )
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
        self.candidate_root = self.case_root
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
        self.reference_root = self.scratch_root / "w9-adapter-ref-e240"
        self.reference_commit, self.reference_tree = self.runner.source_identity(
            self.reference_root
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
                f"source/{index:04}.xsh", f"{index + 1:064x}"
            )
            for index in range(1690)
        }
        self.reference = self._reference_receipt()
        self.helper = self._derive_helper()

    def _cleanup(self):
        for attempt in self.helper_attempts:
            shutil.rmtree(attempt, ignore_errors=True)
        shutil.rmtree(self.case_root, ignore_errors=True)

    def _initialize_source(self, root, source, owner_helper=None, publication_runner=None):
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
        for command in (
            ["git", "init", "--quiet", str(root)],
            ["git", "-C", str(root), "config", "user.name", "Test"],
            ["git", "-C", str(root), "config", "user.email", "test@example.invalid"],
            ["git", "-C", str(root), "add", *add_paths],
            ["git", "-C", str(root), "commit", "--quiet", "-m", "source"],
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
            "wrapper": "/home/josh/d/laputa-systems/xsh-consolidation/.work/consolidation/infra/run-native-secondary.sh",
            "wrapper_root": "/home/josh/d/laputa-systems/xsh",
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

    def test_build_reuses_only_a_fully_pinned_reference_harness(self):
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
        build_roles = []

        def build(_source, _candidate, role, *_args, **_kwargs):
            build_roles.append(role)
            return self.case_root / "candidate-harness", self.case_root / "candidate-build.receipt"

        with (
            patch.object(self.runner, "parse_candidate_receipt", return_value=source_record),
            patch.object(
                self.runner,
                "validate_all_inputs",
                return_value=(self.expected_inputs, {}, self.reference),
            ),
            patch.object(adapter_runner, "build_adapter_harness", side_effect=build),
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

        self.assertEqual(build_roles, ["candidate"])

    def test_build_reuse_rejects_wrong_reference_root_and_helper(self):
        owner_source = self.owner_helper_source
        reference_receipt, _record = self._write_build_receipt(
            "reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
            owner_source_path=owner_source,
        )
        other_root = self.artifact_root / "other/w9-adapter-ref-e240"
        other_root.parent.mkdir()
        subprocess.run(
            ["git", "clone", "--quiet", "--no-local", str(self.reference_root), str(other_root)],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        source_receipt = self.artifact_root / "candidate-source.receipt"
        source_receipt.write_text("fixture\n", encoding="utf-8")
        source_record = self._candidate_source_record()
        build_roles = []

        def build(_source, _candidate, role, *_args, **_kwargs):
            build_roles.append(role)
            return self.case_root / "candidate-harness", self.case_root / "candidate-build.receipt"

        def run_build(reference_path, reference_root):
            with (
                patch.object(self.runner, "parse_candidate_receipt", return_value=source_record),
                patch.object(
                    self.runner,
                    "validate_all_inputs",
                    return_value=(self.expected_inputs, {}, self.reference),
                ),
                patch.object(adapter_runner, "build_adapter_harness", side_effect=build),
            ):
                adapter_runner.build_adapter_harnesses(
                    argparse.Namespace(
                        candidate_root=self.candidate_root,
                        candidate_source_receipt=source_receipt,
                        reference_source_root=reference_root,
                        reference_build_receipt=reference_path,
                    ),
                    self.runner,
                )

        wrong_root_receipt, _record = self._write_build_receipt(
            "reference",
            other_root,
            self.reference_commit,
            self.reference_tree,
            owner_source_path=owner_source,
        )
        with self.assertRaisesRegex(adapter_runner.PublicationError, "different source root"):
            run_build(wrong_root_receipt, self.reference_root)
        self.assertEqual(build_roles, [])

        wrong_source_receipt, _record = self._write_build_receipt(
            "reference",
            self.reference_root,
            "e" * 40,
            self.reference_tree,
            owner_source_path=owner_source,
        )
        with self.assertRaisesRegex(adapter_runner.PublicationError, "source commit differs from its pin"):
            run_build(wrong_source_receipt, self.reference_root)
        self.assertEqual(build_roles, [])

        changed_helper_root = self.artifact_root / "changed-helper/derived"
        changed_helper_root.mkdir(parents=True)
        changed_helper = changed_helper_root / "w9-publication-adapter.rs"
        changed_helper.write_bytes(self.helper[0].read_bytes() + b"\n")
        (changed_helper_root / "w9-publication-owner-helper.rs").write_bytes(
            (self.helper[0].parent / "w9-publication-owner-helper.rs").read_bytes()
        )
        changed_helper_sha = hashlib.sha256(changed_helper.read_bytes()).hexdigest()
        changed_receipt, _record = self._write_build_receipt(
            "reference",
            self.reference_root,
            self.reference_commit,
            self.reference_tree,
            helper=(
                changed_helper,
                changed_helper_sha,
                self.helper[2],
                self.helper[3],
            ),
            owner_source_path=owner_source,
        )
        with self.assertRaisesRegex(adapter_runner.PublicationError, "helper differs from the current helper"):
            run_build(changed_receipt, self.reference_root)
        self.assertEqual(build_roles, [])

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
                    "legacy_capture_receipt": str(legacy_capture),
                    "legacy_capture_receipt_sha256": self.runner.sha256_file(legacy_capture),
                },
                "owners": reference_exact_owners if role == "reference" else candidate_exact_owners,
            }

        def compare(*args, **kwargs):
            comparisons.append((args, kwargs))
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
        ):
            result = self._run_compare(case)

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
        args.derived_reference_source_root = self.reference_root
        args.derived_reference_correction_receipt = correction_receipt
        args.original_capture_receipt = case["reference_capture_receipt"]

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
                "exact-owner reference proof names a different legacy capture receipt",
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
                "exact-owner reference proof legacy capture hash differs",
            ):
                self._run_compare_to(case, "stale-capture-hash")

        self.assertEqual(proof_calls, ["reference"])
        compare.assert_not_called()

    def _run_full_compare_progress_case(self, scenario, input_count=1):
        expected_inputs = self.expected_inputs
        reference_receipt = self.reference
        original_profiles = adapter_runner.PROFILES
        profiles = (
            original_profiles
            if scenario in {
                "cache-identical", "cache-variants", "cache-boundary", "cache-failure",
                "statement-flow", "parse-error", "empty-checked",
                "empty-checked-diagnostics",
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
            "runs": {},
            "records": {},
            "aliases": {},
            "tails": {},
            "calls": {},
            "field_accesses": {},
            "binding_initializers": binding_initializers,
        }
        exact_owner_projection = {
            "reference": {
                index: (reference_projection, {}) for index in self.expected_inputs
            },
            "candidate": {
                index: (candidate_projection, candidate_exact_owners)
                for index in self.expected_inputs
            },
        }
        observed = []
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
            if scenario == "cache-failure":
                raise adapter_runner.adapter.AdapterError("fixture comparison failure")
            if scenario == "statement-flow":
                return actual_compare(*args, **kwargs)

        stderr = io.StringIO()
        actual_compare = adapter_runner.adapter._compare_parsed_publications
        with (
            patch.object(adapter_runner.sys, "stderr", stderr),
            patch.object(self.runner, "regular_file", side_effect=regular_file),
            patch.object(self.runner, "require_sha256", side_effect=require_sha256),
            patch.object(self.runner, "validate_reference_receipt", return_value=self.reference),
            patch.object(self.runner, "parse_receipt", side_effect=parse_shard),
            patch.object(self.runner, "PROFILES", profiles),
            patch.object(adapter_runner, "_load_publication_runner", return_value=self.runner),
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
                self.artifact_root / "full-mismatches.tsv",
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
            )

        self.expected_inputs = expected_inputs
        self.reference = reference_receipt
        return result, observed, stderr.getvalue(), candidate_start

    def test_full_compare_aligns_candidate_exact_owner_spans_before_comparison(self):
        result, observed, progress, candidate_start = self._run_full_compare_progress_case("checked")

        self.assertEqual(result, (1, 0))
        self.assertEqual(len(observed), 1)
        self.assertEqual(
            observed[0][1]["candidate_exact_owners"]["exprs"][0][0],
            source_fixtures.span(5, candidate_start - source_fixtures.DELTA, 1),
        )
        self.assertNotIn("reference_exact_owners", observed[0][1])
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
            progress,
            "comparison_progress inputs=1/1 outcomes=4/4 mismatches=0\n",
        )

    def test_full_compare_reports_load_error_sources_as_processed_outcomes(self):
        result, observed, progress, _candidate_start = self._run_full_compare_progress_case("load-error")

        self.assertEqual(result, (1, 0))
        self.assertEqual(observed, [])
        self.assertEqual(
            progress,
            "comparison_progress inputs=1/1 outcomes=1/1 mismatches=0\n",
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
        candidate_runner = object()
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
