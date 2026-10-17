# Linked-worktree source discovery repair

Source: base `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4`, reproducer
`777a4cc12`, repair `775ca2d1d`, final code and boundary controls
`c30aed3bc657c2f25881d39e348fa114741d79ee`.

The pinned `ignore 0.4.25` walker checks `.git` metadata only when
`require_git` is true (`src/dir.rs`, `add_child_path`). With false, the walker
treats a linked worktree's `.git` file as a directory and loses the shared
`info/exclude`. Coverage discovery then enters ignored worktree scratch,
including another test's unreadable fixture. This is a traversal defect.

The regression uses a real `.git` file, a per-worktree `commondir`, a shared
`info/exclude`, and an excluded directory with mode 000. Its initial run as
UID 1000 fails with PermissionDenied. The repaired run skips that directory.
Privileged runs also detect the defect because the test checks discovered
file membership.

The repair requires Git context inside a repository. An outside-Git walk
keeps applying `.gitignore`, then defers visible nested repositories to
sequential Git-aware walks. The existing worker cap applies to one walker at
a time. Cancellation aborts pending roots. Traversal errors remain fatal.
No dependency, toolchain, public API, or workflow changed.

## Repository boundary behavior

Git ignore rules now stop at the repository root. Parent `.ignore` and
`.fdignore` rules continue across that boundary. An outside-Git `.gitignore`
continues to exclude files within its own tree and controls whether a nested
repository directory is visited.

The additional boundary witness places `*.outside.xsh` in the outside-Git
parent's `.gitignore`. The original walker incorrectly omits
`nested/visible.outside.xsh`; the repaired walker includes it. Both exclude
`hidden.outside.xsh` in the parent. Both retain the parent `.fdignore`
whitelist for `nested/restored.xsh`, and the repaired walker retains parent
`.ignore` and `.fdignore` exclusions for other nested files. This intentionally
removes accidental cross-repository `.gitignore` leakage.

## Focused verification

All Rust runs use pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
target `x86_64-unknown-linux-musl`, four CPUs, 8 GiB, and four build jobs.
Builds use `kache`, `dev/musl-linker.sh` with `mold`, and the repository's musl
symbol aliases. Runtime tests have no compiler preload and execute as UID
1000. `TMPDIR=/scratch/tmp` is a bind mount of the lane's `.work/tmp/discovery/tmp`;
its `/scratch` path has no Git ancestor. This exercises the outside-Git branch.

The retained outputs show:

- Linked-worktree reproducer: one selected test fails before the repair.
- Boundary witness against the original walker: one selected test fails with
  only `nested/restored.xsh` instead of both expected paths.
- Final source: 18 selected files-module tests pass, with 401 filtered out.
  These include explicit files, nested config exclusions, sorted deduplication,
  ordinary Git parent excludes, nested linked worktrees, ignore precedence,
  unreadable unexcluded directories, and a symlinked root with an in-tree cycle.
- Qualified verification `xsh` and `xsht` products build from the final source.

The [boundary RED derivation](boundary-red-derivation.command) keeps the final test module but replaces the
production block from `enum DiscoveredPath` through `collect_xsh_files_parallel`
with the original `collect_xsh_files_parallel` from base `0dc9cff7`. The final
source is restored and checked against `c30aed3bc` before the passing builds.

The narrow production lint check also passes:
`cargo clippy --locked --offline --target "$TARGET" -p xsht --lib -- -D warnings`.
The complete Docker build invocations are retained as
[unit build](build-unit.command), [verification build](build-products.command),
and [Clippy](clippy.command). The [unit invocation](run-unit.command) selects
`xsht::cli::files::tests::` for the final run. All output paths in those captures
refer to this lane's disposable directories.

## Native coverage replay

The [native invocation](run-native.command) holds the shared native-suite lock
across all four exact cases. It runs UID 1000, mounts the common Git directory
read-only at its original absolute path, and keeps
`.work/tmp/discovery/native-denied` at mode 000 throughout the replay.
`git check-ignore .work` succeeds in each container. Each case selects exactly
one test and reports one pass:

- `test_runner_cov_list_does_not_execute_tests`
- `test_runner_cov_exact_prints_coverage_sections`
- `test_runner_cov_api_opt_in_prints_api_sections`
- `test_runner_cov_json_out_writes_structured_report`

The fixture permissions are restored after the run. Source checks against
`c30aed3bc` pass before and after. Product hashes match before and after:

```text
xsh  c493d0324bcd66f15ace3f71be690248b832fecffb795a10f29ec223fad32b02
xsht baeb0526e3b8c0e3075aee2a01fc16092c4f7aab6ee9f9438cfa238519fe5572
```

The `native/` directory retains complete stdout, empty stderr, and the two
product checks. `red-test.*`, `boundary-red-test.*`, and `qualified-unit-test.*`
retain the causal failures and final focused pass. Build stderr captures retain
successful Cargo completion for the final products and unit harness.

These results close the focused source-discovery repair. They do not replace
the canonical full gate on the next integrated source freeze. No performance
claim is made from the recorded execution durations.

The recorded unit phases use the same captured Docker build and runner, with
the output directory label changed for each phase:

| Source | Phase label | Unit filter | Result |
|---|---|---|---|
| `777a4cc12` | `red` | `discovery_skips_linked_worktree_shared_excludes_before_reading_directories` | one failure |
| `775ca2d1d` | `green` | `discovery_skips_linked_worktree_shared_excludes_before_reading_directories` | one pass |
| `c30aed3bc` with the original walker block | `boundary-red` | `discovery_stops_gitignore_at_repository_boundaries_and_keeps_parent_ignore_rules` | one failure |
| `c30aed3bc` | `qualified-unit` | `xsht::cli::files::tests::` | 18 passes |

The unit runner arguments are `PHASE FILTER`. The native runner argument is
`c30aed3bc657c2f25881d39e348fa114741d79ee`. The build commands select
`cargo test --lib --no-run` for unit artifacts and `cargo build --profile
verification -p xsh --bin xsh -p xsht --bin xsht` for native products.
