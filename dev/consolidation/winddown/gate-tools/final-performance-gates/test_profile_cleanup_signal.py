from importlib.util import module_from_spec, spec_from_file_location
import json
from pathlib import Path
import signal
import sys
import tempfile
from unittest.mock import MagicMock, patch

sys.dont_write_bytecode = True

root = Path.cwd()
performance_dir = Path(__file__).resolve().parent
sys.path.insert(0, str(performance_dir))
spec = spec_from_file_location("profile_cost", performance_dir / "profile-cost.py")
profile = module_from_spec(spec)
spec.loader.exec_module(profile)


def exercise_cleanup(resolved):
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp") as temporary:
        directory = Path(temporary)
        log = directory / "stage.log"
        cidfile = log.with_suffix(".cid")
        process = MagicMock()
        process.poll.return_value = None
        cleanup = {
            "resolved": resolved,
            "process": {"resolved": resolved, "commands": []},
            "containers": {"resolved": resolved, "container_ids": [], "commands": []},
        }
        def interrupt_wait(_process, _cleanup):
            cidfile.write_text("0123456789abcdef\n")
            raise profile.SignalReceived(signal.SIGTERM)

        with (
            patch.object(profile, "perf_gate_check", return_value={"exit_code": 0}),
            patch.object(profile.subprocess, "Popen", return_value=process),
            patch.object(profile, "wait_for_owned_process", side_effect=interrupt_wait),
            patch.object(profile, "cleanup_container_process", return_value=cleanup),
        ):
            result = profile.run_stage(
                ["docker", "run", "--rm", "pinned-image"],
                log,
                run_id="self-test-run",
                stage="self-test-stage",
            )

        assert result["interrupted"] is True
        assert result["cleanup_resolved"] is resolved
        assert result["container_cleanup"]["resolved"] is resolved
        assert result["cidfile"] == (None if resolved else str(cidfile))
        assert cidfile.exists() is (not resolved)

        lock = directory / "final-performance.lock"
        receipt_path = directory / "receipt.json"
        receipt = {"state": "running"}
        with profile.exclusive_lock(lock) as ownership:
            profile.terminal_receipt(
                receipt,
                receipt_path,
                ownership,
                "self-test-run",
                "interrupted",
                "received SIGTERM during self-test stage",
                result["container_cleanup"],
            )

        persisted = json.loads(receipt_path.read_text())
        assert persisted["state"] == ("interrupted" if resolved else "incomplete-cleanup")
        marker = lock.read_text()
        if resolved:
            assert marker == ""
            with profile.exclusive_lock(lock, reject_unresolved=True):
                pass
        else:
            assert marker.startswith("cleanup-unresolved:")
            try:
                with profile.exclusive_lock(lock, reject_unresolved=True):
                    raise AssertionError("unresolved cleanup marker did not block reuse")
            except ValueError as error:
                assert "cleanup remains unresolved" in str(error)


exercise_cleanup(True)
exercise_cleanup(False)
print("resolved and unresolved SIGTERM cleanup paths passed")


def exercise_absent_container():
    import owned_containers
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp") as temporary:
        cidfile = Path(temporary) / "build.cid"
        cidfile.write_text("0123456789abcdef\n")
        for message, expected in [
            ("error: no such object: 0123456789abcdef\n", True),
            ("Error: No such container: 0123456789abcdef\n", True),
            ("permission denied while connecting to Docker\n", False),
        ]:
            with patch.object(owned_containers, "_command", side_effect=[
                {"exit_code": 0, "stdout": "", "stderr": ""},
                {"exit_code": 1, "stdout": "[]\n", "stderr": message},
            ]) as command:
                result = owned_containers.cleanup_cidfile_container(
                    cidfile, owner="self-test-run", stage="self-test-stage"
                )
            assert result["resolved"] is expected, message
            assert command.call_count == 2
            assert result["cleaned_container_ids"] == []


exercise_absent_container()
print("absent containers are resolved; daemon errors remain unresolved")
