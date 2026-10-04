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

type HeadAppletOptions = {count: Int, files: List[Str]}

type SortAppletOptions = {numeric: Bool, reverse: Bool, key: Int, delimiter: Str, files: List[Str]}

type FdAppletOptions = {hidden: Bool, no_ignore: Bool, extensions: List[Str], excludes: List[Str], operands: List[Str]}

type RgAppletOptions = {color: Str, pattern: Str, globs: List[Str], roots: List[Str]}

type CpAppletOptions = {no_clobber: Bool, force: Bool, target: Path?, operands: List[Str]}

test test_args_parse_tokens_and_commands {
  let parsed: ParsedArgs = cli.parse(
    ["--count", "3", "-D", "one", "-Dtwo", "--verbose", "src/main.xsh"],
    {
      count: {
        kind: "Int",
        required: true,
      },
      define: {
        kind: "Str",
        repeated: true,
        short: [
          "D",
        ],
      },
      file: {
        kind: "Path",
        positional: true,
      },
      verbose: "Bool",
    },
  )?

  assert parsed.count == 3
  assert parsed.define.join(",") == "one,two"
  assert parsed.file.name() == "main.xsh"
  assert parsed.verbose
  let tokens = cli.tokens(["-abc", "--output=result.txt", "-I", "include", "-1", "file"], ["I", "output"])?
  assert tokens[0].kind == "short"
  assert tokens[0].name == "a"
  assert tokens[3].value == "result.txt"
  assert tokens[4].value == "include"
  assert tokens[5].kind == "operand"
  assert tokens[5].name == "-1"

  let full = cli.parse_full(
    ["--count", "2", "demo.txt"],
    {count: {kind: "Int", required: true}, file: {kind: "Path", positional: true}},
  )?

  assert full.values.count == 2
  assert full.sources.count == "argv"
  let usage = cli.usage({count: {kind: "Int", required: true}}, "demo")
  assert "usage: demo" in usage

  const command_specs = {
    build: {
      positionals: [
        "root",
      ],
      types: {
        root: "Path",
      },
      rest: "raw",
    },
    clean: {
      positionals: [
        "root",
      ],
      types: {
        root: "Path",
      },
      rest: "raw",
    },
  }

  let command: CommandArgs = cli.commands(
    ["deploy", "target/demo", "--dry-run"],
    rootless_default: "build",
    commands: command_specs,
    fallback_command: {positionals: ["action", "root"], types: {root: "Path"}, rest: "raw", command_like: true},
  )?

  assert command.command == "deploy"
  assert command.root.name() == "demo"
  assert command.raw[0] == "--dry-run"
  let explicit: CommandArgs = cli.commands(["clean", "target/demo"], command_specs)?
  assert explicit.command == "clean"
  assert explicit.root.name() == "demo"
}

test test_cli_parse_compact_forms {
  let parsed: ParsedArgs = cli.parse(
    ["--total", "3", "-D", "one", "-Dtwo", "--verbose", "src/main.xsh"],
    {
      count: {
        form: "--total N",
        default: 0,
      },
      define: {
        form: "-D NAME",
        repeated: true,
      },
      file: {
        form: "FILE",
        default: p".",
      },
      verbose: {
        form: "-v --verbose",
        default: false,
      },
    },
  )?

  assert parsed.count == 3
  assert parsed.define.join(",") == "one,two"
  assert parsed.file.name() == "main.xsh"
  assert parsed.verbose
  let cli_tokens = cli.tokens(["--mode=json", "-v"], ["mode"])?
  assert cli_tokens[0].name == "mode"
  assert cli_tokens[0].value == "json"
}

test test_cli_applet_parses_head_attached_value {
  let parsed: HeadAppletOptions = cli.applet(
    ["-n2", "file"],
    {
      count: {
        form: "-n --lines N",
        kind: "Int",
        default: 10,
      },
      files: {
        form: "...FILE",
      },
    },
  )?

  assert parsed.count == 2
  assert parsed.files[0] == "file"
}

