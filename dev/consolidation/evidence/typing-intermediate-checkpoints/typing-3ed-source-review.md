# Source review: typing checkpoint 3edce3a

Reviewed committed `3edce3ab6f80919f2ea7a289f86e6300c10ebf0e` (`866794bfce64a61b4c724b1c03681a1657308217`), diff from `afceaec9d213db268f8df4a807bb476108eddce7`. Source-only; no builds or tests run.

## HOLD: exported scheme index cannot compile / is not connected

At committed `src/sema/check/module_driver.rs:464-465`, `checker.into_output()` returns `CheckOutput` and then assigns `output.function_schemes`. `CheckOutput` at `src/sema/check.rs:537+` has no such field; the only field is `CompactDeclOutput::function_schemes` at `src/sema/check/compact.rs:32`. This is a definite compile error. `CheckedDeclarations::into_compact` at `src/sema/check/checked_declarations.rs:129-177` also no longer populates that map, so removing the invalid assignment alone would leave it empty. Preserve the intended authority: derive exported entries from finalized `UserModuleSig`, then carry them to `CompactDeclOutput`; retain private `scheme_bodies` separately. Add an integration assertion through actual module compacting/import, not only the helper map unit test.

The shared worktree became dirty while this review was running (`checked_declarations.rs`, `module_driver.rs`, `constraints.rs`); findings above refer only to committed 3ed. The current uncommitted `module_driver.rs` has removed the invalid assignment, so I did not treat those edits as part of this checkpoint review.

## Rechecked prior Any/evidence concern: fixed

`constraints.rs:1475-1480` resolves the row payload and rejects `Any` recursively before binding. `schemes.rs:437-445` makes `RowEvidence::new` reject recursive `Any` and unresolved/recovery types. `CallInstantiation` at `schemes.rs:375-413` may trust that invariant: `RowEvidence` fields are private, `new` is its only constructor, and its value is immutable; it still checks binder sets and row lacks. The new test covers nested `Set[Any]`, valid evidence, and explicit `Any` in a nonquantified signature. No remaining source blocker here.

## Parser scope: matches the approved pure-declaration rule

`parser/stmt.rs:1281-1286` opts into omitted required parameter annotations only for `pure` function definitions. Module contracts use `Required` at lines 741/775, streams use it, and callable types use it in `parser/types.rs:144`. The focused parser test retains diagnostics and `None` recovery for proc, stream, module-contract, and callable signatures. This matches `inference.md`'s declaration-only omission rule; no scope leak found.

## Required-return oracle: old rejection is stale under the approved criterion

The removed negative `export pure public_value() { 1 }` has a uniquely inferred closed ground scheme `() -> Int` (zero binders), and so satisfies `inference.md:314-319` (acyclic pure return inference) and `:415-416` (exported inference when the body has a unique closed rank-1 scheme). It is not a valid `check.required-return` oracle under that criterion. The new SPEC says pure returns may be inferred when one closed rank-1 scheme is available. Keep recursive pure and underdetermined/contextual failures; they remain covered. The design's final summary mentions the approved exported identity witness, but its explicit eligibility rule is broader and admits this ground scheme; if only identity was intended, the design condition must be narrowed before restoring the old rejection.

## Gate

HOLD this checkpoint on the compile/API-layer mismatch above. The Any boundary, parser omission scope, and removed required-return oracle are source-consistent. No end-to-end acceptance claim.
