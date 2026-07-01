# PGO Plan

This document tracks the remaining work needed to make profile-guided
optimization useful for XSH instead of occasionally harmful.

## Current State

PGO profiles are target-specific and live under `perf/pgo/`:

- `xsh-aarch64-unknown-linux-musl.profdata`
- `xsh-x86_64-unknown-linux-musl.profdata`

The amd64 profile should be regenerated natively on threadripper. That host is
an amd64 Alpine musl machine, so Docker is not part of the normal PGO path.

The current amd64 corpus still fails acceptance. `archive-package` improves by
about 26-28% and some startup/frontend cases improve, but interpreter-heavy and
mixed CLI workloads regress badly. With the discovered corpus and scale 64,
release-shaped PGO regressed `interp-pipeline-sort` by 86.86%, `interp-regex` by
98.06%, and `mixed-cli` by 61.25%. Profiling-shaped PGO also regressed those
same workloads by 83.25%, 87.36%, and 56.81%, so this is not only a release-build
profile-consumption mismatch.

Release and CI consumption of checked-in profiles is disabled by default while
this remains unresolved. Use `RELEASE_USE_PGO=1 make release-static ...` only for
explicit experiments.

## Goals

- PGO must improve or stay neutral for the common CLI workloads we care about.
- PGO must not trade broad interpreter throughput for one benchmark win.
- The profile corpus must exercise startup, parse/check/lower, interpreter hot
  loops, module loading, JSON/regex/path/string work, and realistic tool scripts.
- Benchmark deltas must be measured against both `profiling` and release-shaped
  builds before we update checked-in `.profdata` files.

## Working Hypotheses

- The current profile likely biases LLVM against interpreter hot loops. Adding
  broad corpus volume did not fix interpreter or mixed-workload regressions.
- Some benchmark regressions may still come from profile/build-shape mismatch,
  but profiling-shaped PGO regresses too, so build-shape mismatch is not the whole
  problem.
- Archive/package work is probably over-benefiting relative to general CLI use;
  the profile may be trading broad interpreter throughput for archive wins.
- Missing-profile warnings are actionable. The latest release-shaped build still
  produced 9235 missing-profile warnings, so warning volume remains a failed
  profile-quality signal.

## Recent Benchmark Evidence

Latest threadripper run after adding discovery, startup fixtures, frontend
fixtures, and the benchmark harness:

```text
release-shaped:
archive-package              -26.25%
extension-count              +20.09%
json-log-rollup              +20.34%
manifest-hash                 +8.67%
startup-minimal               -5.71%
startup-control              +16.56%
startup-modules               +3.52%
frontend-compound-types      +14.20%
frontend-control-flow        +10.92%
frontend-pipeline-shapes      +0.45%
interp-json-record           +35.74%
interp-pipeline-sort         +86.86%
interp-regex                 +98.06%
mixed-cli                    +61.25%

profiling-shaped:
archive-package              -27.91%
extension-count              +19.66%
json-log-rollup              +12.00%
manifest-hash                +11.01%
startup-minimal              -19.73%
startup-control               +5.50%
startup-modules               -9.70%
frontend-compound-types       +1.38%
frontend-control-flow         -1.39%
frontend-pipeline-shapes     -11.68%
interp-json-record           +25.56%
interp-pipeline-sort         +83.25%
interp-regex                 +87.36%
mixed-cli                    +56.81%
```

This profile should not be used in CI or release builds.

## Corpus Work

Keep docs examples out of the training corpus. Examples feed documentation and
should not become performance fixtures. PGO-only startup and smoke workloads
belong under `perf/`.

Add or refine workloads in these groups:

- Startup scripts in `perf/startup/`: tiny scripts that parse, initialize modules,
  run a small amount of control flow, and exit.
- Interpreter stress scripts in `perf/interpreter/`: direct `xsh` execution of
  pipelines, records, JSON, regex, path operations, grouping, sorting, and loops.
- Tool scenarios in `perf/scenarios/`: realistic CLI work such as extension
  counting, manifest hashing, JSON log rollup, and package archiving.
- Parse/check/lower workloads: scripts that are representative of medium-sized
  real XSH programs even when runtime work is small.
- Error-path workloads: a small number of invalid or failing scripts, enough to
  cover common diagnostic machinery without biasing the profile toward failure.

Use explicit weights instead of relying on one flat scenario list:

- High repeat: startup and common short-lived commands.
- Medium repeat: interpreter stress and hot tool scenarios.
- Low repeat: expensive archive/package flows and uncommon error paths.

The repeat counts should be controlled by environment variables so we can run a
quick validation profile and a full profile from the same script.

## Build Discipline

For every profile refresh, record the exact collection command, target triple,
feature set, and consuming build command.

Required collection command on threadripper:

```sh
XSH_PROF_SCALE=64 make prof-pgo
```

When comparing release-shaped PGO locally on threadripper, use the native linker
if the configured Zig musl linker conflicts with the host startup objects:

```sh
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/bin/cc \
CC_x86_64_unknown_linux_musl=/usr/bin/cc \
RUSTFLAGS="-Cprofile-use=$PWD/perf/pgo/xsh-x86_64-unknown-linux-musl.profdata -Cllvm-args=-pgo-warn-missing-function" \
cargo build --release --no-default-features --features tools --bin xsh
```

A PGO profile is not ready to accept if the consuming build produces a large
missing-profile warning count and benchmark results are mixed or negative.

## Benchmark Methodology

Measure deltas with the same corpus scale that produced the candidate profile.
Use warmed filesystem caches and keep benchmark binaries in separate target
directories so baseline and PGO builds are not accidentally reused.

