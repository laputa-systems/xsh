# Embedded standard-library port benchmarks

Paired reference/candidate measurement for the standard-library port. The
remaining tasks live only in `IMPROVEMENT-BACKLOG.md`. Reference and candidate
runs are interleaved within one target so CPU frequency, cache, and scheduler
drift affect both sides. The reported value is the median of independent round
medians of wall-clock samples.

## Acceptance snapshot

The original baseline is `37e1502ec928fb0bd1194f056e4f21c8e621d80b`
(B0); the pre-follow-up port is `d0bbc6e74fa2d48e90e174bb6a5f0f4281c0bea2`
(B1). The original **complete** matched B0/B1 release runs are
`results-paired-{b0,b1}.json` on macOS ARM64 and
`results-paired-linux-{b0,b1}.json` in the pinned ARM64 musl image. They each
contain three counterbalanced rounds, raw samples, binary and fixture hashes,
and status, output, and scoped side-effect parity. All 24 macOS and all 30
Linux workloads had parity. Against B0, 18/24 macOS and 18/30 Linux workloads
still failed their fixed budgets; five Linux R12 rows failed, while
`linux_uptime` passed. All 24 macOS rows pass B1 and 28/30 Linux rows pass B1.
The earlier `linux_modules_partial` row used
`collect()[0]` and therefore measured full consumption. These complete runs
predate the targeted text and Linux module improvements below.

Earlier matched reports show corrected `linux_modules_partial.xsh` passing B0
at 28.03 ms versus 42.63 ms, while the script-backed full scan failed at
603.00 ms versus 126.82 ms (`results-linux-modules-{partial,full}-b0-stream-cursor.json`).
B07 restored the native `linux.modules` stream and removed its script parser.
The matched three-round `results-linux-modules-native-b0.json` report passes
both B0 gates with exact status, output, and scoped side-effect parity: full
consumption is 132.81 ms versus 129.13 ms (12.91 ms allowed) and partial
consumption is 24.82 ms versus 46.42 ms (4.64 ms allowed). The report retains
30 samples per side per round and hashes the same B0 binary used by the earlier
targeted report.
The earlier targeted wrap, pad, and small-byte changes saved about 435/544 ms,
6.8/13.5 ms, and 2.8 ms respectively against their immediate prechange
candidates on the measured hosts; each still failed its targeted B0 gate at
that stage. The later B03 native dispositions for padding and formatting are
recorded below.
`results-text-wrap-fastpath.json`, `results-text-pad-seed.json`, and
`results-fmt-small-bytes.json` retain the raw samples, parity, hashes, and
controls. The later B11 cumulative reports below are the complete candidate
runs after the measured dispositions; B01 still owns the owner-run lint row.

B03 isolated 2,000 calls of `time.duration_compact`, plain `tui.left_pad`, and
ANSI `tui.left_pad` on macOS. Before the native disposition, their candidate
times exceeded B0 by 7.74, 17.16, and 44.47 ms respectively, while a loop
control differed by 0.41 ms. Restoring those native calls made `cold_pad` and
`text_pad_batch` pass B0 on both hosts; `fmt_batch` still failed. An isolated
2,000-call `bytes.human` workload then measured a 9.42 ms macOS gap, matching
the remaining formatter-batch cost. Restoring its native operation removed the
now-unused `bytes` embedded module, as the native duration restoration removed
the `time` embedded module; `tui` keeps only its escape-sequence producers in
XSH. `results-b03-native-disposition.json` preserves all six diagnostic and
matched reports with raw samples and hashes. The fixed-workload reports check
exact output, status, and scoped effects; the isolated calls check output and
status. In the final three-round reports, `fmt_batch`
measures 13.94 versus 13.78 ms on macOS and 12.75 versus 12.77 ms on Linux;
`cold_pad` and `text_pad_batch` also pass their original B0 budgets on both
hosts. The same report includes five paired macOS peak-RSS samples from
`/usr/bin/time -l`: `fmt_batch` fell from 13.12 MB before these native
dispositions to 12.80 MB, and `text_pad_batch` fell from 14.22 MB to 13.88 MB
(medians, decimal MB). No language-facing signature changed.

