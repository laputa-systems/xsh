# Interpreter Performance: tokei.xsh

## Objective

Get `showcase/tokei.xsh` wall-clock time on the Sentry corpus within **1x native tokei** and peak RSS within **1x of native tokei**, with byte-for-byte
identical table and JSON output. This is an interpreter and lowered-runtime
objective. It must not change language semantics or script-visible behavior.

## Current state

The objective is complete as of 2026-07-02 on the current
`/Users/josh/dev/sentry` checkout. The current checkout is about 3.1 GB and
140,909 files; ignore and extension filtering select about 20,101 relevant files
and about 186 MB of selected source.

Fresh release samples from the completing binary:

| Path | XSH release | Native tokei | Target |
|---|---:|---:|---:|
| table | `1.03s / 62,750,720` bytes RSS | `1.22s / 39,223,296` bytes RSS | ≤1.2x wall, ≤66 MB RSS |
| JSON | `1.19s / 65,667,072` bytes RSS | `1.21s / 52,805,632` bytes RSS | ≤1.2x wall, ≤66 MB RSS |

Earlier same-binary samples were also under the fixed memory target: table
`1.55s / 60,407,808` bytes RSS and JSON `1.55s / 63,373,312` bytes RSS. The
warm samples above are the completion evidence because they compare against
native tokei measured on the same current checkout.

The implementation details that made this work are part of the working
architecture now. Keep them in `PIPELINE.md`, not in this file.

## Verification

Recent verification from the completing work:

- `cargo check --lib`
- `cargo test --test syntax`
- `cargo test compact_lowered_runner`
- `cargo build --bin xsh && cargo run -p xsht -- test showcase/tests/test-tokei.xsh`
- `cargo test --test runtime`
- `cargo bench -p xshi --bench bench small_corpus -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1`
- `RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=/tmp/xsh-type-target4 cargo rustc --lib -- -Zprint-type-sizes`
- repeated `cmp -s` checks against saved XSH table and JSON outputs after each
  release sample
- repeated `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- ...`
  table and JSON samples on `/Users/josh/dev/sentry`
- `git diff --check`

When rerunning measurements after a `perf-metrics` build, rebuild a normal
release binary first. The `perf-metrics` feature installs a different allocator
and changes RSS and timing.

## Rejected directions

These trials preserved behavior but were reverted or should not be repeated as
narrow changes:

- Lowered `List` clones as immutable copy-on-write `Arc<Vec<_>>` during clone.
  Allocation volume improved, but RSS did not. The kept design is slot-rooted
  `SharedList` freezing, documented in `PIPELINE.md`.
- Lowered-only anonymous mmap backing for `Path.read_bytes()`. Focused byte
  behavior tests passed, but release RSS did not improve on generated corpora.
- Narrow streaming fusion for `par-map |> where |> flat-map |> reduce-by`.
  Sentry RSS moved only slightly and wall time regressed.
- Worker-local runtime fusion for `par-map |> flat-map(identity) |> reduce-by`
  after the `SummaryRow` script change. It reduced instruction count but did
  not reduce RSS beyond the script change and added evaluator complexity.
- Changing lowered `RecordVec` to `Arc<Vec<_>>`. Table RSS regressed in repeated
  release samples and JSON movement was noisy. Keep owned `RecordVec` unless a
  broader lifetime design changes both paths.
- Calling macOS `malloc_zone_pressure_relief` after lowered `par-map` worker
  joins. RSS and CPU got worse.
- A special lowered `print json.encode(...)` fast path that moved the encoded
  string buffer directly into stdout. It did not reduce the JSON RSS peak.

If aggregation fusion is revisited, it needs to stream results into `reduce-by`
without changing encounter-order error behavior or floating-point aggregation
semantics. It also needs to attack the remaining file-byte/value allocation
floor, not just remove the post-`par-map` row list.

## Why native tokei is small

Native tokei uses `ignore::Walk::parallel()` plus `rayon` to fuse walk, read,
and scan into one parallel operation. There are no evaluator clones, no
intermediate item vectors, and no record/map value construction for each file
entry. Each file's bytes are read, scanned, and dropped before the next file is
pulled. Peak memory is roughly `max(file_size) * thread_count` plus fixed
overhead.

XSH has to preserve language-level values and boundaries, so the successful
work focused on making those values compact and lazy where behavior allows it,
then avoiding retained duplicate result graphs in `showcase/tokei.xsh`.

## References

- `PIPELINE.md` - current compact frontend and lowered runtime architecture
- `docs/IR.md` - lowered IR design and verification guidance
- `perf/README.md` - frontend and runtime profiling commands
- `showcase/tokei.xsh` - benchmark script and output-shape constraints
