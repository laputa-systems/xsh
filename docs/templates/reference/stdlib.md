# Standard Library Index

One line per module function, method, and record, from `xsht api`. Run
`xsht api api:MODULE.FUNCTION`, `xsht api method:TYPE.NAME`, or
`xsht api record:NAME` for the full contract, effects, and examples. Internal
host-support modules are omitted.

## Modules
{{range .stdlib.modules}}
### `{{.name}}`

{{.summary}}
{{range .functions}}
- `{{.signature}}` — {{.summary}}
{{- end}}
{{end}}
## Methods
{{range .stdlib.receivers}}
### {{.name}}
{{range .methods}}
- `{{.signature}}` — {{.summary}}
{{- end}}
{{end}}
## Records
{{range .stdlib.records}}
- `{{.signature}}` — {{.summary}}
{{- end}}