For the three fixed CLI batches, `xsh-runtime-stats` counted 1,338,302,
3,492,356, and 1,236,579 execution allocations on the macOS script candidate.
A 200-iteration control constructing the four-field schema without `cli.parse`
made only 7,463. A five-second sample of 20,000 small-schema parses found
3,570 of 4,061 active execution-thread samples under `ExplicitFrames::run`;
`LoweredValue::into_value` and record-shape creation were visible beneath
script calls. Those nested counts overlap and do not assign independent shares
to each operation. The sample command, scaled script, and counts are in
`results-b02-native-cli-macos-b0.json`.

B02 restored the existing native `cli` policy in `src/modules/cli.rs` and
removed the 3,109-line embedded implementation. The native argument-policy
corpus passes. In three interleaved B0 rounds, all three cold CLI calls and all
three 200-parse batches pass the original budgets with exact output, status,
and scoped-effect parity on both hosts. The macOS batches measure 13.50, 15.10,
and 12.88 ms against B0's 13.59, 15.19, and 13.31 ms; pinned Linux measures
12.76, 12.44, and 12.60 ms against B0's 12.55, 12.40, and 12.78 ms. The raw
rounds and binary hashes are in `results-b02-native-cli-{macos,linux}-b0.json`.
Five paired macOS peak-RSS samples per batch put the candidate 0.13–0.25 MB
above B0 (decimal MB); those small differences do not establish a memory gain.
No public CLI signature or policy changed.

B04's two quoting shapes failed the fixed macOS B0 budgets on the current
script candidate by 19.37 and 31.40 ms, with exact parity. The policy had only
two public functions, so `shlex.quote` and `shlex.join` returned to the existing
native implementation in `src/modules/shlex.rs` and the 64-line embedded
source was removed. Three interleaved rounds now pass both original B0 gates
with output, status, and scoped-effect parity: `quote_batch` is 11.72 versus
13.52 ms on macOS and 12.39 versus 12.29 ms on pinned Linux;
`quote_edge_cases` is 12.61 versus 14.69 ms and 12.28 versus 12.24 ms. The
native XSH quoting tests cover safe bytes, Unicode, apostrophes, newlines,
empty words, and joining. `results-b04-cost-shapes.json` contains the
prechange and final raw macOS rounds and the final Linux rounds. Five paired
macOS peak-RSS samples put the candidate 0.34
and 0.47 MB below B0 for the two shapes (decimal MB). The public contract did
not change. The other B04 cost shapes were measured separately below.

INI encoding was the largest remaining gap: 206.54 versus 16.08 ms on macOS
and 245.23 versus 14.14 ms on Linux. `ini.encode` and `ini.write` returned to
the native implementation in `src/modules/ini.rs`, and the embedded INI module
was removed. The three-round `ini_macos` and `ini_linux` reports in
`results-b04-cost-shapes.json` pass the unchanged B0 gate with exact status,
output, and scoped-effect parity: 15.43 versus 15.64 ms on macOS and 12.52
versus 12.75 ms on Linux. Five paired macOS peak-RSS samples put the candidate
at 14.99 MB versus B0's 14.83 MB (decimal medians). The native XSH corpus
covers encode validation, normalization, multiline values, and write error
ordering. Public signatures and behavior did not change.

MIME's 500-lookup batch had measured 97.96 versus 39.03 ms on macOS and 84.36
versus 15.03 ms on Linux. Its three entries returned to the native
`src/modules/mime.rs` implementation, replacing the embedded MIME parser.
The three-round `mime_macos` and `mime_linux` reports pass the original B0
gate with exact status, output, and scoped-effect parity: 39.30 versus 39.48
ms on macOS and 14.69 versus 13.82 ms on Linux (2.00 ms allowed). Five paired
macOS peak-RSS samples put the candidate at 13.22 MB versus B0's 13.07 MB
(decimal medians). The native XSH corpus covers lookups, suffix order, and
the media-type grammar; a Rust test covers host overlay precedence. Public
signatures and behavior did not change.

JSON paths had measured 43.27 versus 18.31 ms on macOS and 41.02 versus 13.90
ms on Linux. `json.get`, `json.set`, and `json.remove` returned to
`src/modules/json.rs`; the embedded module shrank from 441 to 28 lines and
retains only `json.encode_lines` and its diagnostic bridge. The three-round
`json_path_macos` and `json_path_linux` reports pass B0 with exact status,
output, and scoped-effect parity: 16.94 versus 18.08 ms on macOS and 12.42
versus 12.11 ms on Linux (2.00 ms allowed). Five paired macOS peak-RSS samples
put the candidate at 13.71 MB versus B0's 13.32 MB (decimal medians). The
native XSH corpus covers path validation, traversal, updates, removals, and
dynamic argument errors. The JSON Lines script policy stays in place: the
earlier complete B0 report measured it far below B0 on both hosts. The public
signatures remain unchanged; a dynamic non-list path still fails the argument
boundary with `type-error` before traversal.

