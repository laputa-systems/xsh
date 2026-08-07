# `libxsh`

`libxsh` is the static Rust library produced by the root `xsh` package. It is
the shared implementation boundary for the `xsh`, `xshi`, and `xsht` products;
it is not a dynamic library, an ABI, or the XSH language API.

## Current state

The library exposes a curated façade rather than its implementation tree:

```text
xsh::frontend   source, syntax, loading, and semantic checking
xsh::diagnostic diagnostics and renderable diagnostic data
xsh::execution  script execution, evaluator/session, and runtime values
xsh::process    process lifecycle, redirection, cancellation, and signals
xsh::trace      structured trace events and traceback data
xsh::host       narrow reusable host adapters
```

`xshi` and `xsht` use these façade paths. Their representation-heavy frontend,
evaluator, value, and process types remain first-party tooling APIs; ordinary
script execution, source/diagnostic data, and structured traces are the initial
supported library tier. Implementation namespaces such as `xsh::runtime`,
`xsh::syntax`, `xsh::sema`, and `xsh::runner` are private owners and are not
canonical consumer paths.

Product ownership is package-local:

```text
xsh   package  libxsh library and xsh binary
xshi  package  xshi binary, depending on xsh
xsht  package  xsht binary, depending on xsh
```

The three products can be built in one Cargo invocation. The root integration
harness resolves the package-owned `xshi` and `xsht` binaries from the active
Cargo profile. `xsh-registry` remains the owner of XSH standard-module
signatures, records, documentation, examples, and runtime operation IDs.

```sh
cargo build -p xsh -p xshi -p xsht --bin xsh --bin xshi --bin xsht
```

Trace data belongs to `libxsh`; CLI formatting, JSONL/terminal presentation,
coverage presentation, and syscall reporting belong to `xsht`. The `xsh` CLI
entrypoint is a binary concern and is not part of the library API.

The façade boundary is guarded by API smoke tests, package and integration
tests, `cargo metadata`, and `scripts/check-libxsh-imports.sh`, which rejects
new first-party imports from deprecated implementation paths.

## Future direction

Keep the root static library boundary while the façade and its contracts settle.
Do not add a `cdylib`/`dylib`: a dynamic Rust ABI would add loader, packaging,
platform, and static-musl complexity without a current consumer benefit.

A separate `crates/libxsh` or `crates/xsh-core` package is worth considering
only if it creates a concrete boundary benefit, such as an independent
consumer, reduced feature coupling, or a smaller stable dependency set. Any
extraction should preserve the façade paths or provide an explicit migration.
Until that evidence appears, the existing root library is the appropriate
ownership boundary.
