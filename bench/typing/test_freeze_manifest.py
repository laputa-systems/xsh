import importlib.util
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location("typing_manifest", Path(__file__).with_name("freeze_manifest.py"))
manifest = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(manifest)


class HandoffTests(unittest.TestCase):
    def setUp(self):
        self.original = "a" * 40
        self.current = "b" * 40
        self.selection = {"snapshot_commit": self.original}
        self.annotations = {"snapshot_commit": self.original}
        self.build = {"source_revision": self.current, "status": "passed"}
        self.correction = {
            "historical_revision": self.original,
            "settled_handoff_revision": self.current,
            "frozen_denominators_unchanged": True,
            "frozen_cohort_inputs_unchanged": True,
            "inference_implementation_started": False,
        }

    def test_corrected_compiler_preserves_original_cohort_identity(self):
        self.assertEqual(manifest.handoff_revision(self.selection, self.annotations, self.build, self.correction), self.current)

    def test_original_build_requires_no_correction(self):
        self.build["source_revision"] = self.original
        self.assertEqual(manifest.handoff_revision(self.selection, self.annotations, self.build), self.original)

    def test_changed_revision_requires_explicit_matching_correction(self):
        for correction in [None, {**self.correction, "historical_revision": self.current},
                           {**self.correction, "settled_handoff_revision": self.original}]:
            with self.subTest(correction=correction), self.assertRaises(ValueError):
                manifest.handoff_revision(self.selection, self.annotations, self.build, correction)

    def test_inference_or_changed_scope_cannot_refreeze_the_baseline(self):
        for key, value in [("inference_implementation_started", True),
                           ("frozen_cohort_inputs_unchanged", False),
                           ("frozen_denominators_unchanged", False)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                manifest.handoff_revision(self.selection, self.annotations, self.build, {**self.correction, key: value})

    def test_annotation_snapshot_mismatch_and_unbuilt_product_reject(self):
        with self.assertRaises(ValueError):
            manifest.handoff_revision(self.selection, {"snapshot_commit": self.current}, self.build, self.correction)
        with self.assertRaises(ValueError):
            manifest.handoff_revision(self.selection, self.annotations, {**self.build, "status": "failed"}, self.correction)

    def test_abbreviated_or_invalid_revision_rejects(self):
        for revision in ["abcd1234", "z" * 40, ""]:
            with self.subTest(revision=revision), self.assertRaises(ValueError):
                manifest.handoff_revision(self.selection, self.annotations, {**self.build, "source_revision": revision}, self.correction)
