import io
import json
from argparse import Namespace
from contextlib import contextmanager, nullcontext, redirect_stderr, redirect_stdout
from importlib.util import module_from_spec, spec_from_file_location
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
from unittest.mock import patch

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


@contextmanager
def setgid_repository():
    with tempfile.TemporaryDirectory(dir=Path.cwd() / ".work/tmp") as temporary:
        root = Path(temporary)
        subprocess.run(["git", "init", "--quiet", str(root)], check=True)
        (root / ".git/info/exclude").write_text(".work/\n", encoding="utf-8")
        shared_tmp = root / ".work/tmp"
        shared_tmp.mkdir(parents=True)
        os.chmod(shared_tmp, 0o2755)
        yield root, shared_tmp


def assert_private_tmp(path, shared_tmp):
    assert path != shared_tmp
    assert path.parent == shared_tmp
    mode = path.lstat().st_mode
    assert stat.S_ISDIR(mode)
    assert stat.S_IMODE(mode) == 0o700
    assert not mode & stat.S_ISGID
    assert stat.S_IMODE(shared_tmp.stat().st_mode) == 0o2755


def test_profile_cost_does_not_use_shared_setgid_tmp():
    with setgid_repository() as (root, shared_tmp):
        with patch.object(profile, "ROOT", root):
            with profile.repository_tmp_dir("test-run") as private_tmp:
                assert_private_tmp(private_tmp, shared_tmp)
                inherited = private_tmp / "native-test-tmp"
                inherited.mkdir()
                assert not inherited.stat().st_mode & stat.S_ISGID
        assert not private_tmp.exists()


def test_profile_cost_uses_the_selected_source_tmp_root():
    with setgid_repository() as (root, _):
        with setgid_repository() as (tree, _):
            with patch.object(profile, "ROOT", root):
                with profile.repository_tmp_dir("selected-source", tree) as private_tmp:
                    assert private_tmp.parent == tree / ".work/tmp"
                    assert private_tmp.parent != root / ".work/tmp"
                    assert stat.S_IMODE(private_tmp.lstat().st_mode) == 0o700
            assert not private_tmp.exists()


def test_profile_cost_removes_execute_only_directories():
    with setgid_repository() as (root, _):
        with patch.object(profile, "ROOT", root):
            private_tmp = None
            unreadable = None
            try:
                with profile.repository_tmp_dir("test-run") as private_tmp:
                    unreadable = private_tmp / "unreadable"
                    unreadable.mkdir()
                    os.chmod(unreadable, 0o100)
            finally:
                if unreadable is not None and unreadable.exists():
                    os.chmod(unreadable, 0o700)
                if private_tmp is not None and private_tmp.exists():
                    shutil.rmtree(private_tmp)
        assert private_tmp is not None
        assert not private_tmp.exists()


def test_profile_cost_removes_symlinks_without_touching_targets():
    with setgid_repository() as (root, _):
        target = root / ".work/external-target"
        target.mkdir(parents=True)
        marker = target / "marker"
        marker.write_text("retained", encoding="utf-8")
        os.chmod(target, 0o500)
        with patch.object(profile, "ROOT", root):
            with profile.repository_tmp_dir("test-run") as private_tmp:
                (private_tmp / "external").symlink_to(target, target_is_directory=True)
        assert marker.read_text(encoding="utf-8") == "retained"
        assert stat.S_IMODE(target.stat().st_mode) == 0o500


def test_profile_tmp_cleanup_error_does_not_mask_gate_error():
    with setgid_repository() as (root, _):
        with patch.object(profile, "ROOT", root):
            gate_error = RuntimeError("gate failed")
            cleanup_error = PermissionError("cleanup failed")
            private_tmp = None
            try:
                with patch.object(profile.tempfile.TemporaryDirectory, "cleanup", side_effect=cleanup_error):
                    try:
                        with profile.repository_tmp_dir("test-run") as private_tmp:
                            raise gate_error
                    except RuntimeError as error:
                        assert error is gate_error
                        assert any("profile temporary cleanup also failed" in note for note in error.__notes__)
                    else:
                        raise AssertionError("profile cleanup replaced the gate failure")
            finally:
                if private_tmp is not None and private_tmp.exists():
                    shutil.rmtree(private_tmp)


