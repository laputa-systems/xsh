let unit = template.render(
  """[Service]
ExecStart={{.exec}}
{{- range $key, $value := .env}}
Environment={{$key}}={{$value}}
{{- end}}
""",
  {exec: "/usr/bin/web --port 8080", env: {MODE: "prod", LOG: "info"}},
)?
print $unit
