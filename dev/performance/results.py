import argparse
from collections import Counter
import json
import os
from pathlib import Path
import stat
import sys
from urllib.parse import quote_from_bytes


OUTCOMES = ('passed', 'failed', 'skipped', 'timed_out', 'cancelled', 'not_executed')


class ResultError(ValueError):
    pass


def _require(condition, message):
    if not condition:
        raise ResultError(message)


def _object(pairs):
    result = {}
    for key, value in pairs:
        _require(key not in result, f'duplicate field: {key}')
        result[key] = value
    return result


def _records(text):
    _require(bool(text) and text.endswith('\n'), 'truncated or empty event stream')
    try:
        rows = [json.loads(line, object_pairs_hook=_object) for line in text.splitlines()]
    except json.JSONDecodeError as error:
        raise ResultError(f'malformed event stream: {error}') from error
    _require(all(isinstance(row, dict) for row in rows), 'events must be objects')
    return rows


def _ids(selected):
    _require(bool(selected) and all(isinstance(name, str) and name for name in selected), 'selected IDs must be nonempty strings')
    _require(len(set(selected)) == len(selected), 'duplicate selected ID')
    return tuple(sorted(selected))


def _case_file(path):
    limit = 16 * 1024 * 1024
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    with os.fdopen(descriptor, 'rb') as stream:
        _require(stat.S_ISREG(os.fstat(stream.fileno()).st_mode), 'selected case file must be a regular file')
        data = stream.read(limit + 1)
    _require(len(data) <= limit, 'selected case file exceeds 16 MiB')
    value = json.loads(data.decode('utf-8'), object_pairs_hook=_object)
    _require(isinstance(value, list), 'selected case file must be a JSON array')
    return value


def _integer(value):
    return type(value) is int and value >= 0


def _reason(value):
    return (isinstance(value, dict) and isinstance(value.get('kind'), str)
            and bool(value['kind']) and isinstance(value.get('detail'), str))


def _fields(row, extra):
    _require(set(row) == {'schema', 'run_id', 'event', *extra}, 'missing or unknown native event fields')


def discover_cargo(text):
    rows = _records(text)
    _require(rows[0].get('type') == 'suite' and rows[0].get('event') == 'discovery', 'missing Cargo discovery start')
    _require(len(rows) >= 2 and rows[-1].get('type') == 'suite' and rows[-1].get('event') == 'completed',
             'missing Cargo discovery completion')
    selected = []
    ignored = 0
    for row in rows[1:-1]:
        _require(row.get('type') == 'test' and row.get('event') == 'discovered', 'unexpected Cargo discovery event')
        name = row.get('name')
        _require(isinstance(name, str) and bool(name) and '\n' not in name and '\r' not in name, 'invalid Cargo test name')
        _require(type(row.get('ignore')) is bool, 'invalid Cargo ignore status')
        selected.append(name)
        ignored += row['ignore']
    selected = _ids(selected)
    summary = rows[-1]
    _require(type(summary.get('tests')) is int and summary['tests'] == len(selected)
             and type(summary.get('benchmarks')) is int and summary['benchmarks'] == 0
             and type(summary.get('total')) is int and summary['total'] == len(selected)
             and type(summary.get('ignored')) is int and summary['ignored'] == ignored,
             'Cargo discovery counts disagree')
    return selected


