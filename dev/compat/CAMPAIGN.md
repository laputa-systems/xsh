# XSH Core Compatibility Campaign

Status: active; resumed on 2026-10-07. The full campaign remains incomplete.
The 2026-10-06 wind-down report below is historical and does not describe the
current integration run.

## Campaign verification policy (2026-10-07)

Campaign gates measure correctness and behavioral parity. Performance
benchmarks, latency or memory thresholds, and throughput targets are not
acceptance gates. A timeout remains a parity-test result, but it does not add
a separate performance requirement.

Build and test with Cargo's debug profiles only, using modest optimization
and many codegen units to keep iteration builds quick while retaining debug
assertions:

```sh
export CARGO_PROFILE_DEV_OPT_LEVEL=1
export CARGO_PROFILE_DEV_CODEGEN_UNITS=256
export CARGO_PROFILE_DEV_LTO=false
export CARGO_PROFILE_TEST_OPT_LEVEL=1
export CARGO_PROFILE_TEST_CODEGEN_UNITS=256
export CARGO_PROFILE_TEST_LTO=false
cargo build -p xsh --bins -p xsht --bin xsht
```

Use `target/debug/xsh` and `target/debug/xsht`; this keeps debug assertions,
uses opt-level 1 and 256 codegen units, and disables LTO. Do not use `--release`
or `--profile dist` for campaign verification. Keep Linux checks inside the
`Dockerfile.test` image. Earlier release-build results and benchmark notes
remain historical evidence only.

## Resume checkpoint (2026-10-07)

- Verified the configured host `kache` rustc wrapper and `mold` linker after
  reboot: a disposable build invoked both, and an identical build from a
  second worktree was a Kache cache hit. Host Cargo is available at
  `/home/josh/.cargo/bin/cargo`; it is not on the login `PATH`.
- Resumed implementation with the `stty` integer suffix fix. GNU accepts
  lowercase `b` as 512 and uppercase `B` as 1024; `k` remains invalid. The
  focused native suite passed 26 tests on x86_64 musl and 26 under the pinned
  aarch64-musl QEMU environment.
- A diagnostic uutils slice for `printf`, `split`, and `tee` selected 320 tests:
  165 passed, 154 failed, and 1 was excluded. This is not a full-suite result;
  the report is scratch evidence under ignored `target/compat-resume-uutils`.
- The resumed campaign uses only **`gpt-6-luna` at `xhigh`** for subagents.
  The 16 assigned lanes had disjoint applet and test files and have completed
  their work. Their ownership record is in `CLAIMS.md`. XSH owns command
  behavior, with Rust limited to necessary reusable OS and byte boundaries.

## Current focused integration (2026-10-07)

The same 16 lane owners completed longer, disjoint follow-up tasks based on
`4273cc46`; their source commits are integrated on `master`. The full Gate 3
and Gate 4 reports below remain historical at `92a91b12`; the complete suites
have not been rerun after this wave.

| Lane | Pinned uutils slice | Owned native suites |
|---|---:|---:|
| `awk` | outside Gate 3 | 20 passed |
| `sed` | outside Gate 3 | 27 passed |
| `date` | 161/185 → 169/185 (+8) | date + dircolors: 23 passed |
| `fs-basic` | 192/210 → 197/210 (+5) | 37 passed |
| `fs-misc` | 274/302 → 282/302 (+8) | 54 passed, 1 expected skip |
| `bytes-enc` | 284/310 → 291/310 (+7) | 59 passed |
| `text-a1` | 270/360 → 281/360 (+11) | 62 passed |
| `text-b1` | 470/520 → 482/520 (+12) | 48 passed |
| `cp` | 348/386 → 356/386 (+8) | 43 passed |
| `proc-a` | 106/120, unchanged | 58 passed |
| `perm` | 122/146 → 123/146 (+1) | 42 passed, 10 permission skips |
| `stat-du-df` | 226/236 → 235/236 (+9) | 33 passed |
| `mv-ln` | 333/343 → 337/343 (+4) | 61 passed; protected-target case also passed as UID 1000 |
| `ls` | 181/207 → 186/207 (+5) | 35 passed |
| `printf-env` | 186/252 → 194/252 (+8 in `env`; `printf` unchanged) | 51 passed |
| `checksums` | 504/504 | 65 passed |

Among the measured slices with pass gains, 86 previously failing IDs now pass;
every comparison reports zero regressions. GNU 9.12 checksum tests pass 20/21;
the remaining error is `cksum-c.sh`'s `strace` EIO injection check. The AWK
lane added the POSIX live `ARGV`/`ARGC` matrix; GNU awk was unavailable. The sed
lane added address and replacement coverage and checked the local BusyBox
oracle. No Rust parser or applet backend was added.

## Sort merge follow-up (`a860f47a`)

`sort -m` now merges regular file inputs incrementally to stdout, using bounded
record buffers and stopping input reads after an intermediate write failure.
Other merge paths retain the existing in-memory behavior. The change also adds
`--batch-size` validation, accepts the escaped `-t '\\0'` separator, and
rejects separators longer than one character. The native sort suite passes
39/39. Gate 3 improved from 115/217 after `--files0-from` to 133/217, fixing
18 tests with no regressions; the latest incremental comparison is 130/217 to
133/217. The newest fixed IDs are `test_merge_write_error_does_not_panic`,
`test_separator_attached_equals_double`, and
`test_separator_attached_equals_multi_char`. The run is recorded in
`results/sort-merge-followup/`.

Remaining sort cases include external merge batches, multiple sort keys with
`-t '\\0'`, and a batch-size test whose shell wrapper rejects its reduced file
descriptor limit. The current full 106-utility Gate 3 and 719-test GNU Gate 4
have not been rerun; the campaign remains active and incomplete.

### Current integration result (2026-10-07, `92a91b12`)

- The full native suite passed at parent revision `a0b9ce93` in the pinned
  `Dockerfile.test` image: 4,731 passed, 0 failed, 38 skipped. Later focused
  suites passed for the applets changed in this wave, including cat, chgrp,
  ls, dir, vdir, printf, seq, tail, bytes, tr, od, wc, and stat.
- Gate 3 ran all 106 in-scope utilities against uutils commit
  `e7c9f3194280835c4487c2945c68d5f01ccacc8d` in the pinned x86_64 musl image,
  using the optimized debug profile, three nextest workers, and test children
  as UID/GID 1000. The report records 5,125 pass, 824 fail, 0 skip, and 4
  excluded (5,949 nonexcluded tests). Nextest timed out after 120 seconds in
  `test_wc::test_files0_progressive_stream`. Comparing with the preceding
  full run at the same denominator gives one additional pass and no
  regressions. The full report remains historical; later focused `wc`
  results are recorded below.
- Gate 4 ran the 719 selected GNU 9.12 tests against XSH in the same pinned
  image and debug profile, with three jobs and UID/GID 1000. Results were
  367 PASS, 189 FAIL, 25 ERROR, and 138 SKIP. The four-cell differential
  against the pinned uutils baseline records 358 tests passing on both sides,
  213 passing only on uutils, 9 passing only on XSH, and 52 failing on both.
  In `uniq/uniq-c-width.sh`, the pipeline counted 16,777,216 lines instead of
  30,352,436 because XSH `yes` stopped after 32 MiB. These reports are in
  `results/gnu-xsh.json` and `results/gnu-differential.json`.
- Follow-up at `35faa674` removes that `yes` output cap and streams `uniq -c`
  input. `bytes.repeat_prefix_count` lets XSH count repeated byte-exact records
  within each buffer; other comparison keys stay on the XSH record path. The
  selected `uniq/uniq-c-width.sh` test passes on both XSH and pinned uutils,
  with one shared pass and no mismatches. Its reports are in
  `results/gnu-uniq-followup/`. Focused native suites passed for bytes (5), API
  docs (65), uniq (10), yes (2), head (9), cat (10), tac (8), tail (13), dd
  (23), od (21), and rev (5); `xsht check` passed the three changed source
  modules.
- Follow-up at `d2938a41` makes `wc --files0-from=-` count each complete name
  and flush its output or diagnostic before reading the next one. It stops at
  the first stdout write failure, and `--debug` reports that the XSH line
  counter uses scalar code. The optimized debug Gate 3 `wc` slice passes
  59/59; the focused native suite passes 15/15. This resolves the three `wc`
  cases recorded as failed or timed out in the historical Gate 3 report:
  `test_wc::test_files0_progressive_stream`,
  `test_wc::test_files0_stops_after_stdout_write_error`, and
  `test_wc::test_simd_respects_glibc_tunables`. The per-utility report and
  JUnit are in `results/wc-followup/`.
- The pinned GNU 9.12 `wc` subset also passes on both uutils and XSH: 6 PASS,
  1 SKIP (`wc-sjis.sh`, because Shift-JIS is unavailable), with 6 shared
  passes and no differential mismatches. These reports are in
  `results/wc-followup/`.
- Follow-up at `8f989405` checks every named `sort` input with `fs.stat`
  before opening any of them. A missing later operand is now reported without
  blocking on an earlier FIFO. Its optimized debug Gate 3 slice improves from
  34/183 to 37/180 across 217 cases, fixing three tests with no regressions;
  the native sort suite passes 2/2. Reports are in `results/sort-followup/`.
