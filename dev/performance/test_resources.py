from dataclasses import replace
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from dev.performance import resources


ROOT = Path(__file__).resolve().parents[2]
SCRATCH = ROOT / ".work/tmp"


class ResourceWindowTests(unittest.TestCase):
    def setUp(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-window-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.cgroup = self.root / "cgroup"
        self.group = self.cgroup / "case"
        self.proc = self.root / "proc"
        self.cgroup.mkdir()
        self.group.mkdir()
        (self.proc / "self/ns").mkdir(parents=True)
        (self.proc / "self/ns/cgroup").write_text("")
        (self.proc / "self/cgroup").write_text("0::/case\n")
        (self.proc / "self/mountinfo").write_text(f"42 1 0:42 / {self.cgroup} rw - cgroup2 cgroup2 rw\n")
        (self.proc / "self/status").write_text("VmRSS:\t8 kB\nVmHWM:\t12 kB\n")
        values = {
            "memory.peak": "65536", "memory.current": "32768", "memory.max": "4194304",
            "cgroup.pressure": "1",
            "memory.stat": "anon 16384\nfile 8192\nkernel 8192\n",
            "memory.events": "oom 0\noom_kill 0\noom_group_kill 0\n",
            "cpu.stat": "usage_usec 100\nuser_usec 60\nsystem_usec 40\nnr_periods 3\nnr_throttled 1\nthrottled_usec 7\n",
            "cpu.max": "200000 100000", "cpuset.cpus.effective": "0-3",
            "pids.current": "2", "pids.peak": "4",
        }
        values.update({f"{name}.pressure": "some avg10=0.00 avg60=0.00 avg300=0.00 total=11\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=4\n" for name in ("cpu", "memory", "io")})
        for name, value in values.items():
            (self.group / name).write_text(value)
        (self.cgroup / "memory.peak").write_text("999999")
        self.before = resources._capture(self.proc)

    def window(self, after=None, scope=resources.GroupScope.FRESH_CASE_CONTAINER):
        return resources.validate_case_window(self.before, after or self.before, declared_workers={"driver": 1, "workers": 2}, scope=scope)

    def test_window_distinguishes_container_peak_observer_rss_and_counter_deltas(self):
        after = replace(self.before, memory_peak_bytes=131072,
                        cpu_stat={**self.before.cpu_stat, "usage_usec": 175, "throttled_usec": 12, "nr_throttled": 3},
                        memory_events={"oom": 1, "oom_kill": 1, "oom_group_kill": 0},
                        pressure={name: {"some": 18, "full": 6} for name in ("cpu", "memory", "io")})
        report = self.window(after)
        self.assertEqual(report.memory_peak_sample_bytes, 131072)
        self.assertEqual(report.observer_peak_rss_bytes, 12 * 1024)
        self.assertEqual(report.cpu_usage_ns, 75000)
        self.assertEqual(report.cpu_throttled_ns, 5000)
        self.assertEqual(report.cpu_throttled_periods, 2)
        self.assertEqual(report.pressure_stall_us["memory"], {"some": 7, "full": 2})
        self.assertEqual(report.memory_events_delta["oom_kill"], 1)
        self.assertEqual(report.violations, ("memory.events oom increased by 1", "memory.events oom_kill increased by 1"))
        self.assertEqual(report.observed_cgroup_tasks_peak, 4)
        self.assertEqual(report.observed_cgroup_tasks_current, 2)
        self.assertEqual(report.declared_workers, (("driver", 1), ("workers", 2)))
        self.assertTrue(report.telemetry_complete)

    def test_missing_and_malformed_fields_never_become_zero_or_complete(self):
        (self.group / "memory.peak").unlink()
        (self.group / "cpu.pressure").unlink()
        (self.group / "cpu.stat").write_text("usage_usec 1\nusage_usec 2\n")
        (self.group / "memory.events").write_text("oom -1\n")
        after = resources._capture(self.proc)
        report = self.window(after)
        self.assertFalse(report.telemetry_complete)
        self.assertEqual(report.memory_peak_sample_bytes.availability, resources.Availability.UNSUPPORTED)
        self.assertEqual(report.cpu_usage_ns.availability, resources.Availability.INCOMPLETE)
        self.assertEqual(report.memory_events_delta.availability, resources.Availability.INCOMPLETE)
        self.assertEqual(report.pressure_stall_us["cpu"].availability, resources.Availability.UNSUPPORTED)

    def test_counter_names_must_be_ascii_whitespace_free_tokens(self):
        for counter_file in ("anón 1\n", "bad key 1\n"):
            with self.subTest(counter_file=counter_file):
                (self.group / "memory.stat").write_text(counter_file)
                after = resources._capture(self.proc)
                self.assertIsInstance(after.memory_stat, resources.Unavailable)
                self.assertEqual(after.memory_stat.availability, resources.Availability.INCOMPLETE)
                self.assertFalse(self.window(after).telemetry_complete)

    def test_disabled_or_missing_cgroup_pressure_never_counts_zero_stalls(self):
        (self.group / "cgroup.pressure").write_text("0")
        disabled = resources._capture(self.proc)
        self.assertIs(disabled.pressure_accounting_enabled, False)
        self.assertIsInstance(disabled.pressure["cpu"], resources.Unavailable)
        self.assertEqual(disabled.pressure["cpu"].availability, resources.Availability.UNSUPPORTED)
        disabled_report = self.window(disabled)
        self.assertIsInstance(disabled_report.pressure_accounting_enabled, resources.Unavailable)
        self.assertEqual(disabled_report.pressure_accounting_enabled.availability, resources.Availability.INCOMPLETE)
        self.assertFalse(disabled_report.telemetry_complete)
        stable_disabled = resources.validate_case_window(
            disabled,
            disabled,
            declared_workers={"driver": 1, "workers": 2},
            scope=resources.GroupScope.FRESH_CASE_CONTAINER,
        )
        self.assertIsInstance(stable_disabled.pressure_accounting_enabled, resources.Unavailable)
        self.assertEqual(stable_disabled.pressure_accounting_enabled.availability, resources.Availability.UNSUPPORTED)
        self.assertFalse(stable_disabled.telemetry_complete)

        (self.group / "cgroup.pressure").unlink()
        missing = resources._capture(self.proc)
        self.assertIsInstance(missing.pressure_accounting_enabled, resources.Unavailable)
        self.assertEqual(missing.pressure_accounting_enabled.availability, resources.Availability.UNSUPPORTED)
        self.assertIsInstance(missing.pressure["memory"], resources.Unavailable)
        self.assertFalse(self.window(missing).telemetry_complete)

    def test_psi_accounting_change_during_capture_invalidates_its_totals(self):
        with patch.object(resources, "_toggle", side_effect=(True, False)):
            snapshot = resources._capture(self.proc)
        self.assertIsInstance(snapshot.pressure_accounting_enabled, resources.Unavailable)
        self.assertEqual(snapshot.pressure_accounting_enabled.availability, resources.Availability.INCOMPLETE)
        self.assertTrue(all(isinstance(value, resources.Unavailable) for value in snapshot.pressure.values()))

    def test_partial_psi_rows_are_incomplete(self):
        (self.group / "cpu.pressure").write_text("some avg10=0.00 avg60=0.00 avg300=0.00 total=11\n")
        after = resources._capture(self.proc)
        self.assertIsInstance(after.pressure["cpu"], resources.Unavailable)
        self.assertEqual(after.pressure["cpu"].availability, resources.Availability.INCOMPLETE)
        report = self.window(after)
        self.assertFalse(report.telemetry_complete)
        self.assertEqual(report.pressure_stall_us["cpu"].availability, resources.Availability.INCOMPLETE)

    def test_shared_group_peak_sample_does_not_claim_case_attribution(self):
        report = self.window(scope=resources.GroupScope.SHARED_CONTAINER)
        self.assertEqual(report.memory_peak_sample_bytes, 65536)
        self.assertEqual(report.scope, resources.GroupScope.SHARED_CONTAINER)
        self.assertFalse(report.telemetry_complete)

    def test_mountinfo_maps_nested_membership_to_its_visible_subtree(self):
        (self.proc / "self/cgroup").write_text("0::/tenant/case\n")
        (self.proc / "self/mountinfo").write_text(f"42 1 0:42 /tenant {self.cgroup} rw - cgroup2 cgroup2 rw\n")
        snapshot = resources._capture(self.proc)
        self.assertEqual(snapshot.identity.membership, "/tenant/case")
        self.assertEqual(snapshot.memory_peak_bytes, 65536)

    def test_memory_localevents_prevents_subtree_event_attribution(self):
        (self.proc / "self/mountinfo").write_text(
            f"42 1 0:42 / {self.cgroup} rw - cgroup2 cgroup2 rw,memory_localevents\n"
        )
        local_events = resources._capture(self.proc)
        self.assertIs(local_events.memory_events_hierarchical, False)
        self.assertIsInstance(local_events.memory_events, resources.Unavailable)
        self.assertEqual(local_events.memory_events.availability, resources.Availability.INCOMPLETE)
        stable_report = resources.validate_case_window(
            local_events,
            local_events,
            declared_workers={"driver": 1, "workers": 2},
            scope=resources.GroupScope.FRESH_CASE_CONTAINER,
        )
        self.assertIs(stable_report.memory_events_hierarchical, False)
        self.assertIsInstance(stable_report.memory_events_delta, resources.Unavailable)
        self.assertFalse(stable_report.telemetry_complete)
        changed_report = self.window(local_events)
        self.assertIsInstance(changed_report.memory_events_hierarchical, resources.Unavailable)
        self.assertEqual(changed_report.memory_events_hierarchical.availability, resources.Availability.INCOMPLETE)
        self.assertIsInstance(changed_report.memory_events_delta, resources.Unavailable)

    def test_membership_change_at_snapshot_end_invalidates_identity(self):
        with patch.object(resources, "_unified_membership", side_effect=("/case", "/other")):
            snapshot = resources._capture(self.proc)
        self.assertIsInstance(snapshot.identity, resources.Unavailable)
        self.assertEqual(snapshot.identity.availability, resources.Availability.INCOMPLETE)
        self.assertIn("membership changed", snapshot.identity.reason)

    def test_mountinfo_unescapes_mountpoint_spaces(self):
        mountpoint = self.root / "cgroup mount"
        group = mountpoint / "case"
        group.mkdir(parents=True)
        (group / "memory.peak").write_text("123456")
        encoded_mountpoint = str(mountpoint).replace(" ", "\\040")
        (self.proc / "self/mountinfo").write_text(f"43 1 0:43 / {encoded_mountpoint} rw - cgroup2 cgroup2 rw\n")
        snapshot = resources._capture(self.proc)
        self.assertEqual(snapshot.memory_peak_bytes, 123456)

    def test_ambiguous_mounts_do_not_choose_a_cgroup_by_list_order(self):
        mountpoint = self.root / "other cgroup"
        group = mountpoint / "case"
        group.mkdir(parents=True)
        (group / "memory.peak").write_text("123456")
        first_mount = str(self.cgroup).replace(" ", "\\040")
        second_mount = str(mountpoint).replace(" ", "\\040")
        (self.proc / "self/mountinfo").write_text(
            f"42 1 0:42 / {first_mount} rw - cgroup2 cgroup2 rw\n"
            f"43 1 0:43 / {second_mount} rw - cgroup2 cgroup2 rw\n"
        )
        snapshot = resources._capture(self.proc)
        self.assertIsInstance(snapshot.identity, resources.Unavailable)
        self.assertIn("ambiguous visible mounts", snapshot.identity.reason)
        self.assertIsInstance(snapshot.memory_peak_bytes, resources.Unavailable)

    def test_unmapped_membership_never_falls_back_to_mount_root_counters(self):
        (self.proc / "self/cgroup").write_text("0::/outside/case\n")
        (self.proc / "self/mountinfo").write_text(f"42 1 0:42 /tenant {self.cgroup} rw - cgroup2 cgroup2 rw\n")
        snapshot = resources._capture(self.proc)
        self.assertIsInstance(snapshot.identity, resources.Unavailable)
        self.assertIn("outside visible cgroup2 mounts", snapshot.identity.reason)
        self.assertIsInstance(snapshot.memory_peak_bytes, resources.Unavailable)
        self.assertEqual(snapshot.memory_peak_bytes, snapshot.identity)

    def test_absent_cgroup2_mount_is_unsupported(self):
        (self.proc / "self/mountinfo").write_text("24 1 0:1 / /proc rw - proc proc rw\n")
        snapshot = resources._capture(self.proc)
        self.assertIsInstance(snapshot.identity, resources.Unavailable)
        self.assertEqual(snapshot.identity.availability, resources.Availability.UNSUPPORTED)
        self.assertIsInstance(snapshot.memory_peak_bytes, resources.Unavailable)

    def test_identity_and_cumulative_counter_changes_are_incomplete(self):
        for after in (
            replace(self.before, identity=replace(self.before.identity, inode=self.before.identity.inode + 1)),
            replace(self.before, cpu_stat={**self.before.cpu_stat, "usage_usec": 1}),
            replace(self.before, memory_peak_bytes=1),
            replace(self.before, observer_pid=self.before.observer_pid + 1),
            replace(self.before, cpu_max="100000 100000"),
            replace(self.before, memory_max="2097152"),
            replace(self.before, cpuset_cpus_effective="0-1"),
        ):
            with self.assertRaises(resources.ResourceWindowError):
                self.window(after)

    def test_limits_and_required_fields_retain_unsupported_or_incomplete_status(self):
        for name, value in (("memory.max", "unknown"), ("cpu.max", "0 100000"), ("cpuset.cpus.effective", "4-1")):
            (self.group / name).write_text(value)
        report = self.window(resources._capture(self.proc))
        self.assertFalse(report.telemetry_complete)
        after = replace(self.before, cpu_stat={"usage_usec": 100})
        report = self.window(after)
        self.assertIsInstance(report.cpu_usage_ns, resources.Unavailable)
        self.assertFalse(report.telemetry_complete)

    def test_missing_oom_group_counter_cannot_be_reported_as_complete(self):
        (self.group / "memory.events").write_text("oom 0\noom_kill 0\n")
        report = self.window(resources._capture(self.proc))
        self.assertFalse(report.telemetry_complete)
        self.assertIsInstance(report.memory_events_delta, resources.Unavailable)

    def test_missing_pressure_category_is_incomplete(self):
        after = replace(self.before, pressure={name: value for name, value in self.before.pressure.items() if name != "io"})
        report = self.window(after)
        self.assertFalse(report.telemetry_complete)
        self.assertIsInstance(report.pressure_stall_us["io"], resources.Unavailable)

    def test_controller_scope_and_worker_counts_are_required(self):
        for workers in ({}, {"worker": 0}, {"worker": True}, {"worker": -1}, {"": 1}):
            with self.assertRaises(resources.ResourceWindowError):
                resources.validate_case_window(self.before, self.before, declared_workers=workers, scope=resources.GroupScope.FRESH_CASE_CONTAINER)
        with self.assertRaises(resources.ResourceWindowError):
            self.window(scope="fresh")
        valid = resources.validate_case_window(self.before, self.before, declared_workers={"driver": 1, "workers": 0}, scope=resources.GroupScope.FRESH_CASE_CONTAINER)
        self.assertEqual(valid.declared_workers, (("driver", 1), ("workers", 0)))


if __name__ == "__main__":
    unittest.main()
