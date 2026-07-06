# Handoff: `make boot` Progress

## Current State

The original package build failure is fixed.

`make boot` in `../laputa` now gets through:

- baselayout proof/build/install
- xsh proof/build/install
- xinit proof/build/install
- remote package installs through `sudo-rs`
- rootfs image creation

The current failure is later, during VM launch:

```text
error: boot-console-silent: no console output after 5s; see /Users/josh/d/laputa-systems/laputa/target/linux-vm-aarch64/boot.trace
```

Relevant log files from the latest run:

```text
/Users/josh/d/laputa-systems/laputa/target/linux-vm-aarch64/boot.trace
/Users/josh/d/laputa-systems/laputa/target/linux-vm-aarch64/qemu.log
/Users/josh/d/laputa-systems/laputa/target/linux-vm-aarch64/console.log
```

`qemu.log` currently contains:

```text
qemu-watch: line 38: -netdev: command not found
```

`console.log` is empty.

## Fixed This Turn

The stale failure was:

```text
fs-copy-tree: source entry is not copyable: /Users/josh/d/laputa-systems/laputa/./.git/fsmonitor--daemon.ipc
```

That was not a `fs.copy_tree` policy bug. `copy_tree` should remain strict for
sockets/FIFOs/devices. The real bug was that PM executed the package build with
the wrong cwd, so baselayout copied the Laputa checkout root instead of the
staged package source tree.

Root cause: compact lowering of command bare words in statically imported
modules used the arena default source id instead of the command argument's real
source span. In the failing path, `cd src` in an imported module lowered as an
empty format string instead of resolving the `src` parameter.

Fixes made:

- `src/runtime/eval/lower.rs`
  - Threaded `SourceMap` into compact production lowering for top-level,
    module, and function lowering.
  - Added source-context text lookup for command/run arguments.
  - Added a bare-word fallback from the enclosing command argument span, which
    recovers imported-module command words like `src`.

- `tests/runtime/modules.rs`
  - Added `proc_call_from_module_preserves_runtime_cwd`.

After that, the minimized PM repro advanced to:

```text
archive.tar_create expected List[Path]
```

Root cause: the lowered `archive.tar_create` fast path accepted only owned
`LoweredValue::List`, while `sort-by` can produce `LoweredValue::SharedList`.

Fixes made:

- `src/runtime/eval/lowered_run.rs`
  - `lowered_path_list_arg` now accepts both `List` and `SharedList`.

- `tests/runtime/modules.rs`
  - Added `archive_tar_create_accepts_sorted_path_entries`.

## Existing Related Changes

These were already in the worktree before the final fix:

- `src/runtime/eval/lower.rs`
  - Added `check.capture-root-shadow` diagnostics with source labels.
  - Reports function-body bindings that shadow visible module namespace names.

- `src/runtime/eval.rs`
  - Capture-shadow validation is check-only/advisory during normal execution.

- `src/runtime/eval/lowered_run.rs`
  - Capture hydration prefers matching outer runtime bindings.
  - Over-captured immutable slots fall back to kind-agnostic conversion.
  - `module.load` preserves existing parent `Value::Module` bindings.
  - Dynamic module exported functions use a per-module qualified namespace.
  - Stream producer `return Unit` is treated as ending the stream.
  - Added real `linux.chroot` dispatch in the lowered real-Linux branch.

- `src/trace.rs`
  - Tracebacks render stored messages as `error: kind: message` when useful.

- `src/modules/fs.rs`
  - `fs.copy_tree` unsupported-entry errors now include source path and mode.

- `src/modules/linux.rs`
  - Re-exported `chroot` so lowered real-Linux dispatch can call it.

- `tests/runtime/linux.rs`
  - Added `linux_real_chroot_reports_real_error`.

- `tools/repro-dynamic-proc-cwd.xsh`
  - Direct dynamic build-handle repro; succeeds and remains useful as a contrast
    case for the formerly failing statically imported PM path.

Deleted:

- `tools/repro-fs-copy-tree-socket.xsh`
  - No longer useful. It only proved strict `copy_tree` diagnostics; the real
    bug was wrong cwd.

## Verification

Passed:

```sh
cargo check -p xsh
cargo test -p xsh proc_call_from_module_preserves_runtime_cwd
cargo test -p xsh archive_tar_create_accepts_sorted_path_entries
cargo build --bin xsh
```

Passed minimized PM repro:

```sh
/Users/josh/d/laputa-systems/xsh/target/debug/xsh \
  /Users/josh/d/laputa-systems/packages/pm.xsh -- build-prepared-package \
  /Users/josh/d/laputa-systems/packages/repo/baselayout \
  /Users/josh/d/laputa-systems/laputa/target/linux-vm/native-interactive-work/baselayout-1-14/src \
  /Users/josh/d/laputa-systems/laputa/target/repro-pm-build-prepared-dest \
  /Users/josh/d/laputa-systems/laputa/target/repro-pm-build-prepared.tar.gz
```

Latest `make boot`:

```sh
cd /Users/josh/d/laputa-systems/laputa
make boot
```

Result: package build/install and rootfs image creation pass; VM launch fails
with silent console and `qemu-watch: line 38: -netdev: command not found`.

## Useful Next Step

Investigate the VM launch wrapper in `../laputa`, starting from the generated
`qemu-watch` script or the code that writes it. The current symptom looks like
the QEMU executable/argv prefix is missing, causing the first QEMU argument
(`-netdev`) to be executed as a shell command.

Suggested prompt:

```text
Continue from HANDOFF.md in /Users/josh/d/laputa-systems/xsh. The XSH package-build cwd bug is fixed and the minimized PM repro passes. `make boot` now gets through package builds/install and rootfs image creation, then fails at VM launch with `boot-console-silent`; qemu.log says `qemu-watch: line 38: -netdev: command not found` and console.log is empty. Please inspect the Laputa boot/qemu-watch generation path, fix the missing QEMU executable/argv prefix, and rerun `make boot`. Keep the existing XSH changes scoped; do not loosen `fs.copy_tree` semantics.
```