- Follow-up `273f1312` adds `sort -c`/`-C` conflict diagnostics and checks
  NUL-delimited records with `-z`. Normal `-z` sorting preserves the NUL
  separators. The native sort suite passes 6/6; Gate 3 improves from 37/180
  to 41/176 across 217 tests, with four fixes and no regressions. GNU 9.11
  emits the NUL record terminator in a `-z -c` disorder diagnostic, while the
  corresponding uutils test expects a newline; that test remains a known
  expectation difference. Reports are in `results/sort-check-zero-followup/`.
- Follow-up `3437b52e` keeps an unterminated input file's final record separate
  from the next operand. The focused native sort suite passes 7/7; Gate 3
  improves from 41/176 to 43/174 across 217 cases, fixing `sort_multiple` and
  `test_start_buffer` with no regressions. Reports are in
  `results/sort-input-boundary/`.
- Follow-up `dfed771c` applies `sort -b` to the comparison key while retaining
  GNU's raw-line tie-break, and uses that ordering in `-c` checks. Focused
  native sort tests pass 8/8, and a pinned GNU C-locale probe matches. The
  Gate 3 slice remains 43/174 with no regressions; its `test_blanks` also
  invokes `--debug`, which this applet does not yet implement. The report is
  in `results/sort-blanks-followup/`.
- Follow-up `5f7eedf1` adds GNU `sort --version` output. Focused native sort
  tests pass 9/9; the optimized debug Gate 3 slice improves from 43/174 to
  44/173 across 217 tests, fixing `test_no_error_for_version` with no
  regressions. Reports are in `results/sort-version-followup/`.
- Follow-up `f1aef7a1` accepts `--output`, allows repeated identical output
  paths, rejects different destinations, and preserves output paths beginning
  with `--`. Focused native sort tests pass 10/10; Gate 3 improves from 44/173
  to 46/171 across 217 tests, fixing `test_error_on_multiple_output_flags`
  and `test_output_file_with_leading_dash` with no regressions. GNU probes
  match; reports are in `results/sort-output-followup/`.
- Follow-up `5cd19a0e` reports failed `-o` destination opens with GNU's status
  and diagnostic. Focused native sort tests pass 11/11; Gate 3 improves from
  46/171 to 47/170 across 217 tests, fixing `test_verifies_out_file` with no
  regressions. Reports are in `results/sort-output-open-followup/`.
- Follow-up `373a6c63` uses GNU's uppercase case-fold key and raw-line tie
  breaker for `sort -f`. Focused native sort tests pass 12/12; Gate 3 improves
  from 47/170 to 49/168 across 217 tests, fixing both punctuation-ordering
  tests with no regressions. The GNU 9.11 C-locale probe matches; reports are
  in `results/sort-fold-case/`.
- Follow-up `252844d4` implements dictionary and nonprinting character keys,
  including their conflicts with numeric sorting. Focused native sort tests
  pass 14/14; Gate 3 improves from 49/168 to 51/166 across 217 tests, fixing
  the Unicode character cases with no regressions. GNU 9.11 probes match;
  reports are in `results/sort-char-modes/`.
- Follow-up `6a652532` treats a leading `+` as nonnumeric for `sort -n` and
  uses the full line to order equal numeric keys. Focused native sort tests
  pass 15/15; Gate 3 improves from 51/166 to 52/165 across 217 tests, fixing
  the leading-plus case with no regressions. GNU 9.11 output matches; reports
  are in `results/sort-numeric-plus/`.
- Follow-up `0ec61198` adds stable primary-key ordering with `-s/--stable`.
  Focused native sort tests pass 16/16; Gate 3 improves from 52/165 to 54/163
  across 217 tests, fixing `test_keys_stable` and `test_sort_locale_punctuation`
  with no regressions. GNU 9.11 probes match; reports are in
  `results/sort-stable/`.
- Follow-up `fc7d8b16` adds natural version ordering through `-V`,
  `--version-sort`, and `--sort=version`, with stable ties and leading-dot
  ordering. Focused native sort tests pass 17/17; Gate 3 improves from 54/163
  to 56/161 across 217 tests, fixing the version-sort stable and unstable
  cases with no regressions. GNU 9.11 probes match; the two helper cases that
  add `--debug` still fail because key annotations are not implemented. Reports
  are in `results/sort-version/`.
- Follow-up `60485511` adds `--debug` annotations for the supported default,
  case-folded, dictionary, zero-delimited, and version sort paths. Focused
  native sort tests pass 18/18; Gate 3 improves from 56/161 to 63/154 across
  217 tests, fixing `default_unsorted_ints`, `dictionary_order`, `ignore_case`,
  `version`, `version_empty_lines`, `words_unique`, and `zero_terminated` with
  no regressions. Detailed key-range and locale debug annotations remain open;
  reports are in `results/sort-debug/`.
- Follow-up `3b311480` adds `-g`/`--general-numeric-sort` and
  `--sort=g`/`--sort=general-numeric`. XSH parses decimal and hexadecimal
  numeric prefixes, orders binary64 values through a text key, and uses numeric
  equality for `-u` and `-c`.
  Native sort tests pass 23/23; the optimized debug Gate 3 slice improves from
  63/154 to 69/148 across 217 tests, fixing six cases with no regressions.
  Reports are in `results/sort-general-numeric-followup/`.
- Follow-up `55e6549e` replaces integer-only `-n` keys with exact decimal
  ordering, including fractional and trailing-text prefixes, numeric
  uniqueness, `--sort=n` aliases, numeric key character offsets, and reverse
  text keys. It also omits the secondary whole-line `--debug` annotation under
  `-u`. The native sort suite passes 27/27; the optimized debug Gate 3 slice
  improves from 69/148 to 88/129 across 217 tests, fixing 19 cases with no
  regressions. Remaining general-numeric cases remain open. Reports are in
  `results/sort-numeric-debug-followup/`.
- Follow-up `ed9055ce` adds `-h`/`--human-numeric-sort` and the `--sort`
  aliases. XSH orders recognized units before exact decimal values, preserves
  stable zero ties, and matches GNU's human numeric key annotations. Focused
  native sort tests pass 30/30; the optimized debug Gate 3 slice improves from
  88/129 to 93/124 across 217 tests, fixing five cases with no regressions.
  Pinned GNU 9.12 probes match unit order and zero handling. Reports are in
  `results/sort-human-numeric-followup/`.
- Follow-up `f7f34cd2` keeps the first input spelling for each equal `-n` key
  when `-u` is combined with reverse sorting. The focused native sort suite
  passes 31/31; Gate 3 improves from 93/124 to 94/123 across 217 tests, fixing
  `test_mixed_floats_ints_chars_numeric_reverse` with no regressions. A pinned
  GNU 9.12 probe matches the retained representatives. Reports are in
  `results/sort-numeric-unique-reverse-followup/`.
- Follow-up `02d9ef8e` orders failed `-g` conversions before NaN and numeric
  values, and accepts abbreviated `--sort=general-numeric` mode names. Focused
  native sort tests pass 32/32; Gate 3 improves from 94/123 to 95/122 across
  217 tests, fixing `test_multiple_decimals_general` with no regressions. The
  pinned GNU 9.12 fixture matches. Reports are in
  `results/sort-general-invalid-followup/`.
- Follow-up `df8def80` adds C-locale English month sorting with `-M`,
  `--month-sort`, and abbreviated `--sort=month` modes. It handles leading
  blanks, unknown prefixes, stable ties, uniqueness, diagnostics, and debug
  annotations. The native sort suite passes 33/33; Gate 3 improves from
  95/122 to 100/117 across 217 tests, fixing five month cases with no
  regressions. GNU Coreutils 9.11 order and check-mode probes match; localized
  `LC_TIME` month names remain open. Reports are in
  `results/sort-month-followup/`.
- Follow-up `2309ff5e` adds `--files0-from` input lists, preserving arbitrary
  POSIX filename bytes and rejecting empty entries before opening operands.
  The native sort suite passes 34/34; Gate 3 improves from 100/117 to 115/102
  across 217 tests, fixing 15 list-input cases with no regressions. Reports are
  in `results/sort-files0-followup/`.
- Follow-up at `76f5b213` keeps `tail --sleep-interval` as an optional value,
  distinguishing an unset option from an explicitly empty argument. The empty
  value now fails with the diagnostic emitted by GNU 9.11; the native tail
  suite passes 14/14. The pinned Gate 3 tail slice remains 102/167 with the
  same failing IDs, since its malformed-interval cases also require a help
  hint that GNU 9.11 does not print. Its report and JUnit are in
  `results/tail-followup/`.
- Follow-up at `8bc16fdd` rejects the zero flag for `%c`/`%s` and precision on
  `%c`, warns about operands left after a literal-only format, and suppresses
  that warning when `\c` stops output. The native printf suite passes 19/19;
  its optimized debug Gate 3 slice improves from 109/43 to 113/39 across 152
  cases, with four fixes and no regressions. Reports are in
  `results/printf-followup/`.
- Follow-up `579b2195` recognizes `%q` as shell quoting, keeps `~` unquoted,
  rejects field parameters on `%q`, and writes the literal prefix before an
  invalid conversion error. Direct probes match pinned GNU `printf`. The
  native printf suite passes 21/21; the optimized debug Gate 3 slice improves
  from 113/39 to 117/35 across 152 cases, with four fixes and no regressions.
  Reports are in `results/printf-q-followup/`.
