from argparse import Namespace
from importlib.util import module_from_spec, spec_from_file_location
import json
import os
import subprocess
from pathlib import Path
import tempfile
from unittest.mock import patch

root = Path(__file__).resolve().parents[2]
spec = spec_from_file_location("scheduler_bridge", Path(__file__).with_name("final_scheduler_receipt_bridge.py"))
bridge = module_from_spec(spec)
spec.loader.exec_module(bridge)



def test_reference_output_uses_the_shared_launcher_namespace():
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp") as temporary:
        controller = Path(temporary)
        common = controller / ".git"
        common.mkdir()
        source = controller / ".work/wt/source"
        reference = source / ".work/wt/reference"
        reference.mkdir(parents=True)
        close = source / "close.json"
        close.write_text("{}")
        commit, tree = "a" * 40, "b" * 40
        def git(selected, *args):
            if args[-1] == "--git-common-dir":
                return str(common)
            if args[-1] == "--show-toplevel":
                return str(selected)
            if args == ("rev-parse", "HEAD"):
                return commit
            if args == ("rev-parse", "HEAD^{tree}"):
                return tree
            if args[0] == "status":
                return ""
            raise AssertionError(args)
        args = Namespace(source_root=source, reference_source_root=reference, functional_close=close, profile_receipt=None, commit=commit, tree_id=tree, run_id="unit")
        with patch.object(bridge, "git", side_effect=git):
            bridge.configure(args)
        expected = controller / ".work/final-review-evidence/final-correctness-gates" / f"native-scheduler-reference-{commit}-unit"
        assert bridge.REFERENCE_RUN == expected, (bridge.REFERENCE_RUN, expected)
        assert bridge.BUILD_ROOT == source / f".work/consolidation/native-scheduler-builds/{commit}/unit"
        assert bridge.RUNNER == root / "dev/consolidation/winddown/gate-tools/final-correctness-gates/run-native-final.sh"


def test_reference_runner_receives_and_records_its_tool_directory():
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp") as temporary:
        source = Path(temporary)
        runner = source / "dev/consolidation/winddown/gate-tools/final-correctness-gates/run-native-final.sh"
        runner.parent.mkdir(parents=True)
        runner.write_text('''#!/usr/bin/env python3
import os
import subprocess
from pathlib import Path
import sys
expected = Path(__file__).parent
sys.exit(77 if os.environ.get("XSH_GATE_TOOL_DIR") == str(expected) else 78)
''')
        runner.chmod(0o700)
        build_root = source / ".work/build"
        reference_run = source / ".work/final-review-evidence/final-correctness-gates/unit"
        with patch.multiple(bridge, create=True, ROOT=source, CONTROLLER_ROOT=source, BUILD_ROOT=build_root, REFERENCE_RUN=reference_run, REFERENCE_ROOT=source / "reference", RUNNER=runner), patch.object(bridge, "load_candidate", return_value=({}, {}, None)), patch.object(bridge, "validate_reference_inputs", return_value={}), patch.object(bridge, "profile_laputa_corpus", return_value=source / "laputa"), patch.dict(os.environ, {"XSH_GATE_TOOL_DIR": "unrelated-ambient-tools"}):
            try:
                bridge.reference_build(object())
            except ValueError as error:
                assert "reference Cargo build exited 77" in str(error), str(error)
            else:
                raise AssertionError("reference build accepted the runner failure")
        record = json.loads((build_root / "reference-build-process.json").read_text())
        assert record["runner_environment"]["XSH_GATE_TOOL_DIR"] == str(source / "dev/consolidation/winddown/gate-tools/final-correctness-gates")
        assert record["runner_environment"]["XSH_GATE_LAPUTA_CORPUS"] == str(source / "laputa")
        assert record["runner_environment"]["XSH_GATE_DOCKER_ARGV_CAPTURE"] == str(reference_run / "build.docker-argv")
        assert record["runner_environment"]["XSH_GATE_TARGET_DIR"] == str(reference_run / "target/cargo")
        assert record["returncode"] == 77


