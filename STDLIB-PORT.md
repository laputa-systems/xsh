# Standard library port: remaining work

The Rust-to-XSH standard library port is **implemented but incomplete**. This is
the sole task list for finishing it. The language contract lives in
`docs/SPEC.md`; the embedding and authority boundaries live in
`docs/ARCHITECTURE.md`; test commands live in `docs/TEST-MAP.md`. Benchmark
methods and fixtures live in `bench/stdlib-port/README.md`.

## Shipped state

- R01–R11 are script-backed: quoting, CLI policy, argument words, MIME, TUI
  formatting, numeric presentation, string policy, checksum lines, INI encoding,
  JSON path policy, and environment convenience. R12 is script-backed on Linux
  for `system.os_release`, `system.memory`, `linux.meminfo`, `linux.modules`, and
  `unix.uptime_seconds`; macOS retains its native bindings. The implementations
  are under `stdlib/`, bound by `crates/xsh-registry`, embedded by `src/stdlib.rs`,
  prepared by `src/loader.rs`, and executed by the verified indexed runtime.
- G02 (`hash.verify_file`) is script-backed. G01 (`fs.gitroot`), G03's JSON file
  wrappers, G05's interface inventory, and G06 (`linux.disk_usage`) retain native
  host boundaries. G04 (`linux.routes`), G05's block-device inventory, and G07
  (`linux.modinfo`/`linux.depmod`) were ported, failed their measured gates, and
  reverted. `linux.rfkill_list` was reverted with the block-device prototype
  **without its own measurement**; its performance disposition is unqualified.
- The private bridge owners are constrained by verifier provenance:
  `RecordWithField` and `BridgeTypeName` belong to CLI/JSON,
  `RecordRemoveField` to JSON, `BridgeCommandName` to CLI, and `append_bytes` to
  Linux text policy. Public names, effects, and privilege were not expanded.
- The follow-up repaired product-binary test resolution, nested lexical
  shadowing, genuine append, lazy stream producers, preparation selection,
  container reads/updates, and repeated function-identity resolution. It
  removed superseded Linux-only native policy. The relevant regression owners
  are `tests/stdlib_port.rs`, `tests/xsh/stdlib/`,
  `tests/runtime/collections.rs`, `tests/runtime/streams.rs`, and the catalog
  tests in `src/stdlib.rs`. `map_reads_do_not_copy_the_map` measures `Map`
  reads and updates; it does not establish the same bound for `Record`.

Do not re-port the retained G boundaries or restore required R policy in Rust to
make a benchmark pass. Any change to implementation bindings, private bridge
authority, public signatures, or lazy stream behavior needs matching contract
and regression coverage.

## Acceptance still open

The original baseline is `37e1502ec928fb0bd1194f056e4f21c8e621d80b` (B0);
the pre-follow-up port is `d0bbc6e74fa2d48e90e174bb6a5f0f4281c0bea2`
(B1). On the latest paired macOS release round, **18 of 24 fixed workloads fail
against B0**. The result is unchanged from B1, although the call-heavy rows
improved. The six passes are `cold_trivial`, `cold_quote`, `cold_pad`,
`json_lines_batch`, `native_control`, and `native_hash_control`. Failed rows are:

| Class | Workloads still over budget |
| --- | --- |
| Cold | `cold_cli_parse`, `cold_cli_usage`, `cold_cli_error`, `cold_dynamic_ref` |
| CLI | `cli_small_schema`, `cli_wide_schema`, `cli_repeated_parse` |
| Text | `text_wrap_unicode`, `text_pad_batch`, `fmt_batch` |
| Quoting | `quote_batch`, `quote_edge_cases` |
| MIME and INI | `mime_batch`, `ini_large_record` |
| JSON, environment, checksum, tooling | `json_path_ops`, `env_typed_lookups`, `checksum_batch`, `core_command` |

`bench/stdlib-port/results-followup-b0.json` is the latest B0/candidate pair;
`results-followup-b1.json` is the same candidate against B1. Each contains raw
samples, per-workload medians, fixture and script hashes, binary hashes, budgets,
and skips. These are **one interleaved round** each, not repeated independent
rounds. `results-final.json` and the earlier result files are historical. The
macOS pair skips Linux workloads. It does not qualify Linux performance.

