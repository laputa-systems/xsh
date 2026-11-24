"""Measure paired XSH abstraction costs and rank execution ratios."""

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'dev/performance/abstraction'
PAIRS = (
    ('inline', 'inline expression', 'pure helper'),
    ('import', 'local helper', 'imported helper'),
    ('generic', 'monomorphic helper', 'generic helper'),
    ('callable', 'direct call', 'typed callable'),
    ('bindings', 'annotated bindings', 'inferred bindings'),
    ('pipeline', 'for loop', 'map/where/reduce'),
    ('record', 'scalar arguments', 'record argument'),
    ('stage', 'stage builtin method', 'stage helper'),
)


@dataclass(frozen=True)
class Sample:
    stdout: str
    wall: float
    preparation: float
    execution: float
    run_inclusive: float


def ratio(numerator, denominator):
    if denominator <= 0:
        raise ValueError('ratio denominator must be positive')
    return numerator / denominator


def describe(values):
    return {'median': statistics.median(values), 'min': min(values), 'max': max(values)}


def evaluate(name, a_label, b_label, a, b, cause=None):
    if not a or not b or len(a) != len(b):
        raise ValueError(f'{name}: expected equally sized nonempty samples')
    if len({sample.stdout for sample in (*a, *b)}) != 1:
        raise ValueError(f'{name}: output mismatch between runs')
    if cause is not None and (not cause.get('description') or not cause.get('citation')):
        raise ValueError(f'{name}: explanation requires a cause and citation')
    sides = {}
    for label, samples in (('a', a), ('b', b)):
        sides[label] = {field: describe([getattr(sample, field) for sample in samples])
                        for field in ('wall', 'preparation', 'execution', 'run_inclusive')}
    return {'name': name, 'a_label': a_label, 'b_label': b_label,
            'output': a[0].stdout, **sides,
            'execution_ratio': ratio(sides['b']['execution']['median'], sides['a']['execution']['median']),
            'preparation_ratio': ratio(sides['b']['preparation']['median'], sides['a']['preparation']['median']),
            'explanation': cause if cause else None,
            'status': 'explained' if cause else 'unexplained'}


def rank(rows):
    return sorted(rows, key=lambda row: row['execution_ratio'], reverse=True)


def execute(command, cwd):
    started = time.monotonic()
    done = subprocess.run(command, cwd=cwd, capture_output=True, text=True)
    elapsed = time.monotonic() - started
    if done.returncode:
        raise RuntimeError(f'{command}: exit {done.returncode}\n{done.stderr}')
    return done.stdout, elapsed


def measure(xsh, xsht, script, report_path):
    stdout, wall = execute([xsh, str(script)], ROOT)
    report_path.unlink(missing_ok=True)
    instrumented_stdout, _ = execute([xsht, 'runtime-stats', '--json', str(report_path), str(script)], ROOT)
    if stdout != instrumented_stdout:
        raise ValueError(f'{script}: runtime-stats output mismatch')
    report = json.loads(report_path.read_text())
    if report['status'] != 0:
        raise RuntimeError(f'{script}: runtime-stats status {report["status"]}')
    phases = report['wall_time_ns']
    preparation, execution, inclusive = (phases[k] for k in
        ('preparation_inclusive', 'execution_exclusive', 'run_inclusive'))
    if preparation is None or execution is None or inclusive is None:
        raise RuntimeError(f'{script}: runtime-stats did not report required phases')
    return Sample(stdout, wall, preparation / 1e9, execution / 1e9, inclusive / 1e9)


def run(xsh, xsht, repeats):
    scratch = ROOT / '.work/tmp'
    scratch.mkdir(parents=True, exist_ok=True)
    rows = []
    with tempfile.TemporaryDirectory(dir=scratch, prefix='abstraction-') as temp:
        report = Path(temp) / 'runtime.json'
        for name, a_label, b_label in PAIRS:
            samples = {'a': [], 'b': []}
            for index in range(repeats):
                for side in ('a', 'b') if index % 2 == 0 else ('b', 'a'):
                    samples[side].append(measure(xsh, xsht, FIXTURES / f'{name}-{side}.xsh', report))
            rows.append(evaluate(name, a_label, b_label, samples['a'], samples['b']))
    return rank(rows)


def format_distribution(value):
    return f"{value['median']:.4f} [{value['min']:.4f}, {value['max']:.4f}]"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--xsh', default=str(ROOT / 'target/verification/xsh'))
    parser.add_argument('--xsht', default=str(ROOT / 'target/verification/xsht'))
    parser.add_argument('--json', type=Path)
    options = parser.parse_args(argv)
    if options.repeats < 1:
        parser.error('--repeats must be positive')
    rows = run(os.path.abspath(options.xsh), os.path.abspath(options.xsht), options.repeats)
    print(f'{options.repeats} interleaved pairs; seconds: median [min, max]')
    for position, row in enumerate(rows, 1):
        print(f"{position}. {row['name']}: {row['a_label']} -> {row['b_label']}; "
              f"execution B/A {row['execution_ratio']:.3f}x, preparation B/A {row['preparation_ratio']:.3f}x; {row['status']}")
        for side in ('a', 'b'):
            data = row[side]
            print(f"   {side.upper()} wall {format_distribution(data['wall'])}, "
                  f"preparation {format_distribution(data['preparation'])}, "
                  f"execution {format_distribution(data['execution'])}, "
                  f"run inclusive {format_distribution(data['run_inclusive'])}")
        print(f"   matching output: {row['output'].strip()}")
        if row['explanation']:
            print(f"   cause: {row['explanation']['description']} ({row['explanation']['citation']})")
    if options.json:
        options.json.write_text(json.dumps(rows, indent=2) + '\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