- Follow-up `ce22226e` omits the integer digit for zero when precision is zero,
  while retaining the alternate-form octal `0`. The native printf suite passes
  22/22; its optimized debug Gate 3 slice improves from 117/35 to 118/34
  across 152 cases, with one fix and no regressions. Reports are in
  `results/printf-zero-followup/`.
- Follow-up `a142979c` rejects field parameters on `%b`, which GNU printf also
  rejects, and emits preceding literal text before the error. The native
  printf suite passes 23/23; the optimized debug Gate 3 slice improves from
  118/34 to 119/33 across 152 cases, with one fix and no regressions. Reports
  are in `results/printf-b-followup/`.
- Follow-up `6c4cfa63` reports missing hexadecimal escapes and invalid or
  incomplete universal character escapes, preserving text printed before the
  error. The native printf suite passes 24/24; the optimized debug Gate 3
  slice improves from 119/33 to 122/30 across 152 cases, with three fixes and
  no regressions. Reports are in `results/printf-escape-followup/`.
- Follow-up `21b619c0` formats NaN casing through explicit float display text
  and disables zero padding for NaN and infinity, matching pinned GNU printf.
  The native printf suite passes 25/25; the optimized debug Gate 3 slice
  improves from 122/30 to 125/27 across 152 cases, with three fixes and no
  regressions. Reports are in `results/printf-float-special-followup/`.
- Follow-up `90ce7294` trims insignificant zeroes from `%g` mantissas before
  their exponent. A pinned GNU probe matches; the native printf suite passes
  26/26, and the optimized debug Gate 3 slice improves from 125/27 to 126/26
  across 152 cases with one fix and no regressions. Reports are in
  `results/printf-g-followup/`.
- Follow-up `f5d04662` uses `Float.format_number` for fixed-point precision
  above the scalar formatter's limit. A 70,123-digit request matches pinned
  GNU; the native printf suite passes 27/27, and the optimized debug Gate 3
  slice improves from 126/26 to 127/25 across 152 cases with one fix and no
  regressions. Reports are in `results/printf-precision-followup/`.
- Follow-up `79c1dca3` detects decimal floating overflow and underflow,
  retaining GNU's `inf` or zero output and `Result not representable`
  diagnostic. Eight direct GNU 9.12 probes match; the native printf suite
  passes 28/28. The Gate 3 slice stays 127/25 because
  `test_extreme_exponent_does_not_overflow` expects `Numerical result out of
  range` instead. Its report is in `results/printf-range-followup/`.
- Follow-up `ac1a8af5` flushes `printf` output and reports stdout write errors
  through the GNU helper. The native printf suite passes 29/29; the optimized
  debug Gate 3 slice improves from 127/25 to 129/23 across 152 cases, fixing
  both `/dev/full` write-error cases with no regressions. Reports are in
  `results/printf-write-followup/`.
- Follow-up `964f2fdf` rejects dynamic precision above `INT_MAX` before an
  excessive allocation and matches GNU's invalid-precision diagnostic. The
  native printf suite passes 30/30; the optimized debug Gate 3 slice improves
  from 129/23 to 130/22 across 152 cases, with one fix and no regressions.
  Reports are in `results/printf-precision-limit/`.
- Follow-up `af249f2b` rejects zero-based positional references before they
  index the argument list and reports GNU's `%0$` diagnostic. The native printf
  suite passes 31/31; the optimized debug Gate 3 slice improves from 130/22 to
  131/21 across 152 cases, with one fix and no regressions. Reports are in
  `results/printf-position-index/`.
- Follow-up `4e9eb3bd` reports characters after a numeric character constant,
  unless `POSIXLY_CORRECT` is set. This matches pinned GNU 9.11 and fixes the
  warning case plus two partial-character cases. The native printf suite
  passes 32/32; the optimized debug Gate 3 slice improves from 131/21 to
  134/18 across 152 cases, with no regressions. Reports are in
  `results/printf-char-warning/`.
- Follow-up `6c6af179` uses GNU shell-escape quoting for `%q` arguments with
  control bytes and embedded apostrophes. The native printf suite passes
  33/33; the optimized debug Gate 3 slice improves from 134/18 to 135/17
  across 152 cases, with one fix and no regressions. Its control-and-apostrophe
  result matches pinned GNU 9.11. Reports are in `results/printf-shell-quote/`.
- Follow-up `d7859e78` lets `%b` consume up to three octal digits after its
  `\0` prefix while retaining the format-string escape width. The native
  printf suite passes 33/33; the optimized debug Gate 3 slice improves from
  135/17 to 137/15 across 152 cases, fixing two escape cases with no
  regressions. Reports are in `results/printf-octal-b/`.
- Follow-up `70db4c11` keeps printf formats, arguments, and output as bytes,
  forwards raw command arguments through `env`, and quotes UTF-8 C1 control
  characters with `%q`. The focused native suites pass (env 8/8, printf 35/35,
  GNU helpers 14 passed and one root permission skip). The optimized debug Gate 3
  printf slice improves from 137/15 to 141/11 across 152 cases, fixing raw
  `%c`, multibyte character constants, and invalid UTF-8 arguments with no
  regressions. Pinned GNU 9.12 `printf-quote.sh` and `printf-mb.sh` both pass
  on XSH and uutils. Reports are in `results/printf-raw-bytes/`.
- Follow-ups `8fbf4847` and `981b1751` stream large `printf` fields in 64 KiB
  chunks, reject widths that overflow the C formatter's output count, validate
  conversion flags, and report malformed dynamic width or precision values.
  The native printf suite passes 39/39. The optimized debug Gate 3 slice
  improves from 141/11 to 142/10 across 152 cases, with one fix and no
  regressions. Its remaining `test_large_width_format` failure expects an
  error for a valid 20-million-character field; a GNU 9.11 probe accepts and
  writes that width. The pinned GNU 9.12 `printf.sh` selection passes on both
  XSH and uutils with one shared pass and no mismatches. JSON reports are in
  `results/printf-width-followup/`.
- The full Gate 3 and Gate 4 reports describe revision `92a91b12`. After those
  runs, focused changes to `stat`, `sort`, `yes`, `uniq`, and `wc` have landed,
  but the complete suites have not been rerun. `stat -` now uses `/dev/stdin`;
  its native suite passed 9/9 and the kernel-reader ratchet returned zero
  direct readers.
- The generated per-utility manifest uses the current Gate 3 report.
  `parity.py --check`, `lanes.py check`, the ignored-option ratchet, and the
  four-exclusion check passed.

The campaign remains active and incomplete. The 824 failing uutils cases in
the historical full report, remaining GNU differential gaps, BusyBox
comparison, expanded Linux surface, aarch64 debug gate, and clean-image smoke
remain open. The selected `uniq` stress case and the previously failing `wc`
slice pass in focused follow-ups; the `sort` slice fixes three tests with no
regressions. The full Gate 4 report still describes its earlier state at
`92a91b12`.

## Wind-down report (2026-10-06)

- All 106 in-scope coreutils applets are present. Presence does not establish
  parity. 99 of 125 expanded commands are present; the denominator is unchanged.
- Awk, sed, FAT geometry/checking and human date parsing now live in XSH
  libraries. Whole-program Rust awk/sed/FAT/date-parser backends were removed.
  Rust provides shared codecs, byte/regex operations and OS boundaries.
- Campaign dependencies have standing user approval. Native zstd streaming
  uses `zstd` 0.13.3 with default features disabled.
- Existing account, ACL/capability, DNS and system-control lanes were integrated.
  Unconsumed HTTP and namespace prototypes were removed.
- Ignored-option buckets and direct applet kernel-reader ratchets are zero.
  The four existing upstream exclusions are unchanged.
- Remaining applet limits are recorded in `gaps.json`. Namespace applets,
  storage health/EFI, broader network tools, BusyBox comparison, clean-image
  smoke and a final zero-blocker GNU differential remain unfinished.
- Committed compatibility reports below remain historical: the session’s
  intermediate full uutils run reached 4,347 passes and 1,602 failures with
  four exclusions, but preceded final integration. It is not final-head
  evidence and is not substituted for the historical canonical report.

### Final verification

Implementation revision: `60549b3c`. All builds and Linux runs used the
`Dockerfile.test` image, pinned nightly compiler and static musl symbol flags.
Release snapshots were fixed for each run; native suites ran as UID/GID 1000.

| Check | Result |
|---|---|
| Core check | All 443 XSH files checked, no diagnostics |
| Full core native suite, x86_64 musl | 1,057 passed, 0 failed, 14 skipped |
| Full standard-library suite, x86_64 musl | 688 passed, 0 failed, 22 skipped |
| DNS Rust fixture integration | 1 passed; owned loopback server |
| Generic exec environment and login | 7 focused tests passed |
| Documentation generation/check | Passed, including 3 tour project tests |
| Compatibility stage regression | 1 native test passed |
| GNU runner harness | 3 Python tests passed, 1 platform test skipped |
| Offline inventory, ownership and ratchets | Passed; 0 ignored buckets, 0 direct readers, 4 unchanged exclusions |
| Strict aarch64-musl release build | Passed with the exact target flags |

The x86_64 runs are iteration evidence, not a substitute for the pinned
aarch64 Linux support gate. AArch64 binaries were also run through QEMU in
an isolated user/mount namespace with UID/GID 1000 and no effective
capabilities. Eight focused command checks passed. The broader standard-library
run had 683 passes, 2 failures and 22 skips: one sampling test fixture was
subsequently corrected and passes in the final iteration run; the identity
test sees a parent PID of zero under emulation. The broad emulated core run
was stopped after 105 passes and one machine-name mismatch against native
host tools. Neither broad emulated run establishes a passing native AArch64
gate. Tests were retained; no exclusions were added to hide these results.

