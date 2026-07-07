## compact runtime `use` lowering bugs

Both bugs are in the compact lowering of `use` statements. Fixing either in
isolation triggers a stack overflow in the xsh compilation of pm.xsh when any
other change to `lowered_run.rs`, `modules/fs.rs`, or `module_harvest_source`
is applied at the same time.

To reproduce the stack overflow, apply either fix below, rebuild xsh, then run:

    make -C ../laputa world-build

Verification should be threaded back to
`../laputa/AGENTS.md` (or a note in `Makefile`) so `make world-build` passing
confirms both fixes are in place.

### 1. named arg mapping in `fs.root_symlink` dispatch

`src/runtime/eval/lowered_run.rs` ~L4962

The lowered dispatch for `FsRootSymlink` reads `parents` from `values[3]` and
`overwrite` from `values[4]`.  When `overwrite: overwrite` is the only named
optional arg passed by the caller, the lowering puts it at index 3 instead of
4, so `parents` receives the caller's overwrite value (usually `false`) and
`overwrite` falls to its default `false`.  With `parents=false`,
`create_dir_all` is skipped and the symlink fails with ENOENT.

Affected callsite: `pm/local.xsh:447`

    fs.root_symlink(dest_root, target, rel_path, overwrite: overwrite)?

The `root_install_file` dispatch already has the correct pattern — a match on
`(values[5], values[6])` that treats a single optional arg as `overwrite`.
Apply the same pattern to `FsRootSymlink`.

### 2. `module.load` silently drops `let` exports from PKGBUILDs with `use`

`src/runtime/eval/lower.rs` ~L2978

`compact_module_exports_for_use` iterates the imported module's exported
statements.  For each `export proc`/`export pure`, it checks whether the
qualified function is in the `LowerableFunctions` set.  If any function is
missing, the entire function returns `None`, which kills the `Use` lowering.
The child evaluator's function set for the harvest source doesn't contain the
imported module's functions (they failed to lower because the harvest source
doesn't have the right source text for cross-module function bodies).  The
result is an empty export record — no `let` values.

Fix: change `return None` to `continue` so missing function exports are
skipped instead of aborting.  The imported module's procs don't need to be in
the child's function set for the harvest to complete — the child evaluator
only executes `let` definitions, not function calls.  The parent evaluator
installs the functions separately via the dynamic namespace.

### interaction

Applying fix 2 causes pm.xsh's own `use` imports (`use pm.local`,
`use pm.remote`, etc.) to lower in ways the compact runtime couldn't handle
before.  This changes the lowered-program layout, and any extra change that
affects the lowered representation (fix 1, or any edit to `fs.rs` or
`module_harvest_source`) triggers infinite recursion in the compact lowering
of pm.xsh itself.  The two fixes must be debugged together.
