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
  --candidate "$PWD/target/release/xsh" \
  --out bench/stdlib-port/results.json
```

Both binary paths must be absolute: each workload runs from this directory
(`cwd` matters to `module.load`), so a relative `target/release/xsh` would not
resolve. The runner itself is invoked from the repository root.

It exits non-zero if any workload exceeds its budget and writes every raw
sample to the output file. The ledger cites `results-final.json`, the run that
covers the final tree; the other `results-*.json` files are earlier rounds of
the same runner and are kept for the record, not as the current numbers. Budgets are the specification's: cold startup passes
when `C - B <= max(0.05 * B, 1.0 ms)` and non-hot end to end when
`C - B <= max(0.10 * B, 2.0 ms)`, where `B` and `C` are the reference and
candidate medians in milliseconds.

## Workloads

| File | Class | What it measures |
| --- | --- | --- |
| `cold_trivial.xsh` | cold | a fresh process running a script with no migrated API |
| `cold_quote.xsh` | cold | one `shlex.quote` call |
| `cold_pad.xsh` | cold | one `tui.left_pad` call |
| `cold_cli_parse.xsh` | cold | one `cli.parse` call in a fresh process |
| `cold_cli_usage.xsh` | cold | one `cli.usage` render in a fresh process |
| `cold_cli_error.xsh` | cold | one rejected `cli.parse` in a fresh process |
| `cold_dynamic_ref.xsh` | cold | a program that references `module.load` and loads `dynmod/mod.xsh`, so the mandated complete-set preparation is paid |
| `cli_small_schema.xsh` | end to end | 200 parses of a four-field schema |
| `cli_wide_schema.xsh` | end to end | 200 parses of a 64-field schema across all four descriptor kinds |
| `cli_repeated_parse.xsh` | end to end | repeated parses of a long operand list with repeated and positional fields |
| `text_wrap_unicode.xsh` | end to end | `Str.wrap` passes over the 1 MiB Unicode fixture |
| `text_pad_batch.xsh` | end to end | 2,100 ANSI-aware pads over three shapes |
| `fmt_batch.xsh` | end to end | 4,000 byte-size and duration formatter calls |
| `quote_batch.xsh` | end to end | `shlex.join` over a 1,000-word argv |
| `quote_edge_cases.xsh` | end to end | 200 batches of the quoting edge cases: empty, embedded quotes, newlines, non-ASCII |
| `mime_batch.xsh` | end to end | 500 rounds of `mime.lookup_ext` and `mime.lookup_path` |
| `ini_large_record.xsh` | end to end | one `ini.encode` of a 1,000-key, 20-section record |
| `json_path_ops.xsh` | end to end | 400 rounds of nested `json.get`/`set`/`remove` over records, maps, and lists |
| `json_lines_batch.xsh` | end to end | one bounded `json.encode_lines` of 10,000 small records |
| `env_typed_lookups.xsh` | end to end | 500 rounds of typed `env` lookups with a fallback |
| `checksum_batch.xsh` | end to end | 500 `hash.parse_check_line` calls |
| `core_command.xsh` | end to end | a small core command driven through the CLI policy 40 times |
| `native_control.xsh` | control | an `fs.dirs` walk that must keep its native fast path |
| `native_hash_control.xsh` | control | 200 file hashes, the retained native digest path |

## Linux workloads

There are none. The four gated-Linux prototypes this runner measured
(`linux.routes`, `linux.rfkill_list`, `linux.block_devices`, and the module
policy) each failed the non-hot budget, so all four were reverted to their
native bodies and their `.xsh` implementations were deleted;
`STDLIB-PORT.md` records the measurement that removed each one, under
`## Performance`.

The runner keeps its `--linux` flag and the per-workload `linux_only` mark for
that measurement if a prototype is ever attempted again. A workload marked that
way runs in the `Dockerfile.test` container — its entries read `/proc` and
`/sys`, so it needs the trees built there — with `XSH_LINUX_REAL=1` in the
environment, and is reported as skipped otherwise. Adding one means staging a
fixture tree in the container first, the way the reverted round did:

```sh
docker run --rm --privileged --platform linux/arm64 \
  -v "$PWD:/work" -v "$PWD/lx-target:/work/lx-target" -w /work \
  -e CARGO_TARGET_DIR=/work/lx-target xsh-test sh -c '
    mkdir -p /sys-fixtures && mount -t tmpfs tmpfs /sys/class &&
    ... && cd bench/stdlib-port &&
    python3 run.py --linux --reference /ref/xsh --candidate /work/lx-target/debug/xsh'
```

## Interpreting results

A failing workload is a finding, not necessarily a defect. The migrated batches
are dominated by interpreted per-call cost: a calibration in the same build
measures roughly 0.7 µs per interpreted loop iteration and 1.4 µs per
interpreted function call, against nanoseconds for the native helper each one
replaces. `STDLIB-PORT.md` records which failures are of that kind and which
have a cause that can be fixed.

A budget failure says nothing about behavior, because the runner discards each
workload's output. `parity.py` measures that separately — the same scripts, the
same working directory, both binaries, with exit status, stdout, and stderr
compared byte for byte:

```sh
python3 bench/stdlib-port/parity.py \
  --reference /tmp/xsh-reference/target/release/xsh \
  --candidate "$PWD/target/release/xsh"
```

Do not benchmark through `cargo run`, include compilation time, or compare one
revision on a different CPU or container allocation.
