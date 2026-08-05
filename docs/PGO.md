# Profile-Guided Optimization

PGO is an application optimization workflow, separate from the Divan
benchmark and regression workflow in `docs/BENCHMARKING.md`.

Do not run PGO during ordinary runtime, IR, or representation iteration. The
instrumented rebuild is intentionally expensive and provides low-signal
feedback while non-PGO latency, allocation, behavior, or coverage results are
still changing. First make `make bench` and the relevant correctness gates pass.

## Current POC

The initial POC targets only the standalone `xshi` binary from the `xshi`
crate. `xsh` and `xsht` are not given PGO workloads and remain ordinary release
builds. The workflow keeps the application and its profile driver in separate
target directories:

```sh
make pgo-instrument
make pgo-profile
make release-pgo
```

`pgo-instrument` uses `cargo rustc`'s final-target flags to instrument only the
standalone `xshi` binary; its `xsh` dependency is compiled without them. The
separate integration driver (including `xsht`), test dependencies, benchmark
code, build scripts, and proc macros are also compiled without PGO flags. The
binary is written under
`target/pgo-instrument/<target>/`, while raw and merged profiles live under
`target/pgo-profiles/<target>/`. Override the local host target with
`PGO_TARGET=<target>` when the release matrix needs the package target.

`pgo-profile` builds the ordinary integration driver under
`target/pgo-driver/`, then runs only the ignored
`runtime::interactive::xshi_pgo_profile_workload` test. The driver launches the
instrumented standalone `xshi` binary explicitly through the existing PTY
harness and sets `LLVM_PROFILE_FILE` so the raw profile must come from the
child application.

The workload starts `xshi` with `--no-config`, a temporary home and workdir, a
fixed PATH, no host profile, a 24-by-100 PTY, and a deterministic 45,000-entry
history. It waits for the prompt, opens incremental history search, navigates a
match, accepts and executes it, then exits. This is a proof-of-concept for
startup and history search/rendering, not a measured model of user behavior.
It assumes one session and one fixed fixture; user-curated scenario weights,
cold/warm state, corpus sizes, terminal matrices, and baseline-versus-PGO
latency distributions remain follow-up work.

`llvm-profdata merge` receives only the raw profiles emitted in that profile
directory. The merged profile is intentionally an xshi-only POC: it should
contain `xshi` compilation units and no `xsh.` or `xsht.` compilation-unit
records. To inspect provenance and coverage after collection, use:

```sh
host_target=$(rustc -vV | awk '/^host:/ {print $2}')
$(rustc --print sysroot)/lib/rustlib/$host_target/bin/llvm-profdata show --all-functions --counts target/pgo-profiles/$host_target/merged.profdata
```

The profile has no Divan benchmark entry points. `release-pgo` applies
`-Cprofile-use` only to a separate standalone `xshi` release build under
`target/pgo-use/<target>/`, enables LLVM's missing-function diagnostic, and
rejects a final binary that still contains LLVM profile runtime symbols or
profile-generation sections. `xsh` and `xsht` remain ordinary release builds;
`make bench-pgo` is retained as a compatibility no-op until a frontend-specific
benchmark comparison is designed.

For an end-to-end comparison, point the same profile-only driver at the
ordinary and PGO standalone `xshi` binaries. Set `XSH_PGO_TIMINGS=1` to print
prompt, search, navigation, execute, and total timings; repeat each command
enough to report a distribution rather than treating one run as a result. The
current POC keeps that comparison manual because its fixed single-session
fixture is not yet a user-validated latency benchmark:

```sh
host_target=$(rustc -vV | awk '/^host:/ {print $2}')

XSH_PGO_TIMINGS=1 \
XSH_PGO_BINARY=target/pgo-baseline/$host_target/release/xshi \
LLVM_PROFILE_FILE=/tmp/xsh-baseline-%p.profraw \
CARGO_TARGET_DIR=target/pgo-driver \
cargo test --test integration --features "native-tests net tools" \
  runtime::interactive::xshi_pgo_profile_workload -- --ignored --exact --test-threads=1 --nocapture

XSH_PGO_TIMINGS=1 \
XSH_PGO_BINARY=target/pgo-use/$host_target/release/xshi \
LLVM_PROFILE_FILE=/tmp/xsh-pgo-%p.profraw \
CARGO_TARGET_DIR=target/pgo-driver \
cargo test --test integration --features "native-tests net tools" \
  runtime::interactive::xshi_pgo_profile_workload -- --ignored --exact --test-threads=1 --nocapture
```

## Future frontend separation

The release workflow still packages the normal `xsh-multicall` binary. The
current PGO step validates an xshi-only candidate for the same target; it does
not claim that `xsh` or `xsht` are profiled.

The longer-term direction is to split the multicall binary into individual
`xsh`, `xshi`, and `xsht` binaries. Once that boundary exists, each frontend
can own its curated application workload, profile directory, profile-use build,
and release comparison without cross-frontend profile ambiguity.
