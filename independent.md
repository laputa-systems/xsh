I'd like to get rid of xsh-multicall and instead build individual xsh, xsht, xshi.

The important discovery is that XSH already has the desired source-level architecture:

- The root package exposes direct `xsh`, `xsht`, and `xshi` binaries in `Cargo.toml:127`.
- The root package provides the shared `xsh` library crate.
- `xshi` and `xsht` already depend on that library.
- `xsh-multicall` is mostly a thin `argv[0]` dispatcher that links the three tool crates together.

So the recommended design is:

> Build three independent executables from the existing shared Rust `xsh` library crate. Do not introduce a runtime `libxsh.so` yet.

Cargo will compile common dependencies once per feature/target set, then link separate executables. This gives source/build reuse without the portability and ABI problems of a dynamic Rust library.

### What not to do initially

Do **not** turn `libxsh` into a dynamic system library as part of this migration.

A `cdylib`/`dylib` approach would introduce:

- unstable Rust-to-Rust ABI concerns;
- loader/search-path/package complexity;
- complications for static musl releases;
- separate platform-specific library packaging;
- more failure modes in the factory Docker images.

It might reduce aggregate disk size, but it would be a much larger release architecture change. First get reliable independent binaries.

## Recommended Product Design

Use the root package’s existing binaries:

```text
xsh   -> src/entrypoints/xsh.rs
xsht  -> crates/xsht/src/main.rs
xshi  -> crates/xshi/src/main.rs
```

Build them together in one Cargo invocation:

```sh
cargo build \
  --locked \
  --profile dist \
  --target "$TARGET" \
  --features "net tools" \
  --bin xsh \
  --bin xsht \
  --bin xshi
```

The exact feature set should be confirmed against the current release build, but the key property is **one invocation and one target directory**, not three independent builds.

Cargo will reuse:

- the root `xsh` library compilation;
- shared parser/runtime/registry dependencies;
- shared dependency artifacts;
- incremental/cache state across all three link targets.

Each executable will still contain its own statically linked copy of the code it needs. That is expected and is preferable for the current static Linux/musl release model.

## Execution Plan

### 1. Establish a baseline

(omitted)

### 2. Add direct-binary build support

Modify `../xsh/Makefile` so the Docker cross-build produces:

```text
target/<target>/dist/xsh
target/<target>/dist/xsht
target/<target>/dist/xshi
```

Do not initially delete the multicall crate. First make the direct build work beside it.

The build target should:

- use the same locked dependencies and target flags;
- use one Cargo invocation;
- preserve thin LTO and existing release profile settings;
- verify that all three output files exist;
- verify they are real target binaries, not scripts or placeholders.

The output validation should check at least:

- ELF magic on Linux;
- executable permissions;
- nontrivial file size;
- target architecture;
- successful `--help` execution inside the target container.

A size check alone is insufficient. We already observed that a padded shell stub can pass a size threshold.

### 3. Move or preserve benchmarks

Before removing `crates/xsh-multicall`, deal with its benchmark target.

The current benchmark imports functionality from:

- `xsh`;
- `xshi`;
- `xsht`.

Options, in order of preference:

1. Move it to a neutral `xsh-bench` package that depends on the three libraries.
2. Move individual benchmarks to the owning package if they are clearly tool-specific.
3. Retain the multicall package temporarily only as a benchmark host, but rename it and remove its release-binary role.

Do not silently delete the benchmark coverage. The benchmark crate currently contains user-facing latency and allocation measurements.

### 4. Remove multicall from the release graph

Once direct binaries build and smoke-test successfully:

- remove `crates/xsh-multicall` from the workspace;
- remove its package manifest and dispatcher;
- remove `mimalloc` if `cargo tree` confirms it is no longer needed;
- update `Cargo.lock`;
- remove multicall-specific benchmark/package references or migrate them;
- remove comments such as the disabled “xshi-only PGO” note that exists only because multicall has not been separated.

Keep the change focused: do not extract a new `xsh-core` crate unless the direct-binary migration proves the current root `xsh` library boundary is insufficient.

### 5. Update release packaging

Change:

- `.github/workflows/release.yml`;
- `.github/scripts/release-package-linux.xsh`;
- `.github/scripts/release-package-darwin.xsh`;
- static-link verification scripts;
- release documentation and installation instructions.

