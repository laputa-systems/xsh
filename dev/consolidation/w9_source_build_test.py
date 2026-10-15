import hashlib
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
CONTROLLER_ROOT = pathlib.Path(
    subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "--path-format=absolute", "--git-common-dir"],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()
).resolve().parent
BUILDER = ROOT / "dev/consolidation/w9-source-build.sh"
LINKER = pathlib.Path("/home/josh/d/laputa-systems/xsh/dev/musl-linker.sh")
LINKER_SHA256 = "583039d8ee08330c3eaab1970802181f1ed766f412bfe9d7493fb39135ebc727"
IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
BUILD_COMMAND = [
    "cargo", "test", "--locked", "--profile", "verification", "--target",
    "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration", "--no-run",
    "--message-format=json-render-diagnostics",
]


class SourceBuildRunnerTests(unittest.TestCase):
    def setUp(self):
        (ROOT / ".work/tmp").mkdir(parents=True, exist_ok=True)
        self.scratch = pathlib.Path(tempfile.mkdtemp(prefix="w9-source-build-", dir=ROOT / ".work/tmp"))
        self.source = self.scratch / "source"
        self.source.mkdir()
        (self.source / "tests").mkdir()
        self.tests_path = self.source / "tests/sema.rs"
        self.tests_path.write_bytes(b"#[test]\nfn original() {}\n")
        subprocess.run(["git", "init", "--quiet", str(self.source)], check=True)
        subprocess.run(["git", "-C", str(self.source), "config", "user.name", "Test"], check=True)
        subprocess.run(["git", "-C", str(self.source), "config", "user.email", "test@example.invalid"], check=True)
        (self.source / ".git/info/exclude").write_text("/.work/\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(self.source), "add", "tests/sema.rs"], check=True)
        subprocess.run(["git", "-C", str(self.source), "commit", "--quiet", "-m", "source"], check=True)
        self.commit = subprocess.run(
            ["git", "-C", str(self.source), "rev-parse", "HEAD"],
            check=True, text=True, capture_output=True,
        ).stdout.strip()
        self.tree = subprocess.run(
            ["git", "-C", str(self.source), "rev-parse", "HEAD^{tree}"],
            check=True, text=True, capture_output=True,
        ).stdout.strip()
        self.overlay = b"#[test]\nfn overlay() {}\n"
        self.tests_path.write_bytes(self.overlay)
        self.overlay_sha256 = hashlib.sha256(self.overlay).hexdigest()
        self.environment = {
            **os.environ,
            "W9_SOURCE_COMMIT": self.commit,
            "W9_SOURCE_TREE": self.tree,
            "W9_LINKER_PATH": str(LINKER),
            "W9_LINKER_SHA256": LINKER_SHA256,
        }

    def tearDown(self):
        shutil.rmtree(self.scratch)

    def run_builder(self, mode, args, extra_env=None):
        environment = {**self.environment, **(extra_env or {})}
        return subprocess.run(
            [str(BUILDER), str(self.source), f"plan-{mode}", *args],
            check=False,
            env=environment,
            capture_output=True,
        )

    def planned_argv(self, result):
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        return tuple(argument.decode() for argument in result.stdout.split(b"\0") if argument)

    def test_build_argv_is_pinned_and_reuses_only_same_source_overlay_role(self):
        environment = {"W9_ROLE": "candidate", "W9_OVERLAY_SHA256": self.overlay_sha256}
        first = self.planned_argv(self.run_builder("build", BUILD_COMMAND, environment))
        repeated = self.planned_argv(self.run_builder("build", BUILD_COMMAND, environment))
        self.assertEqual(repeated, first)
        docker_index = first.index("/usr/bin/docker")
        first = first[docker_index + 1:]
        image_index = first.index(IMAGE)
        self.assertEqual(first[:3], ("--host", "unix:///var/run/docker.sock", "run"))
        self.assertLess(first.index("--cpus=12"), image_index)
        self.assertLess(first.index("--memory=24g"), image_index)
        self.assertLess(first.index("--memory-swap=24g"), image_index)
        self.assertLess(first.index("--network=none"), image_index)
        self.assertIn("CARGO_BUILD_WARNINGS=deny", first)
        target_dir = next(value.split("=", 1)[1] for value in first if value.startswith("CARGO_TARGET_DIR="))
        self.assertIn(f"{self.commit}-{self.tree}-{self.overlay_sha256}-candidate", target_dir)
        other_role = self.planned_argv(
            self.run_builder("build", BUILD_COMMAND, {"W9_ROLE": "reference", "W9_OVERLAY_SHA256": self.overlay_sha256})
        )
        other_target = next(value.split("=", 1)[1] for value in other_role if value.startswith("CARGO_TARGET_DIR="))
        self.assertNotEqual(other_target, target_dir)

    def test_run_argv_pins_input_and_preserves_host_identity(self):
        corpus_scratch = CONTROLLER_ROOT / ".work/tmp" / f"w9-corpus-recovery-{self.scratch.name}"
        input_root = corpus_scratch / "input-corpus"
        input_root.mkdir(parents=True)
        self.addCleanup(shutil.rmtree, corpus_scratch)
        self.tests_path.write_bytes(b"#[test]\nfn original() {}\n")
        run_env = {"W9_SOURCE_COMMIT": self.commit, "W9_SOURCE_TREE": self.tree}
        result = self.run_builder(
            "run",
            [
                "/bin/true",
                f"W9_INPUT_ROOT={input_root}",
                "W9_CPU_SET=3",
                "W9_CONTAINER_NAME=w9adapter-123-reference-A-0",
            ],
            run_env,
        )
        run = self.planned_argv(result)
        docker_index = run.index("/usr/bin/docker")
        run = run[docker_index + 1:]
        image_index = run.index(IMAGE)
        self.assertLess(run.index("--memory=1g"), image_index)
        self.assertLess(run.index("--memory-swap=1g"), image_index)
        self.assertLess(run.index("--network=none"), image_index)
        self.assertIn("--cpuset-cpus=3", run)
        name_index = run.index("--name")
        self.assertEqual(run[name_index + 1], "w9adapter-123-reference-A-0")
        user_index = run.index("--user")
        self.assertEqual(run[user_index + 1], f"{os.getuid()}:{os.getgid()}")
        self.assertIn(f"W9_INPUT_ROOT={input_root}", run)
        self.assertEqual(run[-1], "/bin/true")
        mounts = [run[index + 1] for index, value in enumerate(run[:-1]) if value == "-v"]
        input_mount = f"{input_root}:{input_root}:ro"
        scratch_mount = f"{CONTROLLER_ROOT}/.work/tmp/w9-container-scratch:/scratch"
        self.assertIn(f"{CONTROLLER_ROOT}:{CONTROLLER_ROOT}", mounts)
        self.assertIn(input_mount, mounts)
        self.assertIn(scratch_mount, mounts)
        self.assertNotIn(f"{CONTROLLER_ROOT}/.work/tmp:{CONTROLLER_ROOT}/.work/tmp", mounts)
        self.assertNotIn(f"{CONTROLLER_ROOT}/.work/tmp:/scratch", mounts)
        self.assertFalse(input_root == CONTROLLER_ROOT / ".work/tmp/w9-container-scratch")
        self.assertNotIn(CONTROLLER_ROOT / ".work/tmp/w9-container-scratch", input_root.parents)

        overlapping = self.run_builder(
            "run",
            ["/bin/true", f"W9_INPUT_ROOT={CONTROLLER_ROOT}/.work/tmp"],
            run_env,
        )
        self.assertNotEqual(overlapping.returncode, 0)
        self.assertIn(b"W9 input root overlaps container scratch", overlapping.stderr)

        malformed_name = self.run_builder(
            "run",
            ["/bin/true", "W9_CONTAINER_NAME=not-a-campaign-container"],
            run_env,
        )
        self.assertNotEqual(malformed_name.returncode, 0)
        self.assertIn(b"W9 container name is malformed", malformed_name.stderr)

        for name in (
            "w9adapter-123-hash-reference",
            "w9adapter-123-hash-candidate",
            "w9adapter-123-hash-derived-reference",
            "w9exact-123-reference-A-0",
            "w9exact-123-candidate-D-1650",
            "w9exact-123-reference-hash-1",
            "w9exact-123-candidate-hash-2",
        ):
            with self.subTest(name=name):
                named = self.run_builder(
                    "run",
                    ["/bin/true", f"W9_CONTAINER_NAME={name}"],
                    run_env,
                )
                planned = self.planned_argv(named)
                self.assertEqual(planned[planned.index("--name") + 1], name)


if __name__ == "__main__":
    unittest.main()
