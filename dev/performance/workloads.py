from dataclasses import dataclass
import base64
import hashlib
import json
from pathlib import Path
import statistics

from bench.consolidation.fixtures import DEMANDING_PACKAGE_ROW_SHAPES, PACKAGE_ROW_SHAPES


SOURCE_ROOT = Path(__file__).resolve().parents[2]
STREAM_ITEMS = (65536, 131072, 262144)


@dataclass(frozen=True)
class Workload:
    name: str
    family: str
    shape: str
    script: Path
    arguments: tuple[str, ...]
    expected_stdout: bytes
    expected_subprocesses: int

    @property
    def useful_stdout_prefix(self):
        return self.expected_stdout.splitlines(keepends=True)[0]

    def argv(self, binary):
        return [str(binary), str(self.script), *(['--', *self.arguments] if self.arguments else [])]

    @property
    def expected_records(self):
        if self.family == 'streaming':
            return int(self.expected_stdout.removeprefix(b'value=').strip())
        if self.name.startswith('json-'):
            return len(json.loads(self.expected_stdout))
        if self.family == 'filesystem-process':
            return len(self.expected_stdout.splitlines())
        return None


def frozen_workloads(fixtures):
    cases = [
        Workload('startup-trivial', 'startup', 'common', SOURCE_ROOT / 'bench/stdlib-port/cold_trivial.xsh', (), b'hello\n', 0),
        Workload('startup-template', 'startup', 'demanding', SOURCE_ROOT / 'bench/stdlib-port/cold_template.xsh', (), b'hello xsh\n', 0),
    ]
    for shape, count in zip(('n', '2n', '4n'), STREAM_ITEMS):
        for mode in ('count', 'map-where'):
            cases.append(Workload(f'stream-{mode}-{shape}', 'streaming', shape, SOURCE_ROOT / 'bench/stream-stage-cost.xsh', (mode, str(count)), f'value={count}\n'.encode(), 0))
    for shape, count in zip(('n', '2n', '4n'), PACKAGE_ROW_SHAPES):
        root = fixtures / str(count)
        for script, family in (('json', 'algorithms'), ('directory', 'filesystem-process')):
            cases.append(Workload(f'{script}-{shape}', family, shape, SOURCE_ROOT / f'bench/consolidation/xsh/{script}.xsh', (str(root),), (root / 'expected' / script).read_bytes(), 0))
    cases.append(Workload('spawn-128', 'filesystem-process', 'common', SOURCE_ROOT / 'bench/consolidation/xsh/spawn.xsh', (), b''.join(f'probe {index}\n'.encode() for index in range(128)), 128))
    for count in DEMANDING_PACKAGE_ROW_SHAPES:
        root = fixtures / str(count)
        shape = f'package-range-{count}'
        for script, family in (('json', 'algorithms'), ('directory', 'filesystem-process')):
            cases.append(Workload(f'{script}-{shape}', family, shape, SOURCE_ROOT / f'bench/consolidation/xsh/{script}.xsh', (str(root),), (root / 'expected' / script).read_bytes(), 0))
    return tuple(cases)


def trace_counts(rows):
    summaries = []
    starts = 0
    completed = 0
    for row in rows:
        if row.get('type') == 'syscall.summary':
            summaries.append(row)
        payload = row.get('payload', {})
        starts += payload.get('type') in ('run.start', 'spawn.start')
        if payload.get('type') in ('run.end', 'spawn.ready'):
            if type(payload.get('pid')) is not int or payload.get('error') is not None:
                raise ValueError('helper attribution unavailable: incomplete process trace')
            completed += 1
    if len(summaries) != 1 or type(summaries[0].get('syscall_count')) is not int:
        raise ValueError('syscall attribution unavailable: expected one complete syscall summary')
    if completed != starts:
        raise ValueError('helper attribution unavailable: incomplete process trace')
    return {'syscalls': summaries[0]['syscall_count'], 'subprocesses': completed}


