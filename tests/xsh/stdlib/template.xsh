type TemplateService = {name: Str, exec: Str, user: Str, after: List[Str], env: Map[Str], restart: Bool}

# The error message of a failed render, or the rendered text prefixed with `ok:`.
pure render_outcome(result: Result[Str]) -> Str {
  match result {
    Ok(text) => f"ok:{text}"
    Err(error) => error.message
  }
}

test test_template_fields_and_dot {
  let data = {name: "web", owner: {team: {lead: "ada"}}, port: 8080, ratio: 0.5, on: true}
  assert template.render("plain text", data)? == "plain text"
  assert template.render("", data)? == ""
  assert template.render("{{.name}}:{{.port}}", data)? == "web:8080"
  assert template.render("{{.owner.team.lead}}", data)? == "ada"
  assert template.render("{{ .ratio }} {{.on}}", data)? == "0.5 true"
  assert template.render("{{.}}", "scalar")? == "scalar"
  assert template.render("{{$}}", 42)? == "42"
  assert template.render("{{$.name}}", data)? == "web"
  assert template.render("{{.a}}", json.decode("{\"a\":\"from json\"}")?)? == "from json"
  assert template.render("{{\"literal\"}} {{`raw \\n`}} {{-3}}", data)? == "literal raw \\n -3"
  assert template.render("{{\"a\\tb\\\"c\\\\\"}}", data)? == "a\tb\"c\\"
  assert template.render("{{\"}}\"}}", data)? == "}}"
  let typed: Map[Int] = {b: 2, a: 1}
  assert template.render("{{.a}}{{.b}}", typed)? == "12"
}

test test_template_if_truthiness_and_else_chains {
  let falsy = {f: false, n: null, zero: 0, real: 0.0, text: "", list: [], map: {}}
  for key in falsy.keys() {
    assert template.render(f"{{{{if .{key}}}}}T{{{{else}}}}F{{{{end}}}}", falsy)? == "F", key
  }
  let truthy = {t: true, one: -1, real: 0.1, text: " ", list: [0], map: {a: null}}
  for key in truthy.keys() {
    assert template.render(f"{{{{if .{key}}}}}T{{{{else}}}}F{{{{end}}}}", truthy)? == "T", key
  }
  let source = "{{if .a}}A{{else if .b}}B{{else if .c}}C{{else}}none{{end}}"
  assert template.render(source, {a: 1, b: 1, c: 1})? == "A"
  assert template.render(source, {a: 0, b: 1, c: 1})? == "B"
  assert template.render(source, {a: 0, b: 0, c: 1})? == "C"
  assert template.render(source, {a: 0, b: 0, c: 0})? == "none"
  assert template.render("{{if .a}}A{{end}}", {a: false})? == ""
}

test test_template_range_lists_maps_and_variables {
  let data = {ports: [80, 443], env: {ZED: "z", ALPHA: "a"}, none: [], missing: null}
  assert template.render("{{range .ports}}[{{.}}]{{end}}", data)? == "[80][443]"
  assert template.render("{{range $port := .ports}}{{$port}};{{end}}", data)? == "80;443;"
  assert template.render("{{range $i, $port := .ports}}{{$i}}={{$port}} {{end}}", data)? == "0=80 1=443 "
  assert template.render("{{range $key, $value := .env}}{{$key}}={{$value}},{{end}}", data)? == "ALPHA=a,ZED=z,"
  assert template.render("{{range .env}}{{.}}{{end}}", data)? == "az"
  assert template.render("{{range .none}}x{{else}}empty{{end}}", data)? == "empty"
  assert template.render("{{range .missing}}x{{else}}null is empty{{end}}", data)? == "null is empty"
  assert template.render("{{range .ports}}{{$.env.ALPHA}}{{end}}", data)? == "aa"
  let nested = {groups: [{name: "a", members: ["x", "y"]}, {name: "b", members: []}]}
  let source = "{{range .groups}}{{.name}}:{{range .members}}{{.}}{{else}}-{{end}};{{end}}"
  assert template.render(source, nested)? == "a:xy;b:-;"
}

