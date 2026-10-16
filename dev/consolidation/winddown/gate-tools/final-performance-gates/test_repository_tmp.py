from contextlib import contextmanager
from importlib.util import module_from_spec, spec_from_file_location
import os
from pathlib import Path
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


def test_forced_frames_does_not_use_shared_setgid_tmp():
    with setgid_repository() as (root, shared_tmp):
        with patch.object(forced, "ROOT", root):
            with forced.ensure_repository_tmp(forced.repository_tmp_path("test-run")) as private_tmp:
                assert_private_tmp(private_tmp, shared_tmp)
                inherited = private_tmp / "native-test-tmp"
                inherited.mkdir()
                assert not inherited.stat().st_mode & stat.S_ISGID
        assert not private_tmp.exists()


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


if __name__ == "__main__":
    test_profile_cost_does_not_use_shared_setgid_tmp()
    test_forced_frames_does_not_use_shared_setgid_tmp()
    test_profile_cost_rejects_a_symlinked_tmp_parent()
    test_forced_frames_rejects_a_symlinked_tmp_parent()
    print("gate temporary directories are private and clean up shared parent mode")
