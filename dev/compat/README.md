# Compatibility Harness

Tooling for the campaign in [`CAMPAIGN.md`](CAMPAIGN.md); lane process in
[`LANES.md`](LANES.md).

## Campaign subagents

Use only **`gpt-6-luna` at `xhigh`** for every campaign subagent, including
routine work. Set both explicitly when spawning.
`python3 dev/compat/lanes.py brief LANE` renders this requirement and paths
for the current checkout; see `LANES.md` for ownership and integration.

Command behavior and shared semantic domains are implemented in XSH. Native
requests are limited to reusable host, codec and byte boundaries that XSH
cannot express faithfully. Parsers and interpreters belong in XSH. The user
has granted standing dependency approval for this campaign; record each
addition's reason without requesting approval again.

## Setup

```sh
git clone https://github.com/uutils/coreutils ../ref/uutils-coreutils
git -C ../ref/uutils-coreutils checkout "$(python3 -c 'import json;print(json.load(open("dev/compat/upstream.lock.json"))["uutils"]["commit"])')"
export UUTILS_ROOT=$PWD/../ref/uutils-coreutils
export CARGO_PROFILE_DEV_OPT_LEVEL=1
export CARGO_PROFILE_DEV_CODEGEN_UNITS=256
export CARGO_PROFILE_DEV_LTO=false
export CARGO_PROFILE_TEST_OPT_LEVEL=1
export CARGO_PROFILE_TEST_CODEGEN_UNITS=256
export CARGO_PROFILE_TEST_LTO=false
cargo build -p xsh --bins -p xsht --bin xsht
cargo install --debug cargo-nextest --locked   # if run-uutils.sh needs it
```

Campaign verification is correctness and parity only; no benchmark or
performance threshold gates a change. Use Cargo's debug profiles with the
modest optimization, 256 codegen units, and LTO disabled, and run
`target/debug/xsh` and `target/debug/xsht`. Do not use `--release` or
`--profile dist` for campaign checks. Linux checks still run inside the
`Dockerfile.test` image.

GNU runs also need a C toolchain, autotools, perl and the packages uutils'
`build-gnu.sh` uses: `quilt gperf texinfo autopoint gawk help2man rsync`. In a
sandbox where `apt` cannot open `/dev/null` as its `_apt` user, add
`-o APT::Sandbox::User=root` to `apt-get update` and `install` (signature
verification stays on). GNU `configure` refuses to run as root, so `run-gnu.sh`
bypasses that check for configure only. Root suite runs use `setpriv` with an
existing account: `GNU_RUN_UID` and `GNU_RUN_GID` fall back to the corresponding
`UUTESTS_RUN_UID/GID` settings, then UID 1000 and the same GID. The prepared GNU
tree, including `Makefile.in`, must be writable by that account; the runner
reports permission failures and does not change ownership. `GNU_JOBS`
(default 3) sets `make -j`.

Root invocations run reference tests through `setpriv` as an existing
unprivileged account (`UUTESTS_RUN_UID=1000`, GID defaults to that UID).
Provision the account inside the test container first. This preserves
permission fixtures and protected device nodes; build and report publication
still belong to the invoking user. Staged executable shebangs pass `--` before
the script path so an applet's own leading option separator is preserved.
On a musl host, `run-uutils.sh` uses the C linker driver and disables static
crt linking for the reference test harness's `stdbuf` cdylib; this does not
change the XSH debug-profile flags.

`COMPAT_RESULTS_DIR` selects scratch report storage for both suite runners.
`run-uutils.sh` writes nextest's JUnit report there by absolute path, including
when a lane selects a private `UUTILS_TARGET_DIR`. The runners share
`UUTILS_SUITE_LOCK` and serialize reference runs. GNU reports are published
only when fresh logs, per-test results and complete summary counts agree; the
differential requires identical test selections on both sides.

## Commands

