#!/usr/bin/env python3
import argparse
import csv
import pathlib
import re
import subprocess
import sys

sys.dont_write_bytecode = True
from cohort import inventory

parser = argparse.ArgumentParser()
parser.add_argument('original', type=pathlib.Path)
parser.add_argument('plan', type=pathlib.Path)
args = parser.parse_args()
assert not args.plan.exists(), args.plan
repo = pathlib.Path(__file__).resolve().parents[3]
rows = list(csv.reader((repo / 'dev/consolidation/typing-corpus/retained.tsv').open(), delimiter='\t'))
retained = {int(row[0]) for row in rows if row[0].isdigit()}
retry = []
for index, domain, relative, row, category, reason in inventory(repo, args.original):
    if category != 'eligible-local' or not row[10].startswith('var '):
        continue
    source = (args.original / domain / relative).read_bytes()
    start, end = map(int, row[5].split('..'))
    owner_start, owner_end = map(int, row[6].split('..'))
    if source[start:end].startswith(b'Set[') and re.fullmatch(
        rb'var\s+[A-Za-z_][A-Za-z_0-9]*\s*:\s*Set\[[^\n]+\]\s*=\s*set\.empty\(\)\s*',
        source[owner_start:owner_end],
    ):
        retry.append(index)
assert len(retry) == 109 and set(retry) <= retained
args.plan.mkdir()
retained_path = args.plan / 'retained.tsv'
with retained_path.open('w') as stream:
    csv.writer(stream, delimiter='\t', lineterminator='\n').writerows(
        row for row in rows if not row[0].isdigit() or int(row[0]) not in retry)
print('additional mutable Set sites\t' + ','.join(map(str, retry)), flush=True)
tools = pathlib.Path(__file__).resolve().parent
subprocess.run([sys.executable, str(tools / 'cohort.py'), str(args.original),
                '--reduced', str(args.plan / 'candidate'), '--retain', str(retained_path)], check=True)
subprocess.run([sys.executable, str(tools / 'validate.py'), str(args.original),
                str(args.plan / 'candidate'), '--retain', str(retained_path)], check=True)
