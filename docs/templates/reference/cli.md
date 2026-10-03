# Command-Line Reference

The help text of `xsh` and every `xsht` command, as the binaries print it.
`docs/SPEC.md` §16 describes what the commands guarantee.

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
