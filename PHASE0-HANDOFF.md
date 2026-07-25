# Handoff: Complete FRONTEND-CAMPAIGN Phase 0

## Context

Campaign doc: `FRONTEND-CAMPAIGN.md`
Benchmarking: `docs/BENCHMARKING.md`
Agent guide: `AGENTS.md`, `docs/AGENT-ROUTING.md`, `docs/TEST-MAP.md`

**Goal of this campaign (memory-first):** make frontend/IR tighter without language changes.
**Phase 0 goal:** freeze evidence + measurement protocol so Phase 1 success is judgeable.
**Do not start Phase 1 IR store work.**

**User defaults (locked):**
- Keep it simple — **no budget formulas**
- Corpus multi-root: `benches/scripts`, `core/`, `examples/`, `showcase/`, syntax/sema/runtime fixtures, plus a named vertical-slice fixture row
- Evidence pack under `target/frontend-campaign/phase-0/` (local, machine-specific)
- **Stage-split retained + peak is critical** (not only suite Divan `max alloc`)
- Campaign default bench is **`make bench-fast`** (memory-only; no timing noise)
- Freeze vertical-slice fixtures in Phase 0
- Stats as durable diagnostic tool + library API Phase 1+ can keep

---

## Already landed (keep / finish, don’t reinvent)

### Bench-fast protocol
- `scripts/bench-baseline.py --fast` / `make bench-fast`
  - 0 outer warmup, 1 measured suite
  - Divan `--sample-count 1 --sample-size 1`
  - separate `-fast` baseline path
  - **memory-only table** (no per-bench time / run spread)
  - whole-suite wall telemetry only (`wall_s`, measured/warmup)
  - records `max_alloc_count` + `max_alloc_bytes`
- Normal `make bench` still 1 warmup / 3 runs for outside-campaign latency
- Docs updated: `docs/BENCHMARKING.md`, `docs/TEST-MAP.md`, parts of `FRONTEND-CAMPAIGN.md`

### Partial Phase 0 code (incomplete / may not compile)
Inspect and either finish or repair:

| Piece | Path | Status |
|---|---|---|
| Alloc tracking | `src/mem_track.rs` | Written: `CountingAllocator`, stage begin/end, peak/live/count/bytes. Binary must install global allocator + `install_marker()`. |
| Module exports | `src/lib.rs` | Added `pub mod frontend_stats;` and `pub mod mem_track;` |
| CST retained | `src/syntax/cst.rs` | `LazyCst::retained_bytes`, `SyntaxTree::{node_count, retained_bytes, retained_bytes_without_token_table}` |
| Dynamic symbols | `src/symbol.rs` | `dynamic_symbol_stats() -> (count, bytes)` for leaked post-preload symbols |
| Type retained | `src/sema/types.rs` | `Type` / `CallableType` / `ModuleExportType::retained_bytes` |
| Frontend stats | `src/frontend_stats.rs` | **Broken/incomplete.** Structs+helpers partially emitted; `measure_source` body missing/corrupt. Rewrite cleanly. |

**Prior agent struggles:** shell/heredoc corruption when writing large Rust files. Prefer `apply_patch` or small verified writes. Avoid giant one-shot heredocs.

---

## What Phase 0 must deliver

### 1. Stage-split retained + peak (highest priority)

For each file, report stages at least:

1. **tokens** — lex → `TokenTable` retained + peak/traffic during stage
2. **cst** — force build `SyntaxTree` retained (without double-counting token table if shared) + peak
3. **ast_check** — production parse/load/check path retained split:
   - AST/`ArenaProgram` retained via existing `arena.stats()` / `retained_bytes()`
   - semantic retained from `CheckOutput.expr_types` (and any compact decl maps you can charge honestly)
   - peak/traffic for the combined production path is OK if redefine retained components are separate
4. **lower** — install compact lowerer; retained of lowered functions/program + probe counts; peak during lower
5. **after_drop** — clone/hold only what execution needs (lowered maps + SourceMap for diagnostics), drop tokens/CST/AST/scratch, report retained