test test_cli_applet_parses_sort_cluster_and_attached_values {
  let parsed: SortAppletOptions = cli.applet(
    ["-nr", "-k2", "-t,", "file"],
    {
      numeric: {
        form: "-n --numeric-sort",
        default: false,
      },
      reverse: {
        form: "-r --reverse",
        default: false,
      },
      key: {
        form: "-k KEY",
        kind: "Int",
        default: 1,
      },
      delimiter: {
        form: "-t DELIMITER",
        default: "",
      },
      files: {
        form: "...FILE",
      },
    },
  )?

  assert parsed.numeric
  assert parsed.reverse
  assert parsed.key == 2
  assert parsed.delimiter == ","
  assert parsed.files[0] == "file"
}

test test_cli_applet_parses_fd_clusters_and_repeated_values {
  let parsed: FdAppletOptions = cli.applet(
    ["-HI", "-e", "xsh", "-E", "target", "pattern", "root"],
    {
      hidden: {
        form: "-H --hidden",
        default: false,
      },
      no_ignore: {
        form: "-I --no-ignore",
        default: false,
      },
      extensions: {
        form: "-e --extension EXT",
        repeated: true,
      },
      excludes: {
        form: "-E --exclude PATTERN",
        repeated: true,
      },
      operands: {
        form: "...ARG",
      },
    },
  )?

  assert parsed.hidden
  assert parsed.no_ignore
  assert parsed.extensions[0] == "xsh"
  assert parsed.excludes[0] == "target"
  assert parsed.operands.join(",") == "pattern,root"
}

test test_cli_applet_parses_rg_long_assignment_and_attached_values {
  let parsed: RgAppletOptions = cli.applet(
    ["--color=always", "-efoo", "-g*.xsh", "root"],
    {
      color: {
        form: "--color WHEN",
        default: "auto",
      },
      pattern: {
        form: "-e PATTERN",
        required: true,
      },
      globs: {
        form: "-g GLOB",
        repeated: true,
      },
      roots: {
        form: "...ROOT",
      },
    },
  )?

  assert parsed.color == "always"
  assert parsed.pattern == "foo"
  assert parsed.globs[0] == "*.xsh"
  assert parsed.roots[0] == "root"
}

test test_cli_applet_parses_cp_compatibility_flags {
  let parsed: CpAppletOptions = cli.applet(
    ["-n", "-f", "-t", "dest", "src1", "src2"],
    {
      no_clobber: {
        form: "-n",
        default: false,
        conflicts: "force",
      },
      force: {
        form: "-f",
        default: false,
        conflicts: "no_clobber",
      },
      target: {
        form: "-t DIR",
        kind: "Path",
      },
      operands: {
        form: "...FILE",
      },
    },
  )?

  assert ! parsed.no_clobber
  assert parsed.force
  assert parsed.target?.name() == "dest"
  assert parsed.operands.join(",") == "src1,src2"

  let reversed: CpAppletOptions = cli.applet(
    ["-f", "-n", "-t", "dest", "src1", "src2"],
    {
      no_clobber: {
        form: "-n",
        default: false,
        conflicts: "force",
      },
      force: {
        form: "-f",
        default: false,
        conflicts: "no_clobber",
      },
      target: {
        form: "-t DIR",
        kind: "Path",
      },
      operands: {
        form: "...FILE",
      },
    },
  )?
  assert reversed.no_clobber
  assert ! reversed.force
}

test test_cli_applet_last_scalar_occurrence_wins {
  match cli.parse(
    ["-v", "-v"],
    {verbose: {form: "-v", default: false}},
  ) {
    Ok(_) => test.fail("strict cli.parse should reject duplicate scalar options")?
    Err(_) => {}
  }

  let parsed = cli.applet(
    ["-v", "-v"],
    {verbose: {form: "-v", default: false}},
  )?
  assert parsed.verbose
}