def test_reference_docker_capture_matches_the_actual_launcher():
    runner = root / "dev/consolidation/winddown/gate-tools/final-correctness-gates/run-native-final.sh"
    text = runner.read_text()
    base = text[text.index("TASK_DOCKER_BASE=("):text.index("\n\ntask_docker()")]
    temporary_args = next(line for line in text.splitlines() if line.strip().startswith("TASK_TMP_ARGS=("))
    invocation = next(line.strip() for line in text.splitlines() if line.strip().startswith('task_docker "${TASK_DOCKER_BASE[@]}"') and "LD_PRELOAD=" in line)
    invocation = invocation.replace("task_docker ", "printf '%s\\0' ", 1)
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp") as directory:
        controller = Path(directory)
        reference = controller / "reference"
        target = controller / "run/target/cargo"
        laputa = controller / "laputa"
        capture = controller / "build.docker-argv"
        environment = dict(os.environ, TASK_TREE=str(reference), TASK_ROOT=str(controller), TASK_INFRA=str(controller / ".work/consolidation/infra"), TASK_LAPUTA_CORPUS=str(laputa), TASK_CIDFILE=str(controller / "build.cid"), TASK_TARGET_DIR=str(target), TASK_DISK_TMPDIR=str(reference / ".work/tmp"))
        with patch.multiple(bridge, CONTROLLER_ROOT=controller, REFERENCE_ROOT=reference):
            command = bridge.reference_build_command()
            raw = subprocess.check_output(["bash", "-c", base + "\n" + temporary_args + "\n" + invocation, "launcher-capture-test", *command[3:]], env=environment)
            capture.write_bytes(raw)
            actual = bridge.reference_docker_argv(capture, target, laputa, command)
            assert actual[actual.index(bridge.IMAGE_ID) + 1:] == command[3:]
            for mutation in ("truncated", "image", "network", "target", "laputa_env", "laputa_mount", "cargo", "mount_alias"):
                changed = list(actual)
                if mutation == "truncated":
                    capture.write_bytes(raw[:-1])
                else:
                    if mutation == "image":
                        changed[changed.index(bridge.IMAGE_ID)] = "unqualified-image"
                    elif mutation == "network":
                        changed[changed.index("--network=none")] = "--network=host"
                    elif mutation == "target":
                        changed[changed.index(f"CARGO_TARGET_DIR={target}")] = "CARGO_TARGET_DIR=/wrong"
                    elif mutation == "laputa_env":
                        changed[changed.index(f"XSH_LAPUTA_CORPUS={laputa}")] = "XSH_LAPUTA_CORPUS=/wrong"
                    elif mutation == "laputa_mount":
                        changed[changed.index(f"{laputa}:{laputa}:ro")] = f"{laputa}:{laputa}:rw"
                    elif mutation == "cargo":
                        changed[-1] = "unqualified-binary"
                    elif mutation == "mount_alias":
                        changed[2:2] = ["--mount", f"type=bind,src=/wrong,dst={laputa}"]
                    capture.write_bytes(b"\0".join(os.fsencode(value) for value in changed) + b"\0")
                try:
                    bridge.reference_docker_argv(capture, target, laputa, command)
                except ValueError:
                    pass
                else:
                    raise AssertionError(f"invalid runner Docker capture accepted: {mutation}")
            capture.unlink()
            try:
                bridge.reference_docker_argv(capture, target, laputa, command)
            except ValueError:
                pass
            else:
                raise AssertionError("absent runner Docker capture accepted")


