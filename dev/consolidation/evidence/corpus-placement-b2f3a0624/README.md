# Successor fixture placement repair

Old source `19c9aded0ea42c979fbc6be32162320d29ded3b1` fails the exact Rust test
`lint_format_invariance::formatting_preserves_lints_on_the_repository_corpus`.
Repaired source `b2f3a0624205e68d06f9a439ffe2dea5e7dc4541` passes that same
selector with the same prebuilt harness and compiler products.

## Cause and change

The first failing input is
`dev/consolidation/typing-corpus/capability-witnesses/bytes-reduced.xsh:1:17`:

```xsh
pure first_byte(raw) -> Int {
  raw.byte_at(0) ?? 0
}
let byte: Int = first_byte(b"A")
```

The protected consolidation compiler rejects `raw` with
`parse.expected-param-type`. Required parameters still need an annotation or
default. The [read-only parse probe](parse-probe/stderr.log) reports that exact
path and exits 2. The [original-selector stderr](old/stderr.log) then establishes
the actual gate failure at `lint_format_invariance.rs:406`: `xsht fmt failed
for repository`, followed by the same path and other reduced fixture paths.

The helper copies `.xsh` files and `xsht-config.ini`, then invokes no-argument
`xsht fmt` on the copy. These successor fixtures were outside the existing
evidence exclusion, so ordinary discovery admitted them. `format_one_file`
returns their parse diagnostics, formatting exits 2, and the current-corpus
success assertion fails. The test does not reach the lint comparison.

The repair moves all 15 paired capability inputs into
[the existing evidence directory](../typing-corpus-b235/capability-witnesses/).
Every moved file compares byte-for-byte with its original and Git records all
15 as 100% renames. The only other changes are two README links/classifications.
There is no new exclusion, test change, compiler change, inference merge, or
source rewrite. Explicit successor checks retain the original fixture contracts.

## Controlled replay

Both runs select exactly one Rust test with `--exact --nocapture
--test-threads=1`:

| Corpus source | Status | Selected result |
|---|---:|---|
| Original `19c9aded` | 101 | 0 passed, 1 failed, 121 filtered |
| Repaired `b2f3a0624` | 0 | 1 passed, 0 failed, 121 filtered |

The repaired corpus reports 115 layout-independent diagnostics. The raw
[old stdout](old/stdout.log), [old stderr](old/stderr.log),
[candidate stdout](candidate/stdout.log), and
[candidate stderr](candidate/stderr.log) retain both outcomes. Runtime durations
in those captures are not performance evidence.

The unchanged harness is the R4-used `xsht` integration executable with hash
`b1db9ffc2f2c72b0cba144d2991b367d7cf12515dc97903bb334723aa2908abe`.
The protected release products are XSH
`e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b`
and XSHT
`54bb342ae14e77cdacac6db01974f60606ea42ad1a27a6ee3185cbcfe1b01b02`.
Each phase retains matching before/after product hashes and its source pin.
Rust and Cargo inputs are identical between the source commits.

The [replay command](replay-invariance.command) binds the source at the
harness's encoded `/work` manifest root and the protected binaries at its
encoded `/work/target/x86_64-unknown-linux-musl/release` path. The harness and
products are read-only. Each run uses pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
CPUs 28–31, four CPU quota, 8 GiB, no swap, no network, and a 180-second bound.
All scratch and `TMPDIR` storage is under the lane's `.work/tmp`.

Run the [archive setup and complete input audit](archive-setup.command) before
`replay-invariance.command candidate`. It creates the exact pinned archive and
physical mountpoint under the lane's `.work/tmp`, then checks the helper's
observable input set and every input's bytes. It refuses an existing archive.
The command combines the preparation and audit used for the recorded replay;
this documentation followup does not recreate the archive or rerun the gate.
The replay guards still require the recorded source checkouts. Keep the command
copies under `.work/tmp` when selecting those commits.

The candidate mount is a `git archive b2f3a0624` copy with an empty excluded
`target` mountpoint. All 1,835 tracked `.xsh` and `xsht-config.ini` inputs match
the candidate before and after execution. A scan using the helper's exact
`.git`, `.work`, `target`, and `.claude` pruning rules finds 1,835 observable
inputs and zero untracked inputs in both original and candidate roots.
Missing Git metadata in the archive is immaterial to that helper.

Two candidate container setup attempts exited 125 before selecting any test.
The [first](mount-failure/stderr.log) could not create a nested mountpoint in
the read-only source. The [second](skeleton-mount-failure/stderr.log) could not
create the top-level `target` mountpoint either. The archive copy supplies that
physical directory without writing to the real source or a build cache.
Their failure statuses and empty owned-container cleanup results are preserved.
The parse probe also retains its initial host-timeout option rejection, which
occurred before Docker; the corrected command uses BusyBox-compatible flags.

All owned containers are absent after cleanup. Source guards and product hash
comparisons pass. No Cargo build, formatter rewrite of repository source,
native suite, shared-target write, or broader canonical retry ran in this lane.
The original R4 failure remains a failed interrupted batch, separate from this
focused passing result.
