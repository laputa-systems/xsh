# Clippy on the repaired freeze

Source: `ed90857bdd825439159b7d788ea5d57cd9652920`, tree
`efc56a4a81c7993e35b2fd76a2549742eb8d8847`.

The root-package all-target, all-feature strict Clippy command exits zero.
This Cargo invocation selects `xsh`; it does not select the whole workspace.
The retained source snapshots are clean and unchanged before and after.

The focused `xsh-fuzz` soundness command initially includes dependency
diagnostics and exits 101 on 34 existing XSHT Clippy diagnostics. XSHT source
is unchanged between `45b4c1cd8` and this freeze. That failed command is retained
under `fuzz-with-dependencies`.

The focused command with `--no-deps` exits zero and checks the changed soundness
target. Its logs are under `fuzz-no-deps`. Neither successful command establishes
workspace-wide Clippy cleanliness.

Each directory retains the actual command, status, output, and container resource
capture. These are correctness checks with no timing claim. The build-step
jemalloc preload follows the build-only policy in `docs/TESTING.md`.