The fresh `tail_prechange_macos` report confirmed separate macOS gaps for
`env_typed_lookups` and `checksum_batch`: 18.76 versus 13.43 ms and 18.24
versus 13.18 ms. Both already passed in `tail_prechange_linux`. The three
environment conversions returned to their native scoped-overlay operations,
removing `stdlib/env.xsh`. `hash.parse_check_line` returned to
`src/modules/hash.rs`; `stdlib/hash.xsh` now contains only `hash.verify_file`.
The three-round `tail_native_macos` and `tail_native_linux` reports pass both
fixed B0 gates with exact parity: environment is 12.96 versus 13.25 ms on
macOS and 12.24 versus 12.15 ms on Linux; checksum is 12.31 versus 12.34 ms
and 12.55 versus 12.34 ms. Five paired macOS peak-RSS samples put each
candidate about 0.1 MB above B0 (decimal medians). Native XSH tests cover
scoped environment values and errors, integer grammar, checksum separators,
carriage returns, markers, and validation order.

The `final_macos` and `final_linux` reports remeasure all eight B04 shapes
together against their original B0 products. All eight pass on each host in
three counterbalanced rounds, with 30 raw samples per side per round, exact
status, output, and scoped-effect parity, and binary, script, and fixture
hashes. `core_command` passes without a separate implementation change. This
closes B04's measured cost shapes. The B11 cumulative reports below pass the
complete frozen performance gate; B01 retains the owner-run lint row.

B01's `results-b01-tooling.json` holds three paired rounds of `xsht api` and
`xsht check core/ls.xsh` for B1 and the current candidate against B0 on macOS
and pinned Linux. Status, stdout, and stderr match exactly. B1's macOS check
is 18.09 versus 11.81 ms and fails the 2 ms end-to-end allowance. The current
candidate passes at 10.98 versus 11.62 ms on macOS and 14.41 versus 14.33 ms
on Linux. Both current API rows pass. Linux `tooling-runner.xsh` accepts the
selected command names so the two permitted rows can be measured without
running `xsht lint`. The lint row remains owner-run under `AGENTS.md`.

B08 measured narrow costs in the indexed runtime after the port passed its
complete B0 performance gate. Moving fully supplied call arguments into frame
slots, recycling decoded call argument vectors, and recycling decoded `if`
branches each removed about one allocation per affected call or branch. Paired
timings improved or stayed flat on macOS; pinned Linux improved for calls and
stayed near its control for branches. An inline call argument buffer slowed
macOS, and a typed integer `+=` specialization slowed pinned Linux, so both
were reverted. These results do not support a broader runtime rewrite. Raw
samples, output parity, allocation counts, RSS, and decisions are in
`bench/call-slot-ownership-b08-2026-09-25.json`,
`bench/call-argument-pool-b08-2026-09-25.json`,
`bench/if-branch-pool-b08-2026-09-25.json`, and
`bench/augmented-int-assignment-b08-2026-09-25.json`.

The B05 seven-shape cold remeasurement is in `results-b05-cold.json`, with
three paired rounds and exact parity on both hosts. The prechange macOS
`cold_dynamic_ref` failed B0 at 13.35 versus 11.23 ms (1.00 ms allowed).
`src/stdlib.rs::required_modules` now prepares the complete set of script
bindings applicable to the current target when `module.load` is reachable;
the catalog still retains sources for other targets. `src/runner.rs` shares
the owned parsed arena with the full checker and lowering, avoiding a deep
arena clone. In `final_macos` and `final_linux`, all seven cold shapes pass
B0 with exact status and output parity. Dynamic reference is 12.27 versus
11.43 ms on macOS and 13.91 versus 14.49 ms on pinned Linux. The final
macOS margin is 0.16 ms below the fixed allowance, so later runtime work
must keep this gate.

B06's receiver-selection follow-up was removed after `Str.wrap` and
`Str.fields` returned to native. No script-backed methods remain in the
registry, so `src/stdlib.rs` no longer needs a pre-check method-name table or
its record-literal exception. `required_modules` still conservatively selects
script-backed module functions and the complete current-target set for
`module.load`.

