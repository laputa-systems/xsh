import argparse
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FAMILIES = ('preparation and checking', 'text and filesystem processing', 'structured data',
            'build orchestration', 'parallel work', 'long-running services', 'deployment boundaries')

# Fields: family, id, definition, production activity, input regimes, oracle, strength, use, held out.
WORKLOADS = (
    ('preparation and checking', 'frontend-inference', 'dev/performance/inference_workloads.py:334', 'XSH checker and imported modules; representative of checking core and Laputa pm', '52 generated annotated and reduced cases, 64–512 units, chains, records, diamonds and generics', 'inference_measurement.validate_output checks status, exact stdout, stderr and effects; depth 512 expects a semantic rejection', 'strong', 'correctness and diagnostic; no accepted current timing', False),
    ('preparation and checking', 'tooling-check', 'dev/performance/qualified_bench.py:25', 'xsht check on core/lib/search.xsh, dhcp_frame.xsh and text_input.xsh', 'fixed three-file corpus; sequential', 'tooling_complete requires zero status, empty stdout and a check summary for each file; it does not compare semantic facts', 'weak', 'performance benchmark', False),
    ('text and filesystem processing', 'core-wc-{small,medium,large}', 'dev/performance/core_cases.py:72', 'core/wc.xsh line counting', '64 KiB, 1 MiB, 16 MiB; repeated text, NUL and invalid UTF-8', 'equivalent checks exact line count and path, zero status, no stderr, timeout or working-directory effects', 'strong', 'correctness and diagnostic; no accepted current timing', False),
    ('text and filesystem processing', 'core-sort-{small,medium,large}', 'dev/performance/core_cases.py:76', 'core/sort.xsh numeric key ordering', '256, 2048, 16384 reversed tab-separated records', 'equivalent checks all exact sorted bytes, status, stderr, timeout and working-directory effects', 'strong', 'correctness and diagnostic; no accepted current timing', False),
    ('text and filesystem processing', 'core-awk-{small,medium,large}', 'dev/performance/core_cases.py:81', 'core/awk.xsh grouped text aggregation', '256, 2048, 16384 reversed records across 13 groups', 'equivalent checks exact count, sum and two group counts, status, stderr, timeout and working-directory effects', 'strong', 'correctness and diagnostic; no accepted current timing', False),
    ('text and filesystem processing', 'core-traversal-{small,medium,large}', 'dev/performance/core_cases.py:86', 'fs.walk used by XSH filesystem scripts', '32, 256, 2048 files, hidden paths, ignore file and three links', 'equivalent checks ordered path, kind and file size for every entry, status, stderr, timeout and working-directory effects', 'strong', 'correctness and diagnostic; no accepted current timing', True),
    ('text and filesystem processing', 'consolidation-directory', 'dev/performance/workloads.py:61', 'bench/consolidation/xsh/directory.xsh package-tree scan', 'package fixtures at common and 16384–65536 row sizes; warm filesystem', 'equivalent checks exact fixture stdout, status, stderr, timeout and working-directory effects', 'strong', 'performance benchmark; demanding sizes unqualified', False),
    ('structured data', 'core-json-{small,medium,large}', 'dev/performance/core_cases.py:68', 'json.decode and typed records used in Laputa package metadata', '256, 2048, 16384 ordered records with index, label and modular values', 'fixture asserts each index, label and value; equivalent checks exact count and sum, status, stderr, timeout and effects', 'strong', 'correctness and diagnostic; no accepted current timing', False),
    ('structured data', 'consolidation-json', 'dev/performance/workloads.py:61', 'bench/consolidation/xsh/json.xsh package metadata processing', 'common and 16384–65536 synthetic package records with a duplicate and an insertion', 'equivalent checks exact fixture stdout, status, stderr, timeout and working-directory effects', 'strong', 'performance benchmark; demanding sizes unqualified', True),
    ('build orchestration', 'warm-rebuild', 'laputa:bench/registry.xsh:15', 'Laputa pm/execute.xsh rebuilds the baselayout and xsh closure', 'five rounds, warm artifacts, one build job', 'build_oracle requires nonempty plan, no build start, done or failure lines, and exactly one reuse-count line matching plan length', 'strong', 'performance benchmark', False),
    ('build orchestration', 'make-graph', 'laputa:bench/registry.xsh:11', 'Laputa packages/linux/kbuild.xsh and make task execution', 'synthetic small and medium kernel graphs with two workers, fresh, reused and one changed source', 'pyramid_graph_behavior checks task dependencies, outputs, reuse counts and changed-source invalidation; mutation calibration outstanding', 'weak', 'correctness and harness case', True),
    ('parallel work', 'kbuild-parallel-archive-analysis', 'dev/performance/laputa_plan.py:41', 'Laputa packages/linux/kbuild-archive-analysis-worker.xsh archive planning', 'three object inputs, serial versus two workers', 'native test compares archives, links, generated and missing objects; its archive equality rejects a dropped archive. Task ordering, arguments and dependencies are not mutation-calibrated', 'strong', 'correctness and harness case', True),
    ('parallel work', 'make-graph', 'laputa:packages/linux/tests/pyramid.xsh:200', 'Laputa make task execution and kernel archive graph', 'two workers; selected closure, full build, reuse, changed source', 'native test checks exact archive contents and per-task execution counts; mutation calibration outstanding', 'weak', 'correctness and harness case', True),
    ('long-running services', 'xinit-service-start', 'laputa:bench/registry.xsh:19', 'Laputa xinit service scanner starts a notify service', 'five rounds in a disposable container, one service', 'service_oracle checks old service down, new running and ready PID equals notifier; runner checks scanner and container cleanup', 'strong', 'performance benchmark', False),
    ('deployment boundaries', 'world-plan', 'laputa:bench/registry.xsh:14', 'Laputa pm catalog and repo plan', 'five offline world plans for x86_64-linux-musl', 'plan_oracle checks sorted package inventory and nonempty nodes, then stable plan digest across rounds; first digest is not independent', 'weak', 'performance benchmark', False),
    ('deployment boundaries', 'mirror-publish', 'laputa:bench/registry.xsh:17', 'Laputa mirror publishes a package plan', 'five rounds against a private mirror and existing artifacts', 'publish_oracle compares plan keys, row count, proof and fetched payload digests to local store; cleanup checked separately', 'weak', 'performance benchmark; mutation calibration outstanding', False),
    ('deployment boundaries', 'root-compose', 'laputa:bench/registry.xsh:16', 'Laputa pm/root.xsh composes a root', 'five rounds, baselayout and xsh closure, warm artifacts', 'root_oracle compares payload artifact keys and package names and requires nonempty entries; it does not compare every installed byte', 'weak', 'performance benchmark', False),
    ('deployment boundaries', 'profile-ready', 'laputa:bench/registry.xsh:18', 'Laputa profiles/qemu-tinyland-ghostty.xsh boot', 'five x86_64 KVM and virgl boots', 'boot_oracle checks readiness marker and absence of failure marker; process group cleanup checked separately', 'weak', 'performance benchmark', True),
)

