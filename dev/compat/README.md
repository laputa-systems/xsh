# Compatibility Harness

Tooling for the campaign in [`CAMPAIGN.md`](CAMPAIGN.md); lane process in
[`LANES.md`](LANES.md).

## Setup

The utility campaign runs on a native Linux x86_64 host. Source the shared
environment after installing the pinned Rust toolchain and mold:

```sh
export XSH_TOOLS_ROOT="${XSH_TOOLS_ROOT:-$(dirname "$PWD")/.tools}"
export CARGO_HOME="${CARGO_HOME:-$XSH_TOOLS_ROOT/cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$XSH_TOOLS_ROOT/rustup}"
mkdir -p "$CARGO_HOME" "$RUSTUP_HOME" "$XSH_TOOLS_ROOT"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
  env CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" sh -s -- -y --no-modify-path --default-toolchain none
"$CARGO_HOME/bin/rustup" toolchain install nightly-2026-09-15 --profile minimal \
  --component rust-src --component llvm-tools
curl -fL https://github.com/rui314/mold/releases/download/v3.0.0/mold-3.0.0-x86_64-linux.tar.gz \
  -o "$XSH_TOOLS_ROOT/mold-3.0.0-x86_64-linux.tar.gz"
(cd "$XSH_TOOLS_ROOT" && echo '6c90d4a474c7c0409dfb575be03a5345878ac14fdba18de8b40fa58c60121189  mold-3.0.0-x86_64-linux.tar.gz' | sha256sum -c -)
tar -xzf "$XSH_TOOLS_ROOT/mold-3.0.0-x86_64-linux.tar.gz" --strip-components=1 -C "$CARGO_HOME"
source dev/compat/native-env.sh
```

`native-env.sh` defaults tool storage to `../.tools`, requires mold 3.0.0,
and adds `-C link-arg=-fuse-ld=mold` to `RUSTFLAGS`. The release build and
Cargo installs therefore use mold. Builds target the native host triple
`x86_64-unknown-linux-gnu`; the session host has GCC but no musl cross
compiler. The scope and evidence for this campaign are native Linux x86_64
only; do not route these commands through Docker. GNU
preparation needs a C compiler, autotools, Perl, quilt, gperf, texinfo,
autopoint, gawk, help2man, rsync, ACL tools, and `filefrag`. Install them from
the host package manager or provide them in `../.tools/gnu-env` when host
package installation is unavailable.

On this host, system package installation is unavailable to the session user.
`../.tools/gnu-env` contains the GNU tools from conda-forge plus extracted
Debian `quilt`, ACL, attr, capability, and e2fsprogs packages. `run-gnu.sh`
detects the local pkg-config files and supplies their include/library paths to
GNU configure; ACL and capability support are enabled in the prepared tree.
The local `quilt` is Debian quilt 0.69 (the conda package named `quilt` is a
different tool).

```sh
git clone https://github.com/uutils/coreutils ../ref/uutils-coreutils
git -C ../ref/uutils-coreutils checkout "$(python3 -c 'import json;print(json.load(open("dev/compat/upstream.lock.json"))["uutils"]["commit"])')"
git clone https://github.com/laputa-systems/laputa ../laputa
git -C ../laputa checkout master
export UUTILS_ROOT=$PWD/../ref/uutils-coreutils
source dev/compat/native-env.sh
CARGO_TARGET_DIR=/workspace/targets/native-bytes-hash CARGO_BUILD_JOBS=1 \
  CARGO_PROFILE_RELEASE_LTO=false cargo build --release --locked \
  -p xsh --bin xsh -p xsht --bin xsht
cargo install cargo-nextest --locked          # for run-uutils.sh
```

Keep mold enabled for this optimized release build. On the campaign host,
thin-LTO stalled while linking the `xsh` dispatcher after mold exited; setting
`CARGO_PROFILE_RELEASE_LTO=false` completed the build. The `CARGO_TARGET_DIR`
and single build job keep this build isolated from other compatibility runs.

GNU runs also need a C toolchain, autotools, perl and the packages uutils'
`build-gnu.sh` uses: `quilt gperf texinfo autopoint gawk help2man rsync`. In a
sandbox where `apt` cannot open `/dev/null` as its `_apt` user, add
`-o APT::Sandbox::User=root` to `apt-get update` and `install` (signature
verification stays on). GNU `configure` refuses to run as root, so `run-gnu.sh` bypasses that check
for configure only and runs the test suites in a user namespace that maps
`GNU_RUN_UID` (default 1000) to the invoking user: tests see an unprivileged uid,
because many GNU tests change behavior as root, without a second account or any
chown. `GNU_JOBS` (default 3) sets `make -j`.

## Commands

| Command | Does |
|---|---|
| `python3 dev/compat/parity.py` | regenerate `dev/coreutils-parity.json` from the pinned uutils tree and any results |
| `python3 dev/compat/parity.py --check` | fail if the manifest is stale |
| `python3 dev/compat/stage.py [--stage DIR]` | install `core/` in release shape with shebangs at the built `xsh`; writes `applets.json` |
| `dev/compat/xsh-uutests UTIL ARGS...` | manual shell adapter for a generated stage; the suite uses a symlink to the native `xsh` dispatcher so argv, signals and deleted working directories pass through without an extra shell |
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

## Verified setup and current run

The campaign setup has been exercised on a native Linux x86_64 host. The
workspace-local `nightly-2026-09-15` toolchain and mold 3.0.0 x86_64 release
were installed and verified by SHA-256; `native-env.sh` selects both for
builds. Docker is not used. Release `xsh`/`xsht` builds, the pinned uutils
test harness, GNU 9.12 preparation, and real GNU/uutils test runs all work.
Keep this scope to native Linux x86_64. Use one integrator; any subagent must
be GPT-6 Luna at xhigh.

The latest merged uutils results combine the 2026-10-09 full run with a focused
`factor sort unlink` refresh: 5,123 / 5,974 passing, 851 failing, and 4 excluded.
Compared with the checked-in baseline, 2,755 test IDs now pass and none
regressed. The focused `factor sort` result is 139 / 242 (factor 23/25, sort
116/217). The byte-key fast path restored `test_factor::test_parallel`, and
removing unneeded unique-sort key work brought the buffer-size test under its
30-second limit. `--batch-size` validation and bounded merge tests pass. The
large-factor case still times out. The `cat head tail tac rm tee touch` slice is 432 / 499, up 157
passing IDs with no regressions. The `unlink` slice is 6 / 7; raw non-UTF-8
operand paths now work, while its extra-operand wording follows GNU. The merged results and remaining failures are
tracked in [`CAMPAIGN.md`](CAMPAIGN.md), `gaps.json`, and `results/`.

`run-gnu.sh prepare` completed with ACL, capability, and Linux xattr support.
The baseline run against uutils recorded 573 passed, 85 skipped, 58 failed,
and 3 harness errors out of 719 test records. Remaining test gaps and exact
results are maintained in `CAMPAIGN.md`, `gaps.json`, and `results/`.

On this host, the release `xsh` dispatcher build stalled in thin-LTO after its
linker exited. Setting `CARGO_PROFILE_RELEASE_LTO=false` completed the same
optimized release build with mold 3.0.0; `xsht` built successfully with the
workspace release profile. Use this override only for the dispatcher build if
that host-specific stall recurs.
