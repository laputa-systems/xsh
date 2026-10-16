# Cargo queue review

## Existing boundaries

`dev/test_workflows.xsh` serially sends Cargo commands to `dev/cargo_steps.xsh`. For `cargo test`, `test_steps` splits compilation from the test command only when the jemalloc preload is active: it runs `cargo test ... --no-run`, then runs the original `cargo test` command. The latter still owns test execution, so releasing a lock around that Cargo process would not guarantee it cannot compile.

The canonical replay driver holds `.work/consolidation/infra/native-suite.lock` for the entire Docker batch. It does not acquire `.work/consolidation/infra/cargo.lock`; the inner XSH Cargo steps do not consult that lock either. The shared Cargo lock exists and is used by final observer/comparative tooling, but it does not currently coordinate canonical, lane, or W9 builds. Cargo's per-target `.cargo-lock` does not coordinate distinct target directories.

The W9 source builder derives an isolated target directory from source commit, tree, overlay, and role, then launches its Docker build without the shared Cargo lock. The adapter runner invokes that builder directly. Isolation protects artifacts but does not serialize compiler load.

## Safe next-run plan

Use the existing `.work/consolidation/infra/cargo.lock` as the common host-visible compile lock. Every driver must hold it around its real Cargo compile invocation, including Cargo launched inside the canonical container and the W9 host builder. Ordinary `build`, `check`, and `clippy` commands hold it through completion. For example, the canonical XSHT harness compile becomes:

```sh
flock -w 1800 /work/.work/consolidation/infra/cargo.lock \
  cargo test --locked --profile verification --target x86_64-unknown-linux-musl \
    -p xsht --test integration --no-run --message-format=json-render-diagnostics
```

For a Cargo test, the safe overlap boundary is after that locked `--no-run` command has built the selected harness and before test execution. Release the lock there, execute every selected executable reported in Cargo's JSON artifact records directly with the same test filter/arguments and runtime environment, then let the canonical runner reacquire the lock before its next Cargo compile stage. A lane build holding that lock may then finish during the known test-only interval; the canonical next compile waits for it. The current `test_steps` split is insufficient because its second step re-enters Cargo. Until a direct executable run is in place, serialize the complete `cargo test` process and do not overlap lane compilation with it.

This requires a next-run source/driver change. The running 787 replay cannot participate: its outer runner has only the whole-batch native-suite lock, and its inner Cargo steps have no shared compile lock. Acquiring a lock only in a lane would not prevent the current canonical process from entering its next Cargo stage. Keep the current run uninterrupted and leave pending lane builds queued.

## W9 artifact audit

The W9 host assignment probe's `.cargo-lock`, `.cargo-artifact-lock`, `.cargo-build-lock`, and `target/debug/build/*` entries are timestamped `2026-10-16 01:10:52 UTC`; `.rustc_info.json` is `01:11:33`; `target/debug/w9-assignment-probe` and `actual-proof.tsv` are `01:12:17`. The probe directory has no invocation argv or start/end record. These are artifact times only: they do not establish Cargo invocation duration or prove overlap with canonical runtime. The probe used its own host `target/debug` tree, and that tree's Cargo locks do not coordinate with the canonical target. No speed or overlap credit is warranted; the retained artifacts do not qualify this build under the shared compile queue.

Minimum enforcement for future drivers is to require the same shared Cargo lock for every Cargo entry point, including W9 helper builds. Keep per-revision target isolation, but do not treat it as queue serialization. Do not report timing or overlap from artifact mtimes; capture invocation boundaries only if a future performance claim needs them.
