# Source review: typing checkpoint e63689e

Reviewed committed `e63689e1de57ad9c13c989b9619d81b890d6f513` (`4fc3923d98ec09063cd6af1f601039c81b8dd181`) and its `935d7da..e636` diff. Read-only; no builds or tests run.

## Source verdict: GO for export visibility handoff

`statement_is_exported_arena(program, stmt_id)` recognizes the immediate `Export` wrapper (`decl.rs:1561-1563`). Both root and module declaration-registration loops call it with the original top-level statement ID while using `exported_stmt_kind_arena` to inspect the wrapped declaration (`decl.rs:703-738`, `:851-907`). This ties the bit to the exact function declaration rather than to ambient checker state.

`function_sig_arena` stores that bit in `PendingFunctionSig` (`decl.rs:1147-1167,1235-1237`). `PendingFunctionSig` is cloned by the declaration-state machinery; finalization copies `pending.exported` from its snapshot when publishing the checked record (`decl.rs:307-365`). The checked record also receives the explicit bit at initial publication, so inference failure leaves a correctly classified placeholder with no scheme. `insert_pending` asserts one registration (`check.rs:1277-1287`), and the pending-to-checked transition consumes the snapshot before replacing the pending state (`:1314-1325`); there is no later ambient overwrite.

Nested private functions cannot inherit the enclosing export bit: registration examines only the top-level statement lists, and only `function_sig_arena` creates pending function records. Nested declarations are rejected by `check_stmt_arena` (`stmt.rs:3127-3145`) and are not passed through that registration path. The old `current_exported` bit still spans an exported function body for existing diagnostics, but the published scheme visibility no longer reads it. CLI-main publication remains the only direct call using `current_exported`; it has no scheme and cannot enter `function_schemes`.

Module ownership is stable: each module checker is freshly constructed in `module_driver.rs:421-428`; `check_user_module_arena` sets `current_namespace` to that module (`decl.rs:771-773`), and finalization runs on the same checker. The exported bit is stored with the declaration; the namespace is supplied from this module-scoped checker when checked records are published. No cross-module namespace state is reused.

## Compact publication and identity

`CheckedDeclarations::into_compact` gates the existing `CompactDeclOutput.function_schemes` index on `exported` plus namespace and clones the declaration's `Arc<TypeScheme>` (`checked_declarations.rs:130-160`). It retains `scheme_body_facts` independently before that filter, so private generic bodies remain available. This is one compact scheme index; the `UserModuleSig` continues to serve importer typechecking, and both receive the same scheme Arc created in `checked_function_sig_arena` (the checked record clones that `FunctionSig`, and `finalize_user_module_functions` clones it from checked declaration state).

Coverage now includes a direct exported/private root declaration assertion (`checked_declarations.rs:462-506`), a synthetic compact projection test that checks only the exported scheme and preserves a private body fact (`:507-574`), and a source-backed module/import/compact test that checks exported/private status and `Arc::ptr_eq` (`module_driver.rs:941-998`). The existing nested-declaration test still asserts rejection generally; no new source-level test was run here.

## Gate

GO for the source visibility correction and canonical compact Arc flow. This is not end-to-end feature acceptance; runtime and build probes remain pending with the owner.
