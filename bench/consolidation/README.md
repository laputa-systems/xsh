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

The overlay starts from an immutable `Dockerfile.test` image ID. It adds
dash 0.5.12 and Oils/YSH 0.37.0 source builds, Nushell 0.107.0's official musl
binary, and Elvish 0.21.0's official binary. Bash 5.3.9 and Python 3.14.8 come
from that base image. Archives for both architectures are pinned by SHA-256;
the overlay never mutates the parent image. No Rust dependencies are added.
Preparation creates a dedicated local base alias because BuildKit treats bare
image IDs as registry tags; the alias must resolve to the pinned image ID.

Primary release sources: [Nushell 0.107.0](https://www.nushell.sh/blog/2025-09-02-nushell_0_107_0.html),
[Nushell release artifacts](https://github.com/nushell/nushell/releases/tag/0.107.0),
[Elvish binary distribution](https://elv.sh/get/all-binaries.html),
[Oils installation](https://github.com/oils-for-unix/oils/blob/master/INSTALL.txt),
and [Debian's upstream dash archive](https://deb.debian.org/debian/pool/main/d/dash/).
Dash's upstream download host was unavailable at preparation; the pinned
unmodified upstream tarball is served by Debian. Version selection is fixed
for reproducibility and does not claim to be the newest release.

Prepare inputs and the overlay on disk outside `/tmp`:

```sh
python3 -B bench/consolidation/fixtures.py .work/consolidation/comparative-fixtures
python3 -B bench/consolidation/prepare.py .work/consolidation/comparative-overlay --build
```

Coordinate the machine compiler queue before `--build`. Preparation builds
dash with `make -j2`, then Oils with its sequential optimized build; both
use the existing image compiler and libraries. For final ARM verification,
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

The runner uses one unprivileged, read-only, network-disabled container. Suite,
binary, and fixture mounts are read-only; output and per-tool scratch mounts
are writable. Exact stdout bytes, empty stderr, status zero, timeout state,
and no remaining scratch changes must agree before measuring. Any failure
stops the run and leaves its raw observation in the report. Process-group
timeouts kill workload descendants, and scratch is removed on exit.

For measurement, add `--measure --functional-close /absolute/close.json`.
The integrator supplies that JSON record with `functional_complete: true`
and `commit` equal to `--source-commit`. Parity always runs first. One warmup
per tool/workload precedes three rounds of 30 fresh process samples per tool,
or 60 for startup. Filesystem caches are warm. Tools rotate cyclically within
each sample, with alternating reversal and round offsets. Each workload runs
serially; other test/build lanes must be idle during measurement.

`perf_counter_ns` measures from process creation through collection of stdout
and stderr. Script loading/checking, subprocesses, helper work, and output
capture are included; fixture preparation, scratch fingerprinting, and report
writes are outside the interval. Raw samples retain nanoseconds, round,
sample position, order, argv, status, stderr, effects, and stdout hash; exact
successful stdout is retained in the preceding parity observations. Failed
samples retain stdout bytes. Reports include executable/script/helper hashes,
versions, image and target, clock, fixture manifest hash, and source commit.
XSH is pinned by source commit and executable hash, and dash by source archive;
neither runtime supplies a version command. Their version output is null.
Summaries report each tool's pooled median, median absolute deviation, and
three round medians. Commit method, version lock, and the complete raw report
after final measurement; never substitute a summary for the samples.
