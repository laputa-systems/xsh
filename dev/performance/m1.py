"""Milestone M1: profile Laputa's parallel source fetch and judge the profile.

The workload is `pm.xsh -- sources fetch --all` over a warm source cache, whose
`fetch_sources` stage runs `par-map(jobs: 4)`. Plain and profiled runs are
interleaved. The profile fails the milestone when the profiled run's output
differs from the plain run's, when it claims completeness after dropping data,
or when no parallel-worker region shows that the stage kept its parallel route.
"""

import argparse
from collections import Counter
from dataclasses import dataclass
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time


FETCH_ARGS = ('pm.xsh', '--', 'sources', 'fetch', '--repo', '.', '--all')


@dataclass(frozen=True)
class Run:
    status: int
    stdout: str
    stderr: str
    seconds: float


def summary_lines(stdout):
    return [line for line in stdout.splitlines() if line.startswith('sources summary ')]


def evaluate(plain, profiled, report):
    failures = []
    for kind, runs in (('plain', plain), ('profiled', profiled)):
        for run in runs:
            if run.status != 0:
                failures.append(f'{kind} run exited {run.status}')
    if plain and profiled:
        if summary_lines(plain[0].stdout) != summary_lines(profiled[0].stdout):
            failures.append('profiled summary differs from the plain run')
        if Counter(plain[0].stderr.splitlines()) != Counter(profiled[0].stderr.splitlines()):
            failures.append('profiled progress lines differ from the plain run')
    completeness = report.get('completeness', {})
    dropped = completeness.get('dropped', 0)
    if report.get('complete') is True and dropped:
        failures.append(f'profile claims completeness after dropping {dropped} records')
    if report.get('complete') is not True:
        failures.append(f'profile is incomplete: dropped {dropped}')
    workers = [region for region in completeness.get('opaque_regions', [])
               if region.get('kind') == 'parallel-workers' and region.get('overlapping')]
    if not workers or max(region.get('count', 0) for region in workers) < 2:
        failures.append('no parallel-worker region: the fetch stage did not run in parallel')
    if not report.get('stages'):
        failures.append('no parallel stages in profile')
    return failures


def top_sites(report, limit):
    sites = sorted(report.get('sites', []), key=lambda site: site['exclusive_ns'], reverse=True)
    return [f"{site['exclusive_ns'] / 1e9:8.3f}s {site['alloc_bytes'] / 1e6:9.1f}MB {site['calls']:6} "
            f"{site['kind']} {site['name']} {Path(site['file']).name}:{site['line']}"
            for site in sites[:limit]]


def execute(argv, cwd, env):
    started = time.monotonic()
    done = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True)
    return Run(done.returncode, done.stdout, done.stderr, time.monotonic() - started)


def run(laputa_root, xsh_root, xsh, xsht, target, repeats):
    env = dict(os.environ)
    env['XSH_MODULE_PATH'] = f'{laputa_root}:{xsh_root / "core"}'
    env['LAPUTA_SOURCE_CACHE'] = str(laputa_root / '.cache' / 'sources')
    args = [*FETCH_ARGS, '--target', target]
    plain, profiled = [], []
    with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as scratch:
        report_path = Path(scratch) / 'profile.json'
        for _ in range(repeats):
            plain.append(execute([xsh, *args], laputa_root, env))
            profiled.append(execute([xsht, 'profile', '--json', str(report_path), *args], laputa_root, env))
        report = json.loads(report_path.read_text()) if report_path.exists() else {}
    return plain, profiled, report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--laputa-root', type=Path, required=True)
    parser.add_argument('--xsh-root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--xsh')
    parser.add_argument('--xsht')
    parser.add_argument('--target', default='x86_64-linux-musl')
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--json', type=Path)
    options = parser.parse_args(argv)
    xsh_root = options.xsh_root.resolve()
    xsh = options.xsh or str(xsh_root / 'target' / 'verification' / 'xsh')
    xsht = options.xsht or str(xsh_root / 'target' / 'verification' / 'xsht')
    plain, profiled, report = run(options.laputa_root.resolve(), xsh_root, xsh, xsht, options.target, options.repeats)
    failures = evaluate(plain, profiled, report)
    plain_median = statistics.median(run.seconds for run in plain)
    profiled_median = statistics.median(run.seconds for run in profiled)
    print(f'plain median {plain_median:.3f}s, profiled median {profiled_median:.3f}s, '
          f'overhead {profiled_median / plain_median - 1:+.1%} over {options.repeats} interleaved pairs')
    print(*top_sites(report, 10), sep='\n')
    for site in report.get('critical_path', []):
        print(f"critical {site['kind']} {site['name']}: {site['inclusive_ns'] / 1e9:.3f}s")
    for stage in report.get('stages', []):
        print(f"stage {stage['file']}:{stage['line']}: {stage['items']} items, imbalance {stage['imbalance']:.2f}, buffered {stage['buffered_results']}")
    for region in report.get('completeness', {}).get('opaque_regions', []):
        print(f"opaque {region['kind']}: {region['wall_ns'] / 1e9:.3f}s over {region['count']}, "
              f"overlapping={region['overlapping']}")
    if options.json:
        options.json.write_text(json.dumps({'failures': failures, 'plain_s': plain_median,
                                            'profiled_s': profiled_median, 'report': report}) + '\n')
    for failure in failures:
        print(f'M1 FAIL: {failure}', file=sys.stderr)
    print('M1 profile: ' + ('faithful' if not failures else 'not faithful'))
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(main())
