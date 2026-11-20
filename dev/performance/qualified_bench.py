import hashlib
import json
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PRODUCTS = ('xsh', 'xsht', 'xshi')
CORPUS = ('core/lib/search.xsh', 'core/lib/dhcp_frame.xsh', 'core/lib/text_input.xsh',
          'core/tests/test-sha512sum.xsh', 'core/tests/test-touch.xsh', 'core/tests/test-host.xsh')
TREE_OUTPUT = b''.join(f'probe {i}\n'.encode() for i in range(16))
TREE_ROUNDS = 5
TOOLING_SAMPLES = 21
TOOLING_WARMUP = 5
MINIMUM_TOLERANCE = 0.05


def tree_complete(status, stdout, stderr):
    return status == 0 and stdout == TREE_OUTPUT and stderr == b''


def tooling_complete(mode, expected, results):
    report = rb'xsht ' + mode.encode() + rb': 1 file in [0-9.]+s \(thread time by stage: [^\n]+\)\n'
    return len(results) == expected and all(code == 0 and not out and re.fullmatch(report, err) for code, out, err in results)


def products_complete(sizes):
    return set(sizes) == set(PRODUCTS) and all(type(size) is int and size > 0 for size in sizes.values())


def relative_spread(samples):
    middle = statistics.median(samples)
    return statistics.median(abs(sample - middle) for sample in samples) / middle


def anchor_tolerance(samples):
    if not samples: return MINIMUM_TOLERANCE
    return max(MINIMUM_TOLERANCE, round(3 * relative_spread(samples), 4))


def combine_tree_rounds(rounds):
    if len(rounds) != TREE_ROUNDS: raise ValueError(f'expected {TREE_ROUNDS} measured tree rounds, got {len(rounds)}')
    peaks = [round_['value'] for round_ in rounds]
    return {**rounds[0], 'value': statistics.median(peaks), 'samples_bytes': peaks,
            'scope': f'median of {TREE_ROUNDS} fresh container cgroup v2 peaks after one discarded warm-up container, including workload tree and Python driver'}


def verdict(value, anchor, host_ready, host=None):
    reasons = []
    if not host_ready: reasons.append('host readiness check failed')
    if anchor is None: reasons.append('anchor missing')
    if reasons: return 'inconclusive', '; '.join(reasons)
    if (not isinstance(anchor, dict) or any(not anchor.get(key) for key in ('commit', 'host', 'date'))
            or type(anchor.get('value')) not in (int, float) or anchor['value'] <= 0
            or type(anchor.get('tolerance')) not in (int, float) or not 0 <= anchor['tolerance'] < 1):
        return 'inconclusive', 'invalid anchor'
    if host is not None and anchor['host'] != host: return 'inconclusive', 'anchor host differs'
    return ('within budget' if value <= anchor['value'] * (1 + anchor['tolerance']) else 'regression'), None


def identity(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def measure(name, products):
    if name == 'tree-memory':
        p = subprocess.run([str(products / 'xsh'), str(ROOT / 'dev/performance/fixtures/core-cases/pipeline.xsh'), '--', '16'], capture_output=True)
        if not tree_complete(p.returncode, p.stdout, p.stderr):
            raise ValueError(f'tree oracle failed: status={p.returncode} stdout={p.stdout!r} stderr={p.stderr!r}')
        peak = int(Path('/sys/fs/cgroup/memory.peak').read_text().strip())
        if peak <= 0: raise ValueError('invalid cgroup memory peak')
        return {'value': peak, 'unit': 'bytes', 'scope': 'fresh container cgroup v2 including workload tree and Python driver', 'completed_lines': 16}
    if name in ('tooling-check', 'tooling-lint'):
        mode = name.removeprefix('tooling-')
        samples = []
        for index in range(TOOLING_WARMUP + TOOLING_SAMPLES):
            started = time.monotonic()
            results = []
            for path in CORPUS:
                p = subprocess.run([str(products / 'xsht'), mode, str(ROOT / path)], capture_output=True)
                results.append((p.returncode, p.stdout, p.stderr))
            if not tooling_complete(mode, len(CORPUS), results):
                raise ValueError(f'{mode} oracle failed: {results!r}')
            if index >= TOOLING_WARMUP: samples.append(time.monotonic() - started)
        return {'value': statistics.median(samples), 'unit': 'seconds per corpus', 'samples_seconds': samples,
                'files': list(CORPUS), 'completed_files': len(CORPUS), 'expected_findings': 0,
                'files_per_second': len(CORPUS) / statistics.median(samples)}
    if name == 'artifact-size':
        sizes = {}
        for product in PRODUCTS:
            path = products / product
            with path.open('rb') as f:
                if f.read(4) != b'\x7fELF': raise ValueError(f'{product}: not an ELF product')
            sizes[product] = path.stat().st_size
        if not products_complete(sizes): raise ValueError('artifact oracle failed')
        return {'value': sum(sizes.values()), 'unit': 'bytes', 'scope': 'sum of unstripped release xsh, xsht and xshi ELF files', 'product_bytes': sizes}
    raise ValueError(f'unknown workload {name}')


if __name__ == '__main__':
    workload, product_dir, output = sys.argv[1:]
    product_dir = Path(product_dir)
    result = measure(workload, product_dir)
    result['binaries'] = [identity(product_dir / name) for name in (PRODUCTS if workload == 'artifact-size' else ('xsh',) if workload == 'tree-memory' else ('xsht',))]
    Path(output).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