The release package should contain three explicitly named binaries, for example:

```text
xsh-<version>-<arch>-linux-musl
xsht-<version>-<arch>-linux-musl
xshi-<version>-<arch>-linux-musl
```

Or, preferably, a single archive containing:

```text
bin/xsh
bin/xsht
bin/xshi
```

Each artifact needs its own checksum or one checksum for the archive, consistent with the existing release convention.

The workflow should validate all three binaries independently:

```sh
xsh --help
xsht --help
xshi --help
```

For Linux, run static-link checks against each binary rather than only `xsh-multicall`.

### 6. Update the factory boundary

The factory should become simpler, not more coupled.

`evals/Dockerfile.base` already expects direct `.dist/xsh` and `.dist/xsht` files, so the final Dockerfile shape should remain compatible. The factory should not need to know about multicall at all.

Update `factory/controllers/eval.xsh` to validate the built image by running:

```sh
xsh --help
xsht --help
```

inside the target image before worker admission.

This is stronger than inspecting host-side files because it catches:

- wrong architecture;
- shell placeholders;
- broken dynamic loader paths;
- missing executable permissions;
- malformed image copies.

The prior `xsh-multicall` failures demonstrate that this check belongs before paid worker dispatch.

Update the factory tests to:

- use isolated fake product target directories;
- avoid mutating the real `../xsh/target`;
- generate ELF-like fixtures or use a fake image smoke command;
- assert that invalid binaries fail before a worker session is admitted;
- assert that all three direct binaries are staged and smoke-tested.

## Local Pre-Release Smoke Test

### Product checkout

For ordinary coding validation:

```sh
cd ../xsh
cargo build --locked --bin xsh --bin xsht --bin xshi
cargo test --release --locked
```

Use the repository’s prescribed targeted test commands rather than running formatter/autofix commands.

Run direct behavior checks:

```sh
target/debug/xsh --help
target/debug/xsht --help
XSHI_ALLOW_NON_TTY_FOR_TESTS=1 target/debug/xshi --help
```

Also test:

```sh
target/debug/xsh --startup
target/debug/xshi --no-config -c 'print "ok"'
```

### Factory checkout

After staging local binaries into the factory image path:

```sh
cd ../xsh-factory
XSH_MODULE_PATH=. xsht test
xsht check evals/task-grep/evaluator.xsh
```

Then build the local eval image and smoke-test:

```sh
docker run --rm <image> xsh --help
docker run --rm <image> xsht --help
docker run --rm <image> xsh --startup
```

Before any paid factory cycle, assert:

```text
xsh is an ELF executable
xsht is an ELF executable
xshi is an ELF executable
all three execute successfully in the target image
no xsh-multicall reference remains in the build/release path
```

## Performance Evaluation

Measure, do not assume.

Compare the old and new designs on:

- total build wall time;
- incremental rebuild time after changing `xsh` library code;
- total artifact bytes;
- individual binary sizes;
- `xsh` startup time;
- `xsht --help` startup time;
- `xshi --help` startup time;
- cold Docker image startup;
- image size.

Expected tradeoff:

- **Build time:** likely comparable or better if built in one Cargo invocation.
- **Incremental builds:** likely better than three unrelated package builds because Cargo shares the root library graph.
- **Individual binary size:** likely smaller per tool because linker dead-code elimination can remove unrelated entrypoints.
- **Aggregate size:** may be larger than one multicall binary because common code is statically present in multiple files.
- **Runtime reliability:** materially better because there is no `argv[0]` dispatch or multicall packaging dependency.

## Acceptance Criteria Before Release

Do not cut the GitHub release until all of these pass:

1. `cargo metadata` contains no release dependency on `xsh-multicall`.
2. One locked build produces independent `xsh`, `xsht`, and `xshi` binaries.
3. All three are valid target executables.
4. All three pass direct help/startup smoke tests.
5. Linux static-link verification passes for all three.
6. Darwin packaging passes for all three.
7. The release archive/checksums contain the new names.
8. Factory Docker images start both `xsh` and `xsht`.
9. `xsht test` passes in the factory.
10. No paid factory cycle starts when any binary smoke test fails.
11. Build size/timing comparisons are recorded.
12. The old multicall release artifact is removed only after the new artifacts are verified.
