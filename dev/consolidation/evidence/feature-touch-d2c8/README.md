# Feature-touch compiler replay at flow freeze

## Source qualification

The isolated worktree is `.work/wt/feature-touch-final-replay`, branch `campaign/feature-touch-final-replay`, at source commit `d2c842812f87ddc13a84ab77b151b2a3ef0bd2ea`, tree `7126ad81de6674348d66b5a8b6428b36246042c3`. The main branch may advance documentation after this freeze; this result applies to the exact source tree above. The source candidate index was scanned from Rust sources equal to this freeze (`src/`, `crates/xsht/src/`, and `core/`).

The diagnostic diff adds only the synthetic child-bearing `FeatureTouchProbe` expression/tag, its child traversal and compact catalog entry, and 53 compiler-demanded `todo!()` arms. It is 67 inserted lines across 24 Rust files. No placeholder implements the feature, and this check does not establish runtime reachability.

## Compiler replay

All checks used `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`, Linux AMD64 musl, `TARGET=x86_64-unknown-linux-musl`, 8 CPUs, 16 GiB memory with memory-swap capped at 16 GiB, `kache`, `mold`, `dev/musl-linker.sh`, compiler-only jemalloc, isolated `.work/tmp/feature-touch-final-replay/target`, and `TMPDIR=.work/tmp/feature-touch-final-replay/tmp`. Rust flags were `-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol`.

| Stage | Command | Exit | E0004 occurrences | Normalized source sites |
|---|---|---:|---:|---:|
| Seed only | `cargo check --target x86_64-unknown-linux-musl -p xsh --lib --all-features --message-format=json` | 101 | 36 | 35 |
| XSH demanded arms/catalog row added | `cargo check --target x86_64-unknown-linux-musl -p xsht --lib --all-features --message-format=json` | 101 | 19 | 19 |
| XSHT demanded arms added | `cargo check --target x86_64-unknown-linux-musl -p xsh -p xsht --lib --all-features --message-format=json` | 0 | 0 | 0 |

The stage-one difference is one compact catalog invocation reported through two macro expansion spans: 34 ordinary match arms plus one catalog row are 35 source sites. Stage two reports 19 source sites. The final successful check has two synthetic dead-code warnings: the variant is not constructed and its child field is unread. Complete Cargo JSON is retained here as `stage*.json.gz`; stderr and exit files are also retained here; filtered source locations are in `stage1-sites.tsv` and `stage2-sites.tsv`, with nearest function owners in `stage1-xsh-owners.tsv` and `stage2-xsht-owners.tsv`. Stage summaries are the adjacent `stage*-summary.json` files. No unit/native tests, release build, benchmark, or doc generation ran.

## Starting manual-site map

The original method listed 15 pipeline sites. On this source, ten semantic obligations remain and one compact catalog metadata row remains. The diagnostic placeholders at these locations do not satisfy them.

| Original site | Current source disposition |
|---|---|
| `compact_collect_expr_call_edges` | Shared `AstArena::for_each_expr_child` already visits the new child; no independent collector branch. The seed registers this child in that shared traversal. |
| `lower_expr` | Compiler demands an arm. A real implementation must preserve the child instruction and `ExprId`. |
| Explicit-frame `eval_expr` | Compiler demands an arm. A real implementation must schedule the child on the bounded explicit frame stack. |
| `eval_indexed_expr_inner` | No independent child-evaluation site for this row: schema marks it `[0; Scheduled; FrameControl]`, routing it through the explicit-frame match. This is static route metadata, not evidence that the expression ran. |
| `schema_expectation_for_expr` | Compiler demands an arm in the expression match and nested callee match. Real handling must preserve expected schema through the child and existing field/index/try projections. |
| `stmt_always_returns_arena` | Removed before this freeze; current return/control-flow owners below cover the related questions. |
| `stmt_has_exit_point_arena` | Removed before this freeze; current block/control-flow owners below cover exit discovery. |
| `expr_definitely_exits_arena` | Compiler demands an arm; must preserve child termination. |
| `return_inference_stmt_returns` | Compiler demands an arm; must preserve return inference through the child. |
| `match_scrutinee_definitely_exits_arena` | Compiler demands an arm; must preserve terminating scrutinee behavior. |
| `tail_expr_uses_result_context_arena` | Compiler demands an arm; must preserve the complete expected `Result` context. |
| `statement_tail_needs_value_context_arena` | Compiler demands an arm; must preserve whether the child requires declared value context. |
| `LiteralConstant::analyze_depth` | Compiler demands an arm; constant admission must preserve the existing depth bound. |
| `ConstantPreparation::expression_inner` | Compiler demands an arm; preparation must preserve expected/schema context and evaluate the child once. |
| `COMPACT_EXPR_KIND_COUNT` | The compact index derives from the catalog, so the compiler demands its new row. The synthetic row receives index 52, making the derived count 53. The existing test assertion still says 52; updating it is an induced test-literal follow-up, separate from initial compiler demand. No test was run. |

