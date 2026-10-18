from contextlib import contextmanager, nullcontext, redirect_stderr, redirect_stdout
from importlib.util import module_from_spec, spec_from_file_location
from io import StringIO
import json
from pathlib import Path
import re
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


def make_laputa_checkout(path):
    path.mkdir(parents=True)
    (path / "pm").mkdir()
    (path / "pm/plan_json.xsh").write_text("let fixture = true\n", encoding="utf-8")
    subprocess.run(["git", "init", "--quiet", str(path)], check=True)
    subprocess.run(["git", "-C", str(path), "add", "pm/plan_json.xsh"], check=True)
    subprocess.run(
        ["git", "-C", str(path), "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "-m", "fixture"],
        check=True,
    )
    return path


def docker_base_command(tree, *, laputa_corpus=None, user=False, candidate_xsh=None):
    target_dir = tree / ".work/consolidation/target"
    repository_tmp = tree / ".work/tmp/container"
    if laputa_corpus is None:
        laputa_corpus = tree.parent / "laputa-fixture"
        laputa_corpus.mkdir(exist_ok=True)
    with (
        patch.object(profile, "ROOT", tree),
        patch.object(profile, "INFRA", tree / "infra"),
    ):
        return profile.docker_base(
            tree,
            target_dir,
            "0-11",
            "sha256:test-image",
            repository_tmp,
            laputa_corpus=laputa_corpus,
            user=user,
            candidate_xsh=candidate_xsh,
        )


def test_profile_build_context_has_no_baseline_xsh_dependency():
    with make_working_tree() as (tree, _close_record, _commit):
        command = docker_base_command(tree)

        assert not any("baseline-bin" in argument for argument in command)
        assert not any(argument.startswith("XSH_BIN=") for argument in command)
        assert f"{tree / '.work/tmp/container'}:/t:rw" in command
        assert [argument for argument in command if argument.startswith("TMPDIR=")] == ["TMPDIR=/t"]
        path = next(
            argument.removeprefix("PATH=")
            for argument in command
            if argument.startswith("PATH=")
        )
        assert path == "/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin"


def test_profile_build_mounts_selected_source_work_directory():
    with make_working_tree() as (root, _close_record, _commit):
        with make_working_tree() as (tree, _tree_close, _tree_commit):
            target_dir = tree / ".work/consolidation/target"
            repository_tmp = tree / ".work/tmp/container"
            laputa_corpus = tree.parent / "laputa-integration"
            laputa_corpus.mkdir()
            with patch.object(profile, "ROOT", root):
                command = profile.docker_base(
                    tree,
                    target_dir,
                    "0-11",
                    "sha256:test-image",
                    repository_tmp,
                    laputa_corpus=laputa_corpus,
                )
            assert f"{tree / '.work/consolidation'}:{tree / '.work/consolidation'}" in command
            assert f"{root / '.work/consolidation'}:{root / '.work/consolidation'}" not in command
            assert f"{laputa_corpus}:{laputa_corpus}:ro" in command
            assert f"XSH_LAPUTA_CORPUS={laputa_corpus}" in command
            assert "/home/josh/d/laputa-systems/laputa" not in command


def test_laputa_corpus_selection_requires_clean_exact_git_root_and_rejects_symlinks():
    with tempfile.TemporaryDirectory(dir=Path.cwd() / ".work/tmp") as temporary:
        root = Path(temporary)
        laputa = make_laputa_checkout(root / "laputa-integration")
        expected_path = laputa.resolve()
        expected_identity = {
            "commit": subprocess.check_output(["git", "-C", str(laputa), "rev-parse", "HEAD"], text=True).strip(),
            "git_tree": subprocess.check_output(["git", "-C", str(laputa), "rev-parse", "HEAD^{tree}"], text=True).strip(),
            "status": "clean",
        }
        alias = root / "laputa-alias"
        alias.symlink_to(laputa, target_is_directory=True)
        nested = laputa / "pm"
        empty = root / "not-a-checkout"
        empty.mkdir()

        for module in (profile, forced):
            selected_path, identity = module.laputa_corpus_source(laputa)
            assert selected_path == expected_path
            assert identity == expected_identity
            for invalid_path, expected_error in (
                (alias, "symlink"),
                (nested, "exact checkout root"),
                (empty, "checkout"),
            ):
                with redirect_stderr(StringIO()):
                    try:
                        module.laputa_corpus_source(invalid_path)
                    except (ValueError, forced.GateFailure) as error:
                        assert expected_error in str(error)
                    else:
                        raise AssertionError(f"{module.__name__} accepted invalid Laputa input {invalid_path}")

        (laputa / "untracked.xsh").write_text("let dirty = true\n", encoding="utf-8")
        for module in (profile, forced):
            try:
                module.laputa_corpus_source(laputa)
            except (ValueError, forced.GateFailure) as error:
                assert "must be clean" in str(error)
            else:
                raise AssertionError(f"{module.__name__} accepted a dirty Laputa checkout")


def test_profile_and_forced_commands_require_explicit_laputa_corpus():
    profile_arguments = [
        "--tree", "/source", "--commit", "a" * 40,
        "--functional-close", "/close.json", "--output", "/output",
    ]
    forced_arguments = [
        "--tree", "/source", "--commit", "a" * 40,
        "--functional-close", "/close.json", "--profile-receipt", "/profile.json",
        "--output", "/output",
    ]
    for parse_args, arguments in ((profile.parse_args, profile_arguments), (forced.parse_args, forced_arguments)):
        with redirect_stderr(StringIO()):
            try:
                parse_args(arguments)
            except SystemExit as error:
                assert error.code == 2
            else:
                raise AssertionError("gate command accepted an implicit Laputa corpus")
        selected = parse_args(arguments + ["--laputa-corpus", "/laputa-integration"])
        assert selected.laputa_corpus == Path("/laputa-integration")


def test_final_replay_requires_and_forwards_laputa_corpus_to_both_gates():
    source = (performance_dir.parents[2] / "final-replay.xsh").read_text(encoding="utf-8")
    assert 'laputa_corpus: {form: "--laputa-corpus PATH", kind: "Path", required: true}' in source
    for stage in ("release opt3 normal suite", "forced-frame normal suite"):
        command = re.search(
            rf'execute\("{re.escape(stage)}", \[(.*?)\n  \], root, scratch\)',
            source,
            re.DOTALL,
        )
        assert command is not None, f"final replay is missing the {stage} command"
        assert '"--laputa-corpus", laputa_corpus.display(),' in command.group(1)


def test_profile_runtime_context_requires_explicit_candidate_xsh():
    with make_working_tree() as (tree, _close_record, _commit):
        try:
            docker_base_command(tree, user=True)
        except ValueError as error:
            assert "candidate XSH executable" in str(error)
        else:
            raise AssertionError("runtime profile container accepted no candidate XSH executable")

        target_dir = tree / ".work/consolidation/target"
        target_dir.mkdir(parents=True)
        missing = target_dir / "missing-xsh"
        try:
            docker_base_command(tree, user=True, candidate_xsh=missing)
        except FileNotFoundError:
            pass
        else:
            raise AssertionError("runtime profile container accepted a missing candidate executable")


def test_profile_runtime_context_uses_only_the_candidate_xsh():
    with make_working_tree() as (tree, _close_record, _commit):
        candidate_xsh = tree / ".work/consolidation/target/release/xsh"
        candidate_xsh.parent.mkdir(parents=True)
        candidate_xsh.write_bytes(b"candidate xsh")
        candidate_xsh.chmod(0o755)
        command = docker_base_command(tree, user=True, candidate_xsh=candidate_xsh)

        assert not any("baseline-bin" in argument for argument in command)
        assert f"XSH_BIN={candidate_xsh}" in command
        path = next(
            argument.removeprefix("PATH=")
            for argument in command
            if argument.startswith("PATH=")
        )
        assert path == (
            f"{candidate_xsh.parent}:/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        )

        non_executable = candidate_xsh.parent / "non-executable-xsh"
        non_executable.write_bytes(b"not executable")
        non_executable.chmod(0o644)
        try:
            docker_base_command(tree, user=True, candidate_xsh=non_executable)
        except ValueError:
            pass
        else:
            raise AssertionError("runtime profile container accepted a non-executable candidate")

        symlink = candidate_xsh.parent / "symlink-xsh"
        symlink.symlink_to(candidate_xsh)
        try:
            docker_base_command(tree, user=True, candidate_xsh=symlink)
        except ValueError:
            pass
        else:
            raise AssertionError("runtime profile container accepted a symlink candidate")


def test_profile_output_path_rejects_symlinks():
    with tempfile.TemporaryDirectory(dir=Path.cwd() / ".work/tmp") as temporary:
        root = Path(temporary)
        outside = root / "outside"
        outside.mkdir()
        linked = root / "profile-output"
        linked.symlink_to(outside, target_is_directory=True)
        try:
            profile.absolute_without_symlinks(linked, "profile output")
        except ValueError as error:
            assert "cannot contain symlinks" in str(error)
        else:
            raise AssertionError("profile accepted a symlinked output path")


def run_profile_cost(tree, output, close_record, commit, *, artifacts_only=False):
    laputa_corpus = make_laputa_checkout(tree.parent / "laputa-integration")
    source_identity = {"commit": commit, "git_tree": "1" * 40, "status": "clean"}
    laputa_identity = profile.laputa_corpus_source(laputa_corpus)[1]
    stages = []
    lock_paths = []
    close_record.write_text(json.dumps({"functional_complete": True, "commit": commit}) + "\n", encoding="utf-8")

    def run_stage(command, log, *, run_id, stage, verify_after, measure_timing=True):
        stages.append(stage)
        assert measure_timing is (not artifacts_only)
        if stage.endswith("-build"):
            target_dir = Path(next(argument.split("=", 1)[1] for argument in command if argument.startswith("CARGO_TARGET_DIR=")))
            binary_dir = target_dir / profile.TARGET / "release"
            binary_dir.mkdir(parents=True)
            for name in binary_names:
                binary = binary_dir / name
                binary.write_bytes(name.encode())
                binary.chmod(0o755)
        log.write_text(f"{stage}\n")
        result = {
            "command": command,
            "status": 0,
            "wall_ns": 1 if measure_timing else None,
            "timing_claim_eligible": False,
            "timing_accepted": measure_timing,
            "timing_discarded": not measure_timing,
            "container_cleanup": {"resolved": True, "commands": []},
            "log_sha256": profile.digest(log),
            "stage_validation": verify_after(),
        }
        if not measure_timing:
            result["performance_gate"] = {"before": None, "after": None}
            result["timing_discard_reason"] = "artifacts-only mode"
        return result

    args = SimpleNamespace(
        tree=tree,
        commit=commit,
        functional_close=close_record,
        laputa_corpus=laputa_corpus,
        output=output,
        artifacts_only=artifacts_only,
    )

    def exclusive_lock(path, **_kwargs):
        lock_paths.append(path)
        return nullcontext()

    with (
        patch.object(profile, "ROOT", tree),
        patch.object(profile, "check_source", return_value=source_identity),
        patch.object(profile, "laputa_corpus_source", return_value=(laputa_corpus, laputa_identity)),
        patch.object(profile, "check_close", return_value={"functional_complete": True, "commit": commit}),
        patch.object(profile, "image_identity", return_value={"image_id": profile.NATIVE_IMAGE_ID, "clang_driver": "/clang", "tool_hashes": {}}),
        patch.object(profile, "cpu_list", return_value=",".join(str(cpu) for cpu in range(12))),
        patch.object(profile, "exclusive_lock", side_effect=exclusive_lock),
        patch.object(profile, "no_active_native_containers"),
        patch.object(profile, "run_stage", side_effect=run_stage),
        patch.object(profile, "probe_profile_resources", return_value={"affinity_cpus": list(range(12)), "memory_limit_bytes": profile.MEMORY_LIMIT_BYTES, "memory_swap_max_bytes": 0}),
    ):
        with redirect_stdout(StringIO()):
            profile.run(args)

    return json.loads((output / "receipt.json").read_text()), stages, lock_paths


def test_profile_cost_emits_only_the_selected_release_profile():
    with make_working_tree() as (tree, close_record, commit):
        output = tree / ".work/consolidation/profile-cost-test"
        receipt, stages, lock_paths = run_profile_cost(tree, output, close_record, commit)

        assert lock_paths == [
            profile.PROFILE_LOCK,
            profile.INFRA / "native-suite.lock",
            profile.INFRA / "cargo.lock",
        ]

        assert stages == ["release-opt3-build", "release-opt3-native-suite"]
        assert receipt["profile"] == {
            "name": "release",
            "lto": "thin",
            "codegen_units": 16,
            "opt_levels": [3],
        }
        assert [result["name"] for result in receipt["results"]] == ["release-opt3"]
        selected = receipt["results"][0]
        laputa_corpus = Path(receipt["laputa_corpus"]["path"])
        assert receipt["laputa_corpus"]["source_identity"]["status"] == "clean"
        assert receipt["laputa_corpus"]["source_identity_after"] == receipt["laputa_corpus"]["source_identity"]
        assert selected["build"]["status"] == 0
        assert selected["native_suite"]["status"] == 0
        assert not any(argument.startswith("XSH_BIN=") for argument in selected["build_command"])
        assert not any("baseline-bin" in argument for argument in selected["build_command"])
        assert selected["native_suite_command"][-1] == "test"
        assert selected["native_suite_command"][-2].endswith("/opt3/target/x86_64-unknown-linux-musl/release/xsht")
        assert any(argument.startswith("XSH_BIN=") for argument in selected["native_suite_command"])
        build_tmp_volume = next(argument for argument in selected["build"]["command"] if argument.endswith(":/t:rw"))
        test_tmp_volume = next(argument for argument in selected["native_suite"]["command"] if argument.endswith(":/t:rw"))
        build_tmp = Path(build_tmp_volume.removesuffix(":/t:rw"))
        test_tmp = Path(test_tmp_volume.removesuffix(":/t:rw"))
        assert build_tmp == test_tmp
        assert not build_tmp.exists()
        assert [argument for argument in selected["build"]["command"] if argument.startswith("TMPDIR=")] == ["TMPDIR=/t"]
        assert [argument for argument in selected["native_suite"]["command"] if argument.startswith("TMPDIR=")] == ["TMPDIR=/t"]
        for command in (selected["build"]["command"], selected["native_suite"]["command"]):
            assert f"{laputa_corpus}:{laputa_corpus}:ro" in command
            assert f"XSH_LAPUTA_CORPUS={laputa_corpus}" in command
            assert "/home/josh/d/laputa-systems/laputa" not in command
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


def test_artifact_qualification_is_resources_only_input():
    with make_working_tree() as (tree, close_record, commit):
        output = tree / ".work/consolidation/final-performance-gates/profile-artifacts-test"
        receipt, stages, lock_paths = run_profile_cost(tree, output, close_record, commit, artifacts_only=True)

        assert stages == ["release-opt3-build"]
        assert lock_paths == [profile.PROFILE_LOCK, profile.INFRA / "cargo.lock"]
        assert receipt["state"] == "artifact-qualified"
        assert receipt["qualification_mode"] == "artifacts-only"
        assert receipt["native_suite_lock_held_across_gate"] is False
        assert "native_suite" not in receipt["results"][0]
        build = receipt["results"][0]["build"]
        assert build["status"] == 0
        assert build["wall_ns"] is None
        assert build["timing_accepted"] is False
        assert build["timing_discarded"] is True
        assert build["performance_gate"] == {"before": None, "after": None}
        assert receipt["qualification_tool"]["path"].endswith("/profile-cost.py")
        assert receipt["qualification_tool"]["sha256"] == profile.digest(Path(receipt["qualification_tool"]["path"]))

        receipt_path = output / "receipt.json"
        with patch.object(observer, "source_identity", return_value=receipt["source_identity"]):
            try:
                observer.load_profile_binaries(receipt_path, tree, commit, 3)
            except observer.ObserverFailure as error:
                assert "resources-only observer" in str(error)
            else:
                raise AssertionError("timed observer accepted an artifact-qualified profile receipt")
            selected = observer.load_profile_binaries(
                receipt_path, tree, commit, 3, allow_artifact_qualified=True
            )
            assert selected["qualification_tool"] == receipt["qualification_tool"]

            completed_artifact = json.loads(json.dumps(receipt))
            completed_artifact["state"] = "complete"
            completed_artifact_path = output / "state-flipped.json"
            completed_artifact_path.write_text(json.dumps(completed_artifact), encoding="utf-8")
            for allow_artifacts in (False, True):
                try:
                    observer.load_profile_binaries(
                        completed_artifact_path, tree, commit, 3,
                        allow_artifact_qualified=allow_artifacts,
                    )
                except observer.ObserverFailure as error:
                    assert "artifacts-only profile receipts" in str(error)
                else:
                    raise AssertionError("changing only artifact receipt state made it time-qualified")

            outside_receipt_path = tree / ".work/consolidation-sibling/receipt.json"
            outside_receipt_path.parent.mkdir(parents=True)
            outside_receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
            try:
                observer.load_profile_binaries(
                    outside_receipt_path, tree, commit, 3, allow_artifact_qualified=True
                )
            except observer.ObserverFailure as error:
                assert "under the source .work/consolidation" in str(error)
            else:
                raise AssertionError("resource-only observer accepted an artifact receipt outside its qualified output root")

            wrong_source = json.loads(json.dumps(receipt))
            wrong_source["source_commit"] = "b" * 40
            wrong_source_path = output / "wrong-source.json"
            wrong_source_path.write_text(json.dumps(wrong_source), encoding="utf-8")
            try:
                observer.load_profile_binaries(wrong_source_path, tree, commit, 3, allow_artifact_qualified=True)
            except observer.ObserverFailure as error:
                assert "exact source commit" in str(error)
            else:
                raise AssertionError("resource-only observer accepted an artifact receipt for a different source")

            wrong_tree = json.loads(json.dumps(receipt))
            wrong_tree["source_identity"]["git_tree"] = "c" * 40
            wrong_tree_path = output / "wrong-source-tree.json"
            wrong_tree_path.write_text(json.dumps(wrong_tree), encoding="utf-8")
            try:
                observer.load_profile_binaries(wrong_tree_path, tree, commit, 3, allow_artifact_qualified=True)
            except observer.ObserverFailure as error:
                assert "source identity" in str(error)
            else:
                raise AssertionError("resource-only observer accepted an artifact receipt for a different source tree")

            wrong_tool = json.loads(json.dumps(receipt))
            wrong_tool["qualification_tool"]["sha256"] = "0" * 64
            wrong_tool_path = output / "wrong-tool-hash.json"
            wrong_tool_path.write_text(json.dumps(wrong_tool), encoding="utf-8")
            try:
                observer.load_profile_binaries(wrong_tool_path, tree, commit, 3, allow_artifact_qualified=True)
            except observer.ObserverFailure as error:
                assert "tool identity" in str(error)
            else:
                raise AssertionError("resource-only observer accepted a changed profile tool hash")

            wrong_close = json.loads(json.dumps(receipt))
            wrong_close["functional_close"]["record"]["commit"] = "d" * 40
            wrong_close_path = output / "wrong-close-record.json"
            wrong_close_path.write_text(json.dumps(wrong_close), encoding="utf-8")
            try:
                observer.load_profile_binaries(wrong_close_path, tree, commit, 3, allow_artifact_qualified=True)
            except observer.ObserverFailure as error:
                assert "functional-close evidence" in str(error)
            else:
                raise AssertionError("resource-only observer accepted a mismatched embedded functional-close record")

            boolean_status = json.loads(json.dumps(receipt))
            boolean_status["results"][0]["build"]["status"] = False
            boolean_status_path = output / "boolean-build-status.json"
            boolean_status_path.write_text(json.dumps(boolean_status), encoding="utf-8")
            try:
                observer.load_profile_binaries(boolean_status_path, tree, commit, 3, allow_artifact_qualified=True)
            except observer.ObserverFailure as error:
                assert "successful source, resource, and cleanup evidence" in str(error)
            else:
                raise AssertionError("resource-only observer treated false as a successful integer build status")

        try:
            forced.load_profile(receipt_path, tree, commit, receipt["source_identity"])
        except forced.GateFailure as error:
            assert "complete" in str(error)
        else:
            raise AssertionError("forced-frame gate accepted an artifact-qualified profile receipt")

        selected_binary = Path(receipt["results"][0]["binaries"]["xsh"]["path"])
        with patch.object(comparative, "source_state", return_value=receipt["source_identity"]):
            try:
                comparative.profile_cost_receipt_binding(receipt_path, tree, commit, selected_binary)
            except ValueError as error:
                assert "complete" in str(error)
            else:
                raise AssertionError("comparative gate accepted an artifact-qualified profile receipt")

        selected_binary.write_bytes(b"changed after artifact qualification")
        with patch.object(observer, "source_identity", return_value=receipt["source_identity"]):
            try:
                observer.load_profile_binaries(receipt_path, tree, commit, 3, allow_artifact_qualified=True)
            except observer.ObserverFailure as error:
                assert "selected release binary differs" in str(error)
            else:
                raise AssertionError("resource-only observer accepted a changed artifact binary hash")


if __name__ == "__main__":
    test_profile_build_context_has_no_baseline_xsh_dependency()
    test_profile_build_mounts_selected_source_work_directory()
    test_profile_runtime_context_requires_explicit_candidate_xsh()
    test_profile_runtime_context_uses_only_the_candidate_xsh()
    test_profile_output_path_rejects_symlinks()
    test_laputa_corpus_selection_requires_clean_exact_git_root_and_rejects_symlinks()
    test_profile_and_forced_commands_require_explicit_laputa_corpus()
    test_final_replay_requires_and_forwards_laputa_corpus_to_both_gates()
    test_profile_cost_emits_only_the_selected_release_profile()
    test_artifact_qualification_is_resources_only_input()
    print("selected release opt3 and artifact-qualified profile receipts passed")
