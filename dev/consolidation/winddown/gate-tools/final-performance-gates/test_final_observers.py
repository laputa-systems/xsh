from importlib.util import module_from_spec, spec_from_file_location
import json
import os
from pathlib import Path
import shutil
import signal
import sys
import tempfile
from unittest.mock import MagicMock, patch


sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
WORKTREE_ROOT = HERE.parents[4]
TEST_SCRATCH = WORKTREE_ROOT / ".work/tmp"
TEST_SCRATCH.mkdir(parents=True, exist_ok=True)
sys.path.insert(0, str(HERE))


def load_module(name: str, path: Path):
    spec = spec_from_file_location(name, path)
    module = module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


observer = load_module("final_observers", HERE / "final-observers.py")
container_resources = load_module("capture_container_resources", HERE / "capture-container-resources.py")
docker_shim = load_module("docker_observer_shim", HERE / "docker-observer-shim.py")


def successful_resource_receipt(path: Path) -> None:
    cgroup = {
        "memory_peak": "4096",
        "memory_max": "8192",
        "memory_swap_peak": "0",
        "memory_swap_max": "0",
        "memory_events": {"oom": 0, "oom_kill": 0},
        "memory_swap_events": {"high": 0},
        "cpu_max": "1200000 100000",
    }
    path.write_text(json.dumps({
        "schema_version": 1,
        "status": 0,
        "timing_discarded": False,
        "process_rss_peak_bytes": 2048,
        "cgroup_limits_and_peaks": cgroup,
    }) + "\n", encoding="utf-8")


def exercise_stage(*, status: int = 0, ready_codes: tuple[int, int] = (0, 0)) -> dict[str, object]:
    with tempfile.TemporaryDirectory(dir=TEST_SCRATCH) as temporary:
        root = Path(temporary)
        stage_dir = root / "stage"
        stage_dir.mkdir()
        resource_path = stage_dir / "container-resources.json"
        successful_resource_receipt(resource_path)
        process = MagicMock()
        process.wait.return_value = status
        cleanup = {"resolved": True, "container_ids": [], "commands": []}
        checks = [
            {"command": ["perf-gate-ready"], "exit_code": ready_codes[0], "stdout": "pre\n", "stderr": ""},
            {"command": ["perf-gate-ready", "-1"], "exit_code": ready_codes[1], "stdout": "post\n", "stderr": ""},
        ]
        with (
            (root / "performance.lock").open("w+") as ownership_stream,
            patch.object(observer, "perf_gate_check", side_effect=checks) as perf,
            patch.object(observer.time, "perf_counter_ns", side_effect=[100, 250]) as clock,
            patch.object(observer.subprocess, "Popen", return_value=process) as popen,
            patch.object(observer, "cleanup_cidfile_container", return_value=cleanup),
        ):
            stage = observer.run_stage(
                ["synthetic-gate"],
                stage_dir,
                run_id="synthetic-run",
                stage_name="synthetic-stage",
                environment={"LC_ALL": "C"},
                container_id_file=stage_dir / "container.cid",
                resource_file=resource_path,
                ownership_stream=ownership_stream,
                validate_after=lambda: {"source": "sealed"},
            )
        return {
            "stage": stage,
            "perf": perf,
            "popen": popen,
            "process": process,
            "clock": clock,
            "receipt_exists": (stage_dir / "receipt.json").is_file(),
            "receipt": json.loads((stage_dir / "receipt.json").read_text(encoding="utf-8")),
        }


def assert_helper_pin_detects_drift() -> None:
    with tempfile.TemporaryDirectory(dir=TEST_SCRATCH) as temporary:
        root = Path(temporary)
        required = (
            "final-observers.py",
            "capture-container-resources.py",
            "docker-observer-shim.py",
            "owned_containers.py",
            "test_final_observers.py",
            "launch-plan.md",
        )
        assert len(required) == 6
        pins = {}
        for relative in required:
            if relative == "launch-plan.md":
                (root / relative).write_text("synthetic reviewed launch plan\n", encoding="utf-8")
            else:
                shutil.copyfile(HERE / relative, root / relative)
            pins[relative] = observer.sha256(root / relative)
        manifest_path = root / "manifest.json"
        sidecar_path = root / "manifest.json.sha256"
        manifest_path.write_text(json.dumps({"performance_observers": {"files": pins}}), encoding="utf-8")
        sidecar_path.write_text(observer.sha256(manifest_path) + "  manifest.json\n", encoding="ascii")
        with (
            patch.object(observer, "HERE", root),
            patch.object(observer, "ARTIFACT_MANIFEST", manifest_path),
            patch.object(observer, "ARTIFACT_MANIFEST_DIGEST", sidecar_path),
        ):
            pinned = observer.verify_artifact_pins()
            assert len(pinned["file_hashes"]) == len(required)
            assert "owned_containers.py" in pinned["file_hashes"]
            with (root / "owned_containers.py").open("ab") as helper:
                helper.write(b"\nsynthetic drift\n")
            try:
                observer.verify_artifact_pins()
            except observer.ObserverFailure as error:
                assert "owned_containers.py" in str(error)
            else:
                raise AssertionError("runtime cleanup helper drift must invalidate the observer pin")


