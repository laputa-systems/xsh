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

```sh
cargo dev build
cargo clippy --locked
cargo run --locked -p xsht --bin xsht -- lint
cargo dev test linux
```

Linux verification runs the default native suite and the Rust and privileged
Linux tests inside the pinned image. The first run downloads that image. On a
loaded x86-64 host limited to eight CPUs, a cold Ubuntu 24.04 debug build took
93 seconds after setup. The nested Docker image pull took about 3 minutes 18
seconds using the `vfs` storage driver. Those measurements exclude package
installation and toolchain download. A normal Docker host can use its default
storage driver.

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
