# XSH Standard Library Design

The XSH standard library exists to make systems glue predictable: files,
processes, bytes, text, structured data, host state, and explicit interop. It
should make common orchestration tasks safe and readable without turning XSH
into a general application runtime or a compatibility shell.

## What Qualifies

A standard module belongs in XSH when it:

- handles a recurring systems-script boundary such as files, argv, processes,
  archives, config formats, host metadata, checksums, or structured text;
- replaces fragile shell glue with typed values and explicit `Result` failures;
- is small enough to specify completely and test deterministically;
- has stable behavior across ordinary Unix systems or clearly lives under a
  platform-specific module such as `linux` or `unix`;
- avoids ambient process-global policy unless the operation itself is inherently
  process-global.

Prefer narrow helpers over frameworks. Prefer records, paths, bytes, lists,
maps, streams, and `Result` values over hidden handles, background state,
callbacks, or parser objects. A module should expose data XSH can inspect and
compose, not a second runtime living behind an API.

## What Does Not Qualify

The standard library should not absorb work better delegated to specialized
tools or application libraries. The following are explicit non-goals unless a
future design reverses them in this file:

- XDG or user-directory discovery modules such as `dirs` or `xdg`.
- UUID generation or parsing.
- Generic sorted-list helpers such as binary search or bisect APIs.
- Filesystem wildcard matching APIs such as `fnmatch`.
- Global logging frameworks.
- Shell splitting, shell expansion, `wordexp`, or compatibility-shell parsing.
- General buffered I/O, custom stream, memory I/O, or socket frameworks.
- Low-level TCP/UDP APIs beyond focused orchestration helpers such as DNS and
  HTTP.
- Broad cryptography APIs beyond checksums, digests, and verification helpers
  that are directly useful for packaging and manifests.
- Low-level memory, pointer, ABI, or C-interop facilities.

These exclusions keep the library boring in the useful sense: predictable,
auditable, and tied to XSH's orchestration tier.

## Existing Surface

Before proposing or implementing a module, inspect the current stdlib surface:

- `docs/STDLIB.md` is the generated standard-library API manual.
- `docs/REFERENCE.md` is the generated non-stdlib language and tooling
  reference.
- `docs/SPEC.md` is the normative behavior contract; search for the module name
  in the standard modules section.
- `crates/xsh-registry/src/signature/modules.rs` contains checker-visible module
  signatures.
- `crates/xsh-registry/src/signature/methods.rs` contains checker-visible value methods.
- `crates/xsh-registry/src/runtime_op.rs` lists runtime operation IDs.
- `src/modules/` contains stateless host helpers; `src/runtime/eval/modules.rs`
  and `src/runtime/eval/modules/*` contain evaluator-backed dispatch.
- `tests/fixtures/modules/standard-modules.txt` is the compact signature
  fixture used to catch public API drift.

If those files already solve the use case, prefer improving docs or examples
over adding another API.

## Process: Implementing A Proposal

When a stdlib proposal is implemented, the commit must do all of the following.

**1. STDLIB-PROPOSALS.md** - remove the entry from *Open Proposals*.
Implemented APIs belong in `docs/SPEC.md`, generated stdlib docs, examples, and
tests, not in the open-proposal list.

**2. SPEC.md** - update `docs/SPEC.md` in the standard modules section before
or alongside the implementation. Include signatures, return records, defaults,
error behavior, normalization rules, and determinism guarantees.

**3. Signatures and runtime IDs** - add checker-visible signatures in
`crates/xsh-registry/src/signature/modules.rs` and runtime operation IDs in
`crates/xsh-registry/src/runtime_op.rs`.

**4. Implementation** - put host helpers under `src/modules` when evaluator
state is not needed. Keep stateful dispatch in `src/runtime/eval/modules.rs` or
the closest focused runtime module.

**5. Tests** - add focused runtime tests, edge-case unit tests for parsers or
encoders, and update the generated standard-module contract fixture when
signatures change.

**6. Docs and examples** - update the relevant `docs-src/CHAPTER-*.md.in`
template when the API belongs in the tutorial path. Add a small cataloged
example only when it demonstrates a real workflow better than a unit test.

**7. Regenerate docs** - run `xsht docs build` and confirm `xsht docs check`
passes.

**8. Commit message** - reference this file:
`feat: add mime module (STDLIB-PROPOSALS.md)`

---

## Open Proposals

No open proposals.