The script policies retained from R01–R11, including JSON Lines, and Linux
`unix.uptime_seconds` live under `stdlib/`. The CLI policy returned to native after
B02; quoting, INI encoding, MIME, JSON paths, environment conversions, and
checksum parsing followed after the B04
measurements. `bytes.human`,
`time.duration_compact`, and TUI padding returned to their native
implementations after B03 measured their per-call regressions;
`Str.wrap` and `Str.fields` returned to native after the B11 macOS cumulative
gate. `linux.meminfo`, `system.memory`, and `system.os_release` returned to
native after the Linux cumulative run exposed their R12 costs; `linux.modules`
retains its native stream on both targets. G02 `hash.verify_file` is script-backed and
passes B0 on tiny, large, many-small, and handled-error workloads on both
hosts (`results-hash-verify-file-b0.json` and
`results-hash-verify-file-linux-b0.json`). G01 `fs.gitroot`, G03 JSON file
wrappers, G05 interface inventory, and G06 `linux.disk_usage` retain native
host boundaries. G04 routes, G05 block devices, and G07 module policy were
reverted after measured gate failures; the `linux.rfkill_list` prototype was
reverted without a script-versus-native measurement. B10 later measured its
retained native path with a correct fixture, as described below. The port is
accepted only when its cumulative B0 gates and the relevant tests in
`docs/TEST-MAP.md` pass. Retained G boundaries stay native until their own
measurements justify a different disposition.

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
  --rounds 3 \
  --out bench/stdlib-port/results.json
