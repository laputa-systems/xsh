# Layout invariant repair

The repository layout invariant failed because two lint proofs treated
`check.redundant-parens` as a semantic failure. Grouping operands therefore hid
`lint.needless-annotation` in `core/factor.xsh` and
`lint.prefer-callable-alias` in `dev/internal.xsh`.

Source fix: `e9ef913df9ec83cd1bffb4f93fddfbad71031c8c`, based on
`b1bc741fcd512d1c71b43116853278a4a3f1521a`. Local annotation facts still reject
all other checker errors. Callable alias baseline and replacement proofs still
reject every other checker diagnostic. Parser diagnostics retain their veto.
No source selection, configuration, formatter, or invariant assertion changed.

## Failure and focused results

The original R5 broad run reported the layout invariant failure but was stopped
before its buffered panic was printed. An exact full-corpus replay reached its
240-second deadline with exit 143 and one selected test; it emitted no mismatch
and establishes no pass or failure diagnosis. Its raw output and cleanup proof
are retained in `full-baseline-timeout/`.

The diagnostic corpus starts with the two programs changed since the earlier
qualified source: `core/factor.xsh` and `dev/tests/test-lifecycle.xsh`. Recursively
resolving their `use` paths includes 19 files, listed in `narrow-inputs.txt`.
Imports inside lifecycle's generated script strings were included conservatively.
All file bytes and the root configuration are unchanged. This corpus is a
restricted diagnostic scope, not repository coverage.

| Check | Result |
| --- | --- |
| Restricted invariant, baseline XSHT | 1 selected, 1 failed, 121 filtered; exit 101 |
| New unit regressions, production unchanged | 2 selected, 2 failed, 420 filtered; exit 101 |
| New unit regressions, repaired production | 2 passed; warning and fix retained |
| Explicit alias semantic-error veto | 1 passed |
| Annotation cached proof and missing-import veto | 1 passed |
| Entire affected unit modules | 5 alias and 4 annotation tests passed |
| Repaired XSHT minimal CLI pairs | Both warnings and fix hints retained in grouped and ungrouped cases |
| Restricted invariant, repaired XSHT | 1 selected, 1 passed, 121 filtered; 19 files perturbed |
| Full-corpus invariant, repaired XSHT | 1 selected, 1 passed, 121 filtered; 1,675 files perturbed across 1,835 audited inputs; exit 0 |

The baseline restricted panic identifies the precise disappearing findings:
`core/factor.xsh:1172:12` for `lint.needless-annotation` and
`dev/internal.xsh:49:1` for `lint.prefer-callable-alias`.

Minimal CLI probes independently reproduce each disappearance. For annotation,
`var out: List[Str] = []` followed by `out += ["x" + "\n"]` is flagged;
grouping only the concatenation operands hides the finding. For aliases,
`pure target(value: Int) -> Int { value + 1 }` followed by an exact forwarding
wrapper is flagged; grouping only `value` and `1` hides it. `xsht check` confirms
that the grouped variants have only redundant-parentheses diagnostics, while the
ungrouped variants check cleanly. See `minimal-lint/` and `minimal-check/`.

The full-corpus result is retained in [the raw output](full-green/stdout.log)
and [the perturbation count](full-green/stderr.log). The reported 315.83 seconds
is diagnostic test duration, discarded for performance purposes. No performance
readiness protocol was run. The full source and product guards passed and the
owned container was absent after cleanup.

## Executables and environment

Every probe used the pinned Linux image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
network disabled, CPUs 0–11 with quota 12, 24 GiB memory and no swap. This is
correctness evidence; no performance result is claimed. Focused builds used
kache, mold, the pinned musl target flags, locked offline dependencies, and the
shared Cargo lock. Native suites and desugar tests were not run.

| Executable | SHA-256 |
| --- | --- |
| Unchanged integration harness | `b1db9ffc2f2c72b0cba144d2991b367d7cf12515dc97903bb334723aa2908abe` |
| Protected baseline XSH | `e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b` |
| Protected baseline XSHT | `54bb342ae14e77cdacac6db01974f60606ea42ad1a27a6ee3185cbcfe1b01b02` |
| Repaired XSHT | `6e6313ee1ec085a4aa652adb0ba119f1a792746162eba3f0912d9eef30fd082d` |
| Repaired XSHT unit executable | `2007c780a35c7157bc99394c306d9c0826e0e23b9c141972df9170d2c054ad96` |

The integration harness predates the repair and was not rebuilt. Only XSHT was
rebuilt for the repaired process replay. Candidate products were copied into an
owned scratch directory; protected baseline products were never modified.
Replay guards compare executable hashes before and after, assert the source
commit is clean, check a nonzero exact selection, and remove the owned container.

The full source mount is an archive of b1bc. Its entire 1,835-file observable
input set (`.xsh` and `xsht-config.ini`) is byte-identical to e9ef913df, with zero
extra untracked inputs under the helper's exact directory pruning. The repair
changes Rust only. The empty `target` directory is a disposable mountpoint.

## Reproduction

`setup-replay.command` creates only disposable scratch source mounts. It pins
b1bc for the XSH/config corpus, compares every observable input with the repair
worktree, rejects untracked inputs, and constructs the same 19-file closure and
four minimal programs. The source mount's `target` directory contains no cache.
The setup command was syntax checked with `bash -n` and Python `ast.parse`;
its archive and audit operations match the recorded setup, and were not rerun
merely to write this archive.

The other `.command` files retain the actual executed commands and absolute
local paths. Their red and green source guards deliberately differ. Run red
commands against a disposable b1bc worktree; unit RED additionally applies the
archived `unit-red/source.diff` containing only the new regression tests. Run
green commands against e9ef913df. Update only local path variables when
reproducing on another checkout. Preserve the immutable baseline products and
use the repaired XSHT in a separate scratch products directory for green.
`minimal-inputs/` retains the exact four CLI inputs with a `.txt` suffix.

No binary, build target, whole source archive, provenance manifest, or native
suite result is stored here. Raw failing outcomes are retained alongside passes.
