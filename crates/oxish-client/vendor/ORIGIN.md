# Vendored SSH client and substrate

Source: `https://github.com/laputa-systems/ssh-build`.

Revision: `ba4e5e269b1737b32cf7cdaa08e739ec06712cf5`

`crates/oxish-client` is ssh-build's `client/` crate (`oxish-client`) at that
revision, with the files the XSH library does not use left out: `src/bin`, the
`cli`, `copy`, `progress`, `sftp`, `subsystem` and `terminal` modules (and their
`pub mod` lines in `src/lib.rs`), and the `sftp`, `sftp_peer`, `interactive` and
`keygen` test files. `session.rs` stays: the exec path in `stream.rs` and
`client.rs` runs on it. The excluded modules are the only users of `libc` and
the `termios` feature of `rustix`, so the manifest drops both. Every other
file under `src`, `tests` and the three directories below is byte-for-byte
ssh-build; `scripts/vendor-ssh-client.sh verify` checks that and
`update` regenerates it. `fixture/` is XSH's own copy of the OpenSSH fixture,
without the tmux and SFTP additions the excluded tests need.

Included:

- `oxish-proto` 0.2.0 from ssh-build `vendor/oxish/oxish-proto`, which is
  upstream `https://github.com/djc/oxish` at
  `d81b20b7ea494b6aa4cadea179699adbae9bb374` plus ssh-build patches
  `0002-oxish-proto.patch` and the shared workspace manifest. License:
  Apache-2.0 OR MIT (`LICENSE-APACHE`, `LICENSE-MIT`).
- `oxish-graviola` 0.2.0 from ssh-build `vendor/oxish/oxish-graviola`, which is
  the same upstream revision plus `0003-oxish-graviola.patch`. License:
  Apache-2.0 OR MIT.
- `graviola` 0.4.1, crates.io checksum
  `e8596c4fa98466aae2fcf4c72a665bc0e021c0aaab1e47d82044d3dc3e309a76`, plus
  ssh-build `patches/graviola/0001-ssh-chacha20-poly1305.patch`. License:
  Apache-2.0 OR ISC OR MIT-0 (`LICENSE-GRAVIOLA`).

The OxiSH server, `oxish-aws-lc` provider and fuzz targets are not vendored.
No sibling checkout is needed to build or test `oxish-client`.

The client's visibility changes to `oxish-proto` and its private-key
buffer wiping in `host_keys.rs` are in ssh-build, so no vendored source file
differs from it.

## Manifest normalization

Source files are byte-for-byte from ssh-build. The two OxiSH manifests were
made standalone so this workspace does not need ssh-build's workspace fields:
`edition` and `rust-version` replace their `.workspace` forms, workspace
dependency versions are spelled out, the inherited lint table is replaced by
`[lints.clippy] all = "allow"` so the repository clippy gate (`-D warnings`,
root `clippy.toml`) does not apply to vendored source, and `oxish-graviola` names its sibling `graviola` by path. The
Graviola manifest drops its non-root resolver and inferable readme fields. The
workspace also patches crates.io `graviola` to this same path so one copy serves
XSH and `oxish-client`.

## Updating

1. Set the `Revision` line above to the new ssh-build commit.
2. Run `SSH_BUILD_DIR=PATH sh scripts/vendor-ssh-client.sh update` (omit
   `SSH_BUILD_DIR` to fetch into `.work/sources/ssh-build`), then `verify`. In
   the ssh-build checkout, run `sh scripts/vendor-oxish.sh verify` and
   `sh scripts/vendor-graviola.sh verify`.
3. Reapply the manifest normalization if a vendored `Cargo.toml` changed
   upstream, and add any dependency the new client sources need to
   `crates/oxish-client/Cargo.toml`.
4. Run `scripts/check-ssh-crypto.sh` and
   `cargo build --release --target aarch64-apple-darwin -p oxish-client`.

A fix to the client belongs in ssh-build first, then arrives here by step 2.

The Graviola manifest also gains `keywords = ["cryptography"]`, which the
repository `clippy::cargo` lint requires of every workspace package. The
supplemental clippy gate excludes `graviola` from `--all-features` because its
`__ctgrind` feature builds `crabgrind` with bindgen and libclang, which a static
musl build script cannot load.
