#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/consolidation-corpus-repair
scratch=$lane/.work/tmp/corpus-repair
pin=b2f3a0624205e68d06f9a439ffe2dea5e7dc4541
candidate=$scratch/candidate-source
git -C "$lane" check-ignore "$lane/.work/" > /dev/null
git -C "$lane" diff --exit-code "$pin" -- '*.xsh' '*xsht-config.ini'
test ! -e "$candidate"
mkdir -p "$scratch/tmp" "$candidate"
export TMPDIR=$scratch/tmp
git -C "$lane" archive "$pin" | tar -x -C "$candidate"
mkdir -p "$candidate/target/x86_64-unknown-linux-musl/release"
python3 - "$lane" "$candidate" "$pin" <<'PY'
from pathlib import Path
import os
import subprocess
import sys

lane = Path(sys.argv[1])
candidate = Path(sys.argv[2])
pin = sys.argv[3]
skip = {'.git', '.work', 'target', '.claude'}
tracked = subprocess.check_output([
    'git', '-C', str(lane), 'ls-tree', '-r', '--name-only', '-z', pin,
]).split(b'\0')
expected = {
    Path(name.decode()) for name in tracked if name
    and (Path(name.decode()).suffix == '.xsh'
         or Path(name.decode()).name == 'xsht-config.ini')
    and not any(part in skip for part in Path(name.decode()).parts[:-1])
}
assert len(expected) == 1835, len(expected)

def observed(root):
    paths = set()
    def scan(directory):
        with os.scandir(directory) as entries:
            for entry in entries:
                if entry.is_dir(follow_symlinks=False):
                    if entry.name not in skip:
                        scan(Path(entry.path))
                elif entry.name.endswith('.xsh') or entry.name == 'xsht-config.ini':
                    paths.add(Path(entry.path).relative_to(root))
    scan(root)
    return paths

for source in [lane, candidate]:
    actual = observed(source)
    extra = actual - expected
    missing = expected - actual
    assert not extra, (source, sorted(extra))
    assert not missing, (source, sorted(missing))
    print(f'{source}: {len(actual)} observable inputs; 0 extra; 0 missing')

for relative in sorted(expected):
    assert (lane / relative).read_bytes() == (candidate / relative).read_bytes(), relative
print('All 1835 XSH/config archive inputs match candidate bytes')
PY
