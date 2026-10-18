"""Host-boundary checks for evidence rejection and process-tree timeouts."""

import argparse
import base64
from contextlib import redirect_stdout
import io
import json
import os
import signal
from pathlib import Path
import shutil
import subprocess
import sys
import unittest
from unittest.mock import patch

from fixtures import generate
from prepare import prepare
from run import MEMORY_LIMIT_BYTES, equivalent, invoke, order_for, summarize_samples, validate_close, validate_container_resources, validate_fixtures

HERE = Path(__file__).resolve().parent


class HarnessTests(unittest.TestCase):
    def setUp(self):
        self.root = HERE / "../../.work/consolidation/comparative-harness-tests"
        self.root = self.root.resolve()
        self.root.mkdir(parents=True, exist_ok=False)

    def tearDown(self):
        shutil.rmtree(self.root)

    def test_fixture_manifest_rejects_changed_bytes_and_symlink_targets(self):
        fixtures = self.root / "fixtures"
        generate(fixtures)
        validate_fixtures(fixtures)
        link = fixtures / "tree/link-dangling"
        link.unlink()
        link.symlink_to("different")
        with self.assertRaisesRegex(ValueError, "differ from manifest"):
            validate_fixtures(fixtures)

    def test_a_rewritten_manifest_cannot_bless_different_inputs(self):
        fixtures = self.root / "fixtures"
        generate(fixtures)
        manifest = fixtures / "manifest.json"
        manifest.write_text("{}\n")
        with self.assertRaisesRegex(ValueError, "pinned workload"):
            validate_fixtures(fixtures)

    def test_equal_stdout_does_not_hide_failure_stderr_or_effects(self):
        observation = {"status": 0, "timed_out": False, "stdout_base64": base64.b64encode(b"ok\n").decode(), "stderr_base64": "", "effects": {}}
        self.assertTrue(equivalent(observation, b"ok\n"))
        for changed in ({"status": 1}, {"stderr_base64": "ZXJyb3I="}, {"effects": {"file": {"after": "new"}}}, {"timed_out": True}):
            self.assertFalse(equivalent(observation | changed, b"ok\n"))

    def test_measurement_requires_completion_at_the_measured_commit(self):
        commit = "a" * 40
        close = self.root / "close.json"
        close.write_text(json.dumps({"functional_complete": True, "commit": commit}))
        self.assertEqual(validate_close(close, commit)["commit"], commit)
        with self.assertRaisesRegex(ValueError, "measured source commit"):
            validate_close(close, "b" * 40)
        with self.assertRaisesRegex(ValueError, "requires --functional-close"):
            validate_close(None, commit)

    def test_every_counterbalanced_sample_contains_every_tool_once(self):
        tools = ("a", "b", "c", "d", "e", "f", "g")
        for round_index in range(3):
            for sample_index in range(60):
                self.assertEqual(sorted(order_for(round_index, sample_index, tools)), sorted(tools))
        positions = {tool: set() for tool in tools}
        for sample_index in range(14):
            for position, tool in enumerate(order_for(0, sample_index, tools)):
                positions[tool].add(position)
        self.assertTrue(all(len(value) == 7 for value in positions.values()))

    def test_round_medians_use_each_round_instead_of_the_pooled_samples(self):
        samples = [
            {"tool": "dash", "workload": "pipeline", "round": round_index, "wall_ns": value}
            for round_index, values in enumerate(((1, 3, 5), (10, 20, 30), (100, 200, 300)))
            for value in values
        ]

        summary = summarize_samples(samples, "dash", "pipeline")

        self.assertEqual(summary["median_ns"], 20)
        self.assertEqual(summary["round_medians_ns"], [3, 20, 200])

    def test_container_resource_limits_must_match_the_recorded_cpuset_and_memory(self):
        resources = validate_container_resources(3, MEMORY_LIMIT_BYTES, [3], MEMORY_LIMIT_BYTES, 0)
        self.assertEqual(resources["affinity_cpus"], [3])
        self.assertEqual(resources["memory_limit_bytes"], MEMORY_LIMIT_BYTES)
        with self.assertRaisesRegex(ValueError, "CPU affinity"):
            validate_container_resources(3, MEMORY_LIMIT_BYTES, [2, 3], MEMORY_LIMIT_BYTES, 0)
        with self.assertRaisesRegex(ValueError, "memory limit"):
            validate_container_resources(3, MEMORY_LIMIT_BYTES, [3], MEMORY_LIMIT_BYTES * 2, 0)
        with self.assertRaisesRegex(ValueError, "swap limit"):
            validate_container_resources(3, MEMORY_LIMIT_BYTES, [3], MEMORY_LIMIT_BYTES, 1)

    def test_timeout_kills_children_before_they_can_write(self):
        script = self.root / "parent.py"
        script.write_text("import subprocess,sys,time\nsubprocess.Popen([sys.executable, '-c', \"import time,pathlib; time.sleep(0.4); pathlib.Path('leaked').write_text('child')\"])\ntime.sleep(5)\n")
        observation = invoke([sys.executable, str(script)], self.root, {}, 0.05)
        self.assertTrue(observation["timed_out"])
        subprocess.run([sys.executable, "-c", "import time; time.sleep(0.5)"], check=True)
        self.assertFalse((self.root / "leaked").exists())

    def test_first_useful_line_precedes_completion_and_drains_both_pipes(self):
        work = self.root / "work"
        work.mkdir()
        release = self.root / "release"
        os.mkfifo(release)
        readings = iter((100, 200, 300))
        def clock():
            value = next(readings)
            if value == 200:
                release.write_bytes(b"release")
            return value
        script = "import os,pathlib,sys; os.write(1,b're'); os.write(2,b'x'*200000); os.write(1,b'ady\\n'); pathlib.Path(sys.argv[1]).read_bytes(); os.write(1,b'done\\n')"
        observation = invoke([sys.executable, "-c", script, str(release)], work, {}, 5, measured=True, useful_prefix=b"ready\n", clock_ns=clock)
        self.assertTrue(observation["useful_stdout_observed"])
        self.assertEqual(observation["first_useful_stdout_ns"], 100)
        self.assertEqual(observation["wall_ns"], 200)
        self.assertEqual(base64.b64decode(observation["stdout_base64"]), b"ready\ndone\n")
        self.assertEqual(observation["stderr_bytes"], 200000)
        self.assertEqual(base64.b64decode(observation["stderr_base64"]), b"x" * 200000)
        self.assertFalse(observation["timed_out"])

    def test_no_output_early_failure_and_wrong_output_have_no_useful_line(self):
        for script, status in (("pass", 0), ("import sys; sys.stderr.write('failure'); sys.exit(7)", 7), ("print('wrong')", 0)):
            with self.subTest(script=script):
                readings = iter((100, 200))
                observation = invoke([sys.executable, "-c", script], self.root, {}, 5, measured=True, useful_prefix=b"ready\n", clock_ns=lambda: next(readings))
                self.assertEqual(observation["status"], status)
                self.assertIsNone(observation["first_useful_stdout_ns"])
                self.assertFalse(observation["useful_stdout_observed"])
                self.assertFalse(equivalent(observation, b"ready\n"))

    def test_parity_observes_useful_line_without_emitting_timestamps(self):
        observation = invoke([sys.executable, "-c", "print('ready')"], self.root, {}, 5, useful_prefix=b"ready\n")
        self.assertTrue(equivalent(observation, b"ready\n"))
        self.assertTrue(observation["useful_stdout_observed"])
        self.assertIsNone(observation["first_useful_stdout_ns"])
        self.assertIsNone(observation["wall_ns"])

    def test_exited_parent_with_descendant_holding_pipes_hits_deadline_and_cleans_group(self):
        script = "import subprocess,sys; subprocess.Popen([sys.executable,'-c','import signal; signal.pause()']); print('ready',flush=True)"
        observation = invoke([sys.executable, "-c", script], self.root, {}, 0.1, useful_prefix=b"ready\n")
        self.assertTrue(observation["timed_out"])
        self.assertEqual(observation["status"], 0)
        self.assertEqual(base64.b64decode(observation["stdout_base64"]), b"ready\n")
        self.assertFalse(equivalent(observation, b"ready\n"))

    def test_group_cleanup_keeps_completed_leader_unreaped_until_kill(self):
        real_killpg = os.killpg
        leaders = []
        def checked_killpg(pid, signum):
            observed = os.waitid(os.P_PID, pid, os.WEXITED | os.WNOWAIT | os.WNOHANG)
            self.assertIsNotNone(observed)
            self.assertEqual(observed.si_pid, pid)
            leaders.append(pid)
            return real_killpg(pid, signum)
        with patch("run.os.killpg", side_effect=checked_killpg):
            observation = invoke([sys.executable, "-c", "print('ready')"], self.root, {}, 5, useful_prefix=b"ready\n")
        self.assertTrue(equivalent(observation, b"ready\n"))
        self.assertEqual(len(leaders), 1)
        with self.assertRaises(ChildProcessError):
            os.waitid(os.P_PID, leaders[0], os.WEXITED | os.WNOWAIT | os.WNOHANG)

    def test_cancellation_during_spawn_tracks_then_reaps_the_owned_child(self):
        real_popen = subprocess.Popen
        children = []
        def spawn_then_cancel(*args, **kwargs):
            process = real_popen(*args, **kwargs)
            children.append(process)
            os.kill(os.getpid(), signal.SIGTERM)
            return process
        with patch("run.subprocess.Popen", side_effect=spawn_then_cancel):
            with self.assertRaises(SystemExit) as cancelled:
                invoke([sys.executable, "-c", "import signal; signal.pause()"], self.root, {}, 5)
        self.assertEqual(cancelled.exception.code, 143)
        self.assertEqual(len(children), 1)
        self.assertEqual(children[0].returncode, -signal.SIGKILL)
        with self.assertRaises(ChildProcessError):
            os.waitid(os.P_PID, children[0].pid, os.WEXITED | os.WNOWAIT | os.WNOHANG)

    def test_termination_signal_cleans_and_reaps_the_owned_child(self):
        work = self.root / "work"
        work.mkdir()
        ready = self.root / "ready"
        os.mkfifo(ready)
        pid_path = self.root / "child.pid"
        child = "import os,pathlib,signal,sys; pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); print('ready',flush=True); signal.pause()"
        wrapper = (
            "import pathlib,sys\n"
            "sys.path.insert(0,sys.argv[1])\n"
            "from run import invoke\n"
            "readings=iter((100,200))\n"
            "def clock():\n"
            "    value=next(readings)\n"
            "    if value==200: pathlib.Path(sys.argv[3]).write_bytes(b'ready')\n"
            "    return value\n"
            "invoke([sys.executable,'-c',sys.argv[4],sys.argv[5]],pathlib.Path(sys.argv[2]),{},30,measured=True,useful_prefix=b'ready\\n',clock_ns=clock)"
        )
        process = subprocess.Popen([sys.executable, "-c", wrapper, str(HERE), str(work), str(ready), child, str(pid_path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            self.assertEqual(ready.read_bytes(), b"ready")
            process.send_signal(signal.SIGTERM)
            stdout, stderr = process.communicate(timeout=5)
            self.assertEqual(process.returncode, 143)
            self.assertEqual(stdout, b"")
            self.assertEqual(stderr, b"")
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_path.read_text()), 0)
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate()
            process.stdout.close()
            process.stderr.close()
            if pid_path.exists():
                try:
                    os.kill(int(pid_path.read_text()), signal.SIGKILL)
                except ProcessLookupError:
                    pass

    def test_escaped_descendant_holding_pipe_rejects_unresolved_cleanup(self):
        work = self.root / "work"
        work.mkdir()
        pid_path = self.root / "escaped.pid"
        child = "import os,pathlib,signal,sys; os.setsid(); pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); signal.pause()"
        parent = "import subprocess,sys,signal; subprocess.Popen([sys.executable,'-c',sys.argv[1],sys.argv[2]]); signal.pause()"
        try:
            with self.assertRaisesRegex(RuntimeError, "cleanup unresolved"):
                invoke([sys.executable, "-c", parent, child, str(pid_path)], work, {}, 0.2)
        finally:
            if pid_path.exists():
                os.kill(int(pid_path.read_text()), signal.SIGKILL)

    def test_overlay_does_not_build_if_base_alias_resolves_to_another_image(self):
        source = self.root / "suite"
        source.mkdir()
        base = "sha256:" + "a" * 64
        (source / "versions.lock.json").write_text(json.dumps({"base_images": {"amd64": base}, "sources": {}}))
        (source / "Dockerfile").write_text("ARG BASE_REFERENCE\nFROM ${BASE_REFERENCE}\n")
        args = argparse.Namespace(destination=self.root / "context", base_image=None, arch="amd64", build=True, tag="xsh-comparative:test")
        inspect = json.dumps([{"Architecture": "amd64", "Id": base}]).encode()
        with patch("prepare.HERE", source), patch("prepare.subprocess.run") as run, patch("prepare.subprocess.check_output", side_effect=[inspect, "sha256:" + "b" * 64]):
            with self.assertRaisesRegex(ValueError, "base alias differs"):
                prepare(args)
        self.assertFalse(any(call.args[0][1] == "build" for call in run.call_args_list))

    def test_overlay_build_uses_an_alias_and_records_the_immutable_parent(self):
        source = self.root / "suite"
        source.mkdir()
        base = "sha256:" + "a" * 64
        (source / "versions.lock.json").write_text(json.dumps({"base_images": {"amd64": base}, "sources": {}}))
        (source / "Dockerfile").write_text("ARG BASE_REFERENCE\nFROM ${BASE_REFERENCE}\n")
        args = argparse.Namespace(destination=self.root / "context", base_image=None, arch="amd64", build=True, tag="xsh-comparative:test")
        inspect = json.dumps([{"Architecture": "amd64", "Id": base}]).encode()
        with patch("prepare.HERE", source), patch("prepare.subprocess.run") as run, patch("prepare.subprocess.check_output", side_effect=[inspect, base, "sha256:" + "c" * 64]), redirect_stdout(io.StringIO()):
            prepare(args)
        build = next(call.args[0] for call in run.call_args_list if call.args[0][1] == "build")
        self.assertIn("BASE_REFERENCE=xsh-comparative-base:amd64-" + "a" * 64, build)
        self.assertIn("BASE_IMAGE=" + base, build)
        self.assertIn("cpu-quota=400000", build)
        self.assertIn("memory=8g", build)
        self.assertEqual(json.loads((args.destination / "preparation.json").read_text())["base_image"], base)
        self.assertEqual(json.loads((args.destination / "preparation.json").read_text())["build_resources"], {"cpu_quota": 400_000, "memory_bytes": 8 * 1024**3})


if __name__ == "__main__":
    unittest.main()
