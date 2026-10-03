const source = """
  upstream {{.name}} {
  {{range .servers}}    server {{.host}}:{{.port}}{{if .backup}} backup{{end}};
  {{end}}}
  """

let conf = template.render(
  source,
  {
    name: "api",
    servers: [
      {host: "10.0.0.11", port: 8080, backup: false},
      {host: "10.0.0.12", port: 8080, backup: true},
    ],
  },
)?

print $conf
