# Expression fallback dispatch review

This is a static source review of the e048b33f2 snapshot. In that snapshot, `ExprProcessCommandArgv` declared `FrameControl` while `indexed_frame_apply_tag` selected its apply path. The later metadata repair changed the schema row to `FrameApply`; it did not erase the historical mismatch or the accepted route receipts. Reproduce the e048 row with `git show e048b33f2:src/runtime/eval/indexed/full/schema.rs`. This review describes source classification only and adds no runtime route proof.

This review classifies the 63 `fallback_branch_inventory` gaps in the e048 route matrix. They are missing source mappings in the collector, not 63 runtime fallback branches. At e048, the generic collector result had an empty `branches` array and `branches_exhaustive: false` for each row.

## Source dispatch authority

The `instruction_view_kind!` macro in [`schema.rs`](../../../../src/runtime/eval/indexed/full/schema.rs#L43) generates the `FullTag` to `ExprViewKind` map. Each schema row also supplies `InPlaceClass` and `FrameStrategy`. The verified evaluator selects its initial path in [`eval_indexed_expr`](../../../../src/runtime/eval/lowered_run/indexed_run.rs#L1287): forced mode or an unmarked instruction enters `ExplicitFrames`; otherwise it enters `eval_indexed_expr_inner`. The direct evaluator rejects unmarked instructions at its entry guard.

Inside `ExplicitFrames::eval_expr` ([`explicit_run.rs`](../../../../src/runtime/eval/lowered_run/indexed_run/explicit_run.rs#L6406)), an unforced in-place instruction delegates to the direct evaluator. Other instructions pass the eager-atom and typed-scalar checks, then `indexed_frame_apply_tag`, then `expr_view_kind` and `eval_expr_view`. The tag-specific apply predicate runs before the generic view dispatch.

The 63 rows map to these source branches:

| Rows | Schema class | Source branch | Tags |
|---|---|---|---|
| 7 direct operations | `Operands; FrameControl` | Normal: `eval_indexed_expr_inner` match arm. Forced: `eval_expr_view` arm. | `ExprComparisonChain` → `ComparisonChain`; `ExprResultFallback` → `ResultFallback`; `ExprFmtString` and `ExprPathFmtString` → `FmtString` and `PathFmtString`; `ExprRequire` → `Require`; `ExprCheckedValue` → `CheckedValue`; `ExprTry` → `Try`. |
| 2 selections | `Operands; FrameControl` | Normal: the `FullTag::ExprIf | FullTag::ExprMatch` special branch before the direct view match. Forced: `eval_expr_view` arms. | `ExprIf` → `IfExpr`; `ExprMatch` → `MatchExpr`. |
| 24 scheduled control operations | `Scheduled; FrameControl` | `ExplicitFrames::eval_expr` → `eval_expr_view`. | `ExprBinary` → `Binary`; `ExprPatternIf` → `PatternIf`; `ExprStrMatch` and `ExprTagMatch` → `StrMatchExpr` and `TagMatchExpr`; `ExprListComp` and `ExprMapComp` → `ListComp` and `MapComp`; `ExprPipeline` → `ListPipeline`; `ExprMethod` → `Method`; `ExprRunCapture`, `ExprSpawnRun`, and `ExprRunPipeline` → `RunCapture`, `SpawnRun`, and `RunPipeline`; `ExprSpawnCommand` → `SpawnCommand`; `ExprWait` → `Wait`; `ExprCapture` → `Capture`; `ExprValueBlock` → `ValueBlock`; `ExprErrorContext` → `ErrorContext`; `ExprContextScope` → `ContextScope`; `ExprLoop` → `Loop`; `ExprRetry` → `Retry`; `ExprModuleCall` → `ModuleCall`; `ExprProcessCommandBuilder` → `ProcessCommandBuilder`; `ExprSelfCall` → `SelfCall`; `ExprTypedCall` → `TypedCall`; `ExprProcCallResult` → `ProcCallResult`. |
| 29 scheduled apply operations | `Scheduled; FrameApply` | `indexed_frame_apply_tag` → `push_apply_operands` → `indexed_frame_apply_child` and `indexed_frame_apply_value`. | `ExprGlob`, `ExprLastStatus`, `ExprRange`, `ExprTag`, `ExprSlice`, `ExprStrByteLen`, `ExprStrByteAt`, `ExprStrPredicate`, `ExprFsFiles`, `ExprFsWalk`, `ExprFsList`, `ExprFsTempDir`, `ExprFsMkdir`, `ExprFsRemove`, `ExprPathReadText`, `ExprPathReadBytes`, `ExprPathExists`, `ExprPathExecutable`, `ExprPathDu`, `ExprPathMetadata`, `ExprPathReadlink`, `ExprPathResolve`, `ExprPathWrite`, `ExprPathMkdir`, `ExprPathRemove`, `ExprArchiveTarCreate`, `ExprAbort`, `ExprFail`, and `ExprError`. |
| 1 tag-specific apply override | `Scheduled; FrameControl` | `indexed_frame_apply_tag` takes priority over the `ProcessCommandArgv` arm and the explicit error group. | `ExprProcessCommandArgv` → `ProcessCommandArgv`. |

For the 29 `FrameApply` rows, `indexed_frame_apply_child` groups only tags with the same child shape. It handles glob and last-status as childless values; range, tag fields, slice, and string operations through their typed view arms; filesystem operations through path, flag, and list operands; and archive creation through path, root, entries, compression, and overwrite. `Abort`, `Fail`, and structured `Error` use dedicated child conversions. `indexed_frame_apply_value` applies the matching decoded view.

`ExprProcessCommandArgv` is the exception to schema-only dispatch classification. [`lower/calls.rs`](../../../../src/runtime/eval/lower/calls.rs#L1016) lowers it to `BuildExprRow::ProcessCommandArgv`, with child IDs obtained by `lower_call_argument`. Those arguments may already be staged through `argument.lowered`. The frame evaluator records `explicit_frame_control_path` from schema capability before it checks `indexed_frame_apply_tag`; the source then sends the tag to generic operand apply. The accepted normal and forced route artifacts record the tag and schema-class route in both modes. Keep those receipts. They prove that the tested tag entered the frame route, but they do not name the deeper apply branch.

## Defensive outcomes

`ExprView::decode` failures become indexed-IR errors in both dispatchers. A missing `ExprViewKind` becomes `frame_strategy_error`. In the explicit dispatcher, the grouped arm at [`explicit_run.rs`](../../../../src/runtime/eval/lowered_run/indexed_run/explicit_run.rs#L7401) returns the same error for eager and apply tags that should have taken an earlier branch. This is a defensive boundary, not a valid route for the mapped rows.

The direct evaluator's grouped `unsupported()` arm at [`indexed_run.rs`](../../../../src/runtime/eval/lowered_run/indexed_run.rs#L1829) rejects scheduled and control-flow views. Verified `Scheduled` instructions do not enter that evaluator because the wrapper selects frames and the direct evaluator checks the in-place mark. `ExprProcCallResult` has a direct match arm, but its `Scheduled` schema class makes that arm unreachable for a verified instruction. Its live route is the explicit `ProcCallResult` arm.

Typed-view fallthrough in `eval_expr_view` returns a frame-strategy error. The wrong-view arms at the end of `indexed_frame_apply_child` and `indexed_frame_apply_value` use `unreachable!`; `indexed_frame_apply_value` also has explicit early branches for `Abort`, `Fail`, and structured `Error`. Tag-specific decode binds each valid tag to its matching view before those branches. Malformed payloads fail decoding instead. These outcomes must not be counted as fallback routes or runtime coverage. A branch that contains an internal `unreachable!` check while still handling a supported grouped view remains a supported arm; only a branch whose whole body is `unreachable!` is classified as a defensive fallthrough.

## Minimal source extractor predicates

The collector can mark these rows source-exhaustive by matching each tag to the dispatch branch that valid verified execution can take:

1. Read each row's `ExprViewKind`, `InPlaceClass`, and `FrameStrategy` from the schema source. Do not infer tag membership from test names.
2. Extract the exact `FullTag` set from `indexed_frame_apply_tag`.
3. Extract exact arm patterns from `eval_indexed_expr_inner` and `eval_expr_view`. Treat `ExprIf | ExprMatch` in the direct evaluator as a separate tag-level branch.
4. Apply the entry predicate: normal in-place control rows use the direct arm; scheduled rows and forced rows use frames. In the frame dispatcher, the apply-tag set overrides view dispatch. `ExprProcessCommandArgv` therefore maps to apply despite its schema strategy.
5. For apply-set rows, map the decoded view's exact arm in `indexed_frame_apply_child` and `indexed_frame_apply_value`. Preserve the explicit error arms and unreachable boundaries as defensive outcomes.
6. Require one source branch for every row and no duplicates. Mark the inventory exhaustive only when each row has a mapped path and the matching defensive boundaries are recorded.

`source_tag_set` and `explicit_expression_routes` in the route tool are useful starting points. They do not cover the direct evaluator, the `ExprIf | ExprMatch` special branch, or the apply override. The 63 rows all have a named runtime oracle in the matrix. This source review adds no new behavior proof. Runtime receipts and the schema-derived route witness remain separate evidence from the static branch map.
