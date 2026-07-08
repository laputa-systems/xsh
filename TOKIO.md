# Tokio Dependency Notes

XSH does not want an async task runtime as part of the language runtime model.
`docs/ARCHITECTURE.md` and `docs/CHAPTER-15-why-not-xsh.md` both draw that
boundary deliberately: XSH coordinates processes, files, streams, and focused
host APIs rather than exposing an application event loop.

Tokio is still present today because several selected host-library APIs are
Tokio-shaped. This file tracks what is actually required, what can be narrowed,
and what is out of scope unless the dependency choice changes.

## Current Direct Uses

- `src/modules/archive/mod.rs` creates a Tokio runtime for archive operations.
- `src/modules/archive/tar.rs` uses `tokio_tar`, `tokio::io`, and
  `tokio_stream::StreamExt`.
- `src/modules/archive/zip.rs` uses `async_zip::tokio::read::fs::ZipFileReader`
  and `tokio::task::JoinSet`.
- `src/modules/compression.rs` implements Tokio `AsyncRead` and `AsyncWrite`
  adapters over blocking readers and writers for archive code.
- `crates/xsh-net/src/lib.rs` stores a Tokio runtime in `NetAgent` and uses
  Hyper through `hyper-util`'s Tokio executor and I/O adapter.

## Dependency Reality

`astral-tokio-tar` does not support an alternative async runtime. It directly
depends on `tokio` and `tokio-stream`, and its public API is Tokio-oriented.
Replacing it with a synchronous or non-Tokio tar implementation is a larger
archive dependency decision and is not part of the current cleanup scope.

`astral_async_zip` does have a non-Tokio base API built on `futures` I/O traits.
Our current `features = ["full"]` enables `tokio-fs`, and our code uses the
Tokio filesystem reader. This can likely be narrowed without replacing the
crate.

`xsh-net` currently depends on Tokio structurally:

- `hyper` is mostly runtime-agnostic, but our client stack uses `hyper-util`.
- `hyper-util`'s client features enable Tokio networking, synchronization,
  runtime, and timers.
- `hyper-rustls` depends on `hyper-util` with Tokio support and on
  `tokio-rustls`.
- `tokio-rustls` is Tokio-specific.

Keeping `xsh-net` on the current Hyper/Rustls stack means keeping Tokio.
Replacing Tokio there would mean choosing or building a different HTTP/TLS
transport, not simply swapping `tokio` for `futures-lite`.

## Cleanup Direction

The practical goal is to minimize XSH's direct Tokio surface while accepting the
Tokio dependency where selected crates require it.

Near-term cleanup candidates:

1. Remove the direct `tokio-stream` dependency from `Cargo.toml` if tar entry
   iteration can use an already-present stream extension trait. This will not
   remove `tokio-stream` from the dependency graph while `astral-tokio-tar`
   remains, but it avoids declaring it as an XSH dependency.
2. Move ZIP handling away from `async_zip`'s `tokio-fs` path and onto the base
   `futures` I/O API. This should remove the `astral_async_zip -> tokio` and
   `astral_async_zip -> tokio-util` edges if feature selection is kept explicit.
3. Keep `tokio` as a direct dependency as long as archive tar support and
   `xsh-net` use their current libraries.

Out of scope for this cleanup:

- Replacing `astral-tokio-tar`.
- Removing or disabling `xsh-net`.
- Rewriting HTTP/TLS transport away from Hyper, `hyper-rustls`, and
  `tokio-rustls`.

## Async HTTPS Without Tokio

Removing Tokio from `xsh-net` does not mean removing async HTTPS. It means
choosing a different async I/O runtime and HTTP/TLS stack. `futures-lite` or
`futures` can provide traits and utilities, but sockets, timers, spawning, and
driving readiness still need a runtime such as `smol`/`async-io`, `async-std`,
or a custom reactor.

Plausible approaches:

| Option | Shape | Pros | Cons |
|---|---|---|---|
| Hyper without Tokio | Keep `hyper`, replace `hyper-util`'s Tokio adapter and `tokio-rustls` with custom runtime glue over `smol`/`async-io` plus non-Tokio Rustls. | Best HTTP protocol quality; keeps low-level control; preserves current request/response model. | Highest engineering cost; requires executor, timer, I/O, TLS, connector, and likely pool work. |
| `async-h1` plus Rustls | Use `async-h1` for HTTP/1.1, `async-io`/`smol` for sockets, and `futures-rustls` or `async-tls` for HTTPS. | Smaller stack; naturally non-Tokio; easy to reason about. | HTTP/1.1 only; more client behavior becomes ours, including pooling and connection reuse edge cases. |
| `surf` or `http-client` h1 backend | Use the higher-level `async-h1`/Rustls backend exposed by those crates. | Fastest experiment for non-Tokio async HTTPS. | Adds an abstraction layer that may fight XSH's capability-aware connector, TLS policy, and error classification. |
| `isahc` / libcurl | Use libcurl through a runtime-agnostic async Rust API. | Mature HTTP behavior, HTTP/2 support, redirects, timeouts, pooling, and TLS handled by libcurl. | Native C dependency; harder to preserve capability-aware DNS/connect behavior and Rustls-specific TLS policy. |

The best first spike is Hyper without Tokio. Hyper itself exposes runtime traits
for executors, timers, and I/O transports; the Tokio coupling comes from our
current `hyper-util`, `hyper-rustls`, and `tokio-rustls` path. A non-Tokio Hyper
transport keeps us closest to the current design while preserving control over:

- capability-aware DNS and TCP connect behavior;
- custom CA files and TLS verification mode;
- redirects and request timeout semantics;
- body size limits;
- atomic downloads and overwrite policy;
- upload body handling;
- XSH-specific error kind classification.

The likely hard part is connection pooling. Hyper's convenient pooled client is
currently reached through `hyper-util`'s Tokio-oriented client stack. A
non-Tokio Hyper transport may need to use lower-level connection handshakes and
own a small HTTP/1 connection pool inside `xsh-net`.

Suggested sequence:

1. Add an internal `HttpTransport` boundary in `xsh-net`, with the current Tokio
   Hyper implementation behind it.
2. Lock current behavior with focused tests for redirects, timeouts, TLS verify
   on/off, custom CA files, body limits, download overwrite/atomic behavior, and
   upload bodies.
3. Prototype a `smol`/`async-io` plus Hyper HTTP/1 plus non-Tokio Rustls
   transport.
4. If Hyper runtime glue becomes too large or fragile, compare the prototype
   against an `async-h1` transport before committing to the larger rewrite.

## Verification Commands

Useful dependency checks:

```sh
cargo tree -i tokio
cargo tree -i tokio-stream
cargo tree -i futures-lite
cargo tree -e features -i tokio
```

Useful behavior checks after archive dependency cleanup:

```sh
cargo test --test runtime archive
cargo test --test runtime module
cargo build
```
