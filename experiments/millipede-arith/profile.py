#!/usr/bin/env python3
import os
import re
import subprocess
import time
from pathlib import Path

root = Path(__file__).resolve().parent
tmp = (root / '../../.work/tmp').resolve()
tmp.mkdir(parents=True, exist_ok=True)
source = (root / 'Proof.lean').read_text()
source = source.replace('import Arith\n', 'import Arith\nset_option profiler true\nset_option profiler.threshold 0\n', 1)
source = re.sub(r'(?m)^((?:open Classical in\n)?theorem (\w+))',
                lambda m: f'#eval IO.eprintln "THEOREM {m.group(2)}"\n{m.group(1)}', source)
source = source.replace('\n#print axioms checked_answer_refines',
                        '\n#eval IO.eprintln "THEOREM END"\n#print axioms checked_answer_refines', 1)
profile_source = tmp / 'millipede-profile-named.lean'
profile_log = tmp / 'millipede-profile-named.log'
profile_source.write_text(source)
env = os.environ.copy()
env['TMPDIR'] = str(tmp)
start = time.monotonic()
with profile_log.open('w') as log:
    proc = subprocess.run(['lake', 'env', 'lean', str(profile_source)], cwd=root,
                          env=env, stdout=log, stderr=subprocess.STDOUT)
elapsed = time.monotonic() - start
if proc.returncode:
    raise SystemExit(f'profile failed: exit {proc.returncode}; see {profile_log}')
lines = profile_log.read_text().splitlines()
markers = [(i, line.removeprefix('THEOREM ')) for i, line in enumerate(lines)
           if line.startswith('THEOREM ')]
for (i, name), (j, _) in zip(markers, markers[1:]):
    times = [line.removeprefix('elaboration took ') for line in lines[j + 1:j + 40]
             if line.startswith('elaboration took ')]
    if not times:
        raise SystemExit(f'missing profile result for {name}; see {profile_log}')
    print(f'{name}: {times[0]}')
print(f'profile wall: {elapsed:.2f}s')
