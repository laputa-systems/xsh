# Vendored SSH substrate

Source: `https://github.com/laputa-systems/ssh-build` at
`bc2a9fb418ac48c9e00b5138fa800e4a7c724315`.

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

The OxiSH server, `oxish-aws-lc` provider, fuzz targets, and the OxiSH CLI are
not vendored. No sibling checkout is needed to build or test xsh-ssh.

The client needs the packet body and the key-derivation inputs, which the
server-only API kept private. `oxish-proto` therefore makes four visibility-only
changes: `IncomingPacket::payload()`, public `KeyDerivation` fields, public
`KeySourceSide` direction constructors, and a public `encode_mpint`. No protocol
or cryptographic behavior changes.

## Manifest normalization

Source files are byte-for-byte from ssh-build. The two OxiSH manifests were
made standalone so this workspace does not need ssh-build's workspace fields:
`edition` and `rust-version` replace their `.workspace` forms, workspace
dependency versions are spelled out, the inherited lint table is replaced by an
empty `[lints]`, and `oxish-graviola` names its sibling `graviola` by path. The
Graviola manifest drops its non-root resolver and inferable readme fields. The
workspace also patches crates.io `graviola` to this same path so one copy serves
XSH and xsh-ssh.

## Updating

1. Check out the new ssh-build revision and update the revision above.
2. Run `sh scripts/vendor-oxish.sh verify` and
   `sh scripts/vendor-graviola.sh verify` in that checkout.
3. Replace the two OxiSH directories and `graviola` from `vendor/` and rerun
   the manifest normalization.
4. Run `cargo build --release --target aarch64-apple-darwin -p xsh-ssh` and
   `scripts/check-ssh-crypto.sh`.
