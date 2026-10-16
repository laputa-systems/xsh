from contextlib import contextmanager, nullcontext, redirect_stdout
from importlib.util import module_from_spec, spec_from_file_location
from io import StringIO
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

binary_names = ("xsh", "xsht", "xsh-test-helper")
performance_dir = Path(__file__).resolve().parent

sys.path.insert(0, str(performance_dir))


def load_module(name, filename):
    spec = spec_from_file_location(name, performance_dir / filename)
    module = module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


profile = load_module("profile_cost", "profile-cost.py")
forced = load_module("forced_frames", "forced-frames.py")
observer = load_module("final_observers", "final-observers.py")
comparative = load_module("final_comparative", "final-comparative.py")


@contextmanager
def make_working_tree():
    with tempfile.TemporaryDirectory(dir=Path.cwd() / ".work/tmp") as temporary:
        tree = Path(temporary) / "source"
        tree.mkdir()
        subprocess.run(["git", "init", "--quiet", str(tree)], check=True)
        (tree / ".git/info/exclude").write_text(".work/\n", encoding="utf-8")
        for filename in ("Cargo.lock", "Cargo.toml", "rust-toolchain.toml"):
            (tree / filename).write_text(filename)
        close_record = tree / "functional-close.json"
        close_record.write_text('{"functional_complete": true}\n')
        yield tree, close_record, "a" * 40


def run_profile_cost(tree, output, close_record, commit):
    source_identity = {"commit": commit, "git_tree": "1" * 40, "status": "clean"}
    stages = []

    def docker_base(_tree, target_dir, _cpus, image, repository_tmp, user=False, candidate_xsh=None):
        return [
            "docker",
            "run",
            "-e",
            f"TMPDIR={repository_tmp}",
            "-e",
            f"CARGO_TARGET_DIR={target_dir}",
            "-e",
            f"XSH_BIN={candidate_xsh}" if candidate_xsh else "XSH_BIN=baseline",
            image,
        ]

    def run_stage(command, log, *, run_id, stage, verify_after):
        stages.append(stage)
        if stage.endswith("-build"):
            target_dir = Path(next(argument.split("=", 1)[1] for argument in command if argument.startswith("CARGO_TARGET_DIR=")))
            binary_dir = target_dir / profile.TARGET / "release"
            binary_dir.mkdir(parents=True)
            for name in binary_names:
                binary = binary_dir / name
                binary.write_bytes(name.encode())
                binary.chmod(0o755)
        log.write_text(f"{stage}\n")
        return {
            "command": command,
            "status": 0,
            "wall_ns": 1,
            "timing_accepted": True,
            "container_cleanup": {"resolved": True, "commands": []},
            "log_sha256": profile.digest(log),
            "stage_validation": verify_after(),
        }

    args = SimpleNamespace(
        tree=tree,
        commit=commit,
        functional_close=close_record,
        output=output,
    )
    with (
        patch.object(profile, "ROOT", tree),
        patch.object(profile, "check_source", return_value=source_identity),
        patch.object(profile, "check_close", return_value={"functional_complete": True, "commit": commit}),
        patch.object(profile, "image_identity", return_value={"image_id": "sha256:test", "clang_driver": "/clang", "tool_hashes": {}}),
        patch.object(profile, "cpu_list", return_value="0-11"),
        patch.object(profile, "exclusive_lock", side_effect=lambda *args, **kwargs: nullcontext()),
        patch.object(profile, "no_active_native_containers"),
        patch.object(profile, "docker_base", side_effect=docker_base),
        patch.object(profile, "run_stage", side_effect=run_stage),
        patch.object(profile, "probe_profile_resources", return_value={"affinity_cpus": list(range(12)), "memory_limit_bytes": profile.MEMORY_LIMIT_BYTES, "memory_swap_max_bytes": 0}),
    ):
        with redirect_stdout(StringIO()):
            profile.run(args)

    return json.loads((output / "receipt.json").read_text()), stages


