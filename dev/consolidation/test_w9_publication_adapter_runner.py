import os
import pathlib
import shutil
import sys
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch


sys.path.insert(0, str(pathlib.Path(__file__).parent))
import w9_publication_adapter_runner as adapter_runner


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


if __name__ == "__main__":
    unittest.main()
