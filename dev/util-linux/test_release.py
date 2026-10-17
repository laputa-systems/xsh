from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import release
import resources
from acceptance import SourceIdentity


ROOT = Path(__file__).resolve().parents[2]
SCRATCH = ROOT / ".work" / "tmp"
SOURCE = SourceIdentity("a" * 40, "b" * 40, True)
OTHER_SOURCE = SourceIdentity("c" * 40, "d" * 40, True)


class FakeRuntime:
    IMAGE = "pinned-test-image"

    def __init__(
        self,
        root,
        *,
        sources=(),
        missing=(),
        output=None,
        cleanup_error=None,
        leave_cleanup_files=False,
    ):
        self.ROOT = root
        self._sources = iter(sources)
        self._last_source = SOURCE
        self.missing = set(missing)
        self.output = output
        self.cleanup_error = cleanup_error
        self.leave_cleanup_files = leave_cleanup_files
        self.calls = []
        self.identity_calls = 0

    def source_identity(self):
        self.identity_calls += 1
        try:
            self._last_source = next(self._sources)
        except StopIteration:
            pass
        return self._last_source

    def container(self, script, *, mounts, capabilities, timeout, cpus=1, memory_gib=1):
        self.calls.append((script, mounts, capabilities, timeout, cpus, memory_gib))
        target = next(
            Path(mount.split("source=", 1)[1].split(",target=/cargo-target", 1)[0])
            for mount in mounts
            if ",target=/cargo-target," in mount
        )
        if script.startswith("python3 -c "):
            if self.cleanup_error is not None:
                raise self.cleanup_error
            if not self.leave_cleanup_files:
                for child in target.iterdir():
                    shutil.rmtree(child) if child.is_dir() and not child.is_symlink() else child.unlink()
            return b""
        product_root = target / release.TARGET / "release"
        product_root.mkdir(parents=True)
        for name in release.PRODUCTS:
            if name in self.missing:
                continue
            product = product_root / name
            product.write_bytes(name.encode())
            product.chmod(0o755)
        if self.output is not None:
            return self.output
        marker = (
            release._RESULT_PREFIX
            + SOURCE.commit.encode()
            + b":"
            + SOURCE.tree.encode()
            + b":complete\n"
        )
        return marker


class ReleaseProducerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="release-products-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.scratch = self.root / ".work" / "tmp"
        self.scratch.mkdir(parents=True)
        (self.root / ".git").mkdir()
        self.common = self.root / ".git"
        self.jemalloc = self.root / "libjemalloc.so.2"
        self.jemalloc.write_bytes(b"pinned preload fixture")

    def runtime(self, **changes):
        return FakeRuntime(self.root, **changes)

    def cargo_mounts(self, runtime, target):
        return [
            f"type=bind,source={runtime.ROOT},target=/work,readonly",
            f"type=bind,source={target},target=/cargo-target,rw",
        ]

    def run_with_fake_build(self, runtime, operation, source=SOURCE):
        with (
            patch.object(resources, "_cargo_mounts", self.cargo_mounts),
            patch.object(release, "_git_common_dir", return_value=self.common),
            patch.object(release, "JEMALLOC", self.jemalloc),
        ):
            return release.with_release_products(runtime, source, operation)

    def test_fresh_products_exist_only_inside_the_scoped_operation(self):
        runtime = self.runtime()
        self.assertIsNone(release.active_products(runtime))

        def operation():
            products, context = release.active_products(runtime)
            self.assertEqual(context.source, SOURCE.key)
            self.assertEqual(context.profile, "release")
            self.assertEqual(context.target, release.TARGET)
            self.assertEqual(context.image, runtime.IMAGE)
            self.assertEqual(context.allocator, "mimalloc")
            self.assertEqual(context.allocation, "1cpu/1GiB")
            self.assertEqual(tuple(sorted(path.name for path in products.iterdir())), tuple(sorted(release.PRODUCTS)))
            return "measured"

        self.assertEqual(self.run_with_fake_build(runtime, operation), "measured")
        self.assertIsNone(release.active_products(runtime))
        self.assertEqual([(call[4], call[5]) for call in runtime.calls], [(2, 8), (1, 1)])
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_build_script_uses_the_pinned_release_recipe_without_cflags_override(self):
        script = release._build_script(SOURCE)
        self.assertIn("cargo build --locked --offline --release --target x86_64-unknown-linux-musl", script)
        self.assertIn("CARGO_PROFILE_RELEASE_OPT_LEVEL=3", script)
        self.assertIn("CARGO_PROFILE_RELEASE_LTO=thin", script)
        self.assertIn("CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16", script)
        self.assertIn("CARGO_BUILD_JOBS=2", script)
        self.assertIn("CARGO_NET_OFFLINE=true", script)
        self.assertIn("CC=/usr/bin/clang-23", script)
        self.assertIn("AR=/usr/local/bin/llvm-ar", script)
        self.assertIn("LD_PRELOAD=/usr/lib/libjemalloc.so.2", script)
        self.assertNotIn("CFLAGS_x86_64_unknown_linux_musl=", script)

    def test_dirty_source_fails_before_build_dispatch(self):
        dirty = SourceIdentity(SOURCE.commit, SOURCE.tree, False)
        runtime = self.runtime(sources=(dirty,))
        with self.assertRaisesRegex(ValueError, "clean source identity"):
            self.run_with_fake_build(runtime, lambda: None, source=dirty)
        self.assertEqual(runtime.calls, [])

    def test_source_mutation_during_build_rejects_the_product(self):
        runtime = self.runtime(sources=(SOURCE, SOURCE, OTHER_SOURCE))
        with self.assertRaisesRegex(ValueError, "source changed"):
            self.run_with_fake_build(runtime, lambda: None)
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_missing_product_or_completion_marker_cannot_enter_scope(self):
        runtime = self.runtime(missing=("xsht",))
        with self.assertRaises(FileNotFoundError):
            self.run_with_fake_build(runtime, lambda: self.fail("unqualified products entered scope"))
        self.assertEqual(list(self.scratch.iterdir()), [])

        malformed = self.runtime(output=b"build completed\n")
        with self.assertRaisesRegex(ValueError, "completion marker"):
            self.run_with_fake_build(malformed, lambda: self.fail("unqualified products entered scope"))
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_container_cleanup_must_remove_all_target_files(self):
        runtime = self.runtime(leave_cleanup_files=True)
        with self.assertRaisesRegex(RuntimeError, "cleanup left files"):
            self.run_with_fake_build(runtime, lambda: None)
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_product_mutation_during_operation_is_rejected(self):
        runtime = self.runtime()

        def operation():
            products, _ = release.active_products(runtime)
            (products / "xsh").write_bytes(b"mutated")

        with self.assertRaisesRegex(ValueError, "changed during their scoped use"):
            self.run_with_fake_build(runtime, operation)
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_nested_close_measurement_and_timing_reuse_and_revalidate_products(self):
        runtime = self.runtime()
        signatures = []
        signature = resources._product_signature

        def observed_signature(directory):
            signatures.append(directory)
            return signature(directory)

        def timing():
            return release.active_products(runtime)

        def measure():
            return release.with_release_products(runtime, SOURCE, timing)

        def close():
            measured = release.with_release_products(runtime, SOURCE, measure)
            timed = release.with_release_products(runtime, SOURCE, timing)
            return measured, timed

        with patch.object(resources, "_product_signature", observed_signature):
            measured, timed = self.run_with_fake_build(runtime, close)

        self.assertEqual(measured, timed)
        self.assertEqual(measured[1].source, SOURCE.key)
        self.assertIsNone(release.active_products(runtime))
        self.assertEqual(len(runtime.calls), 2)
        self.assertGreaterEqual(len(signatures), 8)
        self.assertGreaterEqual(runtime.identity_calls, 12)
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_nested_scope_rejects_a_changed_source_before_running_callback(self):
        runtime = self.runtime()
        invoked = []

        def close():
            runtime._last_source = OTHER_SOURCE
            return release.with_release_products(runtime, SOURCE, lambda: invoked.append(True))

        with self.assertRaises(BaseExceptionGroup) as failure:
            self.run_with_fake_build(runtime, close)
        messages = []
        pending = [failure.exception]
        while pending:
            error = pending.pop()
            if isinstance(error, BaseExceptionGroup):
                messages.append(error.message)
                pending.extend(error.exceptions)
            else:
                messages.append(str(error))
        self.assertTrue(any("source changed" in message for message in messages))
        self.assertEqual(invoked, [])
        self.assertIsNone(release.active_products(runtime))
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_nested_scope_rejects_a_different_source_identity(self):
        runtime = self.runtime()
        invoked = []

        with self.assertRaisesRegex(ValueError, "changed source identity"):
            self.run_with_fake_build(
                runtime,
                lambda: release.with_release_products(runtime, OTHER_SOURCE, lambda: invoked.append(True)),
            )
        self.assertEqual(invoked, [])
        self.assertIsNone(release.active_products(runtime))
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_cleanup_failure_preserves_operation_failure(self):
        cleanup_error = RuntimeError("owned target cleanup failed")
        runtime = self.runtime(cleanup_error=cleanup_error)

        def operation():
            raise ValueError("acceptance operation failed")

        with self.assertRaises(BaseExceptionGroup) as failure:
            self.run_with_fake_build(runtime, operation)
        self.assertEqual(
            tuple(str(error) for error in failure.exception.exceptions),
            ("acceptance operation failed", "owned target cleanup failed"),
        )
        self.assertEqual(list(self.scratch.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