def test_profile_cost_emits_only_the_selected_release_profile():
    with make_working_tree() as (tree, close_record, commit):
        output = tree / ".work/consolidation/profile-cost-test"
        receipt, stages = run_profile_cost(tree, output, close_record, commit)

        assert stages == ["release-opt3-build", "release-opt3-native-suite"]
        assert receipt["profile"] == {
            "name": "release",
            "lto": "thin",
            "codegen_units": 16,
            "opt_levels": [3],
        }
        assert [result["name"] for result in receipt["results"]] == ["release-opt3"]
        selected = receipt["results"][0]
        assert selected["build"]["status"] == 0
        assert selected["native_suite"]["status"] == 0
        assert selected["native_suite_command"][-1] == "test"
        assert selected["native_suite_command"][-2].endswith("/opt3/target/x86_64-unknown-linux-musl/release/xsht")
        build_tmp = next(argument.removeprefix("TMPDIR=") for argument in selected["build"]["command"] if argument.startswith("TMPDIR="))
        test_tmp = next(argument.removeprefix("TMPDIR=") for argument in selected["native_suite"]["command"] if argument.startswith("TMPDIR="))
        assert build_tmp == test_tmp
        assert not Path(build_tmp).exists()
        receipt_path = output / "receipt.json"
        assert forced.load_profile(receipt_path, tree, commit, receipt["source_identity"])["name"] == "release-opt3"
        with patch.object(observer, "source_identity", return_value=receipt["source_identity"]):
            assert observer.load_profile_binaries(receipt_path, tree, commit, 3)["profile"]["opt_level"] == 3
        selected_binary = Path(receipt["results"][0]["binaries"]["xsh"]["path"])
        with patch.object(comparative, "source_state", return_value=receipt["source_identity"]):
            assert comparative.profile_cost_receipt_binding(receipt_path, tree, commit, selected_binary)["profile"] == "release-opt3"

        invalid_receipts = []
        for legacy_level in (1, 2):
            invalid = json.loads(json.dumps(receipt))
            invalid["results"].append({"name": f"release-opt{legacy_level}"})
            invalid_receipts.append((f"receipt-opt{legacy_level}.json", invalid))
        duplicate = json.loads(json.dumps(receipt))
        duplicate["results"].append(json.loads(json.dumps(duplicate["results"][0])))
        invalid_receipts.append(("receipt-duplicate-opt3.json", duplicate))
        expanded_profile = json.loads(json.dumps(receipt))
        expanded_profile["profile"]["opt_levels"] = [1, 2, 3]
        invalid_receipts.append(("receipt-profile-matrix.json", expanded_profile))

        for filename, invalid in invalid_receipts:
            invalid_path = output / filename
            invalid_path.write_text(json.dumps(invalid))
            try:
                forced.load_profile(invalid_path, tree, commit, receipt["source_identity"])
            except forced.GateFailure as error:
                assert "selected release opt3 profile" in str(error)
            else:
                raise AssertionError(f"forced-frame validation accepted invalid receipt {filename}")
            with patch.object(observer, "source_identity", return_value=receipt["source_identity"]):
                try:
                    observer.load_profile_binaries(invalid_path, tree, commit, 3)
                except observer.ObserverFailure as error:
                    assert "selected release opt3 profile" in str(error)
                else:
                    raise AssertionError(f"observer validation accepted invalid receipt {filename}")
            with patch.object(comparative, "source_state", return_value=receipt["source_identity"]):
                try:
                    comparative.profile_cost_receipt_binding(invalid_path, tree, commit, selected_binary)
                except ValueError as error:
                    assert "selected release opt3 profile" in str(error)
                else:
                    raise AssertionError(f"comparative validation accepted invalid receipt {filename}")


if __name__ == "__main__":
    test_profile_cost_emits_only_the_selected_release_profile()
    print("selected release opt3 profile receipt passed")