def run_cohort(fixtures, xsh, xsht, scratch, output, sampler, resources, *, diagnostic=False):
    cases = frozen_workloads(fixtures)
    report = json.loads(output.read_text()) if output.exists() else {
        'schema': 1, 'mode': 'measure', 'parity': [], 'samples': [], 'summary': {},
        'complete': False, 'timing_complete': False, 'attribution_complete': False, 'attribution': {},
        'method': {'rounds': 3, 'samples_per_tool_per_round': 30, 'startup_samples_per_tool_per_round': 60,
                   'filesystem': 'warm', 'process': 'fresh', 'warmups_per_tool_workload': 1,
                   'first_useful_stdout': 'first complete expected stdout line observed on the host pipe; not first byte',
                   'readiness': 'one host window shared by the entire timed batch',
                   'resource_scope': 'wait4 owned leader; CPU includes only descendant usage waited by leader; RSS is Linux ru_maxrss, not summed or cgroup memory'},
    }
    scratch.mkdir(parents=True, exist_ok=False)
    def publish():
        output.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    def record(case, argv, directory, environment, *, measured=False):
        row = sampler.invoke(argv, directory, environment, 60, measured=measured, useful_prefix=case.useful_stdout_prefix)
        row.update(tool='xsh', workload=case.name, family=case.family, shape=case.shape, argv=argv,
                   equal=sampler.equivalent(row, case.expected_stdout))
        row['failure_count'] = int(not row['equal'])
        row['completed_records'] = case.expected_records if row['equal'] else None
        return row
    report['diagnostic_cgroup_before' if diagnostic else 'timed_batch_cgroup_before'] = resources.cgroup_snapshot()
    publish()
    primary = None
    try:
        for case in cases:
            directory = scratch / case.name
            directory.mkdir()
            for name in ('home', 'tmp', 'config', 'cache', 'data'):
                (directory / name).mkdir()
            environment = {'PATH': '/usr/local/bin:/usr/bin:/bin', 'LC_ALL': 'C', 'LANG': 'C', 'TZ': 'UTC',
                           'HOME': str(directory / 'home'), 'TMPDIR': str(directory / 'tmp'),
                           'XDG_CONFIG_HOME': str(directory / 'config'), 'XDG_CACHE_HOME': str(directory / 'cache'),
                           'XDG_DATA_HOME': str(directory / 'data'), 'NO_COLOR': '1', 'PYTHONDONTWRITEBYTECODE': '1'}
            if diagnostic:
                phase = scratch / f'{case.name}.runtime-stats.json'
                trace = scratch / f'{case.name}.trace.jsonl'
                observations = []
                for argv in ([str(xsht), 'runtime-stats', '--json', str(phase), *case.argv(xsht)[1:]],
                             [str(xsht), 'trace', '--syscalls', '--trace-format', 'jsonl', '--trace-file', str(trace), *case.argv(xsht)[1:]]):
                    row = record(case, argv, directory, environment)
                    observations.append(row)
                    report['attribution'][case.name] = {'observations': observations}
                    publish()
                    if not row['equal']:
                        raise ValueError(f'diagnostic output/status/effects failed: {case.name}')
                with trace.open() as stream:
                    counts = trace_counts(json.loads(line) for line in stream)
                phase_report = json.loads(phase.read_text())
                if phase_report.get('tracking_active') is not True or phase_report.get('status') != 0:
                    raise ValueError(f'ordinary runtime allocation attribution unavailable: {case.name}')
                if phase_report.get('cpu_time_ns', {}).get('clock') != 'process_cpu' or phase_report.get('wall_time_ns', {}).get('clock') != 'monotonic_wall':
                    raise ValueError(f'ordinary runtime phase attribution unavailable: {case.name}')
                report['attribution'][case.name].update(counts, runtime_stats=str(phase), syscall_trace=str(trace),
                    syscall_scope='instrumented indexed XSH process; detached external helper descendants excluded', production_syscalls=None)
                publish()
                if counts['subprocesses'] != case.expected_subprocesses:
                    raise ValueError(f'unexpected helper count: {case.name}')
                continue
            for _ in range(2):
                row = record(case, case.argv(xsh), directory, environment)
                report['parity'].append(row)
                publish()
                if not row['equal'] or not row['useful_stdout_observed']:
                    raise ValueError(f'workload parity failed: {case.name}')
            for round_index in range(3):
                for sample_index in range(60 if case.family == 'startup' else 30):
                    row = record(case, case.argv(xsh), directory, environment, measured=True)
                    row.update(round=round_index, sample=sample_index, position=0, order=['xsh'])
                    report['samples'].append(row)
                    row['stdout_sha256'] = hashlib.sha256(base64.b64decode(row['stdout_base64'])).hexdigest()
                    if row['equal']:
                        del row['stdout_base64']
                    if not row['equal'] or row['first_useful_stdout_ns'] is None:
                        raise ValueError(f'measured output/status/effects failed: {case.name}')
                    if row.get('cpu_ns') is None or row.get('max_rss_bytes') is None:
                        raise ValueError(f'CPU/RSS accounting unavailable: {case.name}')
                publish()
            rows = [row for row in report['samples'] if row['workload'] == case.name]
            cpus = [row['cpu_ns'] for row in rows]
            useful = [row['first_useful_stdout_ns'] for row in rows]
            report['summary'][f'xsh/{case.name}'] = sampler.summarize_samples(report['samples'], 'xsh', case.name) | {
                'median_cpu_ns': statistics.median(cpus), 'min_cpu_ns': min(cpus), 'cpu_range_ns': [min(cpus), max(cpus)],
                'wall_range_ns': [min(row['wall_ns'] for row in rows), max(row['wall_ns'] for row in rows)],
                'median_first_useful_stdout_ns': statistics.median(useful), 'first_useful_stdout_range_ns': [min(useful), max(useful)],
                'max_rss_bytes': max(row['max_rss_bytes'] for row in rows)}
        report['attribution_complete' if diagnostic else 'timing_complete'] = True
    except BaseException as error:
        primary = error
        raise
    finally:
        try:
            report['diagnostic_cgroup_after' if diagnostic else 'timed_batch_cgroup_after'] = resources.cgroup_snapshot()
            publish()
        except BaseException as failure:
            if primary is None:
                raise
            primary.add_note(f'failed resource/report finalization: {failure}')
