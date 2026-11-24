"""Time the core command workloads against their correctness oracles.

Each round runs every selected case once, alternating the order between
rounds. A run counts only when it exits 0 with the exact expected stdout.
"""

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import resource
import statistics
import subprocess
import tempfile
import time

from dev.performance.core_cases import prepare_case


ROOT = Path(__file__).resolve().parents[2]
FAMILIES = ('wc', 'sort', 'awk', 'traversal', 'json')
SHAPES = ('small', 'medium', 'large')
CASES = tuple(f'core-{family}-{shape}' for family in FAMILIES for shape in SHAPES)
PERF = Path('/usr/bin/perf')


@dataclass(frozen=True)
class Sample:
    wall: float
    cpu: float
    cycles: int | None


def describe(values):
    return {'median': statistics.median(values), 'min': min(values), 'max': max(values)}


def evaluate(samples):
    if not samples:
        raise ValueError('no cases measured')
    counts = {len(rows) for rows in samples.values()}
    if len(counts) != 1 or 0 in counts:
        raise ValueError('every case needs the same nonzero sample count')
    cycles = [row.cycles for rows in samples.values() for row in rows]
    if any(value is None for value in cycles) and not all(value is None for value in cycles):
        raise ValueError('incomplete cycle measurements')
    results = {}
    for name, rows in samples.items():
        result = {'wall': describe([row.wall for row in rows]), 'cpu': describe([row.cpu for row in rows])}
        if cycles[0] is not None:
            result['cycles'] = describe([row.cycles for row in rows])
        results[name] = result
    return {'samples': counts.pop(), 'cycles': cycles[0] is not None, 'cases': results}


def parse_cycles(stderr):
    rows = [line.split(',') for line in stderr.splitlines() if ',cycles:u,' in line]
    if len(rows) != 1 or not rows[0][0].isdigit():
        raise RuntimeError(f'perf did not report cycles:u: {stderr}')
    return int(rows[0][0])


def measure(xsh, workload, use_perf):
    argv = workload.argv(xsh)
    if use_perf:
        argv = [str(PERF), 'stat', '-x,', '-e', 'cycles:u', '--', *argv]
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.monotonic()
    done = subprocess.run(argv, cwd=ROOT, capture_output=True)
    wall = time.monotonic() - started
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    if done.returncode != 0 or done.stdout != workload.expected_stdout:
        raise ValueError(f'{workload.name}: exit {done.returncode} or stdout differs from the oracle')
    cpu = after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime
    return Sample(wall, cpu, parse_cycles(done.stderr.decode()) if use_perf else None)


def run(xsh, cases, repeats, use_perf):
    scratch = ROOT / '.work/tmp'
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch, prefix='core-bench-') as temp:
        fixtures = Path(temp).resolve()
        workloads = {name: prepare_case(name, fixtures=fixtures) for name in cases}
        for workload in workloads.values():
            measure(xsh, workload, False)
        samples = {name: [] for name in cases}
        for index in range(repeats):
            for name in (cases if index % 2 == 0 else tuple(reversed(cases))):
                samples[name].append(measure(xsh, workloads[name], use_perf))
    return evaluate(samples)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--xsh', required=True, type=Path)
    parser.add_argument('--case', action='append', choices=CASES)
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--json', type=Path)
    args = parser.parse_args(argv)
    if args.repeats < 1:
        parser.error('--repeats must be positive')
    cases = tuple(args.case or CASES)
    result = run(str(args.xsh.resolve()), cases, args.repeats, PERF.is_file())
    print(f"{result['samples']} interleaved rounds; every run matched its oracle")
    for name, row in result['cases'].items():
        wall, cpu = row['wall'], row['cpu']
        line = (f"{name}: wall {wall['median']:.4f} [{wall['min']:.4f}, {wall['max']:.4f}] s; "
                f"CPU {cpu['median']:.4f} [{cpu['min']:.4f}, {cpu['max']:.4f}] s")
        if 'cycles' in row:
            cycles = row['cycles']
            line += f"; cycles:u {cycles['median']:.4g} [{cycles['min']:.4g}, {cycles['max']:.4g}]"
        print(line)
    if args.json:
        args.json.write_text(json.dumps(result, indent=2) + '\n')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
