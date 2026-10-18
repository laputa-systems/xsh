#!/usr/bin/env python3
"""Adapt completed final release evidence to the scheduler comparator receipt contract."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

ROOT: Path
CONTROLLER_ROOT: Path
COMMIT: str
TREE: str
TARGET = "x86_64-unknown-linux-musl"
IMAGE_ID = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
IMAGE = "xsh-test@" + IMAGE_ID
FUNCTIONAL_CLOSE_PATH: Path
PROFILE_RECEIPT_PATH: Path | None
REFERENCE_ROOT: Path
BUILD_ROOT: Path
REFERENCE_RUN: Path
RUNNER: Path
COMPARE_PATH: Path
PATCH_PATH: Path
BINARY_NAMES = ("xsht", "xsh", "xsh-test-helper")


def load_comparator():
    if str(COMPARE_PATH.parent) not in sys.path:
        sys.path.insert(0, str(COMPARE_PATH.parent))
    spec = importlib.util.spec_from_file_location("scheduler_compare", COMPARE_PATH)
    if spec is None or spec.loader is None:
        raise ValueError(f"cannot import scheduler comparator: {COMPARE_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def configure(args) -> None:
    global ROOT, CONTROLLER_ROOT, COMMIT, TREE, FUNCTIONAL_CLOSE_PATH, PROFILE_RECEIPT_PATH
    global REFERENCE_ROOT, BUILD_ROOT, REFERENCE_RUN, RUNNER, COMPARE_PATH, PATCH_PATH

    require(re.fullmatch(r"[0-9a-f]{40}", args.commit) is not None,
            "source commit must be a full lowercase Git ID")
    require(re.fullmatch(r"[0-9a-f]{40}", args.tree_id) is not None,
            "source tree must be a full lowercase Git tree ID")
    require(re.fullmatch(r"[a-z0-9][a-z0-9-]{0,47}", args.run_id) is not None,
            "run ID must use 1 to 48 lowercase letters, digits, or hyphens")

    source_root_absolute = Path(os.path.abspath(args.source_root))
    ROOT = args.source_root.resolve(strict=True)
    require(source_root_absolute == ROOT, "source root path contains a symlink or noncanonical component")
    COMMIT = args.commit
    TREE = args.tree_id
    require(git(ROOT, "rev-parse", "--show-toplevel") == str(ROOT),
            f"source root is not the canonical Git root: {ROOT}")
    require(git(ROOT, "rev-parse", "HEAD") == COMMIT,
            f"source root HEAD differs from requested commit {COMMIT}")
    require(git(ROOT, "rev-parse", "HEAD^{tree}") == TREE,
            f"source root tree differs from requested tree {TREE}")
    require(not git(ROOT, "status", "--porcelain=v1", "--untracked-files=all"),
            "candidate source root must be clean before final gate launch")
    common_dir = canonical_input_path(Path(git(ROOT, "rev-parse", "--path-format=absolute", "--git-common-dir")), "common Git directory")
    CONTROLLER_ROOT = common_dir.parent
    require(common_dir.name == ".git" and git(CONTROLLER_ROOT, "rev-parse", "--show-toplevel") == str(CONTROLLER_ROOT),
            "common Git directory must belong to the controller checkout")
    tool_root = Path(__file__).resolve().parents[2]
    require(Path(git(tool_root, "rev-parse", "--path-format=absolute", "--git-common-dir")).resolve(strict=True) == common_dir,
            "scheduler bridge tools must share the controller Git directory")

    REFERENCE_ROOT = canonical_input_path(args.reference_source_root, "reference source root")
    profile_receipt = getattr(args, "profile_receipt", None)
    PROFILE_RECEIPT_PATH = canonical_input_path(profile_receipt, "profile-cost receipt") if profile_receipt else None
    BUILD_ROOT = ROOT / f".work/consolidation/native-scheduler-builds/{COMMIT}/{args.run_id}"
    REFERENCE_RUN = CONTROLLER_ROOT / ".work/final-review-evidence/final-correctness-gates" / f"native-scheduler-reference-{COMMIT}-{args.run_id}"
    RUNNER = tool_root / "dev/consolidation/winddown/gate-tools/final-correctness-gates/run-native-final.sh"
    COMPARE_PATH = Path(__file__).resolve().parent / "winddown/gate-tools/native-scheduler-comparison/compare.py"
    PATCH_PATH = ROOT / "dev/consolidation/winddown/unfinished/475-native-scheduler-reference-2083821edb768d2485479c7386289b82e43b1c7c.patch"

    try:
        REFERENCE_ROOT.relative_to(ROOT)
    except ValueError as error:
        raise ValueError(f"reference source root must remain under the source root: {REFERENCE_ROOT}") from error
    try:
        REFERENCE_ROOT.relative_to(ROOT / ".work/wt")
    except ValueError as error:
        raise ValueError(f"reference source root must be under the repository worktree directory: {REFERENCE_ROOT}") from error
    require_frozen_root(REFERENCE_ROOT)
    FUNCTIONAL_CLOSE_PATH = canonical_input_path(args.functional_close, "functional-close record")
    try:
        FUNCTIONAL_CLOSE_PATH.relative_to(ROOT)
    except ValueError as error:
        raise ValueError(f"functional-close record must remain under the source root: {FUNCTIONAL_CLOSE_PATH}") from error
    if PROFILE_RECEIPT_PATH is not None:
        try:
            PROFILE_RECEIPT_PATH.relative_to(ROOT)
        except ValueError as error:
            raise ValueError(f"profile-cost receipt must remain under the source root: {PROFILE_RECEIPT_PATH}") from error


def canonical_input_path(path: Path, label: str) -> Path:
    absolute = Path(os.path.abspath(path))
    resolved = path.resolve(strict=True)
    require(absolute == resolved, f"{label} path contains a symlink or noncanonical component: {path}")
    return resolved


def require_no_symlink(path: Path, root: Path | None = None) -> None:
    absolute = Path(os.path.abspath(path))
    root = ROOT if root is None else root
    relative = absolute.relative_to(root)
    cursor = root
    for part in relative.parts:
        cursor = cursor / part
        try:
            if cursor.is_symlink():
                raise ValueError(f"scheduler evidence path contains a symlink: {cursor}")
        except OSError as error:
            raise ValueError(f"cannot inspect scheduler evidence path: {cursor}") from error


def require_frozen_root(root: Path) -> None:
    require(root.resolve(strict=True) == root, f"source root path is not canonical: {root}")
    require(git(root, "rev-parse", "--show-toplevel") == str(root), f"source is not a Git root: {root}")
    require(git(root, "rev-parse", "HEAD") == COMMIT, f"source HEAD is not pinned to {COMMIT}: {root}")
    require(git(root, "rev-parse", "HEAD^{tree}") == TREE, f"source tree is not pinned to {TREE}: {root}")


def require_successful_stage(stage: dict, label: str) -> None:
    require(isinstance(stage, dict), f"profile receipt is missing its {label} stage")
    require(stage.get("status") == 0, f"profile {label} stage did not exit successfully")
    require(stage.get("timing_accepted") is True and stage.get("timing_discarded") is False,
            f"profile {label} stage did not pass readiness and cleanup validation")
    cleanup = stage.get("container_cleanup")
    require(isinstance(cleanup, dict) and cleanup.get("resolved") is True,
            f"profile {label} container cleanup is unresolved")
    gate = stage.get("performance_gate")
    require(isinstance(gate, dict), f"profile {label} stage has no performance readiness record")
    for boundary in ("before", "after"):
        check = gate.get(boundary)
        require(isinstance(check, dict) and check.get("exit_code") == 0,
                f"profile {label} {boundary} readiness check did not exit zero")


def validate_profile_inputs(compare):
    require(PROFILE_RECEIPT_PATH is not None, "a completed profile-cost receipt is required")
    require_frozen_root(ROOT)
    source = compare.capture_source(ROOT, COMMIT)
    require_no_symlink(PROFILE_RECEIPT_PATH)
    receipt_path = PROFILE_RECEIPT_PATH.resolve(strict=True)
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    require(receipt.get("schema") == 1 and receipt.get("state") == "complete",
            "profile-cost receipt is not a completed schema-1 receipt")
    require(receipt.get("source_commit") == COMMIT and receipt.get("source_tree") == str(ROOT),
            "profile-cost receipt does not name the frozen primary source")
    expected_identity = {
        "commit": COMMIT,
        "git_tree": TREE,
        "status": "clean",
    }
    require(receipt.get("source_identity") == expected_identity and receipt.get("source_status") == "clean",
            "profile-cost source identity differs from the frozen clean source")
    require(receipt.get("target") == TARGET, "profile-cost receipt has a different Rust target")
    profile = receipt.get("profile")
    require(profile == {"name": "release", "lto": "thin", "codegen_units": 16, "opt_levels": [3]},
            "profile-cost receipt is not the selected release opt3 profile")
    image = receipt.get("native_image")
    require(isinstance(image, dict) and image.get("image_id") == IMAGE_ID
            and image.get("architecture") == "amd64",
            "profile-cost receipt does not identify the pinned amd64 image")

    close_path = FUNCTIONAL_CLOSE_PATH
    require_no_symlink(close_path)
    close = json.loads(close_path.read_text(encoding="utf-8"))
    close_record = receipt.get("functional_close")
    require(isinstance(close_record, dict)
            and close_record.get("path") == str(close_path)
            and close_record.get("sha256") == sha256(close_path)
            and close_record.get("record") == close,
            "profile-cost receipt does not bind the current functional-close record")
    require(close.get("functional_complete") is True and close.get("commit") == COMMIT,
            "functional-close record does not name the frozen source as complete")

    resources = receipt.get("resources")
    require(isinstance(resources, dict) and resources.get("cpu_limit") == 12
            and resources.get("memory_limit") == "24g"
            and resources.get("memory_swap_limit") == "24g",
            "profile-cost receipt does not record the selected build resource limits")
    measured_resources = None
    results = receipt.get("results")
    require(isinstance(results, list) and len(results) == 1
            and isinstance(results[0], dict) and results[0].get("name") == "release-opt3",
            "profile-cost receipt must contain exactly one release-opt3 result")
    result = results[0]
    require(result.get("profile") == "release" and result.get("opt_level") == 3
            and result.get("lto") == "thin" and result.get("codegen_units") == 16,
            "profile-cost result does not use release opt3, thin LTO, and 16 codegen units")
    build = result.get("build")
    test = result.get("native_suite")
    require_successful_stage(build, "build")
    require_successful_stage(test, "native suite")
    measured_resources = result.get("container_resources")
    cpus = [int(cpu) for cpu in resources.get("cpuset_cpus", "").split(",") if cpu]
    require(isinstance(measured_resources, dict)
            and measured_resources.get("affinity_cpus") == cpus
            and measured_resources.get("memory_limit_bytes") == 24 * 1024**3
            and measured_resources.get("memory_swap_max_bytes") == 0,
            "profile-cost container resource probe differs from the selected limits")
    require(test.get("status") == 0, "release opt3 native suite did not pass")

    profile_dir = receipt_path.parent / "opt3" / "target" / TARGET / "release"
    recorded = result.get("binaries")
    require(isinstance(recorded, dict) and set(recorded) == set(BINARY_NAMES),
            "profile-cost receipt does not bind exactly xsht, xsh, and xsh-test-helper")
    for stage_name, stage in (("build", build), ("native suite", test)):
        validation = stage.get("stage_validation")
        require(isinstance(validation, dict) and validation.get("source") == expected_identity,
                f"profile {stage_name} source revalidation differs from the frozen tree")
        require(validation.get("binary_hashes") == recorded,
                f"profile {stage_name} binary revalidation differs from the completed build")

    binaries = {}
    for name in BINARY_NAMES:
        path = profile_dir / name
        record = recorded[name]
        require(path.is_file() and os.access(path, os.X_OK), f"release binary is missing or not executable: {path}")
        require(record.get("path") == str(path) and record.get("sha256") == sha256(path),
                f"release binary differs from profile-cost evidence: {name}")
        binaries[name] = {"path": str(path), "sha256": sha256(path)}
    command = build.get("command")
    require(isinstance(command, list) and command and all(isinstance(value, str) for value in command),
            "profile-cost build record has no exact command argv")
    return source, receipt_path, receipt, profile_dir, binaries, command


def candidate_receipt_value(source, binaries, command):
    return {
        "schema_version": 1,
        "source_root": str(ROOT),
        "base_head": COMMIT,
        "source_diff_sha256": source["source_diff_sha256"],
        "working_directory": str(ROOT),
        "target": TARGET,
        "profile": "release",
        "build_image": IMAGE,
        "binaries": binaries,
        "build_command": command,
    }


def copy_binaries(source_paths: dict[str, Path], destination: Path) -> dict:
    destination.mkdir(parents=True, exist_ok=False)
    result = {}
    for name, source in source_paths.items():
        target = destination / name
        shutil.copy2(source, target)
        require(target.is_file() and os.access(target, os.X_OK), f"copied binary is not executable: {target}")
        require(sha256(source) == sha256(target), f"binary copy changed bytes: {name}")
        result[name] = {"path": str(target), "sha256": sha256(target)}
    return result


def write_json_new(path: Path, value: dict) -> None:
    with path.open("x", encoding="utf-8") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def prepare_candidate(compare) -> tuple[dict, dict, dict]:
    source, _, _, _, original_binaries, command = validate_profile_inputs(compare)
    candidate_dir = BUILD_ROOT / "candidate-bin"
    receipt_path = BUILD_ROOT / "candidate-build-receipt.json"
    require(not BUILD_ROOT.exists(),
            "candidate bridge outputs already exist; inspect them before choosing a fresh run root")
    require_no_symlink(BUILD_ROOT)
    binaries = copy_binaries({name: Path(original_binaries[name]["path"]) for name in BINARY_NAMES}, candidate_dir)
    value = candidate_receipt_value(source, binaries, command)
    write_json_new(receipt_path, value)
    try:
        compare.validate_build_receipt(
            receipt_path, label="candidate", source_root=ROOT, source_commit=COMMIT,
            source_manifest=source, binaries=binaries,
        )
    except Exception:
        receipt_path.unlink()
        shutil.rmtree(candidate_dir)
        raise
    print(f"candidate receipt: {receipt_path}")
    print(f"candidate binaries: {candidate_dir}")
    return source, binaries, value


def load_candidate(compare):
    source, _, _, _, profile_binaries, command = validate_profile_inputs(compare)
    candidate_dir = BUILD_ROOT / "candidate-bin"
    receipt_path = BUILD_ROOT / "candidate-build-receipt.json"
    require(candidate_dir.is_dir() and receipt_path.is_file(),
            "run the candidate subcommand after profile-cost completes before reference-build")
    binaries = {}
    for name in BINARY_NAMES:
        path = candidate_dir / name
        require(path.is_file() and os.access(path, os.X_OK), f"candidate bridge binary is missing: {path}")
        binaries[name] = {"path": str(path), "sha256": sha256(path)}
        require(binaries[name]["sha256"] == profile_binaries[name]["sha256"],
                f"candidate bridge binary differs from profile-cost evidence: {name}")
    expected = candidate_receipt_value(source, binaries, command)
    actual = json.loads(receipt_path.read_text(encoding="utf-8"))
    require(actual == expected, "candidate bridge receipt differs from the current profile-cost evidence")
    compare.validate_build_receipt(
        receipt_path, label="candidate", source_root=ROOT, source_commit=COMMIT,
        source_manifest=source, binaries=binaries,
    )
    return source, binaries, receipt_path


def reference_build_command() -> list[str]:
    return [
        str(RUNNER), str(REFERENCE_ROOT), "build", "cargo", "build", "--locked", "--release",
        "--target", TARGET, "--message-format=json-render-diagnostics",
        "--config", "profile.release.opt-level=3",
        "--config", 'profile.release.lto="thin"',
        "--config", "profile.release.codegen-units=16",
        "-p", "xsht", "--bin", "xsht",
    ]


def profile_laputa_corpus() -> Path:
    require(PROFILE_RECEIPT_PATH is not None, "reference build requires the passed profile-cost receipt")
    receipt = json.loads(PROFILE_RECEIPT_PATH.read_text(encoding="utf-8"))
    binding = receipt.get("laputa_corpus")
    require(isinstance(binding, dict) and isinstance(binding.get("path"), str), "profile must bind an explicit Laputa corpus")
    recorded_path = Path(binding["path"])
    require(recorded_path.is_absolute(), "Laputa corpus binding must use an explicit absolute path")
    path = canonical_input_path(recorded_path, "Laputa corpus")
    require(path.is_absolute() and git(path, "rev-parse", "--show-toplevel") == str(path), "Laputa corpus must be its canonical Git root")
    current = {"commit": git(path, "rev-parse", "HEAD"), "git_tree": git(path, "rev-parse", "HEAD^{tree}"), "status": "clean"}
    require(not git(path, "status", "--porcelain=v1", "--untracked-files=all"), "Laputa corpus must be clean")
    require(binding.get("source_identity") == current and binding.get("source_identity_after") == current,
            "Laputa corpus differs from the unchanged passed profile binding")
    return path


def reference_docker_argv(path: Path, target_dir: Path, laputa: Path, command: list[str]) -> list[str]:
    require(not path.is_symlink() and path.is_file(), "reference runner did not produce a regular Docker argv capture")
    raw = path.read_bytes()
    require(raw.endswith(b"\0") and raw != b"\0", "reference Docker argv capture is empty or truncated")
    arguments = [os.fsdecode(value) for value in raw[:-1].split(b"\0")]
    require(all(arguments) and arguments[:2] == ["docker", "run"] and arguments.count(IMAGE_ID) == 1,
            "reference Docker argv must run the pinned image exactly once")
    image_at = arguments.index(IMAGE_ID)
    require(arguments[image_at + 1:] == command[3:], "reference Docker Cargo command differs from the requested build")
    options = arguments[2:image_at]
    for argument in options:
        require(argument.partition("=")[0] not in ("--net", "--mount", "--volume", "--env", "--env-file", "--workdir")
                and not argument.startswith("--platform=")
                and not (argument.startswith("-v") and argument != "-v")
                and not (argument.startswith("-e") and argument != "-e")
                and not (argument.startswith("-w") and argument != "-w"),
                "reference Docker argv contains an alternate mount, environment or network option")
    platform = [options[index + 1] for index, argument in enumerate(options[:-1]) if argument == "--platform"]
    require(options.count("--platform") == 1 and platform == ["linux/amd64"],
            "reference Docker platform differs from the pinned Linux build")
    for flag, value in (("--network", "none"), ("--cpus", "12"), ("--memory", "24g"), ("--memory-swap", "24g")):
        require([argument for argument in options if argument.partition("=")[0] == flag] == [f"{flag}={value}"],
                f"reference Docker build policy differs: {flag}")
    working = [options[index + 1] for index, argument in enumerate(options[:-1]) if argument == "-w"]
    require(options.count("-w") == 1 and working == [str(REFERENCE_ROOT)], "reference Docker working directory differs")
    volumes = [options[index + 1] for index, argument in enumerate(options[:-1]) if argument == "-v"]
    temporary = REFERENCE_ROOT / ".work/tmp"
    expected_volumes = [
        f"{REFERENCE_ROOT}:{REFERENCE_ROOT}:ro", f"{CONTROLLER_ROOT / '.git'}:{CONTROLLER_ROOT / '.git'}:ro",
        f"{laputa}:{laputa}:ro", "/home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro",
        "/usr/local/bin/mold:/usr/local/bin/mold:ro",
        f"{CONTROLLER_ROOT}/.work/consolidation/infra/mold-linker.sh:/usr/local/bin/xsh-mold-linker:ro",
        f"{temporary}:{temporary}:rw", f"{target_dir}:{target_dir}:rw",
        "xsh-consolidation-kache:/root/.cache/kache", "/usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro",
        "xsh-cargo-registry:/root/.cargo/registry",
    ]
    require(options.count("-v") == len(volumes) and volumes == expected_volumes, "reference Docker mounts differ from the reviewed launcher")
    environment = [options[index + 1] for index, argument in enumerate(options[:-1]) if argument == "-e"]
    require(options.count("-e") == len(environment), "reference Docker environment option is incomplete")
    for name, value in (("CARGO_TARGET_DIR", target_dir), ("XSH_LAPUTA_CORPUS", laputa), ("TMPDIR", temporary)):
        require([item for item in environment if item.partition("=")[0] == name] == [f"{name}={value}"],
                f"reference Docker environment differs: {name}")
    return arguments


def reference_build(compare) -> None:
    candidate_source, candidate_binaries, _ = load_candidate(compare)
    reference_source = validate_reference_inputs(compare, candidate_source)
    laputa = profile_laputa_corpus()

    reference_bin_dir = BUILD_ROOT / "reference-bin"
    receipt_path = BUILD_ROOT / "reference-build-receipt.json"
    process_record = BUILD_ROOT / "reference-build-process.json"
    require(not reference_bin_dir.exists() and not receipt_path.exists() and not process_record.exists(),
            "reference bridge outputs already exist; inspect them before choosing a fresh run root")
    require(not REFERENCE_RUN.exists(), f"reference build run directory already exists: {REFERENCE_RUN}")
    require_no_symlink(BUILD_ROOT)
    require_no_symlink(REFERENCE_RUN, CONTROLLER_ROOT)
    BUILD_ROOT.mkdir(parents=True, exist_ok=True)
    REFERENCE_RUN.mkdir(parents=True, exist_ok=False)
    target_dir = REFERENCE_RUN / "target" / "cargo"
    command = reference_build_command()
    runner_environment = {
        "XSH_GATE_TARGET_DIR": str(target_dir),
        "XSH_GATE_CIDFILE": str(REFERENCE_RUN / "build.cid"),
        "XSH_GATE_TOOL_DIR": str(RUNNER.parent),
        "XSH_GATE_LAPUTA_CORPUS": str(laputa),
        "XSH_GATE_DOCKER_ARGV_CAPTURE": str(REFERENCE_RUN / "build.docker-argv"),
        "TMPDIR": str(ROOT / ".work/tmp"),
    }
    environment = dict(os.environ)
    environment.update(runner_environment)
    stdout_path = BUILD_ROOT / "reference-build.stdout.log"
    stderr_path = BUILD_ROOT / "reference-build.stderr.log"
    with stdout_path.open("x", encoding="utf-8") as stdout, stderr_path.open("x", encoding="utf-8") as stderr:
        process = subprocess.run(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, check=False)
    stdout_text = stdout_path.read_text(encoding="utf-8", errors="replace")
    stderr_text = stderr_path.read_text(encoding="utf-8", errors="replace")
    write_json_new(process_record, {
        "argv": command,
        "working_directory": str(ROOT),
        "runner_environment": runner_environment,
        "returncode": process.returncode,
        "stdout": str(stdout_path),
        "stderr": str(stderr_path),
    })
    require(process.returncode == 0,
            f"reference Cargo build exited {process.returncode}; inspect {stdout_path} and {stderr_path}")
    docker_argv = reference_docker_argv(Path(runner_environment["XSH_GATE_DOCKER_ARGV_CAPTURE"]), target_dir, laputa, command)

    artifacts = []
    completed = []
    for line in stdout_text.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("reason") == "compiler-artifact":
            target = event.get("target", {})
            executable = event.get("executable")
            if target.get("name") == "xsht" and "bin" in target.get("kind", []) and executable:
                artifacts.append(Path(executable).resolve(strict=True))
        elif event.get("reason") == "build-finished":
            completed.append(event.get("success"))
    require(completed == [True], "reference Cargo output lacks one successful build-finished JSON event")
    expected_executable = (target_dir / TARGET / "release" / "xsht").resolve(strict=True)
    require(artifacts == [expected_executable],
            f"reference Cargo artifact event differs from isolated xsht output: {artifacts}")
    require(expected_executable.is_file() and os.access(expected_executable, os.X_OK),
            f"reference xsht artifact is not executable: {expected_executable}")

    reference_after = compare.capture_reference_source(REFERENCE_ROOT, COMMIT)
    require(reference_after == reference_source, "reference source changed during the Cargo build")
    require(profile_laputa_corpus() == laputa, "Laputa corpus changed during the reference Cargo build")
    candidate_after, candidate_binaries_after, _ = load_candidate(compare)
    require(candidate_after["tracked_tree_sha256"] == candidate_source["tracked_tree_sha256"],
            "candidate source changed during the reference Cargo build")
    for name in BINARY_NAMES:
        require(candidate_binaries_after[name]["sha256"] == candidate_binaries[name]["sha256"],
                f"candidate bridge binary changed during the reference Cargo build: {name}")
    reference_xsht = copy_binaries({"xsht": expected_executable}, reference_bin_dir)
    for name in ("xsh", "xsh-test-helper"):
        destination = reference_bin_dir / name
        shutil.copy2(candidate_bin_path(name), destination)
        require(sha256(destination) == candidate_binaries[name]["sha256"],
                f"paired child binary changed while constructing reference set: {name}")
        reference_xsht[name] = {"path": str(destination), "sha256": sha256(destination)}
    require(reference_xsht["xsht"]["sha256"] != candidate_binaries["xsht"]["sha256"],
            "reference and candidate xsht artifacts unexpectedly match")

    receipt = {
        "schema_version": 1,
        "source_root": str(REFERENCE_ROOT),
        "base_head": COMMIT,
        "source_diff_sha256": reference_source["source_diff_sha256"],
        "working_directory": str(REFERENCE_ROOT),
        "target": TARGET,
        "profile": "release",
        "build_image": IMAGE,
        "binaries": {"xsht": reference_xsht["xsht"]},
        "build_command": docker_argv,
    }
    write_json_new(receipt_path, receipt)
    try:
        compare.validate_build_receipt(
            receipt_path, label="reference", source_root=REFERENCE_ROOT, source_commit=COMMIT,
            source_manifest=reference_source, binaries={"xsht": reference_xsht["xsht"]},
        )
    except Exception:
        receipt_path.unlink()
        raise
    compare.capture_binaries(reference_bin_dir, BUILD_ROOT / "candidate-bin")
    print(f"reference receipt: {receipt_path}")
    print(f"reference binaries: {reference_bin_dir}")
    print(f"reference build logs: {stdout_path}, {stderr_path}")


def validate_reference_inputs(compare, candidate_source=None):
    require_frozen_root(REFERENCE_ROOT)
    if candidate_source is None:
        candidate_source = compare.capture_source(ROOT, COMMIT)
    reference_source = compare.capture_reference_source(REFERENCE_ROOT, COMMIT)
    changed = compare.validate_reference_overlay(candidate_source, reference_source, REFERENCE_ROOT)
    require(set(changed) == {"crates/xsht/src/commands.rs", "crates/xsht/src/xsht/test.rs"},
            f"reference changes are not the two-file scheduler overlay: {changed}")
    compare.capture_reference_derivation_patch(PATCH_PATH)
    compare.validate_reference_scheduler_ancestry(REFERENCE_ROOT, COMMIT)
    return reference_source


def qualify_reference(compare) -> None:
    require_frozen_root(ROOT)
    require(not git(ROOT, "status", "--porcelain=v1", "--untracked-files=all"),
            "candidate source root must be clean before final gate launch")
    source = compare.capture_source(ROOT, COMMIT)
    reference_source = validate_reference_inputs(compare, source)
    close = json.loads(FUNCTIONAL_CLOSE_PATH.read_text(encoding="utf-8"))
    require(close.get("functional_complete") is True and close.get("commit") == COMMIT,
            "functional-close record does not name the frozen source as complete")
    print(f"candidate source: {COMMIT} {TREE} clean")
    print(f"reference source: {reference_source['base_head']} {reference_source['changed_paths']}")
    print(f"reference patch: {PATCH_PATH}")


def candidate_bin_path(name: str) -> Path:
    path = BUILD_ROOT / "candidate-bin" / name
    require(path.is_file() and os.access(path, os.X_OK), f"candidate child binary is missing: {path}")
    return path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    qualify = actions.add_parser("qualify", help="validate source, close, reference overlay, and scheduler ancestry")
    candidate = actions.add_parser("candidate", help="adapt completed profile-cost evidence only")
    reference = actions.add_parser("reference-build", help="explicitly build the retained-patch reference arm")
    for action in (qualify, candidate, reference):
        action.add_argument("--source-root", type=Path, required=True)
        action.add_argument("--commit", required=True)
        action.add_argument("--tree-id", required=True)
        action.add_argument("--functional-close", type=Path, required=True)
        action.add_argument("--reference-source-root", type=Path, required=True)
        action.add_argument("--run-id", required=True)
    for action in (candidate, reference):
        action.add_argument("--profile-receipt", type=Path, required=True)
    args = parser.parse_args()
    try:
        configure(args)
        compare = load_comparator()
        if args.action == "qualify":
            qualify_reference(compare)
        elif args.action == "candidate":
            prepare_candidate(compare)
        else:
            reference_build(compare)
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"scheduler receipt bridge: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
