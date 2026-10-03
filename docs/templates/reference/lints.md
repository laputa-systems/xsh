# Lint Catalog

Every code `xsht lint` reports, from `xsht lint --list`. Pass any of them to
`xsht lint --only CODE[,CODE...]` to limit reporting and fixing to those rules.
The `check.*` codes are checker diagnostics that carry a fix.

| Code | Summary |
|---|---|
{{- range .lints}}
| `{{.code}}` | {{.summary}} |
{{- end}}
