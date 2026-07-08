# Tokio Dependency Notes

XSH does not want an async task runtime as part of the language runtime model.
`docs/ARCHITECTURE.md` and `docs/CHAPTER-15-why-not-xsh.md` both draw that
boundary deliberately: XSH coordinates processes, files, streams, and focused
host APIs rather than exposing an application event loop.

Tokio is not part of XSH's resolved dependency graph. This file records the
choices that keep it that way and the verification commands that should stay
green when archive or network dependencies change.

## Current Direct Uses

- `src/modules/archive/tar.rs` uses `astral_futures_tar` with the crate's
  futures backend.
- `src/modules/archive/zip.rs` uses `astral_async_zip`'s base `futures` I/O API.
- `src/modules/compression.rs` implements futures `AsyncRead` and `AsyncWrite`
  adapters over blocking readers and writers for tar archive code.
- `crates/xsh-net/src/lib.rs` uses a small blocking HTTP/1.1 transport with
  Rustls for HTTPS and an internal per-origin idle connection pool.

## Dependency Reality

Tar handling uses the local `astral-futures-tar` port with
`default-features = false` and `features = ["futures"]`, removing the tar path's
Tokio and `tokio-stream` requirements while keeping the existing archive module
surface.

`astral_async_zip` is configured with only ZIP deflate support and without
`tokio` or `tokio-fs`. XSH reads ZIP files into memory and uses the base
`futures` reader, preserving ordinary stored/deflated ZIP list/extract behavior
without a Tokio archive runtime.

`xsh-net` no longer depends on Hyper, `hyper-util`, `hyper-rustls`, or Tokio.
Its transport keeps the existing language-facing API while making the runtime
boundary explicit:

- DNS and TCP connection setup remain capability-aware.
- HTTP is HTTP/1.1 only.
- HTTPS uses `rustls` directly with either platform verification, a caller
  supplied CA bundle, or explicit verification disabling.
- Request, connect, and read/write timeouts use blocking socket timeouts.
- Redirects, response body limits, upload bodies, atomic downloads, and
  overwrite policy stay in `xsh-net`.
- Idle connection reuse is handled by a small per-origin pool keyed by scheme,
  host, and port.

## Maintenance Direction

The practical goal is to keep XSH's host helpers free of a general async task
runtime while still using focused libraries where they fit.

Watch points:

- Do not re-enable `astral_async_zip`'s `full`, `tokio`, or `tokio-fs`
  features. Add non-deflate ZIP compression features only if XSH intentionally
  supports those ZIP entry methods.
- Do not switch `astral_futures_tar` back to its default Tokio feature set.
- Do not add HTTP client crates that hide a Tokio runtime or pull in
  `tokio-rustls` unless the architectural tradeoff is explicit.
- Keep network behavior covered by tests for redirects, timeouts, TLS verify
  on/off, custom CA files, body limits, download overwrite/atomic behavior,
  upload bodies, and connection reuse.

## Async HTTPS Without Tokio

The implemented transport is not async. That is intentional for now: XSH does
not need a process-wide async task runtime to issue ordinary HTTP requests from
scripts. Blocking sockets with explicit timeouts preserve the language runtime
boundary and avoid introducing an application event loop.

If XSH later needs truly async HTTPS inside `xsh-net`, that should be treated as
a new transport decision rather than a Tokio dependency cleanup. The likely
options are:

| Option | Shape | Tradeoff |
|---|---|---|
| Keep blocking HTTP/1.1 | Maintain the current Rustls transport and idle pool. | Smallest dependency surface; HTTP/1.1 only. |
| Hyper without Tokio | Add custom runtime glue over another reactor plus non-Tokio Rustls. | Best protocol quality, highest implementation cost. |
| `async-h1` plus Rustls | Use `async-io`/`smol` sockets and a smaller HTTP/1.1 stack. | Natural non-Tokio fit, but more client behavior remains ours. |
| `isahc` / libcurl | Delegate HTTP behavior to libcurl. | Mature behavior, but adds a native C dependency and weakens Rustls-specific policy control. |

## Verification Commands

Useful dependency checks:

```sh
cargo tree -i tokio
cargo tree -i tokio-stream
cargo tree -i hyper
cargo tree -i hyper-rustls
cargo tree -i futures-lite
cargo tree -i astral-futures-tar
cargo tree -e features -i tokio
```

Useful behavior checks after archive or network dependency cleanup:

```sh
cargo test --test runtime archive
cargo test --test runtime module
cargo build
```