Logs remain in ignored scratch directories `target/compat-winddown` and
`target/aarch64-verification/evidence`. Session containers were stopped after
verification. No push or formatter was run. The campaign is incomplete; this
wind-down report releases the lanes rather than claiming its remaining gates.

## Operational handoff (2026-10-05)

This refresh inspected local Git history, worktrees, committed reports, and
campaign ratchets at `37df79b6`. It did not fetch remote branches or rerun the
native, uutils, or GNU suites. Counts below are committed baselines, not a
claim that the current head passes them.

### Verified repository state

- `master` merged the compatibility branch in `9e6428b1`. The locally recorded
  `origin/campaign-utils` tip, `95e35002`, is an ancestor of the current head.
  Continue from `master`; do not resume the old branch as the integration base.
- Subsequent commits migrated the merged applets to the current language and
  regenerated docs and the API surface fixture (`11525cef`), followed by lint
  cleanup. `37df79b6` fixes atomic-replacement lint so it preserves no-clobber
  publications. These changes have not received a fresh compatibility run.
- `git worktree list` shows only the main checkout, and there are no local
  `lane/*` branches. [`CLAIMS.md`](CLAIMS.md) records no verified active local
  claims. Old session IDs and “running” rows are historical; remote workers
  have not been checked.
- The campaign's implementation and regression backlog is in
  [`requests.md`](requests.md), [`gaps.json`](gaps.json), and the current
  repository [`TODO.md`](../../TODO.md). Old runtime requests must be checked
  against today's API and tests before they are treated as missing features.

### Committed scoreboard

The last full uutils report was recorded in `95e35002`, before the merge and
language migrations on `master`:

| Evidence | Recorded state |
|---|---|
| `results/uutils-integration.json` | 2,186 pass, 3,788 fail, 0 skip; 4 per-test exclusions |
| `dev/coreutils-parity.json` | 73 of 106 in-scope utilities present; 33 missing; 108 upstream utilities, 2 capability-gated |
| Expanded Linux surface | 125 commands, 2 present |
| `results/gnu-uutils.json` | 571 PASS, 46 FAIL, 101 SKIP, 1 ERROR across 719 tests |
| XSH GNU run and differential | no committed `gnu-xsh.json` or `gnu-differential.json` |

Presence means an applet exists, not behavioral parity. Reports do not prove
current-head compatibility. Preserve this baseline before publishing a new
full run; compare per-test outcomes, not just totals.

The following offline checks passed during this refresh: `parity.py --check`,
`lanes.py check` (27 lanes covering all 106 in-scope utilities),
`check_ignored_options.py` (14 legacy buckets, none new),
`check_kernel_reads.py` (0 direct-reader applets), and
`check_exclusions.py` (4 categorized exclusions). An unchanged ratchet does
not mean its legacy gaps are resolved.

### Next steps, in order

1. Choose an explicit `master` revision and record the environment and tool
   paths. Read `AGENTS.md`, `docs/TESTING.md`, and `TODO.md`; use the repository's
   Linux test image for Linux verification. Build the required debug-profile tools
   from that revision and keep their paths stable for the whole run. Follow
   [`README.md`](README.md) for the pinned reference checkout and harness
   prerequisites; reference trees were not located under `../ref` in this audit.
2. Run the current native/core gates. Triage failures against `TODO.md` rather
   than importing the old session's 2,459/10 native-suite count. That file
   records musl-sensitive `stty`, `ls`, and `uniq` expectations; verify those
   expectations through correctness tests.
3. Save the committed uutils report outside `results/`, run the full uutils
   suite, compare with `compare.py`, and regenerate `coreutils-parity.json`
   using the pinned checkout. Record revision, host/libc, pass/fail/exclusion
   counts and regressions. Run one full suite at a time.
4. Investigate the historical `uniq` failure
   `test_uniq::test_obsolete_skip_fields_not_read_after_double_dash`, then
   triage `requests.md` and `gaps.json` against the new report. Verify exclusion
   candidates against pinned GNU 9.12 before changing `exclusions.json`.
5. Choose the next unfinished lane from `lanes.json`, check its dependencies
   and current implementation, and record ownership in `CLAIMS.md` before
   starting it. A lane's existence or applet presence is not a completion gate.
6. Run the XSH side of the GNU suite and generate its differential against the
   stored pinned-uutils baseline. Continue the waves and verification gates
   below; campaign completion still requires all applicable tests, zero GNU
   differential blockers, and resolution of legacy ignored-option buckets.

### Working constraints

The 2026-10-06 implementation direction is **XSH first**. Command grammar,
parsers, interpreters, option semantics, traversal, selection, formatting
policy, and repair decisions belong in XSH, including shared domain libraries.
A command implemented mostly as a Rust program with an XSH entry wrapper does
not meet this campaign's purpose. In particular, awk and sed parsers and
execution engines, FAT geometry and checking, and human date parsing are XSH
work. The Rust whole-program implementations were removed during this session.

Rust supplies small reusable boundaries for syscalls, descriptors, codecs,
binary representation, and operations that cannot be expressed faithfully in
XSH. Native API requests must state what XSH cannot express and why that
primitive is reusable beyond one command; whole-program `execute` or `transform`
APIs are not a substitute for implementing the program in XSH. Performance is
not a reason to move command policy or execution into Rust during this campaign.

The user granted standing approval on 2026-10-06 for any dependencies needed
by this campaign. Record the reason and keep additions focused; further
dependency approval requests are unnecessary within this campaign.

No accepted option may be silently ignored; denominators never shrink;
reference utilities are test oracles, never runtime dependencies; applets
use typed native APIs instead of parsing another command's text output.
Every campaign subagent must use **`gpt-6-luna` at `xhigh`**, including routine
tasks. Pass both settings explicitly on every spawn. Follow current `AGENTS.md` and session
instructions for tools, delegation, commit authorization, and publishing. Do not
run formatters or autofixers, or push as part of this handoff refresh.

## Mission

XSH becomes the software-defined Linux systems core for Laputa: one typed
language and native substrate that owns the kernel-ABI-facing userland. A
Laputa system with XSH installed does not carry coreutils, util-linux,
procps-ng, kmod, pciutils, usbutils, findutils, grep, the compression CLIs,
dosfstools, smartmontools, nvme-cli, iproute2, ethtool, iw or similar small
distro packages merely because conventional software expects their executable
names. Traditional source-package boundaries are packaging history, not an
architectural constraint.

### Ownership rule

Implement in XSH when the software is primarily

```text
kernel ABI + sysfs/procfs/netlink/ioctl parsing + conventional CLI presentation
```

Package externally when it contains a deep independent implementation: kernel
and firmware; filesystem repair engines (`e2fsck`); GPU drivers and Mesa;
nftables semantics; ptrace syscall-decoding databases (`strace`); perf's PMU
and profile machinery; packet protocol decoders (`tcpdump`); Git, Make and
rsync (protocols and languages). Libraries that third-party binaries link
against (`libcap`, `libacl`, `libattr`, `libcurl`) may still be packaged later
as ABI compatibility for external software; that is orthogonal to owning the
command surface.

Optimize for removing whole categories of traditional distro package
dependencies through one coherent, typed Linux systems substrate, not for the
number of command names implemented.

### Objectives

1. Match the complete practical Linux command surface of `uutils/coreutils`
   with comparable behavioral compatibility.
2. Measure that with uutils' own integration, GNU and BusyBox suites rather
   than a bespoke suite.
3. Own the rest of the kernel-ABI userland listed in
   [`surface.json`](surface.json): util-linux, procps, kmod, pciutils,
   usbutils, findutils, grep, sed/awk/diffutils, the compression family
   (including zstd), dosfstools, block/mount/partition tools, file
   attribute/ACL/capability tools, namespace and process control, storage
   health (`smartctl`, `nvme`), network control and diagnostics (`ip`, `ss`,
   `ethtool`, `iw`, `dig`, `nc`, `curl`), `cpio` and EFI boot variables.
4. Implement command behavior and shared domain logic in XSH. Use small Rust
   primitives where syscalls, codecs, or faithful byte operations require
   them; keep parsers and interpreters in XSH.
5. Never fake a command: an accepted option works, and unsupported behavior
   fails explicitly.
6. Use canonical tools (dosfstools, smartmontools, nvme-cli, util-linux,
   ethtool, iw, xz, zstd, ...) only as test references, never as runtime
   dependencies.

Two constraints govern every decision:

> Laputa should not need dozens of tiny Unix packages merely because
> conventional software expects familiar executable names.

> Do not reproduce BusyBox's worst failure mode: a command name present while
> important semantics quietly do nothing.

## Architecture

### Two interfaces

- **Native XSH APIs** (`fs`, `process`, `unix`, `linux`, `system`, `hash`,
  `compression`, `archive`, `dns`, `net`, `cli`, ...) expose semantics as typed
  records and operations. Rust implementations in `src/modules/` provide the
  necessary host and byte boundaries, documented through `xsht api`. Shared
  semantic domains are XSH modules wherever the language can express them.
- **Compatibility CLIs** (`core/*.xsh`) translate conventional argv into those
  APIs and print conventional output.

