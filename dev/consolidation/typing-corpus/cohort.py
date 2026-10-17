#!/usr/bin/env python3
import argparse
import collections
import csv
import pathlib
import shutil

XSH_PIN = 'ddc17d3d53c819c343f9ecaa738ac67d59e9033a'
LAPUTA_PIN = 'ca658ce1da9d89c55209c25268dfdf687ff4fdc6'
ROOTS = ('core/wc.xsh', 'core/cut.xsh', 'core/tr.xsh', 'core/date.xsh',
         'core/lib/date_parse.xsh', 'core/system-report.xsh', 'dev/gates.xsh',
         'dev/internal.xsh', 'dev/docker.xsh', 'dev/main.xsh')
VALIDATION_PARAMETERS = {
    'pure encode_typed_report_json parameter report',
    'pure render_typed_text parameter report',
    'pure compare_cpu_id_sets parameter candidate',
    'pure ip_address_scope parameter value',
    'pure ip_rule_hex parameter value',
    'pure ip_route_flags parameter names',
    'pure parse_major_minor_reference parameter value',
}


def inventory(repo, originals):
    path = repo / 'dev/consolidation/evidence/typing-cohort-inventory.tsv'
    with path.open() as stream:
        sites = [r for r in csv.reader(stream, delimiter='\t')
                 if r[0] == 'site' and r[1].isdigit()]
    assert len(sites) == 9194
    for index, row in enumerate(sites):
        domain = 'laputa' if 'typing-cohort-laputa/' in row[2] else 'xsh'
        relative = row[2].split('typing-cohort-' + domain + '/')[1]
        path = originals / domain / relative
        source = path.read_bytes()
        start, end = map(int, row[5].split('..'))
        assert 0 <= start < end <= len(source), (index, path)
        reason = ''
        if row[4] == 'domain-wire-schema':
            reason = 'schema definition; no field or declaration erasure'
            category = 'schema'
        elif row[4] == 'exported-interface':
            reason = 'unchanged exported signature, schema interface or module contract'
            category = 'interface'
        elif row[10] == 'proc main parameter argv' or row[10] == 'proc main parameter raw':
            reason = 'entry argument binding selects byte or text decoding'
            category = 'required'
        elif row[10] == 'proc main effects':
            reason = 'retain explicit entry effect upper bound; not private helper inference'
            category = 'entry-effect'
        elif row[10] in VALIDATION_PARAMETERS:
            reason = 'parameter supplies implicit require() runtime validation target'
            category = 'required'
        elif row[10] == 'proc grammar_reference return':
            reason = 'return supplies implicit require() runtime validation target'
            category = 'required'
        elif row[3] == 'local-type' and '.require()' in source[int(row[6].split('..')[0]):int(row[6].split('..')[1])].decode() and row[10] not in ('let rules', 'let mounts'):
            reason = 'local supplies implicit require() runtime validation target'
            category = 'required'
        else:
            category = 'eligible-local' if row[3] == 'local-type' else 'eligible-internal'
            reason = 'private implementation annotation; schema references remain eligible'
        yield (index, domain, relative, row, category, reason)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('originals', type=pathlib.Path)
    parser.add_argument('--freeze', type=pathlib.Path)
    parser.add_argument('--reduced', type=pathlib.Path)
    parser.add_argument('--retain', type=pathlib.Path)
    parser.add_argument('--refresh', action='store_true')
    args = parser.parse_args()
    repo = pathlib.Path(__file__).resolve().parents[3]
    sites = list(inventory(repo, args.originals))
    counts = collections.Counter(site[4] for site in sites)
    print('\n'.join(f'{key}\t{value}' for key, value in sorted(counts.items())))
    if args.freeze:
        with args.freeze.open('w') as stream:
            writer = csv.writer(stream, delimiter='\t', lineterminator='\n')
            writer.writerow(('site', 'project', 'path', 'line', 'kind', 'category', 'owner', 'reason'))
            for index, domain, relative, row, category, reason in sites:
                writer.writerow((index, domain, relative, row[8], row[3], category, row[10], reason))
    if args.reduced:
        retained = set()
        if args.retain:
            with args.retain.open() as stream:
                retained = {int(row[0]) for row in csv.reader(stream, delimiter='\t') if row and row[0].isdigit()}
        assert all(sites[index][4].startswith('eligible-') for index in retained)
        if args.refresh:
            assert args.reduced.is_dir()
        else:
            shutil.copytree(args.originals, args.reduced, symlinks=True, dirs_exist_ok=False)
        edits = collections.defaultdict(list)
        for index, domain, relative, row, category, reason in sites:
            edits[args.reduced / domain / relative]
        for index, domain, relative, row, category, reason in sites:
            if category.startswith('eligible-') and index not in retained:
                assert row[7] != '-'
                start, end = map(int, row[7].split('..'))
                edits[args.reduced / domain / relative].append((start, end, index))
        for path, spans in edits.items():
            source = (args.originals / path.relative_to(args.reduced)).read_bytes()
            boundary = len(source)
            for start, end, index in sorted(spans, reverse=True):
                assert end <= boundary, (path, index)
                source = source[:start] + source[end:]
                boundary = start
            path.write_bytes(source)
        print('removed\t' + str(sum(map(len, edits.values()))))


if __name__ == '__main__':
    main()
