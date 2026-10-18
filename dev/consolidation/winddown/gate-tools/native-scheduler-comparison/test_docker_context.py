from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "final-performance-gates")]
spec = spec_from_file_location("context_compare", HERE / "compare.py")
compare = module_from_spec(spec)
spec.loader.exec_module(compare)


class DockerContextTests(unittest.TestCase):
    def test_both_arms_preserve_linked_git_and_offline_host_tools(self):
        source = compare.ROOT.resolve()
        common = Path(compare.git(source, "rev-parse", "--path-format=absolute", "--git-common-dir")).resolve()
        for pair in ("reference", "candidate"):
            with self.subTest(pair=pair):
                command = compare.docker_command(
                    source_root=source, binary_dir=Path("/products"), output_dir=Path("/results"),
                    image=compare.PINNED_IMAGE, mode="suite", pair=pair, label="unit", container_name="unit",
                )
                options = command[:command.index(compare.PINNED_IMAGE)]
                self.assertEqual(options[options.index("--network") + 1], "none")
                self.assertEqual(options[options.index("--hostname") + 1], "xsh-test")
                self.assertEqual(options[options.index("--add-host") + 1], "xsh-test:127.0.0.1")
                mounts = [options[index + 1] for index, value in enumerate(options) if value == "-v"]
                for mount in (f"{source}:/work:ro", f"{source}:{source}:ro", f"{common}:{common}:ro", "/usr/local/bin/mold:/usr/local/bin/mold:ro"):
                    self.assertIn(mount, mounts)
                self.assertIn("--cpus=32", options)
                self.assertIn("--memory=40g", options)
                self.assertIn("XSH_MODULE_PATH=/work/core", options)

    def test_mold_must_remain_the_same_regular_executable(self):
        scratch = compare.ROOT / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            mold = Path(directory) / "mold"
            with patch.object(compare, "MOLD", mold):
                with self.assertRaises((ValueError, FileNotFoundError)):
                    compare.mold_sha256()
                mold.write_text("first")
                with self.assertRaises(ValueError):
                    compare.mold_sha256()
                mold.chmod(0o755)
                expected = compare.mold_sha256()
                compare.verify_mold(expected)
                mold.write_text("second")
                with self.assertRaises(ValueError):
                    compare.verify_mold(expected)
                mold.unlink()
                mold.symlink_to(sys.executable)
                with self.assertRaises(ValueError):
                    compare.mold_sha256()


if __name__ == "__main__":
    unittest.main()