test test_template_with_comments_and_trim_markers {
  let data = {server: {host: "db", port: 5432}, absent: null}
  assert template.render("{{with .server}}{{.host}}:{{.port}}{{end}}", data)? == "db:5432"
  assert template.render("{{with $s := .server}}{{$s.host}}{{end}}", data)? == "db"
  assert template.render("{{with .absent}}x{{else}}fallback{{end}}", data)? == "fallback"
  assert template.render("a{{/* note */}}b", data)? == "ab"
  assert template.render("a {{- /* note */ -}} b", data)? == "ab"
  assert template.render("a\n  {{- .server.host -}}  \n b", data)? == "adbb"
  assert template.render("x {{- \" y \" }} z", data)? == "x y  z"
  assert template.render("{{-3}} {{- -3 -}} 4", data)? == "-3-34"
}

test test_template_pipeline_functions {
  let data = {name: "Web", words: ["a", "b"], ports: [80, 443], empty: "", n: null, port: 8080, cfg: json.decode("{\"b\":[1,true],\"a\":\"x\"}")?}
  assert template.render("{{.name | upper}} {{.name | lower}} {{\"  pad \" | trim}}", data)? == "WEB web pad"
  assert template.render("{{len .words}} {{.name | len}} {{len .cfg}} {{len .}} {{len \"é\"}}", data)? == "2 3 2 7 1"
  assert template.render("{{join \", \" .words}} {{.ports | join \":\"}}", data)? == "a, b 80:443"
  assert template.render("{{.empty | default \"none\"}} {{.n | default 7}} {{.name | default \"x\"}}", data)? == "none 7 Web"
  assert template.render("{{.name | quote}} {{.port | quote}} {{\"a\\\"b\" | quote}}", data)? == "\"Web\" \"8080\" \"a\\\"b\""
  assert template.render("{{json .cfg}} {{.words | json}} {{json .n}}", data)? == "{\"a\":\"x\",\"b\":[1,true]} [\"a\",\"b\"] null"
  assert template.render("{{if eq .name \"Web\"}}eq{{end}} {{if ne .port 80}}ne{{end}} {{if eq .n null}}null{{end}}", data)? == "eq ne null"
  assert template.render("{{if not .empty}}not{{end}} {{and .name .port}} {{or .empty \"alt\"}} {{and .empty 1 | quote}}", data)? == "not 8080 alt \"\""
}

test test_template_define_and_template_calls {
  let source = """{{define "entry"}}<{{.}}>{{end}}
{{- define "list"}}{{range .}}{{template "entry" .}}{{end}}{{end}}
{{- template "list" .items}}|{{template "header"}}
{{- define "header"}}H{{end}}"""
  assert template.render(source, {items: ["a", "b"]})? == "<a><b>|H"
  let looping = "{{define \"loop\"}}{{template \"loop\" .}}{{end}}{{template \"loop\" 1}}"
  let message = render_outcome(template.render(looping, null))
  assert "template calls nest deeper than 64 levels" in message, message
}

test test_template_render_errors_report_kind_and_position {
  let data = {name: "web", list: [1], n: null, map: {a: 1}}
  test.error_kind(template.render("{{.missing}}", data), "template-render")?
  assert render_outcome(template.render("line one\n  {{.missing}}", data)) == "template:2:5: missing field `missing`"
  assert render_outcome(template.render("é {{.name.first}}", data)) == "template:1:5: cannot read field `first` of Str"
  assert render_outcome(template.render("{{.n}}", data)) == "template:1:1: cannot render null; use `default` to supply a value"
  assert render_outcome(template.render("{{.list}}", data)) == "template:1:1: cannot render a List directly; use `join` or `json`"
  assert render_outcome(template.render("{{range .name}}{{end}}", data)) == "template:1:1: cannot range over Str"
  assert render_outcome(template.render("{{$nope}}", data)) == "template:1:3: undefined variable `$nope`"
  assert render_outcome(template.render("{{.list | upper}}", data)) == "template:1:11: `upper` expects Str, found List"
  assert render_outcome(template.render("{{if eq .name 1}}{{end}}", data)) == "template:1:6: cannot compare Str with Int"
  assert render_outcome(template.render("{{len 3}}", data)) == "template:1:3: `len` expects Str, List, or Map, found Int"
  # Missing fields are errors even inside conditions; supply null for optional data.
  test.error_kind(template.render("{{if .optional}}x{{end}}", data), "template-render")?
}

