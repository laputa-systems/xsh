# Factor output-buffer repair evidence

The factor command accumulated every result by concatenating onto one growing
string. Runtime string addition copies the left operand, so the parallel case
repeatedly copied each child's full output prefix. The command now appends each
line to a `List[Str]` and joins once at the same error and final flush points.
This preserves output order, trailing newlines, and error-flush behavior.

## Exact case evidence

`r3-canonical-parallel-timeout.txt` is the raw failure excerpt from the R3
canonical gate run at source commit `7eb62cd993e578c13bf6befd68a6a6ac15133c78`.
It shows the unchanged parallel test timing out at its existing two-minute
limit. That run used the R3 source and the release products below.

The baseline and patched selector logs are raw output from the same pinned
release products and the same bounded native runner. Baseline source is
`53ab18f3e909d2cbc2cade833ef005677f4e8e8ba781441cdf90ebd398e254f2`; patched
source is `47b0714bc148ea8bef7f2cf345f0cdde6ca23c1ccd67bdabbd2f2ff84288bbd2`.
The unchanged factor test source is
`634e01ec92e53df226cf33e56780abd130c3fc418ea4c3df09d434590f050238`.

Both focused selectors passed after the change: the parallel case passed with
400,008 output lines and the original expected sorted-output hash assertion;
the existing mixed non-Unicode case passed its exact stdout, stderr, and exit
status checks. The baseline parallel selector also passed in isolation, so
that result alone does not recreate the canonical timeout or establish that
cross-test contention was absent. The edited test has not been replayed in the
full canonical suite.

## Deterministic work accounting

The retained baseline output contains eight identical child outputs, each
50,001 lines and 815,076 bytes. Under string `+=`, the bytes in prior prefixes
copied for one child sum to 19,781,920,511; across eight children that is
158,255,364,088 bytes copied to produce 6,520,608 output bytes. This count is
computed from the retained output and the runtime string-add operation. It is
operation accounting, not a timing or speedup measurement.

## Runner identity

The frozen products are
`.work/final-review-evidence/final-performance-gates/resources-7eb62cd9-20261017-r1/gate-batch-target/x86_64-unknown-linux-musl/release/{xsh,xsht}`:

- `xsh`: `1d7690114501313ffc4706b7f78ab6154ecd668f79d3735700219a00501dbe33`
- `xsht`: `0cd37140bd43ca0ca5104d43d11bc1732084fc2342ee3bb69a73fda38452f1c4`

The runner used pinned image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`, Linux x86_64 musl, cpuset 0–11, 12 CPU quota, 24 GiB memory with no swap, no network, UID/GID 1000, `DIST_PROFILE=release`, `-j 1`, and `--keep-temp`. Each invocation had a 150-second outer cutoff; the test retained its own two-minute timeout. Source, test, and product hashes were checked before and after each invocation. The original shared native-suite lock file (device 66307, inode 62012470) was held exclusively and its inode and metadata remained unchanged.

The harness-reported durations are retained in the raw logs to document the
functional runs. They are not performance measurements: host load was shared,
and no performance gate was run.
