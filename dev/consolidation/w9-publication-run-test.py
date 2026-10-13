#!/usr/bin/env python3
import importlib.util
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNNER_PATH = ROOT / "dev/consolidation/w9-publication-run.py"
spec = importlib.util.spec_from_file_location("w9_publication_run", RUNNER_PATH)
runner = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = runner
spec.loader.exec_module(runner)


class PublicationRunnerTests(unittest.TestCase):
    def setUp(self):
        scratch = ROOT / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="w9-publication-", dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        subprocess.run(["git", "-C", str(self.root), "init", "--quiet"], check=True)

    def paths(self):
        return {0: ("one.xsh", "a" * 64), 1: ("two.xsh", "b" * 64)}

    def write_receipt(self, path, start, end, *, reference=False, omit_outcome=None, changed=None, terminal=True):
        expected_paths = self.paths()
        count = len(expected_paths)
        corpus = "c" * 64
        shard = 0 if reference else start // 100
        shard_count = 1 if reference else (count + 99) // 100
        receipt_range = f"0..{count}" if reference else f"{start}..{end}"
        shard_size = count if reference else 100
        lines = [
            f"corpus={count}",
            f"shard={shard}/{shard_count}",
            f"range={receipt_range}",
            f"shard_size={shard_size}",
            f"input_digest={corpus}",
            runner.OPTION_HEADER,
        ]
        indexes = range(count) if reference else range(start, end)
        for index in indexes:
            relative, source_hash = expected_paths[index]
            lines.append(f"input\t{index}\t{relative}\t{source_hash}")
            for profile in runner.PROFILES:
                if omit_outcome == (index, profile):
                    continue
                diagnostic, publication = "d" * 64, "e" * 64
                if changed == (index, profile, "diagnostic"):
                    diagnostic = "f" * 64
                if changed == (index, profile, "publication"):
                    publication = "f" * 64
                lines.append(f"output\t{index}\t{relative}\t{profile}\t{diagnostic}\t{publication}")
        if terminal:
            lines.append(f"terminal\tshard={shard}\tcompleted={count if reference else end - start}")
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        return corpus

    def candidate_receipt_fields(self):
        return {
            "schema": "2",
            "candidate_root": "/candidate",
            "candidate_commit": "a" * 40,
            "candidate_tree": "b" * 40,
            "candidate_binary": "/candidate/bin",
            "candidate_binary_sha256": "c" * 64,
            "build_receipt": "/candidate/.work/build.receipt",
            "build_receipt_sha256": "5" * 64,
            "helper": "/candidate/helper",
            "helper_sha256": "d" * 64,
            "wrapper": "/wrapper",
            "wrapper_sha256": "e" * 64,
            "wrapper_root": "/candidate",
            "reference_receipt": "/reference/receipt.tsv",
            "reference_receipt_sha256": "f" * 64,
            "reference_binary": "/reference/bin",
            "reference_binary_sha256": "0" * 64,
            "reference_validation": "/reference/validated",
            "reference_validation_sha256": "1" * 64,
            "input_root": "/corpus",
            "input_manifest": "/manifest",
            "input_manifest_sha256": "2" * 64,
            "corpus_blake3": "3" * 64,
            "runner": "/candidate/runner",
            "runner_sha256": "4" * 64,
        }

    def build_receipt_fields(self):
        contract = runner.w9_build_contract()
        return {
            "schema": contract["W9_CACHE_BUILD_SCHEMA"],
            "artifact_kind": runner.BUILD_ARTIFACT_KIND,
            "source_commit": "a" * 40,
            "source_tree": "b" * 40,
            "binary_path": "/candidate/bin",
            "binary_sha256": "c" * 64,
            "target": contract["W9_CACHE_BUILD_TARGET"],
            "profile": contract["W9_CACHE_BUILD_PROFILE"],
            "builder_image": contract["W9_CACHE_BUILD_IMAGE"],
            "builder_path": str(runner.APPROVED_BUILD_RUNNER),
            "builder_sha256": contract["W9_CACHE_BUILD_RUNNER_SHA256"],
            "helper_sha256": runner.HELPER_SHA256,
        }

    def test_cargo_artifact_selection_requires_one_integration_test_harness(self):
        binary = self.root / "integration-harness"
        binary.write_bytes(b"synthetic executable")
        binary.chmod(0o755)
        unrelated = {
            "reason": "compiler-artifact",
            "target": {"name": "xsh", "kind": ["lib"]},
            "executable": str(binary),
        }
        expected = {
            "reason": "compiler-artifact",
            "target": {"name": "integration", "kind": ["test"]},
            "executable": str(binary),
        }
        selected = runner.cargo_integration_executable(
            "\n".join((runner.json.dumps(unrelated), runner.json.dumps(expected))), self.root
        )
        self.assertEqual(selected, binary)
        with self.assertRaisesRegex(runner.PublicationError, "reported 0 integration test harness executables"):
            runner.cargo_integration_executable(runner.json.dumps(unrelated), self.root)
        with self.assertRaisesRegex(runner.PublicationError, "reported 2 integration test harness executables"):
            runner.cargo_integration_executable("\n".join((runner.json.dumps(expected), runner.json.dumps(expected))), self.root)
        outside = self.root.parent / "outside-integration-harness"
        outside.write_bytes(b"synthetic executable")
        outside.chmod(0o755)
        expected["executable"] = str(outside)
        with self.assertRaisesRegex(runner.PublicationError, "outside its source root"):
            runner.cargo_integration_executable(runner.json.dumps(expected), self.root)
        with self.assertRaisesRegex(runner.PublicationError, "output row 1 is not JSON"):
            runner.cargo_integration_executable("cargo emitted non-json output", self.root)

    def test_candidate_receipt_requires_the_current_publication_runner(self):
        alternate = self.root / "alternate-runner.py"
        alternate.write_text("print('alternate')\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "differs from current runner"):
            runner.validate_runner_binding(alternate, runner.sha256_file(alternate))

        current = pathlib.Path(runner.__file__).resolve()
        self.assertEqual(
            runner.validate_runner_binding(current, runner.sha256_file(current)),
            current,
        )

    def test_pin_validation_accepts_exact_hash_and_reports_mismatch(self):
        path = self.root / "pinned-input"
        path.write_bytes(b"approved")
        expected = runner.sha256_file(path)
        self.assertEqual(runner.require_sha256(path, expected, "fixture"), expected)
        with self.assertRaisesRegex(runner.PublicationError, "fixture: SHA-256 mismatch"):
            runner.require_sha256(path, "0" * 64, "fixture")

        pins = {
            "reference_receipt_sha256": runner.REFERENCE_RECEIPT_SHA256,
            "reference_binary_sha256": runner.REFERENCE_BINARY_SHA256,
            "reference_validation_sha256": runner.REFERENCE_VALIDATION_SHA256,
            "input_manifest_sha256": runner.INPUT_MANIFEST_SHA256,
            "corpus_blake3": runner.CORPUS_BLAKE3,
        }
        runner.validate_recorded_pins(pins)
        pins["corpus_blake3"] = "0" * 64
        with self.assertRaisesRegex(runner.PublicationError, "corpus_blake3 differs from approved pin"):
            runner.validate_recorded_pins(pins)

    def test_path_validation_rejects_symlinked_files_and_directories(self):
        target = self.root / "target"
        target.write_text("receipt\n", encoding="utf-8")
        file_link = self.root / "receipt-link"
        file_link.symlink_to(target)
        with self.assertRaisesRegex(runner.PublicationError, "symlink or non-canonical path"):
            runner.regular_file(file_link, "fixture receipt")

        directory_link = self.root / "directory-link"
        directory_link.symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(runner.PublicationError, "symlink or non-canonical path"):
            runner.real_directory(directory_link, "fixture directory")

    def test_frozen_corpus_identity_checks_paths_hashes_and_write_bits(self):
        corpus = self.root / "corpus"
        corpus.mkdir()
        source = corpus / "one.xsh"
        source.write_bytes(b"print 1\n")
        digest = runner.sha256_file(source)
        source.chmod(0o444)
        corpus.chmod(0o555)
        runner.verify_corpus_tree(corpus, {"one.xsh": digest})
        corpus.chmod(0o755)
        with self.assertRaisesRegex(runner.PublicationError, "input corpus directory is writable"):
            runner.verify_corpus_tree(corpus, {"one.xsh": digest})
        source.chmod(0o644)

    def test_frozen_corpus_rejects_tampered_source_contents(self):
        corpus = self.root / "corpus-tampered"
        corpus.mkdir()
        source = corpus / "one.xsh"
        source.write_bytes(b"approved source\n")
        expected = runner.sha256_file(source)
        source.chmod(0o444)
        corpus.chmod(0o555)

        source.chmod(0o644)
        source.write_bytes(b"tampered source\n")
        source.chmod(0o444)
        with self.assertRaisesRegex(runner.PublicationError, "input corpus source changed at one.xsh"):
            runner.verify_corpus_tree(corpus, {"one.xsh": expected})

    def test_frozen_corpus_rejects_extra_and_missing_source_paths(self):
        corpus = self.root / "corpus-paths"
        corpus.mkdir()
        source = corpus / "one.xsh"
        source.write_bytes(b"approved source\n")
        expected = {"one.xsh": runner.sha256_file(source)}
        source.chmod(0o444)
        extra = corpus / "extra.xsh"
        extra.write_bytes(b"extra source\n")
        extra.chmod(0o444)
        corpus.chmod(0o555)
        with self.assertRaisesRegex(runner.PublicationError, r"file list differs from approved manifest.*extra=\['extra.xsh'\]"):
            runner.verify_corpus_tree(corpus, expected)

        corpus.chmod(0o755)
        extra.chmod(0o644)
        extra.unlink()
        source.chmod(0o644)
        source.unlink()
        corpus.chmod(0o555)
        with self.assertRaisesRegex(runner.PublicationError, r"file list differs from approved manifest.*missing=\['one.xsh'\]"):
            runner.verify_corpus_tree(corpus, expected)

    def test_receipt_parser_rejects_duplicate_outcomes_and_malformed_rows(self):
        path = self.root / "duplicate-outcome.receipt"
        self.write_receipt(path, 0, 2)
        lines = path.read_text(encoding="utf-8").splitlines()
        duplicate = next(line for line in lines if line.startswith("output\t1\ttwo.xsh\texecution\t"))
        lines.insert(-1, duplicate)
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "duplicate outcome"):
            runner.parse_receipt(path, self.paths())

        lines = [line for line in lines if line != duplicate]
        lines.insert(6, "output\t1\ttwo.xsh\texecution\tbad")
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "malformed receipt row"):
            runner.parse_receipt(path, self.paths())

    def test_receipt_parser_rejects_malformed_header_and_digests(self):
        path = self.root / "malformed-receipt.receipt"
        self.write_receipt(path, 0, 2)
        lines = path.read_text(encoding="utf-8").splitlines()
        lines[0] = "wrong=2"
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "malformed receipt header"):
            runner.parse_receipt(path, self.paths())

        self.write_receipt(path, 0, 2)
        lines = path.read_text(encoding="utf-8").splitlines()
        lines[4] = "input_digest=not-a-digest"
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "malformed corpus digest"):
            runner.parse_receipt(path, self.paths())

    def test_candidate_receipt_parser_rejects_duplicate_fields_schema_and_bad_digest(self):
        path = self.root / "candidate-input.receipt"
        fields = self.candidate_receipt_fields()
        path.write_text("".join(f"{key}\t{value}\n" for key, value in fields.items()), encoding="utf-8")
        self.assertEqual(runner.parse_candidate_receipt(path), fields)

        path.write_text(path.read_text(encoding="utf-8") + "schema\t2\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "duplicate candidate receipt field schema"):
            runner.parse_candidate_receipt(path)

        fields["schema"] = "3"
        path.write_text("".join(f"{key}\t{value}\n" for key, value in fields.items()), encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "unsupported candidate receipt schema"):
            runner.parse_candidate_receipt(path)

        fields["schema"] = "2"
        fields["candidate_binary_sha256"] = "invalid"
        path.write_text("".join(f"{key}\t{value}\n" for key, value in fields.items()), encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "malformed candidate_binary_sha256"):
            runner.parse_candidate_receipt(path)

    def test_w9_build_receipt_rejects_stale_source_and_malformed_envelopes(self):
        path = self.root / ".work/build.receipt"
        path.parent.mkdir()
        fields = self.build_receipt_fields()
        path.write_text("".join(f"{key}\t{value}\n" for key, value in fields.items()), encoding="utf-8")
        self.assertEqual(runner.parse_build_receipt(path), fields)

        stale = dict(fields)
        stale["source_commit"] = "f" * 40
        path.write_text("".join(f"{key}\t{value}\n" for key, value in stale.items()), encoding="utf-8")
        runner.parse_build_receipt(path)
        with self.assertRaisesRegex(runner.PublicationError, "build receipt source commit mismatch"):
            runner.validate_build_receipt_binding(
                path,
                self.root,
                fields["source_commit"],
                fields["source_tree"],
                pathlib.Path(fields["binary_path"]),
                fields["binary_sha256"],
            )

        wrong_kind = dict(fields)
        wrong_kind["artifact_kind"] = "xsht-cli"
        path.write_text("".join(f"{key}\t{value}\n" for key, value in wrong_kind.items()), encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "unsupported W9 build receipt artifact"):
            runner.parse_build_receipt(path)

        wrong_helper = dict(fields)
        wrong_helper["helper_sha256"] = "5" * 64
        path.write_text("".join(f"{key}\t{value}\n" for key, value in wrong_helper.items()), encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "candidate build receipt helper hash is not pinned"):
            runner.validate_build_receipt_binding(
                path,
                self.root,
                fields["source_commit"],
                fields["source_tree"],
                pathlib.Path(fields["binary_path"]),
                fields["binary_sha256"],
            )

        del fields["builder_sha256"]
        path.write_text("".join(f"{key}\t{value}\n" for key, value in fields.items()), encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, r"fields differ: missing=\['builder_sha256'\]"):
            runner.parse_build_receipt(path)

        fields = self.build_receipt_fields()
        path.write_text("".join(f"{key}\t{value}\n" for key, value in fields.items()) + "schema\t1\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "duplicate W9 build receipt field schema"):
            runner.parse_build_receipt(path)

    def test_pinned_helper_staging_verifies_hash_and_refuses_overwrite(self):
        source = self.root / "approved-helper-source.rs"
        source.write_bytes(b"approved helper source\n")
        digest = runner.sha256_file(source)

        destination = runner.stage_helper(self.root, source, digest)
        self.assertEqual(destination, self.root / runner.HELPER_RELATIVE_PATH)
        self.assertEqual(runner.sha256_file(destination), digest)
        with self.assertRaisesRegex(runner.PublicationError, "refusing to overwrite staged parity helper"):
            runner.stage_helper(self.root, source, digest)

        symlink_root = self.root / "symlink-root"
        symlink_root.mkdir()
        source_link = self.root / "helper-link.rs"
        source_link.symlink_to(source)
        with self.assertRaisesRegex(runner.PublicationError, "symlink or non-canonical path"):
            runner.stage_helper(symlink_root, source_link, digest)

    def test_approved_helper_source_stages_at_the_documented_candidate_path(self):
        self.assertEqual(runner.sha256_file(runner.APPROVED_HELPER_SOURCE), runner.HELPER_SHA256)
        destination = runner.stage_helper(self.root)
        self.assertEqual(destination, self.root / runner.HELPER_RELATIVE_PATH)
        self.assertEqual(runner.sha256_file(destination), runner.HELPER_SHA256)

    def test_output_claim_rejects_existing_output_or_lock(self):
        output = self.root / "attempt"
        runner.check_output_available(output)
        lock = runner.output_lock_path(output)
        lock.write_text("pid=1\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "output lock exists"):
            runner.claim_output(output)
        lock.unlink()
        claimed_lock, descriptor, parent_descriptor = runner.claim_output(output)
        self.addCleanup(lambda: os.close(descriptor))
        self.addCleanup(lambda: os.close(parent_descriptor))
        self.assertTrue(output.is_dir())
        self.assertTrue(claimed_lock.is_file())
        runner.write_status(output, "incomplete", [], "fixture")
        self.assertEqual((output / "attempt-status.txt").read_text(), "status=incomplete\ncompleted_ranges=0\ndetail=fixture\n")
        with self.assertRaisesRegex(runner.PublicationError, "existing output path"):
            runner.check_output_available(output)

        target = self.root / "elsewhere"
        target.mkdir()
        link = self.root / "output-link"
        link.symlink_to(target, target_is_directory=True)
        with self.assertRaisesRegex(runner.PublicationError, "existing output path"):
            runner.check_output_available(link)

    def test_output_path_rejects_colon_used_to_split_docker_path(self):
        work = self.root / ".work"
        work.mkdir()
        unsafe = work / "run:with-colon"
        with self.assertRaisesRegex(runner.PublicationError, "PATH separator"):
            runner.require_output_path(str(unsafe), self.root)

    def test_docker_configuration_must_name_local_context_and_socket(self):
        environment = {"PATH": "/usr/bin:/bin"}
        self.assertEqual(
            runner.validate_docker_target(environment, "default", runner.DOCKER_LOCAL_ENDPOINT),
            {**environment, "DOCKER_HOST": runner.DOCKER_LOCAL_ENDPOINT},
        )
        for name in runner.DOCKER_OVERRIDE_VARIABLES:
            with self.subTest(name=name):
                with self.assertRaisesRegex(runner.PublicationError, "Docker endpoint overrides"):
                    runner.validate_docker_target({**environment, name: "remote"}, "default", runner.DOCKER_LOCAL_ENDPOINT)
        with self.assertRaisesRegex(runner.PublicationError, "context must be local"):
            runner.validate_docker_target(environment, "remote", runner.DOCKER_LOCAL_ENDPOINT)
        with self.assertRaisesRegex(runner.PublicationError, "endpoint must be local"):
            runner.validate_docker_target(environment, "default", "ssh://builder.example")

    def test_docker_endpoint_must_resolve_to_a_local_unix_socket(self):
        path = self.root / "docker.sock"
        regular = self.root / "not-a-socket"
        regular.write_text("not a socket", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "not a Unix socket"):
            runner.verify_unix_socket(regular)

        with patch.object(
            runner.os,
            "stat",
            return_value=SimpleNamespace(st_mode=runner.stat.S_IFSOCK),
        ):
            runner.verify_unix_socket(path)

    def test_global_runner_lock_blocks_different_output_paths(self):
        first_root = self.root / "candidate-one"
        second_root = self.root / "candidate-two"
        first_root.mkdir()
        second_root.mkdir()
        shared_work = self.root / ".work"
        shared_work.mkdir()
        common_git_directory = self.root / ".git"
        first_output = first_root / ".work/attempt-one"
        second_output = second_root / ".work/attempt-two"

        with patch.object(runner, "git_output", return_value=str(common_git_directory)):
            self.assertEqual(
                runner.global_runner_lock_path(first_root),
                runner.global_runner_lock_path(second_root),
            )
            claim = runner.claim_global_runner_lock(first_root, first_output)
            try:
                with self.assertRaisesRegex(runner.PublicationError, "shared lock"):
                    runner.claim_global_runner_lock(second_root, second_output)
            finally:
                runner.release_lock_claim(claim)

            second_claim = runner.claim_global_runner_lock(second_root, second_output)
            runner.release_lock_claim(second_claim)

    def test_docker_stop_timeout_still_signals_worker_process(self):
        signals = []
        waits = []

        class FakeProcess:
            pid = 123456

            def wait(self, timeout=None):
                waits.append(timeout)
                return 0

        def timed_out(*_args, **_kwargs):
            raise subprocess.TimeoutExpired("docker stop", 10)

        with patch.object(runner.subprocess, "run", side_effect=timed_out):
            with patch.object(runner.os, "killpg", side_effect=lambda _pid, sig: signals.append(sig)):
                errors = runner.stop_owned_job(
                    {"container": "stuck-daemon", "process": FakeProcess(), "output": None},
                    {"PATH": "/usr/bin:/bin"},
                )

        self.assertEqual(signals, [signal.SIGINT])
        self.assertEqual(waits, [15])
        self.assertEqual(len(errors), 2)

    def test_signal_after_output_claim_releases_locks_and_records_status(self):
        work = self.root / ".work"
        work.mkdir()
        output = work / "interrupted"
        completed = []
        claim_output = runner.claim_output

        def signal_after_claim(path):
            claim = claim_output(path)
            os.kill(os.getpid(), signal.SIGTERM)
            return claim

        with patch.object(runner, "claim_output", side_effect=signal_after_claim):
            with self.assertRaisesRegex(KeyboardInterrupt, "received signal 15"):
                with runner.claim_run_resources(self.root, output, completed):
                    self.fail("a queued SIGTERM should interrupt before entering the run body")

        self.assertTrue((output / "attempt-status.txt").is_file())
        self.assertFalse(runner.global_runner_lock_path(self.root).exists())
        self.assertFalse(runner.output_lock_path(output).exists())

    def test_signal_after_popen_is_registered_and_worker_is_stopped(self):
        work = self.root / ".work"
        work.mkdir()
        output = work / "worker-interrupted"
        completed = []
        active = []
        joined = []
        docker_calls = []

        class FakeProcess:
            pid = 987654

            def wait(self, timeout=None):
                joined.append(timeout)
                return 0

            def poll(self):
                return 0

        process = FakeProcess()
        real_pthread_sigmask = signal.pthread_sigmask
        delivered = False

        def signal_on_unblock(how, mask):
            nonlocal delivered
            if how == signal.SIG_SETMASK and not delivered:
                delivered = True
                os.kill(os.getpid(), signal.SIGTERM)
            return real_pthread_sigmask(how, mask)

        def fake_run(command, **_kwargs):
            docker_calls.append(command)
            return None

        with self.assertRaisesRegex(KeyboardInterrupt, "received signal 15"):
            with runner.claim_run_resources(self.root, output, completed):
                output_handle = (output / "worker.log").open("wb")
                job = {"container": "fixture-worker", "output": output_handle, "process": None}
                with patch.object(runner.subprocess, "Popen", return_value=process):
                    with patch.object(runner.subprocess, "run", side_effect=fake_run):
                        with patch.object(runner.os, "killpg", side_effect=ProcessLookupError):
                            with patch.object(runner.signal, "pthread_sigmask", side_effect=signal_on_unblock):
                                runner.start_and_register_job(
                                    job,
                                    active,
                                    ["synthetic-worker"],
                                    str(self.root),
                                    {"PATH": "/usr/bin:/bin"},
                                )

        self.assertEqual(active, [])
        self.assertTrue(joined)
        self.assertEqual(len(docker_calls), 2)
        self.assertTrue(output_handle.closed)
        self.assertTrue((output / "attempt-status.txt").is_file())

    def test_reference_tsv_and_candidate_filename_convention(self):
        reference_path = self.root / "receipt.tsv"
        self.write_receipt(reference_path, 0, 2, reference=True)
        reference = runner.parse_receipt(reference_path, self.paths())
        runner.validate_reference_receipt(reference_path, self.paths(), "c" * 64)
        self.assertEqual(len(reference.outcomes), 2 * 4)

        candidate_dir = self.root / "candidate"
        candidate_dir.mkdir()
        candidate_path = runner.candidate_receipt_path(candidate_dir, 0, 2)
        self.assertEqual(candidate_path.name, "parallel-0-1-all.receipt")
        self.write_receipt(candidate_path, 0, 2)
        self.assertEqual(runner.discover_candidate_receipts(candidate_dir, 2), {(0, 2): candidate_path})
        (candidate_dir / "receipt.tsv").write_text("unexpected\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "unexpected candidate receipt filename 'receipt.tsv'"):
            runner.discover_candidate_receipts(candidate_dir, 2)

    def test_incomplete_four_profile_outcomes_are_rejected(self):
        candidate_dir = self.root / "candidate"
        candidate_dir.mkdir()
        candidate_path = runner.candidate_receipt_path(candidate_dir, 0, 2)
        self.write_receipt(candidate_path, 0, 2, omit_outcome=(1, "check-cli"))
        reference_path = self.root / "receipt.tsv"
        self.write_receipt(reference_path, 0, 2, reference=True)
        reference = runner.validate_reference_receipt(reference_path, self.paths(), "c" * 64)
        with self.assertRaisesRegex(runner.PublicationError, "incomplete four-profile outcomes"):
            runner.verify_receipt_bundle(reference, candidate_dir, self.paths(), "c" * 64)

    def test_missing_terminal_is_rejected_as_incomplete(self):
        candidate_dir = self.root / "candidate"
        candidate_dir.mkdir()
        candidate_path = runner.candidate_receipt_path(candidate_dir, 0, 2)
        self.write_receipt(candidate_path, 0, 2, terminal=False)
        with self.assertRaisesRegex(runner.PublicationError, "expected exactly one completion record"):
            runner.parse_receipt(candidate_path, self.paths())

    def test_mismatch_reports_exact_profile_and_digest_field(self):
        for field in ("diagnostic", "publication"):
            with self.subTest(field=field):
                reference_path = self.root / f"{field}-receipt.tsv"
                self.write_receipt(reference_path, 0, 2, reference=True)
                reference = runner.validate_reference_receipt(reference_path, self.paths(), "c" * 64)
                candidate_dir = self.root / f"candidate-{field}"
                candidate_dir.mkdir()
                candidate_path = runner.candidate_receipt_path(candidate_dir, 0, 2)
                self.write_receipt(candidate_path, 0, 2, changed=(1, "execution", field))
                with self.assertRaisesRegex(
                    runner.PublicationError,
                    rf"{field} mismatch at \(1, 'two\.xsh', 'execution'\)",
                ):
                    runner.verify_receipt_bundle(reference, candidate_dir, self.paths(), "c" * 64)


if __name__ == "__main__":
    unittest.main()