def test_forced_frames_does_not_use_shared_setgid_tmp():
    with setgid_repository() as (root, shared_tmp):
        with patch.object(forced, "ROOT", root):
            with forced.ensure_repository_tmp(forced.repository_tmp_path("test-run")) as private_tmp:
                assert_private_tmp(private_tmp, shared_tmp)
                inherited = private_tmp / "native-test-tmp"
                inherited.mkdir()
                assert not inherited.stat().st_mode & stat.S_ISGID
        assert not private_tmp.exists()


def test_forced_frames_uses_the_selected_source_tmp_root():
    with setgid_repository() as (root, _):
        with setgid_repository() as (tree, _):
            with patch.object(forced, "ROOT", root):
                requested = forced.repository_tmp_path("selected-source", tree)
                with forced.ensure_repository_tmp(requested, tree) as private_tmp:
                    assert private_tmp.parent == tree / ".work/tmp"
                    assert private_tmp.parent != root / ".work/tmp"
                    assert stat.S_IMODE(private_tmp.lstat().st_mode) == 0o700
                    command = forced.container_command(
                        tree,
                        Path("/release"),
                        root / ".git",
                        Path("/laputa"),
                        private_tmp,
                        "0-11",
                    )
                    assert f"{tree / '.work/consolidation'}:{tree / '.work/consolidation'}" in command
                    assert f"{root / '.work/consolidation'}:{root / '.work/consolidation'}" not in command
                    assert "/laputa:/laputa:ro" in command
                    assert "XSH_LAPUTA_CORPUS=/laputa" in command
                    assert "/home/josh/d/laputa-systems/laputa" not in command
            assert not private_tmp.exists()


def test_forced_frames_removes_execute_only_directories():
    with setgid_repository() as (root, _):
        with patch.object(forced, "ROOT", root):
            private_tmp = None
            unreadable = None
            try:
                with forced.ensure_repository_tmp(forced.repository_tmp_path("test-run")) as private_tmp:
                    unreadable = private_tmp / "unreadable"
                    unreadable.mkdir()
                    os.chmod(unreadable, 0o100)
            finally:
                if unreadable is not None and unreadable.exists():
                    os.chmod(unreadable, 0o700)
                if private_tmp is not None and private_tmp.exists():
                    shutil.rmtree(private_tmp)
        assert private_tmp is not None
        assert not private_tmp.exists()


def test_forced_frames_removes_symlinks_without_touching_targets():
    with setgid_repository() as (root, _):
        target = root / ".work/external-target"
        target.mkdir(parents=True)
        marker = target / "marker"
        marker.write_text("retained", encoding="utf-8")
        os.chmod(target, 0o500)
        with patch.object(forced, "ROOT", root):
            with forced.ensure_repository_tmp(forced.repository_tmp_path("test-run")) as private_tmp:
                (private_tmp / "external").symlink_to(target, target_is_directory=True)
        assert marker.read_text(encoding="utf-8") == "retained"
        assert stat.S_IMODE(target.stat().st_mode) == 0o500


