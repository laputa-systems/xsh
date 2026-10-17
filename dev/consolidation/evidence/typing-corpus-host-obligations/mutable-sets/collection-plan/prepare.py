import csv
import pathlib
import subprocess
import sys

sys.dont_write_bytecode = True
repo = pathlib.Path('/home/josh/d/laputa-systems/xsh/.work/wt/typing-corpus-b235')
tools = repo / 'dev/consolidation/typing-corpus'
sys.path.insert(0, str(tools))
from cohort import inventory

lists = {1396,1983,1987,2007,2008,2280,2285,2362,2366,2418,2423,2424,2427,2429,2455,2542,2546,2651,2657,2658,2659,2707,2709,2935,3021,3054,6339,6370,6826,7024,7084,7184,7234,7235,7247,7347,7366,7385,7400,7418,7426,7433,7481,7550,7760,7821,7968,8236,8321,8407,8454,9110,9153,9165}
branches = {1929,2288,2332,2446,2538,2543,2544,3052,3406,3407,3476,6286,6372,6373,6374,6375,6436,6564,7427,7822,7823,2963,3020,6338,6371}
retry = lists | branches
assert len(lists) == 54 and len(branches) == 25 and len(retry) == 79
original = repo / '.work/tmp/corpus/original'
plan = repo / '.work/tmp/corpus/collection-plan'
rows = list(csv.reader((repo / '.work/tmp/corpus/mutable-set-plan/retained.tsv').open(), delimiter='\t'))
retained = {int(row[0]) for row in rows if row[0].isdigit()}
assert retry <= retained
sites = list(inventory(repo, original))
maps = set()
for index, domain, relative, row, category, reason in sites:
    if category != 'eligible-local':
        continue
    source = (original / domain / relative).read_bytes()
    start, end = map(int, row[5].split('..'))
    a, b = map(int, row[6].split('..'))
    if row[10].startswith('var ') and source[start:end].startswith(b'Map[') and source[a:b].rstrip().endswith(b'= {}'):
        maps.add(index)
assert len(maps) == 149 and maps <= retained and not maps & retry
assert all(sites[index][4] == 'eligible-local' for index in retry)
plan.mkdir()
with (plan / 'retained.tsv').open('w') as stream:
    csv.writer(stream, delimiter='\t', lineterminator='\n').writerows(
        row for row in rows if not row[0].isdigit() or int(row[0]) not in retry)
for kind, ids in [('List54', lists), ('branch-null25', branches)]:
    print(kind, ','.join(map(str, sorted(ids))), sep='\t', flush=True)
print('required Map{} annotations retained', len(maps), sep='\t', flush=True)
subprocess.run([sys.executable, str(tools / 'cohort.py'), str(original), '--reduced', str(plan / 'candidate'), '--retain', str(plan / 'retained.tsv')], check=True)
subprocess.run([sys.executable, str(tools / 'validate.py'), str(original), str(plan / 'candidate'), '--retain', str(plan / 'retained.tsv')], check=True)
