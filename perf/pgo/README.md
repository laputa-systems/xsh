# Checked-in PGO profile

`xsh-*.profdata` files are merged LLVM profile-guided-optimization profiles for
the `xsh` binary, one per musl target. They are committed as candidate profiles
and historical artifacts while PGO quality is under investigation. Release builds
and CI do not consume them by default right now.

It is a multi-megabyte binary blob, so it is stored with **Git LFS** (see
`.gitattributes`). You need `git-lfs` installed (`brew install git-lfs`) to get the
real file on clone/pull; without it the working copy is a ~130-byte pointer. CI
still fetches LFS objects via `actions/checkout` with `lfs: true`, but the release
path leaves PGO disabled unless explicitly opted in.

## How it's produced

```sh
make prof-pgo            # XSH_PROF_SCALE=<n> to size the corpus
```

`tools/prof-linux.xsh` builds an instrumented `xsh` on the `profiling` profile
(musl, `net` excluded), exercises it through the `perf/scenarios/*.xsh` and the
analysis showcases (tokei/loc/etc.) run over this repo's own source, merges the
counters with `llvm-profdata`, and writes the result here as
`xsh-<rust-host-triple>.profdata`. Set `XSH_PGO_PROFILE=...` to override that
path. See `perf/README.md` for the architecture (including why `cargo test` /
`cargo bench` are not part of the PGO workload).

## How it's consumed

PGO is currently opt-in because the checked-in profiles regress interpreter and
mixed CLI workloads. To test a release build with PGO, run:

```sh
RELEASE_USE_PGO=1 make release-static TARGET=x86_64-unknown-linux-musl
```

When enabled and the target's profile exists, `make release-static` appends:

```
-C profile-use=perf/pgo/xsh-$(TARGET).profdata -C llvm-args=-pgo-warn-missing-function
```

Only the **musl** release targets may use it (it is gathered on musl); darwin
builds skip it. `-pgo-warn-missing-function` makes functions that are absent from
the profile non-fatal warnings rather than errors.

## Maintenance

- **Regenerate after meaningful code changes** on each native musl target you want
  to optimize. Do not re-enable CI consumption until the benchmark acceptance
  criteria in `PGO.md` pass. A stale profile should not miscompile, but it can
  degrade performance materially.
- **Regenerate after a toolchain bump.** The profile format is tied to the rustc /
  LLVM version (pinned in `rust-toolchain.toml`). If the toolchain changes,
  `-Cprofile-use` will reject an incompatible profile with a hard error — rerun
  `make prof-pgo` to refresh it.
- It is a binary blob; review changes by regenerating rather than diffing.
