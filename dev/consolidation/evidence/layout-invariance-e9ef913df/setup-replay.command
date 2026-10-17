#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/consolidation-layout-repair
scratch=$lane/.work/tmp/layout-repair
export TMPDIR=$scratch/tmp
mkdir -p "$scratch/tmp"
test ! -e "$scratch/source"
mkdir "$scratch/source"
git -C "$lane" archive b1bc741fcd512d1c71b43116853278a4a3f1521a | tar -x -C "$scratch/source"
mkdir -p "$scratch/source/target/x86_64-unknown-linux-musl/release"
python3 - "$lane" "$scratch" <<'PY'
from pathlib import Path
import os, re, shutil, subprocess, sys
base, scratch = map(Path, sys.argv[1:])
source = scratch / 'source'
skip = {'.git', '.work', 'target', '.claude'}
def inputs(root):
    result = {}
    for parent, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in skip]
        for name in files:
            if name.endswith('.xsh') or name == 'xsht-config.ini':
                path = Path(parent) / name
                result[path.relative_to(root).as_posix()] = path.read_bytes()
    return result
actual = inputs(base)
assert actual == inputs(source)
assert len(actual) == 1835
tracked = set(subprocess.check_output(['git', '-C', str(base), 'ls-files'], text=True).splitlines())
assert not set(actual) - tracked
print('all 1835 copied .xsh/config inputs byte-identical; zero additional untracked observable inputs')
dest = scratch / 'narrow-source'
dest.mkdir()
pending = [Path('core/factor.xsh'), Path('dev/tests/test-lifecycle.xsh')]
seen = set()
while pending:
    rel = pending.pop()
    if rel in seen:
        continue
    seen.add(rel)
    for module in re.findall(r'^use\s+([A-Za-z_][\w.]*)', (base / rel).read_text(), re.M):
        mp = Path(*module.split('.')).with_suffix('.xsh')
        found = next((p for p in [rel.parent / mp, mp, Path('dev') / mp, Path('core') / mp]
                      if (base / p).is_file()), None)
        if found is not None:
            pending.append(found)
        else:
            print('unresolved (possibly stdlib):', rel, module)
assert len(seen) == 19
for rel in sorted(seen):
    target = dest / rel
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(base / rel, target)
    assert target.read_bytes() == (source / rel).read_bytes()
shutil.copyfile(base / 'xsht-config.ini', dest / 'xsht-config.ini')
(dest / 'crates/xsht').mkdir(parents=True)
(dest / 'target/x86_64-unknown-linux-musl/release').mkdir(parents=True)
for name in ['narrow', 'narrow-green', 'full-green', 'minimal', 'check-minimal', 'minimal-green',
             'unit-red', 'unit-green', 'unit-modules', 'build-xsht']:
    (scratch / name / 'tmp').mkdir(parents=True, exist_ok=True)
fixtures = {
    'annotation-before.xsh': 'proc show() {\n  var out: List[Str] = []\n  out += ["x" + "\\n"]\n  print (out.join(""))\n}\n',
    'annotation-after.xsh': 'proc show() {\n  var out: List[Str] = []\n  out += [("x") + ("\\n")]\n  print (out.join(""))\n}\n',
    'alias-before.xsh': 'pure target(value: Int) -> Int { value + 1 }\npure wrapper(value: Int) -> Int { target(value) }\n',
    'alias-after.xsh': 'pure target(value: Int) -> Int { (value) + (1) }\npure wrapper(value: Int) -> Int { target(value) }\n',
}
for name, text in fixtures.items():
    (scratch / 'minimal' / name).write_text(text)
PY
python3 - "$root" "$scratch" <<'PY'
from pathlib import Path
import hashlib, shutil, sys
root, scratch = map(Path, sys.argv[1:])
dest = scratch / 'candidate-products'
dest.mkdir()
original = root / '.work/consolidation/final-performance-gates/profile-artifacts-b1bc741f-20261017-r1/opt3/target/x86_64-unknown-linux-musl/release/xsh'
candidate = root / '.work/final-review-evidence/final-performance-gates/resources-7eb62cd9-20261017-r1/gate-batch-target/x86_64-unknown-linux-musl/release/xsht'
for source, expected in [
    (original, 'e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b'),
    (candidate, '6e6313ee1ec085a4aa652adb0ba119f1a792746162eba3f0912d9eef30fd082d'),
]:
    assert hashlib.sha256(source.read_bytes()).hexdigest() == expected
    shutil.copy2(source, dest / source.name)
    assert hashlib.sha256((dest / source.name).read_bytes()).hexdigest() == expected
PY
