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
### Common workflows

```text
{{.cli.workflows}}
```