def test_forced_frames_records_the_yielded_tmp_in_receipt_and_command():
    with setgid_repository() as (root, _):
        profile_dir = root / ".work/products"
        profile_dir.mkdir(parents=True)
        output = root / ".work/evidence/forced-frames"
        output.parent.mkdir(parents=True)
        close_path = root / ".work/functional-close.json"
        profile_receipt_path = root / ".work/profile-receipt.json"
        laputa_corpus = root / ".work/laputa-integration"
        laputa_corpus.mkdir()
        laputa_identity = {"commit": "c" * 40, "git_tree": "d" * 40, "status": "clean"}
        source = {"commit": "a" * 40, "git_tree": "b" * 40, "status": "clean"}
        close = {"functional_complete": True}
        profile = {
            "receipt": {"state": "complete"},
            "name": "release",
            "profile": "release",
            "opt_level": 3,
            "directory": str(profile_dir),
            "binaries": {},
            "laputa_corpus": {
                "path": str(laputa_corpus),
                "source_identity": laputa_identity,
                "source_identity_after": laputa_identity,
            },
        }
        stage = {"status": 0, "cleanup_resolved": True, "interrupted_signal": None, "error": None}
        git_common = root / ".git"
        args = Namespace(tree=root, commit=source["commit"], functional_close=close_path, laputa_corpus=laputa_corpus, profile_receipt=profile_receipt_path, output=output)

        def git_path(command, **_kwargs):
            if command[-1] == "--git-common-dir":
                return str(git_common)
            if command[-1] == "info/exclude":
                return str(git_common / "info/exclude")
            raise AssertionError(command)

        with (
            patch.object(forced, "ROOT", root),
            patch.object(forced, "validate_static_paths", return_value=(root, close_path, profile_receipt_path, output, output.parent)),
            patch.object(forced, "source_identity", return_value=source),
            patch.object(forced, "laputa_corpus_source", return_value=(laputa_corpus, laputa_identity)),
            patch.object(forced, "load_functional_close", return_value=close),
            patch.object(forced, "load_profile", return_value=profile),
            patch.object(forced.shutil, "which", return_value="/usr/bin/docker"),
            patch.object(forced, "cpu_set", return_value=",".join(str(cpu) for cpu in range(12))),
            patch.object(forced.subprocess, "check_output", side_effect=git_path),
            patch.object(forced, "native_suite_lock", side_effect=lambda: nullcontext()),
            patch.object(forced, "install_signal_handlers", return_value=None),
            patch.object(forced, "restore_signal_handlers"),
            patch.object(forced, "inspect_native_image", return_value={"image_id": forced.IMAGE}),
            patch.object(forced, "no_active_native_containers"),
            patch.object(forced, "verify_inputs", return_value={"source": source}),
            patch.object(forced, "owned_docker_command", side_effect=lambda command, **_: {"argv": command}),
            patch.object(forced, "run_container", return_value=stage),
            redirect_stdout(io.StringIO()),
        ):
            assert forced.run(args) == 0
        receipt = json.loads((output / "receipt.json").read_text(encoding="utf-8"))
        assert receipt["native_suite_lock"]["path"] == str(forced.NATIVE_LOCK)
        assert receipt["laputa_corpus"] == {
            "path": str(laputa_corpus),
            "source_identity": laputa_identity,
            "source_identity_after": laputa_identity,
        }
        assert receipt["environment"]["XSH_LAPUTA_CORPUS"] == str(laputa_corpus)
        repository_tmp_volume = next(
            receipt["docker_command"]["argv"][index + 1]
            for index, argument in enumerate(receipt["docker_command"]["argv"][:-1])
            if argument == "-v" and receipt["docker_command"]["argv"][index + 1].endswith(":/t:rw")
        )
        repository_tmp = Path(repository_tmp_volume.removesuffix(":/t:rw"))
        assert repository_tmp.parent == root / ".work/tmp"
        assert repository_tmp.name.startswith(f"forced-frames-{receipt['run_id']}-")
        assert receipt["environment"]["TMPDIR"] == "/t"
        assert [argument for argument in receipt["docker_command"]["argv"] if argument.startswith("TMPDIR=")] == ["TMPDIR=/t"]
        assert not repository_tmp.exists()


def test_forced_frames_cleanup_error_does_not_mask_gate_error():
    with setgid_repository() as (root, _):
        with patch.object(forced, "ROOT", root):
            gate_error = forced.GateFailure("gate failed")
            cleanup_error = PermissionError("cleanup failed")
            private_tmp = None
            try:
                with patch.object(forced.tempfile.TemporaryDirectory, "cleanup", side_effect=cleanup_error):
                    try:
                        with forced.ensure_repository_tmp(forced.repository_tmp_path("test-run")) as private_tmp:
                            raise gate_error
                    except forced.GateFailure as error:
                        assert error is gate_error
                        assert any("forced-frame temporary cleanup also failed" in note for note in error.__notes__)
                        stderr = io.StringIO()
                        with patch.object(forced, "parse_args", return_value=Namespace()), patch.object(forced, "run", side_effect=error), redirect_stderr(stderr):
                            assert forced.main() == 1
                        assert "gate failed" in stderr.getvalue()
                        assert "forced-frame temporary cleanup also failed" in stderr.getvalue()
                        assert "cleanup failed" in stderr.getvalue()
                    else:
                        raise AssertionError("forced-frame cleanup replaced the gate failure")
            finally:
                if private_tmp is not None and private_tmp.exists():
                    shutil.rmtree(private_tmp)