```

Both binary paths must be absolute: each workload runs from this directory
(`cwd` matters to `module.load`), so a relative `target/release/xsh` would not
resolve. The runner itself is invoked from the repository root.

It exits non-zero if any workload exceeds its budget or fails parity. The output keeps each
round's raw samples, side order, warmup order, and median under `round_data`;
`workload_order_by_round` records the reversed workload order on alternating
rounds. The top-level workload medians are medians of independent round medians,
and `*_round_mad_ms` reports their median absolute deviation (`null` with one
round). `samples_per_round` records the fixed count for each workload.
`json_lines_batch` uses ten samples per round because a B0 process takes many
seconds; three rounds retain 30 observations. `failed` reflects the final
performance and parity gate once per workload;
`performance_passed` retains the budget result and `parity_failed` names any
behavior mismatch. The runner attaches exit status, stdout, stderr, and
persistent filesystem effect evidence under each workload's `parity` key. The
four `matches` checks cover status (including the declared expected status),
normalized stdout, stderr, and side effects. Raw output is retained as base64.
All frozen workloads currently expect status 0; intentional nonzero cases must
be declared in `run.py::EXPECTED_STATUSES` before timing. `skip_reasons`
explains every skipped workload. `results-paired-b0.json` and
`results-paired-b1.json` are the complete macOS release reports from before
the short-line `text.wrap` change: three independent
rounds, with ten samples per round for `json_lines_batch` and the declared count
for every other row. Both have exact output, status, and scoped side-effect
parity. `results-followup-b0.json`, `results-followup-b1.json`,
`results-final.json`, and the other result files are historical. Cold startup passes
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
| `text_wrap_unicode.xsh` | end to end | `Str.wrap` over 26,613 short Unicode-containing lines at width 72; normalization and page assembly dominate because none of the fixture lines needs splitting |
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
Linux host-text calls. Only uptime uses embedded policy; the other five use
native readers or the retained native module stream. They are marked `linux_only`, so a run
without `--linux` reports them as skipped and a run with it executes them under
`XSH_LINUX_REAL=1`.

The uptime and memory rows read committed `fixtures/uptime.txt` and
`fixtures/meminfo.txt` staged at the fixed `/proc` paths inside each
container's mount namespace. This keeps their observable values identical for
parity while retaining real file reads and parsing. The two module rows use
the committed 200-row `fixtures/modules-200.txt` at `/proc/modules`.
`linux_modules_partial.xsh` now consumes only the first parsed record with
`first()?`; reports recorded before this fixture correction timed
`collect()[0]`, which parsed every row. Compare only reports with the same
`workload_scripts` hash for that row.
`results-linux-modules-partial-b0-fixed.json` and
`results-linux-modules-partial-b1-fixed.json` contain matched three-round
measurements of the corrected row before lazy line splitting, with 30 raw
samples per side per round. The four
`results-linux-modules-{partial,full}-{b0,b1}-lazy-lines.json` reports measure
both module rows after that change with the same sample plan and parity checks.
`results-linux-modules-{partial,full}-b0-fast-fields.json` measure the subsequent
strict-decimal and `-` used-by fast paths with three matched rounds, 30 samples
per side per round, and exact parity. Full consumption still fails B0.
`results-linux-modules-{partial,full}-b0-stream-cursor.json` repeat the same
paired B0 rows after the shared serial-pipeline cursor change, with the same
sample plan and exact parity.
`results-linux-modules-native-b0.json` records the retained native disposition
against that same B0 binary and fixture, with both corrected workloads and
three matched rounds. Both pass their fixed gates and parity checks.
`linux_modules_phases.xsh` is a diagnostic outside `WORKLOADS`; its two
five-round reports separate raw text acquisition, module-stream creation,
first-row consumption, and full parsing.
`run.py::ensure_unicode_fixture` regenerates the large Unicode input whenever
its bytes differ from the fixed seed, including same-size changes, and records
its hash with each run.
`linux_os_release` reads the pinned image's `/etc/os-release`. The runner
checks all three `/proc` fixture hashes before timing, so both revisions read
the same text.
The gated `linux.routes`, `linux.block_devices`, and module-policy prototypes
failed measured non-hot gates and were reverted. The `linux.rfkill_list`
prototype was reverted with block devices without its own timing. Its retained
native path now has a dedicated two-device fixture in `fixtures/rfkill/` and a
full-consumption workload in `linux_rfkill_list.xsh`. The matched three-round
`results-linux-rfkill-list-native-b0.json` report records 30 raw samples per
side per round, fixture and binary hashes, and exact output/status parity. B0
was 12.29 ms and the current native candidate was 12.26 ms, within the fixed
2.00 ms non-hot allowance. The fixture checks sorted IDs and every reported
field on each scan. This measures the retained native path; it makes no claim
about a script implementation. There is no measured reason to reconsider the
native binding now.

The pinned image has no `/sys/class/rfkill`. For this diagnostic, a privileged
container mounts a private tmpfs over `/sys/class`, creates `rfkill`, and copies
the committed fixture there before invoking `linux-runner.xsh`. The mount and
copy live only in that container's namespace and disappear when it exits. No
host `/sys` path is modified.

With absolute `B0` and `CANDIDATE` release binary paths, one round is:

```sh
docker run --rm --privileged --platform linux/arm64 \
  -v "$PWD:/work:ro" -v "$B0:/bench/reference/xsh:ro" \
  -v "$CANDIDATE:/bench/candidate/xsh:ro" \
  -w /work/bench/stdlib-port -e XSH_LINUX_REAL=1 xsh-test \
  sh -c 'mount -t tmpfs -o mode=0755 tmpfs /sys/class && mkdir /sys/class/rfkill && cp -R /work/bench/stdlib-port/fixtures/rfkill/. /sys/class/rfkill/ && exec "$@"' \
  sh /bench/candidate/xsh /work/bench/stdlib-port/linux-runner.xsh -- \
  /bench/reference/xsh /bench/candidate/xsh 0 linux_rfkill_list 30 0
```

Repeat with round indexes `1` and `2`, then take the median of each side's
three round medians. `linux-runner.xsh` counterbalances side order within each
round. `results-linux-rfkill-list-native-b0.json` retains every resulting
sample and the fixture hashes.

Only the `Dockerfile.test` image runs these rows. Build both `xsh` binaries as
matched `aarch64-unknown-linux-musl` release products in that image using the
flags from `dev/targets.xsh::docker_test_env`, then pass their absolute host
paths to the host Python runner:

```sh
python3 bench/stdlib-port/run.py \
  --reference /absolute/path/to/reference/aarch64-unknown-linux-musl/release/xsh \
  --candidate /absolute/path/to/candidate/aarch64-unknown-linux-musl/release/xsh \
  --rounds 3 --linux --out bench/stdlib-port/results-linux.json