**Peak/traffic mechanism:**
- Dedicated bin `xsh-frontend-stats` with
  `#[global_allocator] static A: CountingAllocator = CountingAllocator::new();`
  and `CountingAllocator::install_marker()` at start
- Library uses `mem_track::begin_stage()` / `end_stage()`
- Without tracker installed, peak/traffic = 0 but retained still works
- Do **not** put counting global allocator into multicall/xsh/xsht product binaries

**Accounting rules (campaign):**
- Include capacity, owner headers, backing strings
- Deterministic, diffable output
- `components_sum` vs reported total; expose `reconcile_delta`
- Never “save” bytes by moving into uncounted pools
- Aggregate corpus + per-file maxima

**Suggested counters (extend existing speech, keep simple):**
```
source_bytes
token_count, token_retained_bytes
cst_node_count, cst_retained_bytes
ast_stmt/expr/pattern/type counts, ast_extra_items, ast_retained_bytes
semantic_type_count, semantic_retained_bytes
lowered_function_count, constructed_functions,
lowered_statement/expression/pattern counts,
lowered_retained_bytes, lowered_blocker_events
retained_after_drop_bytes
dynamic_symbol_count, dynamic_symbol_bytes
per-stage: retained_bytes, item_count, peak_bytes, alloc_count, alloc_bytes
```

### 2. `frontend_stats` library API + binary

- `src/frontend_stats.rs`: `measure_source`, `measure_path`, `measure_roots`
- Default roots:
  `crates/xsh-multicall/benches/scripts`, `core`, `examples`, `showcase`,
  `tests/fixtures/syntax`, `tests/fixtures/sema`, `tests/fixtures/runtime`,
  plus vertical-slice fixtures path
- Output: stable text + JSON
- Wire binary in root `Cargo.toml` like other bins, e.g.
  `xsh-frontend-stats` → `src/entrypoints/frontend_stats.rs`
  `required-features = ["tools"]` if appropriate
- Wrapper optional: `tools/xsh-frontend-stats.xsh` or `scripts/frontend-stats` calling the bin

**Lowered retained:** don’t need a perfect enum walker on day one, but need *honest* retained for installed lowered state:
- After `Evaluator::install_compact_lowered_program` (or equivalent production path)
- Charge maps of `LoweredPureFunction` / top-level program / probe retained units
- Prefer real structure sizes + capacities; layout×count only as explicit estimate labeled as such
- Surface construct probe counts already on `CompactLowerConstructProbeOutput` (functions, stmts, exprs, patterns, blockers)

Reuse existing useful APIs:
- `TokenTable::retained_bytes`
- `ArenaProgram::stats` / `retained_bytes`
- `Parser` / `parse_load_check_text` / `CheckedEntry`
- `Checker::check_compact_declarations` / `probe_compact_bodies` / install lower path inside `Evaluator`
- `symbol::dynamic_symbol_stats`

### 3. Freeze Phase 1 scorecard fixtures (in Phase 0)

Checked-in XSH under something like
`tests/fixtures/frontend-campaign/` (or `frontend-campaign/fixtures/`):

**`vertical-slice.xsh`** must exercise (arena-eval today):
- scalar + string literals
- slots, params, captures, assignment, return
- direct + recursive call
- mutual recursion pair
- if, loop, break/continue, propagation
- guarded match with bindings
- record + field access
- one `RuntimeOp`
- one traceable/erroring op with exact source location

**`vertical-slice-unsupported.xsh`**:
- one construct that fails lower transactionally / is unsupported without placeholder success

Document expected arena-oracle behavior briefly beside fixtures (comment header or tiny `README` / campaign section). These fixtures are frozen inputs for Phase 1 — Phase 1 only implements store/exec/parity against them.

### 4. Layout + asserts
- Extend `scripts/ir-layout.py` IR_TYPES with any missing hot types touched by stats/IR campaign context (`TokenTable`, CST/SyntaxTree, compact AST tags/data, CheckOutput-ish if useful, lowered probe output types already partly listed)
- Add exact `size_of` asserts for existing compact AST IDs/tags/data (`StmtId`, `ExprId`, `ArenaStmtTag`, `ArenaExprTag`, `ArenaStmtData`, `ArenaExprData`, `TokenId`, `TokenTag`, etc.) near arena/token tests — exact only for format invariants

