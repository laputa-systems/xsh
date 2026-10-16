# Inference checkpoint 814 review

Source-only review: `81481a573c0eae9095190ca2c7177f178385a38c`, tree `f4df2139fd60ad46bcbca9a66cec418c4f79b546`. The detached source worktree was clean before and after. Prebuilt harness SHA stayed `2f35cac3ffebf7a3227ec26adf0669871d7c6d25ab54cf3e0da0f8be5d4725e7`.

## Verification

- `sema::constraints::tests::`: **41 passed**.
- `sema::schemes::tests::`: **3 passed, 1 failed** (`scheme_ground_type_mapping_visits_both_map_components`). The fixture wraps `Type::Map(Int, Int)` in one `SchemeType::Ground` (schemes.rs:626-647), while the callback only rewrites a whole `Type::Int`; `SchemeType::Ground` invokes it on the wrapped value (411-413). The intended regression is `SchemeType::Map(Ground(Int), Ground(Int))`, which exercises the changed two-child arm (419-421). The importer mapper separately recurses through ground `Type::Map` (check/decl.rs:1156-1158, 1173-1175). This red is a bad oracle fixture, not demonstrated production misbehavior; the scheme-test gate remains **HOLD** until corrected and rerun.
- Focused checker probes passed: local checked-expression history 1/1, completed-body binding history 1/1, imported effects 1/1.
- Pinned Linux musl `cargo check --locked --offline --target x86_64-unknown-linux-musl -p xsht --all-targets` passed with warnings denied, 8 CPU/16 GiB, Kache/mold, compilation-only jemalloc. Consumer compile gate: **GO**.

Raw unit logs and statuses are in `.work/tmp/typing-814-runtime/`; the build command, resource caps, source snapshots, and Cargo output are in `.work/tmp/typing-814-xsht-check/`.

## Source contract review

Source review is **GO** for the requested intermediate boundary. `FunctionSig` stores one `Arc<TypeScheme>` (check.rs:1001-1007); `PendingFunctionSig` owns the provisional/generalized callable shape, and `DeclarationInference` has the closed Pending-to-Checked transition (1022-1034, 1103-1115, 1233-1293). Local calls use the pending monotype while provisional and instantiate its generalized shape afterward (check/call.rs:1313-1359); checked local and imported calls share the `TypeScheme` instantiation path (1318-1320, 1362-1391). Module and entry finalization occurs after effect facts, before module signatures are handed to importers (check/module_driver.rs:203-206, 458-462, 327-335). Ground compact signatures are projections, not a second generic contract.

This checkpoint does not establish end-to-end inferred generic behavior: omitted parameter annotations remain parser errors, and scheme body facts/lowering are unfinished. The XSHT result is compile-only; no native integration or full-feature acceptance is claimed.