```

`run.py --linux` starts one pinned container per round. Its
`linux-runner.xsh` uses `time.measure` around each child process, so Docker
startup and Python aggregation are outside the timed interval. The report
includes the image ID, platform, target, `Dockerfile.test` and target-policy
hashes, all mounted `/proc` fixture hashes, the module row count, and raw samples for all
workloads. `results-paired-linux-b0.json` and `results-paired-linux-b1.json`
are the matched three-round ARM64 musl release reports for the earlier fixture,
with parity for all 30 workloads. A macOS run without `--linux` records the
R12 skips. The image needs no Python. Earlier uninstrumented R12 figures are
historical.

`results-text-wrap-fastpath.json` records the targeted follow-up: paired
prechange and B0 samples against the current `text_wrap_unicode` workload on
both hosts, allocation traffic on macOS, and a reproducible long-line Unicode
control. It is historical: its shorter-line script optimization still missed
the complete B0 gate. `results-b11-text-wrap.json` records the later fixed
workload disposition with three paired rounds at each step. The prechange
macOS candidate took 557.47 versus 168.43 ms. Restoring native `Str.wrap`
alone left a macOS miss at 184.89 versus 165.86 ms; restoring `Str.fields`
also left Linux over budget at 197.34 versus 174.94 ms. The final native
wrapper processes word slices without per-word chunk vectors or repeated
character recounts. It passes with exact output parity at 118.13 versus
168.06 ms on macOS and 160.08 versus 172.57 ms on pinned Linux. The cumulative
reports below close the B0 performance gate.

The first Linux cumulative B11 round exposed three R12 text readers. The
three-round `results-b11-linux-text-prechange.json` report shows exact parity
but B0 misses: `linux_meminfo` 116.39 versus 24.24 ms, `linux_memory` 221.14
versus 24.80 ms, and `linux_os_release` 46.63 versus 12.37 ms. They returned
to their native readers, and their unused embedded modules and private append
bridge were removed. `results-b11-linux-text-native.json` repeats the same
fixed rows with three rounds and exact parity: 24.58 versus 24.35 ms, 24.33
versus 25.11 ms, and 12.38 versus 12.39 ms respectively. An isolated repeat
of `linux_modules_full` passed all three rounds after a six-row control run
missed its median budget. The full cumulative Linux gate passed.

`results-b11-cumulative-linux.json` is the complete three-round pinned Linux
run after the reader cleanup: all 30 workloads pass their
fixed B0 budgets with exact status, output, and scoped-effect parity. The
restored `linux_meminfo`, `linux_memory`, and `linux_os_release` rows are 24.53
versus 24.49 ms, 24.54 versus 23.97 ms, and 12.31 versus 12.25 ms. Full
module consumption is 138.37 versus 127.99 ms, within its 12.80 ms allowance.
`results-b11-cumulative-macos-final.json` measures the final macOS binary:
all 24 applicable workloads pass across
three rounds with exact parity and six declared Linux-only skips. Unicode
wrapping is 118.68 versus 169.20 ms, and dynamic cold preparation is 12.84
versus 12.57 ms.

After the final source change, `profile_parity` passes across debug/release and
default/no-default-feature `xsh` builds on both hosts. Copied-product reports
`results-b11-copied-{macos,linux}.json` pass all seven checks and validate 58
packaged core scripts. The native stdlib suites pass 237 tests with 26 skips
on macOS and 247 tests with 16 skips on pinned Linux; the stdlib port
integration groups pass 17 and 20 tests respectively. Registry, signature,
façade, and all 35 `xsht api` tests pass on both hosts. The three fixed-path
`system.os_release` Linux scenarios pass with the restored native error text.
`results-text-pad-seed.json` does the same for `text_pad_batch`, with 30 samples
per side per round and separate filler and ANSI allocation controls.
`results-fmt-small-bytes.json` measures the under-1024 `bytes.human` path
within `fmt_batch`, separating it from the unchanged duration formatter.

`hash-verify-file.py` qualifies the gated `hash.verify_file` port separately
from the frozen R workloads. It generates deterministic empty, 740 KiB, and 64
distinct small files in a temporary directory, then compares B0 with the
candidate on successful verification and handled errors. Each shape has three
counterbalanced rounds of 30 samples per side, exact status/stdout/stderr
parity, raw samples, source/fixture/binary hashes, and three peak RSS samples
per side. The end-to-end `max(10% of B0, 2 ms)` budget applies to each shape.
`results-hash-verify-file-b0.json` and
`results-hash-verify-file-linux-b0.json` record the passing macOS and pinned
Linux runs; the Linux timer executes child processes inside the container via
`linux-runner.xsh`, so Docker startup is outside each sample. Reproduce with:

```sh
python3 bench/stdlib-port/hash-verify-file.py \
  --reference /absolute/path/to/B0/release/xsh \
  --candidate /absolute/path/to/candidate/release/xsh \
  --out bench/stdlib-port/results-hash-verify-file-b0.json