test test_cli_parse_advanced_descriptors {
  let root_handle = fs.tempdir()?
  defer root_handle.close()?
  let root = root_handle.host_path()?
  let config = fp"{root}/config.toml"
  config.write("ready")?

  let schema = {
    mode: {
      form: "--mode MODE",
      default: "text",
      choices: [
        "text",
        "json",
      ],
    },
    color: {
      form: "--color[=WHEN]",
      default: "auto",
      optional_default: "always",
      choices: [
        "auto",
        "always",
        "never",
      ],
    },
    config: {
      form: "--config PATH",
      kind: "Path",
      file: true,
      default: config,
    },
    workspace: {
      form: "--workspace DIR",
      kind: "Path",
      default: root,
    },
    timeout: {
      form: "--timeout DURATION",
      default: 1s,
      positive: true,
    },
    count: {
      form: "--count N",
      kind: "UInt",
      default: 1,
      min: 1,
    },
    verbose: {
      form: "-v --verbose",
      default: false,
      deprecated: "use --log-level instead",
    },
    json: {
      form: "--json",
      default: false,
      conflicts: "table",
    },
    table: {
      form: "--table",
      default: false,
    },
    output: {
      form: "--output PATH",
      default: "",
      requires: "mode",
    },
    secret: {
      form: "--secret VALUE",
      default: "",
      hidden: true,
    },
    left: {
      form: "--left VALUE",
      required_group: "input",
    },
    right: {
      form: "--right VALUE",
      required_group: "input",
    },
  }

  let full = cli.parse_full(["--color", "-v", "--left", "a"], schema)?
  let values = full.values.require(AdvancedArgs)?
  assert values.color == "always"
  assert values.config.name() == "config.toml"
  assert values.workspace.name() == root.name()
  assert values.count == 1
  assert f"{values.timeout}" == "1s"
  assert values.verbose
  assert full.sources.get("color")?.require(Str)? == "argv"
  assert full.sources.get("mode")?.require(Str)? == "default"
  assert full.warnings.len() == 1

  let env_full = cli.parse_full(
    [],
    {profile: {form: "--profile NAME", default: "dev", env: "XSH_PROFILE"}},
    {XSH_PROFILE: "prod"},
  )?

  assert env_full.values.profile == "prod"
  assert env_full.sources.get("profile")?.require(Str)? == "env"
  let usage = cli.usage(schema, "demo")
  assert "usage: demo [OPTIONS]" in usage
  assert "--mode MODE" in usage
  assert "-h, --help" in usage
  assert ! ("--secret" in usage)

  match cli.parse(["--help"], schema, "demo sub") {
    Ok(_) => test.fail("implicit help should stop parsing")?
    Err(error) => assert "usage: demo sub [OPTIONS]" in error.message
  }

  match cli.parse(["--mode", "xml", "--left", "a"], schema) {
    Ok(_) => test.fail("choice validation should fail")?
    Err(error) => assert "expects one of" in error.message
  }

  match cli.parse(["--json", "--table", "--left", "a"], schema) {
    Ok(_) => test.fail("conflict validation should fail")?
    Err(error) => assert "conflicts" in error.message
  }

  match cli.parse([], schema) {
    Ok(_) => test.fail("required group validation should fail")?
    Err(error) => assert "required group" in error.message
  }

  match cli.parse(["--count", "-1", "--left", "a"], schema) {
    Ok(_) => test.fail("UInt validation should fail")?
    Err(error) => assert "expects UInt" in error.message
  }

  match cli.parse(["--config", f"{root}/missing.toml", "--left", "a"], schema) {
    Ok(_) => test.fail("file path validation should fail")?
    Err(error) => assert "expects a file path" in error.message
  }
}

test test_cli_commands_accept_aliases_forms_and_options {
  let command: CommandOptions = cli.commands(
    ["b", "--verbose", "target/demo", "--", "--dry-run"],
    {
      build: {
        aliases: [
          "b",
        ],
        form: "build ROOT ...REST",
        types: {
          root: "Path",
        },
        options: {
          verbose: {
            form: "-v --verbose",
            default: false,
          },
        },
      },
    },
  )?

  assert command.command == "build"
  assert command.action == "b"
  assert command.root.name() == "demo"
  assert command.verbose
  assert command.rest[0] == "--dry-run"
}

test test_cli_parse_positional_default_is_optional {
  let absent = cli.parse([], {kind: {form: "KIND", default: "rust"}})?
  assert absent.kind == "rust"

  let explicit = cli.parse(["xsh"], {kind: {form: "KIND", default: "rust"}})?
  assert explicit.kind == "xsh"

  let usage = cli.usage({kind: {form: "KIND", default: "rust"}}, "dev")
  assert "[KIND]" in usage

  match cli.parse([], {action: {form: "ACTION", required: true}}) {
    Ok(_) => test.fail("required positional should fail when absent")?
    Err(error) => assert "missing required argument ACTION" in error.message
  }

  let relaxed = cli.parse([], {file: {form: "FILE", required: false}})?
  assert relaxed.file == null
}
