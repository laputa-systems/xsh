type ParsedArgs = {count: Int, define: List[Str], file: Path, verbose: Bool}

type CommandArgs = {command: Str, root: Path, raw: List[Str], action: Str}

type AdvancedArgs = {
  mode: Str,
  color: Str,
  config: Path,
  workspace: Path,
  timeout: Duration,
  count: Int,
  verbose: Bool,
  json: Bool,
  table: Bool,
  output: Str,
}

type CommandOptions = {command: Str, action: Str, root: Path, verbose: Bool, rest: List[Str]}

proc test_args_parse_tokens_and_commands() [error] {
  let parsed: ParsedArgs = cli.parse(
    ["--count", "3", "-D", "one", "-Dtwo", "--verbose", "src/main.xsh"],
    {
      count: {kind: "Int", required: true},
      define: {kind: "Str", repeated: true, short: ["D"]},
      file: {kind: "Path", positional: true},
      verbose: "Bool",
    },
  )?

  test.eq(parsed.count, 3)?
  test.eq(parsed.define.join(","), "one,two")?
  test.eq(parsed.file.name(), "main.xsh")?
  test.ok(parsed.verbose)?
  let tokens = cli.tokens(["-abc", "--output=result.txt", "-I", "include", "-1", "file"], ["I", "output"])?
  test.eq(tokens[0].kind, "short")?
  test.eq(tokens[0].name, "a")?
  test.eq(tokens[3].value, "result.txt")?
  test.eq(tokens[4].value, "include")?
  test.eq(tokens[5].kind, "operand")?
  test.eq(tokens[5].name, "-1")?

  let full = cli.parse_full(
    ["--count", "2", "demo.txt"],
    {count: {kind: "Int", required: true}, file: {kind: "Path", positional: true}},
  )?

  test.eq(full.values.count, 2)?
  test.eq(full.sources.count, "argv")?
  let usage = cli.usage({count: {kind: "Int", required: true}}, "demo")
  test.contains(usage, "usage: demo")?

  let command_specs = {
    build: {positionals: ["root"], types: {root: "Path"}, rest: "raw"},
    clean: {positionals: ["root"], types: {root: "Path"}, rest: "raw"},
  }

  let command: CommandArgs = cli.commands(
    ["deploy", "target/demo", "--dry-run"],
    rootless_default: "build",
    commands: command_specs,
    fallback_command: {positionals: ["action", "root"], types: {root: "Path"}, rest: "raw", command_like: true},
  )?

  test.eq(command.command, "deploy")?
  test.eq(command.root.name(), "demo")?
  test.eq(command.raw[0], "--dry-run")?
  let explicit: CommandArgs = cli.commands(["clean", "target/demo"], command_specs)?
  test.eq(explicit.command, "clean")?
  test.eq(explicit.root.name(), "demo")?
}

proc test_cli_parse_compact_forms() [error] {
  let parsed: ParsedArgs = cli.parse(
    ["--total", "3", "-D", "one", "-Dtwo", "--verbose", "src/main.xsh"],
    {
      count: {form: "--total N", default: 0},
      define: {form: "-D NAME", repeated: true},
      file: {form: "FILE", default: p"."},
      verbose: {form: "-v --verbose", default: false},
    },
  )?

  test.eq(parsed.count, 3)?
  test.eq(parsed.define.join(","), "one,two")?
  test.eq(parsed.file.name(), "main.xsh")?
  test.ok(parsed.verbose)?
  let cli_tokens = cli.tokens(["--mode=json", "-v"], ["mode"])?
  test.eq(cli_tokens[0].name, "mode")?
  test.eq(cli_tokens[0].value, "json")?
}

