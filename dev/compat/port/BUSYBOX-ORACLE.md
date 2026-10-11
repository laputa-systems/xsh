# BusyBox native-port oracle

`Dockerfile.busybox-oracle` builds BusyBox 1.36.1 from the source and archive
pinned in `dev/compat/upstream.lock.json`. The build checks the archive SHA-256
and source version, cleans only the copied source, and starts with `make
defconfig`. It disables `CONFIG_TC` because current Linux headers removed the
CBQ structures used by that release's `tc`; this command is outside the frozen
surface. Every command in `freeze.json`'s `busybox` section must appear in the
built binary's applet list, including `awk`, `sed`, `patch`, and `taskset`.
`CONFIG_FEATURE_GZIP_LEVELS` is enabled because the frozen compression test
requires `gzip -1` and `gzip -9` to produce different streams; `defconfig`
otherwise accepts both options while silently using level 6 for each.

Prepare a fresh context from the locked reference tree and its archive:

```sh
mkdir -p .work/busybox-oracle/context
cp -a ../xsh/.work/upstream/busybox/busybox-1_36_1 .work/busybox-oracle/context/source
cp ../xsh/.work/upstream/busybox/busybox-1.36.1.tar.gz .work/busybox-oracle/context/
python3 - <<'PY'
import json
from pathlib import Path
commands = json.load(open('dev/compat/port/freeze.json'))['busybox']
Path('.work/busybox-oracle/context/required-applets').write_text('\n'.join(commands) + '\n')
PY
DOCKER_BUILDKIT=0 docker build --memory=4g --memory-swap=4g \
  -t xsh-oracle-busybox-1.36.1 \
  -f dev/compat/port/Dockerfile.busybox-oracle .work/busybox-oracle/context
```

Adjust the reference path to the source directory named by the lock file if
working from a different checkout. The Dockerfile uses eight compilation jobs.
The legacy builder enforces the four GiB container limit; BuildKit ignores
these memory options. The existing `xsh-oracle-gnu-9.12` image supplies glibc,
UID/GID 1000, locales, and fixture tools described in `GNU-ORACLE.md`. GNU tools
remain available; only `/bin/busybox` is replaced. Build packages use Debian's
unstable package index. Keep the GNU base image used for a run with its image ID.

Select the pinned image and the explicit BusyBox reference:

```sh
XSH_ORACLE_IMAGE=xsh-oracle-busybox-1.36.1 XSH_BIN=/path/to/release/xsh \
  python3 dev/compat/port/oracle_port.py --reference busybox core/tests/test-bb-awk.xsh
```

The image retains the locked source archive, upstream GPL license, and exact
build configuration under `/usr/share/busybox-oracle/`. Retain this directory
and the Dockerfile when distributing the oracle image so its corresponding
source and build instructions remain available. This oracle is a test tool,
never an XSH runtime dependency.

The pinned release still has the three `sed.tests` cases guarded by
`SKIP_KNOWN_BUGS`: embedded NUL without `g` substitutes both occurrences,
a NUL in a command file fails with `unsupported command o`, and an unresolved
branch on empty input succeeds. These are reference defects recorded by the
upstream suite; selecting this image does not make those expected-output
assertions pass. The `awk` expression `$1==$1="foo" {print $1}` does succeed
with `foo` input in this release.