def test_profile_laputa_binding_requires_the_same_clean_corpus():
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp") as directory:
        temporary = Path(directory)
        corpus = temporary / "laputa"
        corpus.mkdir()
        subprocess.run(["git", "init", "--quiet", str(corpus)], check=True)
        (corpus / "generation.xsh").write_text('print "fixture"\n')
        subprocess.run(["git", "-C", str(corpus), "add", "generation.xsh"], check=True)
        subprocess.run(["git", "-C", str(corpus), "-c", "user.name=Scheduler fixture", "-c", "user.email=scheduler@example.invalid", "commit", "--quiet", "-m", "test(scheduler): freeze corpus fixture"], check=True)
        identity = {"commit": bridge.git(corpus, "rev-parse", "HEAD"), "git_tree": bridge.git(corpus, "rev-parse", "HEAD^{tree}"), "status": "clean"}
        binding = {"path": str(corpus), "source_identity": identity, "source_identity_after": identity}
        receipt = temporary / "profile.json"
        with patch.object(bridge, "PROFILE_RECEIPT_PATH", receipt):
            receipt.write_text(json.dumps({"laputa_corpus": binding}))
            assert bridge.profile_laputa_corpus() == corpus
            for changed in (None, {**binding, "path": os.path.relpath(corpus)}, {**binding, "source_identity_after": None}, {**binding, "source_identity": {**identity, "commit": "0" * 40}}):
                receipt.write_text(json.dumps({"laputa_corpus": changed}))
                try:
                    bridge.profile_laputa_corpus()
                except ValueError:
                    pass
                else:
                    raise AssertionError("incomplete or stale corpus binding accepted")
            receipt.write_text(json.dumps({"laputa_corpus": binding}))
            (corpus / "untracked.xsh").write_text('print "dirty"\n')
            try:
                bridge.profile_laputa_corpus()
            except ValueError as error:
                assert "clean" in str(error)
            else:
                raise AssertionError("dirty corpus accepted")


def test_reference_environment_satisfies_actual_launcher_preflight():
    common = Path(bridge.git(root, "rev-parse", "--path-format=absolute", "--git-common-dir"))
    controller = common.parent
    launcher = root / "dev/consolidation/winddown/gate-tools/final-correctness-gates/run-native-final.sh"
    text = launcher.read_text()
    prefix = text[:text.index('if [ -e "$TASK_NATIVE_CLEANUP_MARKER" ]; then')]
    gate = controller / ".work/final-review-evidence/final-correctness-gates"
    with tempfile.TemporaryDirectory(prefix="scheduler-unit-", dir=gate) as directory, tempfile.TemporaryDirectory(dir=root / ".work/tmp") as corpus_directory:
        run = Path(directory)
        corpus = Path(corpus_directory)
        subprocess.run(["git", "init", "--quiet", str(corpus)], check=True)
        environment = dict(os.environ, XSH_GATE_TOOL_DIR=str(launcher.parent), XSH_GATE_CIDFILE=str(run / "build.cid"), XSH_GATE_TARGET_DIR=str(run / "target/cargo"), XSH_GATE_LAPUTA_CORPUS=str(corpus), XSH_GATE_DOCKER_ARGV_CAPTURE=str(run / "build.docker-argv"))
        def preflight(values):
            return subprocess.run(["bash", "-c", prefix, "launcher-preflight-test", str(root), "build"], env=values, capture_output=True)
        result = preflight(environment)
        assert result.returncode == 0, result.stderr
        for name in ("XSH_GATE_LAPUTA_CORPUS", "XSH_GATE_DOCKER_ARGV_CAPTURE"):
            missing = dict(environment)
            del missing[name]
            assert preflight(missing).returncode == 2
        wrong = dict(environment, XSH_GATE_TARGET_DIR=str(root / ".work/wt/source/.work/final-review-evidence/final-correctness-gates/unit/target/cargo"))
        result = preflight(wrong)
        assert result.returncode == 2 and b"Cargo target must be isolated" in result.stderr
        assert not (run / "target").exists()


def test_reference_preflight_from_tool_and_controller_paths():
    test_reference_environment_satisfies_actual_launcher_preflight()
    common = Path(bridge.git(root, "rev-parse", "--path-format=absolute", "--git-common-dir"))
    with patch.dict(globals(), {"root": common.parent}):
        test_reference_environment_satisfies_actual_launcher_preflight()
    print("launcher namespace policy passed from tool and controller paths")


if __name__ == "__main__":
    test_reference_output_uses_the_shared_launcher_namespace()
    test_reference_runner_receives_and_records_its_tool_directory()
    test_reference_docker_capture_matches_the_actual_launcher()
    test_profile_laputa_binding_requires_the_same_clean_corpus()
    test_reference_preflight_from_tool_and_controller_paths()
    print("scheduler reference runner receives and records the reviewed tool directory")
