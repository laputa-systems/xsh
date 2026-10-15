from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "final-performance-gates"))
compare_path = Path(__file__).with_name("compare.py")
spec = spec_from_file_location("scheduler_compare", compare_path)
compare = module_from_spec(spec)
spec.loader.exec_module(compare)


def snapshot(*, quota="3200000 100000", cpuset="0-31", memory_max=40 * 1024**3,
             swap_current=0, swap_max=0, oom=0, oom_kill=0,
             pids_current=4, pids_peak=7, pids_max="max"):
    return f"""[cpu.max]
{quota}
[cpuset.cpus.effective]
{cpuset}
[memory.max]
{memory_max}
[memory.current]
1048576
[memory.peak]
2097152
[memory.swap.current]
{swap_current}
[memory.swap.max]
{swap_max}
[memory.events]
low 0
high 0
max 0
oom {oom}
oom_kill {oom_kill}
oom_group_kill 0
[pids.current]
{pids_current}
[pids.peak]
{pids_peak}
[pids.max]
{pids_max}
"""


good = snapshot()
result = compare.validate_cgroup_snapshots(good, good)
assert result["contract_valid"] is True
assert result["after"]["cpuset_cpus"] == list(range(32))
assert result["after"]["memory_swap_max_bytes"] == 0
assert result["after"]["pids_max_limit"] is None
assert [compare.suite_execution_order(index) for index in range(compare.SUITE_ROUNDS)] == [
    ("reference", "candidate"),
    ("candidate", "reference"),
    ("reference", "candidate"),
]
assert compare.summarize_suite_samples([3.0, 2.0, 2.5]) == {
    "samples": [3.0, 2.0, 2.5],
    "minimum": 2.0,
    "maximum": 3.0,
    "spread": 1.0,
}


def rejected(value, message):
    try:
        compare.validate_cgroup_snapshots(value, value)
    except ValueError as error:
        assert message in str(error), str(error)
    else:
        raise AssertionError(f"snapshot should be rejected for {message}")


rejected(snapshot(quota="1600000 100000"), "CPU quota")
rejected(snapshot(cpuset="0-30"), "cpuset")
rejected(snapshot(memory_max=39 * 1024**3), "memory.max")
rejected(snapshot(swap_current=1), "swap")
rejected(snapshot(swap_max=1), "swap")
rejected(snapshot(oom=1), "OOM")
rejected(snapshot(oom_kill=1), "OOM")
rejected(snapshot(pids_current=8, pids_peak=7), "pids current")
rejected(snapshot(pids_max=6), "pids peak")
assert compare.validate_cgroup_snapshots(
    snapshot(pids_max=8), snapshot(pids_max=8)
)["after"]["pids_max_limit"] == 8
print("scheduler cgroup CPU, memory, swap, and OOM checks passed")