```

Use the matching `aarch64-unknown-linux-musl/release/xsh` paths and `--linux`
for the pinned image. The image ID, product hashes, and host are in each report.

Adding a Linux workload means adding one frozen script beside the others and
marking it `linux_only` in `WORKLOADS`. Stage a fixture only if the workload
reads a path the container does not already have, mount it inside the
container's own namespace, and never bind a synthetic `/proc` or `/sys` over the
host or over a shared mount.

## Project tooling

`tooling.py` pairs three fixed `xsht` commands: `api summary`,
`check core/ls.xsh`, and `lint core/ls.xsh`. Use matched release `xsht` binaries
from the same checkouts as the `xsh` comparison:

```sh
python3 bench/stdlib-port/tooling.py \
  --reference /tmp/xsh-reference/target/release/xsht \
  --candidate "$PWD/target/release/xsht" \
  --rounds 3 --samples 30 \
  --out bench/stdlib-port/results-tooling-current.json
```

For Linux, build both products in the pinned `Dockerfile.test` environment and
add `--linux` and `--candidate-xsh /absolute/path/to/candidate/xsh`. The XSH
runner measures each `xsht` child inside one container per round; Docker
startup is excluded. The report retains hashes, command argv, independent raw
samples, side order, dispersion, expected-zero output parity, and the original
end-to-end budget. `results-tooling.json` is a historical standalone round;
it is not evidence from this paired route. `tooling.py --self-test` checks the
paired route with a fake executable. The tooling route does not change the 24
frozen standard-library workload scripts or their gate.

## Copied products and packaged core scripts

`tools/copied-product-smoke.py` checks the three copied product binaries from
a temporary directory with no source checkout. It verifies the core archive's
checksum sidecar, exact source-file set, file bytes, and executable modes before
extracting it. It then runs a script-backed `xsh` call, a non-TTY `xshi`
submission, `xsht check` and `api summary`, the packaged `core/getty` module
import, and `core/ls` and `core/cat` scripts with a minimal environment. The
Linux route mounts only the
temporary bundle into the pinned `xsh-test` image. The macOS and Linux reports
are `results-copied-products-macos.json` and
`results-copied-products-linux.json`; they retain binary and archive hashes and
the output fingerprints for all seven checks. The later
`results-b11-copied-{macos,linux}.json` reports use the final B11 debug
products and the same unchanged core archive.
`release core` expects no other `.xz` artifact in `dist/`.

```sh
cargo build -p xsh -p xshi -p xsht --bin xsh --bin xshi --bin xsht
target/debug/xsh dev/main.xsh release core --tag g03-smoke
python3 tools/copied-product-smoke.py --bin-dir target/debug \
  --core-archive dist/core-g03-smoke.tar.xz \
  --out target/copied-products-macos.json
```

For Linux, build the same three debug products with target
`aarch64-unknown-linux-musl` and the flags in
`dev/targets.xsh::docker_test_env` inside `Dockerfile.test`, then use
`--bin-dir target/aarch64-unknown-linux-musl/debug --linux` and a separate
output path. The core archive is platform independent.

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
`error: system-os-release: stream did not contain valid UTF-8`. A missing
first path would report a file-not-found error, so this message proves the
second read was attempted and its failure was reported.

## Interpreting results

A failing workload needs a profile before its cause is assigned.
The acceptance snapshot above and B items in `IMPROVEMENT-BACKLOG.md` name
the remaining failures; the result files
contain per-workload medians, samples, and parity evidence. On macOS, side
effects cover the benchmark directory and `core/` tree before and after each
run. On Linux, each side runs in a fresh container and side effects cover its
writable layer outside the read-only repository and executable mounts. The
Linux effect report includes changes made during container startup; both sides
use the same pinned image. Effects outside these scopes are not compared.

The six Linux rows print a duration they measured themselves. `parity.py`
masks only that duration field in normalized stdout before comparison, while
retaining raw stdout in the report. Other output bytes must match. The checker
can also run on its own, with `--linux` for Linux binaries and `--out` for JSON:

```sh
python3 bench/stdlib-port/parity.py \
  --reference /tmp/xsh-reference/target/release/xsh \
  --candidate "$PWD/target/release/xsh"
```

Do not benchmark through `cargo run`, include compilation time, or compare one
revision on a different CPU or container allocation.
