#!/usr/bin/env python3
import hashlib
import pathlib
import subprocess
import sys

repo = pathlib.Path(__file__).resolve().parents[3]
relative = 'dev/tests/test-lifecycle.xsh'
original = subprocess.check_output(
    ['git', 'show', 'ddc17d3d53c819c343f9ecaa738ac67d59e9033a:' + relative],
    cwd=repo,
)
helper = b'''proc write_fake_flock(tools: Path) [fs, error] {
  fp"{tools}/flock".write(
    "#!/bin/sh\\nexec /usr/bin/flock \\"$@\\"\\n",
    mode: 0o755,
  )
}

'''
anchor = b'proc write_fake_lint_tools(\n'
assert original.count(anchor) == 1
corrected = original.replace(anchor, helper + anchor, 1)
start = corrected.index(b'test test_docker_container_failure_runs_target_ownership_cleanup ')
corrected = corrected[:start] + corrected[start:].replace(
    b'  tools.mkdir()\n', b'  tools.mkdir()\n  write_fake_flock(tools)\n', 1,
)
for argument in sys.argv[1:]:
    path = pathlib.Path(argument) / relative
    actual = path.read_bytes()
    assert actual in (original, corrected), path
    if actual != corrected:
        path.write_bytes(corrected)
    print(path, hashlib.sha256(original).hexdigest(), hashlib.sha256(corrected).hexdigest(), sep='\t')
