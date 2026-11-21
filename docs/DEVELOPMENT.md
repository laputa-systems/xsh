# Set up development on Ubuntu

From a clone on Ubuntu 24.04, install the development tools and locked crates:

```sh
scripts/setup-dev
. "$HOME/.cargo/env"
```

Run the script as root, or as a user who can install packages with `sudo` or
`su`. For Docker access as a non-root user, start a new login session after the
script adds the user to the `docker` group. Start the Docker service if it is
not running. Docker group membership grants root-equivalent access.

The script installs these packages:

| Package | Consumer |
| --- | --- |
| build-essential | Rust native dependencies and build scripts |
| ca-certificates, curl | rustup and HTTPS downloads |
| docker.io | Linux verification image |
| git | Source checkout and verification |
| git-lfs | Large-file-enabled checkouts |
| musl-tools | Host C compiler for the product target's C boundary |
| pkg-config | Native dependency discovery |
| python3 | Development and test helpers |
| ripgrep | Repository checks |
| sudo | Package administration |
| zip | Native archive tests |

The pinned toolchain is selected by `rust-toolchain.toml`. Rustup installs its
components and Cargo fetches the lockfile through the configured mirror.
Ubuntu 24.04 supplies Python 3.12. Performance helpers require Python 3.11 or
newer.

## Build and verify

The repository owns its verification through `cargo dev`; [Testing](TESTING.md)
describes the cases and profiles behind each command.

```sh
cargo dev build
cargo dev check
cargo dev gate --full
cargo dev test linux
```

`cargo dev build` compiles the workspace for the host target. `cargo dev check`
is the non-mutating check gate: it builds the release products, runs `cargo
fmt --check`, clippy with `-D warnings`, `xsht check`, `xsht fmt --check`,
`xsht lint`, the core contracts, the generated-documentation check, the
deprecated-import contract, and `git diff --check`. `cargo dev gate --full` is
the full local integration gate: every declared Rust harness, the native corpus,
the documentation owner, the repository checks and the structural ratchets.
`cargo dev test linux` runs the Rust and native Linux suite, including the
privileged tests, inside the image pinned by `Dockerfile.test` and
`dev/images.lock`; it needs a working Docker daemon and downloads that image on
first use.

Use `cargo dev gate PATH...` for the narrowest check owned by changed paths.
`cargo dev gate --plan PATH...` lists the selected cases without building or
starting Docker.

Linux verification runs the default native suite and the Rust and privileged
Linux tests inside the pinned image. The first run downloads that image.

## Cold capsule costs

A cold `ubuntu:24.04` x86-64 container with eight CPUs, an inner Docker daemon
on the `vfs` storage driver, and a fresh clone of this repository at the lane
commit measured the following. The host was shared and loaded, so these are
wall times on a busy host, not best-case figures.

| Step | Wall time |
| --- | --- |
| `scripts/setup-dev` as root | 44 s |
| `cargo dev build` | 117 s |
| pinned image pull, nested `vf` daemon | 314 s |

`cargo dev test linux` then builds the Rust and native suites inside the pinned
image. See [Testing](TESTING.md#linux) for what it runs.

The host builds the Linux musl product target, so the host needs that target's
standard library and a C toolchain for it; `scripts/setup-dev` installs both.
On an x86-64 host the build also links through the toolchain's `rust-lld`,
which the pinned toolchain supplies.

Verification runs Docker containers. Where the host is itself a container,
start a daemon inside it and run the capsule privileged. A nested host that
cannot use the overlay storage driver fails to extract an image layer with
`failed to convert whiteout file ...: operation not permitted`; start the inner
daemon with `dockerd --storage-driver=vfs` instead. This is a nested-Docker
detail only; a normal host uses its default storage driver.

## Optional tools

Lean 4.35.0 is required for `formal/` only. Install Lean using the
[Lean installation guide](https://lean-lang.org/install/) and use the pinned
version in `formal/lean-toolchain`. See [formal/README.md](../formal/README.md)
for the project commands. The pinned test image provides
rust-lld. For optional x86-64 container acceleration, install a musl-compatible
[mold release](https://github.com/rui314/mold/releases) at `/usr/local/bin/mold`
and follow the [kache installation instructions](https://github.com/joshuarli/kache#install)
to install its binaries in `~/.cargo/bin`. Both must be present for the
container adapter to use them. Benchmarks, release packaging, cross builds, and macOS
verification have additional requirements in [Testing](TESTING.md) and
[Release engineering](RELENG.md).
