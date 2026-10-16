# Process operand source-order repair

The owner chose the SPEC's source-order contract on 2026-10-16. The repair
is integrated as `68cbed2b4`, with runtime-test correction `877ae06a9` and
generated SPEC update `106ef25fa`.

The qualified lane source is `50f54522cfe512417dbe67665c6e70c9fb770d74`, tree
`d61c6ad1b50b21e9ddd4b0c86f9203d1655bd7fc`. The integrator confirmed that
production source under `src/` and `crates/` matches the integrated source.
Independent source review found no remaining contract defect.

The public run AST replaces fixed option fields with an ordered option range
and an ordered argument/redirection tail. The mandatory target remains inline.
Flat arena pools retain typed references to arguments and redirections. Packed
IR uses `LoweredRunOperands`, shared by capture, pipeline, spawn, and the command
builder. The runtime borrows one cursor and converts each value before advancing.
Nonpositive dynamic cpumax fails at that operand with the existing `cpu-max`
error. AST traversal, lint traversal, and formatting preserve written order.

The corrected [regression fails on e048](baseline-failure.log): only `target`
prints before empty-target conversion fails. The repaired source evaluates
`accept`, `timeout`, `cpu`, `env`, then `target`, and skips the later argument.
An older test used `*${empty}`, which is not splice syntax, and expected checker
exit 2. That test was wrong for runtime conversion. It now uses a computed
`List[Str]` with `@(empty_target())` and observes the runtime target marker,
`empty-command`, and skipped later argument.

## Verification

The pinned Linux musl image is
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
The clean lane's [verification build](product-build.stderr) passes:

```text
cargo build --locked --offline --profile verification --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht
```

The build uses kache, mold, four CPUs, 8 GiB, no swap, and no network.
Qualified product hashes are:

| Product | SHA-256 |
|---|---|
| xsh | `d817ed7e8e43e5b7db5670068eb6fe76186178416233360aa535a5d07b5917af` |
| xsht | `f2efdc4b440b3c12cfc79ad803c94fa9179164cde9563d3b74c856c0f10c8693` |

The lane ran `xsht test tests/xsh/stdlib/process_frames.xsh -j 1` and the same
command with `--force-frames`: nine tests pass in each mode. The two exact
`stdlib/process.xsh` controls, `test_process_command_construction_preserves_operand_order`
and `test_process_command_argv_evaluates_named_operands_in_written_order`, also
pass in each mode. Those outputs were returned directly by the tools and were
not saved as raw files.

The integrator independently reran the corrected regression in both modes on
the clean lane, under the shared native-suite lock, with pinned Docker, two
CPUs, 4 GiB, no swap, no network, read-only source/products, and owned scratch.
[Both raw results pass](root-check.stdout). Source identity and product hashes
remain unchanged. The focused verifier mutation test, compact arena layout
test, and 60 XSHT syntax tests also pass in the lane.

The integrator ran `docs.generate` with these verification products. Only
`docs/SPEC.md` changed, matching its edited template. No formatter ran.

## Bounded memory observation

The same [88-byte fixture](arena-fixture.xsh) has two run segments.
[Baseline](baseline-arena.json) and [candidate](candidate-arena.json) report
3,378 and 3,442 retained AST bytes, an increase of 64 bytes. Segment structs
are 60 and 48 bytes. Their table capacity is four in both runs, using 240 and
192 bytes. The two new pools each hold four eight-byte entries, using 32 bytes
each, and add 48 bytes of Vec descriptors. These are AST retention counts for
this fixture, not RSS or a general memory result. No timed performance gate
or full native suite ran for this repair. Final source qualification remains
open.
