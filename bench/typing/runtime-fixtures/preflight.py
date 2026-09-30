#!/usr/bin/env python3
"""Run isolated native observations against byte-frozen source variants."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

REPO = Path(__file__).resolve().parents[3]
FIXTURES = REPO / 'bench/typing/runtime-fixtures'
OUTPUT = REPO / 'bench/typing/baseline-runtime-extra-preflight.json'
BASELINE = REPO.parent / 'xsh-typing-baseline/target/release'
XSH = BASELINE / 'xsh'
XSHT = BASELINE / 'xsht'
TIMEOUT_SECONDS = 90

CASES = [
    {'id': 'native-system-report-bounded-number', 'source': 'tests/xsh/system-report.xsh',
     'test': 'test_system_report_bounded_number_respects_source_state_and_json_range',
     'observations': ['signed -5000 remains valid; unsigned -5000 is malformed',
                      '9007199254740991 remains representable; 9007199254740992 and integer overflow report range failure',
                      'nondecimal input is malformed; leading-zero decimal 007 gives 7',
                      'absent, truncated, and permission-denied source states retain their structured meanings'],
     'effects': 'loads frozen collectors/model modules; all observations are synthetic SourceRead values; no live host collection'},
    {'id': 'native-dev-report-reference', 'source': 'dev/tests/test-system-report-check.xsh',
     'test': 'test_system_report_lscpu_reference_parser_keeps_sparse_online_ids',
     'observations': ['synthetic JSON online CPU ids are exactly [0,65]',
                      'repeated CPU identity rejects with SystemReportCheckError.Invalid',
                      'wrong online field type rejects with schema error'],
     'effects': 'pure parsing of three inline JSON fixtures; no external lscpu invocation'},
    {'id': 'native-methods-aliases', 'source': 'tests/xsh/stdlib/methods.xsh',
     'test': 'test_list_concatenation_and_compound_assignment_preserve_aliases',
     'observations': ['list compound assignment preserves earlier aliases through append and self-concatenation',
                      'empty lists gain one fixed element type',
                      'record-field and typed Map[List[Int]] updates preserve aliases'],
     'effects': 'pure collection operations and native assertions'},
    {'id': 'native-stream-lifecycle', 'source': 'tests/xsh/stdlib/streams.xsh',
     'test': 'test_stream_producers_are_lazy_and_run_defers_on_stop',
     'observations': ['nested script stdout is exactly false\\n0\\nrow 0\\nclosed\\n',
                      'producer creation performs no write; first pull yields 0; stop runs defer and prevents later rows'],
     'effects': 'harness-owned temporary marker/row paths; nested script uses exact baseline xsh and cleans temporary resources'},
    {'id': 'native-process-capture', 'source': 'tests/xsh/run.xsh',
     'test': 'test_run_capture_record_captures_status_stdout_and_stderr',
     'observations': ['text capture exits with status7, stdout out, stderr err',
                      'byte capture has stdout length2 and one NUL byte on stderr'],
     'effects': 'isolated sh processes emit fixed bytes; head reads one constant byte from /dev/zero; no files or live Linux state'},
    {'id': 'native-local-inference', 'source': 'tests/xsh/local-inference.xsh',
     'test': 'empty_list_collects_loop_contributions_before_earlier_reads',
     'observations': ['nested script stdout is exactly second\\n',
                      'loop contributions establish one List[Path] lifetime type before earlier alias reads'],
     'effects': 'harness-owned nested script executed by exact baseline xsh; no filesystem effects beyond harness temporary script'},
    {'id': 'native-result-context', 'source': 'tests/xsh/result-contexts.xsh',
     'test': 'test_result_return_match_preserves_constructor_context',
     'observations': ['nested script stdout is exactly true\\ntrue\\n7\\n11\\ntrue\\n13\\n',
                      'Result-returning match preserves numeric tag conversion, bare success wrapping, explicit Err and Ok, and block returns'],
     'effects': 'harness-owned nested script executed by exact baseline xsh; fixed in-memory JSON literals'},
]


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def file_record(path):
    raw = path.read_bytes()
    return {'path': path.relative_to(REPO).as_posix(), 'bytes': len(raw), 'sha256': digest(raw)}


def save(data):
    OUTPUT.write_text(json.dumps(data, indent=2, ensure_ascii=False) + '\n')


def execute(argv, cwd, environment, stem):
    started = datetime.now(timezone.utc).isoformat()
    beginning = time.perf_counter_ns()
    process = subprocess.Popen(argv, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    timeout = False
    try:
        stdout, stderr = process.communicate(timeout=TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        timeout = True
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
    elapsed = time.perf_counter_ns() - beginning
    stem.parent.mkdir(parents=True, exist_ok=True)
    stdout_path = stem.with_suffix('.stdout')
    stderr_path = stem.with_suffix('.stderr')
    stdout_path.write_bytes(stdout)
    stderr_path.write_bytes(stderr)
    return {'command': argv, 'cwd': str(cwd), 'environment_overlay': environment_overlay(cwd),
            'started_utc': started, 'elapsed_ns': elapsed, 'exit_status': process.returncode,
            'timeout': timeout, 'children_killed_and_reaped_on_timeout': timeout,
            'stdout': file_record(stdout_path), 'stderr': file_record(stderr_path)}, stdout, stderr


def environment_overlay(cwd):
    return {'XSH_MODULE_PATH': os.pathsep.join([str(cwd), str(cwd / 'dev')]),
            'CARGO_BIN_EXE_xsh': str(XSH), 'CARGO_BIN_EXE_xsht': str(XSHT),
            'PATH': str(BASELINE) + os.pathsep + '/usr/bin:/bin', 'NO_COLOR': '1'}


def main():
    if OUTPUT.exists():
        raise RuntimeError('Preflight expectations are frozen; preserve the existing result instead of overwriting it.')
    inputs = [file_record(path) for path in sorted((FIXTURES / 'tokei-input').rglob('*')) if path.is_file()]
    identities = {name: {'path': str(path), 'sha256': digest(path.read_bytes())} for name, path in [('xsh', XSH), ('xsht', XSHT)]}
    data = {'schema_version': 1, 'status': 'running', 'scope': 'Gate A supplemental baseline runtime preflight only',
            'compiler_source_changed': False, 'frozen_cohort_or_eligibility_changed': False,
            'binary_identities': identities, 'timeout_seconds_per_process': TIMEOUT_SECONDS,
            'cli_contract': {'usage': 'xsht test --jobs 1 --exact MODULE.xsh::NAME',
                             'no_source_or_filter_options': True,
                             'cwd_policy': 'Each variant uses its frozen cwd because xsht discovers test_roots from cwd/xsht-config.ini; final baseline/candidate comparisons use the stabilized cwd for both products.',
                             'exactly_one_native_case_required': True},
            'sampling': {'kind': 'untimed-acceptance-preflight-with-elapsed-observation', 'samples_per_variant_case': 1,
                         'performance_acceptance': False},
            'inputs': inputs, 'selected_native_cases': CASES,
            'runs': [], 'comparisons': [], 'failures': []}
    save(data)
    duration_pattern = r'(?:[0-9]+ms|[0-9]+(?:\.[0-9])?s|[0-9]+m(?:[0-9]+s)?|[0-9]+h(?:[0-9]+m)?)'
    for case in CASES:
        selector = case['source'] + '::' + case['test']
        pattern = r'\Arunning 1 tests\n' + re.escape(selector) + r' \.\.\. ok ' + duration_pattern + r'\ntest result: ok\. 1 passed; 0 failed; 0 skipped\n\Z'
        expected = {'exit_status': 0, 'stdout_pattern': pattern, 'stderr_sha256': digest(b''),
                    'native_case_count': 1, 'passed': 1, 'failed': 0, 'skipped': 0,
                    'semantic_oracle': 'The exact native test body independently asserts the observations listed in selected_native_cases.'}
        normalized = []
        for variant in ['original', 'stabilized']:
            cwd = REPO / 'bench/typing/cohort' / variant
            environment = dict(os.environ)
            environment.update(environment_overlay(cwd))
            argv = [str(XSHT), 'test', '--jobs', '1', '--exact', selector]
            run, stdout, stderr = execute(argv, cwd, environment, FIXTURES / 'raw' / case['id'] / variant)
            run.update({'workload_id': case['id'], 'variant': variant, 'kind': 'native-fixture',
                        'source_sha256': digest((cwd / case['source']).read_bytes()),
                        'config_sha256': digest((cwd / 'xsht-config.ini').read_bytes()), 'expected': expected})
            matched = bool(re.fullmatch(pattern, stdout.decode('utf-8', errors='replace')))
            run['status'] = 'passed' if run['exit_status'] == 0 and not run['timeout'] and not stderr and matched else 'failed'
            run['stdout_pattern_matched'] = matched
            if run['status'] == 'failed':
                data['failures'].append({'workload_id': case['id'], 'variant': variant, 'reason': 'exact native acceptance oracle failed', 'stdout': stdout.decode('utf-8', errors='replace'), 'stderr': stderr.decode('utf-8', errors='replace')})
            normalized.append(re.sub(duration_pattern + r'(?=\n)', '<elapsed>', stdout.decode('utf-8', errors='replace'), count=1))
            data['runs'].append(run)
            save(data)
            print(case['id'], variant, run['status'], round(run['elapsed_ns']/1e9, 3), flush=True)
        equal = normalized[0] == normalized[1]
        data['comparisons'].append({'workload_id': case['id'], 'status': 'passed' if equal else 'failed',
                                    'oracle': 'stdout equality after replacing only the single selected native test elapsed duration',
                                    'normalized_stdout_sha256': digest(normalized[0].encode()) if equal else None})
    outputs = []
    for variant in ['original', 'stabilized']:
        cwd = REPO / 'bench/typing/cohort' / variant
        environment = dict(os.environ)
        environment.update(environment_overlay(cwd))
        argv = [str(XSH), str(cwd / 'showcase/tokei.xsh'), '--', '--json', str(FIXTURES / 'tokei-input')]
        run, stdout, stderr = execute(argv, cwd, environment, FIXTURES / 'raw/tokei-tiny' / variant)
        run.update({'workload_id': 'showcase-tokei-tiny', 'variant': variant, 'kind': 'runtime-program',
                    'source_sha256': digest((cwd / 'showcase/tokei.xsh').read_bytes())})
        decoded = None
        if run['exit_status'] == 0 and not stderr and not run['timeout']:
            try:
                decoded = json.loads(stdout)
                assert set(decoded) == {'JSON', 'Rust', 'Markdown', 'Total'}, set(decoded)
                assert len(decoded['Rust']['reports']) == 1
                assert len(decoded['JSON']['reports']) == 1
                assert len(decoded['Markdown']['reports']) == 1
                assert decoded['Rust']['code'] == 3 and decoded['Rust']['comments'] == 1 and decoded['Rust']['blanks'] == 1
                assert decoded['JSON']['code'] == 1
                assert decoded['Markdown']['children']['Rust'][0]['stats']['code'] == 1
                assert decoded['Total']['code'] == 5 and decoded['Total']['comments'] == 4 and decoded['Total']['blanks'] == 2
                assert b'skip.rs' not in stdout and b'.hidden.rs' not in stdout
            except (ValueError, KeyError, AssertionError) as error:
                run['oracle_failure'] = str(error)
                decoded = None
        run['status'] = 'passed' if decoded is not None else 'failed'
        run['expected'] = {'exit_status': 0, 'stderr_sha256': digest(b''), 'stdout_sha256': digest(stdout) if decoded is not None else None,
                           'semantic_assertions': {'top_level_languages': ['JSON','Rust','Markdown','Total'],
                                                   'rust': {'code':3,'comments':1,'blanks':1,'reports':1},
                                                   'json': {'code':1,'reports':1},
                                                   'markdown_rust_child_code':1,
                                                   'total': {'code':5,'comments':4,'blanks':2},
                                                   'hidden_and_ignored_sources_absent':True}}
        if decoded is not None:
            expected_path = FIXTURES / f'tokei-{variant}-expected.json'
            expected_path.write_text(json.dumps(decoded, indent=2, ensure_ascii=False) + '\n')
            run['decoded_stdout'] = file_record(expected_path)
        else:
            data['failures'].append({'workload_id':'showcase-tokei-tiny','variant':variant,'reason':'fixed tiny-input JSON observation oracle failed','stdout':stdout.decode('utf-8',errors='replace'),'stderr':stderr.decode('utf-8',errors='replace')})
        outputs.append(stdout)
        data['runs'].append(run)
        save(data)
        print('showcase-tokei-tiny', variant, run['status'], round(run['elapsed_ns']/1e9,3), flush=True)
    data['comparisons'].append({'workload_id':'showcase-tokei-tiny','status':'passed' if outputs[0]==outputs[1] else 'failed',
                                'oracle':'exact stdout bytes for the same frozen tiny directory','stdout_sha256':digest(outputs[0]) if outputs[0]==outputs[1] else None})
    final_inputs = [file_record(path) for path in sorted((FIXTURES / 'tokei-input').rglob('*')) if path.is_file()]
    data['inputs_unchanged'] = inputs == final_inputs
    data['binary_identities_unchanged'] = all(digest((BASELINE/name).read_bytes())==record['sha256'] for name,record in identities.items())
    data['status'] = 'passed' if not data['failures'] and all(c['status']=='passed' for c in data['comparisons']) and data['inputs_unchanged'] and data['binary_identities_unchanged'] else 'failed'
    data['completed_utc'] = datetime.now(timezone.utc).isoformat()
    save(data)
    print('overall',data['status'],flush=True)


if __name__ == '__main__':
    main()
