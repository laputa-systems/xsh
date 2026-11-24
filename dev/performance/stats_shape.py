"""Compare record shapes against the runtime's stats specialization."""

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import resource
import statistics
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'dev/performance/stats_shape'
VARIANTS = ('exact', 'exact-blob', 'renamed', 'extra', 'fewer', 'wider-payload')
COUNTERFACTUALS = ('renamed', 'extra', 'fewer')


@dataclass(frozen=True)
class Sample:
    stdout: str
    cpu: float
    cycles: int | None


def describe(values):
    return {'median': statistics.median(values), 'min': min(values), 'max': max(values)}


def evaluate(samples):
    if set(samples) != set(VARIANTS) or not samples['exact']:
        raise ValueError('expected samples for every variant')
    count = len(samples['exact'])
    oracle = samples['exact'][0].stdout
    for name, rows in samples.items():
        if len(rows) != count:
            raise ValueError(f'{name}: unequal sample count')
        if any(row.stdout != oracle for row in rows):
            raise ValueError(f'{name}: output mismatch')
    cycle_values = [row.cycles for rows in samples.values() for row in rows]
    if any(value is None for value in cycle_values) and not all(value is None for value in cycle_values):
        raise ValueError('incomplete cycle measurements')
    metric = 'cpu' if cycle_values[0] is None else 'cycles'
    results = {}
    for name, rows in samples.items():
        results[name] = {'cpu': describe([row.cpu for row in rows])}
        if metric == 'cycles':
            results[name]['cycles'] = describe([row.cycles for row in rows])
    baseline = results['exact'][metric]
    if baseline['median'] <= 0:
        raise ValueError('baseline must be positive')
    for result in results.values():
        result['ratio'] = result[metric]['median'] / baseline['median']
    cliff = any(results[name]['ratio'] > 1.25 and results[name][metric]['min'] > baseline['max']
                for name in COUNTERFACTUALS)
    return {'metric': metric, 'verdict': 'cliff shown' if cliff else 'no cliff',
            'output': oracle, 'variants': results}


def command(argv):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    done = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    if done.returncode:
        raise RuntimeError(f'{argv}: exit {done.returncode}\n{done.stderr}')
    cpu = after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime
    return done.stdout, done.stderr, cpu


def measure(xsh, script, use_perf):
    argv = [xsh, str(script)]
    if use_perf:
        argv = ['/usr/bin/perf', 'stat', '-x,', '-e', 'cycles:u', '--'] + argv
    stdout, stderr, cpu = command(argv)
    cycles = None
    if use_perf:
        rows = [line.split(',') for line in stderr.splitlines() if ',cycles:u,' in line]
        if len(rows) != 1 or not rows[0][0].isdigit():
            raise RuntimeError(f'perf did not report cycles:u: {stderr}')
        cycles = int(rows[0][0])
    return Sample(stdout, cpu, cycles)


def run(xsh, xsht, repeats):
    scratch = ROOT / '.work/tmp'
    scratch.mkdir(parents=True, exist_ok=True)
    samples = {name: [] for name in VARIANTS}
    with tempfile.TemporaryDirectory(dir=scratch, prefix='stats-shape-') as temp:
        report = Path(temp) / 'runtime.json'
        oracle = None
        for name in VARIANTS:
            stdout, _, _ = command([xsht, 'runtime-stats', '--json', str(report), str(FIXTURES / f'{name}.xsh')])
            if oracle is None:
                oracle = stdout
            if stdout != oracle or json.loads(report.read_text())['status'] != 0:
                raise ValueError(f'{name}: runtime-stats output mismatch or failure')
        for index in range(repeats):
            for name in (VARIANTS if index % 2 == 0 else reversed(VARIANTS)):
                sample = measure(xsh, FIXTURES / f'{name}.xsh', Path('/usr/bin/perf').is_file())
                if sample.stdout != oracle:
                    raise ValueError(f'{name}: output mismatch with runtime-stats')
                samples[name].append(sample)
    return evaluate(samples)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--xsh', default=str(ROOT / 'target/verification/xsh'))
    parser.add_argument('--xsht', default=str(ROOT / 'target/verification/xsht'))
    parser.add_argument('--json', type=Path)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error('--repeats must be positive')
    result = run(os.path.abspath(args.xsh), os.path.abspath(args.xsht), args.repeats)
    print(f"{args.repeats} interleaved runs; output: {result['output'].strip()}")
    for name, row in result['variants'].items():
        cpu = row['cpu']
        line = f"{name}: CPU seconds {cpu['median']:.6f} [{cpu['min']:.6f}, {cpu['max']:.6f}]"
        if 'cycles' in row:
            cycles = row['cycles']
            line += f"; cycles:u {cycles['median']:.0f} [{cycles['min']:.0f}, {cycles['max']:.0f}]"
        print(line + f"; {result['metric']} ratio {row['ratio']:.3f}x")
    print(f"verdict: {result['verdict']} ({result['metric']})")
    if args.json:
        args.json.write_text(json.dumps(result, indent=2) + '\n')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
