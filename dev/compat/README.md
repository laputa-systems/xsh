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

Gate 3 was rerun at `92a91b12` against pinned uutils
`e7c9f3194280835c4487c2945c68d5f01ccacc8d` in the `Dockerfile.test` image.
The report records 5,125 pass, 824 fail, and four exclusions across 106
utilities; it improves the preceding full run by one pass with no regressions.
The pinned GNU 9.12 XSH run at the same revision records 367 PASS, 189 FAIL,
25 ERROR, and 138 SKIP across 719 tests. Its four-cell differential has 358
shared passes, 213 uutils-only passes, 9 XSH-only passes, and 52 shared
failures. `test_wc::test_files0_progressive_stream` timed out at 120 seconds in
Gate 3. Follow-up commit `d2938a41` streams `wc --files0-from=-` counts and
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
`uniq/uniq-c-width.sh` counted 16,777,216 lines rather than 30,352,436
because XSH `yes` stopped after 32 MiB. Follow-up commit `35faa674` streams
`yes` output and `uniq -c` input; the selected GNU stress test now passes on
both XSH and pinned uutils with one shared pass and no mismatches. Its reports
are in `results/gnu-uniq-followup/`; the full Gate 4 report above remains at
`92a91b12`.
The pinned GNU uutils baseline has 571 PASS, 46 FAIL, 101 SKIP and one ERROR.
BusyBox comparison, the expanded Linux surface, aarch64 debug gate, and
clean-image smoke remain open. See the current checkpoint and full results in
[`CAMPAIGN.md`](CAMPAIGN.md) and `results/`.

The most recent full native suite passed at parent revision `a0b9ce93`
(4,731 passed, 0 failed, 38 skipped). Focused native suites passed after later
changes to printf, seq, tail, bytes, tr, od, wc (15), and stat. The `stat -` path
now uses `/dev/stdin`; its 9-test suite and the zero-direct-reader ratchet
pass. Full suites run serially; each new baseline records its exact source
revision, reference pin, and test image. Offline manifest, lane-ownership,
ignored-option, kernel-read and exclusion checks passed during the resumed
campaign; these checks do not establish behavioral parity.
The x86_64 results do not close the pinned `aarch64-unknown-linux-musl` gate;
it still needs a debug-profile run in the test image.
