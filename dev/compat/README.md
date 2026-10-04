# Compatibility Harness

Tooling for the campaign in [`CAMPAIGN.md`](CAMPAIGN.md); lane process in
[`LANES.md`](LANES.md).

## Setup

```sh
git clone https://github.com/uutils/coreutils ../ref/uutils-coreutils
git -C ../ref/uutils-coreutils checkout "$(python3 -c 'import json;print(json.load(open("dev/compat/upstream.lock.json"))["uutils"]["commit"])')"
export UUTILS_ROOT=$PWD/../ref/uutils-coreutils
cargo build --release -p xsh --bins -p xsht --bin xsht
cargo install cargo-nextest --locked          # for run-uutils.sh
```

GNU runs also need a C toolchain, autotools, perl and `quilt` (uutils'
`build-gnu.sh` requirements).

## Commands

| Command | Does |
|---|---|
| `python3 dev/compat/parity.py` | regenerate `dev/coreutils-parity.json` from the pinned uutils tree and any results |
| `python3 dev/compat/parity.py --check` | fail if the manifest is stale |
| `python3 dev/compat/stage.py [--stage DIR]` | install `core/` in release shape with shebangs at the built `xsh`; writes `applets.json` |
| `dev/compat/xsh-uutests UTIL ARGS...` | uutils multicall contract over the stage (`XSH_COMPAT_STAGE`) |
| `dev/compat/run-uutils.sh [UTIL...]` | Gate 3: uutils `tests/by-util` against XSH; writes `results/uutils-integration.json` |
| `dev/compat/run-gnu.sh prepare` | fetch GNU 9.12 and prepare its tests with uutils' `build-gnu.sh` |
| `dev/compat/run-gnu.sh uutils [TEST...]` | GNU tests against pinned uutils (cached baseline) |
| `dev/compat/run-gnu.sh xsh [TEST...]` | the same tests against the XSH stage |
| `dev/compat/run-gnu.sh diff` | Gate 4 four-cell differential into `results/gnu-differential.json` |
| `python3 dev/compat/check_ignored_options.py` | Gate 6 ratchet over discard buckets in `core/*.xsh` |

## Data files

| File | Owner | Content |
|---|---|---|
| `upstream.lock.json` | integrator | pinned uutils commit and GNU version |
| `../coreutils-parity.json` | generated | per-utility parity manifest |
| `aliases.json` | integrator | alias executable → shared applet |
| `exclusions.json` | integrator | per-test-ID uutils exclusions with category and reason |
| `gaps.json` | lanes (own utilities) | known semantic gaps |
| `host-deps.json` | generated once | uutils tests that spawn host programs or use host oracles |
| `gnu-patches.json` | `gnu-patch-classify` lane | classification of uutils' GNU test patches |
| `ignored-options-baseline.json` | integrator | remaining legacy discard buckets (shrink-only) |
| `results/` | generated | suite outputs |

## Status

Verified in the authoring session: manifest generation against the pinned
tree (108 utilities, 106 in scope, 41 present), the ignored-options ratchet
(16 legacy buckets), staging and the adapter with a stand-in interpreter
(argv including empty arguments and `--`, NUL bytes on stdin, exit status,
alias names, `false` placeholders), and both `results.py` modes on synthetic
reports.

Not yet executed, because that session's network policy blocked crates.io:
building XSH, `run-uutils.sh` against real applets, and `run-gnu.sh`. The
first session with crates.io access should run the baseline (see the handoff
checklist in the `campaign-utils` commit log) and fix whatever the scripts
get wrong.
