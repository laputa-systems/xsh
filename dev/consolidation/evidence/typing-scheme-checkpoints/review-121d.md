# Rank-1 scheme substrate review

**GO on source** for owner fix `121d39242b9e7066a5a4dde8dbb15fb1336d022a` (tree `9b5fa46779b2324ca9747ff290be1273800ed267`). This is a read-only review; runtime evidence is pending with the owner.

- `union` now lowers the selected inherited `Binding::Type` or `Binding::Record` directly after lowering the representative (`constraints.rs:3019–3049`). This bypasses the old root-level early exit and includes both record fields and row tail. The regression `union_lowers_levels_of_inherited_open_record_descendants` checks both child levels and that level-0 generalization rejects them (`4364–4399`).
- Lowering uses the existing shared change journal: each type/row level update records its prior representative (`2915–2927`), and failed constraints restore entries in reverse (`1911–1959`, `2929–2942`). I found no unjournaled write. A focused failed-constraint test that asserts descendant levels restore would strengthen coverage; this is not a source blocker.
- `union_rows` still lowers from the merged root, but it is called only after `unify_rows_inner` resolves and flattens bound tails (`1177–1203`, `1307–1310`). The remaining row roots are unbound and carry no structural descendants, so there is no analogous skipped-binding bug.
- `Type::contains_recovery` now traverses `Set`, module values, and module callable parameter/return types (`types.rs:818–847`). Scheme construction delegates to it and removes its duplicate traversal (`schemes.rs:313–320`). Existing tests cover nested Set/Module recovery and constructor rejection.

No builds or tests run by reviewer. Declaration inference and body-fact integration are outside this checkpoint.