| Command | Does |
|---|---|
| `python3 dev/compat/parity.py` | regenerate `dev/coreutils-parity.json` from the pinned uutils tree and any results |
| `python3 dev/compat/parity.py --check` | fail if the manifest is stale |
| `python3 dev/compat/stage.py [--stage DIR]` | install `core/` in the standard staged layout with shebangs at the built `xsh`; writes `applets.json` |
| `dev/compat/xsh-uutests UTIL ARGS...` | uutils multicall contract; `stage.py` installs a copy in the stage that finds the stage from its own path (the uutils framework clears the environment) |
| `dev/compat/run-uutils.sh [UTIL...]` | Gate 3: uutils `tests/by-util` against XSH; writes `results/uutils-integration.json` |
| `python3 dev/compat/compare.py BEFORE.json AFTER.json` | merge gate: before/after totals, per-utility change, exit 1 on any test that passed before and fails now |
| `dev/compat/run-gnu.sh prepare` | fetch GNU 9.12 and prepare its tests with uutils' `build-gnu.sh` |
| `dev/compat/run-gnu.sh uutils [TEST...]` | GNU tests against pinned uutils (cached baseline) |
| `dev/compat/run-gnu.sh xsh [TEST...]` | the same tests against the XSH stage |
| `dev/compat/run-gnu.sh diff` | Gate 4 four-cell differential into `results/gnu-differential.json` |
| `python3 dev/compat/check_option_surface.py --util cat` | Gate 5 cat pilot; reports spelling and arity differences against pinned uutils |
| `python3 dev/compat/check_ignored_options.py` | Gate 6 ratchet over discard buckets in `core/*.xsh` |
| `python3 dev/compat/check_exclusions.py` | validates `exclusions.json`: exact IDs, closed category list, a reason each (and existence in the pinned tree with `UUTILS_ROOT`) |
| `python3 dev/compat/check_kernel_reads.py` | Gate 9 ratchet: no `/proc`/`/sys` literal in a top-level applet |

## Data files

| File | Owner | Content |
|---|---|---|
| `upstream.lock.json` | integrator | pinned uutils commit and GNU version |
| `../coreutils-parity.json` | generated | per-utility parity manifest |
| `aliases.json` | integrator | alias executable → shared applet, as `{name, target}` entries |
| `surface.json` | integrator | commands in the expanded scope beyond uutils, with phase, domain and test-only reference tool; the count is pinned in `upstream.lock.json` and can only grow |
| `kernel-reads-baseline.json` | integrator | applets still reading `/proc`/`/sys` directly (shrink-only; empty today) |
| `exclusions.json` | integrator | per-test-ID uutils exclusions with category and reason |
| `gaps.json` | lanes (own utilities) | known semantic gaps |
| `host-deps.json` | generated once | uutils tests that spawn host programs or use host oracles |
| `gnu-patches.json` | `gnu-patch-classify` lane | classification of uutils' GNU test patches |
| `ignored-options-baseline.json` | integrator | remaining legacy discard buckets (shrink-only) |
| `results/` | generated | suite outputs |

## Status

The campaign resumed on 2026-10-07. See the current checkpoint in
[`CAMPAIGN.md`](CAMPAIGN.md) and the completed 16-lane ownership record in
[`CLAIMS.md`](CLAIMS.md).

