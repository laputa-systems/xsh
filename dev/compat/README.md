# Compatibility Harness

Tooling for the campaign in [`CAMPAIGN.md`](CAMPAIGN.md); lane process in
[`LANES.md`](LANES.md).

## Campaign subagents

Use only **`gpt-6-luna` at `xhigh`** for every campaign subagent, including
routine work. Set both explicitly when spawning.
`python3 dev/compat/lanes.py brief LANE` renders this requirement and paths
for the current checkout; see `LANES.md` for ownership and integration.

Command behavior and shared semantic domains are implemented in XSH. Native
requests are limited to necessary host, codec and byte boundaries, or a
measured XSH bottleneck. Parsers and interpreters belong in XSH. The user has
granted standing dependency approval for this campaign; record each addition's
reason without requesting approval again.

## Setup

```sh
git clone https://github.com/uutils/coreutils ../ref/uutils-coreutils
git -C ../ref/uutils-coreutils checkout "$(python3 -c 'import json;print(json.load(open("dev/compat/upstream.lock.json"))["uutils"]["commit"])')"
export UUTILS_ROOT=$PWD/../ref/uutils-coreutils
cargo build --release -p xsh --bins -p xsht --bin xsht
cargo install cargo-nextest --locked          # for run-uutils.sh
```

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

`COMPAT_RESULTS_DIR` selects scratch report storage for both suite runners.
They share `UUTILS_SUITE_LOCK` and serialize reference runs. GNU reports are
published only when fresh logs, per-test results and complete summary counts
agree; the differential requires identical test selections on both sides.

## Commands

| Command | Does |
|---|---|
| `python3 dev/compat/parity.py` | regenerate `dev/coreutils-parity.json` from the pinned uutils tree and any results |
| `python3 dev/compat/parity.py --check` | fail if the manifest is stale |
| `python3 dev/compat/stage.py [--stage DIR]` | install `core/` in release shape with shebangs at the built `xsh`; writes `applets.json` |
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

The campaign resumed on 2026-10-07. See the resume checkpoint in
[`CAMPAIGN.md`](CAMPAIGN.md) for current verification and [`CLAIMS.md`](CLAIMS.md)
for active ownership.

Committed reports are historical: the uutils report has 2,186/5,974 passing
and four exclusions; the manifest has 73/106 in-scope applets present. The
pinned-uutils GNU report has 571 PASS, 46 FAIL, 101 SKIP and one ERROR. No XSH
GNU report or differential is committed. The full suites have not yet been
rerun against the resumed integration head.

Before running a new baseline, preserve `results/uutils-integration.json`
outside `results/` for `compare.py`, select release tools built from an exact
revision, and record the reference pin and host/libc alongside the result.
Run full suites serially. Offline manifest, lane-ownership, ignored-option,
kernel-read and exclusion checks passed in the 2026-10-05 handoff refresh;
this does not establish behavioral parity.
