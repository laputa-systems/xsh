#!/usr/bin/env python3
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
ADAPTER = ROOT / "dev/performance/corpus/check.sh"
NATIVE_OUTPUT = ROOT / "dev/consolidation/evidence/typing-corpus-host-obligations/native-attempt-1/original/1.stdout"
XSH_SOURCE = ROOT
LAPUTA_SOURCE_VALUE = os.environ.get("XSH_LAPUTA_CORPUS")
if not LAPUTA_SOURCE_VALUE:
    raise SystemExit("set XSH_LAPUTA_CORPUS to a Laputa checkout before running this test")
LAPUTA_SOURCE = Path(LAPUTA_SOURCE_VALUE).expanduser().resolve()
if not LAPUTA_SOURCE.is_dir():
    raise SystemExit(f"XSH_LAPUTA_CORPUS is not a directory: {LAPUTA_SOURCE}")
if subprocess.run(
    ["git", "-C", str(LAPUTA_SOURCE), "rev-parse", "--show-toplevel"],
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
).returncode != 0:
    raise SystemExit(f"XSH_LAPUTA_CORPUS is not a Git checkout: {LAPUTA_SOURCE}")


class CorpusAdapterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        exclude = Path(
            subprocess.check_output(
                ["git", "rev-parse", "--git-path", "info/exclude"], cwd=ROOT, text=True
            ).strip()
        )
        if not exclude.is_absolute():
            exclude = ROOT / exclude
        patterns = {
            line.strip()
            for line in exclude.read_text().splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        }
        if not ({".work/", "/.work/"} & patterns):
            raise RuntimeError(f".work/ is not excluded in {exclude}")
        ignored = subprocess.run(["git", "check-ignore", "-q", "--no-index", ".work/tmp/performance-corpus-test"], cwd=ROOT)
        if ignored.returncode != 0:
            raise RuntimeError(".work/tmp is not ignored")
        scratch_root = ROOT / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        cls.temp = tempfile.TemporaryDirectory(prefix="corpus-adapter-test-", dir=scratch_root)
        cls.addClassCleanup(cls.temp.cleanup)
        cls.scratch = Path(cls.temp.name)
        cls.xsh_root = cls.scratch / "xsh"
        cls.laputa_root = cls.scratch / "laputa"
        cls.clone_source(XSH_SOURCE, cls.xsh_root)
        cls.clone_source(LAPUTA_SOURCE, cls.laputa_root)
        cls.xsh_revision = cls.git(cls.xsh_root, "rev-parse", "HEAD")
        cls.laputa_revision = cls.git(cls.laputa_root, "rev-parse", "HEAD")

    @classmethod
    def clone_source(cls, source, destination):
        subprocess.run(
            ["git", "clone", "--shared", "--no-checkout", str(source), str(destination)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        subprocess.run(
            ["git", "-C", str(destination), "checkout", "--detach", "HEAD"],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

    @staticmethod
    def git(root, *args):
        return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()

    def products(self, name, mode):
        directory = self.scratch / name
        directory.mkdir()
        xsh = directory / "xsh"
        xsht = directory / "xsht"
        xsh.write_text("#!/usr/bin/env bash\nexit 0\n")
        xsht.write_text(
            "#!/usr/bin/env bash\n"
            "set -Eeuo pipefail\n"
            "[[ ${XSH_BIN:-} == \"$(dirname \"$0\")/xsh\" ]]\n"
            "[[ ${XSHT_BIN:-} == \"$0\" ]]\n"
            f"mode={mode!r}\n"
            "if [[ $1 == check ]]; then\n"
            "  count=$(($# - 1))\n"
            "  printf 'xsht check: %s files in 0.01s (thread time by stage: discover 0, load 0, check 0, lower 0)\\n' \"$count\" >&2\n"
            "  exit 0\n"
            "fi\n"
            "if [[ $1 == test && $2 == --exact ]]; then\n"
            "  selector=$3\n"
            "  if [[ $mode == zero ]]; then exit 0; fi\n"
            "  if [[ $mode == wrong && $selector == core/tests/test-wc.xsh::* ]]; then selector=wrong::selector; fi\n"
            "  if [[ $mode == captured && $selector == core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes ]]; then\n"
            "    cat \"$CAPTURED_NATIVE_OUTPUT\"\n"
            "  else\n"
            "    printf 'running 1 tests\\n%s ... ok 5ms\\ntest result: ok. 1 passed; 0 failed; 0 skipped\\n' \"$selector\"\n"
            "  fi\n"
            "  if [[ $mode == source-mutation ]]; then touch \"$PWD/.adapter-source-mutation\"; fi\n"
            "  if [[ $mode == product-mutation ]]; then printf changed >> \"$XSH_BIN\"; fi\n"
            "  exit 0\n"
            "fi\n"
            "exit 2\n"
        )
        xsh.chmod(0o755)
        xsht.chmod(0o755)
        return xsh, xsht

    def run_adapter(self, name, mode):
        xsh, xsht = self.products(name, mode)
        env = os.environ.copy()
        env["CAPTURED_NATIVE_OUTPUT"] = str(NATIVE_OUTPUT)
        env["XSH_BIN"] = "/stale/xsh"
        env["XSHT_BIN"] = "/stale/xsht"
        command = [
            "bash", str(ADAPTER), self.xsh_revision, self.laputa_revision,
            str(xsh), str(xsht), hashlib.sha256(xsh.read_bytes()).hexdigest(),
            hashlib.sha256(xsht.read_bytes()).hexdigest(), str(self.xsh_root), str(self.laputa_root),
        ]
        return subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def test_accepts_captured_native_exact_output(self):
        captured = NATIVE_OUTPUT.read_text()
        self.assertIn("core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes ... ok ", captured)
        result = self.run_adapter("captured", "captured")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_rejects_wrong_selector(self):
        result = self.run_adapter("wrong", "wrong")
        self.assertNotEqual(result.returncode, 0)

    def test_rejects_zero_test_success(self):
        result = self.run_adapter("zero", "zero")
        self.assertNotEqual(result.returncode, 0)

    def test_rejects_source_mutation_during_run(self):
        mutation = self.xsh_root / ".adapter-source-mutation"
        try:
            result = self.run_adapter("source-mutation", "source-mutation")
            self.assertNotEqual(result.returncode, 0)
        finally:
            mutation.unlink(missing_ok=True)

    def test_rejects_product_mutation_during_run(self):
        result = self.run_adapter("product-mutation", "product-mutation")
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
