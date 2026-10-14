import argparse
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
        ranges, workers = adapter_runner._capture_plan(1690, runner)

        self.assertEqual(len(ranges), 68)
        self.assertEqual(sum(end - start for start, end in ranges), 1690)
        self.assertEqual({name: len(queue) for name, queue in workers.items()}, {
            "A": 17, "B": 17, "C": 17, "D": 17
        })
        self.assertEqual(1690 * len(adapter_runner.PROFILES), 6760)

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
        source_receipt = self.artifact_root / "candidate-source.receipt"
        source_receipt.write_text("fixture\n", encoding="utf-8")
        return {
            "candidate_source_receipt": source_receipt,
            "reference_capture_receipt": reference_capture,
            "candidate_capture_receipt": candidate_capture,
            "reference_ranges": reference_ranges,
            "candidate_ranges": candidate_ranges,
            "source_record": self._candidate_source_record(),
        }

    def _compare_args(self, case, suffix="compare"):
        return argparse.Namespace(
            candidate_root=self.candidate_root,
            candidate_source_receipt=case["candidate_source_receipt"],
            reference_source_root=self.reference_root,
            reference_capture_receipt=case["reference_capture_receipt"],
            candidate_capture_receipt=case["candidate_capture_receipt"],
            output=str(self.artifact_root / suffix),
        )

    def _run_compare(self, case):
        return adapter_runner.compare_captured_publications(
            self._compare_args(case),
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

    def test_run_reuses_reference_capture_and_captures_only_the_candidate(self):
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
        comparisons = []

        def capture(runner, record, output, role, _build, _binary, _count, _environment):
            capture_roles.append(role)
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

        def compare(*args):
            comparisons.append(args)
            return len(self.expected_inputs) * len(adapter_runner.PROFILES), 0

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
            patch.object(adapter_runner, "compare_full_publications", side_effect=compare),
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
                    dry_run=False,
                ),
                self.runner,
            )

        self.assertEqual(result, 0)
        self.assertEqual(capture_roles, ["candidate"])
        self.assertEqual(len(comparisons), 1)
        self.assertEqual(comparisons[0][7], reference_capture)
        output = self.candidate_root / ".work/consolidation/new-publication"
        self.assertTrue((output / "candidate/capture.receipt").is_file())
        self.assertFalse((output / "reference/capture.receipt").exists())
        self.assertEqual(
            (output / "attempt-status.txt").read_text(encoding="utf-8").splitlines()[0],
            "status=complete",
        )

    def test_compare_captures_rechecks_source_and_routes_both_roles_to_comparator(self):
        case = self._compare_case()
        comparisons = []

        def compare(*args):
            comparisons.append(args)
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
            patch.object(adapter_runner, "compare_full_publications", side_effect=compare),
        ):
            result = self._run_compare(case)

        self.assertEqual(result, 0)
        self.assertEqual(len(comparisons), 1)
        self.assertEqual(comparisons[0][2], case["candidate_capture_receipt"].parent / "receipts")
        self.assertEqual(comparisons[0][3], case["reference_capture_receipt"].parent / "raw")
        self.assertEqual(comparisons[0][4], case["candidate_capture_receipt"].parent / "raw")
        output = self.artifact_root / "compare"
        self.assertEqual(
            (output / "attempt-status.txt").read_text(encoding="utf-8").splitlines()[0],
            "status=complete",
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
        candidate_runner = object()
        loaded = []
        source_receipt = self.artifact_root / "source.receipt"
        source_receipt.write_text("fixture\n", encoding="utf-8")
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
                    "--reference-capture-receipt",
                    str(self.artifact_root / "reference/capture.receipt"),
                    "--candidate-capture-receipt",
                    str(self.artifact_root / "candidate/capture.receipt"),
                    "--output",
                    str(self.artifact_root / "compare-cli"),
                ]
            )

        self.assertEqual(result, 0)
        self.assertEqual(loaded[1], self.candidate_root / "dev/consolidation/w9-publication-run.py")


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
