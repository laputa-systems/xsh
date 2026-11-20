import argparse
import datetime
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
TIERS = ('edit', 'change', 'scenario', 'full')
BINARY = Path(os.environ.get('XSH_BENCH_BINARY', ROOT / 'target/bench-linux/x86_64-unknown-linux-musl/release/xsh'))
FIXTURES = ROOT / '.work/consolidation/comparative-fixtures'
IMAGE = 'xsh-comparative:consolidation'
REGISTRY = (
    ('harness-tests', 'edit', 'harness', 'benchmark accounting and parity contracts', 'estimated 30s'),
    ('consolidation-startup', 'change', 'startup', 'fresh process startup and output parity', 'estimated 1m'),
    ('stdlib-cold', 'change', 'stdlib', 'cold script and reference parity', 'estimated 1m'),
    ('consolidation-pipeline', 'scenario', 'pipeline', 'stream transformation and output parity', 'estimated 10m'),
    ('consolidation-spawn', 'scenario', 'process', 'sequential process creation and output parity', 'estimated 10m'),
    ('consolidation-directory', 'scenario', 'filesystem', 'directory traversal and output parity', 'estimated 10m'),
    ('consolidation-json', 'scenario', 'json', 'JSON processing and output parity', 'estimated 10m'),
    ('legacy-fast', 'scenario', 'allocation', 'xshi allocation baseline', 'estimated 10m'),
    ('legacy-syscalls', 'scenario', 'syscalls', 'xshi syscall diagnostic', 'estimated 10m'),
    ('consolidation-full', 'full', 'comparative', 'all comparative workloads and full samples', 'estimated 1h'),
    ('stdlib-full', 'full', 'stdlib', 'all host standard-library workloads', 'estimated 1h'),
    ('legacy-baseline', 'full', 'runtime', 'complete xshi latency baseline', 'estimated 1h'),
)


def select(args):
    known = {row[0] for row in REGISTRY}
    for name in (args.workload or []) + (args.skip or []):
        if name not in known:
            raise ValueError(f'unknown workload: {name}')
    if args.area and args.area not in {row[2] for row in REGISTRY}:
        raise ValueError(f'unknown area: {args.area}')
    if not (args.tier or args.workload or args.area or args.full or args.list):
        raise ValueError('select a tier, workload, or area; full requires --full')
    if args.tier == 'full' and not args.full:
        raise ValueError('full requires --full')
    ceiling = TIERS.index(args.tier) if args.tier else (3 if args.full else 2)
    selected = []
    excluded = []
    for row in REGISTRY:
        name, tier, area, _, _ = row
        reason = None
        if name in (args.skip or []): reason = 'explicitly skipped'
        elif args.workload and name not in args.workload: reason = 'not requested'
        elif args.area and area != args.area: reason = 'different area'
        elif TIERS.index(tier) > ceiling: reason = 'above tier ceiling'
        elif tier == 'full' and not args.full: reason = 'full requires --full'
        if reason: excluded.append((name, reason))
        else: selected.append(row)
    if not selected and not args.list:
        raise ValueError('selection is empty')
    if args.workload and any(name not in {r[0] for r in selected} for name in args.workload):
        raise ValueError('requested workload excluded by selectors or full guard')
    return selected, excluded