On ARM Linux/musl in the `Dockerfile.test` image, the five R12 entries also
failed: over 200 calls, `linux.meminfo` was 196 versus 2 ms, `linux.modules`
2007 versus 37 ms over a 200-module fixture, `system.memory` 165 versus 2 ms,
`system.os_release` 52 versus 1 ms, and `unix.uptime_seconds` 6 versus a
sub-millisecond reference. These three-round in-process measurements have no
raw-sample JSON in the runner; they need a reproducible runner route and a fresh
qualification. The fixed-path public `system.os_release` fixture cases are in
`tests/stdlib_port.rs` and staged as described in the benchmark README.

The fixed budgets remain `C - B <= max(0.05 * B, 1 ms)` for cold startup and
`C - B <= max(0.10 * B, 2 ms)` for end-to-end work, with matched release builds
on one target. B0 decides acceptance; B1 measures improvement. The three
project-tooling measurements (`xsht api summary`, `xsht check core/ls.xsh`, and
`xsht lint core/ls.xsh`) also need a reproducible route; an earlier standalone
round found the single-file check over budget because it prepares `cli.xsh`.

### Work to finish

1. **Reduce call and stage execution cost.** Profile the failing complete
   workloads, then remove verified per-step dispatch, argument, block, or frame
   overhead in the existing indexed runtime. The function-header cache,
   statement-list reuse, function-identity index, shared container backing,
   consuming list updates, and empty resource-table guard are already present.
   A single-return specialization, statement-list predecode, and word-based
   `Str.wrap` were tried and removed because they did not improve the fixed
   workloads. Preserve evaluation order, Result propagation, captures, trace
   reconstruction, cancellation, small-stack fallback, and private bridge
   provenance. `src/runtime/eval/lowered_run/indexed_run.rs` has a test hook
   comparing the recursive and explicit-frame call routes.
2. **Reduce cold preparation cost.** The four CLI/dynamic cold rows fail; the
   previous phase profile attributed most embedded-module preparation time to
   lowering and verification. Use resolved registry identities when selecting
   dependencies. Native-only programs must prepare none; a genuine
   `module.load` reference must prepare the applicable closure before execution.
   No first-call compilation, filesystem fallback, global executable cache, or
   stale cross-program IDs. Recheck `xshi` submissions, loaded modules, and
   preparation counters after successful and failing runs.
3. **Complete benchmark evidence.** Extend the existing `bench/stdlib-port/`
   runner to record independent rounds, counterbalanced order, dispersion,
   parity status, expected errors, and the three tooling commands. Make the
   Linux R12 runner actually executable in the pinned image with exact fixture
   identity, including the 200-module case; distinguish acquisition, stream
   creation, partial consumption, and full row parsing. Keep the original 24
   workload scripts and budgets fixed. Add diagnostic size sweeps for containers,
   CLI, JSON paths, Unicode wrapping, calls, and stages without changing the
   acceptance workloads. Record toolchain, flags, environment, image, hashes,
   and allocation/memory evidence. Re-run B0 and B1 alongside the candidate.
4. **Qualify G02 across its workload class.** The retained large-file result was
   +2 ms against a 2.8 ms budget for 50 verifications of a 740 KiB file. Test
   tiny files, large files, many small files, and error cases. If a reproduced
   G02 gate fails, apply the gated-group retention rule with its own measurement.
   If revisiting `linux.rfkill_list`, first measure it with its own correct
   fixture; the block-device number is not evidence for that entry.
5. **Pin Record read complexity.** Add a counting-allocator case that reads
   `Record` fields at several sizes and separates construction from traversal.
   The existing map case cannot prove the `Record` invariant.
6. **Run final gates after performance changes.** Follow `docs/TEST-MAP.md` for
   root and product integration, native XSH stdlib/corpus, registry/API,
   `native-tests`, `--no-default-features`, copied-product, and relevant runtime
   control tests. Use the `Dockerfile.test` ARM Linux route for Linux; verify
   fixed-path fixtures and the Linux corpus there. Existing Linux corpus failures
   in the old ledger were reproduced in the reference image and should be
   classified against B0 again if they recur. Report the exact tests run and
   failures, not just an aggregate pass claim.

The port is complete only when required behavior, architecture, tests, and the
**cumulative B0 performance gates** pass. A failed mandatory R workload remains
open within the agreed embedding/runtime design; a failed G prototype retains
its native implementation with a measured reason. Preserve the public API and
native control paths while improving the runtime.
