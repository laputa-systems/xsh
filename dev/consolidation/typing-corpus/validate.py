#!/usr/bin/env python3
import argparse
import collections
import csv
import pathlib
import sys

sys.dont_write_bytecode = True

from cohort import inventory

repo = pathlib.Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument('original', type=pathlib.Path)
parser.add_argument('reduced', type=pathlib.Path)
parser.add_argument('--retain', type=pathlib.Path,
                    default=repo / 'dev/consolidation/typing-corpus/retained.tsv')
args = parser.parse_args()
original, reduced = args.original, args.reduced
with args.retain.open() as stream:
    retained = {int(row[0]) for row in csv.reader(stream, delimiter='\t') if row[0].isdigit()}
sites = list(inventory(repo, original))
removed = collections.defaultdict(list)
protected = collections.defaultdict(list)
counts = collections.Counter()
file_counts = collections.Counter()
for index, domain, relative, row, category, reason in sites:
    key = (domain, relative)
    if category.startswith('eligible-') and index not in retained:
        start, end = map(int, row[7].split('..'))
        removed[key].append((start, end))
        counts[category] += 1
        file_counts[(domain, relative, category)] += 1
    else:
        protected[key].append(tuple(map(int, row[5].split('..'))))
for domain, relative in {(site[1], site[2]) for site in sites}:
    source = (original / domain / relative).read_bytes()
    edits = removed[(domain, relative)]
    for start, end in edits:
        assert all(end <= a or b <= start for a, b in protected[(domain, relative)]), (relative, start, end)
    for start, end in sorted(edits, reverse=True):
        source = source[:start] + source[end:]
    assert source == (reduced / domain / relative).read_bytes(), relative
print('checked48loaded sources; all retained sites, schemas and exported interface spans protected')
for domain, relative, category in sorted(file_counts):
    print(domain, relative, category, file_counts[(domain, relative, category)], sep='\t')
for category, count in sorted(counts.items()):
    print('total', category, count, sep='\t')