def command(name, out):
    python = ['python3', '-B']
    if name == 'harness-tests':
        return python + ['-m', 'unittest', 'discover', '-s', 'bench/consolidation', '-p', 'test_harness.py']
    if name.startswith('consolidation-'):
        argv = python + ['bench/consolidation/run.py', '--image', IMAGE, '--xsh', str(BINARY), '--fixtures', str(FIXTURES), '--output', str(out / 'result.json'), '--scratch', str(out / 'scratch'), '--source-commit', subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), '--source-dirty', 'dirty' if subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip() else 'clean', '--measure']
        if name != 'consolidation-full':
            argv += ['--workload', name.removeprefix('consolidation-')]
            if name == 'consolidation-startup': argv += ['--rounds', '1', '--samples', '3']
        return argv
    if name in ('stdlib-cold', 'stdlib-full'):
        argv = ['docker', 'run', '--rm', '--network', 'none', '--user', f'{os.getuid()}:{os.getgid()}', '-v', f'{ROOT}:{ROOT}', '-w', str(ROOT), '-e', f'TMPDIR={out}', 'xsh-test', *python, 'bench/stdlib-port/run.py', '--reference', str(BINARY), '--candidate', str(BINARY), '--observe', '--out', str(out / 'result.json')]
        if name == 'stdlib-cold': argv += ['--workload', 'cold_trivial', '--samples', '3']
        else: argv += ['--rounds', '3']
        return argv
    if name == 'legacy-baseline':
        prefix = shlex.split(os.environ.get('RUSTYBENCH', '')) or ['cargo', 'run', '--quiet', '--manifest-path', str(ROOT / '../../rustybench/Cargo.toml'), '--']
        return prefix + ['baseline', '--root', str(ROOT), '--baseline', str(ROOT / 'crates/xshi/benches/baseline.json'), '--', 'cargo', 'bench', '-p', 'xshi', '--bench', 'bench', '--features', 'benchmark']
    return ['cargo', 'dev', 'bench', {'legacy-fast': '--fast', 'legacy-syscalls': '--syscalls'}[name]]


def requirements(name):
    if name.startswith('consolidation-'): return 'python3, git, docker, comparative image, release xsh, pinned fixtures, 1 GiB free'
    if name in ('stdlib-cold', 'stdlib-full'): return 'python3, git, docker, xsh-test image, release xsh, fixtures, 1 GiB free'
    if name.startswith('legacy-'): return 'cargo, rustybench, 1 GiB free'
    return 'python3, git, 1 GiB free'


