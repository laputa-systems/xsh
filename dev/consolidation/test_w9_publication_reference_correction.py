import hashlib
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


sys.path.insert(0, str(pathlib.Path(__file__).parent))
import w9_publication_reference_correction as correction


class CorrectionRunner:
    def real_directory(self, path, _kind):
        return pathlib.Path(path).resolve(strict=True)

    def regular_file(self, path, _kind):
        return pathlib.Path(path).resolve(strict=True)

    def read_regular_bytes(self, path):
        return pathlib.Path(path).read_bytes()

    def sha256_file(self, path):
        return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()

    def source_identity(self, root):
        return (
            self.git_output(root, "rev-parse", "HEAD"),
            self.git_output(root, "rev-parse", "HEAD^{tree}"),
        )

    def git_output(self, root, *args):
        result = subprocess.run(
            ["git", "-C", str(root), *args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return result.stdout.strip()


class ReferenceCorrectionTests(unittest.TestCase):
    def setUp(self):
        self.scratch_root = pathlib.Path(__file__).resolve().parents[2] / ".work/tmp"
        self.scratch_root.mkdir(parents=True, exist_ok=True)
        self.scratch = tempfile.TemporaryDirectory(
            prefix="w9-reference-correction-", dir=self.scratch_root
        )
        self.root = pathlib.Path(self.scratch.name)
        self.runner = CorrectionRunner()

    def tearDown(self):
        self.scratch.cleanup()

    def _git(self, *args):
        return subprocess.run(
            ["git", "-C", str(self.root), *args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ).stdout.strip()

    def _source_fixture(self):
        self._git("init", "--quiet")
        self._git("config", "user.name", "Test")
        self._git("config", "user.email", "test@example.invalid")
        for relative in correction.ALLOWED_PATCH_PATHS:
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("before\n", encoding="utf-8")
        self._git("add", ".")
        self._git("commit", "--quiet", "-m", "base")
        base_commit = self._git("rev-parse", "HEAD")
        base_tree = self._git("rev-parse", "HEAD^{tree}")
        for relative in correction.ALLOWED_PATCH_PATHS:
            (self.root / relative).write_text("after\n", encoding="utf-8")
        self._git("add", ".")
        self._git("commit", "--quiet", "-m", "correction")
        source_commit = self._git("rev-parse", "HEAD")
        source_tree = self._git("rev-parse", "HEAD^{tree}")
        patch_bytes = subprocess.run(
            ["git", "-C", str(self.root), "diff", "--binary", base_commit, source_commit],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout
        patch_path = self.root.parent / f"{self.root.name}.patch"
        patch_path.write_bytes(patch_bytes)
        self.addCleanup(patch_path.unlink, missing_ok=True)
        pins = (
            patch.object(correction, "SOURCE_ROOT", self.root),
            patch.object(correction, "PATCH_PATH", patch_path),
            patch.object(correction, "BASE_COMMIT", base_commit),
            patch.object(correction, "BASE_TREE", base_tree),
            patch.object(correction, "SOURCE_PARENT", base_commit),
            patch.object(correction, "SOURCE_COMMIT", source_commit),
            patch.object(correction, "SOURCE_TREE", source_tree),
            patch.object(correction, "PATCH_SHA256", hashlib.sha256(patch_bytes).hexdigest()),
        )
        for pin in pins:
            pin.start()
            self.addCleanup(pin.stop)
        return patch_path, patch_bytes

    def test_validate_source_accepts_the_exact_single_commit_patch(self):
        patch_path, _patch_bytes = self._source_fixture()
        identity = correction.validate_source(self.root, patch_path, self.runner)
        self.assertEqual(identity["source_commit"], correction.SOURCE_COMMIT)
        self.assertEqual(identity["source_tree"], correction.SOURCE_TREE)
        self.assertEqual(identity["source_parent"], correction.BASE_COMMIT)

    def test_validate_source_rejects_wrong_base_parent_patch_and_dirty_worktree_changes(self):
        patch_path, patch_bytes = self._source_fixture()
        with patch.object(correction, "BASE_TREE", "0" * 40):
            with self.assertRaisesRegex(correction.CorrectionError, "base tree"):
                correction.validate_source(self.root, patch_path, self.runner)
        with patch.object(correction, "SOURCE_PARENT", "0" * 40):
            with self.assertRaisesRegex(correction.CorrectionError, "parent"):
                correction.validate_source(self.root, patch_path, self.runner)
        patch_path.write_bytes(patch_bytes + b"tampered")
        with self.assertRaisesRegex(correction.CorrectionError, "SHA-256"):
            correction.validate_source(self.root, patch_path, self.runner)
        patch_path.write_bytes(patch_bytes)
        (self.root / "untracked").write_text("dirty\n", encoding="utf-8")
        with self.assertRaisesRegex(correction.CorrectionError, "not clean"):
            correction.validate_source(self.root, patch_path, self.runner)

    def test_validate_source_rejects_source_identity_and_allowed_path_drift(self):
        patch_path, _patch_bytes = self._source_fixture()
        with patch.object(correction, "SOURCE_COMMIT", "0" * 40):
            with self.assertRaisesRegex(correction.CorrectionError, "source commit or tree"):
                correction.validate_source(self.root, patch_path, self.runner)
        with patch.object(correction, "SOURCE_TREE", "0" * 40):
            with self.assertRaisesRegex(correction.CorrectionError, "source commit or tree"):
                correction.validate_source(self.root, patch_path, self.runner)
        with patch.object(correction, "ALLOWED_PATCH_PATHS", correction.ALLOWED_PATCH_PATHS[:-1]):
            with self.assertRaisesRegex(correction.CorrectionError, "changed paths"):
                correction.validate_source(self.root, patch_path, self.runner)

    def test_retry_correction_keeps_original_reference_base_and_full_coverage(self):
        self.assertEqual(correction.BASE_COMMIT, "e240efad273fe35d66d91f4037b6a8193527da2e")
        self.assertEqual(correction.BASE_TREE, "a1ef04ac7b6f759c272e8f841acee3a3b3a6ff08")
        self.assertEqual(correction.SOURCE_ROOT.name, "w9-derived-retry-context-bb")
        self.assertEqual(correction.SOURCE_PARENT, "bb29697265b29540f7d3a944a2454d2a4152dcc7")
        self.assertEqual(correction.SOURCE_COMMIT, "1fd9e5f2f366833b786e57d1fc54bcd9a238731e")
        self.assertEqual(correction.EXPECTED_RANGE_COUNT, 68)
        self.assertEqual(correction.EXPECTED_OUTCOME_COUNT, 6760)

    def _receipt_record(self):
        patch_path, _patch_bytes = self._source_fixture()
        source = correction.validate_source(self.root, patch_path, self.runner)
        paths = {}
        for name in ("build", "capture", "original", "exact"):
            path = self.root / f"{name}.receipt"
            path.write_text(name, encoding="utf-8")
            paths[name] = path
        build = {
            "role": "derived-reference",
            "source_root": str(self.root),
            "source_commit": correction.SOURCE_COMMIT,
            "source_tree": correction.SOURCE_TREE,
            "binary_sha256": "1" * 64,
            "helper_sha256": "2" * 64,
            "owner_helper_sha256": "3" * 64,
        }
        capture = {
            "role": "derived-reference",
            "source_root": str(self.root),
            "source_commit": correction.SOURCE_COMMIT,
            "source_tree": correction.SOURCE_TREE,
            "binary_sha256": "1" * 64,
            "helper_sha256": "2" * 64,
            "owner_helper_sha256": "3" * 64,
            "range_count": str(correction.EXPECTED_RANGE_COUNT),
            "outcome_count": str(correction.EXPECTED_OUTCOME_COUNT),
        }
        record = correction.create_receipt_record(
            source,
            paths["build"],
            build,
            paths["capture"],
            paths["original"],
            paths["exact"],
            self.runner,
        )
        return record, paths, build, capture

    def test_receipt_rejects_role_profile_and_capture_coverage_changes(self):
        record, _paths, _build, _capture = self._receipt_record()
        contents = correction.receipt_bytes(record).decode("utf-8")
        self.assertEqual(correction.parse_receipt(self.root / "valid.receipt", contents), record)
        for field, value, message in (
            ("role", "reference", "schema or role"),
            ("profiles", "default,reveal", "profiles"),
        ):
            changed = dict(record)
            changed[field] = value
            with self.assertRaisesRegex(correction.CorrectionError, message):
                correction.validate_receipt_record(changed)
        waived = dict(record, diagnostic_waivers="1678")
        with self.assertRaisesRegex(correction.CorrectionError, "fields differ"):
            correction.validate_receipt_record(waived)

    def test_receipt_association_rejects_changed_helper_and_outcome_coverage(self):
        record, paths, build, capture = self._receipt_record()
        correction.validate_receipt_associations(
            record,
            paths["build"],
            build,
            paths["capture"],
            capture,
            paths["original"],
            paths["exact"],
            self.runner,
        )
        changed_build = dict(build, helper_sha256="4" * 64)
        with self.assertRaisesRegex(correction.CorrectionError, "helper_sha256"):
            correction.validate_receipt_associations(
                record,
                paths["build"],
                changed_build,
                paths["capture"],
                capture,
                paths["original"],
                paths["exact"],
                self.runner,
            )
        changed_capture = dict(capture, outcome_count="6759")
        with self.assertRaisesRegex(correction.CorrectionError, "coverage"):
            correction.validate_receipt_associations(
                record,
                paths["build"],
                build,
                paths["capture"],
                changed_capture,
                paths["original"],
                paths["exact"],
                self.runner,
            )

    def test_first_difference_records_earliest_delta_after_full_coverage(self):
        original = []
        derived = []
        for index in range(correction.EXPECTED_OUTCOME_COUNT):
            key = (index // len(correction.PROFILES), str(index), correction.PROFILES[index % len(correction.PROFILES)])
            value = ("checked", "same", "same")
            original.append((key, value))
            derived_value = ("checked", "changed", "same") if index in {1, 20} else value
            derived.append((key, derived_value))
        consumed = {"original": 0, "derived": 0}

        def rows(name, values):
            for value in values:
                consumed[name] += 1
                yield value

        difference = correction.first_difference(rows("original", original), rows("derived", derived))
        self.assertEqual(difference[:4], ("0", "1", "reveal", "diagnostics"))
        self.assertEqual(consumed, {"original": correction.EXPECTED_OUTCOME_COUNT, "derived": correction.EXPECTED_OUTCOME_COUNT})
        self.assertEqual(len(correction.first_difference_bytes(difference).splitlines()), 3)

    def test_first_difference_rejects_incomplete_and_extra_outcome_streams(self):
        rows = [((index, str(index), "default"), ("checked", "", "")) for index in range(correction.EXPECTED_OUTCOME_COUNT)]
        with self.assertRaises(ValueError):
            correction.first_difference(iter(rows), iter(rows[:-1]))
        with self.assertRaises(ValueError):
            correction.first_difference(iter(rows), iter(rows + [rows[-1]]))


if __name__ == "__main__":
    unittest.main()
