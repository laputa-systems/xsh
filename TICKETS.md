# Tickets

Narrow, actionable bugs observed across the xsh runtime and standard modules.
Each entry describes the symptom, a minimal reproduction, the likely area, and
any known workarounds. When a ticket is resolved, delete it.

## Open

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

