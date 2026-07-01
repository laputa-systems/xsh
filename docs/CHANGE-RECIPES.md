# Change Recipes

These are file checklists, not design rules. Use the nearest existing pattern
before adding a new abstraction.

## Add Syntax

1. Update `docs/SPEC.md` first or in the same change.
2. Add arena storage/accessors in `src/syntax/arena.rs`, and shared leaf syntax
   in `src/syntax/node.rs` when needed.
3. Parse through the arena builder in `src/syntax/parser.rs` or the focused
   parser submodule.
4. Format in `crates/xsht/src/format.rs`.
5. Check in `src/sema/check.rs` or the focused `src/sema/check/*` module.
6. Evaluate/lower in `src/runtime/eval.rs`, `src/runtime/eval/lower.rs`,
   `src/runtime/eval/lowered_run.rs`, or a focused runtime module.
7. Update `crates/xsht/src/lint.rs` and `crates/xsht/src/grep.rs` if the syntax
   affects lint or grep behavior.
8. Add syntax fixtures and targeted parser/formatter tests.
9. Add sema/runtime tests if the syntax changes behavior.

## Add A Value Method

1. Add the method signature and `RuntimeOp` in `crates/xsh-registry/src/signature/*`.
2. Add checker support if type behavior is not fully described by the signature.
3. Implement runtime dispatch in `src/runtime/eval/methods.rs` or a focused
   helper.
4. Add tests in `tests/runtime/modules.rs` or the closest runtime module.
5. Regenerate docs; `docs/STDLIB.md` and `docs-html/*` should update.

## Add A Module Function

1. Add the module signature and `RuntimeOp` in `crates/xsh-registry/src/signature/*`.
2. Put reusable host logic in `src/modules/<module>.rs`.
3. Dispatch evaluator-stateful behavior from `src/runtime/eval/modules.rs`.
4. Add or update record schemas in `crates/xsh-registry/src/records.rs` when the API returns
   structured records.
5. Add runtime tests and regenerate stdlib docs.

## Add Runtime Process Behavior

1. Read `docs/SPEC.md` sections 9-12 and relevant `docs/SPEC-OS.md` sections.
2. Update `src/runtime/run.rs`, `src/runtime/process.rs`,
   `src/runtime/eval/command.rs`, or `src/runtime/eval/stmt.rs`.
3. Preserve status-as-data versus propagated-error behavior.
4. Add focused tests in `tests/runtime/run.rs`, `process.rs`, `os.rs`, or
   `unix.rs`.

## Add Structured Stream Behavior

1. Read `docs/SPEC.md` section 14 and `docs/STREAMS.md`.
2. Update signatures in `crates/xsh-registry/src/signature/streams.rs`.
3. Update checking in `src/sema/check/stream.rs`.
4. Update runtime behavior in `src/runtime/eval/stream.rs`.
5. Add tests in `tests/runtime/streams.rs` and examples when user-facing.

## Update Tutorial Docs

1. Edit `docs-src/CHAPTER-*.md.in`.
2. Update or add examples in `examples/`.
3. Add new examples to `examples/catalog.json`.
4. Run the docs gate in `docs/TEST-MAP.md`.
5. Keep generated `docs/CHAPTER-*.md` and `docs-html/*` changes.

## Update Reference Or Standard-Library Docs

1. Edit implementation metadata or signatures, usually in `src/docs.rs` or
   `crates/xsh-registry/src/signature/*`.
2. Run the docs gate in `docs/TEST-MAP.md`.
3. Keep generated `docs/REFERENCE.md`, `docs/STDLIB.md`, `docs/XSH-GUIDE.md`,
   and `docs-html/*` changes that match the source change.

## Add Lowered IR Support

1. Read `docs/IR.md` and the IR map in `docs/ARCHITECTURE.md`.
2. Confirm normal AST behavior is already correct.
3. Add lowering only for exact pure/effect-free behavior.
4. Update `tools/xsh-ir-coverage.xsh` and a `perf/interpreter/*.xsh` scenario
   when the surface expands.
5. Run targeted runtime tests plus the relevant IR coverage or perf check.
