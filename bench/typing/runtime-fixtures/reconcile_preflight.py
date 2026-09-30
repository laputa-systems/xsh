#!/usr/bin/env python3
"""Retain an initial counting-oracle correction and freeze successful observations."""
import sys
sys.dont_write_bytecode = True
import json
import os
from pathlib import Path
import preflight as fixture


def main():
    data = json.loads(fixture.OUTPUT.read_text())
    if data.get('observation_workloads') or data.get('workloads'):
        raise RuntimeError('Supplemental observations are frozen.')
    if len(data['failures']) != 2 or any(failure['workload_id'] != 'showcase-tokei-tiny' for failure in data['failures']):
        raise RuntimeError('Only the documented initial tiny-input comment-count correction may be reconciled.')
    for failure in data['failures']:
        failure['classification'] = 'test-oracle-miscalculation'
        failure['resolution'] = 'A fenced Rust code line contributes to the Rust child code count, not Markdown comments; Total comments are4. Raw failed oracle records remain unchanged.'
    for run in data['runs']:
        run['attempt'] = 1
    data['oracle_corrections'] = [{'workload_id':'showcase-tokei-tiny','old_expected_total_comments':5,'correct_expected_total_comments':4,
                                  'reason':'README.md has three Markdown comment lines, one blank line, and one Rust child code line; main.rs has one additional comment.',
                                  'source_or_input_change':False,'compiler_change':False,'historical_records_retained':True}]
    outputs = []
    semantic = {'top_level_languages':['JSON','Rust','Markdown','Total'],
                'rust':{'code':3,'comments':1,'blanks':1,'reports':1},'json':{'code':1,'reports':1},
                'markdown':{'comments':3,'blanks':1,'reports':1},'markdown_rust_child_code':1,
                'total':{'code':5,'comments':4,'blanks':2},'hidden_and_ignored_sources_absent':True}
    for variant in ['original','stabilized']:
        cwd = fixture.REPO / 'bench/typing/cohort' / variant
        environment = dict(os.environ)
        environment.update(fixture.environment_overlay(cwd))
        argv = [str(fixture.XSH),str(cwd / 'showcase/tokei.xsh'),'--','--json',str(fixture.FIXTURES / 'tokei-input')]
        run,stdout,stderr = fixture.execute(argv,cwd,environment,fixture.FIXTURES / 'raw/tokei-tiny-retry' / variant)
        decoded = json.loads(stdout)
        assert run['exit_status'] == 0 and not run['timeout'] and not stderr
        assert set(decoded) == set(semantic['top_level_languages'])
        for language,expected in [('Rust',semantic['rust']),('JSON',semantic['json']),('Markdown',semantic['markdown'])]:
            assert len(decoded[language]['reports']) == expected['reports']
            assert all(decoded[language][key] == value for key,value in expected.items() if key != 'reports')
        assert decoded['Markdown']['children']['Rust'][0]['stats']['code'] == 1
        assert all(decoded['Total'][key] == value for key,value in semantic['total'].items())
        assert b'.hidden.rs' not in stdout and b'skip.rs' not in stdout
        expected_path = fixture.FIXTURES / f'tokei-{variant}-expected.json'
        expected_path.write_text(json.dumps(decoded,indent=2,ensure_ascii=False)+'\n')
        run.update({'workload_id':'showcase-tokei-tiny','variant':variant,'kind':'runtime-program','attempt':2,'status':'passed',
                    'source_sha256':fixture.digest((cwd / 'showcase/tokei.xsh').read_bytes()),
                    'expected':{'exit_status':0,'stderr_sha256':fixture.digest(b''),'stdout_sha256':fixture.digest(stdout),'semantic_assertions':semantic},
                    'decoded_stdout':fixture.file_record(expected_path)})
        data['runs'].append(run)
        outputs.append(stdout)
        print('showcase-tokei-tiny',variant,'passed',round(run['elapsed_ns']/1e9,3),flush=True)
    assert outputs[0] == outputs[1]
    data['comparisons'].append({'workload_id':'showcase-tokei-tiny','attempt':2,'status':'passed','oracle':'exact stdout bytes for unchanged tiny input after independently accounting for embedded child code','stdout_sha256':fixture.digest(outputs[0])})
    common = {'variant':'stabilized','cwd':'cohort/stabilized','timeout_seconds':fixture.TIMEOUT_SECONDS,
              'env':{'XSH_MODULE_PATH':'{root}/cohort/stabilized:{root}/cohort/stabilized/dev','NO_COLOR':'1'},
              'applicability':{'status':'applicable','reason':'Offline fixture with isolated effects; no live Linux execution.'}}
    data['observation_workloads'] = []
    for case in data['selected_native_cases']:
        run = next(run for run in data['runs'] if run['workload_id']==case['id'] and run['variant']=='stabilized')
        assert run['status'] == 'passed'
        data['observation_workloads'].append({**common,'id':case['id'],'kind':'native-compatibility-observation',
                                              'source':'cohort/stabilized/'+case['source'],
                                              'commandTemplate':['{xsht}','test','--jobs','1','--exact',case['source']+'::'+case['test']],
                                              'expected':{'exit_status':0,'stdout_pattern':run['expected']['stdout_pattern'],'stderr_sha256':fixture.digest(b'')},
                                              'asserted_observations':case['observations'],
                                              'execution_scope':case['effects'],
                                              'performance_acceptance':False,
                                              'nested_product_requirement':'The native harness must resolve xsh from the same delivered product set as xsht; supplemental baseline preflights pin CARGO_BIN_EXE_xsh to the immutable baseline executable.'})
    data['workloads'] = [{**common,'id':'runtime-tokei-tiny','kind':'runtime','source':'cohort/stabilized/showcase/tokei.xsh',
                          'commandTemplate':['{xsh}','{source}','--','--json','{root}/runtime-fixtures/tokei-input'],
                          'expected':{'exit_status':0,'stdout_sha256':fixture.digest(outputs[0]),'stderr_sha256':fixture.digest(b'')},
                          'semantic_assertions':semantic,'frozen_input_directory':'runtime-fixtures/tokei-input'}]
    final_inputs = [fixture.file_record(path) for path in sorted((fixture.FIXTURES/'tokei-input').rglob('*')) if path.is_file()]
    assert data['inputs'] == final_inputs
    assert all(fixture.digest((fixture.BASELINE/name).read_bytes())==record['sha256'] for name,record in data['binary_identities'].items())
    data['inputs_unchanged'] = True
    data['binary_identities_unchanged'] = True
    data['status'] = 'passed-with-retained-oracle-correction'
    data['accepted_native_runs'] = 14
    data['accepted_tokei_runs'] = 2
    data['retained_initial_oracle_failures'] = 2
    data['unresolved_failures'] = []
    data['frozen_expectation_sha256'] = fixture.digest(json.dumps({'observation_workloads':data['observation_workloads'],'workloads':data['workloads'],'inputs':data['inputs']},sort_keys=True,separators=(',',':')).encode())
    fixture.save(data)
    print('frozen: seven native observation workloads and one production runtime workload',flush=True)


if __name__ == '__main__':
    main()
