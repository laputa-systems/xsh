"""Compare the frozen workloads' output between the reference and the candidate.

`run.py` times each workload and discards its output, so it cannot say whether a
workload that fails its budget *behaves* the same. This harness answers that
question for every workload the runner declares: same script, same working
directory, both binaries, with exit status, stdout, and stderr compared byte for
byte.

```sh
python3 bench/stdlib-port/parity.py \
  --reference /tmp/xsh-reference/target/release/xsh \
  --candidate "$PWD/target/release/xsh"
```

It exits non-zero when any workload differs and prints the first differing bytes
of each side. Binary paths must be absolute for the same reason they must be in
`run.py`: every workload runs from this directory.
"""

import argparse
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


# Workloads that print a duration they measured themselves. Their stdout is a
# timing, so it is compared with the value masked: everything else about the
# line still has to match byte for byte.
SELF_TIMED = {
    "linux_uptime",
    "linux_meminfo",
    "linux_memory",
    "linux_modules_full",
    "linux_modules_partial",
    "linux_os_release",
}
SELF_TIMED_FIELD = re.compile(rb"\b\d+ ms\b")


def mask_self_timed(name, run_result):
    if name not in SELF_TIMED:
        return run_result
    status, stdout, stderr = run_result
    return status, SELF_TIMED_FIELD.sub(b"<ms>", stdout), stderr


def workload_names():
    """The workload names `run.py` declares, in declaration order."""
    source = open(os.path.join(HERE, "run.py")).read()
    return re.findall(r'\("([a-z_0-9]+)",\s*"', source)


def run(binary, script):
    completed = subprocess.run(
        [binary, script],
        cwd=HERE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return completed.returncode, completed.stdout, completed.stderr


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True)
    parser.add_argument("--candidate", required=True)
    args = parser.parse_args()

    names = workload_names()
    if not names:
        print("no workloads found in run.py", file=sys.stderr)
        return 2

    differing = []
    for name in names:
        script = os.path.join(HERE, name + ".xsh")
        if not os.path.isfile(script):
            print(f"{name:22} missing script")
            differing.append((name, None, None))
            continue
        reference = mask_self_timed(name, run(args.reference, script))
        candidate = mask_self_timed(name, run(args.candidate, script))
        same = reference == candidate
        print(f"{name:22} {'identical' if same else 'DIFFERS'}")
        if not same:
            differing.append((name, reference, candidate))

    print()
    if not differing:
        print(f"all {len(names)} workloads byte-identical")
        return 0

    for name, reference, candidate in differing:
        print(f"--- {name}")
        if reference is None:
            continue
        print(f"    exit:        {reference[0]} vs {candidate[0]}")
        print(f"    ref stdout:  {reference[1][:300]!r}")
        print(f"    cand stdout: {candidate[1][:300]!r}")
        print(f"    ref stderr:  {reference[2][:300]!r}")
        print(f"    cand stderr: {candidate[2][:300]!r}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