proc test_cli_parse_advanced_descriptors() [fs, error] {
  let root_handle = fs.tempdir()?
  defer fs.close_root(root_handle)?
  let root = fs.root_path(root_handle)?
  let config = fp"${root}/config.toml"
  config.write("ready")?

  let schema = {
    mode: {form: "--mode MODE", default: "text", choices: ["text", "json"]},
    color: {form: "--color[=WHEN]", default: "auto", optional_default: "always", choices: ["auto", "always", "never"]},
    config: {form: "--config PATH", kind: "Path", file: true, default: config},
    workspace: {form: "--workspace DIR", kind: "Path", default: root},
    timeout: {form: "--timeout DURATION", default: 1s, positive: true},
    count: {form: "--count N", kind: "UInt", default: 1, min: 1},
    verbose: {form: "-v --verbose", default: false, deprecated: "use --log-level instead"},
    json: {form: "--json", default: false, conflicts: "table"},
    table: {form: "--table", default: false},
    output: {form: "--output PATH", default: "", requires: "mode"},
    secret: {form: "--secret VALUE", default: "", hidden: true},
    left: {form: "--left VALUE", required_group: "input"},
    right: {form: "--right VALUE", required_group: "input"},
  }

  let full = cli.parse_full(["--color", "-v", "--left", "a"], schema)?
  let values: AdvancedArgs = full.values
  test.eq(values.color, "always")?
  test.eq(values.config.name(), "config.toml")?
  test.eq(values.workspace.name(), root.name())?
  test.eq(full.values.count, 1)?
  test.eq(f"${values.timeout}", "1s")?
  test.ok(values.verbose)?
  test.eq(full.sources.color, "argv")?
  test.eq(full.sources.mode, "default")?
  test.eq(full.warnings.len(), 1)?

  let env_full = cli.parse_full(
    [],
    {profile: {form: "--profile NAME", default: "dev", env: "XSH_PROFILE"}},
    {XSH_PROFILE: "prod"},
  )?

  test.eq(env_full.values.profile, "prod")?
  test.eq(env_full.sources.profile, "env")?
  let usage = cli.usage(schema, "demo")
  test.ok("usage: demo [OPTIONS]" in usage)?
  test.ok("--mode MODE" in usage)?
  test.ok("-h, --help" in usage)?
  test.ok(! ("--secret" in usage))?

  match cli.parse(["--help"], schema, "demo sub") {
    Ok(_) => test.fail("implicit help should stop parsing")?
    Err(error) => test.ok("usage: demo sub [OPTIONS]" in error.message)?
  }

  match cli.parse(["--mode", "xml", "--left", "a"], schema) {
    Ok(_) => test.fail("choice validation should fail")?
    Err(error) => test.ok("expects one of" in error.message)?
  }

  match cli.parse(["--json", "--table", "--left", "a"], schema) {
    Ok(_) => test.fail("conflict validation should fail")?
    Err(error) => test.ok("conflicts" in error.message)?
  }

  match cli.parse([], schema) {
    Ok(_) => test.fail("required group validation should fail")?
    Err(error) => test.ok("required group" in error.message)?
  }

  match cli.parse(["--count", "-1", "--left", "a"], schema) {
    Ok(_) => test.fail("UInt validation should fail")?
    Err(error) => test.ok("expects UInt" in error.message)?
  }

  match cli.parse(["--config", f"${root}/missing.toml", "--left", "a"], schema) {
    Ok(_) => test.fail("file path validation should fail")?
    Err(error) => test.ok("expects a file path" in error.message)?
  }
}

proc test_cli_commands_accept_aliases_forms_and_options() [error] {
  let command: CommandOptions = cli.commands(
    ["b", "--verbose", "target/demo", "--", "--dry-run"],
    {
      build: {
        aliases: ["b"],
        form: "build ROOT ...REST",
        types: {root: "Path"},
        options: {verbose: {form: "-v --verbose", default: false}},
      },
    },
  )?

  test.eq(command.command, "build")?
  test.eq(command.action, "b")?
  test.eq(command.root.name(), "demo")?
  test.ok(command.verbose)?
  test.eq(command.rest[0], "--dry-run")?
}
