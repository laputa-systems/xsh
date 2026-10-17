# Permission fixture lint repair

Base source `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`; repaired source
`7eb62cd993e578c13bf6befd68a6a6ac15133c78` changes exactly two lines in
`crates/xsht/src/cli/files.rs`: `Permissions::from_mode(0)` becomes
`Permissions::from_mode(0o0)` at lines 671 and 764. Both fixtures retain mode
zero. No behavior, public contract, dependency, toolchain, or workflow changes.

The [original R6 log](original-r6-red.log) records the workspace all-target,
all-feature Clippy failure on the base source. Both diagnostics are
`clippy::non_octal_unix_permissions`. The original failed receipt and logs
remain unchanged in
`.work/final-review-evidence/final-correctness-gates/correctness-b235a1a43-20261017-r1`.
The retained copy compares byte-for-byte with its original step-six log.

The [focused command](focused-clippy.command) checks the repaired source:

```sh
cargo clippy --locked --offline -p xsht --target x86_64-unknown-linux-musl \
  --all-targets --all-features --quiet -- -D warnings
```

It exits 0, as recorded in [status](green.status), with empty
[stdout](green.stdout) and [stderr](green.stderr). Source checks against the
repair commit pass before and after. The container uses pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
four CPUs, 8 GiB, four Cargo jobs, `kache`, `mold`, and the original R6 musl
linker flags. `TMPDIR` stays under this lane's ignored `.work/tmp`.

The root explicitly authorized reuse of the terminal R6 Cargo target for this
focused check under the original Cargo lock. The lock was released after the
passing run. No product build, native suite, release check, or full workspace
Clippy rerun was performed. The existing lint failure supplies the reproducer;
no behavior test was added for the equivalent numeric spelling.
