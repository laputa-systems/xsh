# Implementation proofs

This page records proofs about selected production Rust functions. They are an
experiment inside the [semantic soundness campaign](../xsh-lean.md) under
`IMPL-REFINE`, which stays planned. Universal verification of Rust, LLVM,
Linux, external commands and native libraries is outside the campaign's
completion criteria. [`coverage.json`](coverage.json) records each accepted
claim in `implementation_proofs` (schema 4, evidence class
`EXTRACTED_REFINEMENT`).

An `EXTRACTED_REFINEMENT` record is a kernel-checked Lean theorem relating a
mechanically extracted model of identified production source to XSH's
independent definitions. It holds conditionally on the named extraction
pipeline and library models. It is not `SPEC_PROVED` (the subject is a model of
Rust, not the independent semantics), not `RUST_CONFORMANCE` (no named Rust
test produced it) and not a proof of a shipped binary. The inventory checker
fails a `checked` record when a production source, copied declaration,
translation, library patch or toolchain pin differs from the recorded
identity, and when any harness file or formal module that the harness imports
(`proof_sources`: `Core/Arith.lean`, `Values/DurationOps.lean`,
`Values/Scalars.lean`, `Foundation.lean` and every file under
`dev/verify/aeneas`) changes or appears. Such a record must be regenerated and
rechecked, or marked `stale`, which keeps it out of the evidence.

## Methods adopted

