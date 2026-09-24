# Embedded standard-library port benchmarks

Paired reference/candidate measurement for the remaining work in
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
sample to the output file. `results-followup-b0.json` is the latest run against
the original baseline; `results-followup-b1.json` compares the same candidate
with the pre-follow-up port. Both contain one interleaved round;
`results-final.json` and the other result files are historical. Cold startup passes
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

The six R12 workloads — `linux_uptime`, `linux_meminfo`, `linux_memory`,
`linux_os_release`, `linux_modules_full`, and `linux_modules_partial` — measure
the entries whose Linux policy lives in the embedded standard library. They are
marked `linux_only`, so a run without `--linux` reports them as skipped and a
run with it executes them under `XSH_LINUX_REAL=1`.

Four of them read the container's own `/proc/uptime`, `/proc/meminfo`, and
`/etc/os-release`. The two module rows read `/proc/modules`, which the container
stages as a 200-module fixture inside its own mount namespace: the container ships
no modules, and a fixed row set is what makes the full-consumption row compare
equal work between the two binaries. Both revisions then read the same text.
The gated `linux.routes`, `linux.block_devices`, and module-policy prototypes
failed measured non-hot gates and were reverted. `linux.rfkill_list` was
reverted with the block-device prototype without its own timing. Their staged
`/sys` trees are gone; `STDLIB-PORT.md` records the remaining qualification.

Only the `Dockerfile.test` container runs these rows — it owns the compiler, the
musl CRT objects, and the symbol aliases the tree links against. The container
invocation the rest of this project uses, and which this section's statements
about the container were verified with, is:

```sh
docker run --rm --privileged --platform linux/arm64 \
  -v "$PWD:/work" -v "$PWD/target:/work/target" \
  -v xsh-cargo-registry:/root/.cargo/registry -w /work \
  -e TARGET=aarch64-unknown-linux-musl -e CARGO_TARGET_DIR=/work/target \
  xsh-test sh -c 'cargo test --features linux-priv-tests --test integration'
```

For the Linux rows of this runner, that invocation needs three more things, and
the runner itself does not stage any of them:

- a **release** build on each side — `cargo build --release -p xsh --bin xsh`
  for the candidate, and the same build of the starting revision in a mounted
  worktree for the reference, so both sides are matched release binaries from
  the same image;
- the `/proc/modules` fixture for the two module rows, staged inside the
  container's own mount namespace (a `tmpfs` file bind-mounted over
  `/proc/modules`, as the reverted prototype round did) and never over a shared
  mount;
- `--linux`, without which the six rows are reported as skipped.

The image itself ships no Python, so `run.py` cannot run *inside* it: these rows
are driven from a Linux host that has Python and the container-built binaries,
or from a container image that adds Python. The R12 figures in `STDLIB-PORT.md`
were taken with an in-process harness over the same entries instead (200 calls,
three interleaved rounds, matched container release binaries), which is what
the image can run today; the transport of those numbers through
`run.py --linux` is therefore declared here but not yet a verified transcript.

Adding a Linux workload means adding one frozen script beside the others and
marking it `linux_only` in `WORKLOADS`. Stage a fixture only if the workload
reads a path the container does not already have, mount it inside the
container's own namespace, and never bind a synthetic `/proc` or `/sys` over the
host or over a shared mount.

## The public wrapper's fixed-path reads

`tests/stdlib_port.rs::os_release_entry_reads_the_fixed_paths` exercises the
public `system.os_release` entry against the two paths the entry itself reads,
`/etc/os-release` and `/usr/lib/os-release`. The entry is never redirected: the
route stages container-owned fixtures *at* those paths, inside the container's
own writable layer (nothing outside the container changes, and no `/proc` or
`/sys` path is touched). The fixture contents are committed under
`tests/fixtures/stdlib/os_release/fixed-path/`, reachable in the container
through the `/work` mount, and the scenario name tells the test which of the
three readings to assert:

```sh
run() { # scenario, shell setup
  docker run --rm --privileged --platform linux/arm64 \
    -v "$PWD:/work" -v "$PWD/target:/work/target" \
    -v xsh-cargo-registry:/root/.cargo/registry -w /work \
    -e TARGET=aarch64-unknown-linux-musl -e CARGO_TARGET_DIR=/work/target \
    -e XSH_OS_RELEASE_SCENARIO="$1" \
    xsh-test sh -c "$2; cargo test --features linux-priv-tests --test integration \
      stdlib_port::os_release_entry_reads_the_fixed_paths -- --exact"
}

FIX=/work/tests/fixtures/stdlib/os_release/fixed-path
# The fixture answers from /etc/os-release.
run etc "rm -f /etc/os-release; ln -s $FIX/etc-os-release.txt /etc/os-release"
# /etc/os-release cannot be read; the second path answers.
run fallback "rm -f /etc/os-release; ln -s $FIX/no-such-file.txt /etc/os-release; \
  rm -f /usr/lib/os-release; ln -s $FIX/usr-lib-os-release.txt /usr/lib/os-release"
# Both reads fail, differently: the call reports the second read's failure.
run neither "rm -f /etc/os-release; ln -s $FIX/no-such-file.txt /etc/os-release; \
  rm -f /usr/lib/os-release; ln -s $FIX/not-utf8.txt /usr/lib/os-release"
```

An ordinary container run does not set `XSH_OS_RELEASE_SCENARIO`, so the test
reports itself as skipped there rather than passing quietly, and the container's
real release files are never replaced in a run that does not stage fixtures.

Each scenario points the two fixed paths at the committed sources with
`ln -s` after `rm -f`, rather than with bind mounts, because the image ships
`/etc/os-release` as a symlink to `/usr/lib/os-release`: with the link in place
the two paths are the same file, so bind mounts cannot control them
independently (and a directory cannot be mounted over a file). The first read
fails by pointing `/etc/os-release` at a target that does not exist; the second
one fails in the `neither` scenario with `not-utf8.txt`, which is `NAME=`
followed by the bytes `\xff\xfe`, so the entry reports
`error: system-os-release: file is not valid UTF-8 at byte 5` — the second
read's offset, which is what proves the failure is the second read's.

## Interpreting results

A failing workload needs a profile and a parity check before its cause is
assigned. `STDLIB-PORT.md` lists the remaining acceptance failures; the result
files contain the per-workload medians and samples.

A budget failure says nothing about behavior, because the runner discards each
workload's output. `parity.py` measures that separately — the same scripts, the
same working directory, both binaries, with exit status, stdout, and stderr
compared byte for byte. The six Linux rows print a duration they measured
themselves, so that one field is masked on both sides before the comparison:
the duration is the measurement, not the behavior, and everything else in those
lines still has to match:

```sh
python3 bench/stdlib-port/parity.py \
  --reference /tmp/xsh-reference/target/release/xsh \
  --candidate "$PWD/target/release/xsh"
```

Do not benchmark through `cargo run`, include compilation time, or compare one
revision on a different CPU or container allocation.