One compatibility command never parses another's text output. Shared data
flows through typed collectors:

```text
BAD:   lsusb -> run system-report -> parse human output
GOOD:  typed USB inventory -> { system-report, lsusb }
```

The same holds for PCI, block devices, processes, network state, mounts and
modules. `system-report`'s collectors (`core/lib/system_report_*.xsh`, about
11.7k lines) are extracted into reusable typed modules without weakening its
bounded reads, redaction, safety, or capture/replay guarantees. Presentation
commands may show local detail by default where convention expects it;
`system-report`'s share-safe redaction stays its own policy.

### One model per kernel ABI

The main architectural risk of this scope is duplicating kernel parsing in
dozens of applets. Each kernel ABI gets exactly one typed collector or
controller; applets, `system-report`, Laputa boot and install scripts and user
programs are renderers over it. Target domains (the split follows the existing
module architecture; these names are targets, not required spellings):

`block`, `mount`, `partition`, `fs` (FAT first), `device`, `pci`, `usb`,
`storage_health`, `nvme`, `net`, `wifi`, `process` (including sampling),
`namespace`, `capability`, `xattr`, `acl`, `efi`, `module`, `power`, `sensor`.

- A command reads `/proc`, `/sys`, netlink or ioctl state only through its
  domain's API. An applet that needs a field the domain lacks adds it to the
  domain through the owning lane, never a private reader.
  [`check_kernel_reads.py`](check_kernel_reads.py) ratchets this for top-level
  applets.
- Point-in-time inventory (`system-report`) and time-series sampling (`top`,
  `vmstat`, `iostat`, `pidstat`, `watch`) are different APIs. Sampling has
  explicit sample types: stable process identity (pid plus start time), CPU
  ticks, RSS/VSS, I/O counters, context switches, state, threads, system CPU
  totals, memory, paging and block-I/O counters. It is not built by abusing
  the report collector.
- Mutating operations (partition tables, `wipefs`, `mkfs`, EFI variables,
  `ethtool`/`iw` set, `setcap`, namespace and privilege transitions) validate
  explicitly, act only on a target the caller named, and are tested only
  against loop files, namespaces and synthetic fixtures (Gate 7).

### Shared applet foundation (revised: Wave 0, before any fan-out)

`core/README.md` currently keeps applets self-contained and usage errors
local. The compatibility campaign needs one exception, because uutils and GNU
tests assert exact option grammar, diagnostics and exit codes, and ninety
applets must not each reinvent them:

- **GNU option grammar in `cli`** (native, `src/modules/cli.rs`). Today
  `cli.applet`/`cli.parse` reject repeated options (`duplicate argument at
  argv[N]`; GNU `ls -l -l` is valid, last value wins), do not resolve
  unambiguous long-option abbreviations (`--all` from `--al`), and report
  errors in XSH style. Add a GNU mode: short-option bundling, attached and
  separate values, optional arguments (`--color[=WHEN]`), `--opt=value`,
  abbreviation with ambiguity errors, argument permutation with
  `POSIXLY_CORRECT` and a leading `+` disabling it, `--`, repeated options,
  numeric-option shapes (`head -5`, `nice -n -5`), and an explicit
  `unsupported` declaration that fails with a clear diagnostic.
- **GNU diagnostics and exit codes** (`core/lib/gnu.xsh` plus any native
  support). `PROG: message`, `PROG: cannot access 'X': No such file or
  directory` (with GNU quoting of names), `Try 'PROG --help' for more
  information.`, per-utility usage-error exit status (1 for most, 2 for `ls`,
  `cmp`, `diff`, `grep`; 125 for `env`, `nice`, `nohup`, `timeout`,
  `stdbuf`, `chroot`), `--help`/`--version` handling, and SIGPIPE/EPIPE
  behavior on stdout.
- **Invoked-name access** for applets. Aliases (`dir`, `vdir`, `[`, `egrep`,
  `gunzip`, `zcat`, ...) are installed as symlinks; the applet reads the name
  it was invoked as (the script path the kernel passes) to choose defaults,
  help and diagnostics. Expose this cleanly in the runtime if it is not
  already available.
- **Byte-stream helpers** so binary data never passes through `Str`.

`core/README.md` is updated to describe this shared layer as an audited
command family, matching its existing rule for `lib/auth.xsh` and
`lib/text_input.xsh`.

## Decisions

- **Diagnostic wording is GNU's.** uutils builds on clap and about 60 of its
  roughly 6,000 integration tests assert clap wording (`error: unexpected
  argument`, `a value is required for`, `invalid value`). XSH follows
  GNU `getopt_long` and coreutils wording (`invalid option -- 'x'`,
  `unrecognized option '--foo'`, `cannot access 'X': No such file or
  directory`). Tests that assert clap wording are excluded per test ID with
  category `clap-wording`, never per module; the GNU differential covers the
  GNU wording.
- **Execution phrase.** Usage errors end with `Try 'PHRASE --help' for more
  information.` PHRASE is `$XSH_EXECUTION_PHRASE` when set, else the invoked
  name (the basename of the script path as the kernel passed it; aliases are
  symlinks, so `dir` sees `dir`). The uutils adapter sets it to
  `"<adapter path> <util>"`, the multicall form uutils' `usage_error` helper
  expects.
- **Alias table shape.** `aliases.json` holds `{name, target}` entries, because
  XSH decodes a JSON object as a record and cannot schema-check an open map.
- **Expanded-scope inventory.** [`surface.json`](surface.json) lists every
  command beyond the uutils set with phase, domain and test-only reference
  tool. The count is pinned in `upstream.lock.json` and only grows.

## Phase 0: upstream baseline (done)

- Pinned uutils/coreutils `e7c9f3194280835c4487c2945c68d5f01ccacc8d`
  (`0.12.0-289-ge7c9f3194`, 2026-10-04) in
  [`upstream.lock.json`](upstream.lock.json). Checkout lives outside the tree
  via `UUTILS_ROOT`.
- GNU coreutils 9.12, pinned transitively by uutils' `util/fetch-gnu.sh` at
  that commit; fetched from GitHub releases into `GNU_ROOT`, never vendored.
- [`parity.py`](parity.py) derives the utility list from the pinned
  `Cargo.toml` (`feat_os_unix`, expanded recursively, filtered to `src/uu/*`
  crates) and writes [`../coreutils-parity.json`](../coreutils-parity.json).
  `parity.py --check` fails when the manifest is stale.
- Denominator: **108 upstream utilities = 106 in scope + 2 SELinux-gated**
  (`chcon`, `runcon`, tracked with `capability: "selinux"` and still counted).
  Every `src/uu` crate is accounted for except the `checksum_common` library.
- Baseline presence: **41 of 106** in-scope utilities have an XSH applet; 65
  are missing. XSH has 58 applets in total (17 are outside the uutils set:
  `fd`, `getty`, `host`, `ifdown`, `ifup`, `ip`, `mdev`, `passwd`, `pstree`,
  `rev`, `rg`, `strings`, `su`, `system-report`, `tar`, `tree`, `which`).
- The denominator never shrinks. Exclusions are per test ID, categorized and
  explained, never per module.

Manifest fields per utility: upstream commit (top level), utility, capability
gate, XSH implementation path, present, native test file, uutils test count,
uutils integration pass/fail/skip/excluded, GNU differential, BusyBox
results, excluded test IDs, known semantic gaps.

## Phase 1: harness (Wave 0)

### A. uutils integration suite (Gate 3)

- [`xsh-uutests`](xsh-uutests) implements the multicall contract
  `BINARY UTILITY ARGS...` by exec'ing the staged applet. It is a dispatcher,
  never a second implementation; argv, stdio bytes, exit status, cwd,
  environment and signals pass through untouched (verified with a stand-in
  interpreter, including empty arguments, `--`, and NUL bytes on stdin). A
  missing applet exits 127 with a diagnostic.
- [`stage.py`](stage.py) installs `core/` in its standard staged layout (suffix
  dropped, `lib/` adjacent, aliases as symlinks, shebang pointing at the built
  `xsh`) and writes the stage's `applets.json`. Suites test the installed
  shape, not the source tree.
- [`run-uutils.sh`](run-uutils.sh) builds the uutils `tests` target with
  `--features feat_os_unix` (the test crate requires
  `CARGO_BIN_EXE_coreutils` and per-utility features, so uutils
  implementations are compiled but never exercised), makes the uutils
  `coreutils` binary non-executable as a tripwire for anything that bypasses
  `UUTESTS_BINARY_PATH`, runs `cargo nextest` with
  `UUTESTS_BINARY_PATH=xsh-uutests`, `LC_ALL=C`, `TZ=UTC`, and converts the
  JUnit report with [`results.py`](results.py) into
  `results/uutils-integration.json`. Partial runs (named utilities) update
  only their entries.
- Cross-utility calls such as `scene.ccmd("touch")` also go to XSH. Fix
  widely used cheap applets first; their failures cascade.
- [`host-deps.json`](host-deps.json) lists the 28 utilities whose tests spawn
  host programs directly (`sh`, `strace`, `setfacl`, `unshare`, `locale`,
  ...) or compare against a host GNU tool. These are explicit, not
  accidental: in the clean environment they come from XSH or are excluded
  with category `host-oracle`.