Anthropic's [formal verification report](https://www.anthropic.com/research/formal-verification)
describes the program: Claude-written Rocq proofs that OSS-Fuzz entry points,
compiled to RISC-V, exit normally without memory-safety errors or failed
assertions on all inputs under 64 MB, with patches made during proving
penalized and audited so that a theorem is not made provable by rewriting the
program. Theorem's [OSS Verify proofs](https://github.com/theorem-labs/oss-verify-proofs)
(read at `90e648c1`, release 2026.11.2) state Rocq 9.2.0 theorems about
compiled static riscv64 binaries in LPS, a model of a Linux process running a
RISC-V program. An agreement statement says that on every input within a
memory budget neither the plain nor the sanitizer build reaches an error and
both have the same set of observable traces; a halts statement says every run
ends with an allowed status. A harness assertion compiled into the binary turns
a halts proof into a proof of that property. The trusted base is the Rocq
kernel with `vm_compute`, an axiom allowlist, LPS, the ELF-to-image tools and
the `oft check` wrapper; the compiler and the relation to source are not
trusted and nothing is claimed about them. Binaries are built with clang 20.1.8
against musl (Rust: nightly-2025-07-16 with a sealed, rebuilt standard
library and `panic = "abort"`). Enrollment
([oss-verify](https://github.com/theorem-labs/oss-verify), read at `dc8238d0`)
is a maintainer pull request naming a public repository; the proofs are
written by Theorem's proving systems, and one check needs Docker and up to 20
GiB of memory.

[Aeneas](https://github.com/AeneasVerif/aeneas) translates Charon's LLBC of a
Rust crate into pure Lean functions over a library of Rust models. Its
nightly-2026.11.09-5a392f4 release is the last on Lean 4.35.0 and bundles
Charon 0.1.315 at its pinned commit `454586e9`.

This harness adopts four methods: the subject is identified bytes, the
statement is fixed before the proof, the checker audits every assumption
against an allowlist, and an assertion's failure path must be present in the
checked artifact. It uses Aeneas for the proofs because the theorem then
relates the production function to the independent Lean definitions over its
full domain.

## Claims

### `checked_int_binary`

- **Function and callers.** `src/arith.rs::checked_int_binary`. Constant
  preparation (`src/sema/constants.rs::fold_constant_binary`) folds
  `Some(Ok)` and declines `Some(Err)`; a `const` initializer then reports
  `invalid constant operation or arithmetic overflow`. Execution
  (`src/runtime/eval/lowered_ops.rs::checked_int_binary`) expects `Some` and
  reports the error's `division-by-zero` or `integer-overflow` diagnostic at
  the operator span; `lowered_binary_value` and the typed scalar
  `ApplyIntBinary` step call it.
- **Property.** `Xsh.Core.Arith.checked` for `+ - * / %`: exact sum,
  difference and product, `Int.tdiv` and `Int.tmod`, a zero divisor of `/` or
  `%` is `divisionByZero`, and a result whose witness (the quotient for `%`)
  is outside signed 64 bits is `overflow`. Every other operator returns `None`.
- **Domain.** All 18 `BinaryOp` constructors and all `i64` pairs.
- **Termination.** Claimed: the extracted function returns `ok`.
- **Connection.** The slice crate `dev/verify/aeneas/slice` compiles
  `src/arith.rs` verbatim and a copy of the `BinaryOp` declaration that its
  build script requires to equal `src/syntax/node.rs`. Charon and Aeneas
  translate it to `dev/verify/aeneas/lean/XshImpl/XshArithSlice.lean`.

### `checked_duration_binary`

- **Function and callers.** `src/duration.rs::checked_duration_binary`.
  Constant preparation folds `Ok` and declines `Err`; `lowered_binary_value`
  maps `Duration` and `Int` operands and reports the error's diagnostic.
- **Property.** The SPEC 4.3 table over `Xsh.Values.DurationOps`:
  `Duration ± Duration`, `Duration * Int` and `Int * Duration`,
  `Duration / Int` (truncating) and `Duration / Duration` (an `Int` count);
  every other operator or dimension pair is `InvalidDimensions`. The six
  error identities are distinct: overflow, underflow, negative factor,
  nonpositive divisor (zero and negative `Int`, zero `Duration`), interval
  count overflow and invalid dimensions.
- **Domain.** All 18 operators, all `u64` milliseconds and all `i64` counts in
  either position.
- **Connection.** The same slice and translation. The translation includes
  core's bodies of `TryFrom<i64> for u64` and `TryFrom<u64> for i64` as the
  extraction nightly-2026-09-17 defines them, not the production nightly's;
  the slice witness tests run the production toolchain's conversions on the
  same cases.

## Accepted theorems

```lean
theorem XshImpl.Arith.checked_int_binary_refines
    (op : syntax.node.BinaryOp) (left right : Std.I64) :
    ∃ out, arith.checked_int_binary op left right = ok out ∧
      decode out = expected op left.val right.val

theorem XshImpl.Duration.checked_duration_binary_refines
    (op : syntax.node.BinaryOp) (left right : duration.DurationOperand) :
    ∃ out, duration.checked_duration_binary op left right = ok out ∧
      decode out = expected op (operand left) (operand right)
```

`expected` reads only the independent definitions. For Int
([`ArithSpec.lean`](../dev/verify/aeneas/lean/XshImpl/ArithSpec.lean)) it
maps `+ - * / %` to `Xsh.Core.Arith.checked` and every other operator to
`none`. For Duration
([`DurationSpec.lean`](../dev/verify/aeneas/lean/XshImpl/DurationSpec.lean))
the dimension table is written in the harness: it dispatches each admitted
operator and dimension pair to `Xsh.Values.DurationOps` and every other pair
to invalid dimensions, matching the SPEC 4.3 table (`docs/templates/SPEC.md`,
rendered at `docs/SPEC.md` lines 879–883).
`decode_injective` holds for both decoders, so each extracted result, including
its error constructor, is the unique one with the specified meaning. The
theorems depend only on `propext`, `Quot.sound` and `Classical.choice`.

**Trust boundary.** The theorems are about the Aeneas translation of the
identified source. They rely on rustc nightly-2026-09-17 (the extraction
compiler, not the production nightly-2026-11-09) producing MIR with the
production meaning, on core's `TryFrom` bodies at that nightly matching the
production ones, on Charon and Aeneas preserving it, on the audited Aeneas
library models, and on the Lean 4.35.0 kernel. They say nothing about LLVM,
the linked binary or the host.

## Library audit

The translation leaves no axiom, opaque item or noncomputable section;
`extract.sh` refuses one. It calls these library models: `I64`
`checked_add`, `checked_sub`, `checked_mul`, `checked_div` and `checked_rem`;
`U64` `checked_add`, `checked_sub` and `checked_mul`; unsigned `/`
(`UScalar.div`); `Option::ok_or`; `Result::map_err`; `Try::branch` and
`FromResidual` for `?`; and the `as` casts `IScalar.hcast` and
`UScalar.hcast`.
The proofs use the library's specification lemmas, which the kernel checks, so
the trust is in each model definition matching Rust; each was read against
the Rust documentation. One was wrong: upstream `checked_rem` for signed integers is
`Option.ofResult (IScalar.rem x y)`, and `IScalar.rem` fails only on a zero
divisor, so the model answers `Some(0)` for `i64::MIN.checked_rem(-1)`.
Rust returns `None`, which the slice witness test checks with the production
compiler. The upstream `%` operator model has the same gap at `MIN % -1`,
where Rust panics; the translation does not use it.
[`patches/signed-checked-rem.patch`](../dev/verify/aeneas/patches/signed-checked-rem.patch)
returns `None` in that case and restates the model's own lemmas;
`check.sh` applies it to the pinned library. XSH-156 tracks the fork and
upstream draft. Aeneas models
`core::num::error::TryFromIntError` as `Unit`, while Charon translates core's
constructor with its `IntErrorKind` payload; `extract.sh` erases exactly those
two payloads to `()`, and the production code discards them in `map_err`.

The generated module sets `maxHeartbeats 1000000` and `maxRecDepth 2048`, the
Aeneas defaults; the hand-written modules set no option.

## Detection evidence

- **Witnesses on the extracted functions**
  (`XshImpl.Controls`): `int_success_and_truncation` computes `2 + 3`,
  `-7 / 2 = -3`, `-7 % 2 = -1`, `7 / -2 = -3` and `7 % -2 = 1`;
  `int_error_branches` computes both zero divisors, add, subtract and multiply
  overflow, `MIN / -1`, `MIN % -1` and an unsupported operator;
  `duration_success_and_truncation` and `duration_error_branches` cover every
  duration result and error. `int_diagnostics` and `duration_diagnostics` pin
  each error's code to the SPEC name.
- **Production execution.** `cargo test` in the slice runs the same cases
  against `checked_int_binary` and `checked_duration_binary` compiled by
  rustc 1.102.0-nightly (5b7b37aef 2026-11-08), the production toolchain, in
  debug and release builds, and checks `i64::MIN.checked_rem(-1)` and
  `checked_div(-1)` are `None`.
- **Harness mutants.** Each is a copy in `XshImpl.Controls` that changes one
  rule, and Lean proves it violates the same refinement property: wrapping
  addition, the upstream remainder model, floored division, an overflow error
  for a zero divisor, an admitted comparison, an implementation that always
  overflows, a division that panics at `MIN / -1` instead of reporting
  overflow, wrapping duration addition, saturating subtraction, a wrapped
  negative factor and a wrapping interval count, and an implementation that
  always reports a type error.
- **Source mutants.** `mutant.sh` replaces `checked_add` with `wrapping_add`
  in a copy of `src/arith.rs` and checked Duration subtraction with
  `saturating_sub` in a copy of `src/duration.rs`, extracts the copies, and
  requires that both unchanged refinement proofs fail and that Lean proves
  each extracted mutant violates its property, at `i64::MAX + 1` and at
  `0ms - 1ms`.

## Adapters

The theorems prove the shared decisions, not their adapters. Native
regressions in [`int-arithmetic-routes.xsh`](../tests/xsh/int-arithmetic-routes.xsh)
and [`duration-arithmetic.xsh`](../tests/xsh/duration-arithmetic.xsh) check each
behavior class (overflow of `+ - *`, both zero divisors, `MIN / -1`,
`MIN % -1`, truncation toward zero, every duration error) through
unannotated and annotated `let` bindings, returned expressions, compound
assignment, literal operands and `const` initializers, with the error code
and, except on one route, the operator's line and column. Which execution
route each form takes is from reading the lowerer. The exception is a
reproduced blame defect (XSH-157): an unannotated `let result = left + right`
over `Int` parameters takes the typed `IntBinary` route, which carries no
operator span, and reports the failure at the caller's `apply(...)` instead
of the operator that the annotated and returned forms report. That regression
asserts the error code only until the span is carried. Unproved:
that the adapters pass the operator and operands unchanged, that constant
preparation's decline leaves the runtime to report the failure, and that the
span is the operator's. The execution adapter's
`expect("verified integer binary operation")` relies on the lowerer emitting
only `+ - * / %` into `IntBinary` (`src/runtime/eval/lower/scalar.rs`);
`FullVerifier::verify` does not check that operator (`verify_scalar` in
`src/runtime/eval/indexed/full/verify_schemes/producers.rs`), so IR built
another way could reach the panic. This was found by reading and is not
reproduced (XSH-155).

## Reproduce

From the repository root. `extract.sh` and `mutant.sh` need Docker, the first
run needs network access, and `check.sh` and `mutant.sh` need Lean 4.35.0
(elan toolchain v4.35.0) on `PATH`:

```sh
dev/verify/aeneas/extract.sh --check
dev/verify/aeneas/check.sh
dev/verify/aeneas/mutant.sh
cargo test --locked --manifest-path dev/verify/aeneas/slice/Cargo.toml
python3 formal/scripts/check-inventory.py
```

`extract.sh` downloads the Aeneas release and checks its SHA-256, builds the
extraction image from `extract.Dockerfile`, extracts the slice and compares the
translation with the committed file (without `--check` it installs it).
`check.sh` fetches Mathlib's cache, requires the Aeneas package to be the
pinned revision whose working diff is exactly the patch (applying the patch
when absent), builds `XshImpl`, audits the axioms of the 25 recorded theorems
and runs `leanchecker --fresh XshImpl`.

Measured on the shared 32-CPU host in one 4-CPU Lean slot, over two runs:
extraction 5 s; warm `check.sh` 184 and 228 s; `leanchecker --fresh XshImpl`
172 and 204 s wall, 164 and 194 s user, 2.51 GiB peak resident memory both
times, over the whole import closure including the Mathlib and Aeneas modules
it reaches; rebuilding the 60 Aeneas modules the patch
affects 44 s and 2.0 GiB; both source mutants 44 s. Mathlib v4.35.0
(`56a00585`, Apache-2.0) and Aeneas (Apache-2.0) are dev-only dependencies of
this harness, not of `formal/` or any product.

## Binary route

One bounded investigation of the OSS Verify route used the harness
[`dev/verify/binary/fuzz`](../dev/verify/binary/fuzz/fuzz_targets/checked_int_binary.rs):
an `LLVMFuzzerTestOneInput` that decodes an operator and two `i64` values,
calls the production `checked_int_binary`, compares it with an `i128`
restatement of `Xsh.Core.Arith.checked`, and aborts on a difference. It
returns -1, libFuzzer's rejection, for an input shorter than 17 bytes, so
rejection is distinct from a checked input. A halts proof of its plain build
would prove the property for the binary. The package is a static library,
because the driver supplies `main`. Built for riscv64 musl with rustc
nightly-2026-09-17 (`panic = "abort"`, `opt-level = "s"`, `-Zub-checks=no
--cfg fuzzing`), the entry point keeps the guarded `div`, `rem` and `mulh`
instructions and a reachable call to `std::process::abort`:

```sh
rustup target add riscv64gc-unknown-linux-musl --toolchain nightly-2026-09-17
RUSTFLAGS='-Zub-checks=no --cfg fuzzing' cargo +nightly-2026-09-17 build --locked --release \
  --target riscv64gc-unknown-linux-musl --manifest-path dev/verify/binary/fuzz/Cargo.toml
cd dev/verify/binary/fuzz/target/riscv64gc-unknown-linux-musl/release
member=$(llvm-ar t libxsh_arith_fuzz.a | rg '^xsh_arith_fuzz-[0-9a-f]+\.xsh_arith_fuzz\.')
llvm-ar x libxsh_arith_fuzz.a "$member"
llvm-objdump -dr "$member" | rg 'R_RISCV_CALL|\b(div|rem|mulh)\b'
```

The route stops there. A statement exists only for binaries that the OSS Verify
builder makes from a public repository named in a maintainer's enrollment pull
request, with its own toolchain image (rustc 1.90.0 and a sealed standard
library). Local proof construction over LPS is not available: published proofs
are written by Theorem's proving systems. Local checking would need the
checker image and up to 20 GiB per proof. Even a proof would cover a riscv64
binary built by another compiler, not the x86_64 and aarch64 products.

## Attempts

| Attempt | Classification | Outcome |
|---|---|---|
| Charon from source at `454586e9` | resource exhaustion (shared Rust build slot) | Replaced by the Charon bundled in the pinned Aeneas release, run under glibc in the extraction image |
| Aeneas `-print-unknown-externals` | tool limitation | Aeneas crashed naming a tuple; the flag is not used |
| Translation without core bodies | tool limitation | Two `TryFrom` impls became axioms; including core's bodies replaced them |
| Including core's `TryFrom` bodies alone | tool limitation | Aeneas could not read the fields of opaque `TryFromIntError`; including the struct fixed it |
| Including `TryFromIntError` | harness/correspondence defect | Lean rejected the `IntErrorKind` payload against the library's `Unit` model; `extract.sh` erases the two payloads |
| `checked_rem` refinement with the upstream library | harness/correspondence defect in a library model | Lean proved the refinement false at `(Rem, i64::MIN, -1)` under the upstream model; production Rust returns `None` there, so XSH is correct; the library model was patched |
| `ratio_case` closed by `simp` over unfolded bounds | tool limitation | The kernel reported deep recursion; the case now uses the model's own `ratio` lemmas |
| Concrete witnesses by `decide` on `checked_div` and `checked_rem` | tool limitation | Reduction stops at the irreducible `IScalar.min`; the witnesses derive from the model's specification lemmas |

No proof attempt exposed an XSH defect, and no production source changed.
Writing the adapter regressions exposed the typed-route blame defect
(XSH-157); its repair is in the lowering and runtime owners, outside this
increment.
XSH-150 to XSH-154 concern paths, `.require`, module contracts and map writes,
not these functions, and were not reused.

## Next

Close the adapter gap: carry the operator span on `IntBinary` (XSH-157) and
check its operator in `FullVerifier::verify` (XSH-155), then prove that both
execution routes pass operators and operands to the shared decision and report
its error at the operator span, which needs the lowering and indexed-execution
steps for `IntBinary` and `ExprBinary` in the IR model. Then apply the same route to the membership decision
behind `value_matches_static_type`.
