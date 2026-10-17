import collections
import hashlib
import importlib.util
import pathlib
import subprocess
import sys

sys.dont_write_bytecode = True
lane = pathlib.Path('/home/josh/d/laputa-systems/xsh/.work/wt/typing-corpus-b235')
repo = pathlib.Path('/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-retry-seal')
originals = lane / '.work/tmp/corpus/original'
path = repo / 'dev/consolidation/evidence/typing-cohort-inventory.tsv'
expected_blob = 'e67dd0f609d8fe2384532f7a2cc3273b43feacb6'
assert subprocess.check_output(['git', 'hash-object', str(path)], text=True).strip() == expected_blob
helper = lane / 'dev/consolidation/typing-corpus/view_digest.py'
before = subprocess.check_output([sys.executable, str(helper), str(originals)], text=True)
assert before.strip().split('\t')[1:] == ['14287', 'e3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899']
spec = importlib.util.spec_from_file_location('restored_cohort', repo / 'dev/consolidation/typing-corpus/cohort.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
sites = list(module.inventory(repo, originals))
counts = collections.Counter(site[4] for site in sites)
expected = {'eligible-local': 693, 'eligible-internal': 2190, 'schema': 4364, 'interface': 1920, 'required': 23, 'entry-effect': 4}
assert len(sites) == 9194
assert dict(counts) == expected, counts
assert {site[0] for site in sites} == set(range(9194))
assert len({(site[1], site[2]) for site in sites}) == 48
after = subprocess.check_output([sys.executable, str(helper), str(originals)], text=True)
assert before == after
print('inventory blob', expected_blob)
print('site count', len(sites))
print('source count', len({(site[1], site[2]) for site in sites}))
for key, value in sorted(counts.items()):
    print(key, value, sep='\t')
print('original before', before.strip(), sep='\t')
print('original after', after.strip(), sep='\t')
print('PASS: actual restored cohort.inventory; frozen categories and original input unchanged')