The ten remaining semantic obligations are lowering, explicit-frame evaluation, schema/type context, exit/return/context classifiers, and bounded constant analysis/preparation. The catalog row is metadata. Current compiler demand is broader than this list: it also reaches indexed checked-type/producer views, grouping/parentheses, scope checking, resource effects, and tooling. The `todo!()` arms remain unimplemented.

## Current candidate scan and dispositions

The lexical `scan.py` index at this freeze has 300 raw rows (165 `src/`, 135 `crates/xsht/src/`) across 251 function owners (133 XSH, 118 XSHT). There are 241 rows with wildcard fallbacks (127 XSH, 114 XSHT) across 202 owners (104 XSH, 98 XSHT). These are scanner/review counts, not edit counts, semantic-site counts, or runtime reachability. The earlier XSHT 113-row / 92-owner figures are stale for this source.

The regenerated `scan.py` JSON index is `source-candidates-regenerated-d2.json.gz`; the exact XSHT wildcard-bearing owner inventory is `xsht-wildcard-owners.tsv`. Review dispositions:

- The 19 E0004 locations in XSHT require explicit variant arms. They cover formatter/equivalence and grep consumers, child safety/type classifications, linter traversal, and flow analysis. Their `todo!()` bodies are placeholders only.
- `lint_walk::visit_expression` delegates its generic fallback to `walk_expr`; `walk_expr` is itself one of the compiler-demanded exhaustive sites. The child walk consumes the shared arena child iterator. There is no separate unchanged fallback that already interprets the new expression.
- `lint_safety::expr_references_name` falls back to `expr_children_reference_name`; that shared descendant traversal sees the seed-registered child. No variant-specific branch is added there.
- `lint_flow::stmt_flow` at `crates/xsht/src/lint_flow.rs:217` recognizes literal `Bool(true/false)` as a syntax-specific optimization; its wildcard treats other conditions as dynamic and unions the possible else flow. Wrapping a boolean therefore loses that refinement conservatively. This is excluded by the method's no-token-literal/no-syntax-refinement contract. `expr_flow` is a separate compiler-demanded effect/control site and still needs a real child-flow rule.
- The remaining formatter, source-rewrite, name/path/call/literal recognizers, proof/safety recognizers, and lint fixtures are source-form selectors. A wrapper is not one of those forms; their current no-match/no-fix behavior is not evidence that the feature's semantic identity contract is covered.

No additional unchanged XSHT wildcard was found that must be taught identity semantics under the stated method. This finding is limited to the scanned `src/` and `crates/xsht/src/` candidates and the stated value-identity contract; it does not claim all linter/tool behavior for a future syntax feature.

## Semantic limits and follow-up

The synthetic child is intended to be an ordinary identity value: retain contextual type/schema, child effects and control transfer, admitted constants subject to the bound, and one child evaluation. The run used no parser spelling, executed no synthetic expression, and establishes only compiler acceptance of unreachable scaffolds. It is not a feature implementation or runtime reachability result. A future implementation would need separate semantic tests for lowering, child scheduling, context propagation, effect/control classification, and constants. This recount authorizes no syntax implementation.

No production source, tests, docs, or shared campaign files were edited by this diagnostic replay. The branch is intentionally diagnostic and must never be merged.

Diagnostic source commit: `300d284d4e24cd0541da82c892cfe130106a708e` (`test(consolidation): replay feature touch at flow freeze`). It is local to `campaign/feature-touch-final-replay` and must never be merged.
