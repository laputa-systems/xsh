#!/usr/bin/env python3
import contextlib
import importlib.util
import io
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

    def test_source_scratch_checkouts_use_the_worktree_directory(self):
        self.assertEqual(
            runner.candidate_scratch_parent(self.root),
            self.root / ".work/wt",
        )

    def paths(self):
        return {
            0: runner.ManifestSource("one.xsh", "a" * 64),
            1: runner.ManifestSource("two.xsh", "b" * 64),
        }

    def receipt_paths(self):
        return {
            0: runner.ReceiptSource("one.xsh", "c" * 64),
            1: runner.ReceiptSource("two.xsh", "d" * 64),
        }

    def write_receipt(self, path, start, end, *, reference=False, omit_outcome=None, changed=None, terminal=True):
        receipt_paths = self.receipt_paths()
        count = len(receipt_paths)
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
            source = receipt_paths[index]
            relative = source.relative_path
            lines.append(f"input\t{index}\t{relative}\t{source.blake3}")
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

    def write_reference_receipt(self, path, receipt_paths, corpus_digest):
        count = len(receipt_paths)
        lines = [
            f"corpus={count}",
            "shard=0/1",
            f"range=0..{count}",
            f"shard_size={count}",
            f"input_digest={corpus_digest}",
            runner.OPTION_HEADER,
        ]
        for index, source in receipt_paths.items():
            relative = source.relative_path
            lines.append(f"input\t{index}\t{relative}\t{source.blake3}")
            for profile in runner.PROFILES:
                lines.append(f"output\t{index}\t{relative}\t{profile}\t{'d' * 64}\t{'e' * 64}")
        lines.append(f"terminal\tshard=0\tcompleted={count}")
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")

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
            "overlay_sha256": "6" * 64,
        }

    def candidate_run_fixture(self, name):
        candidate = self.root / name
        candidate.mkdir()
        subprocess.run(["git", "-C", str(candidate), "init", "--quiet"], check=True)
        subprocess.run(["git", "-C", str(candidate), "config", "user.name", "synthetic"], check=True)
        subprocess.run(
            ["git", "-C", str(candidate), "config", "user.email", "synthetic@example.invalid"], check=True
        )
        source = candidate / "source.xsh"
        source.write_text("synthetic source\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(candidate), "add", "source.xsh"], check=True)
        subprocess.run(["git", "-C", str(candidate), "commit", "--quiet", "-m", "initial source"], check=True)

        work = candidate / ".work"
        (candidate / ".git/info/exclude").write_text(".work/\n", encoding="utf-8")
        helper = candidate / runner.HELPER_RELATIVE_PATH
        helper.parent.mkdir(parents=True)
        helper.write_bytes(b"synthetic pinned helper\n")
        subprocess.run(["git", "-C", str(candidate), "add", "-f", str(helper.relative_to(candidate))], check=True)
        subprocess.run(
            ["git", "-C", str(candidate), "commit", "--quiet", "-m", "stage synthetic helper"], check=True
        )
        binary = work / "candidate-bin"
        binary.write_bytes(b"synthetic executable\n")
        binary.chmod(0o755)
        wrapper = work / "native-test-wrapper"
        wrapper.write_text(f"#!/bin/sh\n{runner.PINNED_IMAGE}\n", encoding="utf-8")
        wrapper.chmod(0o755)
        build_receipt = work / "build.receipt"
        build_receipt.write_text("synthetic build receipt\n", encoding="utf-8")
        reference_receipt = work / "reference.receipt"
        reference_binary = work / "reference-bin"
        reference_binary.write_bytes(b"synthetic reference executable\n")
        reference_binary.chmod(0o755)
        reference_validation = work / "reference-validation"
        input_manifest = work / "input-manifest.tsv"
        input_root = work / "reference-corpus"
        input_root.mkdir()
        corpus_source = input_root / "one.xsh"
        corpus_source.write_bytes(b"synthetic corpus source\n")
        corpus_source.chmod(0o444)
        input_root.chmod(0o555)
        self.addCleanup(input_root.chmod, 0o755)

        source_hash = runner.sha256_file(corpus_source)
        helper_hash = runner.sha256_file(helper)
        corpus_digest = runner.CORPUS_BLAKE3
        paths_by_index = {0: runner.ManifestSource("one.xsh", source_hash)}
        paths_by_path = {"one.xsh": source_hash}
        receipt_paths = {0: runner.ReceiptSource("one.xsh", "f" * 64)}
        input_manifest.write_text(f"0\tone.xsh\t{source_hash}\t100644\n", encoding="utf-8")
        self.write_reference_receipt(reference_receipt, receipt_paths, corpus_digest)
        reference_validation.write_text(
            "\n".join(
                (
                    "status=validated",
                    f"reference_head={runner.REFERENCE_HEAD}",
                    f"reference_binary_sha256={runner.sha256_file(reference_binary)}",
                    f"helper_sha256={helper_hash}",
                    f"receipt_sha256={runner.sha256_file(reference_receipt)}",
                    f"input_root={input_root}",
                    f"input_blake3_manifest_digest={corpus_digest}",
                    f"input_file_manifest_sha256={runner.sha256_file(input_manifest)}",
                    "input_sources=1690 input_rows=1690 output_rows=6760 profiles=default,reveal,execution,check-cli",
                    f"options_header={runner.OPTION_HEADER}",
                    "",
                )
            ),
            encoding="utf-8",
        )

        commit, tree = runner.source_identity(candidate)
        record = self.candidate_receipt_fields()
        record["reference_receipt_sha256"] = runner.sha256_file(reference_receipt)
        record["reference_binary_sha256"] = runner.sha256_file(reference_binary)
        record["reference_validation_sha256"] = runner.sha256_file(reference_validation)
        record["input_manifest_sha256"] = runner.sha256_file(input_manifest)
        record["corpus_blake3"] = corpus_digest
        record.update(
            {
                "candidate_root": str(candidate),
                "candidate_commit": commit,
                "candidate_tree": tree,
                "candidate_binary": str(binary),
                "candidate_binary_sha256": runner.sha256_file(binary),
                "build_receipt": str(build_receipt),
                "helper": str(helper),
                "helper_sha256": runner.sha256_file(helper),
                "wrapper": str(wrapper),
                "wrapper_sha256": runner.sha256_file(wrapper),
                "wrapper_root": str(candidate),
                "reference_receipt": str(reference_receipt),
                "reference_binary": str(reference_binary),
                "reference_validation": str(reference_validation),
                "input_root": str(input_root),
                "input_manifest": str(input_manifest),
                "runner": str(pathlib.Path(runner.__file__).resolve()),
                "runner_sha256": runner.sha256_file(pathlib.Path(runner.__file__).resolve()),
            }
        )
        supporting_files = {
            "reference receipt": (reference_receipt, record["reference_receipt_sha256"]),
            "reference binary": (reference_binary, record["reference_binary_sha256"]),
            "reference validation": (reference_validation, record["reference_validation_sha256"]),
            "input manifest": (input_manifest, record["input_manifest_sha256"]),
        }
        return (
            record,
            source,
            binary,
            work,
            supporting_files,
            input_root,
            corpus_source,
            paths_by_index,
            paths_by_path,
        )

    def test_publication_finalization_rejects_candidate_drift_after_workers(self):
        mutations = {
            "dirty source": ("dirty", "candidate source worktree is dirty"),
            "committed tree": ("tree", "candidate commit mismatch"),
            "pinned binary": ("binary", "candidate test binary: SHA-256 mismatch"),
            "reference receipt": ("reference receipt", "approved reference receipt: SHA-256 mismatch"),
            "reference binary": ("reference binary", "approved reference binary: SHA-256 mismatch"),
            "reference validation": ("reference validation", "approved reference validation: SHA-256 mismatch"),
            "input manifest": ("input manifest", "approved input manifest: SHA-256 mismatch"),
            "corpus source": ("corpus", "input corpus source changed at one.xsh"),
        }
        for name, (mutation, expected_error) in mutations.items():
            with self.subTest(mutation=name):
                (
                    record,
                    source,
                    binary,
                    work,
                    supporting_files,
                    input_root,
                    corpus_source,
                    paths,
                    paths_by_path,
                ) = self.candidate_run_fixture(
                    f"candidate-{mutation.replace(' ', '-')}"
                )
                receipt = self.root / f"{mutation}.receipt"
                receipt.write_text("synthetic candidate receipt\n", encoding="utf-8")
                output = work / f"publication-{mutation}"
                statuses = []
                args = SimpleNamespace(
                    candidate_receipt=str(receipt),
                    output=str(output),
                    dry_run=False,
                )
                def claim_resources(_candidate_root, output_path, _completed):
                    output_path.mkdir()
                    return contextlib.nullcontext()

                def launch(_record, output_path, worker, start, end, active, _environment):
                    candidate_receipt = runner.candidate_receipt_path(output_path / "candidate", start, end)
                    candidate_receipt.write_text("synthetic worker receipt\n", encoding="utf-8")
                    job_dir = output_path / "jobs" / worker / f"{start}-{end}"
                    job_dir.mkdir(parents=True)
                    timing = job_dir / "timing.txt"
                    timing.write_text("synthetic timing\n", encoding="utf-8")
                    job = {
                        "worker": worker,
                        "start": start,
                        "end": end,
                        "receipt": candidate_receipt,
                        "timing": timing,
                        "log": job_dir / "worker.log",
                        "process": SimpleNamespace(poll=lambda: 0),
                        "output": io.BytesIO(),
                    }
                    active.append(job)
                    return job

                def mutate_during_bundle(*_args):
                    if mutation == "dirty":
                        source.write_text(source.read_text(encoding="utf-8") + "changed\n", encoding="utf-8")
                    elif mutation == "tree":
                        source.write_text(source.read_text(encoding="utf-8") + "changed\n", encoding="utf-8")
                        subprocess.run(
                            ["git", "-C", record["candidate_root"], "add", "source.xsh"], check=True
                        )
                        subprocess.run(
                            [
                                "git",
                                "-C",
                                record["candidate_root"],
                                "commit",
                                "--quiet",
                                "-m",
                                "concurrent source change",
                            ],
                            check=True,
                        )
                    elif mutation == "binary":
                        binary.write_bytes(b"changed pinned executable\n")
                    elif mutation == "corpus":
                        corpus_source.chmod(0o644)
                        corpus_source.write_bytes(b"changed corpus source\n")
                        corpus_source.chmod(0o444)
                    else:
                        path, _digest = supporting_files[mutation]
                        path.write_bytes(path.read_bytes() + b"changed\n")
                    return "synthetic complete bundle"

                with contextlib.ExitStack() as patches:
                    patches.enter_context(patch.object(runner, "HELPER_SHA256", record["helper_sha256"]))
                    patches.enter_context(patch.object(runner, "WRAPPER_SHA256", record["wrapper_sha256"]))
                    patches.enter_context(
                        patch.object(runner, "REFERENCE_RECEIPT_SHA256", record["reference_receipt_sha256"])
                    )
                    patches.enter_context(
                        patch.object(runner, "REFERENCE_BINARY_SHA256", record["reference_binary_sha256"])
                    )
                    patches.enter_context(
                        patch.object(runner, "REFERENCE_VALIDATION_SHA256", record["reference_validation_sha256"])
                    )
                    patches.enter_context(
                        patch.object(runner, "INPUT_MANIFEST_SHA256", record["input_manifest_sha256"])
                    )
                    patches.enter_context(patch.object(runner, "CORPUS_BLAKE3", record["corpus_blake3"]))
                    patches.enter_context(
                        patch.object(runner, "REFERENCE_INPUT_ROOT", pathlib.Path(record["input_root"]))
                    )
                    patches.enter_context(patch.object(runner, "validate_wrapper_root"))
                    patches.enter_context(
                        patch.object(
                            runner,
                            "validate_build_receipt_binding",
                            return_value=(pathlib.Path(record["build_receipt"]), record["build_receipt_sha256"]),
                        )
                    )
                    patches.enter_context(patch.object(runner, "parse_candidate_receipt", return_value=record))
                    patches.enter_context(
                        patch.object(runner, "parse_input_manifest", return_value=(paths, paths_by_path))
                    )
                    validate_all_inputs = patches.enter_context(
                        patch.object(runner, "validate_all_inputs", wraps=runner.validate_all_inputs)
                    )
                    patches.enter_context(patch.object(runner, "require_output_path", return_value=output))
                    patches.enter_context(patch.object(runner, "local_docker_environment", return_value={}))
                    patches.enter_context(patch.object(runner, "claim_run_resources", side_effect=claim_resources))
                    patches.enter_context(
                        patch.object(
                            runner,
                            "write_status",
                            side_effect=lambda _o, status, *_args: statuses.append(status),
                        )
                    )
                    patches.enter_context(
                        patch.object(
                            runner,
                            "expected_ranges",
                            return_value=[(index, index + 1) for index in range(4)],
                        )
                    )
                    patches.enter_context(patch.object(runner, "launch_job", side_effect=launch))
                    patches.enter_context(patch.object(runner, "compare_shard"))
                    patches.enter_context(patch.object(runner, "inspect_owned_containers", return_value=[]))
                    patches.enter_context(
                        patch.object(runner, "verify_receipt_bundle", side_effect=mutate_during_bundle)
                    )
                    patches.enter_context(patch.object(runner, "stop_jobs"))
                    with self.assertRaisesRegex(runner.PublicationError, expected_error):
                        runner.execute_run(args)

                self.assertNotIn("complete", statuses)
                self.assertEqual(validate_all_inputs.call_count, 2)

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
        with self.assertRaisesRegex(runner.PublicationError, "publication runner: SHA-256 mismatch"):
            runner.validate_runner_binding(current, "0" * 64)

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

        self.write_receipt(path, 0, 2)
        lines = path.read_text(encoding="utf-8").splitlines()
        lines[6] = "input\t0\tone.xsh\tnot-a-blake3-digest"
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "malformed source BLAKE3 for index 0"):
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

    def test_w9_build_contract_rejects_target_profile_and_builder_pin_drift(self):
        contract = runner.w9_build_contract()
        self.assertEqual(contract["W9_CACHE_BUILD_SCHEMA"], "2")
        self.assertEqual(contract["W9_CACHE_BUILD_TARGET"], "x86_64-unknown-linux-musl")
        self.assertEqual(contract["W9_CACHE_BUILD_PROFILE"], "verification")
        self.assertEqual(
            contract["W9_CACHE_BUILD_RUNNER_SHA256"],
            "3477001c6cbe8e9604cb712c10d73447b7e5ee08f0ae4f5bf606f7d8a319e46c",
        )
        config_path = self.root / "dev/consolidation/w9-cache-build-config.sh"
        config_path.parent.mkdir(parents=True)
        mutations = {
            "W9_CACHE_BUILD_TARGET": "aarch64-unknown-linux-musl",
            "W9_CACHE_BUILD_PROFILE": "release",
            "W9_CACHE_BUILD_RUNNER_SHA256": "0" * 64,
        }
        with patch.object(runner, "ROOT", self.root):
            for key, value in mutations.items():
                with self.subTest(key=key):
                    changed = dict(contract)
                    changed[key] = value
                    config_path.write_text(
                        "".join(f"readonly {name}={item}\n" for name, item in changed.items()),
                        encoding="utf-8",
                    )
                    runner.w9_build_contract.cache_clear()
                    with self.assertRaisesRegex(
                        runner.PublicationError,
                        f"W9 build config {key} differs from its approved pin",
                    ):
                        runner.w9_build_contract()
        runner.w9_build_contract.cache_clear()

    def test_parity_build_overlay_is_source_derived_and_adds_one_test(self):
        source = b"use std::path::PathBuf;\n"
        overlay = runner.parity_overlay_bytes(source)
        self.assertEqual(
            overlay,
            source + b'include!("../.work/consolidation/w9-repository-parity.rs");\n',
        )
        with self.assertRaisesRegex(runner.PublicationError, "already includes the parity helper"):
            runner.parity_overlay_bytes(overlay)

    def test_scratch_overlay_guard_accepts_git_status_normalization_only(self):
        tests = self.root / "tests"
        tests.mkdir()
        semantic_tests = tests / "sema.rs"
        semantic_tests.write_bytes(b"fn existing_test() {}\n")
        subprocess.run(["git", "-C", str(self.root), "config", "user.name", "synthetic"], check=True)
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.email", "synthetic@example.invalid"], check=True
        )
        subprocess.run(["git", "-C", str(self.root), "add", "tests/sema.rs"], check=True)
        subprocess.run(["git", "-C", str(self.root), "commit", "--quiet", "-m", "source"], check=True)

        semantic_tests.write_bytes(b"fn existing_test() {}\ninclude!(\"overlay.rs\");\n")
        runner.require_only_parity_overlay(self.root, "unexpected scratch changes")

        (self.root / "outside.rs").write_text("fn outside() {}\n", encoding="utf-8")
        with self.assertRaisesRegex(runner.PublicationError, "unexpected scratch changes"):
            runner.require_only_parity_overlay(self.root, "unexpected scratch changes")

    def test_harness_listing_requires_the_exact_parity_test_once(self):
        listing = "sema::w9_repository_parity_receipt: test\nother::test: test\n"
        runner.validate_harness_test_listing(listing)
        with self.assertRaisesRegex(runner.PublicationError, "expected exactly one"):
            runner.validate_harness_test_listing("other::test: test\n")
        with self.assertRaisesRegex(runner.PublicationError, "expected exactly one"):
            runner.validate_harness_test_listing(listing + "sema::w9_repository_parity_receipt: test\n")

    def test_build_receipt_overlay_hash_is_bound_to_committed_test_source(self):
        (self.root / "tests").mkdir()
        (self.root / "tests/sema.rs").write_bytes(b"use std::path::PathBuf;\n")
        receipt = self.build_receipt_fields()
        receipt["overlay_sha256"] = runner.parity_overlay_sha256(self.root)
        runner.validate_overlay_binding(receipt, self.root)
        receipt["overlay_sha256"] = "f" * 64
        with self.assertRaisesRegex(runner.PublicationError, "overlay hash mismatch"):
            runner.validate_overlay_binding(receipt, self.root)

    def test_build_receipt_binding_rejects_source_tree_and_binary_mismatches(self):
        path = self.root / ".work/build.receipt"
        path.parent.mkdir()
        binary = self.root / ".work/candidate-bin"
        fields = self.build_receipt_fields()
        fields["binary_path"] = str(binary)

        def write_receipt(record):
            path.write_text("".join(f"{key}\t{value}\n" for key, value in record.items()), encoding="utf-8")

        stale_tree = dict(fields)
        stale_tree["source_tree"] = "f" * 40
        write_receipt(stale_tree)
        with self.assertRaisesRegex(runner.PublicationError, "build receipt source tree mismatch"):
            runner.validate_build_receipt_binding(
                path,
                self.root,
                fields["source_commit"],
                fields["source_tree"],
                binary,
                fields["binary_sha256"],
            )

        wrong_binary_path = dict(fields)
        wrong_binary_path["binary_path"] = str(self.root / ".work/other-bin")
        write_receipt(wrong_binary_path)
        with self.assertRaisesRegex(runner.PublicationError, "build receipt binary path mismatch"):
            runner.validate_build_receipt_binding(
                path,
                self.root,
                fields["source_commit"],
                fields["source_tree"],
                binary,
                fields["binary_sha256"],
            )

        wrong_binary_hash = dict(fields)
        wrong_binary_hash["binary_sha256"] = "f" * 64
        write_receipt(wrong_binary_hash)
        with self.assertRaisesRegex(runner.PublicationError, "build receipt binary hash mismatch"):
            runner.validate_build_receipt_binding(
                path,
                self.root,
                fields["source_commit"],
                fields["source_tree"],
                binary,
                fields["binary_sha256"],
            )

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

    def test_output_path_rejects_location_outside_candidate_work(self):
        outside = self.root / "outside"
        with self.assertRaisesRegex(runner.PublicationError, r"under candidate source \.work"):
            runner.require_output_path(str(outside), self.root)

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

    def test_local_docker_environment_removes_inherited_shell_overrides(self):
        hostile = {
            "BASH_ENV": "/home/attacker/override.sh",
            "ENV": "/home/attacker/env.sh",
            "SHELLOPTS": "xtrace",
            "BASHOPTS": "expand_aliases",
            "BASH_FUNC_docker%%": "() { printf hostile; }",
            "BASH_FUNC_timeout%%": "() { printf hostile; }",
        }
        responses = iter(("default\n", f"{runner.DOCKER_LOCAL_ENDPOINT}\n"))

        def inspect_context(_command, *, env, **_kwargs):
            self.assertFalse(hostile.keys() & env.keys())
            self.assertEqual(env["PATH"], "/usr/bin:/bin")
            return SimpleNamespace(stdout=next(responses))

        with patch.dict(
            runner.os.environ,
            {"PATH": "/home/attacker/bin:/usr/bin:/bin", **hostile},
            clear=True,
        ):
            with patch.object(runner.subprocess, "run", side_effect=inspect_context):
                with patch.object(runner, "verify_local_docker_socket"):
                    environment = runner.local_docker_environment()

        self.assertFalse(hostile.keys() & environment.keys())
        self.assertEqual(environment["DOCKER_HOST"], runner.DOCKER_LOCAL_ENDPOINT)
        self.assertEqual(environment["PATH"], "/usr/bin:/bin")

    def test_existing_container_inspection_has_a_timeout(self):
        with patch.object(
            runner.subprocess,
            "run",
            side_effect=subprocess.TimeoutExpired("docker ps", 10),
        ) as inspect:
            with self.assertRaisesRegex(runner.PublicationError, "cannot inspect Docker container"):
                runner.existing_container("w9pub-123-A-0", {"PATH": "/usr/bin:/bin"})

        self.assertEqual(inspect.call_args.kwargs["timeout"], 10)

    def test_docker_shim_applies_exact_worker_limits_from_pinned_wrapper(self):
        wrapper = runner.APPROVED_BUILD_RUNNER.parent / "run-native-secondary.sh"
        runner.require_sha256(wrapper, runner.WRAPPER_SHA256, "pinned native test wrapper")
        wrapper_text = runner.read_regular_text(wrapper)
        docker_array_line = next(line for line in wrapper_text.splitlines() if line.startswith("TASK_DOCKER=("))
        wrapper_args = runner.shlex.split(docker_array_line.removeprefix("TASK_DOCKER=(").removesuffix(")"))
        self.assertEqual(wrapper_args[:2], ["docker", "run"])
        self.assertIn("--cpus=8", wrapper_args)
        self.assertIn("--memory=16g", wrapper_args)

        job_dir = self.root / "jobs/A/0-100"
        job_dir.mkdir(parents=True)
        shim_dir = runner.docker_shim(job_dir, "fixture-worker", 2, self.root / "input")
        shim = shim_dir / "docker"
        fake_docker = self.root / "record-docker"
        record_path = self.root / "docker-argv.bin"
        fake_docker.write_text(
            "#!/usr/bin/env bash\nprintf '%s\\0' \"$@\" > \"$W9_DOCKER_ARGV_FILE\"\n",
            encoding="utf-8",
        )
        fake_docker.chmod(0o755)
        shim.write_text(
            runner.read_regular_text(shim).replace("/usr/bin/docker", str(fake_docker)),
            encoding="utf-8",
        )
        extra_resource_args = [
            "--memory", "4g", "--memory-swap=8g", "--cpus", "4",
            "--cpu-quota=50000", "--cpu-period", "100000", "--cpu-shares", "512",
            "--cpuset-cpus", "0-7", "--cpuset-mems=0",
        ]
        result = subprocess.run(
            ["/bin/bash", str(shim), "run", *wrapper_args[2:], *extra_resource_args],
            check=False,
            env={"PATH": "/usr/bin:/bin", "W9_DOCKER_ARGV_FILE": str(record_path)},
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8"))
        final_args = [value.decode("utf-8") for value in record_path.read_bytes().split(b"\0")[:-1]]
        exercised_resource_names = {
            "--cpus",
            "--cpu-period",
            "--cpu-quota",
            "--cpu-shares",
            "--cpuset-cpus",
            "--cpuset-mems",
            "--memory",
            "--memory-swap",
        }
        final_resources = [arg for arg in final_args if arg.split("=", 1)[0] in exercised_resource_names]
        self.assertEqual(final_resources, ["--cpuset-cpus=2", "--cpus=1", "--memory=1g"])

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
                with patch.object(runner, "existing_container", return_value=True):
                    errors = runner.stop_owned_job(
                        {"container": "stuck-daemon", "process": FakeProcess(), "output": None},
                        {"PATH": "/usr/bin:/bin"},
                    )

        self.assertEqual(signals, [signal.SIGINT])
        self.assertEqual(waits, [15])
        self.assertEqual(len(errors), 3)

    def test_docker_cleanup_failure_retains_shared_and_output_locks(self):
        work = self.root / ".work"
        work.mkdir()
        output = work / "cleanup-failed"
        active = []

        class FakeProcess:
            pid = 123456

            def wait(self, timeout=None):
                return 0

        job = {"container": "stuck-daemon", "process": FakeProcess(), "output": None}
        real_run = runner.subprocess.run

        def successful_stop(command, *args, **kwargs):
            if command[:3] == ["/usr/bin/docker", "stop", "--time"]:
                return SimpleNamespace(returncode=0)
            return real_run(command, *args, **kwargs)

        with self.assertRaisesRegex(runner.CleanupFailure, "remains after cleanup"):
            with runner.claim_run_resources(self.root, output, []):
                active.append(job)
                with patch.object(runner.subprocess, "run", side_effect=successful_stop):
                    with patch.object(runner.os, "killpg", side_effect=ProcessLookupError):
                        with patch.object(runner, "existing_container", return_value=True):
                            runner.stop_jobs(active, {"PATH": "/usr/bin:/bin"})

        self.assertEqual(active, [])
        self.assertTrue(runner.global_runner_lock_path(self.root).exists())
        self.assertTrue(runner.output_lock_path(output).exists())

    def test_owned_container_inspection_failure_retains_shared_and_output_locks(self):
        work = self.root / ".work"
        work.mkdir()
        output = work / "inspect-failed"

        with self.assertRaisesRegex(runner.CleanupFailure, "confirm owned Docker containers"):
            with runner.claim_run_resources(self.root, output, []):
                with patch.object(
                    runner,
                    "docker_container_names",
                    side_effect=runner.PublicationError("inspection timed out"),
                ):
                    runner.inspect_owned_containers("^w9pub-123-", {"PATH": "/usr/bin:/bin"})

        self.assertTrue(runner.global_runner_lock_path(self.root).exists())
        self.assertTrue(runner.output_lock_path(output).exists())

    def test_nonzero_docker_stop_is_safe_after_final_inspection_confirms_absence(self):
        work = self.root / ".work"
        work.mkdir()
        output = work / "container-disappeared"

        class FakeProcess:
            pid = 123456

            def wait(self, timeout=None):
                return 0

        active = [{"container": "failed-stop", "process": FakeProcess(), "output": None}]
        with runner.claim_run_resources(self.root, output, []):
            with patch.object(
                runner.subprocess,
                "run",
                return_value=SimpleNamespace(returncode=1),
            ):
                with patch.object(runner.os, "killpg", side_effect=ProcessLookupError):
                    with patch.object(runner, "existing_container", side_effect=[True, False, False]):
                        runner.stop_jobs(active, {"PATH": "/usr/bin:/bin"})

        self.assertEqual(active, [])
        self.assertFalse(runner.global_runner_lock_path(self.root).exists())
        self.assertFalse(runner.output_lock_path(output).exists())

    def test_signal_during_worker_cleanup_cannot_interrupt_lock_retention(self):
        work = self.root / ".work"
        work.mkdir()
        output = work / "cleanup-interrupted"
        active = []
        inspections = 0
        masks_seen = []
        original_mask = signal.pthread_sigmask(signal.SIG_BLOCK, set())

        class FakeProcess:
            pid = 123456

            def wait(self, timeout=None):
                return 0

        def container_remains(_name, _environment):
            nonlocal inspections
            inspections += 1
            masks_seen.append(signal.pthread_sigmask(signal.SIG_BLOCK, set()))
            if inspections == 1:
                os.kill(os.getpid(), signal.SIGTERM)
            return True

        job = {"container": "stuck-daemon", "process": FakeProcess(), "output": None}
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaisesRegex(runner.CleanupFailure, "remains after cleanup"):
                with runner.claim_run_resources(self.root, output, []):
                    active.append(job)
                    try:
                        raise runner.PublicationError("worker failed")
                    except runner.PublicationError as worker_error:
                        with patch.object(
                            runner.subprocess,
                            "run",
                            return_value=SimpleNamespace(returncode=0),
                        ):
                            with patch.object(runner.os, "killpg", side_effect=ProcessLookupError):
                                with patch.object(runner, "existing_container", side_effect=container_remains):
                                    try:
                                        runner.stop_jobs(active, {"PATH": "/usr/bin:/bin"})
                                    except runner.CleanupFailure as cleanup_error:
                                        raise runner.CleanupFailure(
                                            f"run failed: {worker_error}; {cleanup_error}"
                                        ) from worker_error

        self.assertEqual(active, [])
        self.assertEqual(inspections, 3)
        self.assertTrue(all({signal.SIGINT, signal.SIGTERM} <= mask for mask in masks_seen))
        self.assertTrue(runner.global_runner_lock_path(self.root).exists())
        self.assertTrue(runner.output_lock_path(output).exists())
        self.assertEqual(signal.pthread_sigmask(signal.SIG_BLOCK, set()), original_mask)

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

        with self.assertRaisesRegex(KeyboardInterrupt, "received signal 15"):
            with runner.claim_run_resources(self.root, output, completed):
                output_handle = (output / "worker.log").open("wb")
                job = {"container": "fixture-worker", "output": output_handle, "process": None}
                with patch.object(runner.subprocess, "Popen", return_value=process):
                    with patch.object(runner, "existing_container", return_value=False):
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

    def test_reference_receipt_blake3_source_hashes_are_distinct_from_manifest_sha256(self):
        reference_path = self.root / "receipt.tsv"
        manifest_paths = self.paths()
        receipt_paths = self.receipt_paths()
        self.write_reference_receipt(reference_path, receipt_paths, "e" * 64)

        reference = runner.validate_reference_receipt(reference_path, manifest_paths, "e" * 64)

        self.assertEqual(reference.inputs[0], receipt_paths[0])
        self.assertNotEqual(reference.inputs[0].blake3, manifest_paths[0].sha256)

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