The latest focused integration includes the longer 16-lane follow-up, further
module-owned byte-path and text fixes, and external `sort -S` spill runs. The
sort native suite passes 45/45 and its current pinned slice passes 154/217;
selected GNU 9.12 sort tests have no XSH-only disagreement. Other focused
uutils slices fixed additional cases in `cp`, `date`, filesystem applets,
`stat`, text utilities, and `env`. The full optimized-debug native suite now
passes 4,964/0/38 (pass/fail/skip) at source `89c4e143`. The runtime now
services pending termination signals while an XSH child waits on stdin, and
unhooked SIGINT/SIGTERM completes with the default shell status. Current Gate 3 is
5,469 pass, 480 fail, 0 skip, and 4 excluded. The before/after comparison has
six `pr` expectation differences: direct GNU 9.11 probes match XSH diagnostics,
while uutils expects different clap messages. Current Gate 4 is 412 PASS,
156 FAIL, 17 ERROR, and 134 SKIP; its 633-cell differential records
394 shared passes, 177 uutils-only passes, 18 XSH-only passes, and 44 shared
failures. Relative to the prior full Gate 4 run, the new report has two more
passes, one fewer failure, and one fewer skip. `split/filter.sh` remains an
error; `tee/tee.sh` passes; `misc/yes.sh` still fails. See `CAMPAIGN.md` and
`results/` for the reports and remaining limits.

The Gate 5 cat pilot compares the pinned Clap declarations with XSH's
`cli.applet` schema. It found 21 uutils spellings and 19 XSH spellings, with
only `-h` and `-V` missing from XSH; there are no XSH-only spellings or arity
mismatches. These are Clap-generated short help/version forms that GNU cat
does not use. The strict checker exits 1 to report them, and `gaps.json`
records the known difference. Four parser tests pass; direct pinned-uutils and
XSH probes confirm both tools accept `--help` and `--version`, while only
uutils accepts `-h` and `-V`. This pilot does not close Gate 5 for the other
utilities. It also reports `-u` as parsed-but-unused on both sides, matching
cat's documented ignored option.

