# W9 compact publication diagnostic

This note retains one bounded serialization probe for the reference memory failure at input 1655. It establishes canonical equality for the same checked object and one completed compact publication row. The evidence is diagnostic only. It carries no full corpus capture, parity acceptance, or speedup claim.

## Source and harness pins

The diagnostic source stays on never-merge branch `campaign/w9-memory-profile-ref-e240` at commit `ccd03d382b34d587e86f5e3542294c15645aebb7`. Inspect it with `git show ccd03d382b34d587e86f5e3542294c15645aebb7:tests/sema.rs`.

The build receipt pins these inputs:

- Source commit `2d27f20fa663cb3dfb21ac54f821499be2fcd041` and tree `7acc4e6469839a3821db3789d777b6d1fffab9c9`.
- `tests/sema.rs` overlay SHA-256 `85fdf87572eb3c97079fc3c4f25b9873e34b6833c12157af58b9c326c8449060`.
- Integration harness SHA-256 `172bc0630b5101b3a38e364c2bad484405ec8e7e5aafef0e89223100dab800fa`.
- Container image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`, platform `linux/amd64`.

The earlier assertion expected `r#"escaped \" quote"#`, with one backslash before the quote. Rust Debug output had three backslashes before the quote. The corrected assertion is `r#"escaped \\\" quote"#`. The [failed output](failed-escape-assertion.log) and [exit status](failed-escape-assertion.status) are retained. The corrected line appears in `tests/sema.rs` on the never-merge branch.

Independent read-only review of repair commit `d3b2b2e95d7f650a6d11bf999246e8c53a897b46` returned GO. The reviewer ran no tests or builds. The shared `derive_helper` change covers the source reference and candidate, derived reference, and exact-owner reference and candidate roles. The pinned raw helper, owner helper, and exact-owner helper remain unchanged. The legacy raw-helper builder remains separate. Review found no need for a schema branch. Fresh build reuse compares the freshly derived helper hash, so an older pretty-derived binary needs a rebuild. Archival readers keep validating recorded helper identities. The full [review note](review.md) is retained.

## Input 1655 result

Input `tests/xsh/system-report.xsh` was index 1655 in the 1,690-input corpus, default profile, shard 16 of 17. Pretty and compact formatting of the same checked object each normalized to 104,463,580 bytes and BLAKE3 `4e929100b4a82c895dd1cdc40e5fb2a2ffc40ae0f46bd0a675e4f3071cc81b11`. The compact Debug string was 111,318,310 bytes. The canonical publication was 104,463,580 bytes. The rendered publication was 123,623,086 bytes. The serialization row became ready.

The diagnostic receipt rows were:

```text
input 1655 tests/xsh/system-report.xsh 9b3f6a2fef9067cd78900e20a9eee86af24e21cecb5ee66c4781087c0505ec33
output 1655 tests/xsh/system-report.xsh default d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab ee0b1f674e8cc56e2c626e3751b2e519886e4e64d84959332d473cd2f0b287e8
terminal shard=16 completed=1
```

The output row's first hash covers diagnostic facts. The second covers rendered publication facts.

The container used one CPU pinned to CPU 0, a 1 GiB memory limit, zero swap, and no network. The harness exited 0. Cgroup `memory.max` was 1,073,741,824 bytes, `memory.swap.max` was 0, and `memory.peak` was 903,163,904 bytes. Cgroup `oom` and `oom_kill` were both 0.

The no-run build exited 0. Its argv was `/home/josh/d/laputa-systems/xsh/dev/consolidation/w9-source-build.sh /home/josh/d/laputa-systems/xsh/.work/wt/w9-memory-profile-ref-e240 build cargo test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test integration --no-run --message-format=json-render-diagnostics`.

Docker ran with `--init --platform linux/amd64 --cpus=1 --memory=1g --memory-swap=1g --network=none --user 1000:1000 --cpuset-cpus=0`. It mounted the frozen input corpus read-only. The child argv was `taskset -c 0 <harness> --exact sema::w9_repository_parity_receipt --nocapture`.

The command set `W9_INPUT_ROOT` to the frozen corpus. It wrote `W9_RECEIPT_OUTPUT`, `W9_TIMING_OUTPUT`, and `W9_ADAPTER_OUTPUT_DIR` under scratch. Its run arguments were `W9_START=1655`, `W9_END=1656`, `W9_PROFILE=default`, `W9_SHARD_INDEX=16`, and `W9_SHARD_SIZE=100`.

The scratch wrapper's `completed-input-count.txt` records 0 because its grep searched stdout. The completion marker was on stderr. The diagnostic receipt records one completed input. This probe receives no acceptance capture credit.

The exact top-level regular files from the final run are retained in [run-1655/](run-1655/), including the original stdout, stderr, command, cgroup readings, phase markers, receipt, and status. The `raw/` publication directory is excluded. The final no-run build's [argv](build/argv.txt), [source identity](build/source.txt), [Cargo JSON output](build/cargo-json.stdout), and [exit status](build/exit.status) are retained separately.

## Test outcomes

The final pinned Rust harness run passed these four focused tests, each with one pass, zero failures, and 273 tests filtered out:

- `sema::w9_normalized_debug_writer_matches_canonicalizer_across_chunks`
- `sema::w9_compact_and_pretty_checked_publications_are_canonical_equal`
- `sema::w9_debug_canonicalizer_preserves_owner_associations_and_sequence_order`
- `sema::w9_name_symbol_renderer_changes_only_validated_constructors`

The four captured [unit-test logs](unit-tests/) combine each test's stdout and stderr; each contains its `test result` status. The last test prints expected panics while checking invalid symbol cases. Its test result is `ok`.

The complete Python adapter runner module passed 67 tests. The exact-owner runner module passed 54. Both exited 0, for 121 passing tests total. They ran as `python3 dev/consolidation/<module>.py` with `PYTHONDONTWRITEBYTECODE=1` and `TMPDIR` under `.work/tmp`. Their captured [adapter output](python-tests/adapter.log), [adapter status](python-tests/adapter.status), [exact-owner output](python-tests/exact-owner.log), and [exact-owner status](python-tests/exact-owner.status) are retained.

Both modules also pass after integration at `71f7cd689`, with 67 and 54 tests.
Their [integrated adapter output](python-tests/integrated-71f7-adapter.log) and
[integrated exact-owner output](python-tests/integrated-71f7-exact-owner.log)
retain those results. Each command exits 0.
