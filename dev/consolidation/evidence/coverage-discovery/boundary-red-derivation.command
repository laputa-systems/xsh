python3 - <<'PY'
from pathlib import Path
import subprocess
p = Path('crates/xsht/src/cli/files.rs')
current = p.read_text()
Path('.work/tmp/discovery/boundary-final-files.rs').write_text(current)
base = subprocess.check_output([
    'git', 'show',
    '0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4:crates/xsht/src/cli/files.rs',
], text=True)
a = current.index('enum DiscoveredPath {')
b = current.index('\nfn check_cancellation()', a)
x = base.index('#[allow(clippy::single_call_fn)]\nfn collect_xsh_files_parallel')
y = base.index('\nfn check_cancellation()', x)
p.write_text(current[:a] + base[x:y] + current[b:])
PY