The historical full Gate 3 and Gate 4 results at `92a91b12` were 5,125/824/0
with four exclusions for Gate 3, and 367 PASS, 189 FAIL, 25 ERROR, and 138
SKIP for Gate 4. Gate 3 was run against pinned uutils
`e7c9f3194280835c4487c2945c68d5f01ccacc8d` in the `Dockerfile.test` image.
across 106 utilities, one more pass than the preceding full run. The pinned
GNU 9.12 XSH run was across 719 tests; its four-cell differential had 358
shared passes, 213 uutils-only passes, 9 XSH-only passes, and 52 shared
failures. `test_wc::test_files0_progressive_stream` timed out at 120 seconds in
that Gate 3 run. Follow-up commit `d2938a41` streams `wc --files0-from=-` counts and
diagnostics as names arrive, stops on the first stdout write error, and reports
that its line counter uses scalar code for `--debug`. Its optimized debug
Gate 3 slice passes 59/59, including all three `wc` cases that failed or timed
out in the historical full run. The report is in `results/wc-followup/`.
The matching pinned GNU 9.12 `wc` subset has 6 shared passes and one shared
Shift-JIS skip, with no differential mismatches; its reports are in the same
directory.
Follow-up commit `8f989405` preflights every named `sort` input before opening
any operand, so a missing later file is reported without blocking on an earlier
FIFO. The optimized debug Gate 3 `sort` slice improves from 34/183 to 37/180
across 217 cases, with three fixes and no regressions. Its native sort suite
passes 2/2, and the per-utility report and JUnit are in
`results/sort-followup/`.
Follow-up `273f1312` adds `sort -c`/`-C` conflict diagnostics and NUL-delimited
`-z` checking and output. Native sort tests pass 6/6; its optimized debug Gate
3 slice improves from 37/180 to 41/176 across 217 cases, with four fixes and
no regressions. GNU 9.11 emits the NUL record terminator in a `-z -c`
diagnostic; the corresponding uutils test expects a newline and remains a
known expectation difference. Reports are in
`results/sort-check-zero-followup/`.
Follow-up `3437b52e` keeps an unterminated final record in one sort input
operand separate from the next file. The native sort tests pass 7/7; the
optimized debug Gate 3 slice improves from 41/176 to 43/174 across 217 cases,
fixing `sort_multiple` and `test_start_buffer` with no regressions. Reports are
in `results/sort-input-boundary/`.
Follow-up `dfed771c` applies `sort -b` to the comparison key, retains GNU's raw
line tie-break, and uses the same order for `-c`. Focused native sort tests pass
8/8, and a pinned GNU C-locale probe matches. Its Gate 3 slice stays 43/174
with no regressions; the upstream `test_blanks` also invokes unsupported
`--debug`. The report is in `results/sort-blanks-followup/`.
Follow-up `5f7eedf1` adds GNU `sort --version` output. Native sort tests pass
9/9; the optimized debug Gate 3 slice improves from 43/174 to 44/173 across
217 tests, fixing `test_no_error_for_version` with no regressions. Reports are
in `results/sort-version-followup/`.
Follow-up `f1aef7a1` accepts `--output`, allows repeated identical output paths,
rejects different destinations, and accepts output paths beginning with
`--`. Native sort tests pass 10/10; the optimized debug Gate 3 slice improves
from 44/173 to 46/171 across 217 tests, fixing
`test_error_on_multiple_output_flags` and `test_output_file_with_leading_dash`
with no regressions. GNU probes match; reports are in
`results/sort-output-followup/`.
Follow-up `5cd19a0e` reports failed `-o` destination opens with GNU's status
and diagnostic. Native sort tests pass 11/11; the optimized debug Gate 3 slice
improves from 46/171 to 47/170 across 217 tests, fixing
`test_verifies_out_file` with no regressions. Reports are in
`results/sort-output-open-followup/`.
Follow-up `373a6c63` uses GNU's uppercase case-fold key and raw-line tie
breaker for `sort -f`. Native sort tests pass 12/12; the optimized debug Gate 3
slice improves from 47/170 to 49/168 across 217 tests, fixing both punctuation
ordering tests with no regressions. A GNU 9.11 C-locale probe matches; reports
are in `results/sort-fold-case/`.
Follow-up `252844d4` implements dictionary and nonprinting character keys,
including their conflicts with numeric sorting. Native sort tests pass 14/14;
the optimized debug Gate 3 slice improves from 49/168 to 51/166 across 217
tests, fixing the Unicode character cases with no regressions. GNU 9.11 probes
match; reports are in `results/sort-char-modes/`.
Follow-up `6a652532` treats a leading `+` as nonnumeric for `sort -n` and uses
the full line to order equal numeric keys. Native sort tests pass 15/15; the
optimized debug Gate 3 slice improves from 51/166 to 52/165 across 217 tests,
fixing the leading-plus case with no regressions. GNU 9.11 output matches;
reports are in `results/sort-numeric-plus/`.
Follow-up `0ec61198` adds stable primary-key ordering with `-s/--stable`.
Native sort tests pass 16/16; the optimized debug Gate 3 slice improves from
52/165 to 54/163 across 217 tests, fixing `test_keys_stable` and
`test_sort_locale_punctuation` with no regressions. GNU 9.11 probes match;
reports are in `results/sort-stable/`.
Follow-up `fc7d8b16` adds natural version ordering through `-V`,
`--version-sort`, and `--sort=version`, with stable ties and leading-dot
ordering. Native sort tests pass 17/17; the optimized debug Gate 3 slice
improves from 54/163 to 56/161 across 217 tests, fixing the version-sort stable
and unstable cases with no regressions. GNU 9.11 probes match; the two helper
cases that add `--debug` still fail because key annotations are not implemented.
Reports are in `results/sort-version/`.
Follow-up `60485511` adds `--debug` annotations for the supported default,
case-folded, dictionary, zero-delimited, and version sort paths. Native sort
tests pass 18/18; the optimized debug Gate 3 slice improves from 56/161 to
63/154 across 217 tests, fixing `default_unsorted_ints`, `dictionary_order`,
`ignore_case`, `version`, `version_empty_lines`, `words_unique`, and
`zero_terminated` with no regressions. Detailed key-range and locale debug
annotations remain open; reports are in `results/sort-debug/`.
Follow-up `3b311480` adds `-g`/`--general-numeric-sort` and
`--sort=g`/`--sort=general-numeric`. XSH parses decimal and hexadecimal
numeric prefixes, orders binary64 values through a text key, and uses numeric
equality for `-u` and `-c`. Native
sort tests pass 23/23; its optimized debug Gate 3 slice improves from 63/154 to
69/148 across 217 cases, fixing six tests with no regressions. Reports are in
`results/sort-general-numeric-followup/`.
Follow-up `55e6549e` replaces integer-only `-n` keys with exact decimal
ordering, handles numeric prefixes, `-u`, `--sort=n` aliases, numeric key
character offsets, and reverse text keys, and omits the secondary `--debug`
annotation with `-u`. Native sort tests pass 27/27; the optimized debug Gate 3
slice improves from 69/148 to 88/129 across 217 tests, fixing 19 cases with no
regressions. Remaining general-numeric cases remain open. Reports are in
`results/sort-numeric-debug-followup/`.
Follow-up `ed9055ce` adds `-h`/`--human-numeric-sort` and the `--sort` aliases.
XSH orders recognized units before exact decimal values, preserves stable zero
ties, and matches GNU's human numeric key annotations. Focused native sort tests
pass 30/30; the optimized debug Gate 3 slice improves from 88/129 to 93/124
across 217 tests, fixing five cases with no regressions. Pinned GNU 9.12 probes
match unit order and zero handling. Reports are in
`results/sort-human-numeric-followup/`.
Follow-up `f7f34cd2` keeps the first input spelling for each equal `-n` key
when `-u` is combined with reverse sorting. The focused native sort suite passes
31/31; Gate 3 improves from 93/124 to 94/123 across 217 tests, fixing
`test_mixed_floats_ints_chars_numeric_reverse` with no regressions. A pinned
GNU 9.12 probe matches the retained representatives. Reports are in
`results/sort-numeric-unique-reverse-followup/`.
Follow-up `02d9ef8e` orders failed `-g` conversions before NaN and numeric
values, and accepts abbreviated `--sort=general-numeric` mode names. Focused
native sort tests pass 32/32; Gate 3 improves from 94/123 to 95/122 across 217
tests, fixing `test_multiple_decimals_general` with no regressions. The pinned
GNU 9.12 fixture matches. Reports are in
`results/sort-general-invalid-followup/`.
Follow-up `df8def80` adds C-locale English month sorting with `-M`,
`--month-sort`, and abbreviated `--sort=month` modes. It handles leading
blanks, unknown prefixes, stable ties, uniqueness, check mode, and debug
annotations. The native sort suite passes 33/33; the optimized debug Gate 3
slice improves from 95/122 to 100/117 across 217 tests, fixing five month
cases with no regressions. GNU Coreutils 9.11 order and check-mode probes
match; localized `LC_TIME` month names remain open. Reports are in
`results/sort-month-followup/`.
Follow-up `2309ff5e` adds `--files0-from` input lists, preserving arbitrary
POSIX filename bytes and rejecting empty entries before opening operands. The
native sort suite passes 34/34; its optimized debug Gate 3 slice improves from
100/117 to 115/102 across 217 tests, fixing 15 list-input cases with no
regressions. Reports are in `results/sort-files0-followup/`.
Follow-up commit `76f5b213` distinguishes an omitted `tail --sleep-interval`
from an explicit empty argument; the latter now fails with GNU 9.11's
diagnostic. The native tail suite passes 14/14. Its pinned Gate 3 slice stays
at 102/167 with the same failing IDs because the malformed-interval tests
require a help hint that GNU 9.11 does not print. The report and JUnit are in
`results/tail-followup/`.
Follow-up `8bc16fdd` rejects zero flags for `%c` and `%s`, rejects precision on
`%c`, warns about unused operands after literal-only formats, and suppresses
that warning when `\c` stops output. Native printf tests pass 19/19; its
optimized debug Gate 3 slice improves from 109/43 to 113/39 across 152 cases,
with no regressions. Its report and JUnit are in `results/printf-followup/`.
Follow-up `579b2195` recognizes `%q` shell quoting, leaves `~` unquoted,
rejects field parameters on `%q`, and writes literal output before an invalid
conversion error. Direct probes match pinned GNU `printf`; the native suite
passes 21/21, and the optimized debug Gate 3 slice improves from 113/39 to
117/35 with four fixes and no regressions. Reports are in
`results/printf-q-followup/`.
Follow-up `ce22226e` omits zero for integer conversions with zero precision
while retaining `0` for alternate-form octal. The native suite passes 22/22;
the optimized debug Gate 3 slice improves from 117/35 to 118/34 across 152
cases with one fix and no regressions. Reports are in
`results/printf-zero-followup/`.
Follow-up `a142979c` rejects field parameters on `%b` as GNU printf does and
emits the preceding literal text before the error. The native suite passes
23/23; the optimized debug Gate 3 slice improves from 118/34 to 119/33 across
152 cases, with one fix and no regressions. Reports are in
`results/printf-b-followup/`.
Follow-up `6c4cfa63` reports missing hexadecimal escapes and invalid or
incomplete universal character escapes while preserving preceding output.
The native suite passes 24/24; the optimized debug Gate 3 slice improves from
119/33 to 122/30 across 152 cases, with three fixes and no regressions.
Reports are in `results/printf-escape-followup/`.
Follow-up `21b619c0` formats NaN casing from its display text and uses spaces
for zero-padded NaN and infinity, matching pinned GNU printf. The native suite
passes 25/25; the optimized debug Gate 3 slice improves from 122/30 to 125/27
across 152 cases, with three fixes and no regressions. Reports are in
`results/printf-float-special-followup/`.
Follow-up `90ce7294` trims insignificant `%g` mantissa zeroes before the
exponent, matching a pinned GNU probe. The native suite passes 26/26; the
optimized debug Gate 3 slice improves from 125/27 to 126/26 across 152 cases
with one fix and no regressions. Reports are in `results/printf-g-followup/`.
Follow-up `f5d04662` uses `Float.format_number` for fixed-point precision above
the scalar formatter's limit. Its 70,123-digit case matches pinned GNU; the
native suite passes 27/27, and the optimized debug Gate 3 slice improves from
126/26 to 127/25 across 152 cases with one fix and no regressions. Reports are
in `results/printf-precision-followup/`.
Follow-up `79c1dca3` detects decimal floating overflow and underflow while
preserving pinned GNU's `inf` or zero output and `Result not representable`
diagnostic. Eight GNU 9.12 probes match and the native suite passes 28/28.
The Gate 3 slice stays 127/25 because `test_extreme_exponent_does_not_overflow`
expects `Numerical result out of range`. Its report is in
`results/printf-range-followup/`.
Follow-up `ac1a8af5` flushes `printf` output and routes write failures through
GNU-compatible diagnostics. The native printf suite passes 29/29; the
optimized debug Gate 3 slice improves from 127/25 to 129/23 across 152 cases,
fixing both `/dev/full` write-error cases with no regressions. Reports are in
`results/printf-write-followup/`.
Follow-up `964f2fdf` rejects dynamic precision above `INT_MAX` before it can
trigger a huge allocation, matching GNU's `invalid precision` diagnostic. The
native printf suite passes 30/30; the optimized debug Gate 3 slice improves
from 129/23 to 130/22 across 152 cases, with one fix and no regressions.
Reports are in `results/printf-precision-limit/`.
Follow-up `af249f2b` rejects zero-based positional references before they can
index the argument list and reports GNU's `%0$` diagnostic. The native printf
suite passes 31/31; the optimized debug Gate 3 slice improves from 130/22 to
131/21 across 152 cases, with one fix and no regressions. Reports are in
`results/printf-position-index/`.
Follow-up `4e9eb3bd` reports trailing characters after a numeric character
constant, unless `POSIXLY_CORRECT` is set. This matches pinned GNU 9.11 and
fixes the warning case plus two partial-character cases. The native printf
suite passes 32/32; the optimized debug Gate 3 slice improves from 131/21 to
134/18 across 152 cases, with no regressions. Reports are in
`results/printf-char-warning/`.
Follow-up `6c6af179` uses GNU's shell-escape quoting for `%q` arguments that
contain control bytes, including embedded apostrophes. The native printf suite
passes 33/33; the optimized debug Gate 3 slice improves from 134/18 to 135/17
across 152 cases, with one fix and no regressions. The control-and-apostrophe
case matches pinned GNU 9.11. Reports are in `results/printf-shell-quote/`.
Follow-up `d7859e78` makes `%b` consume up to three octal digits after its
`\0` prefix while retaining the format-string escape width. The native printf
suite passes 33/33; the optimized debug Gate 3 slice improves from 135/17 to
137/15 across 152 cases, fixing two escape cases with no regressions. Reports
are in `results/printf-octal-b/`.
Follow-up `70db4c11` preserves raw bytes through printf formats, arguments,
`%c`, `%b`, `%q`, and stdout, and lets `env` forward invalid UTF-8 command
arguments. The native suites pass (env 8/8, printf 35/35, GNU helpers 14 pass
with one root permission skip). The optimized debug Gate 3 printf slice
improves from 137/15 to 141/11 across 152 cases with four fixes and no
regressions. Pinned GNU 9.12 `printf-quote.sh` and `printf-mb.sh` both pass on
XSH and uutils. Reports are in `results/printf-raw-bytes/`.
Follow-ups `8fbf4847` and `981b1751` stream large printf fields in 64 KiB
chunks, reject widths that overflow the C formatter's output count, validate
conversion flags, and report malformed dynamic width or precision values. The
native printf suite passes 39/39; the optimized debug Gate 3 slice improves
from 141/11 to 142/10 across 152 cases with one fix and no regressions. Its
remaining `test_large_width_format` failure expects an error for a valid
20-million-character field, which GNU 9.11 accepts and writes. The pinned GNU
9.12 `printf.sh` selection passes on both XSH and uutils with one shared pass
and no mismatches. Reports are in `results/printf-width-followup/`.
`uniq/uniq-c-width.sh` counted 16,777,216 lines rather than 30,352,436
because XSH `yes` stopped after 32 MiB. Follow-up commit `35faa674` streams
`yes` output and `uniq -c` input; the selected GNU stress test now passes on
both XSH and pinned uutils with one shared pass and no mismatches. Its reports
are in `results/gnu-uniq-followup/`.
The pinned GNU uutils baseline has 571 PASS, 46 FAIL, 101 SKIP and one ERROR.
The full option-surface comparison, BusyBox comparison, expanded Linux surface,
and clean-image smoke remain open. ARM64 verification is out of scope; an
attempted emulated debug run was stopped and did not produce gate results. See
the current checkpoint and full results in
[`CAMPAIGN.md`](CAMPAIGN.md) and `results/`.

An earlier full native suite passed at parent revision `a0b9ce93`
(4,731 passed, 0 failed, 38 skipped). Focused native suites passed after later
changes to printf, seq, tail, bytes, tr, od, wc (15), and stat. The `stat -` path
now uses `/dev/stdin`; its 9-test suite and the zero-direct-reader ratchet
pass. Full suites run serially; each new baseline records its exact source
revision, reference pin, and test image. The current full native result is
4,964/0/38 at `89c4e143`. Offline manifest, lane-ownership,
ignored-option, kernel-read and exclusion checks passed during the resumed
campaign; these checks do not establish behavioral parity.
ARM64 verification is out of scope under the current x86_64-only campaign
scope. A later emulated debug run was stopped before producing gate results.
