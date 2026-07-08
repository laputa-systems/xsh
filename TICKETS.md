# Tickets

Narrow, actionable bugs observed across the xsh runtime and standard modules.
Each entry describes the symptom, a minimal reproduction, the likely area, and
any known workarounds. When a ticket is resolved, delete it.

## Open

### process.command_argv argv should accept Path values

**Symptom:** `process.command_argv` accepts a `Path` executable and `Path`
fields such as `cwd`, `stdin`, `stdout`, and `stderr`, but every argv element
must still be a `Str`. Scripts that already model filesystem values as `Path`
have to add `.display()` at each argv use, which weakens path ergonomics in the
most common subprocess API.

**Minimal reproduction:**

```xsh
proc main() [process, error] -> Result[Unit] {
  let marker: Path = /tmp/marker
  let command = process.command_argv("echo", ["echo", marker])
  let _ = process.run(command)?
}

main()?
```

Current checker error:

```text
expected Str, found Path
```

**Expected direction:** allow argv words to be `Str|Path` and lower `Path`
values losslessly to argv bytes at the process boundary, matching existing
`process.command_argv(target: Str|Path, ...)` behavior. This should also apply
to `process.command` run entries if the builder path has the same limitation.
