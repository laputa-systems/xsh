# Command-Line Reference

The help text of `xsh` and every `xsht` command, as the binaries print it.
Tool behavior and configuration are documented in [XSHT](../XSHT.md);
language and interactive behavior are specified in [SPEC](../SPEC.md) and
[the interactive SPEC](../SPEC-INTERACTIVE.md).

## `xsh`

```text
{{.cli.xsh}}
```

## `xsht`

```text
{{.cli.xsht}}
```
{{range .cli.commands}}
### `xsht {{.name}}`

```text
{{.help}}
```
{{end}}
The `xsht trace` function table ranks calls by self time in microseconds. `total`
adds the inclusive duration of every call; `outer` counts recursive activations
only at their outermost call. Percentiles and maximum remain per-call inclusive
durations. Self time excludes traced children. JSON summaries expose these as
`self_us` and `outer_total_us`.

Flamegraph format emits folded stacks with each timed frame's self time, including
script, proc, pure, and operation frames. Their values sum to traced root time.

### Common workflows

```text
{{.cli.workflows}}
```