test test_template_syntax_errors_report_kind_and_position {
  let data = {a: 1}
  test.error_kind(template.render("{{.a", data), "template-syntax")?
  assert render_outcome(template.render("ok\n {{.a", data)) == "template:2:2: unclosed action; expected `}}`"
  assert render_outcome(template.render("{{if .a}}x", data)) == "template:1:1: unclosed `{{if}}`; expected `{{end}}`"
  assert render_outcome(template.render("{{end}}", data)) == "template:1:1: unexpected `{{end}}`"
  assert render_outcome(template.render("{{else}}", data)) == "template:1:1: unexpected `{{else}}`"
  assert render_outcome(template.render("{{if .a}}{{else}}{{else}}{{end}}", data)) == "template:1:18: `{{else}}` after the final `{{else}}`"
  assert render_outcome(template.render("{{ shell .a }}", data)) == "template:1:4: unknown function `shell`"
  assert render_outcome(template.render("{{upper}}", data)) == "template:1:3: `upper` takes 1 argument(s), found 0"
  assert render_outcome(template.render("{{.a .a}}", data)) == "template:1:6: only functions take arguments"
  assert render_outcome(template.render("{{.a | .a}}", data)) == "template:1:8: only functions can receive a piped value"
  assert render_outcome(template.render("{{}}", data)) == "template:1:1: empty action"
  assert render_outcome(template.render("{{/* open", data)) == "template:1:1: unclosed comment; expected `*/}}`"
  assert render_outcome(template.render("{{len (.a)}}", data)) == "template:1:7: parentheses are not supported"
  assert render_outcome(template.render("{{$x := .a}}", data)) == "template:1:1: variables can only be declared by `range` and `with`"
  assert render_outcome(template.render("{{template \"nope\"}}", data)) == "template:1:12: no template named `nope`"
  assert render_outcome(template.render("{{define \"a\"}}{{end}}{{define \"a\"}}{{end}}", data)) == "template:1:31: template `a` is already defined"
  assert render_outcome(template.render("{{\"open}}", data)) == "template:1:3: unterminated string literal"
  assert render_outcome(template.render("{{.a.}}", data)) == "template:1:5: expected a field name after `.`"
  # The whole template parses first, so an error in an unused branch still fails.
  assert render_outcome(template.render("{{if false}}{{nope}}{{end}}", data)) == "template:1:15: unknown function `nope`"
}

test test_template_renders_a_systemd_unit_from_a_record {
  let service = TemplateService(
    name: "web",
    exec: "/usr/bin/web --port 8080",
    user: "www",
    after: ["network-online.target", "postgresql.service"],
    env: {RUST_LOG: "info", MODE: "prod"},
    restart: true,
  )
  let unit = template.render(
    """# Generated for {{.name}}; do not edit.
[Unit]
Description={{.name}} service
After={{join " " .after}}

[Service]
User={{.user}}
ExecStart={{.exec}}
{{- range $key, $value := .env}}
Environment={{$key}}={{$value | quote}}
{{- end}}
{{- if .restart}}
Restart=on-failure
{{- end}}
""",
    service,
  )?
  assert unit == """# Generated for web; do not edit.
[Unit]
Description=web service
After=network-online.target postgresql.service

[Service]
User=www
ExecStart=/usr/bin/web --port 8080
Environment=MODE="prod"
Environment=RUST_LOG="info"
Restart=on-failure
"""
}

test test_template_renders_nginx_upstreams_from_json {
  let config = json.decode(
    "{\"sites\":[{\"host\":\"a.example\",\"backends\":[\"10.0.0.1:80\",\"10.0.0.2:80\"],\"tls\":true},{\"host\":\"b.example\",\"backends\":[],\"tls\":false}]}",
  )?
  let source = """{{range .sites -}}
server {
    server_name {{.host}};
    listen {{if .tls}}443 ssl{{else}}80{{end}};
{{- range .backends}}
    proxy_pass http://{{.}};
{{- else}}
    return 503;
{{- end}}
}
{{end -}}"""
  assert template.render(source, config)? == """server {
    server_name a.example;
    listen 443 ssl;
    proxy_pass http://10.0.0.1:80;
    proxy_pass http://10.0.0.2:80;
}
server {
    server_name b.example;
    listen 80;
    return 503;
}
"""
}

test test_template_large_range_renders_every_item {
  var rows = []
  var index = 0
  while index < 1000 {
    rows += [index]
    index += 1
  }
  let rendered = template.render("{{range $i, $v := .}}{{if $i}},{{end}}{{$v}}{{end}}", rows)?
  assert rendered.split(",").len() == 1000
  assert rendered.ends_with(",998,999")
}