def assert_supplemental_handoff() -> dict[str, object]:
    with tempfile.TemporaryDirectory(dir=TEST_SCRATCH) as temporary:
        root = Path(temporary)
        manifest_dir = root / "final-correctness"
        manifest_dir.mkdir()
        contexts = [{"harness": "other", "filter": "single-case", "privileged": False} for _ in range(4)]
        contexts.append({
            "harness": "core-compat-extra-privileged",
            "filter": "cp::privileged::test_cp_recursive_block_device_",
            "privileged": True,
        })
        manifest = {
            "linux_context_runs": contexts,
            "rust_harness_runs": [],
            "focused_native_files": [],
            "native_extra": [],
            "products": [{"name": "xsh"}],
            "rust_harness_builds": [{"name": "core-compat-extra-privileged"}],
            "warning_deny": [{"name": "clippy"}],
        }
        (manifest_dir / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        launcher = manifest_dir / "launch.py"
        with (
            patch.object(observer, "FINAL_CORRECTNESS_DIR", manifest_dir),
            patch.object(observer, "FINAL_CORRECTNESS_LAUNCHER", launcher),
        ):
            plan = observer.plan_data()
    handoff = plan["post_batch_supplemental_launcher"]
    assert handoff["after_stage"] == "canonical-gate-batch"
    assert handoff["linux_context_runs"] == len(manifest["linux_context_runs"]) == 5
    assert handoff["broad_test_run_counts"] == {
        "rust_harness_runs": 0,
        "focused_native_files": 0,
        "native_extra": 0,
        "docs_gate": 0,
        "full_native_suite": 0,
    }
    assert handoff["recursive_cp_filter_count"] == 1
    assert handoff["separate_prerequisites"] == {
        "product_builds": len(manifest["products"]),
        "rust_harness_builds": len(manifest["rust_harness_builds"]),
        "clippy_commands": len(manifest["warning_deny"]),
    }
    assert handoff["separate_prerequisites"]["clippy_commands"] > 0
    assert handoff["command"][2] == str(launcher)
    assert "--execute" in handoff["command"]
    assert "--expected-commit" in handoff["command"]
    assert "--expected-tree" in handoff["command"]
    assert "--out" in handoff["command"]
    return plan


def assert_gate_target_reuse_contract() -> None:
    with tempfile.TemporaryDirectory(dir=TEST_SCRATCH) as temporary:
        root = Path(temporary)
        output = root / "fresh-output"
        output.mkdir()
        selected_release = root / "selected-release"
        selected_release.mkdir()
        sealed_artifact = root / "reference-build"
        sealed_artifact.mkdir()
        source_worktrees = root / "source-worktrees"
        checkout = source_worktrees / "checkout"
        checkout.mkdir(parents=True)
        common = {
            "output": output,
            "tree": observer.ROOT,
            "selected_release_directory": selected_release,
            "source_worktrees": source_worktrees,
            "sealed_inputs": (sealed_artifact,),
        }

        target, reused = observer.gate_target_directory(None, **common)
        assert target == output / "gate-batch-target"
        assert reused is False
        assert not target.exists()

        cache = root / "existing-cache"
        cache.mkdir()
        target, reused = observer.gate_target_directory(cache, **common)
        assert target == cache.resolve()
        assert reused is True
        assert observer.gate_target_build_input(cache, target, reused) == {
            "requested_path": str(cache),
            "actual_path": str(cache.resolve()),
            "reused_existing_directory": True,
            "existing_contents_are_untrusted_until_cargo_revalidates_them": True,
        }
        assert observer.gate_target_build_input(None, output / "gate-batch-target", False) == {
            "requested_path": None,
            "actual_path": str(output / "gate-batch-target"),
            "reused_existing_directory": False,
            "existing_contents_are_untrusted_until_cargo_revalidates_them": False,
        }

        def reject(path: Path, *, expected: str) -> None:
            try:
                observer.gate_target_directory(path, **common)
            except observer.ObserverFailure as error:
                assert expected in str(error)
            else:
                raise AssertionError(f"unsafe Cargo target directory was accepted: {path}")

        reject(observer.ROOT, expected="below the source .work")
        reject(observer.ROOT / ".git", expected="below the source .work")
        reject(observer.ROOT.parent, expected="below the source .work")
        reject(checkout, expected="source worktrees")

        output_cache = output / "nested-cache"
        output_cache.mkdir()
        reject(output_cache, expected="observer output")
        reject(selected_release, expected="selected release binaries")

        reference_target = sealed_artifact / "target"
        reference_target.mkdir()
        reject(reference_target, expected="sealed input")

        symlink_target = root / "symlink-target"
        symlink_target.mkdir()
        cache_link = root / "cache-link"
        cache_link.symlink_to(symlink_target, target_is_directory=True)
        reject(cache_link, expected="symlink components")


def assert_gate_target_cli_forwarding() -> None:
    cache = HERE / "existing-cache"
    arguments = [
        "final-observers.py",
        "--execute",
        "--tree", str(observer.ROOT),
        "--commit", "a" * 40,
        "--functional-close", str(HERE / "functional-close.json"),
        "--profile-receipt", str(HERE / "profile-receipt.json"),
        "--release-opt-level", "3",
        "--output", str(HERE / "observer-output"),
        "--gate-target-dir", str(cache),
    ]
    sentinel = object()
    with (
        patch.object(sys, "argv", arguments),
        patch.object(observer, "install_signal_handlers", return_value=sentinel),
        patch.object(observer, "restore_signal_handlers") as restore,
        patch.object(observer, "run", return_value=0) as run,
    ):
        assert observer.main() == 0
    assert run.call_args.args[0].gate_target_dir == cache
    restore.assert_called_once_with(sentinel)


def exercise_repeated_signal_cleanup(*, process_exited: bool) -> tuple[dict[str, object], list[str]]:
    with tempfile.TemporaryDirectory(dir=TEST_SCRATCH) as temporary:
        root = Path(temporary)
        stage_dir = root / "stage"
        stage_dir.mkdir()
        resource_path = stage_dir / "container-resources.json"
        successful_resource_receipt(resource_path)
        lock_path = root / "performance.lock"
        marker_path = root / "native.cleanup-unresolved"
        process = MagicMock()
        process.wait.side_effect = observer.SignalReceived(signal.SIGTERM) if process_exited else None
        process.wait.return_value = 0
        process.poll.return_value = 0
        cleanup = {"resolved": False, "container_ids": ["synthetic-container"], "commands": []}
        events: list[str] = []

        def readiness(*, after: bool = False) -> dict[str, object]:
            events.append("after" if after else "before")
            return {"command": ["perf-gate-ready"] + (["-1"] if after else []), "exit_code": 0, "stdout": "", "stderr": ""}

        def repeated_signal_cleanup(*_args: object, **_kwargs: object) -> dict[str, object]:
            events.append("cleanup")
            signal.raise_signal(signal.SIGTERM)
            signal.raise_signal(signal.SIGINT)
            return cleanup

        previous = observer.install_signal_handlers()
        try:
            with (
                lock_path.open("w+") as ownership_stream,
                patch.object(observer, "perf_gate_check", side_effect=readiness),
                patch.object(observer.subprocess, "Popen", return_value=process),
                patch.object(observer, "cleanup_cidfile_container", side_effect=repeated_signal_cleanup),
                patch.object(observer, "NATIVE_CLEANUP_MARKER", marker_path),
            ):
                stage = observer.run_stage(
                    ["synthetic-gate"],
                    stage_dir,
                    run_id="synthetic-run",
                    stage_name="synthetic-stage",
                    environment={"LC_ALL": "C"},
                    container_id_file=stage_dir / "container.cid",
                    resource_file=resource_path,
                    ownership_stream=ownership_stream,
                    validate_after=lambda: {"source": "sealed"},
                )
        finally:
            observer.restore_signal_handlers(previous)
        stage_receipt = json.loads((stage_dir / "receipt.json").read_text(encoding="utf-8"))
        assert stage_receipt["state"] == "interrupted"
        assert stage_receipt["stage"]["container_cleanup"]["resolved"] is False
        assert lock_path.read_text(encoding="utf-8").startswith("cleanup-unresolved:")
        assert marker_path.read_text(encoding="utf-8").startswith("cleanup-unresolved:")
        assert stage["interrupted"] is True
        assert stage["timing_discarded"] is True
        assert stage["wall_ns"] is None
        assert stage["deferred_signals"]["counts"] == {"SIGINT": 1, "SIGTERM": 1}
        expected_events = ["before", "cleanup", "after"] if process_exited else ["before", "after", "cleanup"]
        assert events == expected_events
        return {"stage": stage}, events


def main() -> None:
    assert observer.gate_batch_command() == ["cargo", "dev", "gate", "--batch"]
    assert_helper_pin_detects_drift()
    plan = assert_supplemental_handoff()
    assert plan["stages"][1]["full_native_suite_count"] == 1
    assert plan["canonical_batch_rust_exclusions"] == ["cp::privileged::test_cp_recursive_block_device_"]
    assert plan["supplemental_isolated_tests"]["stage"] == "core-compat-extra-privileged"
    assert plan["supplemental_isolated_tests"]["context"].find("nodev /tmp") >= 0
    assert_gate_target_reuse_contract()
    assert_gate_target_cli_forwarding()

    args = [
        "run", "--rm", "--init", "--platform", "linux/amd64", "-v", "/source:/work",
        docker_shim.NATIVE_IMAGE_ID, *docker_shim.GATE_BATCH_TAIL,
    ]
    transformed = docker_shim.instrument_batch_argv(
        args,
        cidfile="/evidence/container.cid",
        owner="synthetic-run",
        container_observer="/work/observer.py",
        container_resource_file="/work/resource.json",
    )
    assert transformed is not None
    image_at = transformed.index(docker_shim.NATIVE_IMAGE_ID)
    assert transformed[image_at + 1:] == [
        "/usr/bin/python3", "/work/observer.py", "/work/resource.json", "--",
        "/root/.cargo/bin/cargo", *docker_shim.GATE_BATCH_TAIL[1:],
    ]
    assert transformed[1:5] == [
        "--cidfile", "/evidence/container.cid", "--label", "org.xsh.final-performance.owner=synthetic-run",
    ]
    assert docker_shim.instrument_batch_argv(
        args[:-1] + ["gate-lane"],
        cidfile="/evidence/container.cid",
        owner="synthetic-run",
        container_observer="/work/observer.py",
        container_resource_file="/work/resource.json",
    ) is None

    with tempfile.TemporaryDirectory(dir=TEST_SCRATCH) as temporary:
        root = Path(temporary)
        cgroup = root / "cgroup"
        proc = root / "proc"
        cgroup.mkdir()
        proc.mkdir()
        (proc / "42").mkdir()
        (proc / "42" / "statm").write_text("10 4 0 0 0 0 0\n", encoding="ascii")
        values = {
            "memory.current": "1024\n",
            "memory.peak": "2048\n",
            "memory.max": "8192\n",
            "memory.swap.current": "0\n",
            "memory.swap.peak": "0\n",
            "memory.swap.max": "0\n",
            "memory.events": "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n",
            "memory.swap.events": "high 0\nmax 0\nfail 0\n",
            "cpu.max": "1200000 100000\n",
            "pids.current": "3\n",
            "pids.peak": "5\n",
            "pids.max": "max\n",
        }
        for name, value in values.items():
            (cgroup / name).write_text(value, encoding="ascii")
        sample = container_resources.snapshot(cgroup, proc)
        assert sample["process_rss_sum_bytes"] == 4 * os.sysconf("SC_PAGE_SIZE")
        assert sample["cgroup"]["memory_peak"] == "2048"
        assert sample["cgroup"]["memory_events"]["oom_kill"] == 0
        sample_path = root / "resource.json"
        successful_resource_receipt(sample_path)
        assert observer.resource_receipt(sample_path)["receipt"]["process_rss_peak_bytes"] == 2048
        unavailable_path = root / "resource-unavailable.json"
        unavailable = json.loads(sample_path.read_text(encoding="utf-8"))
        unavailable["cgroup_limits_and_peaks"]["memory_swap_max"] = None
        unavailable_path.write_text(json.dumps(unavailable), encoding="utf-8")
        try:
            observer.resource_receipt(unavailable_path)
        except observer.ObserverFailure:
            pass
        else:
            raise AssertionError("unavailable cgroup evidence must reject a resource receipt")

        binary_dir = root / "release"
        binary_dir.mkdir()
        binary = binary_dir / "xsh"
        binary.write_bytes(b"sealed synthetic executable")
        binary.chmod(0o755)
        snapshot = observer.file_snapshot(binary)
        assert snapshot["mode"] == "0o0755"
        assert snapshot["size_bytes"] == len(b"sealed synthetic executable")
        assert len(snapshot["sha256"]) == 64
        linked_binary = root / "release-link"
        linked_binary.symlink_to(binary)
        try:
            observer.file_snapshot(linked_binary)
        except observer.ObserverFailure:
            pass
        else:
            raise AssertionError("sealed candidate binaries must be regular files, not symlinks")

        sealed_dir = root / "sealed-release"
        sealed_dir.mkdir()
        for name in observer.BINARY_NAMES:
            child = sealed_dir / name
            child.write_bytes(name.encode("ascii"))
            child.chmod(0o755)
        sealed = observer.binary_set(sealed_dir)
        assert observer.sealed_binary_set(sealed_dir, sealed) == sealed
        (sealed_dir / "xsh").write_bytes(b"changed")
        try:
            observer.sealed_binary_set(sealed_dir, sealed)
        except observer.ObserverFailure:
            pass
        else:
            raise AssertionError("a changed selected-release binary must reject timing")

    success = exercise_stage()
    assert success["stage"]["timing_accepted"] is True
    assert success["stage"]["observation_accepted"] is True
    assert success["stage"]["wall_ns"] > 0
    assert success["perf"].call_args_list[0].args == ()
    assert success["perf"].call_count == 2
    assert success["stage"]["stdout"]["path"].endswith("stdout.log")
    assert success["stage"]["stderr"]["path"].endswith("stderr.log")
    assert success["receipt_exists"] is True

    rejected_before = exercise_stage(ready_codes=(1, 0))
    assert rejected_before["stage"]["observation_accepted"] is True
    assert rejected_before["stage"]["timing_discarded"] is True
    assert rejected_before["stage"]["wall_ns"] is None
    assert "wall_ns_discarded" not in rejected_before["stage"]
    assert rejected_before["stage"]["timing_discard_reason"] == "pre-stage readiness failed; stage ran for resource observations only"
    assert rejected_before["stage"]["container_resources"] is not None
    assert rejected_before["receipt"]["state"] == "complete"
    assert rejected_before["popen"].call_count == 1
    assert rejected_before["perf"].call_count == 1
    assert rejected_before["clock"].call_count == 0

    rejected_after = exercise_stage(ready_codes=(0, 1))
    assert rejected_after["stage"]["observation_accepted"] is True
    assert rejected_after["stage"]["timing_discarded"] is True
    assert rejected_after["stage"]["wall_ns"] is None
    assert rejected_after["stage"]["wall_ns_discarded"] > 0
    assert rejected_after["receipt"]["state"] == "complete"

    failed_command = exercise_stage(status=7)
    assert failed_command["stage"]["status"] == 7
    assert failed_command["stage"]["observation_accepted"] is False
    assert failed_command["stage"]["timing_discarded"] is True
    assert failed_command["stage"]["wall_ns"] is None
    assert failed_command["receipt"]["state"] == "failed"

    normal_cleanup_signal, normal_events = exercise_repeated_signal_cleanup(process_exited=False)
    assert normal_cleanup_signal["stage"]["container_cleanup"]["resolved"] is False
    assert normal_events == ["before", "after", "cleanup"]
    exited_signal, exited_events = exercise_repeated_signal_cleanup(process_exited=True)
    assert exited_signal["stage"]["signal"] == "SIGTERM"
    assert exited_events == ["before", "cleanup", "after"]

    print("final observer pin, supplemental handoff, cgroup, sealing, readiness, discard, and repeated-signal cleanup self-tests passed")


if __name__ == "__main__":
    main()
