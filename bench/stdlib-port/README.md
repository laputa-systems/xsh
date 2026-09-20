# Embedded standard-library port benchmarks

Paired reference/candidate measurement for the port described in
`STDLIB-PORT.md`. Reference and candidate runs are interleaved within one
target so CPU frequency, cache, and scheduler drift affect both sides, and the
reported value is the median of per-sample wall-clock durations.

## Reproducing

Build both sides as release binaries from equivalent checkouts, in separate
target directories:

```sh
git worktree add /tmp/xsh-reference <starting-revision>
(cd /tmp/xsh-reference && cargo build --release -p xsh -p xsht --bin xsh --bin xsht)
cargo build --release -p xsh -p xsht --bin xsh --bin xsht
```

Then run the paired runner:

```sh
python3 bench/stdlib-port/run.py \
  --reference /tmp/xsh-reference/target/release/xsh \
  --candidate target/release/xsh \
  --out bench/stdlib-port/results.json
```

It exits non-zero if any workload exceeds its budget and writes every raw
sample to the output file. Budgets are the specification's: cold startup passes
when `C - B <= max(0.05 * B, 1.0 ms)` and non-hot end to end when
`C - B <= max(0.10 * B, 2.0 ms)`, where `B` and `C` are the reference and
candidate medians in milliseconds.

Each workload runs from this directory (`cwd` matters to `module.load`), so run
the runner from the repository root with the paths above.

## Workloads

| File | Class | What it measures |
| --- | --- | --- |
| `cold_trivial.xsh` | cold | a fresh process running a script with no migrated API |
| `cold_quote.xsh` | cold | one `shlex.quote` call |
| `cold_pad.xsh` | cold | one `tui.left_pad` call |
| `cold_dynamic_ref.xsh` | cold | a program that references `module.load` and loads `dynmod/mod.xsh` |
| `text_pad_batch.xsh` | end to end | 2,100 ANSI-aware pads over three shapes |
| `quote_batch.xsh` | end to end | `shlex.join` over a 1,000-word argv |
| `fmt_batch.xsh` | end to end | 4,000 byte-size and duration formatter calls |
| `checksum_batch.xsh` | end to end | 500 `hash.parse_check_line` calls |
| `native_control.xsh` | control | an `fs.dirs` walk that must keep its native fast path |

## Interpreting results

A failing workload is a finding, not necessarily a defect. The ended port
batches are dominated by interpreted per-call cost: a calibration in the same
build measures roughly 0.7 µs per interpreted loop iteration and 1.4 µs per
interpreted function call, against nanoseconds for the native helper each one
replaces. `STDLIB-PORT.md` records which failures are of that kind and which
have a cause that can be fixed.

Do not benchmark through `cargo run`, include compilation time, or compare one
revision on a different CPU or container allocation.