def _reconcile_xsht(text, selected, *, child_status):
    selected = _ids(selected)
    rows = _records(text)
    states = dict.fromkeys(selected, 'pending')
    cases = {}
    run_id = None
    summary = None
    for index, row in enumerate(rows):
        _require(type(row.get('schema')) is int and row['schema'] == 1, 'unsupported XSHT event schema')
        _require(isinstance(row.get('run_id'), str) and bool(row['run_id']), 'missing run identity')
        event = row.get('event')
        if index == 0:
            _require(event == 'run_started', 'missing initial run_started')
            _fields(row, ('selected',))
            _require(isinstance(row.get('selected'), list) and _ids(row['selected']) == selected, 'selected scope differs from runner')
            run_id = row['run_id']
            continue
        _require(row['run_id'] == run_id, 'mixed run identities')
        _require(summary is None, 'event after final summary')
        if event in ('case_started', 'case_finished'):
            name = row.get('case_id')
            _require(isinstance(name, str) and name in states, 'unselected case result')
            if event == 'case_started':
                _fields(row, ('case_id',))
                _require(states[name] == 'pending', f'duplicate or out-of-order start: {name}')
                states[name] = 'running'
            else:
                _fields(row, ('case_id', 'outcome', 'reason', 'duration_ns'))
                outcome = row.get('outcome')
                _require(outcome in OUTCOMES, f'unknown terminal outcome: {name}')
                _require((states[name] == 'running' and outcome != 'not_executed')
                         or (states[name] == 'pending' and outcome == 'not_executed'), f'duplicate or out-of-order terminal: {name}')
                _require(_integer(row.get('duration_ns')), f'invalid duration: {name}')
                _require(row.get('reason') is None if outcome == 'passed' else _reason(row.get('reason')), f'invalid terminal reason: {name}')
                states[name] = 'finished'
                cases[name] = {key: row[key] for key in ('outcome', 'reason', 'duration_ns')}
        elif event == 'run_finished':
            _fields(row, ('status', 'reason', 'selected', 'completed', 'counts', 'duration_ns'))
            _require(all(state == 'finished' for state in states.values()), 'missing mandatory terminal result')
            _require(type(row.get('selected')) is int and type(row.get('completed')) is int
                     and row['selected'] == len(selected) and row['completed'] == len(selected), 'summary scope count mismatch')
            counts = Counter(case['outcome'] for case in cases.values())
            _require(row.get('counts') == {outcome: counts[outcome] for outcome in OUTCOMES}, 'summary outcome counts mismatch')
            _require(all(type(value) is int for value in row['counts'].values()), 'invalid summary count')
            _require(_integer(row.get('duration_ns')), 'invalid run duration')
            _require(type(row.get('status')) is int and row['status'] in (0, 1, 2, 130), 'invalid runner status')
            _require(row.get('reason') is None or _reason(row['reason']), 'invalid run reason')
            summary = row
        else:
            raise ResultError(f'unknown or out-of-order event: {event}')
    _require(summary is not None, 'missing final summary')
    _require(type(child_status) is int and summary['status'] == child_status, 'child status contradicts summary')
    _require(child_status == 130 or all(case['outcome'] != 'cancelled' for case in cases.values()),
             'cancelled case contradicts run status')
    if summary['reason'] is not None:
        kind = summary['reason']['kind']
        case_failed = any(case['outcome'] in ('failed', 'timed_out') for case in cases.values())
        _require((kind in ('cleanup_failure', 'runner_failure') and
                  (child_status == 2 or child_status == 1 and case_failed)) or
                 {'coverage_failure': 1, 'cancellation': 130}.get(kind) == child_status,
                 'run reason contradicts status')
    _require(child_status != 0 or all(case['outcome'] == 'passed' for case in cases.values()), 'successful summary has unresolved required results')
    _require(child_status != 0 or summary['reason'] is None, 'successful summary has unresolved run reason')
    _require(child_status == 0 or summary['reason'] is not None or any(case['outcome'] != 'passed' for case in cases.values()),
             'nonzero status has no unresolved result or run reason')
    _require(child_status != 2 or summary['reason'] is not None
             or not any(case['outcome'] in ('failed', 'timed_out') for case in cases.values()),
             'incomplete status contradicts a failed case')
    return _result('xsht', selected, cases, child_status, summary['reason'])


def _result(runner, selected, cases, child_status, reason):
    outcomes = {case['outcome'] for case in cases.values()}
    status = (130 if child_status == 130 or 'cancelled' in outcomes else
              1 if outcomes & {'failed', 'timed_out'} or child_status == 1 else
              2 if child_status != 0 or outcomes - {'passed'} else 0)
    return {'runner': runner, 'selected': list(selected), 'cases': {name: cases[name] for name in selected},
            'child_status': child_status, 'status': status, 'reason': reason,
            'coverage_complete': True, 'unreconciled_selected': []}


