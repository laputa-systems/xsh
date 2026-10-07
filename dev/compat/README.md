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

Gate 3 was rerun at `fabe52f6` against pinned uutils
`e7c9f3194280835c4487c2945c68d5f01ccacc8d` in the `Dockerfile.test` image.
The current report records 5,124 pass, 825 fail, and four exclusions across
106 utilities; the previous full run had 5,110 pass at the same nonexcluded
denominator, with no regressions in the new run. The pinned GNU baseline still
has 571 PASS, 46 FAIL, 101 SKIP and one ERROR. The XSH-side GNU run and
differential, BusyBox suite, expanded Linux surface, and clean-image smoke
remain open.

The most recent full native suite passed at parent revision `a0b9ce93`
(4,731 passed, 0 failed, 38 skipped). The changed applet suites at `fabe52f6`
passed 57 focused tests. Full suites run serially; each new baseline records
its exact source revision, reference pin, and test image. Offline manifest,
lane-ownership, ignored-option, kernel-read and exclusion checks passed during
the resumed campaign; these checks do not establish behavioral parity.
The x86_64 results do not close the pinned `aarch64-unknown-linux-musl` gate;
it still needs a debug-profile run in the test image.
