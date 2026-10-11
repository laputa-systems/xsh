# Feature-touch starting evidence

The campaign starts at `5e66b6b896a47d0c692739135d51200b007a2a67`.
The actual probe uses `44c7567df12a82056d2fd97523a825c08aa125e8`.
`hashes.json` records identical bytes at both revisions for all 25 relevant
source files. The revisions also differ in campaign instructions, a compression
script and exec regression tests; this is source equivalence for the recorded
probe consumers, not equality of the complete repository.

The result is **four compiler-required pipeline sites versus 15 manual pipeline
sites**. Eleven ancillary compiler sites comprise two arena storage sites and
nine tooling sites. The shared expression codec registration counts once: its
compiler-required encoding arm also generates structural verification. There
is no separate verifier edit for this identity instruction.

Only this evidence directory is committed. The enum additions and unreachable
scaffolds remain patches for a discardable worktree; no synthetic feature is
merged into implementation code.

## Probe and counting contract

Append `ArenaExprKind::FeatureTouchProbe { child: ExprId }` and its arena tag,
plus `BuildExprRow::FeatureTouchProbe { child: BuildExprId }` and
`FullTag::ExprFeatureTouchProbe`. Append tags to preserve existing ordinals.
The synthetic node is an ordinary value identity: contextual type/schema
checking, retained child effects and control transfer, bounded constant
admission when its child is admitted, and one child evaluation. Nested wrappers
must retain the existing bounded frame-execution contract. It adds no binding,
parser spelling, token-literal property or syntax-specific refinement.

Count distinct consumer sites, excluding the enum/tag declarations themselves.
Compiler demand means an observed `E0004` naming the new variant, attributed to
its concrete source match or macro invocation. Every error from the three checks
is retained in `compiler-diagnostics.json`; `compiler-sites.json` normalizes
locations to the probe base. A shared codec invocation contributes one site,
not each generated function or a copy of the macro definition.

Manual demand is a reviewed extension obligation whose existing fallback
compiles unchanged. `manual-sites.json` contains the fallback, source context,
reason and synthetic AST witness for each of the 15 sites. These witnesses are
not executed tests. `hashes.json` proves each complete manual function/constant
definition stayed byte-identical through the successful scaffold check.

`exclusions.json` retains the 96 reviewed static candidates and their exclusion
rules. It preserves enum-only scan lines and supplies normalized base lines.
`inventory-review.json` records the additional tag, coverage, effect and schema
reviews. Optional optimization misses and existing-syntax selectors are not
required identity edits. The compact expression counter length is a manual
site: a new index does not force the length to grow. The induced consistency
assertion error after editing that length is not an initial exhaustiveness error.

## Repeat the experiment

Use disk-backed disposable worktrees and logs. Run one compiler check at a time
through the approved `Dockerfile.test` environment. The observed image is
`sha256:99e195582ba55dc427b1551022fbe56f8f344ed7838e971ecba86ce43a7733e9`
(AMD64); the target is `x86_64-unknown-linux-musl`. Image-definition and original
campaign-wrapper hashes are in `report.json` and `hashes.json`.

Each patch is cumulative against the probe base. In a fresh worktree at that
base, apply one patch and run its corresponding command:

| Patch | Command | Observed result |
|---|---|---|
| `enum-additions.patch` | `cargo check --target x86_64-unknown-linux-musl -p xsh --lib --all-features --message-format=json` | exit 101; six `E0004`, no other errors |
| `xsh-demanded-scaffold.patch` | `cargo check --target x86_64-unknown-linux-musl -p xsht --lib --all-features --message-format=json` | exit 101; nine `E0004`, no other errors |
| `compiler-demanded-scaffold.patch` | `cargo check --target x86_64-unknown-linux-musl -p xsh -p xsht --lib --all-features --message-format=json` | exit 0; no compiler errors |

For sequential use of one isolated tree, reverse the previous cumulative patch
before applying the next. Keep original JSON stdout, stderr and exit status on
disk. Satisfy only compiler-demanded arms and the minimal shared codec row;
leave manual sites untouched. No native or Rust test suite was run by this probe.

The following is a portable compiler-only invocation recipe. It is not a newly
executed check; the recorded checks used the campaign wrapper. Supply the absolute
disk-backed worktree path, the recorded local image and the same musl-compatible
jemalloc library used on the approved host. The original wrapper also mounted
Git metadata, Laputa and verified baseline binaries for its test/run mode; these
library checks invoke no XSH binaries. The owner schedules compilation.

```sh
probe_tree="$(pwd)/.work/feature-touch-replay"
probe_image=sha256:99e195582ba55dc427b1551022fbe56f8f344ed7838e971ecba86ce43a7733e9
probe_preload=/usr/lib/libjemalloc.so.2
probe_flags='-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol'

docker run --rm --init --platform linux/amd64 \
  --tmpfs /tmp:rw,exec,nosuid,mode=1777 \
  -v "$probe_tree:$probe_tree" -w "$probe_tree" \
  -v "$probe_preload:/usr/lib/libjemalloc.so.2:ro" \
  -v xsh-cargo-registry:/root/.cargo/registry \
  -e LC_ALL=C -e TZ=UTC -e TMPDIR=/tmp \
  -e "CARGO_TARGET_DIR=$probe_tree/target" -e CARGO_BUILD_JOBS=12 \
  -e "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=$probe_flags" \
  -e LD_PRELOAD=/usr/lib/libjemalloc.so.2 \
  "$probe_image" \
  cargo check --target x86_64-unknown-linux-musl -p xsh --lib \
    --all-features --message-format=json
```

Use the other two Cargo argument lists from the table for their stages. On close,
repeat this exact variant shape, target/configuration and counting rule against
the consolidated owner modules; remap consumer locations by symbol rather than
reusing start line numbers. Do not reuse host-built target artifacts as Linux
verification evidence.

`scan.py REPO_ROOT` regenerates lexical match candidates from `src/` and
`crates/xsht/src/`. It masks literals/comments and tracks delimiters; it is a
review aid, not a certified Rust parser or an automatic semantic cost measure.
Use the recorded manual witnesses and exclusion rules to review its output.

## Evidence retained and omitted

The tracked bundle contains every compiler error message, primary/secondary
span and macro invocation/definition location, final success, exact commands,
exit statuses, source and patch hashes, manual witnesses and exclusions.
`SHA256SUMS` hashes all other files in this directory.

Complete original Cargo JSON streams and stderr remain in the local campaign
archive only. Their byte lengths and SHA256 hashes are in
`compiler-diagnostics.json`. The filtered evidence omits Cargo artifact messages,
warnings, full rendered strings and full macro expansion bodies. Those hashes
identify the omitted logs; they cannot validate raw bytes without that archive.
This is static extension-cost evidence and compiler acceptance of unreachable
scaffolds, not runtime acceptance of a new language feature.
