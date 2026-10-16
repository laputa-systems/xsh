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
PROGRAM_ROOT = CONTROLLER_ROOT / ".work/wt/w9-program-e240"
TASK_TMP = CONTROLLER_ROOT / ".work/tmp"
LINKER_SHA256 = "583039d8ee08330c3eaab1970802181f1ed766f412bfe9d7493fb39135ebc727"
IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
BUILD_COMMAND = [
    "cargo", "test", "--locked", "--profile", "verification", "--target",
    "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration", "--no-run",
    "--message-format=json-render-diagnostics",
]


class SourceBuildRunnerTests(unittest.TestCase):
    def setUp(self):
        (ROOT / ".work/wt").mkdir(parents=True, exist_ok=True)
        self.scratch = pathlib.Path(tempfile.mkdtemp(prefix="w9-source-build-", dir=ROOT / ".work/wt"))
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
        subprocess.run(
            ["git", "-C", str(self.source), "commit", "--quiet", "-m", "test(fixtures): initialize source"],
            check=True,
        )
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
        self.generated_helpers = self.source / ".work/consolidation"
        self.generated_helpers.mkdir(parents=True)
        self.adapter_helper = self.generated_helpers / "w9-publication-adapter.rs"
        self.owner_helper = self.generated_helpers / "w9-publication-owner-helper.rs"
        self.adapter_helper.write_bytes(b"fn adapter_helper() {}\n")
        self.owner_helper.write_bytes(b"fn owner_helper() {}\n")
        self.environment = {
            **os.environ,
            "W9_SOURCE_COMMIT": self.commit,
            "W9_SOURCE_TREE": self.tree,
            "W9_LINKER_PATH": str(LINKER),
            "W9_LINKER_SHA256": LINKER_SHA256,
        }

    def tearDown(self):
        shutil.rmtree(self.scratch)

    def run_builder(self, mode, args, extra_env=None, source=None, builder=None):
        environment = {**self.environment, **(extra_env or {})}
        return subprocess.run(
            [str(builder or BUILDER), str(source or self.source), f"plan-{mode}", *args],
            check=False,
            env=environment,
            capture_output=True,
        )

    def path_overlaps(self, left, right):
        left = pathlib.Path(left)
        right = pathlib.Path(right)
        return left == right or left in right.parents or right in left.parents

    def controller_identity(self):
        values = subprocess.run(
            ["git", "-C", str(CONTROLLER_ROOT), "rev-parse", "HEAD", "HEAD^{tree}"],
            check=True,
            text=True,
            capture_output=True,
        ).stdout.splitlines()
        return {"W9_SOURCE_COMMIT": values[0], "W9_SOURCE_TREE": values[1]}

    def assert_writable_mounts_disjoint(self, mounts, protected_paths):
        for mount in mounts:
            source, destination, *options = mount.split(":")
            mode = options[0] if options else "rw"
            if mode == "ro":
                continue
            destination = pathlib.Path(destination)
            for protected in protected_paths:
                self.assertFalse(self.path_overlaps(destination, protected), mount)
            source = pathlib.Path(source)
            if source.is_absolute():
                for protected in protected_paths:
                    self.assertFalse(self.path_overlaps(source, protected), mount)

    def planned_argv(self, result):
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        return tuple(argument.decode() for argument in result.stdout.split(b"\0") if argument)

    def build_input_sha256(self):
        digest_input = b"xsh-w9-build-inputs-v1\0"
        for label, path in (
            (b"overlay", self.tests_path),
            (b"adapter", self.adapter_helper),
            (b"owner", self.owner_helper),
        ):
            digest_input += label + b"\0"
            digest_input += hashlib.sha256(path.read_bytes()).hexdigest().encode("ascii") + b"\0"
        return hashlib.sha256(digest_input).hexdigest()

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
        mounts = [first[index + 1] for index, value in enumerate(first[:-1]) if value == "-v"]
        controller_mount = f"{CONTROLLER_ROOT}:{CONTROLLER_ROOT}:ro"
        target_mount = f"{TASK_TMP}:{TASK_TMP}:rw"
        scratch_mount = f"{TASK_TMP}:/scratch:rw"
        self.assertIn(controller_mount, mounts)
        self.assertIn(target_mount, mounts)
        self.assertIn(scratch_mount, mounts)
        self.assertIn(CONTROLLER_ROOT, self.tests_path.parents)
        self.assertEqual(
            first[-(len(BUILD_COMMAND) + 5) :],
            (IMAGE, "/usr/bin/env", "-u", "XSH_MODULE_PATH", "LD_PRELOAD=/usr/lib/libjemalloc.so.2", *BUILD_COMMAND),
        )
        target_dir = next(value.split("=", 1)[1] for value in first if value.startswith("CARGO_TARGET_DIR="))
        build_input_sha256 = self.build_input_sha256()
        self.assertIn(f"{self.commit}-{self.tree}-{build_input_sha256}-candidate", target_dir)
        self.assertLess(len(pathlib.Path(target_dir).name), 255)
        self.assertEqual(pathlib.Path(target_dir).parent, TASK_TMP)
        self.assertEqual(self.tests_path.read_bytes(), self.overlay)
        self.assert_writable_mounts_disjoint(mounts, [self.source])
        other_role = self.planned_argv(
            self.run_builder("build", BUILD_COMMAND, {"W9_ROLE": "reference", "W9_OVERLAY_SHA256": self.overlay_sha256})
        )
        other_target = next(value.split("=", 1)[1] for value in other_role if value.startswith("CARGO_TARGET_DIR="))
        self.assertNotEqual(other_target, target_dir)

    def test_build_target_changes_when_generated_helper_bytes_change(self):
        environment = {"W9_ROLE": "candidate", "W9_OVERLAY_SHA256": self.overlay_sha256}

        def target_dir():
            planned = self.planned_argv(self.run_builder("build", BUILD_COMMAND, environment))
            return next(value.split("=", 1)[1] for value in planned if value.startswith("CARGO_TARGET_DIR="))

        original_adapter = self.adapter_helper.read_bytes()
        original_owner = self.owner_helper.read_bytes()
        baseline = target_dir()
        self.assertEqual(target_dir(), baseline)
        self.adapter_helper.write_bytes(b"fn changed_adapter_helper() {}\n")
        self.assertNotEqual(target_dir(), baseline)
        self.adapter_helper.write_bytes(original_adapter)
        self.owner_helper.write_bytes(b"fn changed_owner_helper() {}\n")
        self.assertNotEqual(target_dir(), baseline)
        self.owner_helper.write_bytes(original_owner)
        self.assertEqual(target_dir(), baseline)

    def test_build_rejects_missing_or_symlinked_generated_helpers(self):
        environment = {"W9_ROLE": "candidate", "W9_OVERLAY_SHA256": self.overlay_sha256}
        target = self.scratch / "helper-target.rs"
        target.write_bytes(b"fn helper_target() {}\n")
        for helper in (self.adapter_helper, self.owner_helper):
            with self.subTest(helper=helper.name, state="missing"):
                content = helper.read_bytes()
                helper.unlink()
                missing = self.run_builder("build", BUILD_COMMAND, environment)
                self.assertNotEqual(missing.returncode, 0)
                self.assertIn(b"generated adapter build input is missing", missing.stderr)
                helper.write_bytes(content)
            with self.subTest(helper=helper.name, state="symlink"):
                content = helper.read_bytes()
                helper.unlink()
                helper.symlink_to(target)
                symlinked = self.run_builder("build", BUILD_COMMAND, environment)
                self.assertNotEqual(symlinked.returncode, 0)
                self.assertIn(b"generated adapter build input is missing", symlinked.stderr)
                helper.unlink()
                helper.write_bytes(content)

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
                f"W9_PROGRAM_ROOT={PROGRAM_ROOT}",
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
        self.assertIn(f"W9_PROGRAM_ROOT={PROGRAM_ROOT}", run)
        self.assertEqual(run[-1], "/bin/true")
        mounts = [run[index + 1] for index, value in enumerate(run[:-1]) if value == "-v"]
        input_mount = f"{input_root}:{input_root}:ro"
        program_mount = f"{PROGRAM_ROOT}:{PROGRAM_ROOT}:ro"
        controller_mount = f"{CONTROLLER_ROOT}:{CONTROLLER_ROOT}:ro"
        scratch_root = TASK_TMP / "w9-container-scratch"
        scratch_mount = f"{scratch_root}:/scratch:rw"
        self.assertIn(controller_mount, mounts)
        self.assertIn(program_mount, mounts)
        self.assertIn(input_mount, mounts)
        self.assertIn(scratch_mount, mounts)
        self.assertNotIn(f"{TASK_TMP}:{TASK_TMP}:rw", mounts)
        self.assert_writable_mounts_disjoint(mounts, [self.source, PROGRAM_ROOT, input_root])
        self.assertEqual(run[-5:-2], (IMAGE, "/usr/bin/env", "-u"))
        self.assertEqual(run[-2], "XSH_MODULE_PATH")
        self.assertFalse(input_root == CONTROLLER_ROOT / ".work/tmp/w9-container-scratch")
        self.assertNotIn(CONTROLLER_ROOT / ".work/tmp/w9-container-scratch", input_root.parents)

        overlapping = self.run_builder(
            "run",
            ["/bin/true", f"W9_INPUT_ROOT={CONTROLLER_ROOT}/.work/tmp", f"W9_PROGRAM_ROOT={PROGRAM_ROOT}"],
            run_env,
        )
        self.assertNotEqual(overlapping.returncode, 0)
        self.assertIn(b"W9 input root overlaps container scratch", overlapping.stderr)

        shared_program_input = self.run_builder(
            "run",
            ["/bin/true", f"W9_INPUT_ROOT={PROGRAM_ROOT}", f"W9_PROGRAM_ROOT={PROGRAM_ROOT}"],
            run_env,
        )
        self.assertNotEqual(shared_program_input.returncode, 0)
        self.assertIn(b"W9 program root overlaps input root", shared_program_input.stderr)

        for root in (self.source, self.scratch, self.source / ".git"):
            with self.subTest(program_root=root):
                overlapping_program = self.run_builder(
                    "run",
                    ["/bin/true", f"W9_PROGRAM_ROOT={root}"],
                    run_env,
                )
                self.assertNotEqual(overlapping_program.returncode, 0)
                self.assertIn(b"W9 program root overlaps compiler source root", overlapping_program.stderr)

        malformed_name = self.run_builder(
            "run",
            ["/bin/true", f"W9_PROGRAM_ROOT={PROGRAM_ROOT}", "W9_CONTAINER_NAME=not-a-campaign-container"],
            run_env,
        )
        self.assertNotEqual(malformed_name.returncode, 0)
        self.assertIn(b"W9 container name is malformed", malformed_name.stderr)

        missing_program_root = self.run_builder("run", ["/bin/true"], run_env)
        self.assertNotEqual(missing_program_root.returncode, 0)
        self.assertIn(b"W9 program root is required", missing_program_root.stderr)

        overlapping_source = self.run_builder(
            "run",
            ["/bin/true", f"W9_PROGRAM_ROOT={PROGRAM_ROOT}"],
            self.controller_identity(),
            source=CONTROLLER_ROOT,
        )
        self.assertNotEqual(overlapping_source.returncode, 0)
        self.assertIn(b"compiler source root overlaps writable container scratch", overlapping_source.stderr)

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
                    ["/bin/true", f"W9_PROGRAM_ROOT={PROGRAM_ROOT}", f"W9_CONTAINER_NAME={name}"],
                    run_env,
                )
                planned = self.planned_argv(named)
                self.assertEqual(planned[planned.index("--name") + 1], name)

    def test_capture_outputs_mount_only_the_declared_output_root(self):
        input_root = CONTROLLER_ROOT / ".work/tmp" / f"w9-corpus-recovery-{self.scratch.name}"
        input_root.mkdir(parents=True)
        self.addCleanup(shutil.rmtree, input_root)
        output_root = pathlib.Path(tempfile.mkdtemp(prefix="w9-capture-output-", dir=TASK_TMP))
        self.addCleanup(shutil.rmtree, output_root)
        self.tests_path.write_bytes(b"#[test]\nfn original() {}\n")
        run_env = {"W9_SOURCE_COMMIT": self.commit, "W9_SOURCE_TREE": self.tree}
        capture_commands = (
            (
                "adapter",
                [
                    "env",
                    f"W9_RECEIPT_OUTPUT={output_root}/receipts/candidate/0-100.tsv",
                    f"W9_TIMING_OUTPUT={output_root}/jobs/candidate/A/0-100/timing.txt",
                    f"W9_ADAPTER_OUTPUT_DIR={output_root}/raw/candidate",
                    "/bin/true",
                ],
            ),
            (
                "exact-owner",
                [
                    "env",
                    f"W9_EXACT_OWNER_OUTPUT_DIR={output_root}/exact/raw/candidate",
                    f"W9_EXACT_OWNER_RECEIPT={output_root}/exact/receipts/candidate/0-100.tsv",
                    "/bin/true",
                ],
            ),
        )
        for _, command in capture_commands:
            for assignment in command:
                if assignment.startswith((
                    "W9_RECEIPT_OUTPUT=",
                    "W9_TIMING_OUTPUT=",
                    "W9_ADAPTER_OUTPUT_DIR=",
                    "W9_EXACT_OWNER_OUTPUT_DIR=",
                    "W9_EXACT_OWNER_RECEIPT=",
                )):
                    output_path = pathlib.Path(assignment.split("=", 1)[1])
                    if assignment.startswith(("W9_ADAPTER_OUTPUT_DIR=", "W9_EXACT_OWNER_OUTPUT_DIR=")):
                        output_path.mkdir(parents=True)
                    else:
                        output_path.parent.mkdir(parents=True, exist_ok=True)
        for role, command in capture_commands:
            with self.subTest(role=role):
                planned = self.planned_argv(
                    self.run_builder(
                        "run",
                        [
                            f"W9_INPUT_ROOT={input_root}",
                            f"W9_PROGRAM_ROOT={PROGRAM_ROOT}",
                            f"W9_OUTPUT_ROOT={output_root}",
                            *command,
                        ],
                        run_env,
                    )
                )
                docker_index = planned.index("/usr/bin/docker")
                run = planned[docker_index + 1:]
                mounts = [run[index + 1] for index, value in enumerate(run[:-1]) if value == "-v"]
                self.assertIn(f"{output_root}:{output_root}:rw", mounts)
                self.assertIn(f"{CONTROLLER_ROOT}:{CONTROLLER_ROOT}:ro", mounts)
                self.assertIn(f"{PROGRAM_ROOT}:{PROGRAM_ROOT}:ro", mounts)
                self.assertIn(f"{input_root}:{input_root}:ro", mounts)
                self.assert_writable_mounts_disjoint(mounts, [self.source, PROGRAM_ROOT, input_root])

        missing_output_root = self.run_builder(
            "run",
            [
                f"W9_PROGRAM_ROOT={PROGRAM_ROOT}",
                "env",
                f"W9_RECEIPT_OUTPUT={output_root}/receipts/candidate/0-100.tsv",
                "/bin/true",
            ],
            run_env,
        )
        self.assertNotEqual(missing_output_root.returncode, 0)
        self.assertIn(b"W9 output root is required for capture output paths", missing_output_root.stderr)

        outside_output = self.run_builder(
            "run",
            [
                f"W9_PROGRAM_ROOT={PROGRAM_ROOT}",
                f"W9_OUTPUT_ROOT={output_root}",
                "env",
                f"W9_RECEIPT_OUTPUT={self.source}/receipt.tsv",
                "/bin/true",
            ],
            run_env,
        )
        self.assertNotEqual(outside_output.returncode, 0)
        self.assertIn(b"W9 capture output path is outside W9 output root", outside_output.stderr)

        dot_leaf_output = self.run_builder(
            "run",
            [
                f"W9_PROGRAM_ROOT={PROGRAM_ROOT}",
                f"W9_OUTPUT_ROOT={output_root}",
                "env",
                f"W9_ADAPTER_OUTPUT_DIR={output_root}/..",
                "/bin/true",
            ],
            run_env,
        )
        self.assertNotEqual(dot_leaf_output.returncode, 0)
        self.assertIn(b"W9 capture output path is not a canonical leaf", dot_leaf_output.stderr)

        overlapping_output = self.run_builder(
            "run",
            [
                f"W9_PROGRAM_ROOT={PROGRAM_ROOT}",
                f"W9_OUTPUT_ROOT={self.source}",
                "env",
                f"W9_RECEIPT_OUTPUT={self.source}/receipt.tsv",
                "/bin/true",
            ],
            run_env,
        )
        self.assertNotEqual(overlapping_output.returncode, 0)
        self.assertIn(b"W9 output root overlaps a protected source or scratch root", overlapping_output.stderr)

        nested_builder_root = self.scratch / "nested-builder"
        builder_commit = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"],
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        subprocess.run(
            [
                "git",
                "-C",
                str(ROOT),
                "worktree",
                "add",
                "--quiet",
                "--detach",
                str(nested_builder_root),
                builder_commit,
            ],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            nested_output_root = nested_builder_root / ".work/capture-output"
            nested_output_root.mkdir(parents=True)
            nested_source_output = self.run_builder(
                "run",
                [
                    f"W9_PROGRAM_ROOT={PROGRAM_ROOT}",
                    f"W9_OUTPUT_ROOT={nested_output_root}",
                    "env",
                    f"W9_RECEIPT_OUTPUT={nested_output_root}/receipt.tsv",
                    "/bin/true",
                ],
                run_env,
                builder=nested_builder_root / "dev/consolidation/w9-source-build.sh",
            )
            self.assertNotEqual(nested_source_output.returncode, 0)
            self.assertIn(
                b"W9 output root overlaps a protected source or scratch root",
                nested_source_output.stderr,
            )
        finally:
            subprocess.run(
                ["git", "-C", str(ROOT), "worktree", "remove", "--force", str(nested_builder_root)],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

        for protected_root in (PROGRAM_ROOT, input_root):
            with self.subTest(protected_output_root=protected_root):
                overlapping_protected_root = self.run_builder(
                    "run",
                    [
                        f"W9_INPUT_ROOT={input_root}",
                        f"W9_PROGRAM_ROOT={PROGRAM_ROOT}",
                        f"W9_OUTPUT_ROOT={protected_root}",
                        "env",
                        f"W9_RECEIPT_OUTPUT={protected_root}/receipt.tsv",
                        "/bin/true",
                    ],
                    run_env,
                )
                self.assertNotEqual(overlapping_protected_root.returncode, 0)
                self.assertIn(
                    b"W9 output root overlaps a protected source or input root",
                    overlapping_protected_root.stderr,
                )

    def test_build_rejects_source_overlapping_writable_target_mount(self):
        overlapping_source = self.run_builder(
            "build",
            BUILD_COMMAND,
            {
                **self.controller_identity(),
                "W9_ROLE": "candidate",
                "W9_OVERLAY_SHA256": self.overlay_sha256,
            },
            source=CONTROLLER_ROOT,
        )
        self.assertNotEqual(overlapping_source.returncode, 0)
        self.assertIn(b"compiler source root overlaps writable build output", overlapping_source.stderr)


if __name__ == "__main__":
    unittest.main()
