from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "final-performance-gates")]
spec = spec_from_file_location("foreign_cpu_compare", HERE / "compare.py")
compare = module_from_spec(spec)
spec.loader.exec_module(compare)

TICKS = 100


def cgroup_text(usage_usec, nr_throttled=0):
    return f"""[/sys/fs/cgroup/cpu.stat]
usage_usec {usage_usec}
user_usec 1
system_usec 1
nr_periods 10
nr_throttled {nr_throttled}
throttled_usec 0
[/sys/fs/cgroup/cpu.max]
3200000 100000
"""


def host_sample(busy_ticks, seconds):
    return {"monotonic_ns": int(seconds * 1e9), "busy_ticks": busy_ticks}


def verdict(host_busy_ticks, container_usec, wall=100, throttled=0, idle=0.0):
    return compare.foreign_cpu_verdict(
        host_before=host_sample(1000, 0),
        host_after=host_sample(1000 + host_busy_ticks, wall),
        cgroup_before_text=cgroup_text(500_000),
        cgroup_after_text=cgroup_text(500_000 + container_usec, throttled),
        clock_ticks_per_second=TICKS,
        idle_host_cores=idle,
    )


class ForeignCpuTests(unittest.TestCase):
    def test_own_load_is_not_foreign(self):
        result = verdict(host_busy_ticks=320_000 + 2_000, container_usec=3_200_000_000)
        self.assertEqual(result["exit_code"], 0)
        self.assertAlmostEqual(result["foreign_cpu_seconds"], 20.0)
        self.assertAlmostEqual(result["foreign_cpu_cores"], 0.2)
        self.assertAlmostEqual(result["container_cpu_seconds"], 3200.0)

    def test_foreign_load_above_half_a_core_is_rejected(self):
        result = verdict(host_busy_ticks=320_000 + 6_000, container_usec=3_200_000_000)
        self.assertEqual(result["exit_code"], 1)
        self.assertFalse(result["accepted"])
        self.assertAlmostEqual(result["foreign_cpu_cores"], 0.6)

    def test_the_host_idle_level_is_not_foreign(self):
        result = verdict(host_busy_ticks=320_000 + 14_000, container_usec=3_200_000_000, idle=1.0)
        self.assertEqual(result["exit_code"], 0)
        self.assertAlmostEqual(result["max_foreign_cpu_cores"], 1.5)
        result = verdict(host_busy_ticks=320_000 + 16_000, container_usec=3_200_000_000, idle=1.0)
        self.assertEqual(result["exit_code"], 1)

    def test_throttling_is_rejected_even_without_foreign_load(self):
        result = verdict(host_busy_ticks=320_000, container_usec=3_200_000_000, throttled=1)
        self.assertEqual(result["exit_code"], 1)
        self.assertEqual(result["nr_throttled_delta"], 1)

    def test_missing_cpu_stat_counter_fails_loudly(self):
        with self.assertRaises(ValueError):
            compare.parse_cgroup_cpu_counters("[/sys/fs/cgroup/cpu.max]\n3200000 100000\n")
        with self.assertRaises(ValueError):
            compare.parse_cgroup_cpu_counters("[/sys/fs/cgroup/cpu.stat]\nusage_usec 5\n")

    def test_host_cpu_line_sums_busy_fields_only(self):
        text = "cpu  10 20 30 400 50 6 7 8 9 10\ncpu0 1 1 1 1 1 1 1 1 1 1\n"
        self.assertEqual(compare.parse_host_cpu_busy_ticks(text), 10 + 20 + 30 + 6 + 7 + 8)
        with self.assertRaises(ValueError):
            compare.parse_host_cpu_busy_ticks("cpu0 1 2 3 4 5 6 7 8\n")
        with self.assertRaises(ValueError):
            compare.parse_host_cpu_busy_ticks("cpu  1 2 x 4 5 6 7 8\n")

    def test_missing_snapshot_discards_the_timing(self):
        with tempfile.TemporaryDirectory(dir=compare.ROOT / ".work/tmp") as directory:
            before = Path(directory) / "before.txt"
            after = Path(directory) / "after.txt"
            before.write_text(cgroup_text(1))
            after.write_text(cgroup_text(2))
            good = host_sample(1, 1)
            for host_before, host_after in ((None, good), (good, None)):
                result = compare.foreign_cpu_check(
                    host_before=host_before, host_after=host_after,
                    cgroup_before_path=before, cgroup_after_path=after, idle_host_cores=0.0,
                )
                self.assertNotEqual(result["exit_code"], 0)
                self.assertIn("missing", result["error"])
            after.unlink()
            result = compare.foreign_cpu_check(
                host_before=good, host_after=host_sample(2, 2),
                cgroup_before_path=before, cgroup_after_path=after, idle_host_cores=0.0,
            )
            self.assertNotEqual(result["exit_code"], 0)
            self.assertIn("error", result)
            before.write_text(cgroup_text(1))
            after.write_text(cgroup_text(2))
            result = compare.foreign_cpu_check(
                host_before=good, host_after=host_sample(2, 2),
                cgroup_before_path=before, cgroup_after_path=after, idle_host_cores=None,
            )
            self.assertEqual(result["exit_code"], 2)
            self.assertIn("idle", result["error"])


if __name__ == "__main__":
    unittest.main()