Baseline against the current XSH (2026-10-04, pinned uutils `e7c9f31`, 5,978
tests across 106 utilities, 22.6 minutes at 2 threads): **539 pass, 5,439 fail**
(14 of those timed out; 9.0%). The 41 utilities that already have an applet pass
483 of 3,277 tests. The other 56 passes belong to the 65 utilities without an
applet: "command not found" satisfies every expects-failure test, so those are
vacuous, and `results.py` records `"applet": false` for them so `compare.py`
does not count them as regressions when the applet arrives. Dominant failure
causes seen so far: unsupported GNU options (`unknown argument at argv[0]`),
non-UTF-8 argv rejected by `xsh`, wording and exit-status mismatches, buffered
stdout (no EPIPE or write-error behavior), and large-input throughput.

### B. GNU suite differential (Gate 4)

[`run-gnu.sh`](run-gnu.sh) prepares one GNU 9.12 tree with uutils'
`util/build-gnu.sh`, then runs the identical tests twice, changing only the
`PATH` entry that `tests/local.mk` prepends: the pinned uutils multicall
directory, then the XSH stage's `gnu-bin/`. `gnu-bin/` has every GNU program
name; names XSH lacks are copies of `false`, so nothing falls through to host
GNU binaries (uutils uses the same trick). Because both sides share one
patched harness, uutils' patches cannot bias the differential.

