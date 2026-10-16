# afceaec9 source review

**HOLD for this source slice.** The body-fact and call-instantiation ownership is otherwise aligned with `dev/consolidation/designs/inference.md`; two publication invariants still need tightening before downstream lowering/verifier work relies on these facts.

## Blockers

1. **A row metavariable can be solved by `Any` payload evidence.** `unify_row_tails` binds an open tail directly to extra actual fields (`src/sema/constraints.rs:1355-1368`); `bind_row` checks row lacks but does not reject dynamic payloads (`1484-1488`). Thus a call expecting `{known: Int, ..R}` can unify against `{known: Int, extra: Any}` and resolve `R` to `{extra: Any}`. `resolve_call_instantiation` then preserves that field (`src/sema/constraints.rs:482-514`). `RowEvidence::new` and `CallInstantiation::new` only call `validate_ground` for row payloads, which allows `Any` (`src/sema/schemes.rs:396-410, 441-448`), whereas type arguments explicitly reject nested `Any`. This bypasses the approved rule that `Any` cannot anchor a type or row metavariable absent an explicit dynamic-boundary operation (`inference.md:154-155`). Add a focused row-solver regression for an extra `Any` field, plus constructor coverage proving nested `Any` in row evidence is rejected; preserve explicit `Any` in nonquantified ground signatures.

2. **The new qualified scheme index includes private functions.** `CheckedDeclarations::into_compact` inserts every declaration with a namespace into `function_schemes` (`src/sema/check/checked_declarations.rs:152-156`), but the module checker handles both explicit exports and unwrapped declarations (`src/sema/check/decl.rs:953-963, 1064-1075`). `CompactModuleGraph::from_file_unit` copies this map as its `qualified_schemes` import/lowering index (`src/loader.rs:219`). The `UserModuleSig` checker handoff correctly filters to wrapped exports (`decl.rs:388-415`), but this new qualified export index needs the same boundary. Carry export status into the declaration record or construct the index from the finalized exported signature.

## Reviewed ownership

- `SchemeBodyFacts` keeps the finalized scheme `Arc` and binder-scoped expression types. Creation rejects `Any`, recovery, and inference recursively, and validates type/row binder scope (`src/sema/schemes.rs:463-490, 602-621`). The same scheme pointer is asserted when body facts are published (`src/sema/check/checked_declarations.rs:143-150`).
- Pending generic call substitutions remain checker-local and are resolved before output (`src/sema/check/local_inference.rs:203`; `call.rs:1323-1340`). Published `CallInstantiation` contains concrete maps only. Direct user-function routes use the contract helper; typed callables and module APIs remain ground contracts.
- The qualified `UserModuleSig` import route retains the checked `FunctionSig` scheme, and the compact scheme index clones the same `Arc`; `CompactFunctionSig` remains a ground projection. I found no duplicate checked parameter/return/effect authority in this diff.
- The previously failing map-ground mapper fixture is corrected: it now uses `SchemeType::Map(Ground(Int), Ground(Int))`, which exercises both mapper branches (`src/sema/schemes.rs:807-838`).

This is a read-only source review. No build, test, or runtime result is claimed; the owner’s narrow build remains separate.
