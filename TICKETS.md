# Tickets

Narrow, actionable bugs observed across the xsh runtime and standard modules.
Each entry describes the symptom, a minimal reproduction, the likely area, and
any known workarounds. When a ticket is resolved, delete it.

## Greppable implementation handles

| Ticket area | Symbols | Owner and coverage |
|---|---|---|
| filesystem-bound `par-map` | `eval_indexed_par_map_item`, `eval_indexed_par_map_parallel`, `fs.files`, `fs.walk` | `src/runtime/eval/lowered_run/indexed_run.rs`, filesystem module dispatch; runtime stream tests |
| direct directory enumeration | `lower_fs_files_args`, `fs.files`, `fs.walk`, `CompactBodyProbe` | `src/runtime/eval/lower.rs`, `src/modules/fs.rs`, `src/sema/check/compact.rs`; `tests/runtime/collections.rs`, `tests/runtime/streams.rs` |
| macOS process-tree path | `process.list`, `ProcessStatus`, `core/pstree.xsh` | `src/modules/process.rs`, `src/runtime/process.rs`, `core/pstree.xsh`; `core_pstree_prints_spawned_parent_before_child` |
| cancellation responsiveness | `run_cancelable_temp_script`, `cancel_managed`, `CancellationDecision` | `tests/runtime/common.rs`, `src/runtime/process.rs`; process and OS cancellation tests |

Treat these as issue-to-owner handles. Update the nearest behavior test and
`docs/TEST-MAP.md` when a ticket changes runtime or module behavior.

## Open

### Reduce `par-map` overhead for filesystem-bound workloads

**Symptom**

The Linux Kbuild local-record prototype scans 629 active directories through
XSH `par-map`. On the macOS arm64 Docker path, forced-cold discovery remains
roughly 38–41 seconds, and changing the worker count from 1 through 64 does
not materially change the result. This is an observation, not yet proof of a
runtime defect: the workload may be dominated by filesystem latency or worker
serialization.

**Desired behavior**

Independent XSH tasks that read and parse separate files should obtain useful
parallel speedup when the host has available CPU and I/O capacity, without
large per-worker startup, serialization, or result-collection overhead.

**Likely area**

`par-map` lowering and worker scheduling, especially closure/result
serialization and filesystem effects across Docker or other mounted
filesystems. The investigation should first distinguish scheduler overhead
from the underlying filesystem using a standalone benchmark.

**Minimal reproduction**

From the Laputa checkout, after the source tree is warm:

```sh
make linux-plan-only PKGNAME=linux \
  XSH_LINUX_KBUILD_LOCAL_RECORDS=1 \
  XSH_LINUX_KBUILD_FORCE_DISCOVER=1 \
  XSH_LINUX_KBUILD_DISCOVER_JOBS=1

make linux-plan-only PKGNAME=linux \
  XSH_LINUX_KBUILD_LOCAL_RECORDS=1 \
  XSH_LINUX_KBUILD_FORCE_DISCOVER=1 \
  XSH_LINUX_KBUILD_DISCOVER_JOBS=8
```

Compare the `linux-kbuild-timing-done discover` values and CPU utilization.
Require identical complete plans before comparing timings.

### Provide efficient direct-directory entry enumeration

**Symptom**

XSH scripts that must honor a two-file precedence rule, such as Linux
`Kbuild` over `Makefile`, currently choose between repeated `exists`/read
probes or a recursive `fs.files` index. The recursive index is too broad for
this use, while the repeated probes are costly across mounted source trees.

**Desired behavior**

Expose a standard, non-recursive directory-entry operation that can enumerate
one directory's names and kinds with optional metadata. A script should be
able to select the preferred file and read exactly one source file while
retaining ordinary XSH error propagation.

**Likely area**

The standard `fs` module and its filesystem lowering. Preserve the existing
`fs.files`/`fs.walk` semantics; this ticket is for a narrow direct-directory
operation rather than a Linux-specific API.

**Minimal reproduction**

The Linux source reader in `packages/repo/linux/kbuild.xsh` must probe
`Kbuild` and `Makefile` for hundreds of active directories. Compare its
forced-cold timing with a prototype using a direct-entry operation, and verify
that Kbuild precedence and complete plan equivalence are unchanged.

### Optimize the slow macOS `pstree` process listing path

**Symptom**

`unix::core_pstree_without_root_prints_visible_roots` takes approximately 16
seconds on macOS. Running `target/debug/xsh core/pstree.xsh` directly shows the
same cost, so the Rust assertion is not the bottleneck.

**Desired behavior**

The default `pstree` view should enumerate and render the host process tree in
well under a second for ordinary developer machines.

**Likely area**

`process.list()` on macOS and the collection-heavy process grouping and parent
lookup logic in `core/pstree.xsh`. The current implementation loads the entire
process table and performs substantial interpreted sorting, grouping, and
scanning.

**Minimal reproduction**

```sh
time target/debug/xsh core/pstree.xsh
cargo test --test runtime unix::core_pstree_without_root_prints_visible_roots
```

**Possible directions**

- Reduce per-process work in the macOS `process.list()` implementation.
- Provide an indexed or purpose-built process-tree operation for `pstree`.
- Optimize the XSH grouping, sorting, and parent lookup operations used by the
  applet.

### Long-running XSH scripts should respond promptly to Ctrl-C and SIGTERM

**Symptom**

A long-running package checksum refresh did not stop when interrupted from the
terminal with Ctrl-C, and also did not exit after `kill -TERM` was sent to both
the parent `make update-checksums` process and the child `xsh pm.xsh --
update-checksums ...` process. It had to be terminated with `kill -KILL`.

Observed command shape from `packages`:

```sh
make update-checksums
```

which was running:

```sh
xsh pm.xsh -- update-checksums repo/alsa-lib repo/alsa-ucm-conf ...
```

The script was doing network/source checksum work and package file rewrites
serially when the interrupt was attempted.

**Desired behavior**

XSH should treat terminal interrupt and termination signals as cancellation
requests for the running script, unwind promptly, and exit non-zero. If a child
process or blocking host operation is active, the runtime should either
interrupt it or return control once the operation reports interruption. Defers
should still run where the runtime can safely unwind.

**Likely area**

Signal handling and cancellation in the runtime/process path:

- top-level runner signal handling;
- blocking host operations such as network downloads, filesystem work, and
  child process waits;
- propagation of cancellation through `run`, `net.download`, and ordinary
  script evaluation.

**Minimal reproduction**

Use a script that performs a long blocking host operation or loop, start it from
an interactive terminal, then press Ctrl-C:

```xsh
proc main() [time, error] {
  while true {
    time.sleep(10s)?
  }
}

main()?
```

Also test a process-backed operation:

```xsh
proc main() [process, error] {
  run sleep 300 ?
}

main()?
```

Both should exit promptly on Ctrl-C and SIGTERM.

**Workaround**

Use `kill -KILL` on the stuck `xsh` process. Callers such as Make targets can
reduce exposure by running smaller independent XSH invocations, so a single
stuck script holds less work.