def test_profile_cost_rejects_a_symlinked_tmp_parent():
    with setgid_repository() as (root, shared_tmp):
        shared_tmp.rmdir()
        target = root / ".work/real-tmp"
        target.mkdir(parents=True)
        shared_tmp.symlink_to(target, target_is_directory=True)
        with patch.object(profile, "ROOT", root):
            try:
                with profile.repository_tmp_dir("test-run"):
                    raise AssertionError("profile accepted a symlinked temporary parent")
            except ValueError as error:
                assert "cannot contain symlinks" in str(error)
        assert list(target.iterdir()) == []


def test_forced_frames_rejects_a_symlinked_tmp_parent():
    with setgid_repository() as (root, shared_tmp):
        shared_tmp.rmdir()
        target = root / ".work/real-tmp"
        target.mkdir(parents=True)
        shared_tmp.symlink_to(target, target_is_directory=True)
        with patch.object(forced, "ROOT", root):
            try:
                with forced.ensure_repository_tmp(forced.repository_tmp_path("test-run")):
                    raise AssertionError("forced frames accepted a symlinked temporary parent")
            except forced.GateFailure as error:
                assert "symlinks" in str(error)
        assert list(target.iterdir()) == []


def test_forced_frames_accepts_reviewed_helper_and_rejects_changes():
    with setgid_repository() as (root, _):
        evidence = root / ".work/final-review-evidence"
        performance = evidence / "final-performance-gates"
        performance.mkdir(parents=True)
        output_parent = root / ".work/consolidation/final-performance-gates"
        profile_dir = output_parent / f"profile-cost-{'a' * 40}"
        profile_dir.mkdir(parents=True)
        close = evidence / f"functional-close-{'a' * 40}.json"
        close.touch()
        profile_receipt = profile_dir / "receipt.json"
        profile_receipt.touch()
        helper = performance / "owned_containers.py"
        helper.write_bytes((performance_dir / "owned_containers.py").read_bytes())
        args = Namespace(tree=root, commit="a" * 40, functional_close=close, profile_receipt=profile_receipt, output=output_parent / f"forced-frames-{'a' * 40}")
        with patch.multiple(forced, ROOT=root, PERFORMANCE_DIR=performance, PROFILE_OUTPUT_ROOT=output_parent, FUNCTIONAL_CLOSE_ROOT=evidence):
            assert forced.validate_static_paths(args) == (root, close, profile_receipt, args.output, output_parent)
            helper.write_bytes(helper.read_bytes() + b"\n")
            try:
                forced.validate_static_paths(args)
            except forced.GateFailure as error:
                assert "differs from its inspected version" in str(error)
            else:
                raise AssertionError("forced-frame acceptance admitted a changed cleanup helper")


if __name__ == "__main__":
    test_profile_cost_does_not_use_shared_setgid_tmp()
    test_profile_cost_uses_the_selected_source_tmp_root()
    test_profile_cost_removes_execute_only_directories()
    test_profile_cost_removes_symlinks_without_touching_targets()
    test_profile_tmp_cleanup_error_does_not_mask_gate_error()
    test_forced_frames_does_not_use_shared_setgid_tmp()
    test_forced_frames_uses_the_selected_source_tmp_root()
    test_forced_frames_removes_execute_only_directories()
    test_forced_frames_removes_symlinks_without_touching_targets()
    test_forced_frames_records_the_yielded_tmp_in_receipt_and_command()
    test_forced_frames_cleanup_error_does_not_mask_gate_error()
    test_profile_cost_rejects_a_symlinked_tmp_parent()
    test_forced_frames_rejects_a_symlinked_tmp_parent()
    test_forced_frames_accepts_reviewed_helper_and_rejects_changes()
    print("gate temporary directories are private and clean up shared parent mode")
