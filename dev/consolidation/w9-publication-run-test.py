#!/usr/bin/env python3
import importlib.util
import os
import pathlib
import sys
import tempfile
import unittest


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
