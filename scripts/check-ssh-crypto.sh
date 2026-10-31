#!/bin/sh
# Fail if xsh-ssh has a crypto provider other than the vendored Graviola, or if
# Graviola does not resolve to the pinned patched copy.
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
manifest=$root_dir/Cargo.toml
lock=$root_dir/Cargo.lock
vendor=$root_dir/crates/xsh-ssh/vendor
origin=$vendor/ORIGIN.md

providers='aws-lc|aws-lc-rs|aws-lc-sys|aws-lc-fips-sys|openssl|openssl-sys|openssl-src|boring|boring-sys|boringssl|libressl|ring|rustls|rustls-.*|native-tls|mbedtls|mbedtls-sys|wolfssl|wolfssl-sys|nss|nss-sys|gnutls|security-framework|security-framework-sys|schannel'
primitives='sha1|sha2|sha3|hmac|hkdf|aes|aes-gcm|chacha20|chacha20poly1305|poly1305|curve25519-dalek|ed25519-dalek|x25519-dalek|p256|p384|p521|ecdsa|rsa|ml-kem|pqcrypto.*|libcrux.*|subtle'

fail() { echo "check-ssh-crypto: $*" >&2; exit 1; }

scan_names() {
    pattern=$1
    awk -v re="^($pattern)\$" '$1 ~ re { print $1 }' | sort -u
}

for required in "$origin" "$vendor/LICENSE-APACHE" "$vendor/LICENSE-MIT" "$vendor/LICENSE-GRAVIOLA"; do
    [ -f "$required" ] || fail "missing vendored license or origin manifest: $required"
done

grep -q 'bc2a9fb418ac48c9e00b5138fa800e4a7c724315' "$origin" ||
    fail "origin manifest does not pin the ssh-build revision"

command -v cargo >/dev/null || fail "cargo not found"
cd "$root_dir"

graph=$(cargo tree --locked --manifest-path "$manifest" -p xsh-ssh --edges normal,build --target all --prefix none) ||
    fail "cannot resolve the xsh-ssh dependency graph"

hits=$(echo "$graph" | sed 's/ v[0-9].*$//' | scan_names "$providers|$primitives") || true
[ -z "$hits" ] || fail "prohibited crates in the xsh-ssh release graph: $(echo $hits)"

echo "$graph" | grep -q '^graviola v[^ ]* (.*crates/xsh-ssh/vendor/graviola)' ||
    fail "graviola does not resolve to the vendored patched copy"

stanza=$(sed -n '/^name = "graviola"$/,/^$/p' "$lock")
echo "$stanza" | grep -q '^version = "0\.4\.1"$' || fail "Cargo.lock does not pin graviola 0.4.1"
if echo "$stanza" | grep -q '^source = '; then
    fail "Cargo.lock still resolves graviola from a registry"
fi

echo "ssh crypto graph ok: vendored graviola 0.4.1 is the only provider"
