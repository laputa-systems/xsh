# Source review: typing checkpoint 935d7da

Reviewed committed `935d7da01b6898fde91e24f61ff4c3f04b205e77` (`72692512cc7f0597dcfae282f9af552bb03009bc`) and its `3edce3a..935` diff. Read-only; no build or tests run.

## HOLD: export visibility is read after its scope has ended

`CheckedFunctionDeclaration.exported` is assigned from `self.current_exported` in `checked_declarations.rs:320-327`. For ordinary root declarations, `decl.rs:703-721` registers every function via `function_sig_arena` after unwrapping `Export`, without setting `current_exported`. The final scheme publication is later in `finalize_function_declarations` (`decl.rs:369-385`), after the export statement wrapper has restored the flag (`stmt.rs:3185-3188`; module path `decl.rs:906-907,1027`). Module declarations have the same order: function headers are registered at `decl.rs:843-871`, and `module_driver.rs:459` finalizes after `check_user_module_arena` returned and restored the flag. Thus final publication sees `false` for exported declarations as well as private ones; `CompactDeclOutput` then omits all schemes because `checked_declarations.rs` gates insertion on `exported` (`into_compact`, lines 152-161). The new direct test at `checked_declarations.rs:462-503` asserts the opposite for exported identity and is the narrow oracle to run.

Capture visibility per top-level declaration from its actual `Export(inner)` wrapper and retain that fact through finalization. Do not keep the checker-wide bit true while checking a function body: `decl.rs:905-907` and `stmt.rs:3185-3187` wrap the whole body check. `check_stmt_arena` still visits nested declarations after issuing `CheckNestedDeclaration` (`stmt.rs:3127-3145`), and `check_function_arena` treats a nested test as exported when this bit is set (`stmt.rs:4490-4496`). This can add an export diagnostic to a declaration that was only nested/private. Add a diagnostic-specific nested-declaration regression in addition to the existing generic rejection test (`tests/xsh/test-declarations.xsh:71-76`).

## Output projection and Arc identity: sound in isolation

`CheckedDeclarations::into_compact` inserts only declarations with `exported == true` and a namespace, cloning the existing `Arc<TypeScheme>` (`checked_declarations.rs` around lines 152-161). The new synthetic test (`:507-568`) verifies exported/private filtering and `Arc::ptr_eq`; the exported identity test verifies body facts retain that same scheme. These tests cover the downstream projection, not the missing syntax-to-finalizer visibility handoff.

## Restoration and namespace lifecycle

Both export wrappers restore `current_exported` unconditionally after the inner check. Checker operations return diagnostics rather than `Result`, and the wrapper branches have no early-return path, so ordinary checking errors still reach restoration. Module checks use a fresh `session_effect_checker` (`module_driver.rs:188-195`) and `check_user_module_arena` sets its namespace (`decl.rs:771-773`); there is no cross-module checker reuse that leaks namespace or export state.

## Gate

HOLD. The new export bit is not carried from syntax registration through final publication, so direct exported-scheme publication is dropped. The synthetic compact test is valid but cannot detect that handoff failure. No runtime or end-to-end acceptance claim.
