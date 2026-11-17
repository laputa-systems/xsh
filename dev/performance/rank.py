#!/usr/bin/env python3
import argparse
from collections import defaultdict
import json
from pathlib import Path
import re
import sys

import results

CARGO_CASE = re.compile(r'^test (?P<name>.+?) \.\.\. (?P<outcome>ok|FAILED|ignored)(?:, [^<]*)?(?: <(?P<seconds>\d+(?:\.\d+)?)s>)?$')
CARGO_RESULT = re.compile(r'^test result: (?:ok|FAILED)\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; (?P<ignored>\d+) ignored;')


def seconds(ns):
    return f'{ns / 1_000_000_000:.6f}s'


def native_summary(text, limit):
    rows = results._records(text)
    summary = rows[-1]
    result = results.reconcile_xsht(text, rows[0]['selected'], child_status=summary['status'])
    if not result['coverage_complete']:
        raise results.ResultError('native results are incomplete')
    starts = {row['case_id']: row for row in rows if row['event'] == 'case_started'}
    cases = [(name, case['duration_ns'], starts[name]['start_offset_ns'], starts[name]['worker'])
             for name, case in result['cases'].items() if name in starts]
    files = defaultdict(list)
    workers = defaultdict(int)
    for name, duration, start, worker in cases:
        files[name.rsplit('::', 1)[0]].append((duration, start))
        workers[worker] += duration
    wall = summary['duration_ns']
    ranked_files = sorted(files.items(), key=lambda item: -sum(v[0] for v in item[1]))
    last = max(cases, key=lambda c: c[2] + c[1], default=None)
    return {
        'wall_ns': wall,
        'case_count': len(cases),
        'case_sum_ns': sum(c[1] for c in cases),
        'last_case_end_ns': max((start + duration for _, duration, start, _ in cases), default=0),
        'workers': [{'worker': worker, 'busy_ns': workers[worker], 'idle_ns': max(0, wall - workers[worker])}
                    for worker in range(rows[0]['workers'])],
        'files': [{'file': name, 'cases': len(values), 'case_sum_ns': sum(d for d, _ in values),
                   'span_ns': max(s + d for d, s in values) - min(s for _, s in values)}
                  for name, values in ranked_files[:limit]],
        'cases': [{'case': name, 'duration_ns': duration} for name, duration, _, _ in sorted(cases, key=lambda c: -c[1])[:limit]],
        'tail': [{'case': name, 'start_ns': start, 'end_ns': start + duration, 'worker': worker}
                 for name, duration, start, worker in sorted(cases, key=lambda c: -c[2])[:limit]],
        'last_to_finish': None if last is None else {'case': last[0], 'worker': last[3], 'start_ns': last[2], 'duration_ns': last[1]},
    }


def render_native(data):
    lines = [f"Native run: wall {seconds(data['wall_ns'])}, last case ends at {seconds(data['last_case_end_ns'])}, "
             f"case sum {seconds(data['case_sum_ns'])}", 'Files (case sum, wall span):']
    lines += [f"  {seconds(f['case_sum_ns'])} {seconds(f['span_ns'])} {f['file']}" for f in data['files']]
    lines.append('Cases (duration):')
    lines += [f"  {seconds(c['duration_ns'])} {c['case']}" for c in data['cases']]
    lines.append('Workers (busy, idle over run wall):')
    lines += [f"  {w['worker']}: {seconds(w['busy_ns'])} {seconds(w['idle_ns'])}" for w in data['workers']]
    lines.append('Tail (last starts; end offset):')
    lines += [f"  start {seconds(t['start_ns'])} end {seconds(t['end_ns'])} worker {t['worker']} {t['case']}" for t in data['tail']]
    if last := data['last_to_finish']:
        lines.append(f"Last to finish: {last['case']} (worker {last['worker']}, start {seconds(last['start_ns'])}, "
                     f"duration {seconds(last['duration_ns'])})")
    return '\n'.join(lines)


def cargo_summary(text, limit):
    cases = []
    counted = {'passed': 0, 'failed': 0, 'ignored': 0}
    summaries = 0
    for line in text.splitlines():
        if summary := CARGO_RESULT.match(line):
            summaries += 1
            for key in counted:
                counted[key] -= int(summary[key])
            continue
        case = CARGO_CASE.match(line)
        if not case:
            continue
        counted[{'ok': 'passed', 'FAILED': 'failed', 'ignored': 'ignored'}[case['outcome']]] += 1
        if case['outcome'] == 'ignored':
            continue
        if case['seconds'] is None:
            raise results.ResultError(f'Cargo test duration missing for {case["name"]}; record with --report-time')
        cases.append((case['name'], round(float(case['seconds']) * 1_000_000_000)))
    if summaries == 0 or any(counted.values()):
        raise results.ResultError('incomplete Cargo output: test lines and result summaries disagree')
    return {'case_count': len(cases), 'case_sum_ns': sum(duration for _, duration in cases),
            'cases': [{'case': name, 'duration_ns': duration} for name, duration in sorted(cases, key=lambda c: -c[1])[:limit]]}


def render_cargo(data):
    lines = [f"Cargo cases: {data['case_count']}, case sum {seconds(data['case_sum_ns'])}", 'Cases (duration):']
    lines += [f"  {seconds(c['duration_ns'])} {c['case']}" for c in data['cases']]
    return '\n'.join(lines)


def cargo_binary(path):
    name = path.name.removeprefix('cargo-').rsplit('.', 1)[0]
    return name.rsplit('-', 1)[0] if re.fullmatch(r'.+-[0-9a-f]{16}', name) else name


def main():
    parser = argparse.ArgumentParser(description='Rank recorded native or Cargo test timings')
    parser.add_argument('events', type=Path, nargs='+')
    parser.add_argument('--limit', type=int, default=10)
    parser.add_argument('--json', type=Path, help='also write the summaries to this JSON file')
    parser.add_argument('--label', default='', help='free-form identity stored in the JSON summary')
    args = parser.parse_args()
    if args.limit < 1:
        parser.error('--limit must be positive')
    record = {'schema': 1, 'label': args.label, 'native': [], 'cargo': []}
    try:
        for path in args.events:
            text = results._event_file(path)
            print(f'{path}:')
            if text.startswith('{'):
                data = native_summary(text, args.limit)
                record['native'].append({'source': path.name, **data})
                print(render_native(data))
            else:
                data = cargo_summary(text, args.limit)
                record['cargo'].append({'binary': cargo_binary(path), **data})
                print(render_cargo(data))
    except (ValueError, OSError, UnicodeError, KeyError) as error:
        print(f'rank: {error}', file=sys.stderr)
        return 2
    if args.json:
        record['cargo'].sort(key=lambda entry: -entry['case_sum_ns'])
        args.json.write_text(json.dumps(record, indent=2, sort_keys=True) + '\n')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
