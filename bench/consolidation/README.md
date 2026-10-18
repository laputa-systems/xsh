# Comparative glue workloads

These are reduced, fixed workloads from Laputa at the commit in
`versions.lock.json`. They measure interpreter and glue costs; they are not
package builds or a claim about language optimality. Results have no pass/fail
performance threshold. Timing starts after the integrator records functional
completion at the measured source commit.

| Workload | Laputa source | Fixed observable contract |
| --- | --- | --- |
| startup | `pm.xsh` entry point | Fresh process loads a minimal script and prints `ready\n`. No import graph or package work. |
| spawn | `pm/proof.xsh` process orchestration | 128 sequential external `/usr/bin/printf` calls, printing `probe 0` through `probe 127`. No shell builtin substitutes for the child. |
| pipeline | `packages/libcap/PKGBUILD.xsh` capability name generator | Read 9,633,792 ASCII bytes, select the exact define pattern, lowercase names, preserve digit spelling, print 16,384 rows in source order. |
| directory | `pm/generation.xsh` overlay traversal | Include hidden and ignored entries, globally sort relative paths, classify file/dir/link, never follow links. Spaces, glob characters, dangling links, and a directory cycle are fixtures. |
| json | `pm/remote.xsh` replace/append/name sort | Parse 2,049 records, replace both occurrences of one name, append a new name, sort, emit compact JSON with fixed field order. |

The text pattern is
`^#define[ \t]+(CAP_[A-Z0-9_]+)[ \t]+([0-9]+)[ \t]*$` on ASCII LF lines.
JSON names are ASCII identifiers and versions are integers. Directory names
exclude tabs and newlines. These restrictions are shared input contracts,
not claims about general filename or JSON behavior. Fixtures and expected
stdout are generated deterministically by `fixtures.py`; every file hash,
mode, directory, and link target is checked against its manifest.
The manifest itself must match its committed hash in `versions.lock.json`.

All seven tools run the same contracts. Bash and dash share POSIX source.
Their text workload uses the image's GNU awk; their JSON workload explicitly
starts the shared Python implementation. POSIX directory enumeration and
sorting use image `find` and `sort`, with checked intermediate files removed
before exit. YSH and Elvish also use image `find`; YSH sorts JSON names with
image `sort`. Other parsing/transformation stays in each language. Helpers
are part of the timed command and their executable hashes are recorded.

The overlay starts from the immutable `Dockerfile.test` image ID and adds the
other pinned tool versions. URLs, SHA-256 pins, and architecture choices live
in `versions.lock.json`. The overlay does not mutate its base image.

Prepare inputs and the overlay under ignored `.work/`. Set `TMPDIR` there for
any temporary files:

```sh
mkdir -p .work/tmp .work/consolidation
export TMPDIR="$PWD/.work/tmp"
python3 -B bench/consolidation/fixtures.py .work/consolidation/comparative-fixtures
python3 -B bench/consolidation/prepare.py .work/consolidation/comparative-overlay --build
```

Before creating `.work/`, make sure it is ignored through `.git/info/exclude`.

Coordinate the machine compiler queue before `--build`. Preparation builds
dash with `make -j2`, then Oils with its sequential optimized build; both
use the existing image compiler and libraries. BuildKit limits the overlay
build to four CPU-equivalents and 8 GiB, and the preparation receipt records
those limits. For an internal ARM run,
pass `--arch arm64 --base-image sha256:THE_ARM_DOCKERFILE_TEST_IMAGE_ID` and
use a separate overlay tag. The result records the exact base and overlay IDs.

Check the host orchestration and all 35 observable cases:

```sh
python3 -B bench/consolidation/test_harness.py
python3 -B bench/consolidation/run.py --image xsh-comparative:consolidation \
  --xsh /absolute/release/xsh --fixtures .work/consolidation/comparative-fixtures \
  --output .work/consolidation/parity.json --scratch .work/consolidation/parity-scratch \
  --source-commit FULL_SOURCE_COMMIT
```

The runner uses one unprivileged, read-only, network-disabled container.
Inputs are read-only; output and per-tool scratch are writable. It requires
exact stdout, empty stderr, status zero, no timeout, and unchanged scratch before
measuring. Failures stop the run and retain raw observations. Process-group
timeouts kill descendants and scratch is removed on exit. The runner pins one
allowed host CPU, disables swap, and limits memory to 4 GiB.

For measurement, add `--measure --functional-close /absolute/close.json`.
The integrator supplies that JSON record with `functional_complete: true`
and `commit` equal to `--source-commit`. Parity always runs first. One warmup
per tool/workload precedes three rounds of 30 fresh process samples per tool,
or 60 for startup. Filesystem caches are warm. Tools rotate cyclically within
each sample, with alternating reversal and round offsets. Each workload runs
serially; other test/build lanes must be idle during measurement.

`perf_counter_ns` measures process creation through stdout and stderr capture.
Fixture preparation and report writes are outside the interval. Reports retain
raw samples and tool, script, helper, image, fixture, and source identities.
Summaries report pooled medians, median absolute deviation, and round medians.
Keep complete reports with final measurements.

The separate XSH performance cohort uses 4,096- and 8,192-row shapes from the
same fixture generator. [`dev/performance/README.md`](../../dev/performance/README.md)
owns its measurement policy. Scaled fixtures do not replace the pinned inputs
to this seven-tool comparison.