SOURCE_FILES = (
    'core/wc.xsh', 'core/sort.xsh', 'core/awk.xsh', 'core/lib/search.xsh',
    'core/lib/dhcp_frame.xsh', 'core/lib/text_input.xsh', 'dev/performance/inference_measurement.py',
    'bench/consolidation/xsh/directory.xsh', 'bench/consolidation/xsh/json.xsh',
    'laputa:pm/execute.xsh', 'laputa:pm/root.xsh', 'laputa:packages/linux/kbuild.xsh',
    'laputa:packages/linux/kbuild-archive-analysis-worker.xsh', 'laputa:packages/linux/tests/pyramid.xsh',
    'laputa:packages/linux/tests/main.xsh', 'laputa:profiles/qemu-tinyland-ghostty.xsh',
    'laputa:bench/support.sh', 'laputa:bench/scenario.py',
)


# Fields: family, missing workload or oracle, why it matters.
GAPS = (
    ('preparation and checking', 'Semantic checking oracle for real corpora', 'A successful check summary does not prove inferred facts or diagnostics are correct.'),
    ('text and filesystem processing', 'Byte content and broad skew coverage', 'Traversal checks sizes and names rather than file contents; generated inputs do not represent all real distributions.'),
    ('structured data', 'Corruption preserving aggregate values', 'The JSON output check alone cannot detect compensating changes; the fixture assertions cover the generated records.'),
    ('build orchestration', 'Package build throughput', 'Warm reuse at one job does not measure building packages or concurrent throughput.'),
    ('build orchestration', 'Graph mutation calibration', 'The native graph oracle has not been fed a stale reuse mutant in this matrix.'),
    ('parallel work', 'Calibrated representative parallel benchmark', 'The archive equality rejects a dropped item, but task-order and graph checks lack mutation calibration and a performance baseline; upstream consumption, failure selection and cleanup need coverage.'),
    ('long-running services', 'Sustained load and shutdown', 'A one-service startup does not measure steady-state throughput, tail latency or long-lived resource cleanup.'),
    ('deployment boundaries', 'Install, remove and generation-switch latency', 'Root composition does not measure PM installation, removal or activation transitions.'),
    ('deployment boundaries', 'Independent world-plan and boot behavior oracles', 'A self-consistent plan digest and readiness marker can accept a consistently wrong plan or unusable system.'),
    ('deployment boundaries', 'Mirror and root mutation calibration', 'The available checks have not been tested here against dropped artifacts or incorrect installed content.'),
    ('preparation and checking', 'Historical held-out provenance', 'The held-out selections in this matrix are reserved for future tuning; prior tuning history is not established.'),
)