### 5. Evidence pack (local)

Script e.g. `scripts/frontend-campaign-phase0` that writes:

```
target/frontend-campaign/phase-0/
  PROTOCOL.md          # what stages mean, how peak is measured, corpus roots
  bench-fast-1.txt
  bench-fast-2.txt     # two runs; alloc/peak must match (bit-stable)
  ir-layout.txt
  frontend-stats.json  # + maybe .txt
  frontend-stats-vertical-slice.json
  coverage.json        # tools/xsh-ir-coverage.xsh output
  line-counts.txt      # lower.rs, lowered_run.rs, eval.rs (wc -l)
  host.txt             # rustc -vV, host, date
```

Validate:
- two fast benches: allocation + peak columns identical (or only explainable tiny flukes; overshoot means nondeterminism bug)
- frontend-stats no-change double run identical
- components reconcile

### 6. Campaign doc completion
Update `FRONTEND-CAMPAIGN.md` Phase 0:
- check all work + exit-gate boxes only when evidence exists
- decision log entry with paths under `target/frontend-campaign/phase-0/`
- ensure campaign uses **`make bench-fast`** as default gate (partially done)
- Phase 0 exit gate should **not** require multi-run timing travel; wall is telemetry only
- remove “budget formula” expectations if any crept in

Also lightly touch `docs/FRONTEND.md` / `docs/BENCHMARKING.md` / `docs/TEST-MAP.md` if stats tool needs routing.

### 7. Tests (narrow)
- size_of asserts for compact IDs/tags
- frontend_stats determinism on vertical-slice fixture (retained columns stable)
- reconcile_delta == 0 on a small fixture (or documented overhead line)
- do **not** run formatters/clippy --fix / `make lint`
- verify with `cargo build` / targeted `cargo test` / `make bench-fast` / stats tool

---

## Explicit non-goals for Phase 0
- No `IrTag` / indexed IR implementation
- No production lowerer behavior change
- No benchmark-only fast paths changing product semantics
- No multi-sample latency campaigns as Phase 0 gate
- Fairly no new deps
- No budget formula machinery

---

## Phase 1 readiness checklist (how you’ll know Phase 0 is done)

Phase 1 can answer without inventing metrics:
1. Frozen vertical-slice + unsupported fixtures with arena oracle notes
2. Stage retained + peak for those fixtures and full multi-root corpus
3. `retained_after_drop` baseline for “self-contained executable” claim
4. Stable `make bench-fast` suite numbers (esp. `xsht_check_xsh_repository` memory/peak)
5. IR coverage/blocker snapshot
6. Line counts of lowerer/runner/evaluator
7. Layout snapshot via `ir-layout.py`
8. PROTOCOL.md describing measurement so Phase 1 only plugs new storage into the same stages

Phase 1 will still own ≤16B/≤24B budget math itself — Phase 0 just supplies the measurement rails and baseline evidence.

---

## Suggested implementation order
1. Repair/rewrite `src/frontend_stats.rs` + `src/entrypoints/frontend_stats.rs` + Cargo bin
2. Add Evaluator-facing lowered retained helper if needed (keep private IR details inside `runtime::eval`)
3. Vertical-slice fixtures
4. Size asserts + ir-layout extensions
5. Evidence pack script + run it once
6. Tests
7. Checkbox + decision log in `FRONTEND-CAMPAIGN.md`

## Style constraints
- Simplest correct solution; no banner comments
- Preserve useful comments
- No dependencies unless unavoidable
- Update closest tests/docs
- Don’t touch `docs-html/`
- Don’t commit machine baselines; leave under `target/`
- Don’t push / don’t run pre-commit hooks

## Verification commands (narrow → wider)
```sh
cargo build -p xsh --bin xsh-frontend-stats   # or whatever bin name
cargo test --test integration <focused>
make bench-fast
scripts/ir-layout.py
# evidence pack driver
```

When finished: Phase 0 checkboxes checked only with evidence paths recorded; leave working tree ready for Phase 1 vertical-slice IR work.