Patch policy (revised): the brief asks to separate plumbing, accepted
semantic divergences and uutils wording patches. That classification
matters for absolute GNU conformance, not for the uutils-parity milestone,
so it is a tracked task in [`gnu-patches.json`](gnu-patches.json) (15 patches
plus `build-gnu.sh`'s sed rewrites, all `unclassified` today) rather than a
precondition for running the differential.

Results: `results/gnu-uutils.json` (cached per uutils commit),
`results/gnu-xsh.json`, and `results/gnu-differential.json` with the four
cells globally and per test directory. `uutils pass / xsh fail` is the
blocker list. GNU groups some tests under `misc/`; the per-utility rollup
maps those by test name when the manifest ingests them.

GNU baseline against the pinned uutils (2026-10-04, GNU 9.12 via uutils'
`build-gnu.sh`, run unprivileged in a user namespace, 21 minutes): 719 tests,
**571 pass, 46 fail, 101 skip, 1 error**
([`results/gnu-uutils.json`](results/gnu-uutils.json)). The XSH side
(`run-gnu.sh xsh`) is not run yet: with most applets absent nearly every test
would fail, so the four-cell differential starts in Wave 2.

### C. BusyBox suite

After A and B are green enough to be useful, adapt uutils' BusyBox route
(`GNUmakefile` `busytest`). BusyBox behavior never overrides the uutils/GNU
contract where the projects intentionally differ.

### D. Native tests remain

`core/tests/test-<applet>.xsh` stay the fast regression suite and cover
XSH-specific boundaries. Upstream suites supplement them.

### E. Ratchets

- [`check_ignored_options.py`](check_ignored_options.py) (Gate 6): fails on any
  new discard bucket (`ignored`, `unused`, `compat`, `noop`), on a bucket that
  gains options, and on a stale baseline. Baseline: 16 buckets in chgrp,
  chmod, chown, cp, df, du, getty, hostname, ifdown, ifup, ln, ls, mv, pstree,
  touch, which ([`ignored-options-baseline.json`](ignored-options-baseline.json)).
  It can only shrink.
- Test ratchet: the integrator rejects a merge that turns any previously
  passing uutils test into a failure (compared against the committed
  `results/uutils-integration.json`).

## Phase 2: complete uutils Linux surface

The authoritative list is `totals.missing_in_scope` in the manifest, never a
handwritten list. Notes:

- Names that are xshi builtins (`echo`, `kill`, `true`, `false`, `test`) still
  need external applets for `env CMD`, `execve`, `PATH` lookups and programs
  that bypass the shell. Provide `[` for `test`.
- Share implementations through aliases in [`aliases.json`](aliases.json):
  `arch` over `uname` machinery, `dir`/`vdir` over `ls` with different
  defaults, the checksum family (`md5sum`, `sha*sum`, `b2sum`; `cksum` and
  `sum` share the native hash layer), `true`/`false`. Preserve each command's
  argv, help and output.
- `stdbuf` needs an LD_PRELOAD-style buffering shim in uutils; on musl that is
  unavailable (uutils drops it from `feat_require_unix_musl`). Decide with
  evidence: implement over a native mechanism or record a capability
  exclusion; never ship a no-op.
- `pinky`, `users`, `who`, `uptime` (user count) read utmp; Laputa's utmp
  story decides between real support and explicit failure.

Quality bar per command: options and their interactions, stdout, stderr, exit
status, binary safety, symlink follow/no-follow, permissions, ownership,
timestamps, hardlinks, sparse files, recursion, filesystem boundaries, broken
symlinks, material races, TTY behavior, signals, numeric parsing and
overflow, empty input, `-` operands, multiple operands and error continuation.
When POSIX/GNU is unclear, the pinned uutils implementation and tests are the
specification. uutils is a reference and oracle, never a cargo dependency;
reusing a small MIT-licensed algorithm is acceptable with attribution.

The no-ignored-options rule applies to every existing applet before it counts
as passing: implement or reject each discarded option.

## Native API growth

Use existing XSH facilities first. Add a narrow primitive when faithful host
or byte behavior is missing, keeping command policy and algorithms in XSH.
Expected boundary areas:

- **fs**: complete nofollow metadata, ownership/mode preservation, ns
  timestamps (`utimensat`), hardlinks, FIFOs and device nodes, sparse files
  (`SEEK_DATA`/`SEEK_HOLE`), efficient copy (`copy_file_range`, reflink),
  xattrs if `cp -a` requires them, statvfs and mount identity, atomic and
  no-clobber operations (`RENAME_NOREPLACE`, `O_EXCL`).
- **process**: signals, process groups and sessions, priority, terminal
  association, identity, wait with timeout, rlimits, stdio and environment
  control (for `nice`, `nohup`, `timeout`, `setsid`, `ps`, `pgrep`, `pkill`).
- **unix**: termios, PTYs and sessions (`stty`, `tty`, `getty`, `login`).
- **bytes**: streaming byte I/O for base encodings, checksums, `dd`, `od`.
- **hash**: one native layer for the checksum family; add missing algorithms
  (BLAKE2b with variable length, CRC for `cksum`, BSD/SysV `sum`).
- **compression**: existing gzip/bzip2/xz/lzma codecs back Phase 5.
- **block/mount/partition**: `BLKRRPART`, `BLKGETSIZE64`, sector sizes,
  read-only state, discard, flush, `FITRIM`, `FIFREEZE`/`FITHAW`, signature
  probing and removal, partition reread (Phase 7A).
- **fs (FAT)**: bounded descriptor I/O and byte encoding; FAT12/16/32 layout,
  parsing, checking and repair policy belong in a typed XSH module (Phase 7B).
- **xattr, acl, capability**: typed representations, including
  `security.capability` revisions (Phase 7C).
- **storage_health, nvme**: ATA pass-through, `SG_IO`, NVMe admin commands,
  identify, SMART and logs, self-test control (Phase 8).
- **namespace, process control**: `setns`, `unshare`, affinity, scheduler class,
  I/O priority, rlimits, uid/gid and supplementary groups, capabilities,
  securebits, `no_new_privs`, parent-death signal, and the structured process
  and system sampling layer (Phases 3D and 9).
- **net control**: ethtool netlink, nl80211, rtnetlink mutation, real ICMP
  (Phase 3E and 10).
- **efi**: typed efivarfs variables (Phase 11).
- **compression**: zstd, and streaming reader/writer forms for every codec
  (Phase 5).
- **archive**: cpio read/write (Phase 11).


## Phase 3: practical Linux surface beyond coreutils

Required, built over typed collectors and existing `linux.*` APIs:

- **3A hardware/inventory**: `lscpu`, `lspci` (`-n -nn -k -v -vv -D -t -s -d`;
  numeric output without a database, names from `/usr/share/hwdata/pci.ids`
  when present, never embedded), `lsusb` (`-t -v -d -s -D`, `usb.ids` when
  present), `rfkill`, `sensors`. Block and mount inventory (`lsblk`, `blkid`,
  `findmnt`) is Phase 7A.
- **3B util-linux control**: `swapon`, `swapoff`, `mkswap`, `dmesg`, `sysctl`,
  `hwclock`, `flock`, `setsid`, `pivot_root`, `switch_root`, `reboot`,
  `poweroff`, `halt` over `linux.swapon`, `linux.dmesg`, `linux.sysctl_*`,
  `linux.hwclock`, `linux.pivot_root`, `linux.switch_root` and friends. The
  mount family and loop devices are Phase 7A.
- **3C kmod**: `lsmod`, `modinfo`, `modprobe`, `insmod`, `rmmod`, `depmod` over
  `linux.modules`, `linux.modinfo`, `linux.modprobe`, `linux.depmod`,
  `linux.insmod`, `linux.rmmod`, with kmod-compatible argv and output.
- **3D procps and observability**: `ps` (`-e -f -ef aux -o -p --ppid --sort`,
  mainstream Linux forms tested comprehensively), `pgrep`/`pkill`/`pidof`/
  `killall` over one typed selector, `pstree`, `free` (`-h -m -g -s`), `lsof`
  over `linux.open_files()` grown to files, pipes, sockets, devices,
  cwd/root/exe, `fuser`, and the sampling tools `top`, `watch`, `vmstat`, with
  `iostat` and `pidstat` if the sampling primitives are shared cleanly. All of
  them sit on one native structured sampling layer (see "One model per kernel
  ABI"), never on per-command `/proc` readers.
- **3E networking**: `ip` grows to `link show/set up/down`, `address
  show/add/del`, `route show/add/del`, `rule show/add/del`, `-4 -6 -j -o`,
  via native rtnetlink mutation APIs next to `linux.network_dump()`; `ss`
  (TCP/UDP/listening/process); `ping`/`ping6` with real ICMP. Legacy
  `ifconfig`/`route`/`arp` only if cheap. Not `tc` or `bridge`. Device control
  (`ethtool`, `iw`) and diagnostics are Phase 10.
- **3F udevadm**: honest `info`, `trigger`, `settle`, `monitor` over sysfs and
  XSH's uevent machinery; no operation reports success without effect.
- **3G accounts**: `login` sharing `lib/auth.xsh` with `passwd`, `su`, `getty`
  (which already execs `login`); `getent` for passwd, group, hosts, and
  services/protocols where present. No NSS empire.

## Phase 4: script-ecosystem commands

`grep`/`egrep`/`fgrep` (shared lower-level search primitives, not `rg`; `-E
-F -G -i -v -w -x -n -H -h -l -L -c -q -r -R -e -f --color`, `-P` only if
truly supported), `find` (`-name -iname -type -path -regex -size -mtime
-newer -user -group -perm -maxdepth -mindepth -prune -print -print0 -exec
-execdir -delete` with boolean composition), `xargs` (byte/NUL-safe `-0 -n -L
-P -I -r`, correct exit codes), `sed` (a real parser and addressing model;
uutils/sed and its GNU shim as reference and tests), `awk` (credible POSIX
awk with an XSH parser and runtime), `cmp`, `diff`, `patch` with shared XSH
logic and necessary byte or filesystem primitives.

Revised: `sed` and `awk` share nothing with coreutils and are each multi-week
efforts, so they start in Wave 1 as long-running lanes.

## Phase 5: compression

### Zstd implementation decision (2026-10-06)

The integrated implementation uses `zstd` 0.13.3 with default features disabled,
statically linking its contained upstream codec. The dependency and lockfile
were updated after the user approved the measured candidate.
The alternative `ruzstd` 0.9 encoder leaves its Default, Better and Best levels
unimplemented, so it cannot supply the required level contract.

The candidate's bounded streaming wrapper round-tripped binary input and
concatenated frames and rejected truncated and checksum-corrupted input. On a
256 MiB mixed corpus, encode and decode each took 0.22 s, with peak RSS of
3,888 and 2,916 KiB respectively. These are single local measurements with
10 ms timing resolution, not a performance guarantee. Reproduction details
and smaller input measurements are in
[`bench/zstd-rust-candidate-2026-10-06.json`](../../bench/zstd-rust-candidate-2026-10-06.json).
The user approved this dependency and granted standing dependency approval
for the campaign on 2026-10-06. Compression completion still requires the
canonical differential tests.

`gzip`/`gunzip`/`zcat`, `bzip2`/`bunzip2`/`bzcat`, `xz`/`unxz`/`xzcat`,
`lzma`/`unlzma`/`lzcat` and `zstd`/`unzstd`/`zstdcat` as one family: thin XSH
presentation layers over a common native streaming implementation. Do not
package the traditional implementations to get familiar binaries.

- XSH already has native gzip, bzip2, xz and lzma codecs. zstd joins the native
  compression layer with full frame encode and decode. Choose an implementation
  that provides the required format support and correctness. Performance
  measurements may be recorded as context, but do not gate the choice or
  campaign completion.
- Required semantics: stdin/stdout streaming, multiple files, file replacement,
  `-c -d -k -f`, compression levels, integrity testing (`-t`), concatenated
  streams where the format permits them, original name and timestamp where the
  format records them, each tool's documented exit codes, and binary-safe
  operation.
- `compression.*` is refactored as needed so the CLIs share one streaming
  reader/writer implementation; no whole file passes through memory.

## Phase 6: small high-value commands

`file` (deterministic subset: ELF, shebangs, archives and compression, text
encodings, common image/container formats; no libmagic database), `ldd`
(inspect the ELF interpreter and dependencies on musl without executing
untrusted binaries), `clear`, `reset`.

## Phase 7: storage and filesystems

### 7A block, mount and partition

Turn `linux.block_devices`, `linux.blkid`, `linux.partition_table`,
`linux.write_partition_table`, `linux.loop_attach/detach/list` and
`linux.fsck` into a coherent practical storage environment: `lsblk` (`-a -b -d
-f -J -l -n -o -p -r`, relationships from typed identity, not names), `blkid`,
`findmnt` (mountinfo first: `TARGET`, `--json`, `--types`, `--source`,
`--target`; `--fstab` after), `mount`, `umount`, `losetup`, `blockdev`,
`wipefs`, `partx`, `partprobe`, `fstrim`, `fsfreeze`, and `sfdisk`/`fdisk`
with their existing syntax, not a bespoke one (`sfdisk --json` is the machine
interface). GPT and MBR both, correctly.

New native primitives: `BLKRRPART`, `BLKGETSIZE64`, logical and physical sector
size, read-only state, discard, block-device flush, `FITRIM`,
`FIFREEZE`/`FITHAW`, partition reread, signature probing and removal.
`wipefs` understands and selectively erases known signatures; it is never
"zero the start of the disk".

### 7B FAT (replaces Laputa's handwritten FAT tooling)

A typed FAT module (parse and manipulate), then `mkfs.fat`/`mkfs.vfat`,
`fsck.fat`/`fsck.vfat` and `fatlabel`. Real FAT12/FAT16/FAT32 handling, not
just the geometry the Laputa installer happens to generate.

- `mkfs`: FAT type selection, automatic layout, volume label and ID, sector
  size, cluster sizing, reserved sectors, number of FATs, regular files and
  block devices, and reproducible images when explicitly requested.
- `fsck`: an actual checker, not a header sanity check. Validate and, where
  appropriate, repair the boot sector and BPB, backup boot sector, FAT copies,
  cluster chains, loops, cross-links, lost clusters, directory entries, `.` and
  `..`, invalid cluster references, long-filename chains, free-space
  accounting, FAT32 FSInfo and dirty/error flags. Conventional noninteractive
  modes serve installation and recovery workflows; destructive repair is
  explicitly controlled.
- `laputa-fs` is migrated onto this module. Only a genuinely Laputa-specific
  helper (such as `fat-put`) may remain there, or it migrates too.

### 7C attributes, ACLs, capabilities

`lsattr`/`chattr` over the existing native file-attribute support;
`getfattr`/`setfattr`, `getfacl`/`setfacl` and `getcap`/`setcap` over typed
xattr, ACL and capability domains. File capabilities understand and validate
the `security.capability` xattr versions (revision, flags, permitted and
inheritable sets, root id) rather than treating the payload as opaque bytes.

## Phase 8: storage health

`smartctl` as a serious surface over typed APIs, not a text parser: ATA/SATA
and NVMe first, SCSI/SAS once the transport abstraction is clean. New native
Linux functionality: ATA pass-through, `SG_IO`, NVMe admin commands, identify
data, SMART data and logs, health information, self-test control.

Surface: `smartctl DEVICE`, `-i`, `-H`, `-A`, `-a`, `-x`, `-l error`,
`-l selftest`, `-t short|long`, `-s on|off`, `-j`. No `smartd`: Laputa needs no
further background daemon. The read-only inventory is reusable by
`system-report`, which reports normalized health and identify data while
`smartctl` exposes the lower-level raw fields.

`nvme` inspection over the same typed NVMe transport: `list`, `id-ctrl`,
`id-ns`, `smart-log`, `error-log`, `self-test-log`, `fw-log`. Read-only first.
Mutating admin operations (format, firmware activation) arrive only with
explicit validation and strong tests, never merely to claim coverage.

## Phase 9: namespaces and process control

`nsenter`, `unshare`, `lsns`, `taskset`, `chrt`, `ionice`, `prlimit`,
`setpriv` as thin applets over reusable typed XSH APIs: `setns`, `unshare`,
namespace fd discovery, CPU affinity, scheduler class and priority, I/O
priority, rlimits, uid/gid transitions, supplementary groups, Linux
capabilities, securebits, `no_new_privs` and parent-death signals. The logic
lives in the `process`, `namespace` and `capability` domains, not in the
applets.

## Phase 10: network device control and diagnostics

- `ethtool`: modern netlink API first, ioctl only where required. `DEV`, `-i`,
  `-k`, `-K DEV FEATURE on|off`, `-S`, `-g`, `-G`, `-c`, `-C`, `-a`,
  `--show-eee`; inspection and common controls first.
- `iw`: a typed nl80211 API and `iw dev`, `dev DEV info|link|scan`, `phy`,
  `phy PHY info`, `reg get|set CC`. No association or authentication
  (`wpa_supplicant` stays separate); `wireless-regdb` stays external.
- Diagnostics over the existing `dns` and `net` modules: `traceroute`,
  `tracepath`, `dig`, `nslookup`, `nc` (a clean TCP/UDP client and listener,
  not every historical netcat incompatibility; no `socat` clone). `host`
  already exists.
- HTTP transfer: a focused `curl` (`-L -f -s -S -o -O -I -X -H -d
  --data-binary -u --connect-timeout --max-time --retry --cacert -k`, correct
  stdin, stdout and file streaming), and a small `wget` over the same
  implementation if worthwhile. Not every curl feature.

## Phase 11: boot

- `cpio` with `newc` first, then `crc` and `odc`; `-o -i -t -p -d -m -u -v
  --null`; binary-safe streaming. Reading and writing live in the archive
  module, not only in the CLI.
- `efibootmgr` over a typed EFI-variable domain on efivarfs: `efibootmgr`, `-v`,
  `-c ...`, `-b XXXX -B`, `-o`, `-n`. Writes are extremely careful: synthetic
  efivarfs fixtures first, then QEMU/OVMF, before any real-host mutation. The
  Laputa installer is an eventual consumer. No D-Bus firmware stack.

## Phase 12: clock

Laputa's clock model stays deliberately simple: RTC, then `linux.hwclock()`,
then `linux.set_system_clock()`, or the explicit boot epoch when present. No
NTP, chrony or ntpd in this campaign. Maintain and complete `date` and
`hwclock` and their typed APIs.

## Non-goals

External by design (a deep independent implementation): the kernel and
firmware, `e2fsck` and other filesystem repair engines, GPU drivers and Mesa,
nftables semantics, `strace`, `perf`, `tcpdump`, Git, Make, rsync, browsers,
databases, PipeWire, ALSA playback, `wpa_supplicant`, Tailscale, SSH, full
`curl` feature parity.

Not introduced merely because conventional distros ship them: systemd, D-Bus,
elogind, polkit, NetworkManager, a udev daemon, FUSE, chrony, ntpd, smartd,
udisks, upower, fwupd. A daemon is not added when a direct kernel interface and
an on-demand XSH command suffice.

Rule: command veneers and kernel/userspace inspection and control belong in
XSH when XSH owns the structured capability; independent application or
protocol semantics stay packages.

## Semantics and style rules

- Never fake success. No accepted semantic flag is discarded; unsupported
  behavior fails clearly.
- Output is conventional; typed records belong in the native API. Support
  established JSON options (`ip -j`, `lsblk -J`, `findmnt --json`).
- `LC_ALL=C` and `TZ=UTC` are the deterministic baseline. UTF-8 is still
  handled correctly; full i18n does not block the milestone.
- Linux first. Coreutils applets stay cross-platform where the API already
  is; Linux-only commands report unsupported cleanly elsewhere.

## Non-gating performance notes

The 2026-10-04 cold-start measurements and later zstd measurements are retained
as historical context. They do not set acceptance thresholds or block campaign
progress. Do not run performance benchmarks as campaign gates. Use the
correctness and parity suites to decide whether behavior is acceptable.

## Packaging

`dev/release.xsh::package_core` already ships every `core/*.xsh`. Add a
deterministic applet manifest (applets, aliases, libraries; the shape
`stage.py` writes as `applets.json`) to the release artifact so Laputa can
materialize `/usr/bin` links from it instead of its hand-maintained list.
Aliases (`[`, `dir`, `vdir`, `egrep`, `fgrep`, `gunzip`, `zcat`, `mkfs.vfat`,
`fsck.vfat`, ...) come from [`aliases.json`](aliases.json) with no duplicated
sources. Laputa itself
changes only if needed to prove the interface.

## Verification gates

These gates assess correctness, behavioral parity, and required surface
coverage. There is no performance gate.

1. **Native**: project Rust tests, `xsht check`, `core/tests` pass.
2. **Inventory**: every in-scope uutils utility implemented or carrying an
   accepted capability/platform rationale in the manifest.
3. **uutils integration**: all applicable `tests/by-util` tests pass through
   `UUTESTS_BINARY_PATH`; exclusions are per test ID, categorized, explained.
4. **GNU differential**: zero `uutils pass / xsh fail`, reported globally and
   per utility with all four cells.
5. **Options**: automated comparison of each uutils command's option surface
   with XSH's (help-text parsing is one input, not proof of semantics); gaps
   appear in the manifest.
6. **No ignored semantic options**: ratchet at zero buckets.
7. **Practical Linux suite**: deterministic tests for every command in
   [`surface.json`](surface.json). Families with a canonical implementation get
   a differential harness against it (dosfstools, smartmontools, nvme-cli,
   util-linux, ethtool, iw, xz, zstd, procps, kmod, ...), used only as a
   test/reference dependency; where output carries volatile identifiers or
   counters, compare parsed semantics, not unstable text. Destructive and
   system operations run only in sandboxes: loop files, mount, user and
   network namespaces, veth pairs, synthetic sysfs/procfs/efivarfs fixtures,
   QEMU virtual disks and NVMe devices, QEMU/OVMF for EFI, `scsi_debug` and
   nvme loop facilities when safe. Never mutate the developer's real block
   devices, firmware variables, network interfaces or SMART state.
8. **Clean smoke**: a Linux image with XSH first on `PATH` and no GNU
   coreutils, util-linux, procps, findutils, grep, sed, gawk, diffutils or
   kmod, running real workflows: file manipulation, archives, checksum
   verification, find/xargs, grep/sed/awk pipelines, process, mount/block,
   network, module and hardware inspection, partitioning, FAT creation and
   checking, compression round trips, SMART/NVMe inspection against QEMU
   devices, namespace and process control, boot-adjacent commands.
9. **One model per kernel ABI**: [`check_kernel_reads.py`](check_kernel_reads.py)
   ratchet; a new `/proc` or `/sys` reader in an applet fails the build.
10. **Surface inventory**: `surface.json` entries are only ever added;
   `parity.py --check` fails when the count drops below the pin in
   `upstream.lock.json`, and every command is implemented or carries an
   accepted capability/platform rationale.

## Deliverables

Implementation; pinned reference manifest; uutils adapter; GNU differential
runner; per-command parity report; native tests for new primitives and
applets; documentation (what "XSH core compatibility" means, baseline
revision, rerunning the suites, unsupported capability areas, downstream
discovery and installation); and a final report: applets before/after,
uutils coverage, uutils integration pass rate, GNU differential, intentional
gaps, extra Linux utilities, new native APIs.

## Sequence (revised)

Wave 0 is serial and must land before fan-out; later waves are parallel lanes
defined in [`LANES.md`](LANES.md). Native domain lanes land before the applet
lanes that consume them.

| Wave | Work |
|---|---|
| 0 | parity manifest and adapter (done); toolchain and suite build; baseline uutils run of current XSH; `cli` GNU mode; `lib/gnu` diagnostics; invoked-name access; byte helpers; ratchets wired into `make check`; applet manifest |
| 1 | existing-applet repair (ignored buckets, GNU diagnostics); missing cheap coreutils; native fs, process/tty and bytes/hash primitives; text and checksum families; `system-report` collector extraction; `sed` and `awk` start |
| 2 | difficult coreutils finish; GNU differential to zero blockers; native domains for block/partition, process sampling, compression streaming (+zstd), xattr/ACL/capability; compression CLIs; hardware/kmod/util-linux wrappers; procps and sampling tools; `ip`/`ss`/`ping`; grep/find/xargs; diff/cmp/patch; login/getent/udevadm |
| 3 | block, mount and partition tools; FAT module and dosfstools surface; attributes/ACL/capabilities; namespace and process control; storage health (`smartctl`, `nvme`); `ethtool`, `iw`, diagnostics and HTTP; `cpio`; `efibootmgr`; Phase 6 commands |
| 4 | BusyBox route; Gate 5 option comparison; Gate 8 clean smoke image; final correctness and parity report |

## Environment notes

- The repository pins `nightly-2026-09-15`; XSH uses no `#![feature]`, so
  stable 1.97 builds it when the pinned toolchain is unreachable
  (`RUSTUP_TOOLCHAIN=stable`). Do not commit a toolchain change for this.
- The test environment needs network access to crates.io (`index.crates.io`,
  `static.crates.io`), `static.rust-lang.org`, GitHub release downloads, and,
  for the `xsh-test` image, Docker Hub and `dl-cdn.alpinelinux.org`. The
  network policy must allow those hosts when preparing fetched inputs.
- The reference host has 4 cores, about 15 GB RAM, a 14.3 GB memory cgroup
  shared by every process the session starts (builds, lanes and suites), and
  about 27 GB of disk. First full runs showed what that costs: a runaway
  applet and a test process buffering gigabytes each got `cargo-nextest`
  OOM-killed. The uutils runner therefore caps each test process and each
  applet (address space, 4 GiB and 3 GiB), limits nextest threads, and refuses
  to publish results from a run nextest did not finish.
- The uutils test crate embeds the build-time `PATH` (`env!("PATH")`) and its
  `build.rs` lists the gitignored `docs/tldr.zip` as a rerun trigger; either
  one changing makes cargo recompile the whole test crate (five minutes).
  `run-uutils.sh` pins `PATH` and creates an empty placeholder.
- The uutils framework runs each command with a cleared environment, so the
  adapter finds its stage from its own path rather than from a variable.
- Native-lane features need hardware or kernel facilities the host may lack
  (QEMU/OVMF, NVMe devices, `scsi_debug`, privileged namespaces): those tests
  are written against synthetic fixtures here and run for real in the
  `xsh-test` image or a privileged CI lane.
- AGENTS.md makes `Dockerfile.test` (`xsh-test`, musl) the authority for Linux
  evidence. Host-glibc runs are fast iteration only; gate results that count
  come from the image.