Build three binaries when practical:

- `profiling` baseline: same profile used by `make prof-*` comparison work.
- `profiling` plus PGO: generated by `make prof-pgo` or an equivalent command.
- Release-shaped baseline and PGO: `--release --no-default-features --features tools`.

Benchmark these groups separately:

- Tool scenarios: `archive-package`, `extension-count`, `manifest-hash`,
  `json-log-rollup`.
- Startup scripts: every script under `perf/startup/`.
- Interpreter scripts: selected `perf/interpreter/*.xsh` files that cover JSON,
  pipelines, sorting, records, paths, regex, and grouping.
- A mixed workload that interleaves startup, interpreter, and tool scenarios in a
  fixed order. This catches profiles that improve one isolated workload by
  harming the aggregate CLI experience.

For each benchmark:

- Run a warmup batch before measuring.
- Measure at least 5 batches and report the median.
- Use enough repetitions per batch that timer noise is small: hundreds for tiny
  startup scripts, tens for tool scenarios, and enough interpreter repetitions to
  exceed several hundred milliseconds per batch.
- Report wall-clock seconds and percent delta using `(pgo - baseline) / baseline`.
  Negative is faster.
- Keep raw CSV output under `/tmp` unless the user asks to preserve it.

The repository includes a Python-free benchmark harness for this matrix:

```sh
XSH_PROF_SCALE=64 \
XSH_PGO_BENCH_BASELINE=target/bench-release-base/release/xsh \
XSH_PGO_BENCH_PGO=target/bench-release-pgo/release/xsh \
perl perf/pgo-bench.pl
```

The harness writes CSV to `/tmp/xsh-pgo-bench.csv` by default and prints one
summary line per workload. Override `XSH_PGO_BENCH_OUT`,
`XSH_PGO_BENCH_BATCHES`, `XSH_PGO_BENCH_REP_SCALE`, and
`XSH_PGO_BENCH_CORPUS` when comparing multiple candidate profiles. Use a small
rep scale such as `0.05` only for smoke-testing the harness, not for acceptance
numbers.

Summarize missing-profile warnings from a captured release-shaped PGO build log
with:

```sh
perl perf/pgo-warn-summary.pl /tmp/xsh-release-pgo-build.log
```

A candidate PGO profile should meet these acceptance criteria before committing:

- No benchmark group has a severe regression. Treat anything worse than +5% as a
  blocker unless there is a deliberate tradeoff.
- The mixed workload improves or is within noise.
- Startup does not regress.
- At least one meaningful real tool scenario improves.
- Missing-profile warnings are substantially reduced from the current state, or
  there is a written reason why remaining warnings do not matter.

## Investigation Loop

1. Generate a small validation profile with low repeat counts and confirm the
   training script produces `.profraw` and merges `.profdata` correctly.
2. Generate the full target profile with `XSH_PROF_SCALE=64 make prof-pgo`.
3. Build profiling and release-shaped baselines plus their PGO variants from
   clean target directories.
4. Capture missing-profile warning counts from the consuming PGO builds.
5. Run the benchmark matrix and save the raw CSV in `/tmp`.
6. If regressions remain, inspect which benchmark group regressed and adjust only
   that corpus weight or missing workload class.
7. Repeat until benchmark deltas and missing-profile warnings both support the
   checked-in profile update.

## Next Investigation

Do not add more broad corpus volume until the regression source is isolated.
Generate and benchmark profile subsets first:

1. `startup-only`: startup scripts only.
2. `scenario-only`: `perf/scenarios/*.xsh` only.
3. `archive-only`: `archive-package.xsh` only.
4. `hot-scenarios-no-archive`: extension count, manifest hash, and JSON rollup.
5. `interpreter-only`: direct `perf/interpreter/*.xsh` execution.
6. `frontend-only`: `perf/frontend/*.xsh` execution.
7. Pairwise combinations that look promising, especially `startup-only` plus
   `interpreter-only` and `hot-scenarios-no-archive` plus `interpreter-only`.

For each subset:

- Generate a throwaway profile under `target/pgo/subsets/NAME.profdata`.
- Build both profiling-shaped and release-shaped PGO consumers from clean target
  directories.
- Run `perl perf/pgo-bench.pl` and compare against the same baselines.
- Keep profiles only if interpreter and mixed CLI are neutral or better.
- Capture missing-profile warning counts, but let benchmark deltas decide the
  subset ranking.

If every subset that includes interpreter training still regresses interpreter
benchmarks, inspect generated code or LLVM decisions rather than continuing to
adjust corpus weights. Useful next probes are `cargo asm`/`cargo llvm-lines`,
`perf record` on `interp-pipeline-sort` baseline vs PGO, and checking whether PGO
is increasing code size or instruction-cache pressure in `runtime::eval` and
lowered-IR helpers.

## Open Work

- Add more parse/check/lower-heavy training scripts under `perf/frontend/` when
  a benchmark exposes a missing front-end shape.
- Keep extending `perf/pgo-bench.pl` as new acceptance workloads are identified.
- Improve `perf/pgo-warn-summary.pl` after capturing representative warning logs;
  the current version provides a coarse count and bucketed summary.
- Decide whether `make prof-pgo` should build a release-shaped PGO binary in
  addition to the profiling binary, or whether that should be a separate target.
- Re-run the full benchmark matrix after each corpus change before accepting a
  new `perf/pgo/*.profdata` artifact.
- Keep CI/release PGO disabled until a candidate profile meets the acceptance
  criteria here.
