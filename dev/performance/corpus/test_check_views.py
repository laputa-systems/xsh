#!/usr/bin/env python3
import csv
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
ADAPTER = ROOT / "dev/performance/corpus/check_views.sh"
XSH_REVISION = "ddc17d3d53c819c343f9ecaa738ac67d59e9033a"
LAPUTA_REVISION = "ca658ce1da9d89c55209c25268dfdf687ff4fdc6"
CHECKPOINT = "15243e97f6fd751016ab28690ada2cadb6fc973a"
SELECTOR = "core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes"
LAPUTA_SOURCE_VALUE = os.environ.get("XSH_LAPUTA_CORPUS")
if not LAPUTA_SOURCE_VALUE:
    raise SystemExit("set XSH_LAPUTA_CORPUS to a Laputa checkout before running this test")
LAPUTA_SOURCE = Path(LAPUTA_SOURCE_VALUE).expanduser().resolve()
if not LAPUTA_SOURCE.is_dir():
    raise SystemExit(f"XSH_LAPUTA_CORPUS is not a directory: {LAPUTA_SOURCE}")


class FrozenViewAdapterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        scratch_root = ROOT / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        cls.temp = tempfile.TemporaryDirectory(prefix="frozen-view-adapter-", dir=scratch_root)
        cls.addClassCleanup(cls.temp.cleanup)
        cls.scratch = Path(cls.temp.name)
        cls.compiler_source = cls.clone_source(ROOT, cls.scratch / "compiler")
        cls.xsh_source = cls.clone_source(ROOT, cls.scratch / "xsh-source", XSH_REVISION)
        cls.laputa_source = cls.clone_source(LAPUTA_SOURCE, cls.scratch / "laputa-source", LAPUTA_REVISION)
        cls.compiler_revision = cls.git(cls.compiler_source, "rev-parse", "HEAD")
        exclude = Path(cls.git(cls.compiler_source, "rev-parse", "--git-path", "info/exclude"))
        if not exclude.is_absolute():
            exclude = cls.compiler_source / exclude
        exclude.write_text(exclude.read_text() + "\n.work/\n")
        cls.products = cls.compiler_source / ".work/frozen-view-adapter-products"
        cls.products.mkdir(parents=True)
        cls.xsh = cls.products / "xsh"
        cls.xsht = cls.products / "xsht"
        cls.write_products()
        cls.retained = cls.scratch / "retained-1040.tsv"
        with cls.retained.open("wb") as retained:
            subprocess.run(
                ["git", "-C", str(ROOT), "show", f"{CHECKPOINT}:dev/consolidation/typing-corpus/retained.tsv"],
                check=True,
                stdout=retained,
            )
        cls.views = cls.scratch / "views"
        cls.prepare_views()
        witness_output = subprocess.check_output(
            [
                "python3", "-B", str(ROOT / "dev/performance/corpus/frozen_witnesses.py"),
                str(cls.scratch / "prepared"),
            ],
            text=True,
        )
        witness_rows = csv.DictReader(witness_output.splitlines(), delimiter="\t")
        proc_sites = {
            int(row["site"])
            for row in witness_rows
            if row["classification"] in {"proc-parameter-contract", "proc-return-contract"}
        }
        with cls.retained.open(newline="") as source:
            retained_sites = {
                int(row["site"])
                for row in csv.DictReader(source, delimiter="\t")
            }
        cls.nonproc_retained_site = min(retained_sites - proc_sites)
        cls.calls = cls.scratch / "xsht-calls"

    @classmethod
    def clone_source(cls, source, destination, revision=None):
        subprocess.run(
            ["git", "clone", "--shared", "--no-checkout", str(source), str(destination)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        subprocess.run(
            ["git", "-C", str(destination), "checkout", "--detach", revision or "HEAD"],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return destination

    @staticmethod
    def git(root, *args):
        return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()

    @classmethod
    def write_products(cls):
        cls.xsh.write_text("#!/usr/bin/env bash\nexit 0\n")
        cls.xsht.write_text(
            "#!/usr/bin/env bash\n"
            "set -Eeuo pipefail\n"
            "[[ ${XSH_BIN:-} == \"$(dirname \"$0\")/xsh\" ]]\n"
            "[[ ${XSHT_BIN:-} == \"$0\" ]]\n"
            "printf '%s\\n' \"$PWD $*\" >>\"$XSHT_CALLS\"\n"
            "if [[ $1 == check && ${2:-} == --compare-with ]]; then\n"
            "  if [[ -n ${XSHT_MUTATE_SOURCE:-} ]]; then printf '\\n' >>\"$XSHT_MUTATE_SOURCE\"; fi\n"
            "  if [[ -n ${XSHT_MUTATE_PRODUCT:-} ]]; then printf '\\n' >>\"$XSHT_MUTATE_PRODUCT\"; fi\n"
            "  if [[ -n ${XSHT_MUTATE_VIEW:-} ]]; then printf '\\n' >>\"$XSHT_MUTATE_VIEW\"; fi\n"
            "  case ${XSHT_INTERFACE_MODE:-pass} in\n"
            "    pass) printf 'xsht check interface qualification: 2 callable, 1 CLI\\n'; exit 0 ;;\n"
            "    nonzero) printf 'xsht check interface qualification: 2 callable, 1 CLI\\n'; exit 23 ;;\n"
            "    error) printf 'xsht check interface qualification: 2 callable, 1 CLI\\n'; printf 'err[check.interface-compare] rejected\\n' >&2; exit 0 ;;\n"
            "    empty) printf 'xsht check interface qualification: 0 callable, 0 CLI\\n'; exit 0 ;;\n"
            "    missing) printf 'xsht check completed\\n'; exit 0 ;;\n"
            "    duplicate) printf 'xsht check interface qualification: 2 callable, 1 CLI\\n'; printf 'xsht check interface qualification: 2 callable, 1 CLI\\n'; exit 0 ;;\n"
            "    malformed) printf 'xsht check interface qualification: many callable, 1 CLI\\n'; exit 0 ;;\n"
            "  esac\n"
            "  exit 2\n"
            "fi\n"
            "if [[ $1 == check ]]; then\n"
            "  count=$(($# - 1))\n"
            "  printf 'xsht check: %s files in 0.01s (thread time by stage: check 0.001, lower 0.001)\\n' \"$count\" >&2\n"
            "  exit 0\n"
            "fi\n"
            "if [[ $1 == test && $2 == --exact ]]; then\n"
            "  if [[ ${XSHT_MODE:-pass} == skip ]]; then\n"
            "    printf 'running 1 tests\\n%s ... ignored\\ntest result: ok. 0 passed; 0 failed; 1 skipped\\n' \"$3\"\n"
            "  else\n"
            "    printf 'running 1 tests\\n%s ... ok 5ms\\ntest result: ok. 1 passed; 0 failed; 0 skipped\\n' \"$3\"\n"
            "  fi\n"
            "  exit 0\n"
            "fi\n"
            "exit 2\n"
        )
        cls.xsh.chmod(0o755)
        cls.xsht.chmod(0o755)

    @classmethod
    def prepare_views(cls):
        archived = cls.scratch / "prepared"
        original = archived / "original"
        original_xsh = original / "xsh"
        original_laputa = original / "laputa"
        original_xsh.mkdir(parents=True)
        original_laputa.mkdir(parents=True)
        with (original_xsh / ".archive.tar").open("wb") as archive:
            subprocess.run(
                ["git", "-C", str(cls.xsh_source), "archive", XSH_REVISION],
                check=True,
                stdout=archive,
            )
        subprocess.run(["tar", "-xf", str(original_xsh / ".archive.tar"), "-C", str(original_xsh)], check=True)
        (original_xsh / ".archive.tar").unlink()
        with (original_laputa / ".archive.tar").open("wb") as archive:
            subprocess.run(
                ["git", "-C", str(cls.laputa_source), "archive", LAPUTA_REVISION],
                check=True,
                stdout=archive,
            )
        subprocess.run(["tar", "-xf", str(original_laputa / ".archive.tar"), "-C", str(original_laputa)], check=True)
        (original_laputa / ".archive.tar").unlink()
        reduced = archived / "reduced-1040"
        subprocess.run(
            [
                "python3", str(ROOT / "dev/consolidation/typing-corpus/cohort.py"),
                str(original), "--reduced", str(reduced), "--retain", str(cls.retained),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        subprocess.run(
            [
                "python3", str(ROOT / "dev/consolidation/typing-corpus/apply_witness_fixture.py"),
                str(original_xsh), str(reduced / "xsh"),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        subprocess.run(
            [
                "python3", str(ROOT / "dev/consolidation/typing-corpus/validate.py"),
                str(original), str(reduced), "--retain", str(cls.retained),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        cls.views.mkdir()
        shutil.copytree(original_xsh, cls.views / "original/xsh", symlinks=True)
        shutil.copytree(original_laputa, cls.views / "original/laputa", symlinks=True)
        shutil.copytree(reduced / "xsh", cls.views / "reduced/xsh", symlinks=True)
        shutil.copytree(reduced / "laputa", cls.views / "reduced/laputa", symlinks=True)

    def command(
        self, *, mode="pass", interface_mode="pass", retained=None, views=None,
        xsh_hash=None, xsh_revision=None, mutate_source=None, mutate_product=None, mutate_view=None,
    ):
        self.calls.unlink(missing_ok=True)
        environment = {**os.environ, "XSHT_CALLS": str(self.calls), "XSHT_MODE": mode}
        environment["XSHT_INTERFACE_MODE"] = interface_mode
        if mutate_source is not None:
            environment["XSHT_MUTATE_SOURCE"] = str(mutate_source)
        if mutate_product is not None:
            environment["XSHT_MUTATE_PRODUCT"] = str(mutate_product)
        if mutate_view is not None:
            environment["XSHT_MUTATE_VIEW"] = str(mutate_view)
        return subprocess.run(
            [
                "bash", str(ADAPTER), "--frozen",
                str(self.compiler_source), self.compiler_revision,
                str(self.xsh_source), xsh_revision or XSH_REVISION,
                str(self.laputa_source), LAPUTA_REVISION,
                str(self.xsh), str(self.xsht),
                xsh_hash or hashlib.sha256(self.xsh.read_bytes()).hexdigest(),
                hashlib.sha256(self.xsht.read_bytes()).hexdigest(),
                str(retained or self.retained), str(views or self.views),
                "--selector", "xsh", SELECTOR,
            ],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=120,
        )

    def write_retained_variant(self, path, *, omit=None, change_reason=None):
        with self.retained.open(newline="") as source:
            reader = csv.DictReader(source, delimiter="\t")
            fieldnames = reader.fieldnames
            rows = [row for row in reader if int(row["site"]) != omit]
        if change_reason is not None:
            for row in rows:
                if int(row["site"]) == change_reason:
                    row["reason"] += " changed"
                    break
        with path.open("w", newline="") as destination:
            writer = csv.DictWriter(destination, fieldnames=fieldnames, delimiter="\t", lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)

    def test_reconstructs_the_accepted_1040_pair_and_requires_one_exact_pass(self):
        result = self.command()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls.read_text().splitlines()
        self.assertEqual(sum(" check " in call and " --compare-with " not in call for call in calls), 4)
        self.assertEqual(sum(" test --exact " in call for call in calls), 2)
        xsh_roots = [
            "core/wc.xsh", "core/cut.xsh", "core/tr.xsh", "core/date.xsh", "core/lib/date_parse.xsh",
            "core/system-report.xsh", "dev/gates.xsh", "dev/internal.xsh", "dev/docker.xsh", "dev/main.xsh",
        ]
        interface_calls = [call for call in calls if " check --compare-with " in call]
        self.assertEqual(interface_calls, [
            f"{self.views / 'reduced/xsh'} check --compare-with {self.views / 'original/xsh'} "
            f"--candidate-root {self.views / 'reduced/xsh'} {' '.join(xsh_roots)}",
            f"{self.views / 'reduced/laputa'} check --compare-with {self.views / 'original/laputa'} "
            f"--candidate-root {self.views / 'reduced/laputa'} pm/plan_json.xsh",
        ])
        self.assertIn(
            "paired frozen corpus: 11 roots and 1 exact selectors passed in both views; "
            "interface qualification: 4 callable, 2 CLI",
            result.stdout,
        )

    def test_rejects_a_mutated_candidate_view_before_running_xsht(self):
        path = self.views / "reduced/xsh/core/wc.xsh"
        original = path.read_bytes()
        try:
            path.write_bytes(original + b"\n")
            result = self.command()
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertFalse(self.calls.exists())
        finally:
            path.write_bytes(original)

    def test_rejects_wrong_product_hash_and_wrong_view_root_before_running_xsht(self):
        bad_product = self.command(xsh_hash="0" * 64)
        self.assertNotEqual(bad_product.returncode, 0)
        self.assertFalse(self.calls.exists())
        bad_root = self.command(views=self.scratch / "wrong-root")
        self.assertNotEqual(bad_root.returncode, 0)
        self.assertFalse(self.calls.exists())

    def test_rejects_a_wrong_source_revision_before_running_xsht(self):
        result = self.command(xsh_revision="HEAD")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.calls.exists())

    def test_rejects_nonproc_erasure_and_retained_reason_change(self):
        erased = self.scratch / "retained-nonproc-erasure.tsv"
        self.write_retained_variant(erased, omit=self.nonproc_retained_site)
        result = self.command(retained=erased)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.calls.exists())

        changed = self.scratch / "retained-reason-change.tsv"
        self.write_retained_variant(changed, change_reason=self.nonproc_retained_site)
        result = self.command(retained=changed)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.calls.exists())

    def test_rejects_skipped_exact_test(self):
        result = self.command(mode="skip")
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(self.calls.exists())
        self.assertIn("test --exact", self.calls.read_text())

    def test_rejects_failed_empty_missing_duplicate_and_malformed_interface_results(self):
        for mode in ("nonzero", "error", "empty", "missing", "duplicate", "malformed"):
            with self.subTest(mode=mode):
                result = self.command(interface_mode=mode)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                interface_calls = [
                    call for call in self.calls.read_text().splitlines()
                    if " check --compare-with " in call
                ]
                self.assertEqual(len(interface_calls), 1)

    def test_cleanup_runs_all_guards_after_success_and_preserves_primary_failure(self):
        source = self.compiler_source / "Cargo.toml"
        product = self.xsh
        view = self.views / "reduced/xsh/core/wc.xsh"
        originals = {path: path.read_bytes() for path in (source, product, view)}
        try:
            result = self.command(
                mutate_source=source, mutate_product=product, mutate_view=view,
            )
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            for expected in (
                "source checkout changed during frozen paired checking",
                "compiler product changed during frozen paired checking",
                "reduced xsh view changed during frozen paired checking",
            ):
                self.assertIn(expected, result.stderr)
            for path, original in originals.items():
                path.write_bytes(original)

            result = self.command(
                interface_mode="nonzero", mutate_source=source,
                mutate_product=product, mutate_view=view,
            )
            self.assertEqual(result.returncode, 23, result.stdout + result.stderr)
            for expected in (
                "source checkout changed during frozen paired checking",
                "compiler product changed during frozen paired checking",
                "reduced xsh view changed during frozen paired checking",
            ):
                self.assertIn(expected, result.stderr)
        finally:
            for path, original in originals.items():
                path.write_bytes(original)


if __name__ == "__main__":
    unittest.main()
