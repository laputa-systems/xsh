from importlib.util import module_from_spec, spec_from_file_location
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

repo_root = Path(__file__).resolve().parents[5]
compare_path = Path(__file__).with_name("compare.py")
spec = spec_from_file_location("scheduler_compare", compare_path)
compare = module_from_spec(spec)
spec.loader.exec_module(compare)
compare.ROOT = repo_root


def run_git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def expect_value_error(action, message):
    try:
        action()
    except ValueError as error:
        assert message in str(error), str(error)
    else:
        raise AssertionError(f"expected ValueError containing {message!r}")


def make_candidate(root):
    for relative in (
        "crates/xsht/src/commands.rs",
        "crates/xsht/src/xsht/test.rs",
    ):
        destination = root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(repo_root / relative, destination)
    subprocess.run(["git", "-C", str(root), "init", "--quiet"], check=True)
    subprocess.run(["git", "-C", str(root), "add", "."], check=True)
    env = dict(os.environ)
    env.update({
        "GIT_AUTHOR_NAME": "scheduler qualification",
        "GIT_AUTHOR_EMAIL": "scheduler-qualification@example.invalid",
        "GIT_COMMITTER_NAME": "scheduler qualification",
        "GIT_COMMITTER_EMAIL": "scheduler-qualification@example.invalid",
    })
    subprocess.run(
        [
            "git", "-C", str(root), "commit", "--quiet", "-m",
            "test(scheduler): create candidate source",
        ],
        check=True,
        env=env,
    )
    return run_git(root, "rev-parse", "HEAD")


def cloned_reference(candidate, reference):
    subprocess.run(
        ["git", "clone", "--quiet", "--shared", str(candidate), str(reference)],
        check=True,
    )


def test_derivation_patch_pin_and_ancestry():
    head = run_git(repo_root, "rev-parse", "HEAD")
    path = repo_root / compare.REFERENCE_AGGREGATE_PATCH_RELATIVE
    patch_bytes = path.read_bytes()
    expected = {
        str(path): {
            "sha256": compare.REFERENCE_AGGREGATE_PATCH_SHA256,
            "size_bytes": len(patch_bytes),
        }
    }
    assert compare.capture_reference_derivation_patches(repo_root, head) == expected

    with tempfile.TemporaryDirectory(dir=repo_root / ".work/tmp") as scratch:
        scratch_path = Path(scratch)
        missing = scratch_path / "missing.patch"
        expect_value_error(
            lambda: compare.capture_reference_derivation_patch(missing),
            "reference derivation patch is missing",
        )
        changed = scratch_path / "changed.patch"
        changed.write_bytes(path.read_bytes() + b"changed")
        expect_value_error(
            lambda: compare.capture_reference_derivation_patch(changed),
            "reference derivation patch hash changed",
        )

    expect_value_error(
        lambda: compare.validate_reference_scheduler_ancestry(
            repo_root, "97d3bec672d2e3673f9988840d70f9f6f274ccb8"
        ),
        "is not an ancestor",
    )


def test_exact_reference_overlay_and_two_file_control():
    aggregate = repo_root / compare.REFERENCE_AGGREGATE_PATCH_RELATIVE
    with tempfile.TemporaryDirectory(dir=repo_root / ".work/tmp") as scratch:
        scratch_path = Path(scratch)
        candidate = scratch_path / "candidate"
        reference = scratch_path / "reference"
        altered = scratch_path / "altered"
        candidate.mkdir()
        base_commit = make_candidate(candidate)
        cloned_reference(candidate, reference)
        subprocess.run(
            ["git", "-C", str(reference), "apply", str(aggregate)],
            check=True,
        )

        candidate_source = compare.capture_source(candidate, base_commit)
        reference_source = compare.capture_reference_source(reference, base_commit)
        changed = compare.validate_reference_overlay(
            candidate_source, reference_source, reference
        )
        assert changed == [
            "crates/xsht/src/commands.rs",
            "crates/xsht/src/xsht/test.rs",
        ]

        cloned_reference(candidate, altered)
        for relative in (
            "crates/xsht/src/commands.rs",
            "crates/xsht/src/xsht/test.rs",
        ):
            path = altered / relative
            path.write_bytes(path.read_bytes() + b"\n")
        altered_source = compare.capture_reference_source(altered, base_commit)
        expect_value_error(
            lambda: compare.validate_reference_overlay(
                candidate_source, altered_source, altered
            ),
            "does not match the pinned scheduler reference overlay",
        )


test_derivation_patch_pin_and_ancestry()
test_exact_reference_overlay_and_two_file_control()
print("scheduler reference derivation and source qualification passed")