def preflight(selected):
    failures = []
    for row in selected:
        name = row[0]
        tools = ['python3', 'git']
        if name.startswith('consolidation-') or name in ('stdlib-cold', 'stdlib-full'): tools += ['docker']
        if name.startswith('legacy-'):
            tools += ['cargo']
            configured = shlex.split(os.environ.get('RUSTYBENCH', ''))
            if configured and not shutil.which(configured[0]): failures.append(f'{name}: missing {configured[0]}; fix RUSTYBENCH')
            if not configured and not (ROOT / '../../rustybench/Cargo.toml').is_file(): failures.append(f'{name}: missing rustybench checkout; set RUSTYBENCH to its executable')
        for tool in tools:
            if not shutil.which(tool): failures.append(f'{name}: missing {tool}; install it and retry')
        if name.startswith('consolidation-') or name in ('stdlib-cold', 'stdlib-full'):
            if not BINARY.is_file() or not os.access(BINARY, os.X_OK): failures.append(f'{name}: missing executable {BINARY}; build a release xsh and retry')
            elif not BINARY.resolve().is_relative_to(ROOT): failures.append(f'{name}: product must reside in this worktree; copy or build it here')
        if name in ('stdlib-cold', 'stdlib-full'):
            for fixture in ('unicode.txt', 'modules-200.txt', 'meminfo.txt', 'uptime.txt'):
                if not (ROOT / 'bench/stdlib-port/fixtures' / fixture).is_file(): failures.append(f'{name}: missing fixture {fixture}; restore it before running')
        if name.startswith('consolidation-'):
            if not (FIXTURES / 'manifest.json').is_file(): failures.append(f'{name}: missing fixtures; run python3 -B bench/consolidation/fixtures.py {FIXTURES}')
            else:
                sys.path.insert(0, str(ROOT / 'bench/consolidation'))
                from run import validate_fixtures
                try: validate_fixtures(FIXTURES)
                except (ValueError, OSError) as error: failures.append(f'{name}: invalid fixtures ({error}); regenerate them')
            if shutil.which('docker'):
                inspection = subprocess.run(['docker', 'image', 'inspect', IMAGE], capture_output=True, text=True)
                if inspection.returncode: failures.append(f'{name}: missing {IMAGE}; prepare the comparative overlay')
                else:
                    image = json.loads(inspection.stdout)[0]
                    base = image.get('Config', {}).get('Labels', {}).get('org.xsh.comparative.base')
                    if not base or not base.startswith('sha256:'): failures.append(f'{name}: overlay has no pinned base identity; prepare it again')
    if any(row[0] in ('stdlib-cold', 'stdlib-full') for row in selected) and shutil.which('docker') and subprocess.run(['docker', 'image', 'inspect', 'xsh-test'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        failures.append('stdlib-cold: missing xsh-test image; build Dockerfile.test and retry')
    if Path('/proc').is_dir():
        for process in Path('/proc').iterdir():
            if not process.name.isdigit() or int(process.name) == os.getpid(): continue
            try:
                if process.stat().st_uid != os.getuid() or (process / 'cwd').resolve() != ROOT: continue
                argv = (process / 'cmdline').read_bytes().split(b'\0')
                running = next((word.decode(errors='replace') for word in argv if word in (b'bench/consolidation/run.py', b'bench/stdlib-port/run.py')), None)
                if running: failures.append(f'selected workloads: previous runner {process.name} ({running}) is active; wait for it to finish')
            except (OSError, PermissionError): continue
    if shutil.disk_usage(ROOT).free < 1024**3:
        failures.append('selected workloads: less than 1 GiB free; free space and retry')
    if sys.version_info < (3, 11): failures.append('selected workloads: Python 3.11 or newer required; update Python')
    return failures


def ready(result):
    return result is not None and result.returncode == 0 and 'hot process tree' not in result.stdout


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--full', action='store_true')
    parser.add_argument('--tier', choices=TIERS)
    parser.add_argument('--workload', action='append')
    parser.add_argument('--area')
    parser.add_argument('--skip', action='append')
    args = parser.parse_args()
    try: selected, excluded = select(args)
    except ValueError as error: parser.error(str(error))
    if args.list:
        for name, tier, area, purpose, cost in REGISTRY:
            print(f'{name}\t{tier}\t{area}\t{purpose}\t{cost}\trequirements: {requirements(name)}\tcommand: cargo dev bench --workload {name}{" --full" if tier == "full" else ""}\toutput: .work/bench/<run>/{name}/')
        return 0
    for name, reason in excluded: print(f'excluded {name}: {reason}')
    if args.dry_run:
        for row in selected: print(shlex.join(command(row[0], ROOT / '.work/bench/<run>' / row[0])))
        return 0
    failures = preflight(selected)
    if failures:
        for failure in failures: print(failure, file=sys.stderr)
        print('preflight failed; nothing ran', file=sys.stderr)
        return 1
    if any(row[0].startswith('consolidation-') or row[0] in ('stdlib-cold', 'stdlib-full') for row in selected):
        newest = max((p.stat().st_mtime for directory in ('src', 'crates', 'core') for p in (ROOT / directory).rglob('*.rs')), default=0)
        if BINARY.stat().st_mtime < newest: print(f'warning: {BINARY} may be stale; rebuild it if unintended')
    if args.check:
        host_ready = subprocess.run(['perf-gate-ready'], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True) if shutil.which('perf-gate-ready') else None
        print('host readiness: ' + ('ready' if ready(host_ready) else 'inconclusive'))
        print('preflight passed; nothing ran')
        return 0
    run = ROOT / '.work/bench' / datetime.datetime.now().strftime('%Y%m%d-%H%M%S-%f')
    run.mkdir(parents=True)
    records = []
    for row in selected:
        name = row[0]
        out = run / name
        out.mkdir()
        before = subprocess.run(['perf-gate-ready'], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True) if name != 'harness-tests' and shutil.which('perf-gate-ready') else None
        start = time.monotonic()
        with (out / 'log.txt').open('w') as log:
            result = subprocess.run(command(name, out), stdout=log, stderr=subprocess.STDOUT, cwd=ROOT)
        duration = time.monotonic() - start
        after = subprocess.run(['perf-gate-ready', '-1'], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True) if before else None
        valid = name == 'harness-tests' or (ready(before) and ready(after))
        records.append({'workload': name, 'exit': result.returncode, 'duration_seconds': duration, 'timing_validity': 'valid' if valid else 'inconclusive: host contention or readiness check failed', 'readiness_before': before.stdout if before else None, 'readiness_after': after.stdout if after else None, 'verdict': 'inconclusive' if not valid or name in ('stdlib-cold', 'stdlib-full') else 'completion is not a performance verdict'})
    summary = {'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), 'dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()), 'workloads': records, 'exclusions': dict(excluded)}
    (run / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(f'summary: {run / "summary.json"}')
    return int(any(row['exit'] for row in records))


if __name__ == '__main__': sys.exit(main())
