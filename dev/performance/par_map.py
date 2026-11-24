"""Measure observable par-map setup, capture, and nested-worker costs."""

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import resource
import statistics
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'dev/performance/par_map'
MAIN = ROOT.parents[2] if ROOT.parent.name == 'wt' else ROOT
PAIRS = (('setup-5', 'setup-five-one', 'setup-five'),
         ('setup-20', 'setup-one', 'setup-many'),
         ('capture', 'capture-small', 'capture-large'),
         ('nested-2', 'flat-four', 'nested-two'),
         ('nested-4', 'flat', 'nested'))


@dataclass(frozen=True)
class Sample:
    output: str
    wall: float
    cpu: float
    report: dict


def stage_totals(report):
    stages = report['stages']
    return {'workers': sum(s['workers'] for s in stages),
            'items': sum(s['items'] for s in stages),
            'setup_ns': sum(w['setup_ns'] for s in stages for w in s['worker']),
            'start_delay_ns': sum(w['start_delay_ns'] for s in stages for w in s['worker']),
            'busy_ns': sum(w['busy_ns'] for s in stages for w in s['worker']),
            'buffered_results': sum(s['buffered_results'] for s in stages),
            'site_alloc_bytes': sum(s['alloc_bytes'] for s in report['sites'])}


def verdict(name, a, b):
    if name.startswith('nested'):
        return 'inconclusive: extra workers observed but concurrent oversubscription is unmeasured'
    if name.startswith('setup'):
        return 'confirmed: repeated worker creation' if b['workers'] > a['workers'] else 'refuted: no extra workers'
    return 'inconclusive: parent cloning and per-worker allocation bytes are unobserved'


def evaluate(name, a, b):
    if not a or len(a) != len(b):
        raise ValueError('expected equal nonempty samples')
    if len({s.output for s in a + b}) != 1:
        raise ValueError(f'{name}: output mismatch')
    if any(not s.report['complete'] or not s.report['stages'] for s in a + b):
        raise ValueError(f'{name}: incomplete or absent stage profile')
    def aggregate(samples):
        totals = [stage_totals(s.report) for s in samples]
        return {key: statistics.median(row[key] for row in totals) for key in totals[0]}
    left, right = aggregate(a), aggregate(b)
    return {'name': name, 'output': a[0].output.strip(), 'a': left, 'b': right,
            'cpu_ratios': [y.cpu / x.cpu for x, y in zip(a, b)],
            'wall_ratios': [y.wall / x.wall for x, y in zip(a, b)],
            'verdict': verdict(name, left, right)}


def command(argv):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.monotonic()
    done = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True)
    wall = time.monotonic() - start
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    if done.returncode:
        raise RuntimeError(f'{argv}: exit {done.returncode}: {done.stderr}')
    return done.stdout, wall, after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime


def measure(xsh, xsht, fixture, report_path):
    output, wall, cpu = command([xsh, str(fixture)])
    profiled, _, _ = command([xsht, 'profile', '--json', str(report_path), str(fixture)])
    if profiled.split('profile: complete:', 1)[0] != output:
        raise ValueError(f'{fixture}: profile output differs')
    return Sample(output, wall, cpu, json.loads(report_path.read_text()))


def run(xsh, xsht, repeats):
    scratch = ROOT / '.work/tmp'
    scratch.mkdir(parents=True, exist_ok=True)
    rows = []
    with tempfile.TemporaryDirectory(dir=scratch, prefix='par-map-') as temp:
        report = Path(temp) / 'profile.json'
        for name, first, second in PAIRS:
            samples = [[], []]
            for index in range(repeats):
                for side in ((0, 1) if index % 2 == 0 else (1, 0)):
                    samples[side].append(measure(xsh, xsht, FIXTURES / ((first if side == 0 else second) + '.xsh'), report))
            rows.append(evaluate(name, *samples))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--xsh', default=str(MAIN / 'target/verification/xsh'))
    parser.add_argument('--xsht', default=str(MAIN / 'target/verification/xsht'))
    parser.add_argument('--repeats', type=int, default=5)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error('--repeats must be positive')
    for row in run(args.xsh, args.xsht, args.repeats):
        print(f"{row['name']}: {row['verdict']}; output {row['output']}")
        print(f"  profile medians: {row['a']} -> {row['b']}")
        for key in ('cpu_ratios', 'wall_ratios'):
            values = row[key]
            print(f'  busy host: paired ratio {key} median {statistics.median(values):.3f}, range {min(values):.3f}–{max(values):.3f}')
    print('retention: inconclusive; profile buffered_bytes is always null')
    print('ordered buffering: inconclusive; profile buffered_bytes is always null')
    return 1


if __name__ == '__main__':
    raise SystemExit(main())
