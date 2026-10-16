import hashlib
import pathlib
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch


sys.path.insert(0, str(pathlib.Path(__file__).parent))
import w9_publication_reference_derivation as derivation
import w9_publication_reference_correction as retry_context
import w9_publication_adapter_runner as adapter_runner


class DerivationRunner:
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


class ReferenceDerivationTests(unittest.TestCase):
    def setUp(self):
        self.scratch_root = pathlib.Path(__file__).resolve().parents[2] / ".work/tmp"
        self.scratch_root.mkdir(parents=True, exist_ok=True)
        self.scratch = tempfile.TemporaryDirectory(
            prefix="w9-reference-derivation-", dir=self.scratch_root
        )
        self.root = pathlib.Path(self.scratch.name)
        self.runner = DerivationRunner()
        self.context_root = self.root / "context"
        self.context_root.mkdir()
        self._git("init", "--quiet")
        self._git("config", "user.name", "Test")
        self._git("config", "user.email", "test@example.invalid")
        scopecheck = self.root / derivation.API_IMPORT_FIX_PATH
        scopecheck.parent.mkdir(parents=True)
        scopecheck.write_bytes(self._scopecheck_before())
        regression = self.root / "tests/xsh/checker-modules.xsh"
        regression.parent.mkdir(parents=True)
        regression.write_bytes(b"before\n")
        self._git("add", ".")
        self._git("commit", "--quiet", "-m", "base")
        self.base_commit = self._git("rev-parse", "HEAD")
        self.base_tree = self._git("rev-parse", "HEAD^{tree}")
        self._git("checkout", "--quiet", "-b", "context")
        context_marker = self.root / "retry-context"
        context_marker.write_bytes(b"retry context\n")
        self._git("add", "retry-context")
        self._git("commit", "--quiet", "-m", "retry context")
        self.context_commit = self._git("rev-parse", "HEAD")
        self.context_tree = self._git("rev-parse", "HEAD^{tree}")
        self._git("checkout", "--quiet", "-b", "api-import-fix", self.base_commit)
        (self.root / derivation.API_IMPORT_FIX_PATH).write_bytes(self._scopecheck_after())
        regression.write_bytes(b"after\n")
        self._git("add", ".")
        self._git("commit", "--quiet", "-m", "approved api import fix")
        self.fix_commit = self._git("rev-parse", "HEAD")
        self.fix_tree = self._git("rev-parse", "HEAD^{tree}")
        self.fix_parent = self._git("rev-parse", "HEAD^")
        self.fix_patch = subprocess.run(
            [
                "git",
                "-C",
                str(self.root),
                "diff",
                "--binary",
                self.fix_parent,
                self.fix_commit,
                "--",
                derivation.API_IMPORT_FIX_PATH,
            ],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout
        self.fix_patch_sha256 = hashlib.sha256(self.fix_patch).hexdigest()
        self._git("checkout", "--quiet", "context")
        applied = subprocess.run(
            ["git", "apply", "-"],
            cwd=self.root,
            check=True,
            input=self.fix_patch,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self._git("add", derivation.API_IMPORT_FIX_PATH)
        self._git("commit", "--quiet", "-m", "combine api import fix")
        self.source_commit = self._git("rev-parse", "HEAD")
        self.source_tree = self._git("rev-parse", "HEAD^{tree}")
        self.source_patch = subprocess.run(
            ["git", "-C", str(self.root), "diff", "--binary", self.context_commit, self.source_commit],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout
        self.source_patch_sha256 = hashlib.sha256(self.source_patch).hexdigest()
        self._patch_pins()

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

    @staticmethod
    def _scopecheck_before():
        return (
            b"impl ScopeChecker<'_> {\n"
            b"    fn import(&mut self, import: UseStmtId, span: Span, global: bool) {\n"
            b"        let entry = self.program.arena.use_stmt(import);\n"
            b"        if let Some(name) = entry.alias.or_else(|| self.program.arena.names(entry.path).last()) {\n"
            b"            let kind = entry.resolved.as_deref().map_or(BindingKind::Runtime,\n"
            b"                |key| BindingKind::Module(Name::intern(key)));\n"
            b"            let id = self.declare(BindingOrigin::Import(import), name, span, kind, global, false);\n"
            b"            if !self.scopes.is_empty() { self.bind(name, id); }\n"
            b"        }\n"
            b"    }\n"
            b"}\n"
        )

    @classmethod
    def _scopecheck_after(cls):
        before = cls._scopecheck_before()
        marker = b"        let entry = self.program.arena.use_stmt(import);\n"
        insertion = (
            b"        if entry.resolved.is_none() && entry.alias.is_none() {\n"
            b"            let mut path = self.program.arena.names(entry.path);\n"
            b"            if let Some(name) = path.next()\n"
            b"                && path.next().is_none()\n"
            b"                && super::api_spec().module(&name.as_str()).is_some()\n"
            b"            {\n"
            b"                return;\n"
            b"            }\n"
            b"        }\n"
        )
        return before.replace(marker, marker + insertion, 1)

    def _patch_pins(self):
        patches = (
            patch.object(derivation, "CHECKOUT_ROOT", self.root),
            patch.object(derivation, "SOURCE_ROOT", self.root),
            patch.object(derivation, "SOURCE_PARENT", self.context_commit),
            patch.object(derivation, "SOURCE_COMMIT", self.source_commit),
            patch.object(derivation, "SOURCE_TREE", self.source_tree),
            patch.object(derivation, "SOURCE_PATCH_SHA256", self.source_patch_sha256),
            patch.object(derivation, "API_IMPORT_FIX_COMMIT", self.fix_commit),
            patch.object(derivation, "API_IMPORT_FIX_TREE", self.fix_tree),
            patch.object(derivation, "API_IMPORT_FIX_PARENT", self.fix_parent),
            patch.object(derivation, "API_IMPORT_FIX_PATCH_SHA256", self.fix_patch_sha256),
            patch.object(retry_context, "BASE_COMMIT", self.base_commit),
            patch.object(retry_context, "BASE_TREE", self.base_tree),
            patch.object(retry_context, "SOURCE_ROOT", self.context_root),
            patch.object(retry_context, "SOURCE_PARENT", self.base_commit),
            patch.object(retry_context, "SOURCE_COMMIT", self.context_commit),
            patch.object(retry_context, "SOURCE_TREE", self.context_tree),
            patch.object(retry_context, "PATCH_PATH", self.root / "retry-context.patch"),
            patch.object(retry_context, "PATCH_SHA256", "a" * 64),
        )
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)

        def validate_context(_source_root, _patch_path, _runner):
            return {
                "base_commit": self.base_commit,
                "base_tree": self.base_tree,
                "patch_path": str(self.root / "retry-context.patch"),
                "patch_sha256": "a" * 64,
                "source_root": str(self.context_root),
                "source_commit": self.context_commit,
                "source_tree": self.context_tree,
                "source_parent": self.base_commit,
                "source_state": "clean",
            }

        context_validator = patch.object(
            retry_context, "validate_source", side_effect=validate_context
        )
        context_validator.start()
        self.addCleanup(context_validator.stop)

    def _replace_source_pins(self):
        for name, value in (
            ("SOURCE_COMMIT", self.source_commit),
            ("SOURCE_TREE", self.source_tree),
            ("SOURCE_PATCH_SHA256", self.source_patch_sha256),
        ):
            patcher = patch.object(derivation, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)

    def _validate(self):
        return derivation.validate_source(self.root, self.runner)

    def test_validate_source_requires_the_full_exact_transform_and_records_both_fixes(self):
        identity = self._validate()

        self.assertEqual(identity["retry_context_source_commit"], self.context_commit)
        self.assertEqual(identity["retry_context_source_parent"], self.base_commit)
        self.assertEqual(identity["retry_context_patch_sha256"], "a" * 64)
        self.assertEqual(identity["api_import_fix_commit"], self.fix_commit)
        self.assertEqual(identity["api_import_fix_patch_sha256"], self.fix_patch_sha256)
        self.assertEqual(identity["source_commit"], self.source_commit)
        self.assertEqual(identity["source_patch_sha256"], self.source_patch_sha256)

    def test_validate_source_rejects_suffix_edit_even_when_source_pins_match(self):
        source_file = self.root / derivation.API_IMPORT_FIX_PATH
        source_file.write_bytes(source_file.read_bytes() + b"unexpected suffix\n")
        self._git("add", derivation.API_IMPORT_FIX_PATH)
        self._git("commit", "--quiet", "--amend", "--no-edit")
        self.source_commit = self._git("rev-parse", "HEAD")
        self.source_tree = self._git("rev-parse", "HEAD^{tree}")
        self.source_patch = subprocess.run(
            ["git", "-C", str(self.root), "diff", "--binary", self.context_commit, self.source_commit],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout
        self.source_patch_sha256 = hashlib.sha256(self.source_patch).hexdigest()
        self._replace_source_pins()

        with self.assertRaisesRegex(derivation.DerivationError, "exact retry-context plus API import patch"):
            self._validate()

    def test_validate_source_rejects_an_extra_changed_path_before_source_comparison(self):
        extra = self.root / "src/sema/check/extra.rs"
        extra.write_bytes(b"unexpected\n")
        self._git("add", extra.relative_to(self.root).as_posix())
        self._git("commit", "--quiet", "--amend", "--no-edit")
        self.source_commit = self._git("rev-parse", "HEAD")
        self.source_tree = self._git("rev-parse", "HEAD^{tree}")
        self.source_patch = subprocess.run(
            ["git", "-C", str(self.root), "diff", "--binary", self.context_commit, self.source_commit],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout
        self.source_patch_sha256 = hashlib.sha256(self.source_patch).hexdigest()
        self._replace_source_pins()

        with self.assertRaisesRegex(derivation.DerivationError, "changed paths"):
            self._validate()

    def test_schema_three_receipt_rejects_missing_extra_or_changed_fix_identity(self):
        record = self._receipt_record()
        contents = derivation.receipt_bytes(record).decode("utf-8")
        self.assertEqual(derivation.parse_receipt(self.root / "valid.receipt", contents), record)
        with self.assertRaisesRegex(derivation.DerivationError, "field count"):
            derivation.parse_receipt(self.root / "short.receipt", "\n".join(contents.splitlines()[:-1]))
        with self.assertRaisesRegex(derivation.DerivationError, "schema or role"):
            derivation.parse_receipt(
                self.root / "old-schema.receipt",
                contents.replace("schema\t3\n", "schema\t2\n", 1),
            )
        changed = dict(record, api_import_fix_patch_sha256="0" * 64)
        with self.assertRaisesRegex(derivation.DerivationError, "api_import_fix_patch_sha256"):
            derivation.validate_receipt_record(changed)
        changed = dict(record, retry_context_source_commit="0" * 40)
        with self.assertRaisesRegex(derivation.DerivationError, "retry_context_source_commit"):
            derivation.validate_receipt_record(changed)
        changed = dict(record, unexpected_field="value")
        with self.assertRaisesRegex(derivation.DerivationError, "fields differ"):
            derivation.validate_receipt_record(changed)

    def test_receipt_associations_bind_build_capture_and_immutable_original_receipts(self):
        record = self._receipt_record()
        paths = {
            name: self.root / f"{name}.receipt"
            for name in ("build", "capture", "original", "exact")
        }
        for name, path in paths.items():
            path.write_text(name, encoding="utf-8")
        record.update(
            {
                "build_receipt": str(paths["build"]),
                "build_receipt_sha256": self.runner.sha256_file(paths["build"]),
                "capture_receipt": str(paths["capture"]),
                "capture_receipt_sha256": self.runner.sha256_file(paths["capture"]),
                "original_capture_receipt": str(paths["original"]),
                "original_capture_receipt_sha256": self.runner.sha256_file(paths["original"]),
                "original_exact_capture_receipt": str(paths["exact"]),
                "original_exact_capture_receipt_sha256": self.runner.sha256_file(paths["exact"]),
            }
        )
        build = {
            "role": "derived-reference",
            "source_root": record["source_root"],
            "source_commit": record["source_commit"],
            "source_tree": record["source_tree"],
            "binary_sha256": "1" * 64,
            "helper_sha256": "2" * 64,
            "owner_helper_sha256": "3" * 64,
        }
        capture = {
            **build,
            "range_count": str(derivation.EXPECTED_RANGE_COUNT),
            "outcome_count": str(derivation.EXPECTED_OUTCOME_COUNT),
        }
        record.update(
            {
                "binary_sha256": build["binary_sha256"],
                "helper_sha256": build["helper_sha256"],
                "owner_helper_sha256": build["owner_helper_sha256"],
            }
        )
        derivation.validate_receipt_associations(
            record,
            paths["build"],
            build,
            paths["capture"],
            capture,
            paths["original"],
            paths["exact"],
            self.runner,
        )
        wrong_capture = dict(capture, source_tree="0" * 40)
        with self.assertRaisesRegex(derivation.DerivationError, "capture source identity"):
            derivation.validate_receipt_associations(
                record,
                paths["build"],
                build,
                paths["capture"],
                wrong_capture,
                paths["original"],
                paths["exact"],
                self.runner,
            )

    def test_combined_build_rejects_bad_derivation_before_building_harness(self):
        candidate_root = self.root / "candidate"
        candidate_root.mkdir()
        source_receipt = self.root / "candidate.source.receipt"
        source_receipt.write_text("candidate source\n", encoding="utf-8")
        source_record = {
            "schema": "3",
            "candidate_root": str(candidate_root),
            "candidate_commit": "c" * 40,
            "candidate_tree": "d" * 40,
        }
        runner = SimpleNamespace(
            SOURCE_COHORT_SCHEMA="3",
            real_directory=lambda path, _label: pathlib.Path(path).resolve(strict=True),
            regular_file=lambda path, _label: pathlib.Path(path).resolve(strict=True),
            parse_candidate_receipt=lambda _path: source_record,
            validate_source_cohort_inputs=lambda _record: (
                {index: object() for index in range(1690)},
                {},
            ),
            source_identity=lambda _path: (
                source_record["candidate_commit"],
                source_record["candidate_tree"],
            ),
        )
        args = SimpleNamespace(
            candidate_root=candidate_root,
            candidate_source_receipt=source_receipt,
            derived_reference_source_root=self.root,
            derivation_schema="3",
        )
        with (
            patch.object(
                adapter_runner.reference_derivation,
                "validate_source",
                side_effect=derivation.DerivationError("wrong derived source"),
            ),
            patch.object(adapter_runner, "build_adapter_harness") as build_harness,
        ):
            with self.assertRaisesRegex(adapter_runner.PublicationError, "wrong derived source"):
                adapter_runner.build_derived_reference_harness(args, runner)
        build_harness.assert_not_called()

    def _receipt_record(self):
        source = self._validate()
        paths = {}
        for name in ("build", "capture", "original", "exact"):
            path = self.root / f"placeholder-{name}.receipt"
            path.write_text(name, encoding="utf-8")
            paths[name] = path
        build = {
            "binary_sha256": "1" * 64,
            "helper_sha256": "2" * 64,
            "owner_helper_sha256": "3" * 64,
        }
        return derivation.create_receipt_record(
            source,
            paths["build"],
            build,
            paths["capture"],
            paths["original"],
            paths["exact"],
            self.runner,
        )


if __name__ == "__main__":
    unittest.main()