def _reconcile_cargo(text, selected, *, child_status):
    selected = _ids(selected)
    rows = _records(text)
    _require(rows[0].get('type') == 'suite' and rows[0].get('event') == 'started', 'missing Cargo suite start')
    _require(type(rows[0].get('test_count')) is int and rows[0]['test_count'] == len(selected), 'Cargo selected count mismatch')
    states = dict.fromkeys(selected, 'pending')
    cases = {}
    summary = None
    for row in rows[1:]:
        _require(summary is None, 'event after Cargo summary')
        if row.get('type') == 'test':
            name, event = row.get('name'), row.get('event')
            _require(isinstance(name, str) and name in states, 'unselected Cargo test')
            if event == 'started':
                _require(states[name] == 'pending', 'duplicate Cargo start')
                states[name] = 'running'
                continue
            if event == 'timeout':
                _require(states[name] == 'running', 'Cargo timeout progress without a running test')
                continue
            _require(event in ('ok', 'failed', 'ignored'), 'unknown Cargo test outcome')
            _require(isinstance(row.get('stdout', ''), str), 'invalid Cargo output')
            _require(states[name] == 'running' or (event == 'ignored' and states[name] == 'pending'), 'out-of-order Cargo terminal')
            states[name] = 'finished'
            cases[name] = {'outcome': {'ok': 'passed', 'failed': 'failed', 'ignored': 'skipped'}[event],
                           'reason': None if event == 'ok' else {'kind': event, 'detail': row.get('stdout', '')},
                           'duration_ns': None}
        elif row.get('type') == 'suite' and row.get('event') in ('ok', 'failed'):
            _require(all(state == 'finished' for state in states.values()), 'missing Cargo terminal result')
            counts = Counter(case['outcome'] for case in cases.values())
            for key, outcome in (('passed', 'passed'), ('failed', 'failed'), ('ignored', 'skipped')):
                _require(type(row.get(key)) is int and row[key] == counts[outcome], 'Cargo summary count mismatch')
            _require(row['event'] == ('failed' if counts['failed'] else 'ok'), 'Cargo summary contradicts results')
            summary = row
        else:
            raise ResultError('unknown Cargo event')
    _require(summary is not None, 'missing Cargo final summary')
    _require(type(child_status) is int and (child_status == 0) == (summary['event'] == 'ok'), 'Cargo child status contradicts summary')
    return _result('cargo', selected, cases, child_status, None)


def _reconcile(runner, parse, text, selected, *, child_status, cancelled):
    selected = _ids(selected)
    _require(type(child_status) is int and type(cancelled) is bool, 'invalid controller result status')
    cancellation = cancelled or child_status == 130
    child_failure = child_status not in (0, 130) and (runner == 'cargo' or child_status != 2)
    try:
        result = parse(text, selected, child_status=child_status)
    except ResultError as error:
        if not cancellation and not child_failure:
            raise
        return {'runner': runner, 'selected': list(selected), 'cases': {}, 'child_status': child_status,
                'status': 130 if cancellation else 1,
                'reason': {'kind': 'cancellation', 'detail': 'controller observed user cancellation'} if cancellation else
                          {'kind': 'child_failure', 'detail': f'required {runner} invocation exited {child_status}'},
                'coverage_complete': False, 'unreconciled_selected': list(selected),
                'coverage_reason': {'kind': 'unreconciled_results', 'detail': str(error)}}
    if cancellation:
        result['status'] = 130
        if result['reason'] is None:
            result['reason'] = {'kind': 'cancellation', 'detail': 'controller observed user cancellation'}
    return result


def reconcile_xsht(text, selected, *, child_status, cancelled=False):
    return _reconcile('xsht', _reconcile_xsht, text, selected, child_status=child_status, cancelled=cancelled)


def reconcile_cargo(text, selected, *, child_status, cancelled=False):
    return _reconcile('cargo', _reconcile_cargo, text, selected, child_status=child_status, cancelled=cancelled)


