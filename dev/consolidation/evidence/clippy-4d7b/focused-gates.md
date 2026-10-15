# Focused verification: clippy integration 4d7b

Source is `4d7b0d0d103ab2bff68bf0a019e7a172cb9ce6ae`, tree `bf7a566fbf2ca782a4473307746cec9be01d7ff1`; the worktree remained clean and the tree id was unchanged after verification.

## Serialized build queue and reusable products

Root ran the required gates serially. The all-target, all-feature Clippy command (`cargo clippy --locked --offline --target x86_64-unknown-linux-musl --all-targets --all-features --quiet -- -D warnings`) passed with zero diagnostics. The debug unit harness command (`cargo test --locked --offline --target x86_64-unknown-linux-musl -p xsh --lib --no-run --message-format=json`) passed and produced `target/x86_64-unknown-linux-musl/debug/build/xsh/692c8c084af82ba2/out/xsh-692c8c084af82ba2`, SHA-256 `f4947c99a830e76cb669387aec987931fa11df2d54669180d98685d4512e77d9`. The verification product command (`cargo build --locked --offline --profile verification --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht --message-format=json`) passed. Products are under `.work/target-clippy-integration/x86_64-unknown-linux-musl/verification`: `xsh` SHA-256 `1be6688ba89fec73eb7c4d765b59e22a3338c23c9562cad36ad476bd0feb9847`; `xsht` SHA-256 `4f0b504db48df6add53e41ba82d1a40f0d11e4387ebd5349229b7e06d13bf07f`.

Reuse these artifacts only for this exact source and profile. Clippy check metadata is not a linked debug harness or verification executable. Keep the `0d6` binaries as baseline references only; do not copy products between revisions. A source change requires a fresh matching build.

## Native selection and result

The focused run used the candidate verification `xsht`, pinned image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`, CPU 3 only, 1 CPU, 2 GiB with no swap, network disabled, and no runtime preload. Source and target were mounted read-only. Results are in `.work/tmp/clippy-focused-gates-4d7b/results/`.

Fifteen `xsht test -j 1 --exact PATH::TEST` selections passed:

```text
tests/xsh/stdlib/net.xsh::test_net_start_mock_job_is_single_consumption
tests/xsh/stdlib/net.xsh::test_net_start_scope_cleanup_releases_admission
tests/xsh/checker-streams.xsh::test_checker_rejects_list_comprehension_domains_and_conditions
tests/xsh/stdlib/time.xsh::test_time_calendar_timezone_and_dst
tests/xsh/stdlib/unix_sessions.xsh::test_read_utmp_ignores_a_partial_trailing_record_and_names_failures
tests/xsh/stdlib/compression.xsh::test_compression_detected_lzma_header_is_strict_and_named_lzma_is_lenient
tests/xsh/parser-forms.xsh::test_keywords_are_schema_and_record_field_labels
tests/xsh/parser-forms.xsh::test_record_literal_spans_lines
tests/xsh/checker-records.xsh::test_checker_computed_map_literals_locate_key_errors_without_weakening_values
tests/xsh/basic.xsh::test_process_command_builder
tests/xsh/lowering-coverage.xsh::test_registered_calls_lower_named_entries_through_checked_bindings
tests/xsh/typed-callables.xsh::test_typed_callable_map_literal_is_checked_and_called
tests/xsh/typed-callables.xsh::test_record_spread_keeps_a_compatible_callable_field
tests/xsh/stage-list-stream-parity.xsh::test_stage_transform_outputs_match_list_and_live_stream_sources
tests/xsh/stage-list-stream-parity.xsh::test_stage_terminals_and_keyed_aggregates_match_list_and_live_stream_sources
```

The list preflight matched each full ID exactly. Per-case stdout, stderr, argv, and exit status are retained in `results/`.

The additional command `xsht test -j 1 tests/xsh/nested-machine-bounds.xsh` selected 8/8 tests and passed. Total: 23 tests, 16 commands, all exit 0. No full native suite ran.

## Gate width and parity ownership

`cargo dev gate` on the 52-path diff is broad: `src/syntax/grammar.rs` selects Syntax, Grammar, and Docs; `src/sema/` selects Checker; `src/runtime/eval/` selects both Lowering and Runtime; `src/runtime/eval/route_witness.rs` selects RouteWitness; `src/modules/` selects Stdlib and API; and any `src/`, `crates/`, or `tests/` path also selects Compile. This fans out to whole `sema::`, checker-file, `runtime::eval`, runtime-integration, route-witness, API, and stdlib gates rather than the focused changed behaviors. Keep these broader shared-source gates for batch acceptance.

Paired checker parity is assigned to the producer lane: run baseline `.work/consolidation/resume-candidate-0d6/xsht` and candidate verification `xsht` for `check .` and `lint .`, each from both the 4d7b repository root and `/home/josh/d/laputa-systems/laputa` (8 invocations). Compare status and stdout exactly; compare stderr after only the documented trailing timing line is removed. This lane did not duplicate those corpus runs.
