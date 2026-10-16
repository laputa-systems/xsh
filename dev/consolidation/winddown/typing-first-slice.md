# Resume the generic module export slice

The typing pipeline is parked at `e20914bb9` on `campaign/typing-pipeline-integration`. The focused Rust group passed 286 tests; the native group passed 19 of 20. No accepted end-to-end slice was established. The worktree is clean after the winddown handoff.

## Confirmed failure

The native importer witness fails when a statically imported module exports a generic `Pure` function and a value. `module_type_from_user_signature` in `src/sema/check/decl.rs` omits quantified functions because `callable_type_from_function_signature` only returns closed ground signatures. The checked exact `Type::Module` then contains one value export while the lowered import row contains three exports. `FullVerifier::verify_registered_module_type` correctly rejects the unequal sets.

A second ownership defect appears when two importers share a module. The driver expands that module's initialization once, then runtime export materialization reads value exports with `self.lookup(export.name)`. A same-named binding from another module can replace the exported value.

## Accepted implementation direction

Add a payload-free `ModuleExportType::GenericPure { optional }` marker. Encode it as export kind `3`, keep the optional flag at bit `2`, and store `IR_NONE` in the existing payload word. Keep the exact module export set and its names and kinds authoritative. Do not add a generic `Proc` marker.

The verifier must accept the marker only when the module export row names one exported child-owned `Pure` function with a `Scheme` contract and at least one type or row binder. Existing scheme verification must continue to validate the callable scheme and body. Monomorphic exports must keep their ground signature path.

Value export rows must carry the existing lexical binding identity. The verifier must match that identity to one exported writer owned by the module and check its initializer against the checked value type. Runtime must retain and fetch the initialized value by that identity across later name collisions. Audit the existing storage before selecting the smallest program-owned representation; a copied ID followed by name lookup is insufficient. Scope retention to the program/test lifetime and account for owned storage.

## Resume checks

- Keep module `marker = 17` exported by the shared dependency. Both importer roots must read `marker` through their aliases in both import orders.
- Add a private same-named `marker = 99` in one importer. Both roots must still read the exported value `17`.
- Preserve the separate-root and joint-root identity and projection cases.
- Add `model.marker` to the prepared-program frontend-drop fixture and run both indexed execution routes.
- Verify a valid generic module export before mutating its payload, name, kind, namespace, visibility, or scheme contract. Keep each rejection tied to its intended invariant.
- Run the native capability group and the complete affected `FullVerifier` group on one final source pin.

## Existing evidence

At `5ec135383e6c8cd476cb6d1581bb5b9259b09e67`, the focused Rust group passed 286 tests. The native capability group passed 19 of 20 tests; the joint importer case exposed duplicate shared-module writers. Followup `0d75720f6` deduplicated shared initialization. At `e20914bb9`, the native capability group still passed 19 of 20 tests because the checked module type omitted generic exports. Neither result accepts the combined first slice.