def render_report(result, *, artifact=None):
    counts = Counter(case['outcome'] for case in result['cases'].values())
    verdict = {0: 'passed', 1: 'failed', 2: 'incomplete', 130: 'cancelled'}[result['status']]
    def detail(value):
        text = json.dumps(value, sort_keys=True)
        if artifact is not None and len(text) > 240:
            return json.dumps({'kind': value['kind'], 'detail': value['detail'][:160] + '...'}, sort_keys=True)
        return text
    unresolved = [f'{json.dumps(name)}: {case["outcome"]} {detail(case["reason"])}'
                  for name, case in result['cases'].items() if case['outcome'] != 'passed']
    if artifact is not None and len(unresolved) > 3:
        unresolved = unresolved[:3] + [f'... {len(unresolved) - 3} more cases']
    if result['reason'] is not None:
        unresolved.append('run: ' + detail(result['reason']))
    elif result['child_status'] != 0:
        unresolved.append(f'runner exited {result["child_status"]}')
    coverage = 'complete' if result['coverage_complete'] else 'incomplete: ' + detail(result['coverage_reason'])
    lines = [f'Selected scope: {verdict} ({len(result["selected"])} cases; {result["runner"]})',
                      'Results: ' + (', '.join(f'{outcome}={counts[outcome]}' for outcome in OUTCOMES if counts[outcome]) or 'none reconciled'),
                      f'Child status: {result["child_status"]}',
                      'Coverage: ' + coverage,
                      'Unresolved: ' + ('; '.join(unresolved) if unresolved else 'none'),
                      'Full: not run']
    if artifact is not None:
        target = quote_from_bytes(os.fsencode(Path(artifact).absolute()), safe='/')
        lines.append(f'Details: [events]({target})')
    return '\n'.join(lines) + '\n'


def cargo_diagnostics(path):
    limit = 2 * 1024 * 1024
    with path.open('rb') as source:
        data = source.read(limit + 1)
    truncated = len(data) > limit
    data = data[:limit]
    lines = data.split(b'\n')
    if lines[-1]:
        lines.pop()
        truncated = True
    count = 0
    for line in lines:
        if not line.startswith(b'{'):
            continue
        try:
            record = json.loads(line)
        except (ValueError, UnicodeError, RecursionError):
            continue
        if not isinstance(record, dict) or record.get('reason') != 'compiler-message':
            continue
        message = record.get('message')
        rendered = message.get('rendered') if isinstance(message, dict) else None
        if not isinstance(rendered, str):
            continue
        print(rendered[:4096])
        count += 1
        if count == 16:
            print('Cargo diagnostics limited to 16 messages')
            return
    if truncated:
        print('Cargo diagnostic input limited to 2 MiB')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--runner', required=True, choices=('xsht', 'cargo'))
    parser.add_argument('--events', required=True, type=Path)
    parser.add_argument('--case', action='append')
    parser.add_argument('--cases-json', type=Path)
    parser.add_argument('--child-status', type=int)
    parser.add_argument('--cancelled', action='store_true')
    parser.add_argument('--no-artifact-link', action='store_true')
    parser.add_argument('--discover-cargo', action='store_true')
    parser.add_argument('--cargo-diagnostics', action='store_true')
    args = parser.parse_args()
    try:
        if args.cargo_diagnostics:
            _require(args.runner == 'cargo' and args.case is None and args.cases_json is None and args.child_status is None and not args.cancelled and not args.discover_cargo,
                     'Cargo diagnostics cannot include execution results')
            cargo_diagnostics(args.events)
            return 0
        if args.discover_cargo:
            _require(args.runner == 'cargo' and args.case is None and args.cases_json is None and args.child_status is None and not args.cancelled,
                     'Cargo discovery cannot include execution results')
            print('\n'.join(discover_cargo(args.events.read_text())))
            return 0
        _require((args.case is None) != (args.cases_json is None) and args.child_status is not None,
                 'result reconciliation requires one case source and child status')
        if args.cases_json is not None:
            args.case = _case_file(args.cases_json)
        reconcile = reconcile_xsht if args.runner == 'xsht' else reconcile_cargo
        read_error = None
        try:
            event_text = args.events.read_text()
        except (OSError, UnicodeError) as error:
            if not (args.cancelled or args.child_status == 130
                    or args.child_status not in (0, 130) and (args.runner == 'cargo' or args.child_status != 2)):
                raise
            event_text, read_error = '', {'kind': 'events_unavailable', 'detail': str(error)}
        result = reconcile(event_text, args.case, child_status=args.child_status, cancelled=args.cancelled)
        if read_error is not None:
            result['coverage_reason'] = read_error
        print(render_report(result, artifact=None if args.no_artifact_link else args.events), end='')
        return result['status']
    except (ValueError, OSError, UnicodeError) as error:
        print(f'Selected scope: incomplete\nReason: {json.dumps(str(error))}\nFull: not run', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