def validate(laputa=None):
    laputa = Path(laputa) if laputa is not None else None
    checked = 0
    unchecked = 0
    for source in SOURCE_FILES:
        external = source.startswith('laputa:')
        if external and laputa is None:
            unchecked += 1
            continue
        root = laputa if external else ROOT
        if not (root / source.removeprefix('laputa:')).is_file():
            raise ValueError(f'missing source file: {source}')
        checked += 1
    ids = set()
    for family, name, location, activity, regime, oracle, strength, use, held in WORKLOADS:
        if family not in FAMILIES or (family, name) in ids:
            raise ValueError(f'invalid family or duplicate workload: {family} {name}')
        ids.add((family, name))
        if not all((name, activity, regime, oracle, use)) or strength not in ('strong', 'weak') or type(held) is not bool:
            raise ValueError(f'incomplete workload: {name}')
        path, line = location.rsplit(':', 1)
        if not line.isdigit() or int(line) <= 0:
            raise ValueError(f'invalid source location: {location}')
        external = path.startswith('laputa:')
        if external and laputa is None:
            unchecked += 1
            continue
        root = laputa if external else ROOT
        target = root / path.removeprefix('laputa:')
        if not target.is_file() or int(line) > len(target.read_text().splitlines()):
            raise ValueError(f'invalid source location: {location}')
        checked += 1
    for family, missing, reason in GAPS:
        if family not in FAMILIES or not missing or not reason:
            raise ValueError(f'invalid gap: {family} {missing}')
    if set(FAMILIES) != {row[0] for row in WORKLOADS} | {row[0] for row in GAPS}:
        raise ValueError('family coverage incomplete')
    if laputa is not None:
        fixtures = ROOT / 'dev/performance/oracle_mutants'
        support = (laputa / 'bench/support.sh').read_text()
        native = (laputa / 'packages/linux/tests/main.xsh').read_text()
        scenario = (laputa / 'bench/scenario.py').read_text()
        start = native.index('    assert parallel.archives == serial.archives', native.index('test test_kbuild_parallel_archive_analysis_matches_serial'))
        copies = {
            'build_oracle.sh': support[support.index('build_oracle() {'):support.index('root_oracle() {')],
            'parallel_assertions.xsh': native[start:native.index('    assert parallel.tasks.len()', start)],
            'service_oracle.py': scenario[scenario.index('def service_oracle('):scenario.index('def publish_oracle(')].rstrip() + '\n',
        }
        for name, expected in copies.items():
            if (fixtures / name).read_text() != expected:
                raise ValueError(f'Laputa oracle fixture differs: {name}')
    return checked, unchecked


def summary():
    workloads = Counter(row[0] for row in WORKLOADS)
    gaps = Counter(row[0] for row in GAPS)
    weak = Counter(row[0] for row in WORKLOADS if row[6] == 'weak')
    held = Counter(row[0] for row in WORKLOADS if row[8])
    rows = ['| Family | Workloads | Weak oracles | Held out | Gaps |', '| --- | ---: | ---: | ---: | ---: |']
    rows.extend(f'| {family} | {workloads[family]} | {weak[family]} | {held[family]} | {gaps[family]} |' for family in FAMILIES)
    return '\n'.join(rows)


def markdown():
    rows = ['# Workload and oracle matrix', '', 'This reference maps existing workloads to production activities and their actual checks. Strong means the named check rejects a representative mutant, not that it proves all behavior. Weak includes uncalibrated checks. Held-out cases are reserved for future tuning, not a claim about past use. Performance benchmark identifies an accepted benchmark definition, not a current measurement.', '', 'Oracle fixtures copy the build, archive comparison and service checks from Laputa `bench/support.sh`, `packages/linux/tests/main.xsh` and `bench/scenario.py`; `--check --laputa PATH` checks their provenance against the checkout.', '', summary(), '', '## Workloads', '', '| Family | ID and definition | Production activity | Input regimes | Oracle | Strength | Use | Held out |', '| --- | --- | --- | --- | --- | --- | --- | --- |']
    for family, name, location, activity, regime, oracle, strength, use, held in WORKLOADS:
        rows.append(f'| {family} | {name} ({location}) | {activity} | {regime} | {oracle} | {strength} | {use} | {"yes" if held else "no"} |')
    rows.extend(['', '## Gaps', '', '| Family | Missing workload or oracle | Why it matters |', '| --- | --- | --- |'])
    rows.extend(f'| {family} | {missing} | {reason} |' for family, missing, reason in GAPS)
    return '\n'.join(rows) + '\n'


def main():
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('--check', action='store_true')
    group.add_argument('--markdown', action='store_true')
    parser.add_argument('--laputa', type=Path)
    args = parser.parse_args()
    checked, unchecked = validate(args.laputa)
    if args.check:
        print(summary())
        print(f'References checked: {checked}; Laputa references not checked: {unchecked}')
    else:
        print(markdown(), end='')


if __name__ == '__main__':
    main()
