"""Archive-shape regressions for the copied-product smoke verifier."""

import importlib.util
import hashlib
from io import BytesIO
from pathlib import Path
import tarfile
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("copied-product-smoke.py")
SPEC = importlib.util.spec_from_file_location("copied_product_smoke", MODULE_PATH)
SMOKE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SMOKE)


class CoreArchiveValidationTests(unittest.TestCase):
    def test_sidecar_uses_the_archive_path_relative_to_dist(self):
        with tempfile.TemporaryDirectory(dir=SMOKE.ROOT / "dist") as directory:
            archive = Path(directory) / "core-local.tar.xz"
            archive.write_bytes(b"archive")
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            sidecar = archive.with_suffix("").with_suffix(".sha256")
            sidecar.write_text(f"{digest}  {archive.relative_to(SMOKE.ROOT).as_posix()}\n")
            SMOKE.validate_core_sidecar(archive)
            sidecar.write_text(f"{digest}  dist/core-local.tar.xz\n")
            with self.assertRaises(RuntimeError):
                SMOKE.validate_core_sidecar(archive)

    def test_expected_entries_follow_core_package_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            core = Path(directory) / "core"
            for name in ("bin/report.xsh-helper.xsh", "lib/tests/helper.xsh", "tests/ignored.xsh"):
                source = core / name
                source.parent.mkdir(parents=True, exist_ok=True)
                source.write_bytes(b"print \"ok\"\n")
            expected = SMOKE.expected_core_entries(core)
            self.assertEqual(set(expected), {"core/bin/report.xsh-helper", "core/lib/tests/helper.xsh"})
            self.assertEqual(expected["core/bin/report.xsh-helper"][1], 0o755)
            self.assertEqual(expected["core/lib/tests/helper.xsh"][1], 0o644)

    def test_accepts_exact_regular_member(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "report.xsh"
            source.write_bytes(b"print \"ok\"\n")
            for mode in (0o755, 0o100755):
                archive = Path(directory) / f"valid-{mode}.tar.xz"
                with tarfile.open(archive, "w:xz") as packaged:
                    member = tarfile.TarInfo("core/report")
                    member.mode = mode
                    member.size = source.stat().st_size
                    packaged.addfile(member, BytesIO(source.read_bytes()))
                with tarfile.open(archive, "r:xz") as packaged:
                    with self.subTest(mode=oct(mode)):
                        SMOKE.validate_core_archive(packaged, {"core/report": (source, 0o755)})

    def test_rejects_duplicate_nonfile_and_special_mode_members(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "report.xsh"
            source.write_bytes(b"print \"ok\"\n")
            expected = {"core/report": (source, 0o755)}
            for extra in ("duplicate", "symlink", "setuid"):
                archive = Path(directory) / f"{extra}.tar.xz"
                with tarfile.open(archive, "w:xz") as packaged:
                    regular = tarfile.TarInfo("core/report")
                    regular.mode = 0o4755 if extra == "setuid" else 0o755
                    regular.size = source.stat().st_size
                    packaged.addfile(regular, BytesIO(source.read_bytes()))
                    if extra == "duplicate":
                        packaged.addfile(regular, BytesIO(source.read_bytes()))
                    elif extra == "symlink":
                        link = tarfile.TarInfo("core/alias")
                        link.type = tarfile.SYMTYPE
                        link.linkname = "report"
                        packaged.addfile(link)
                with tarfile.open(archive, "r:xz") as packaged:
                    with self.subTest(extra=extra), self.assertRaises(RuntimeError):
                        SMOKE.validate_core_archive(packaged, expected)


if __name__ == "__main__":
    unittest.main()
