# XSH-111: Review of the Graviola primitives XSH uses

## Scope and revision

- 2026-11-20, an independent expert-tier agent review outside the
  implementation lanes. The owner ruled that such a review satisfies XSH-111's
  Graviola condition.
- Graviola 0.4.1, crates.io checksum `e8596c4f…` (as in
  `crates/oxish-client/vendor/ORIGIN.md`), plus ssh-build
  `patches/graviola/0001-ssh-chacha20-poly1305.patch`. The pristine crate with
  the patch applied is byte-identical to the vendored `src/`.
- Covered: every Graviola primitive the SSH client reaches. These are X25519,
  ML-KEM-768 key generation and decapsulation, the `mlkem768x25519-sha256`
  combine, SHA-256 (exchange hash, KDF, fingerprints), AES-128-GCM, SSH
  ChaCha20-Poly1305, Ed25519 host-key verification and user-key signing, PKCS#8
  key parsing and `getrandom`. The review also covered what XSH reaches outside
  SSH through the same crate: TLS through rustls-graviola,
  `crypto.ed25519_verify` and the SPKI digest in `crates/xsh-net/src/trust.rs`.
  Call paths were followed into the high, mid and low layers on x86-64 and
  aarch64.
- Not covered:
  - a constant-time (ctgrind) run;
  - Wycheproof and CAVP vectors, which the vendored crate omits;
  - a line review of the s2n-bignum assembly, which relies on its formal proofs;
  - RSA and ECDSA signing;
  - the rustls and SSH protocol layers. The
    [client review](XSH-111-ssh-client-review.md) covers the SSH protocol layer.

## Verdict

No Critical, High or Medium finding, and nothing in the reached primitives
blocks lifting the client's experimental label. Seven findings are Low or Info.

| # | Finding | Severity | Status |
|---|---|---|---|
| G-1 | `oxish-graviola`'s `Sha256Context::finish` moved the context out of its box, so the freed box kept the pre-finish state: the hybrid exchange secrets, fragments of K, and KDF chaining state. This contradicted the wipe claim in `docs/ssh.md`. Only a local memory disclosure could read it. | Low | fixed in ssh-build `789d4cd`; `hash_wipe.rs` probes finish and fork |
| G-2 | The AVX-512 AES-GCM path leaves round keys and data in zmm16 to zmm31; `leave_cpu_state` clears only with `vzeroall`. Graviola 0.4.5 still does this. | Low | open, [XSH-141](../../TODO.md) |
| G-3 | The CPU gate omitted BMI2 (`mulx` in the curve and bignum code) and POPCNT (ML-KEM rejection sampling). XSH's TLS, `crypto` module and SPKI digest never call the gate. | Low | client and server gates fixed in ssh-build `789d4cd`; XSH's other entry points are [XSH-141](../../TODO.md) |
| G-4 | TLS only: RSA verification accepts e=1, even exponents and short signatures. A certificate for such a key is still needed, from a trusted or user-added CA. | Low | open, [XSH-142](../../TODO.md): fixed in Graviola 0.4.3 |
| G-5 | TLS only: P-256 and P-384 accept public coordinates of at least p. This gives a point a second encoding but enables no forgery. | Info | open, [XSH-142](../../TODO.md) |
| G-6 | Ed25519 verification accepts small-order public keys, as OpenSSH does. A host key must already be trusted, so SSH is unaffected. `crypto.ed25519_verify` callers that take keys from untrusted sources should know. | Info | open, [XSH-141](../../TODO.md) |
| G-7 | Some temporaries are not wiped: the Poly1305 key (zeroed by a plain loop), Poly1305 state, per-call ChaCha20 key copies, the ML-KEM seed, SHA-3 contexts, and the `Blockwise` and padding copies. | Low/Info | open, [XSH-141](../../TODO.md) |

## Cleared

- **SSH ChaCha20-Poly1305 patch.**
  - Key split: K_2 is `key[..32]` and K_1 is `key[32..]`.
  - Nonce: the sequence number, 64-bit big-endian.
  - Counters: the Poly1305 key comes from counter 0 and the payload starts at counter 1.
  - Tag: computed over the encrypted length and the ciphertext, and compared in constant time before decryption.
  - The decrypted length is bounded at 64 KiB by the caller.
  - 440 OpenSSH-generated vectors pass on x86-64, and on macOS arm64 since 2026-11-20.
- **AES-GCM.**
  - RFC 5647 invocation counter, with an overflow check.
  - The full 16-byte tag is compared in constant time, and the plaintext is zero-filled on failure.
  - Key and GHASH tables are wiped on drop.
  - Hardware AES and carry-less multiply only; no lookup tables.
- **X25519.**
  - Clamping is done in the formally verified s2n-bignum code.
  - An all-zero shared secret is refused in constant time; the five small-order points are refused.
  - Private keys and shared secrets are wiped.
- **ML-KEM-768.**
  - Implicit rejection is constant time.
  - The modulus of the encapsulation key is checked, and the ciphertext length is enforced by its type.
  - The hybrid secret is SHA-256(K_PQ ‖ K_CL), encoded as a string, as the draft and OpenSSH specify.
  - Compression uses no division.
- **Ed25519 and ECDSA.**
  - Ed25519: S < L is required; A and R are decoded canonically; signing is deterministic per RFC 8032, and the key is wiped.
  - ECDSA verification: r and s are range-checked, and the point at infinity is refused.
- **RNG.** `getrandom(2)` on Linux and `getentropy(2)` on macOS. Failures are typed errors at every layer.
- **unsafe and assembly.**
  - The mid and high layers deny unsafe code; `oxish-graviola` forbids it.
  - Assembly wrappers take fixed-size arrays and use unaligned loads.
  - The SSH path calls `cpu::require` before any Graviola call.
  - On macOS arm64, the baseline target already includes the AES, SHA-2 and NEON features.
- **Advisories.** RustSec and the upstream repository list none for Graviola as of 2026-11-20.

## Reproduce

```sh
cargo test -p oxish-graviola                       # 440 OpenSSH ChaCha20-Poly1305 vectors and the glue
cargo test -p oxish-client --test hash_wipe        # SHA-256 contexts wiped on drop, finish and fork
cargo test -p oxish-client --lib cpu               # the CPU gate
```
